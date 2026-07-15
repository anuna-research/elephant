//! The pinned `cbcl-elephant` dialect (SPEC-001 ADR-001).
//!
//! The dialect file is vendored from `cbcl-rs/dialects/elephant.cbcl`; its
//! content hash is part of the protocol identity. A registry is built once
//! per process; installation re-runs R1/R2/R3/R5/R6 every time, so a
//! tampered vendored file fails at startup, not at message time.

use crate::errors::{AppError, AppResult};
use cbcl_core::canonical::dialect_canonical_bytes;
use cbcl_core::dialect::{Dialect, DialectRegistry};
use sha2::{Digest, Sha256};

/// Vendored verbatim from ../cbcl-rs/dialects/elephant.cbcl.
pub const DIALECT_TEXT: &str = include_str!("../../vendor/cbcl-elephant.cbcl");

pub const DIALECT_NAME: &str = "cbcl-elephant";

/// The seven speech-act performatives of the dialect.
pub const PERFORMATIVES: [&str; 7] = [
    "assert", "retract", "query", "concede", "commit", "request", "justify",
];

pub fn load_dialect() -> AppResult<Dialect> {
    let sexpr = cbcl_parser::parse(DIALECT_TEXT)
        .map_err(|e| AppError::Internal(format!("vendored dialect unparseable: {e:?}")))?;
    cbcl_parser::parse_dialect(&sexpr)
        .map_err(|e| AppError::Internal(format!("vendored dialect invalid: {e}")))
}

/// Canonical content hash of the vendored dialect (protocol identity).
///
/// Computed as `sha256` over cbcl-rs's canonical dialect bytes
/// (`dialect_canonical_bytes`) — the same bytes used for signing — so a
/// silent change to the vendored file or to cbcl-rs canonicalisation moves
/// this value. The self-declared `:hash` field (if any) is deliberately not
/// trusted here; we recompute from content.
pub fn dialect_hash() -> AppResult<String> {
    let d = load_dialect()?;
    let digest = Sha256::digest(dialect_canonical_bytes(&d));
    Ok(format!("sha256:{digest:x}"))
}

/// Registry with cbcl-base + cbcl-elephant installed (R1/R2/R3/R5/R6 checked).
pub fn registry() -> AppResult<DialectRegistry> {
    let mut reg = DialectRegistry::new();
    reg.install(load_dialect()?)
        .map_err(|e| AppError::Internal(format!("dialect install failed: {e:?}")))?;
    Ok(reg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialect_installs_and_declares_all_performatives() {
        let reg = registry().unwrap();
        let d = reg.find_by_name(DIALECT_NAME).expect("dialect installed");
        for p in PERFORMATIVES {
            assert!(
                d.performatives.iter().any(|pd| pd.name == p),
                "missing performative {p}"
            );
        }
    }

    /// The dialect content hash is pinned: a silent change to the vendored
    /// file (or to cbcl-rs canonicalisation) must break the build visibly.
    #[test]
    fn dialect_hash_is_pinned() {
        let hash = dialect_hash().expect("dialect hashes");
        assert_eq!(
            hash, PINNED_DIALECT_HASH,
            "cbcl-elephant dialect hash changed — protocol identity moved; \
             update PINNED_DIALECT_HASH deliberately if intended"
        );
    }

    pub const PINNED_DIALECT_HASH: &str =
        "sha256:9042d9c06f1d4fb39e9f5e9aba132ff50bc927a74a90b22b72f17737edb7b484";
}
