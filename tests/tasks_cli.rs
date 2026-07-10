//! TEST-201..TEST-211 — the hence-successor task layer through the binary.

use assert_cmd::Command;

struct Env {
    _dir: tempfile::TempDir,
    home: String,
}

/// A three-task plan: models → api (depends on models), docs (independent).
const PLAN: &[&str] = &[
    "(given task-models)",
    "(given task-api)",
    "(given task-docs)",
    "(given no-deps-models)",
    "(given no-deps-docs)",
    "(normally r-ready-models (and task-models no-deps-models) ready-models)",
    "(normally r-ready-api (and task-api completed-models) ready-api)",
    "(normally r-ready-docs (and task-docs no-deps-docs) ready-docs)",
    "(normally r-assign-models (and ready-models agent-alice-available) assign-to-models-alice)",
    "(normally r-assign-api (and ready-api agent-alice-available) assign-to-api-alice)",
    "(normally r-assign-docs (and ready-docs agent-bob-available) assign-to-docs-bob)",
];

impl Env {
    fn new() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_string_lossy().to_string();
        let e = Env { _dir: dir, home };
        e.ok(&["id", "create", "--name", "alice"]);
        e.ok(&["theory", "create", "plan", "--template", "plan"]);
        for stmt in PLAN {
            e.ok(&["assert", stmt, "-t", "plan"]);
        }
        e.ok(&["plan", "join-as", "alice", "-t", "plan"]);
        e
    }

    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("elephant").unwrap();
        c.env("ELEPHANT_HOME", &self.home);
        c
    }

    fn ok(&self, args: &[&str]) {
        let out = self.cmd().args(args).output().unwrap();
        assert!(
            out.status.success(),
            "command {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
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

    fn bucket(&self, board: &serde_json::Value, name: &str) -> Vec<String> {
        board[name]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["task"].as_str().unwrap().to_string())
            .collect()
    }
}

/// TEST-205 + TEST-206: board buckets and next actions.
#[test]
fn board_and_next() {
    let e = Env::new();
    let b = e.json(&["plan", "board", "-t", "plan"]);
    assert_eq!(e.bucket(&b, "ready"), vec!["docs", "models"]);
    assert_eq!(e.bucket(&b, "backlog"), vec!["api"]);
    assert!(e.bucket(&b, "done").is_empty());

    let n = e.json(&["task", "next", "--agent", "alice", "-t", "plan"]);
    let actions: Vec<_> = n["next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["task"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(actions, vec!["models"], "only ready+assigned to alice: {n}");
}

/// TEST-201/202/203: claim → in_progress; unclaim → ready; complete → done
/// and downstream becomes ready (dependency extraction works).
#[test]
fn lifecycle_claim_unclaim_complete() {
    let e = Env::new();
    e.ok(&["task", "claim", "models", "-t", "plan"]);
    let b = e.json(&["plan", "board", "-t", "plan"]);
    assert_eq!(e.bucket(&b, "in_progress"), vec!["models"]);

    // idempotent claim
    let again = e.json(&["task", "claim", "models", "-t", "plan"]);
    assert_eq!(again["noop"], true);

    // unclaim → back to ready (chain cancellation over v1)
    e.ok(&["task", "unclaim", "models", "-t", "plan"]);
    let b = e.json(&["plan", "board", "-t", "plan"]);
    assert_eq!(e.bucket(&b, "ready"), vec!["docs", "models"]);

    // re-claim (v2 over prior unclaim), complete
    e.ok(&["task", "claim", "models", "-t", "plan"]);
    e.ok(&["task", "complete", "models", "-t", "plan"]);
    let b = e.json(&["plan", "board", "-t", "plan"]);
    assert_eq!(e.bucket(&b, "done"), vec!["models"]);
    // api's readiness rule depends on completed-models → now ready
    assert!(e.bucket(&b, "ready").contains(&"api".to_string()));

    // claim of unready task refused (docs is ready; api ready now; use a
    // fresh backlog check: unclaim on completed refused)
    e.cmd()
        .args(["task", "unclaim", "models", "-t", "plan"])
        .assert()
        .failure()
        .code(1);
}

/// TEST-201 negative-input: claiming a backlog task without --force fails.
#[test]
fn claim_unready_refused() {
    let e = Env::new();
    e.cmd()
        .args(["task", "claim", "api", "-t", "plan"])
        .assert()
        .failure()
        .code(1);
    // --force overrides
    e.ok(&["task", "claim", "api", "--force", "-t", "plan"]);

    // unknown task → 8 with the available list
    let out = e
        .cmd()
        .args(["task", "claim", "nope", "-t", "plan"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(8));
    assert!(String::from_utf8_lossy(&out.stderr).contains("models"));
}

/// TEST-204: block carries the reason (fixing hence 0.7's drop) and flips
/// the bucket; unblock restores; propagation blocks downstream.
#[test]
fn block_unblock_with_reason_and_propagation() {
    let e = Env::new();
    e.ok(&[
        "task",
        "block",
        "models",
        "waiting on schema sign-off",
        "-t",
        "plan",
    ]);
    let b = e.json(&["plan", "board", "-t", "plan"]);
    // hence semantics: a manual block does NOT upstream-block dependents
    // (only failure states propagate); api stays backlog via its unmet
    // readiness rule.
    assert_eq!(e.bucket(&b, "blocked"), vec!["models"]);
    assert_eq!(e.bucket(&b, "backlog"), vec!["api"]);

    // But a failure state does propagate: assert failed-models and see api
    // upstream-blocked (the propagation rules were appended by block/claim).
    e.ok(&["assert", "failed-models", "-t", "plan"]);
    let b2 = e.json(&["plan", "board", "-t", "plan"]);
    let api_item = b2["blocked"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["task"] == "api")
        .expect("api upstream-blocked after failed-models");
    assert_eq!(api_item["block_type"], "upstream_blocked");
    // Clean up the failure fact so the rest of the test proceeds.
    let log = e.json(&["log", "-t", "plan"]);
    let failed_sid = log["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["cbcl"].as_str().unwrap_or("").contains("failed-models"))
        .and_then(|x| x["sid"].as_str())
        .unwrap()
        .to_string();
    e.ok(&["retract", &failed_sid, "-t", "plan"]);

    // The reason is in the theory as a blocked-by fact.
    let s = e.json(&["status", "-t", "plan"]);
    assert!(
        s["conclusions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["literal"].as_str().unwrap().starts_with("blocked-by")),
        "blocked-by reason fact must be derivable: {s}"
    );

    e.ok(&["task", "unblock", "models", "-t", "plan"]);
    let b = e.json(&["plan", "board", "-t", "plan"]);
    assert!(e.bucket(&b, "ready").contains(&"models".to_string()));

    // block of completed task refused
    e.ok(&["task", "claim", "models", "-t", "plan"]);
    e.ok(&["task", "complete", "models", "-t", "plan"]);
    e.cmd()
        .args(["task", "block", "models", "r", "-t", "plan"])
        .assert()
        .failure()
        .code(1);
}

/// TEST-209/210: template seeds (meta plan …); join-as binds assignments.
#[test]
fn plan_info_and_join_as() {
    let e = Env::new();
    let info = e.json(&["plan", "info", "-t", "plan"]);
    assert!(
        !info["plan"].is_null(),
        "template must seed meta plan: {info}"
    );
    let rendered = info["plan"].to_string();
    assert!(rendered.contains("plan"), "title present: {rendered}");

    // bob isn't available yet → docs unassigned in next
    let n = e.json(&["task", "next", "--agent", "bob", "-t", "plan"]);
    assert!(
        n["fallback"] == true
            || n["next_actions"].as_array().unwrap().is_empty()
            || !n["next_actions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["agent"] == "bob"),
        "bob has no assignment before join-as: {n}"
    );
    e.ok(&["plan", "join-as", "bob", "-t", "plan"]);
    let n = e.json(&["task", "next", "--agent", "bob", "-t", "plan"]);
    assert!(
        n["next_actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["task"] == "docs" && a["agent"] == "bob"),
        "docs assigned to bob after join-as: {n}"
    );
}

/// TEST-211 (migration oracle): hence 0.7 on the equivalent plan file agrees
/// on board buckets. Skips silently when hence isn't installed.
#[test]
fn migration_oracle_vs_hence() {
    let hence = which::which("hence");
    let Ok(hence) = hence else {
        eprintln!("hence not on PATH — skipping migration oracle");
        return;
    };

    // The same plan in hence's file format (claims wrappers optional).
    let dir = tempfile::tempdir().unwrap();
    let plan_path = dir.path().join("plan.spl");
    std::fs::write(&plan_path, PLAN.join("\n")).unwrap();

    // Drive both through the same lifecycle: claim models via hence, and in
    // elephant; then compare buckets.
    let e = Env::new();

    let hence_out = std::process::Command::new(&hence)
        .args([
            "task",
            "claim",
            "models",
            plan_path.to_str().unwrap(),
            "--agent",
            "alice",
        ])
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert!(
        hence_out.status.success(),
        "hence claim failed: {}",
        String::from_utf8_lossy(&hence_out.stderr)
    );
    e.ok(&["task", "claim", "models", "-t", "plan"]);

    let hence_board: serde_json::Value = serde_json::from_slice(
        &std::process::Command::new(&hence)
            .args(["plan", "board", plan_path.to_str().unwrap(), "--json"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let ele_board = e.json(&["plan", "board", "-t", "plan"]);

    for bucket in ["backlog", "ready", "in_progress", "done"] {
        let mut h: Vec<String> = hence_board[bucket]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["task"].as_str().unwrap().to_string())
            .collect();
        let mut m: Vec<String> = ele_board[bucket]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["task"].as_str().unwrap().to_string())
            .collect();
        h.sort();
        m.sort();
        assert_eq!(h, m, "bucket '{bucket}' diverges from hence");
    }
}
