//! Error and exit-code contract (SPEC-001 REQ-024).
//!
//! Exit codes are a stable CLI contract, pinned by a unit test:
//!   0 success · 1 usage · 2 config/identity · 3 parse/validation
//!   4 signature/verification · 5 E1 violation · 6 reasoner exhaustion
//!   7 transport/daemon · 8 not-found · 9 internal (incl. unimplemented)
//!  10 predicate-not-satisfied — a checked condition ran cleanly but did not
//!     hold (e.g. `closure compare` mismatch, #20). Distinct from an error:
//!     the command did its job; the answer is "no".

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    Success = 0,
    Usage = 1,
    Config = 2,
    Parse = 3,
    Signature = 4,
    E1 = 5,
    Reasoner = 6,
    Transport = 7,
    NotFound = 8,
    Internal = 9,
    /// A checked predicate ran to completion but did not hold (not an error).
    Predicate = 10,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("usage: {0}")]
    Usage(String),
    #[error("configuration: {0}")]
    Config(String),
    #[error("parse: {0}")]
    Parse(String),
    #[error("signature: {0}")]
    Signature(String),
    #[error("retraction refused (E1 same-signer): {0}")]
    E1(String),
    #[error("reasoner: {0}")]
    Reasoner(String),
    #[error("transport: {0}")]
    Transport(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("internal: {0}")]
    Internal(String),
}

impl AppError {
    pub fn exit(&self) -> Exit {
        match self {
            AppError::Usage(_) => Exit::Usage,
            AppError::Config(_) => Exit::Config,
            AppError::Parse(_) => Exit::Parse,
            AppError::Signature(_) => Exit::Signature,
            AppError::E1(_) => Exit::E1,
            AppError::Reasoner(_) => Exit::Reasoner,
            AppError::Transport(_) => Exit::Transport,
            AppError::NotFound(_) => Exit::NotFound,
            AppError::Internal(_) => Exit::Internal,
        }
    }

    /// Error category slug used in `--json` error objects (CON-004).
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Usage(_) => "usage",
            AppError::Config(_) => "config",
            AppError::Parse(_) => "parse",
            AppError::Signature(_) => "signature",
            AppError::E1(_) => "e1",
            AppError::Reasoner(_) => "reasoner",
            AppError::Transport(_) => "transport",
            AppError::NotFound(_) => "not-found",
            AppError::Internal(_) => "internal",
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Config(e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

impl fmt::Display for Exit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", *self as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REQ-024: the exit-code contract is stable. Do not renumber.
    #[test]
    fn exit_code_contract() {
        assert_eq!(Exit::Success as u8, 0);
        assert_eq!(Exit::Usage as u8, 1);
        assert_eq!(Exit::Config as u8, 2);
        assert_eq!(Exit::Parse as u8, 3);
        assert_eq!(Exit::Signature as u8, 4);
        assert_eq!(Exit::E1 as u8, 5);
        assert_eq!(Exit::Reasoner as u8, 6);
        assert_eq!(Exit::Transport as u8, 7);
        assert_eq!(Exit::NotFound as u8, 8);
        assert_eq!(Exit::Internal as u8, 9);
        assert_eq!(Exit::Predicate as u8, 10);
    }
}
