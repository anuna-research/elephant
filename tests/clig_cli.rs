//! CLI guideline conformance (clig.dev review, 2026-07-11):
//! confirmation before destructive actions, no silently-ignored flags,
//! examples + support path in help.

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

    fn setup_theory(&self) {
        self.cmd()
            .args(["id", "create", "--name", "alice"])
            .assert()
            .success();
        self.cmd()
            .args(["theory", "create", "release"])
            .assert()
            .success();
    }
}

// ── item 1: `theory remove` confirms before acting ─────────────────────

/// Non-interactive removal without --force is refused before any state
/// changes (clig.dev: confirm moderate/severe actions; never require a
/// prompt — provide a flag alternative).
#[test]
fn remove_without_force_refused_when_not_a_tty() {
    let env = Env::new();
    env.setup_theory();
    env.cmd()
        .args(["theory", "remove", "release", "did:crdt:whoever"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("--force"));
}

/// --force skips the confirmation gate: the command proceeds to the real
/// membership check (exit 4: not a member) instead of the usage refusal.
#[test]
fn remove_with_force_passes_the_confirmation_gate() {
    let env = Env::new();
    env.setup_theory();
    env.cmd()
        .args(["theory", "remove", "release", "did:crdt:whoever", "--force"])
        .assert()
        .failure()
        .code(4)
        .stderr(predicate::str::contains("not a member"));
}

// ── item 2: `--at` is refused on writes instead of silently ignored ────

/// `--at` is a read-time lens; a write accepting it would let the user
/// believe they backdated an entry in an append-only corpus.
#[test]
fn at_flag_refused_on_writes() {
    let env = Env::new();
    env.setup_theory();
    for args in [
        vec!["assert", "x", "-t", "release"],
        vec!["retract", "s-0000000000000000", "-t", "release"],
        vec!["promise", "done", "-t", "release"],
        vec!["theory", "create", "other"],
    ] {
        let mut full = args.clone();
        full.extend(["--at", "2020-01-01T00:00:00Z"]);
        env.cmd()
            .args(&full)
            .assert()
            .failure()
            .code(1)
            .stderr(predicate::str::contains("--at"));
    }
}

/// Reads still honour `--at`.
#[test]
fn at_flag_still_works_on_reads() {
    let env = Env::new();
    env.setup_theory();
    env.cmd()
        .args(["assert", "x", "-t", "release"])
        .assert()
        .success();
    env.cmd()
        .args(["status", "-t", "release", "--at", "2030-01-01T00:00:00Z"])
        .assert()
        .success();
}

// ── item 3: help carries examples and a support path ────────────────────

/// clig.dev: lead with examples; link docs; provide a feedback path.
#[test]
fn top_level_help_has_examples_and_support_path() {
    let env = Env::new();
    env.cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Examples:"))
        .stdout(predicate::str::contains(
            "https://codeberg.org/anuna/elephant-3000",
        ));
}

/// Non-obvious commands explain themselves in long help.
#[test]
fn non_obvious_commands_have_long_help() {
    let env = Env::new();
    for (cmd, needle) in [
        ("what-if", "without writing"),
        ("require", "Abduction"),
        ("watch", "Ctrl-C"),
    ] {
        env.cmd()
            .args([cmd, "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains(needle));
    }
}

// ── theory aliases obey the declared grammar (SPEC-001 REQ-003) ─────────

/// TEST-003 negative-input: a malformed alias is refused (exit 1) before
/// anything is created — LDH labels only, 1..63, lowercase, RFC 6335
/// hyphen placement, and never the 64-hex theory-id shape.
#[test]
fn theory_create_refuses_malformed_aliases() {
    let env = Env::new();
    env.cmd()
        .args(["id", "create", "--name", "alice"])
        .assert()
        .success();
    let too_long = "a".repeat(64); // also exactly the id shape
    for bad in [
        "Release-V1",  // uppercase
        "-staging",    // leading hyphen
        "release-",    // trailing hyphen
        "release--v1", // adjacent hyphens
        "my theory",   // space
        "plan.spl",    // dot
        "../escape",   // path fragment
        "",            // empty
        too_long.as_str(),
    ] {
        env.cmd()
            .args(["theory", "create", bad])
            .assert()
            .failure()
            .code(1);
    }
    env.cmd()
        .args(["theory", "list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"theories\":[]"));
}

/// An uppercase alias error suggests the lowercase spelling (reject, do
/// not normalise — PROTO-001 LangSec principle 4).
#[test]
fn uppercase_alias_error_suggests_lowercase() {
    let env = Env::new();
    env.cmd()
        .args(["id", "create", "--name", "alice"])
        .assert()
        .success();
    env.cmd()
        .args(["theory", "create", "Release-V1"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("release-v1"));
}

/// TEST-003 positive: the full LDH range is accepted — single char,
/// digit-led, and 63 chars (one under the id length).
#[test]
fn theory_create_accepts_ldh_aliases() {
    let env = Env::new();
    env.cmd()
        .args(["id", "create", "--name", "alice"])
        .assert()
        .success();
    let max = format!("a{}", "b".repeat(62)); // 63 chars
    for good in ["release-v1", "x", "9lives", max.as_str()] {
        env.cmd()
            .args(["theory", "create", good])
            .assert()
            .success();
    }
}

/// `-t` dispatch is decided by grammar: an argument that is neither a
/// 64-hex id nor a valid alias is a usage error, not a lookup miss.
#[test]
fn theory_lookup_refuses_non_grammar_input() {
    let env = Env::new();
    env.setup_theory();
    env.cmd()
        .args(["status", "-t", "../release"])
        .assert()
        .failure()
        .code(1);
}
