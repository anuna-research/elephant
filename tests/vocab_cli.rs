//! SPEC-005 TEST-401/TEST-403/TEST-404 — `vocab` and `define` end-to-end
//! through the binary (the pure-core kernels live in `core::vocab` units).

use assert_cmd::Command;

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

    fn setup_theory(&self) -> &Env {
        self.cmd()
            .args(["id", "create", "--name", "alice"])
            .assert()
            .success();
        self.cmd()
            .args(["theory", "create", "release"])
            .assert()
            .success();
        self
    }

    fn json(&self, args: &[&str]) -> serde_json::Value {
        let out = self.cmd().args(args).arg("--json").output().unwrap();
        assert!(
            out.status.success(),
            "command {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).expect("stdout must be one JSON object")
    }

    fn entries(&self) -> u64 {
        self.json(&["theory", "list"])["theories"][0]["entries"]
            .as_u64()
            .unwrap()
    }

    fn vocab_rows(&self) -> Vec<serde_json::Value> {
        self.json(&["vocab", "-t", "release"])["vocab"]
            .as_array()
            .cloned()
            .unwrap()
    }

    fn row(&self, kind: &str, family: &str) -> serde_json::Value {
        self.vocab_rows()
            .into_iter()
            .find(|r| r["kind"] == kind && r["family"] == family)
            .unwrap_or_else(|| panic!("no vocab row ({kind}, {family})"))
    }
}

// ── TEST-403 positive ────────────────────────────────────────────────────

/// define with --arg emits a predicate declaration Entry; the view keys the
/// documentation on the symbol ci-green/1.
#[test]
fn define_with_args_declares_and_documents() {
    let env = Env::new();
    env.setup_theory();
    let before = env.entries();
    let v = env.json(&[
        "define",
        "ci-green/1",
        "--arg",
        "task:symbol",
        "--desc",
        "CI pipeline green for task ?t",
        "--kind",
        "evidence",
        "--asserter",
        "role:ci",
        "-t",
        "release",
    ]);
    assert_eq!(v["performative"], "assert");
    assert_eq!(v["spl_form"], "predicate");
    assert!(v["receipt"].as_str().unwrap().starts_with("s-"));
    assert_eq!(env.entries(), before + 1);

    let row = env.row("predicate", "ci-green/1");
    assert_eq!(row["functor"], "ci-green");
    assert_eq!(row["arity"], 1);
    assert_eq!(row["doc"]["description"], "CI pipeline green for task ?t");
    assert_eq!(row["doc"]["kind"], "evidence");
    assert_eq!(row["doc"]["asserter"], "role:ci");
    assert_eq!(row["doc"]["redefined"], false);
    assert_eq!(row["doc"]["malformed"], serde_json::json!([]));
    // Declared but not yet occurring: the documentation joins nothing.
    assert_eq!(row["doc"]["detached"], true);

    // An occurrence attaches it.
    env.json(&["assert", "(given (ci-green m1))", "-t", "release"]);
    let row = env.row("predicate", "ci-green/1");
    assert_eq!(row["doc"]["detached"], false);
    assert_eq!(row["roles"], serde_json::json!(["fact"]));
}

/// define without --arg emits the bare meta-target form.
#[test]
fn define_without_args_emits_meta_target() {
    let env = Env::new();
    env.setup_theory();
    let v = env.json(&[
        "define",
        "review-approved/1",
        "--desc",
        "a human reviewer approved the change for ?t",
        "-t",
        "release",
    ]);
    assert_eq!(v["spl_form"], "meta");
    let row = env.row("predicate", "review-approved/1");
    assert_eq!(
        row["doc"]["description"],
        "a human reviewer approved the change for ?t"
    );
}

/// Arity disambiguates: define succeeds even when a task named `green` is
/// declared (no 0.3.0 reachability warning — TEST-403).
#[test]
fn define_succeeds_with_declared_task_suffix() {
    let env = Env::new();
    env.setup_theory();
    env.json(&["assert", "task-green", "-t", "release"]);
    let v = env.json(&[
        "define",
        "ci-green/1",
        "--desc",
        "CI green",
        "-t",
        "release",
    ]);
    assert_eq!(v["performative"], "assert");
}

// ── TEST-403 negative-input: exit 3, corpus unchanged ───────────────────

fn refuse(env: &Env, args: &[&str]) {
    let before = env.entries();
    env.cmd()
        .args(args)
        .args(["-t", "release"])
        .assert()
        .failure()
        .code(3);
    assert_eq!(
        env.entries(),
        before,
        "refusal must store nothing: {args:?}"
    );
}

#[test]
fn define_refusals_exit_3_and_store_nothing() {
    let env = Env::new();
    env.setup_theory();
    // malformed indicators
    refuse(&env, &["define", "nope", "--desc", "d"]);
    refuse(&env, &["define", "Weird/1", "--desc", "d"]);
    // nullary target (CON-402)
    refuse(&env, &["define", "foo/0", "--desc", "d"]);
    // leading-zero and overflowing arity (CON-401)
    refuse(&env, &["define", "x/01", "--desc", "d"]);
    refuse(&env, &["define", "x/4294967296", "--desc", "d"]);
    // built-in functor collisions at any arity (REQ-404)
    refuse(&env, &["define", "completed/1", "--desc", "d"]);
    refuse(&env, &["define", "commitment-state/2", "--desc", "d"]);
    refuse(&env, &["define", "verified/2", "--desc", "d"]);
    // arg count ≠ arity; duplicate names; bad sort
    refuse(&env, &["define", "p/2", "--arg", "a:symbol", "--desc", "d"]);
    refuse(
        &env,
        &[
            "define", "p/2", "--arg", "a:symbol", "--arg", "a:symbol", "--desc", "d",
        ],
    );
    refuse(
        &env,
        &["define", "p/1", "--arg", "a:strange", "--desc", "d"],
    );
    // value grammar: >512-byte description, control chars, bad kind,
    // no properties at all
    let big = "x".repeat(513);
    refuse(&env, &["define", "p/1", "--desc", &big]);
    refuse(&env, &["define", "p/1", "--desc", "a\u{1b}b"]);
    refuse(&env, &["define", "p/1", "--desc", "d", "--kind", "control"]);
    refuse(&env, &["define", "p/1", "--desc", "d", "--kind", "bogus"]);
    refuse(&env, &["define", "p/1"]);
}

/// CON-401 permits any control-free UTF-8 — quotes, backslashes and
/// semicolons are conforming and must round-trip through the quoted SPL
/// atom (escaped at payload construction, never refused).
#[test]
fn define_escapes_quotes_backslashes_semicolons() {
    let env = Env::new();
    env.setup_theory();
    let desc = r#"say "hi"; a\b path"#;
    let asserter = r#"agent "a;b"\ crew"#;
    env.json(&[
        "define", "p/1", "--desc", desc, "--asserter", asserter, "-t", "release",
    ]);
    let row = env.row("predicate", "p/1");
    assert_eq!(row["doc"]["description"], desc);
    assert_eq!(row["doc"]["asserter"], asserter);
}

// ── TEST-401/404 through the CLI ────────────────────────────────────────

/// Mixed corpus: verified-m1 and (verified m1) are two distinct rows, both
/// built-in; collision rows (p x) vs flat p/1 never merge; ordering is
/// kind then family (CON-403).
#[test]
fn vocab_rows_kinds_and_ordering() {
    let env = Env::new();
    env.setup_theory();
    env.json(&["assert", "verified-m1", "-t", "release"]);
    env.json(&["assert", "(given (verified m1))", "-t", "release"]);
    env.json(&["assert", "(given (p x))", "-t", "release"]);
    env.json(&["assert", "p/1", "-t", "release"]);

    let legacy = env.row("legacy", "verified");
    let param = env.row("predicate", "verified/1");
    assert_eq!(legacy["built_in"], true);
    assert_eq!(param["built_in"], true);

    let rows = env.vocab_rows();
    let p_positions: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r["family"] == "p/1")
        .map(|(i, _)| i)
        .collect();
    assert_eq!(p_positions.len(), 2, "flat p/1 and predicate p/1 distinct");
    assert_eq!(rows[p_positions[0]]["kind"], "predicate");
    assert_eq!(rows[p_positions[1]]["kind"], "legacy");

    // Global ordering: every predicate row before every legacy row.
    let first_legacy = rows.iter().position(|r| r["kind"] == "legacy").unwrap();
    assert!(
        rows[..first_legacy]
            .iter()
            .all(|r| r["kind"] == "predicate"),
        "kind order predicate < legacy < malformed"
    );
}

/// Hole/orphan classes and the docs join surface through the CLI.
#[test]
fn vocab_classes_and_holes() {
    let env = Env::new();
    env.setup_theory();
    env.json(&[
        "assert",
        "(normally r-verified (and (ci-green ?t) (review-approved ?t)) (verified ?t))",
        "-t",
        "release",
    ]);
    env.json(&["assert", "(given (review-approved m1))", "-t", "release"]);
    env.json(&["assert", "orphan-note", "-t", "release"]);

    let ci = env.row("predicate", "ci-green/1");
    assert_eq!(ci["class"], "hole");
    assert_eq!(ci["roles"], serde_json::json!(["body"]));
    assert_eq!(env.row("legacy", "orphan-note")["class"], "orphan");
    assert_eq!(env.row("predicate", "review-approved/1")["class"], "active");
}

/// Redefinition provenance is visible in the view (REQ-407 kernel is unit
/// tested; here: the documenter DID rides the JSON row).
#[test]
fn vocab_documenter_provenance() {
    let env = Env::new();
    env.setup_theory();
    env.json(&["assert", "deploy-thing", "-t", "release"]);
    env.json(&[
        "define",
        "deploy-marker/1",
        "--desc",
        "a deploy marker",
        "-t",
        "release",
    ]);
    let row = env.row("predicate", "deploy-marker/1");
    assert!(
        row["doc"]["documenter"]
            .as_str()
            .unwrap()
            .starts_with("did:crdt:"),
        "documenter must be the signer DID"
    );
}

/// TEST-408 (cross-process determinism): two separate binary invocations
/// of `vocab --json` over the same corpus are byte-identical — any
/// HashMap-iteration order leaking into the output would differ across
/// process hash seeds with high probability.
#[test]
fn vocab_json_identical_across_processes() {
    let env = Env::new();
    env.setup_theory();
    env.json(&["assert", "(given task-m1)", "-t", "release"]);
    env.json(&[
        "assert",
        "(normally r-verified (and (ci-green ?t) (review-approved ?t)) (verified ?t))",
        "-t",
        "release",
    ]);
    env.json(&["assert", "(given (review-approved m1))", "-t", "release"]);
    env.json(&[
        "define",
        "ci-green/1",
        "--desc",
        "CI green",
        "-t",
        "release",
    ]);
    env.json(&["assert", "deploy-thing", "-t", "release"]);
    let run = || {
        let out = env
            .cmd()
            .args(["vocab", "-t", "release", "--json"])
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap()
    };
    assert_eq!(run(), run(), "vocab --json must be process-deterministic");
}
