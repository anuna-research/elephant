//! Agent identity (SPEC-001 REQ-001/REQ-002, CON-005, ADR-003).
//!
//! Custody rules (the hark `identity.rs` discipline): the Ed25519 seed is a
//! raw 32-byte file created 0600 with `create_new` — never truncated, never
//! silently regenerated. A wrong-sized file is a hard error; only NotFound
//! may trigger generation, and `create` refuses to overwrite.

use crate::errors::{AppError, AppResult};
use crate::paths::Paths;
use base64ct::{Base64UrlUnpadded, Encoding as _};
use did_crdt::{Did, Document};
use ed25519_dalek::{Signer as _, SigningKey, Verifier as _};
use serde::{Deserialize, Serialize};
use std::io::Write as _;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub did: String,
}

pub struct Identity {
    pub signing_key: SigningKey,
    pub did: Did,
    pub document: Document,
    pub profile: Profile,
}

// Manual Debug: never risk the seed reaching logs (NFR-004).
impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity")
            .field("did", &self.did.to_string())
            .field("name", &self.profile.name)
            .finish_non_exhaustive()
    }
}

impl Identity {
    /// Multibase (base64url-no-pad, `u` prefix) of the public key —
    /// did-crdt's required encoding.
    pub fn public_key_multibase(&self) -> String {
        format!(
            "u{}",
            Base64UrlUnpadded::encode_string(self.signing_key.verifying_key().as_bytes())
        )
    }

    pub fn public_key_hex(&self) -> String {
        self.signing_key
            .verifying_key()
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// The seed bytes, for HKDF-derived subkeys (SPEC-004 CON-304).
    /// Never expose outside key-derivation call sites.
    pub(crate) fn seed(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }
}

/// Create a fresh identity. Refuses if a key already exists (TEST-001).
pub fn create(paths: &Paths, name: Option<String>) -> AppResult<Identity> {
    let key_path = paths.key_file();
    if key_path.exists() {
        return Err(AppError::Config(format!(
            "identity already exists at {} — refusing to overwrite",
            key_path.display()
        )));
    }
    std::fs::create_dir_all(paths.identity_dir())?;

    let signing_key = SigningKey::generate(&mut rand_core::OsRng);
    write_secret_file(&key_path, &signing_key.to_bytes())?;

    let pk_mb = format!(
        "u{}",
        Base64UrlUnpadded::encode_string(signing_key.verifying_key().as_bytes())
    );
    let (document, _genesis) =
        Document::new(&pk_mb).map_err(|e| AppError::Internal(format!("did-crdt genesis: {e}")))?;
    let did = document.did.clone();

    std::fs::write(
        paths.did_document(),
        document
            .to_bytes()
            .map_err(|e| AppError::Internal(format!("did document serialise: {e}")))?,
    )?;

    let name = name.unwrap_or_else(|| whoami_fallback(&did));
    let profile = Profile {
        name,
        did: did.to_string(),
    };
    std::fs::write(
        paths.profile(),
        toml::to_string_pretty(&profile).map_err(|e| AppError::Internal(e.to_string()))?,
    )?;

    Ok(Identity {
        signing_key,
        did,
        document,
        profile,
    })
}

/// Load the existing identity (config error if absent or malformed).
pub fn load(paths: &Paths) -> AppResult<Identity> {
    let seed = read_secret_file(&paths.key_file())?;
    let signing_key = SigningKey::from_bytes(&seed);

    let doc_bytes = std::fs::read(paths.did_document()).map_err(|e| {
        AppError::Config(format!(
            "DID document missing at {} ({e}); run `elephant id create`",
            paths.did_document().display()
        ))
    })?;
    let document = Document::from_bytes(&doc_bytes)
        .map_err(|e| AppError::Config(format!("DID document corrupt: {e}")))?;
    let did = document.did.clone();

    let profile: Profile =
        toml::from_str(&std::fs::read_to_string(paths.profile()).map_err(|e| {
            AppError::Config(format!(
                "profile missing at {} ({e})",
                paths.profile().display()
            ))
        })?)
        .map_err(|e| AppError::Config(format!("profile corrupt: {e}")))?;

    // Sanity: profile DID must match the document (REQ-001 negative-output).
    if profile.did != did.to_string() {
        return Err(AppError::Config(
            "profile DID does not match DID document — state dir is inconsistent".into(),
        ));
    }

    // Bind the private key to the document (REQ-001): the seed's public key
    // must be one the DID document authorises. Without this, a swapped or
    // corrupt key file loads happily under the old DID and every subsequent
    // "successful" write is signed by a key no verifier accepts — a failure
    // that only surfaces remotely, long after the cause.
    let our_pk = signing_key.verifying_key();
    if !verifying_keys_for(&doc_bytes, did.as_str())
        .iter()
        .any(|k| k == &our_pk)
    {
        return Err(AppError::Config(
            "private key does not match the DID document — state dir is inconsistent".into(),
        ));
    }

    Ok(Identity {
        signing_key,
        did,
        document,
        profile,
    })
}

fn whoami_fallback(did: &Did) -> String {
    let id = did.method_specific_id();
    format!("agent-{}", &id[..8.min(id.len())])
}

/// The verification keys a serialized DID document publishes, but only if the
/// document actually resolves to `expected_did`. Used by the join ceremony to
/// verify a joiner's proof of possession (REQ-104): the joiner signs a
/// session-bound challenge with its DID identity key, and the inviter checks
/// that signature against the keys this returns — proving the joiner controls
/// the DID it claims, and that the supplied document is that DID's document.
/// Returns empty (verification must fail closed) on any parse/mismatch.
pub fn verifying_keys_for(
    doc_bytes: &[u8],
    expected_did: &str,
) -> Vec<ed25519_dalek::VerifyingKey> {
    let Ok(document) = Document::from_bytes(doc_bytes) else {
        return Vec::new();
    };
    let Ok(resolved) = document.resolve() else {
        return Vec::new();
    };
    let Some(dd) = resolved.did_document else {
        return Vec::new();
    };
    if dd.id != expected_did {
        return Vec::new();
    }
    let mut keys = Vec::new();
    for vm in &dd.verification_method {
        let Some(b64) = vm.public_key_multibase.strip_prefix('u') else {
            continue;
        };
        let Ok(bytes) = Base64UrlUnpadded::decode_vec(b64) else {
            continue;
        };
        if let Ok(arr) = <[u8; 32]>::try_from(bytes.as_slice()) {
            if let Ok(vk) = ed25519_dalek::VerifyingKey::from_bytes(&arr) {
                keys.push(vk);
            }
        }
    }
    keys
}

/// cbcl-rs R4 signer over the identity key.
pub struct IdentitySigner<'a>(pub &'a SigningKey);

impl cbcl_core::r4::Signer for IdentitySigner<'_> {
    fn sign(&self, data: &[u8]) -> Vec<u8> {
        self.0.sign(data).to_bytes().to_vec()
    }

    fn verify(&self, data: &[u8], sig: &[u8]) -> bool {
        let Ok(sig64) = <&[u8; 64]>::try_from(sig) else {
            return false;
        };
        self.0
            .verifying_key()
            .verify(data, &ed25519_dalek::Signature::from_bytes(sig64))
            .is_ok()
    }
}

fn write_secret_file(path: &Path, bytes: &[u8; 32]) -> AppResult<()> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}

fn read_secret_file(path: &Path) -> AppResult<[u8; 32]> {
    let bytes = std::fs::read(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::Config(format!(
                "no identity at {}; run `elephant id create`",
                path.display()
            ))
        } else {
            AppError::Config(format!("cannot read key {}: {e}", path.display()))
        }
    })?;
    <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| {
        AppError::Config(format!(
            "key file {} has wrong size ({} bytes, expected 32) — refusing to proceed",
            path.display(),
            bytes.len()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paths() -> (tempfile::TempDir, Paths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            home: dir.path().to_path_buf(),
        };
        (dir, paths)
    }

    /// TEST-001 positive: create → key 0600, DID resolvable, reload matches.
    #[test]
    fn create_and_reload() {
        let (_dir, paths) = temp_paths();
        let created = create(&paths, Some("alice".into())).unwrap();
        assert!(created.did.to_string().starts_with("did:crdt:"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(paths.key_file())
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "key file must be 0600");
        }

        let loaded = load(&paths).unwrap();
        assert_eq!(loaded.did, created.did);
        assert_eq!(loaded.profile.name, "alice");
        assert_eq!(
            loaded.public_key_multibase(),
            created.public_key_multibase()
        );

        // DID document resolves to a W3C result with our key.
        let res = loaded.document.resolve().unwrap();
        assert!(res.did_document.is_some());
    }

    /// TEST-001 negative-input: second create refused.
    #[test]
    fn create_refuses_overwrite() {
        let (_dir, paths) = temp_paths();
        create(&paths, None).unwrap();
        let err = create(&paths, None).unwrap_err();
        assert!(matches!(err, AppError::Config(_)));
    }

    /// TEST-001 negative-output: corrupt key size is a hard error, never
    /// silent regeneration.
    #[test]
    fn wrong_key_size_is_fatal() {
        let (_dir, paths) = temp_paths();
        std::fs::create_dir_all(paths.identity_dir()).unwrap();
        std::fs::write(paths.key_file(), b"short").unwrap();
        let err = load(&paths).unwrap_err();
        assert!(err.to_string().contains("wrong size"));
    }

    /// REQ-001 negative-input: a key file swapped for an unrelated (but
    /// well-formed) seed must not load under the old DID — the private key is
    /// bound to the DID document.
    #[test]
    fn swapped_private_key_is_rejected() {
        let (_dir, paths) = temp_paths();
        create(&paths, Some("alice".into())).unwrap();
        // Overwrite the seed with a different, valid 32-byte key, leaving the
        // DID document and profile untouched.
        let other = SigningKey::from_bytes(&[9u8; 32]);
        std::fs::remove_file(paths.key_file()).unwrap();
        write_secret_file(&paths.key_file(), &other.to_bytes()).unwrap();
        let err = load(&paths).unwrap_err();
        assert!(
            err.to_string().contains("private key does not match"),
            "a mismatched key must be refused: {err}"
        );
    }

    /// R4 signer roundtrip.
    #[test]
    fn r4_signer_roundtrip() {
        use cbcl_core::r4::Signer as _;
        let (_dir, paths) = temp_paths();
        let ident = create(&paths, None).unwrap();
        let signer = IdentitySigner(&ident.signing_key);
        let sig = signer.sign(b"payload");
        assert!(signer.verify(b"payload", &sig));
        assert!(!signer.verify(b"other", &sig));
    }
}
