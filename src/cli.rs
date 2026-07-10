//! CLI surface (effectful shell). SPEC-001 §REQ-001..REQ-024.

use std::process::ExitCode;

pub fn run() -> ExitCode {
    eprintln!("elephant {}: not yet implemented", crate::VERSION);
    ExitCode::from(1)
}
