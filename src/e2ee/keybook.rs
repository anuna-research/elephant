//! The keybook (SPEC-004 CON-303, REQ-304/REQ-305).
//!
//! `{gen → K_gen}` plus the current generation. Distributed as an MLS
//! application message after every Add and every rotation, so an admitted
//! member can read the whole corpus history — while a removed member never
//! receives the post-rotation generation. An elephant never forgets, but it
//! chooses who may read the minute-book.

use super::seal::DataKey;
use crate::errors::{AppError, AppResult};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const KEYBOOK_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Keybook {
    pub v: u16,
    /// generation → base64 key
    pub keys: BTreeMap<String, String>,
    pub current: u32,
}

impl Keybook {
    /// A fresh keybook with generation 0.
    pub fn genesis() -> (Keybook, DataKey) {
        let k = super::seal::random_key();
        let mut keys = BTreeMap::new();
        keys.insert("0".to_string(), B64.encode(k));
        (
            Keybook {
                v: KEYBOOK_VERSION,
                keys,
                current: 0,
            },
            k,
        )
    }

    pub fn key(&self, generation: u32) -> Option<DataKey> {
        let raw = B64.decode(self.keys.get(&generation.to_string())?).ok()?;
        <[u8; 32]>::try_from(raw.as_slice()).ok()
    }

    pub fn current_key(&self) -> Option<DataKey> {
        self.key(self.current)
    }

    /// REQ-305: mint the next generation (called by the steward on removal).
    pub fn rotate(&mut self) -> DataKey {
        let k = super::seal::random_key();
        self.current += 1;
        self.keys.insert(self.current.to_string(), B64.encode(k));
        k
    }

    /// Merge a received keybook: union of keys, monotone `current`.
    /// A regressed `current` is rejected (CON-303 negative-input).
    pub fn merge(&mut self, other: &Keybook) -> AppResult<()> {
        if other.v != KEYBOOK_VERSION {
            return Err(AppError::Parse(format!(
                "unsupported keybook version {}",
                other.v
            )));
        }
        if other.current < self.current {
            return Err(AppError::Signature(format!(
                "keybook generation regressed ({} < {}) — refusing",
                other.current, self.current
            )));
        }
        for (g, k) in &other.keys {
            // A differing key for a known generation is a fork or an attack.
            if let Some(existing) = self.keys.get(g) {
                if existing != k {
                    return Err(AppError::Signature(format!(
                        "keybook conflict at generation {g}"
                    )));
                }
            }
            self.keys.insert(g.clone(), k.clone());
        }
        self.current = other.current;
        Ok(())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("keybook serialisation is infallible")
    }

    pub fn from_bytes(bytes: &[u8]) -> AppResult<Keybook> {
        serde_json::from_slice(bytes).map_err(|e| AppError::Parse(format!("keybook: {e}")))
    }

    /// Persist 0600 (CON-305). Never logged, never in JSON output (NFR-303).
    pub fn save(&self, path: &std::path::Path) -> AppResult<()> {
        use std::io::Write as _;
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let tmp = path.with_extension("json.tmp");
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(&self.to_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn load(path: &std::path::Path) -> AppResult<Option<Keybook>> {
        match std::fs::read(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(AppError::Config(format!("keybook read: {e}"))),
            Ok(bytes) => Keybook::from_bytes(&bytes).map(Some),
        }
    }
}

// Keys must never reach logs or error text (NFR-303).
impl std::fmt::Display for Keybook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Keybook {{ generations: {}, current: {} }}",
            self.keys.len(),
            self.current
        )
    }
}

pub fn keybook_path(paths: &crate::paths::Paths, theory_id: &str) -> std::path::PathBuf {
    paths.theory_dir(theory_id).join("mls").join("keybook.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REQ-304: a joiner merging the steward's keybook can read all history.
    #[test]
    fn merge_gives_full_history() {
        let (mut steward, k0) = Keybook::genesis();
        let k1 = steward.rotate();
        assert_eq!(steward.current, 1);

        let mut joiner = Keybook {
            v: KEYBOOK_VERSION,
            keys: BTreeMap::new(),
            current: 0,
        };
        joiner.merge(&steward).unwrap();
        assert_eq!(joiner.key(0), Some(k0), "history readable");
        assert_eq!(joiner.key(1), Some(k1), "current readable");
        assert_eq!(joiner.current, 1);
    }

    /// CON-303 negative-input: regression and conflict are refused.
    #[test]
    fn regression_and_conflict_refused() {
        let (mut a, _) = Keybook::genesis();
        a.rotate();
        let (stale, _) = Keybook::genesis();
        assert!(a.merge(&stale).is_err(), "generation must not regress");

        let (mut b, _) = Keybook::genesis();
        let (other, _) = Keybook::genesis(); // different gen-0 key
        assert!(
            b.merge(&other).is_err(),
            "conflicting key for a known generation is refused"
        );
    }

    /// NFR-303: Display never renders key material.
    #[test]
    fn display_hides_keys() {
        let (kb, k) = Keybook::genesis();
        let rendered = format!("{kb}");
        assert!(!rendered.contains(&B64.encode(k)));
        assert!(rendered.contains("current: 0"));
    }

    #[cfg(unix)]
    #[test]
    fn saved_keybook_is_0600() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keybook.json");
        let (kb, _) = Keybook::genesis();
        kb.save(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(Keybook::load(&path).unwrap().unwrap(), kb);
    }
}
