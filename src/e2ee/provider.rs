//! Durable OpenMLS provider (SPEC-004 CON-305), ported from ../hark.
//!
//! `MemoryStorage` holds the live map so every OpenMLS storage call behaves
//! exactly as upstream tests it; `persist` snapshots that map atomically to
//! one 0600 file. A snapshot of the current map is by construction free of
//! deleted entries, so delete fidelity reduces to "persist after every
//! mutating group operation". A version mismatch is a re-join, never a
//! silent migration.

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use openmls_rust_crypto::{MemoryStorage, RustCrypto};
use openmls_traits::OpenMlsProvider;
use std::collections::HashMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use super::MlsError;

const STATE_VERSION: u32 = 1;

#[derive(serde::Serialize, serde::Deserialize)]
struct StateFile {
    version: u32,
    values: HashMap<String, String>,
}

pub struct DurableProvider {
    crypto: RustCrypto,
    storage: MemoryStorage,
    path: PathBuf,
}

impl OpenMlsProvider for DurableProvider {
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = MemoryStorage;

    fn storage(&self) -> &Self::StorageProvider {
        &self.storage
    }
    fn crypto(&self) -> &Self::CryptoProvider {
        &self.crypto
    }
    fn rand(&self) -> &Self::RandProvider {
        &self.crypto
    }
}

impl DurableProvider {
    /// Missing file → fresh provider. Unreadable / malformed /
    /// version-mismatched → set aside as `.stale`, start empty (re-join).
    pub fn open(path: &Path) -> Result<Self, MlsError> {
        let provider = Self {
            crypto: RustCrypto::default(),
            storage: MemoryStorage::default(),
            path: path.to_path_buf(),
        };
        match fs::read(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(provider),
            Err(e) => Err(MlsError::Storage(e)),
            Ok(bytes) => match Self::decode(&bytes) {
                Ok(values) => {
                    *provider.storage.values.write().unwrap() = values;
                    Ok(provider)
                }
                Err(reason) => {
                    tracing::warn!(path = %path.display(), reason,
                        "mls state unreadable; setting aside and re-joining");
                    fs::rename(path, path.with_extension("stale"))?;
                    Ok(provider)
                }
            },
        }
    }

    fn decode(bytes: &[u8]) -> Result<HashMap<Vec<u8>, Vec<u8>>, String> {
        let state: StateFile =
            serde_json::from_slice(bytes).map_err(|e| format!("malformed state file: {e}"))?;
        if state.version != STATE_VERSION {
            return Err(format!(
                "state version {} != supported {STATE_VERSION} (re-join, no silent migration)",
                state.version
            ));
        }
        let mut values = HashMap::with_capacity(state.values.len());
        for (k, v) in state.values {
            let key = B64.decode(k).map_err(|e| format!("bad key b64: {e}"))?;
            let value = B64.decode(v).map_err(|e| format!("bad value b64: {e}"))?;
            values.insert(key, value);
        }
        Ok(values)
    }

    /// Atomic 0600 snapshot: temp file → fsync → rename.
    pub fn persist(&self) -> Result<(), MlsError> {
        let values = {
            let map = self.storage.values.read().unwrap();
            map.iter()
                .map(|(k, v)| (B64.encode(k), B64.encode(v)))
                .collect()
        };
        let state = StateFile {
            version: STATE_VERSION,
            values,
        };
        let bytes = serde_json::to_vec(&state).map_err(std::io::Error::other)?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("tmp");
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    /// Discard in-memory mutations since the last persist (used when a
    /// staged Welcome fails validation — the consumed init key stays intact).
    pub fn rollback_to_disk(&self) -> Result<(), MlsError> {
        let values = match fs::read(&self.path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(e) => return Err(MlsError::Storage(e)),
            Ok(bytes) => {
                Self::decode(&bytes).map_err(|r| MlsError::Storage(std::io::Error::other(r)))?
            }
        };
        *self.storage.values.write().unwrap() = values;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TEST-308 negative-output: a version bump is a re-join, not a
    /// migration; the old state is set aside, not destroyed.
    #[test]
    fn version_bump_is_a_rejoin() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.bin");
        fs::write(
            &path,
            serde_json::to_vec(&StateFile {
                version: STATE_VERSION + 1,
                values: HashMap::from([(B64.encode(b"k"), B64.encode(b"v"))]),
            })
            .unwrap(),
        )
        .unwrap();
        let provider = DurableProvider::open(&path).unwrap();
        assert!(provider.storage.values.read().unwrap().is_empty());
        assert!(path.with_extension("stale").exists());
    }

    /// TEST-308: corrupt state is set aside, never a crash loop.
    #[test]
    fn corrupt_state_is_set_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.bin");
        fs::write(&path, b"not json").unwrap();
        let provider = DurableProvider::open(&path).unwrap();
        assert!(provider.storage.values.read().unwrap().is_empty());
        assert!(path.with_extension("stale").exists());
    }

    /// NFR-303 (at rest): the state file is 0600.
    #[cfg(unix)]
    #[test]
    fn state_file_is_0600() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.bin");
        let provider = DurableProvider::open(&path).unwrap();
        provider.persist().unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "group secrets must be owner-only");
    }
}
