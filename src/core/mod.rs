//! Pure core — no I/O, no clocks, no globals (SPEC-001 §5 Purity Boundary).
//!
//! Everything here is a deterministic function of its arguments; wall time
//! enters only as an explicit `TimePoint` parameter.

pub mod closure;
pub mod dialect;
pub mod envelope;
pub mod vocab;
