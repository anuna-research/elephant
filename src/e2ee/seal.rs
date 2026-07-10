//! SealedEntry — the encrypted corpus element (SPEC-004 CON-301, REQ-303).
//!
//! `XChaCha20-Poly1305(K_gen)` over the canonical Entry JSON, with the
//! theory id and key generation bound into the AAD so a ciphertext cannot be
//! replayed into another theory or mis-attributed to another generation.

use crate::core::envelope::Entry;
use crate::errors::{AppError, AppResult};
use chacha20poly1305::XChaCha20Poly1305;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use serde::{Deserialize, Serialize};

pub const SEAL_VERSION: u16 = 1;
const AAD_TAG: &[u8] = b"elephant-seal-v1";

/// A 32-byte corpus data key (ADR-302: random, not an MLS exporter).
pub type DataKey = [u8; 32];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SealedEntry {
    pub v: u16,
    #[serde(rename = "gen")]
    pub generation: u32,
    #[serde(with = "b64")]
    pub nonce: Vec<u8>,
    #[serde(with = "b64")]
    pub ct: Vec<u8>,
}

fn aad(theory_id: &str, generation: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(AAD_TAG.len() + theory_id.len() + 4);
    out.extend_from_slice(AAD_TAG);
    out.extend_from_slice(theory_id.as_bytes());
    out.extend_from_slice(&generation.to_be_bytes());
    out
}

pub fn random_key() -> DataKey {
    use rand_core::RngCore as _;
    let mut k = [0u8; 32];
    rand_core::OsRng.fill_bytes(&mut k);
    k
}

/// Seal one Entry under `K_gen`.
pub fn seal(
    entry: &Entry,
    theory_id: &str,
    generation: u32,
    key: &DataKey,
) -> AppResult<SealedEntry> {
    let plaintext = crate::core::envelope::entry_to_json(entry);
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce_bytes = [0u8; 24];
    use rand_core::RngCore as _;
    rand_core::OsRng.fill_bytes(&mut nonce_bytes);
    let ct = cipher
        .encrypt(
            &nonce_bytes.into(),
            Payload {
                msg: plaintext.as_bytes(),
                aad: &aad(theory_id, generation),
            },
        )
        .map_err(|_| AppError::Internal("seal failed".into()))?;
    Ok(SealedEntry {
        v: SEAL_VERSION,
        generation,
        nonce: nonce_bytes.to_vec(),
        ct,
    })
}

/// Open a SealedEntry with the keybook. Fail closed: any AEAD or version
/// failure quarantines the element (REQ-303).
pub fn open(
    sealed: &SealedEntry,
    theory_id: &str,
    keys: &dyn Fn(u32) -> Option<DataKey>,
) -> Result<Entry, OpenError> {
    if sealed.v != SEAL_VERSION {
        return Err(OpenError::Version(sealed.v));
    }
    if sealed.nonce.len() != 24 {
        return Err(OpenError::Malformed("nonce length"));
    }
    let key = keys(sealed.generation).ok_or(OpenError::UnknownGeneration(sealed.generation))?;
    let cipher = XChaCha20Poly1305::new(&key.into());
    let nonce: [u8; 24] = sealed
        .nonce
        .as_slice()
        .try_into()
        .map_err(|_| OpenError::Malformed("nonce length"))?;
    let pt = cipher
        .decrypt(
            &nonce.into(),
            Payload {
                msg: &sealed.ct,
                aad: &aad(theory_id, sealed.generation),
            },
        )
        .map_err(|_| OpenError::AeadFailed)?;
    let text = String::from_utf8(pt).map_err(|_| OpenError::Malformed("utf8"))?;
    crate::core::envelope::entry_from_json(&text).map_err(|_| OpenError::Malformed("entry json"))
}

#[derive(Debug, PartialEq, Eq)]
pub enum OpenError {
    Version(u16),
    UnknownGeneration(u32),
    AeadFailed,
    Malformed(&'static str),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::Version(v) => write!(f, "unsupported seal version {v}"),
            OpenError::UnknownGeneration(g) => write!(f, "no key for generation {g}"),
            OpenError::AeadFailed => write!(f, "decryption failed (tampered or wrong key)"),
            OpenError::Malformed(what) => write!(f, "malformed sealed entry: {what}"),
        }
    }
}

pub fn to_json(s: &SealedEntry) -> String {
    serde_json::to_string(s).expect("SealedEntry serialisation is infallible")
}

pub fn from_json(text: &str) -> AppResult<SealedEntry> {
    serde_json::from_str(text).map_err(|e| AppError::Parse(format!("sealed entry json: {e}")))
}

mod b64 {
    use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&B64.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        B64.decode(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::envelope::{Hlc, SpeechAct};

    fn entry() -> Entry {
        Entry::create(
            "th-1",
            Hlc {
                wall_ms: 1,
                logical: 0,
                node_id: 2,
            },
            "did:crdt:aa",
            "did:crdt:aa#key-0",
            &SpeechAct::Assert {
                sentence_id: "s-1".into(),
                spl: "(given secret-fact)".into(),
            },
            "2026-07-11T00:00:00Z",
            &ed25519_dalek::SigningKey::from_bytes(&[3u8; 32]),
        )
    }

    /// TEST-303 positive: roundtrip; ciphertext leaks no plaintext.
    #[test]
    fn seal_open_roundtrip() {
        let k = random_key();
        let e = entry();
        let sealed = seal(&e, "th-1", 0, &k).unwrap();
        let wire = to_json(&sealed);
        assert!(
            !wire.contains("secret-fact") && !wire.contains("did:crdt:aa"),
            "sealed wire must not leak plaintext: {wire}"
        );
        let back = from_json(&wire).unwrap();
        let opened = open(&back, "th-1", &|g| (g == 0).then_some(k)).unwrap();
        assert_eq!(opened, e);
    }

    /// TEST-303 negative-input: tampering, wrong theory, wrong generation.
    #[test]
    fn tampering_fails_closed() {
        let k = random_key();
        let e = entry();
        let sealed = seal(&e, "th-1", 0, &k).unwrap();

        let mut bad = sealed.clone();
        bad.ct[0] ^= 0xff;
        assert_eq!(
            open(&bad, "th-1", &|_| Some(k)).unwrap_err(),
            OpenError::AeadFailed
        );

        let mut bad = sealed.clone();
        bad.nonce[0] ^= 0xff;
        assert_eq!(
            open(&bad, "th-1", &|_| Some(k)).unwrap_err(),
            OpenError::AeadFailed
        );

        // AAD binds the theory: a ciphertext replayed into another theory
        // does not open even with the right key.
        assert_eq!(
            open(&sealed, "th-2", &|_| Some(k)).unwrap_err(),
            OpenError::AeadFailed
        );

        // AAD binds the generation.
        let mut bad = sealed.clone();
        bad.generation = 1;
        assert_eq!(
            open(&bad, "th-1", &|_| Some(k)).unwrap_err(),
            OpenError::AeadFailed
        );

        // Unknown generation is reported distinctly (keybook gap).
        assert_eq!(
            open(&sealed, "th-1", &|_| None).unwrap_err(),
            OpenError::UnknownGeneration(0)
        );

        // A wrong key never opens.
        let other = random_key();
        assert_eq!(
            open(&sealed, "th-1", &|_| Some(other)).unwrap_err(),
            OpenError::AeadFailed
        );
    }

    /// REQ-305 kernel: a key from before rotation cannot open a later entry.
    #[test]
    fn rotation_locks_out_old_keys() {
        let k0 = random_key();
        let k1 = random_key();
        let after = seal(&entry(), "th-1", 1, &k1).unwrap();
        assert_eq!(
            open(&after, "th-1", &|g| (g == 0).then_some(k0)).unwrap_err(),
            OpenError::UnknownGeneration(1),
            "a removed member holding only gen 0 cannot even name the key"
        );
        assert_eq!(
            open(&after, "th-1", &|_| Some(k0)).unwrap_err(),
            OpenError::AeadFailed,
            "and if they guess the generation, the AEAD refuses"
        );
    }
}
