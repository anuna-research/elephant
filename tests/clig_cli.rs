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
