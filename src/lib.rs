//! elephant — speech-act coordination on shared defeasible theories.
//!
//! "I meant what I said, and I said what I meant.
//!  An elephant's faithful, one hundred percent!
//!  moreover, an elephant never forgets."
//!
//! Architecture per SPEC-001 (core) and SPEC-002 (p2p): an effectful shell
//! (`cli`, `store`, `id`, `daemon`) around a pure core (`core::*`) that
//! turns an append-only corpus of signed CBCL speech acts into defeasible
//! conclusions and commitment states.

pub mod cli;
pub mod core;
pub mod daemon;
pub mod dag;
pub mod e2ee;
pub mod errors;
pub mod id;
pub mod p2p;
pub mod paths;
pub mod queries;
pub mod skill;
pub mod store;

/// Placeholder entry point wiring; replaced as IMPL-001 tasks land.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
