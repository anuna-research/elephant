//! Entry — the corpus element (SPEC-001 CON-002). Skeleton.

use serde::{Deserialize, Serialize};

/// Format version of the Entry encoding.
pub const ENTRY_VERSION: u16 = 1;

/// Hybrid logical clock stamp; `node_id` is bound to the signer's key
/// (did-crdt `node_id_from_pubkey`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Hlc {
    pub wall_ms: u64,
    pub logical: u32,
    pub node_id: u64,
}

/// One corpus element: a signed, canonical CBCL speech-act message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub v: u16,
    pub theory: String,
    pub hlc: Hlc,
    pub signer: String,
    pub key_id: String,
    pub cbcl: String,
    #[serde(with = "sig_hex")]
    pub sig: [u8; 64],
}

mod sig_hex {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(sig: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex(sig))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        let s = String::deserialize(d)?;
        unhex(&s).ok_or_else(|| serde::de::Error::custom("invalid signature hex"))
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn unhex(s: &str) -> Option<[u8; 64]> {
        if s.len() != 128 || !s.is_ascii() {
            return None;
        }
        let mut out = [0u8; 64];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            out[i] = u8::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
        }
        Some(out)
    }
}
