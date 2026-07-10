//! TEST-101/102/109 — daemon lifecycle, routing, and watch push.

use assert_cmd::cargo::cargo_bin;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

struct Env {
    _dir: tempfile::TempDir,
    home: String,
}

impl Env {
    fn new() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_string_lossy().to_string();
        let e = Env { _dir: dir, home };
        e.ok(&["id", "create", "--name", "alice"]);
        e.ok(&["theory", "create", "release"]);
        e
    }

    fn cmd(&self) -> Command {
        let mut c = Command::new(cargo_bin("elephant"));
        c.env("ELEPHANT_HOME", &self.home);
        c
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.cmd().args(args).output().unwrap();
        assert!(
            out.status.success(),
            "command {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    fn json(&self, args: &[&str]) -> serde_json::Value {
        let out = self.cmd().args(args).arg("--json").output().unwrap();
        assert!(
            out.status.success(),
            "command {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }

    fn stop(&self) {
        let _ = self.cmd().args(["daemon", "stop"]).output();
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        self.stop();
    }
}

/// TEST-101 + TEST-102: start/status/stop; idempotent start; routing note.
#[test]
fn daemon_lifecycle_and_routing() {
    let e = Env::new();
    let started = e.json(&["daemon", "start"]);
    assert!(started["pid"].as_u64().is_some());

    // Idempotent start returns the same daemon.
    let again = e.json(&["daemon", "start"]);
    assert_eq!(started["addr"], again["addr"]);

    // Status reports theories.
    let st = e.json(&["daemon", "status"]);
    assert!(st["uptime_s"].as_u64().is_some());

    // Asserts route through the daemon (counters increase).
    e.ok(&["assert", "qa-signed", "-t", "release"]);
    let st2 = e.json(&["daemon", "status"]);
    assert!(
        st2["counters"]["entries_merged"].as_u64().unwrap() >= 1,
        "daemon must have merged the CLI assert: {st2}"
    );

    // Reads still work while the daemon runs.
    let s = e.json(&["status", "-t", "release"]);
    assert!(
        s["conclusions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["literal"] == "qa-signed"),
        "{s}"
    );

    // Stop; second stop errors (exit 7).
    e.ok(&["daemon", "stop"]);
    let out = e.cmd().args(["daemon", "stop"]).output().unwrap();
    assert_eq!(out.status.code(), Some(7));
}

/// TEST-109: a watch stream sees the tag flip pushed by a daemon merge.
#[test]
fn watch_receives_pushed_flip() {
    let e = Env::new();
    e.ok(&[
        "assert",
        "(normally r-ready qa-signed release-ready)",
        "-t",
        "release",
    ]);
    e.json(&["daemon", "start"]);

    let mut watcher: Child = e
        .cmd()
        .args(["watch", "release-ready", "-t", "release", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // Give the stream a moment to connect and seed.
    std::thread::sleep(std::time::Duration::from_millis(700));

    e.ok(&["assert", "qa-signed", "-t", "release"]);

    // Read one event line with a timeout guard.
    let stdout = watcher.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        if reader.read_line(&mut line).is_ok() {
            let _ = tx.send(line);
        }
    });
    let line = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("watch event within 10s");
    let ev: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(ev["literal"], "release-ready");
    assert_eq!(ev["new"], "+d");

    let _ = watcher.kill();
    let _ = watcher.wait();
}
