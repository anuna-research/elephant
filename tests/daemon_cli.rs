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

// ── SPEC-005 TEST-406: near-miss advisory on the daemon append path ─────

impl Env {
    /// assert via --json, returning the receipt object (which may carry
    /// the REQ-406 advisory).
    fn assert_json(&self, spl: &str) -> serde_json::Value {
        self.json(&["assert", spl, "-t", "release"])
    }
}

/// Inert-family and sibling advisories ride the append receipt on the live
/// daemon path; suppressions hold; exit codes never change.
#[test]
fn advisory_on_daemon_append() {
    let e = Env::new();
    e.json(&["daemon", "start"]);

    // Warm the reference view: these merges each recompute it. They also
    // set up the flat near-miss corpus (tasks declared so ci-green-m1/m2
    // share the family `ci-green` — CON-402 rule 3).
    e.assert_json("(given task-m1)");
    e.assert_json("(given task-m2)");
    let r = e.assert_json("(normally r ci-green-m1 verified-m1)");
    assert!(r.get("advisory").is_none(), "rule payload never advises");

    // Sibling advisory: the wrong-argument near-miss (HP-A1 shape).
    let s = e.assert_json("ci-green-m2");
    let adv = s.get("advisory").expect("sibling advisory in receipt");
    assert_eq!(adv["kind"], "sibling");
    assert_eq!(adv["family"], "ci-green");
    assert_eq!(adv["family_kind"], "legacy");
    assert_eq!(adv["candidates"][0]["literal"], "ci-green-m1");
    assert_eq!(adv["candidates"][0]["listener"], "r");

    // Inert-family advisory: nothing listens, not documented discovery.
    let i = e.assert_json("random-note");
    let adv = i.get("advisory").expect("inert advisory");
    assert_eq!(adv["kind"], "inert-family");

    // Suppressions: --no-advice, built-in (discovery vocabulary),
    // demanded instance itself.
    let n = e.json(&["assert", "stray-note", "--no-advice", "-t", "release"]);
    assert!(n.get("advisory").is_none(), "--no-advice suppresses");
    let b = e.assert_json("discovered-api-flaky");
    assert!(b.get("advisory").is_none(), "built-in never advises");
    let d = e.assert_json("ci-green-m1");
    assert!(d.get("advisory").is_none(), "the demanded literal itself");

    // Daemon counters surfaced (OBS-401 / SPEC-001 OBS-002 extension).
    let st = e.json(&["daemon", "status"]);
    assert!(st["counters"]["advisories_emitted"].as_u64().unwrap() >= 2);
    assert!(st["counters"]["vocab_views_served"].as_u64().unwrap() >= 2);
}

/// Parameterised witness case: (review-approved m1) proven binds ?t=m1, so
/// asserting (ci-green m2) names (ci-green m1) as the live demand.
#[test]
fn advisory_parameterised_witness_via_daemon() {
    let e = Env::new();
    e.json(&["daemon", "start"]);
    e.assert_json("(normally r-verified (and (ci-green ?t) (review-approved ?t)) (verified ?t))");
    e.assert_json("(given (review-approved m1))");
    let s = e.assert_json("(given (ci-green m2))");
    let adv = s.get("advisory").expect("sibling advisory");
    assert_eq!(adv["kind"], "sibling");
    assert_eq!(adv["family"], "ci-green/1");
    assert_eq!(adv["family_kind"], "predicate");
    assert_eq!(adv["candidates"][0]["literal"], "(ci-green m1)");
    assert_eq!(adv["candidates"][0]["listener"], "r-verified");
}

/// Commitment goals are listeners: the canonical fulfilment assert gets no
/// advisory; a wrong-goal assert names the commitment's goal as candidate.
#[test]
fn advisory_and_commitment_goals() {
    let e = Env::new();
    e.json(&["daemon", "start"]);
    e.assert_json("(given warm-up)"); // warm the view
    e.json(&[
        "promise",
        "legal-signed",
        "--by",
        "2036-01-01T00:00:00Z",
        "-t",
        "release",
    ]);
    let ok = e.assert_json("legal-signed");
    assert!(
        ok.get("advisory").is_none(),
        "commitment goal is a listener and its goal literal is the demand"
    );
}

/// Direct-store mode (no daemon) emits no advisory and text mode prints
/// the advisory to stderr without changing the exit code.
#[test]
fn advisory_direct_mode_and_stderr() {
    let e = Env::new();
    // No daemon: direct store, no advisory ever.
    let v = e.json(&["assert", "loner-fact", "-t", "release"]);
    assert!(v.get("advisory").is_none());

    // With a daemon, text mode: advisory on stderr, receipt on stdout,
    // exit 0.
    e.json(&["daemon", "start"]);
    e.assert_json("(given task-m9)");
    e.assert_json("(normally r9 ci-green-m9 verified-m9)");
    let out = e
        .cmd()
        .args(["assert", "another-loner", "-t", "release"])
        .output()
        .unwrap();
    assert!(out.status.success(), "advisory must not change exit code");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("advisory (inert-family)"),
        "stderr advisory expected, got: {stderr}"
    );
    assert!(
        stderr.contains("ci-green-m9"),
        "candidate expected on stderr: {stderr}"
    );
}

/// NFR-402 cold cache: the first daemon-served append has no reference
/// view yet — the append proceeds and no advisory is emitted.
#[test]
fn advisory_cold_cache_degrades_to_none() {
    let e = Env::new();
    // Corpus written BEFORE the daemon starts (direct mode).
    e.ok(&[
        "assert",
        "(normally rc ci-green-c1 verified-c1)",
        "-t",
        "release",
    ]);
    e.json(&["daemon", "start"]);
    // First append through the fresh daemon: cache is cold for this theory.
    let v = e.assert_json("stray-cold-note");
    assert!(
        v.get("advisory").is_none(),
        "cold cache must degrade to no advisory, got {v}"
    );
    // The append itself landed and warmed the cache; the next one advises.
    let v2 = e.assert_json("stray-cold-note-2");
    assert!(v2.get("advisory").is_some(), "warm cache advises: {v2}");
}
