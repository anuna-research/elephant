//! TEST-005..TEST-009 — producer commands end-to-end through the binary.

use assert_cmd::Command;
use predicates::prelude::*;

struct Env {
    _dir: tempfile::TempDir,
    home: String,
}

impl Env {
    fn new() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_string_lossy().to_string();
        Env { _dir: dir, home }
    }

    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("elephant").unwrap();
        c.env("ELEPHANT_HOME", &self.home);
        c
    }

    fn setup_theory(&self) -> Env2<'_> {
        self.cmd()
            .args(["id", "create", "--name", "alice"])
            .assert()
            .success();
        self.cmd()
            .args(["theory", "create", "release"])
            .assert()
            .success();
        Env2 { env: self }
    }
}

struct Env2<'a> {
    env: &'a Env,
}

impl Env2<'_> {
    fn json(&self, args: &[&str]) -> serde_json::Value {
        let out = self.env.cmd().args(args).arg("--json").output().unwrap();
        assert!(
            out.status.success(),
            "command {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).expect("stdout must be one JSON object")
    }
}

/// TEST-005 positive: assert bare literal → receipt; corpus grows.
#[test]
fn assert_bare_literal() {
    let env = Env::new();
    let t = env.setup_theory();
    let v = t.json(&["assert", "qa-signed", "-t", "release"]);
    assert_eq!(v["performative"], "assert");
    assert_eq!(v["spl_form"], "given");
    assert!(v["receipt"].as_str().unwrap().starts_with("s-"));

    let log = t.json(&["theory", "list"]);
    assert_eq!(log["theories"][0]["entries"], 2); // genesis + assert
}

/// TEST-005 negative-input: malformed SPL → exit 3, corpus unchanged.
#[test]
fn assert_malformed_spl_rejected() {
    let env = Env::new();
    let t = env.setup_theory();
    env.cmd()
        .args(["assert", "(given (unbalanced", "-t", "release"])
        .assert()
        .failure()
        .code(3);
    let log = t.json(&["theory", "list"]);
    assert_eq!(log["theories"][0]["entries"], 1); // genesis only
}

/// TEST-005: inline claims rejected before signing (ADR-012).
#[test]
fn assert_inline_claims_rejected() {
    let env = Env::new();
    env.setup_theory();
    env.cmd()
        .args(["assert", "(claims agent:x (given y))", "-t", "release"])
        .assert()
        .failure()
        .code(3);
}

/// TEST-006 pre-checks: retract own works; unknown id → 8.
#[test]
fn retract_own_assertion() {
    let env = Env::new();
    let t = env.setup_theory();
    let v = t.json(&["assert", "qa-signed", "-t", "release"]);
    let sid = v["receipt"].as_str().unwrap();
    let r = t.json(&["retract", sid, "--reason", "misread", "-t", "release"]);
    assert_eq!(r["performative"], "retract");

    env.cmd()
        .args(["retract", "s-doesnotexist0000", "-t", "release"])
        .assert()
        .failure()
        .code(8);
}

/// TEST-007: promise with future deadline ok; past deadline → exit 3.
#[test]
fn promise_deadlines() {
    let env = Env::new();
    let t = env.setup_theory();
    let v = t.json(&[
        "promise",
        "legal-signed",
        "--by",
        "2036-01-01T00:00:00Z",
        "-t",
        "release",
    ]);
    assert_eq!(v["performative"], "commit");

    env.cmd()
        .args([
            "promise",
            "legal-signed",
            "--by",
            "2020-01-01T00:00:00Z",
            "-t",
            "release",
        ])
        .assert()
        .failure()
        .code(3);
}

/// TEST-008: request addressed to an agent.
#[test]
fn request_commitment() {
    let env = Env::new();
    let t = env.setup_theory();
    let v = t.json(&[
        "request",
        "did:crdt:0000",
        "legal-signed",
        "--when",
        "qa-signed",
        "-t",
        "release",
    ]);
    assert_eq!(v["performative"], "request");
}

/// TEST-009: concede references an existing sentence; dangling → 8.
#[test]
fn concede_referenced_sentence() {
    let env = Env::new();
    let t = env.setup_theory();
    let v = t.json(&["assert", "docs-ready", "-t", "release"]);
    let sid = v["receipt"].as_str().unwrap().to_string();
    let c = t.json(&["concede", "docs-ready", "--re", &sid, "-t", "release"]);
    assert_eq!(c["performative"], "concede");

    env.cmd()
        .args([
            "concede",
            "x",
            "--re",
            "s-nope000000000000",
            "-t",
            "release",
        ])
        .assert()
        .failure()
        .code(8);
}

/// REQ-024 usage: missing -t is a usage error (exit 1).
#[test]
fn missing_theory_is_usage_error() {
    let env = Env::new();
    env.setup_theory();
    env.cmd()
        .args(["assert", "x"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("-t"));
}
