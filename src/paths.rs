//! State directory layout (SPEC-001 CON-005).
//!
//! `ELEPHANT_HOME` overrides everything (used by tests). Otherwise:
//! macOS `~/Library/Application Support/elephant`, elsewhere
//! `$XDG_DATA_HOME/elephant` (default `~/.local/share/elephant`).

use crate::errors::{AppError, AppResult};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Paths {
    pub home: PathBuf,
}

impl Paths {
    /// Resolve the state root. Reads env; shell-side only.
    pub fn resolve() -> AppResult<Self> {
        if let Some(home) = std::env::var_os("ELEPHANT_HOME") {
            return Ok(Self {
                home: PathBuf::from(home),
            });
        }
        let base = if cfg!(target_os = "macos") {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
        } else {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        };
        let base = base.ok_or_else(|| {
            AppError::Config("cannot resolve state dir: HOME not set (or set ELEPHANT_HOME)".into())
        })?;
        Ok(Self {
            home: base.join("elephant"),
        })
    }

    /// Where the resolved home came from — the `ELEPHANT_HOME` override or the
    /// platform default. Surfaced by `elephant info` so "one home = one
    /// identity = one steward" is legible and a wrong home is self-diagnosing
    /// rather than looking like data loss (SPEC-001 CON-005, #8).
    pub fn home_source() -> &'static str {
        if std::env::var_os("ELEPHANT_HOME").is_some() {
            "ELEPHANT_HOME"
        } else {
            "platform default"
        }
    }

    /// True when the resolved home sits under an ephemeral/world-writable root.
    /// Durable state there (`create`, `assert`, `join`) is silently lost on
    /// reboot, and nothing else signals it (#8).
    pub fn is_ephemeral(&self) -> bool {
        ["/tmp/", "/private/tmp/", "/var/tmp/"]
            .iter()
            .any(|root| self.home.starts_with(root))
    }

    pub fn identity_dir(&self) -> PathBuf {
        self.home.join("identity")
    }
    pub fn key_file(&self) -> PathBuf {
        self.identity_dir().join("key.ed25519")
    }
    pub fn did_document(&self) -> PathBuf {
        self.identity_dir().join("did.json")
    }
    pub fn profile(&self) -> PathBuf {
        self.identity_dir().join("profile.toml")
    }
    pub fn theories_dir(&self) -> PathBuf {
        self.home.join("theories")
    }
    pub fn theory_dir(&self, theory_id: &str) -> PathBuf {
        self.theories_dir().join(theory_id)
    }
    pub fn theory_doc(&self, theory_id: &str) -> PathBuf {
        self.theory_dir(theory_id).join("doc.loro")
    }
    pub fn theory_meta(&self, theory_id: &str) -> PathBuf {
        self.theory_dir(theory_id).join("meta.toml")
    }
    pub fn trust_file(&self, theory_id: &str) -> PathBuf {
        self.home.join("trust").join(format!("{theory_id}.spl"))
    }
    pub fn daemon_dir(&self) -> PathBuf {
        self.home.join("daemon")
    }
    pub fn daemon_record(&self) -> PathBuf {
        self.daemon_dir().join("daemon.json")
    }
    pub fn daemon_lock(&self) -> PathBuf {
        self.daemon_dir().join("lock")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elephant_home_override_wins() {
        // Env-var isolation: this test sets and removes ELEPHANT_HOME; keep
        // it the only test touching that variable in-process.
        unsafe { std::env::set_var("ELEPHANT_HOME", "/tmp/elehome") };
        let p = Paths::resolve().unwrap();
        unsafe { std::env::remove_var("ELEPHANT_HOME") };
        assert_eq!(p.home, PathBuf::from("/tmp/elehome"));
        assert_eq!(
            p.key_file(),
            PathBuf::from("/tmp/elehome/identity/key.ed25519")
        );
        assert_eq!(
            p.theory_doc("abc"),
            PathBuf::from("/tmp/elehome/theories/abc/doc.loro")
        );
    }
}
