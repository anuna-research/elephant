//! Daemon lifecycle (SPEC-002 REQ-101/REQ-102, ADR-106 — the hark pattern):
//! exclusive flock is the liveness source of truth; an atomic 0600
//! `daemon.json` discovery record carries {pid, addr, token, api_version};
//! startup classifies prior state by probing before acting.

pub mod api;
pub mod client;

use crate::errors::{AppError, AppResult};
use crate::paths::Paths;
use serde::{Deserialize, Serialize};
use std::io::Write as _;

pub const API_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryRecord {
    pub pid: u32,
    pub addr: String, // 127.0.0.1:port
    pub token: String,
    pub started_at: String,
    pub version: String,
    pub api_version: u16,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Liveness {
    Live(DiscoveryRecord),
    ApiIncompatible,
    Stale,
    Missing,
}

pub fn read_record(paths: &Paths) -> Option<DiscoveryRecord> {
    let bytes = std::fs::read(paths.daemon_record()).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Probe-classify the daemon state (hark daemon.rs pattern).
pub fn classify(paths: &Paths) -> Liveness {
    let Some(rec) = read_record(paths) else {
        return Liveness::Missing;
    };
    if rec.api_version != API_VERSION {
        // Only incompatible if it also answers; a dead record is just stale.
        if client::ping(&rec) {
            return Liveness::ApiIncompatible;
        }
        return Liveness::Stale;
    }
    if client::ping(&rec) {
        Liveness::Live(rec)
    } else {
        Liveness::Stale
    }
}

pub fn generate_token() -> String {
    use base64ct::{Base64UrlUnpadded, Encoding as _};
    let mut bytes = [0u8; 32];
    use rand_core::RngCore as _;
    rand_core::OsRng.fill_bytes(&mut bytes);
    Base64UrlUnpadded::encode_string(&bytes)
}

pub fn write_record(paths: &Paths, rec: &DiscoveryRecord) -> AppResult<()> {
    std::fs::create_dir_all(paths.daemon_dir())?;
    let path = paths.daemon_record();
    let tmp = path.with_extension("json.tmp");
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp)?;
    f.write_all(serde_json::to_string(rec).unwrap().as_bytes())?;
    f.sync_all()?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Held for the daemon's lifetime; the lock file is the single-writer gate.
pub struct DaemonLock {
    _file: std::fs::File,
}

pub fn acquire_lock(paths: &Paths) -> AppResult<DaemonLock> {
    use fs2::FileExt as _;
    std::fs::create_dir_all(paths.daemon_dir())?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(paths.daemon_lock())?;
    file.try_lock_exclusive()
        .map_err(|_| AppError::Transport("another elephant daemon holds the store lock".into()))?;
    Ok(DaemonLock { _file: file })
}

/// `elephant daemon run` — foreground.
pub fn run(paths: &Paths) -> AppResult<()> {
    match classify(paths) {
        Liveness::Live(rec) => {
            return Err(AppError::Transport(format!(
                "daemon already running (pid {}, {})",
                rec.pid, rec.addr
            )));
        }
        Liveness::Stale => {
            let _ = std::fs::remove_file(paths.daemon_record());
        }
        _ => {}
    }
    let lock = acquire_lock(paths)?;
    let ident = crate::id::load(paths)?;
    api::serve(paths.clone(), ident, lock)
}

/// `elephant daemon start` — detach: respawn ourselves as `daemon run`.
pub fn start(paths: &Paths) -> AppResult<DiscoveryRecord> {
    match classify(paths) {
        Liveness::Live(rec) => return Ok(rec), // idempotent (REQ-101)
        Liveness::ApiIncompatible => {
            return Err(AppError::Transport(
                "a daemon with an incompatible api version is running — stop it first".into(),
            ));
        }
        Liveness::Stale => {
            let _ = std::fs::remove_file(paths.daemon_record());
        }
        Liveness::Missing => {}
    }
    let exe = std::env::current_exe()?;
    std::process::Command::new(exe)
        .args(["daemon", "run"])
        .env("ELEPHANT_HOME", &paths.home)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| AppError::Transport(format!("spawn daemon: {e}")))?;
    // Poll readiness (bounded).
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if let Liveness::Live(rec) = classify(paths) {
            return Ok(rec);
        }
    }
    Err(AppError::Transport(
        "daemon did not become ready within 10s".into(),
    ))
}

pub fn stop(paths: &Paths) -> AppResult<()> {
    match classify(paths) {
        Liveness::Live(rec) => {
            client::stop(&rec)?;
            for _ in 0..50 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if !client::ping(&rec) {
                    let _ = std::fs::remove_file(paths.daemon_record());
                    return Ok(());
                }
            }
            Err(AppError::Transport("daemon did not exit within 5s".into()))
        }
        _ => Err(AppError::Transport("no daemon running".into())),
    }
}
