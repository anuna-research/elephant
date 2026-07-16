//! TEST-010..TEST-016, TEST-019 — consumer commands through the binary.
//! Follows HP-O3/HP-O4 (users/operator/happy-paths.md) on one machine.

use assert_cmd::Command;

struct Env {
    _dir: tempfile::TempDir,
    home: String,
}

impl Env {
    fn new() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_string_lossy().to_string();
        let e = Env { _dir: dir, home };
        e.cmd()
            .args(["id", "create", "--name", "alice"])
            .assert()
            .success();
        e.cmd()
            .args(["theory", "create", "release"])
            .assert()
            .success();
        e
    }

    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("elephant").unwrap();
        c.env("ELEPHANT_HOME", &self.home);
        c
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

    fn ok(&self, args: &[&str]) {
        self.cmd().args(args).assert().success();
    }
}

fn tag_of(v: &serde_json::Value, lit: &str) -> Option<String> {
    v["conclusions"]
        .as_array()?
        .iter()
        .find(|c| c["literal"] == lit)
        .map(|c| c["tag"].as_str().unwrap().to_string())
}

/// HP-O3: assert → rule → status shows -d → missing fact lands → +d.
#[test]
fn happy_path_o3_status_flip() {
    let e = Env::new();
    e.ok(&["assert", "qa-signed", "-t", "release"]);
    e.ok(&[
        "assert",
        "(normally r-ready (and qa-signed legal-signed) release-ready)",
        "-t",
        "release",
    ]);

    let s = e.json(&["status", "-t", "release"]);
    assert_eq!(tag_of(&s, "qa-signed").as_deref(), Some("+D"));
    assert_eq!(tag_of(&s, "release-ready").as_deref(), Some("-D"));

    // TEST-012: why-not names the missing premise.
    let w = e.json(&["why-not", "release-ready", "-t", "release"]);
    let blocked = w["blocked_by"].as_array().unwrap();
    assert!(
        blocked.iter().any(|b| {
            b["rule"] == "r-ready"
                && b["missing"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|m| m == "legal-signed")
        }),
        "why-not must name legal-signed as missing: {w}"
    );

    // TEST-013: require abduces the missing fact.
    let r = e.json(&["require", "release-ready", "-t", "release"]);
    assert_eq!(r["already_provable"], false);
    assert!(
        r["solutions"].as_array().unwrap().iter().any(|s| s
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "legal-signed")),
        "require must propose legal-signed: {r}"
    );

    // TEST-014: what-if is non-destructive.
    let wi = e.json(&["what-if", "legal-signed", "release-ready", "-t", "release"]);
    assert_eq!(wi["provable"], true);
    let s2 = e.json(&["status", "-t", "release"]);
    assert_eq!(
        tag_of(&s2, "release-ready").as_deref(),
        Some("-D"),
        "what-if must not change the theory"
    );

    // The missing fact lands; the conclusion flips.
    e.ok(&["assert", "legal-signed", "-t", "release"]);
    let s3 = e.json(&["status", "-t", "release"]);
    assert_eq!(tag_of(&s3, "release-ready").as_deref(), Some("+d"));

    // TEST-011: explain now yields a derivation.
    let ex = e.json(&["explain", "release-ready", "-t", "release"]);
    assert!(!ex["explanation"].is_null());
}

/// HP-O4 / TEST-015: promise → outstanding → fulfilled; late promise violated.
#[test]
fn happy_path_o4_commitments() {
    let e = Env::new();
    let p = e.json(&[
        "promise",
        "legal-signed",
        "--by",
        "2036-01-01T00:00:00Z",
        "-t",
        "release",
    ]);
    let cid = p["receipt"].as_str().unwrap().to_string();

    let c1 = e.json(&["commitments", "-t", "release"]);
    let item = &c1["commitments"].as_array().unwrap()[0];
    assert_eq!(item["id"].as_str().unwrap(), cid);
    assert_eq!(item["state"], "outstanding");

    e.ok(&["assert", "legal-signed", "-t", "release"]);
    let c2 = e.json(&["commitments", "-t", "release"]);
    assert_eq!(c2["commitments"][0]["state"], "fulfilled");

    // Violation via --at evaluation beyond a (former) deadline: retract the
    // fulfilment first, then re-evaluate after the deadline.
    // (Simpler: a second promise with a near-future deadline evaluated later.)
    let p2 = e.json(&[
        "promise",
        "docs-ready",
        "--by",
        "2036-06-01T00:00:00Z",
        "-t",
        "release",
    ]);
    assert!(p2["receipt"].as_str().is_some());
    let c3 = e.json(&[
        "commitments",
        "-t",
        "release",
        "--at",
        "2036-07-01T00:00:00Z",
    ]);
    let states: Vec<_> = c3["commitments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["goal"].as_str().unwrap().to_string(),
                c["state"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(
        states.contains(&("docs-ready".into(), "violated".into())),
        "unfulfilled past-deadline promise must be violated at --at: {states:?}"
    );
    assert!(
        states.contains(&("legal-signed".into(), "fulfilled".into())),
        "fulfilled promise stays fulfilled: {states:?}"
    );
}

/// TEST-016: log shows every entry including retracted, with status labels.
#[test]
fn journal_shows_everything() {
    let e = Env::new();
    let a = e.json(&["assert", "qa-signed", "-t", "release"]);
    let sid = a["receipt"].as_str().unwrap().to_string();
    e.ok(&["retract", &sid, "-t", "release"]);

    let log = e.json(&["log", "-t", "release"]);
    let entries = log["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 3); // genesis + assert + retract
    let retracted = entries
        .iter()
        .find(|x| x["sid"].as_str() == Some(sid.as_str()))
        .unwrap();
    assert_eq!(retracted["status"], "retracted");

    // And the retracted fact is out of the closure.
    let s = e.json(&["status", "-t", "release"]);
    assert!(tag_of(&s, "qa-signed").as_deref() != Some("+D"));
}

/// TEST-018: two theories with identical literals stay disjoint.
#[test]
fn theories_are_disjoint() {
    let e = Env::new();
    e.cmd()
        .args(["theory", "create", "homelab"])
        .assert()
        .success();
    e.ok(&["assert", "qa-signed", "-t", "release"]);

    let s = e.json(&["status", "-t", "homelab"]);
    assert!(
        tag_of(&s, "qa-signed").is_none(),
        "literal must not leak across theories"
    );
}

/// SPEC-003 REQ-208 (ADR-205): describe/trace are flat commands, and the
/// v0.1 `query` group spelling is a usage error, not a silent alias.
#[test]
fn describe_and_trace_are_flat() {
    let e = Env::new();
    e.ok(&["assert", "qa-signed", "-t", "release"]);
    e.ok(&["assert", "(normally r-ok qa-signed ok)", "-t", "release"]);
    let tr = e.json(&["trace", "-t", "release"]);
    assert!(!tr["trace"].as_array().unwrap().is_empty());

    // TEST-208 positive: describe shows the rule for a known label…
    let d = e.json(&["describe", "r-ok", "-t", "release"]);
    let rule = d["labels"][0]["rule"].as_str().expect("rule spl present");
    assert!(rule.contains("qa-signed"));
    // …negative: unknown label is a clean miss, not an error.
    let miss = e.json(&["describe", "no-such-label", "-t", "release"]);
    assert!(miss["labels"][0]["rule"].is_null());

    e.cmd()
        .args(["query", "why-not", "nope", "-t", "release"])
        .assert()
        .failure();
}

/// #12: an un-adjudicated mutual conflict is reported as a first-class
/// `ambiguity` block naming the opposing rule, not an `undetermined`
/// diagnostics-gap fallthrough.
#[test]
fn why_not_reports_ambiguity_blocking() {
    let e = Env::new();
    // Two-level mutual block: lab-origin and zoonotic-origin each attack the
    // other, both supported, no preference — both settle at -D.
    e.ok(&["assert", "ev-lab", "-t", "release"]);
    e.ok(&["assert", "ev-zoo", "-t", "release"]);
    e.ok(&["assert", "(normally r-lab ev-lab lab-origin)", "-t", "release"]);
    e.ok(&["assert", "(normally r-zoo ev-zoo zoonotic-origin)", "-t", "release"]);
    e.ok(&["assert", "(normally r-lab-not-zoo lab-origin (not zoonotic-origin))", "-t", "release"]);
    e.ok(&["assert", "(normally r-zoo-not-lab zoonotic-origin (not lab-origin))", "-t", "release"]);

    let w = e.json(&["why-not", "lab-origin", "-t", "release"]);
    let b = w["blocked_by"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["type"] == "ambiguity")
        .unwrap_or_else(|| panic!("expected an ambiguity blocker, got: {w}"));
    assert_eq!(b["type"], "ambiguity");
    assert!(
        b["opposing_rules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r == "r-zoo-not-lab"),
        "ambiguity must name the opposing rule: {b}"
    );
    assert!(
        b["opposing_literal"].as_str().unwrap().contains("lab-origin"),
        "ambiguity must name the opposing literal: {b}"
    );
    assert!(b["explanation"].as_str().unwrap().contains("prefer"), "explanation hints at adjudication");
}

/// #9: `show <sentence-id>` inspects one entry without dumping the journal;
/// a miss reuses the same "no entry with sentence-id …" error.
#[test]
fn show_inspects_single_entry_by_id() {
    let e = Env::new();
    let a = e.json(&["assert", "qa-signed", "-t", "release"]);
    let sid = a["receipt"].as_str().unwrap().to_string();

    let s = e.json(&["show", &sid, "-t", "release"]);
    assert_eq!(s["sentence_id"].as_str(), Some(sid.as_str()));
    assert_eq!(s["performative"], "assert");
    assert_eq!(s["status"], "active");
    assert!(s["spl_form"].as_str().unwrap().contains("qa-signed"));
    assert!(!s["signer"].as_str().unwrap().is_empty());
    assert!(!s["entry"].is_null(), "raw entry object present in --json");

    // After retraction the same id shows status retracted.
    e.ok(&["retract", &sid, "-t", "release"]);
    let s2 = e.json(&["show", &sid, "-t", "release"]);
    assert_eq!(s2["status"], "retracted");

    // A miss is a clean not-found, not a crash.
    let out = e
        .cmd()
        .args(["show", "s-doesnotexist0000", "-t", "release"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("no entry with sentence-id"),
        "miss must reuse the standard not-found error"
    );
}

/// #11: what-if rejects a `(prefer …)`/rule hypothetical with a clear error
/// instead of silently coercing it into an inert fact and reporting no change.
#[test]
fn what_if_rejects_structural_hypothetical() {
    let e = Env::new();
    // Symmetric mutual attack, no preference: `lab` and `zoo` both block.
    e.ok(&["assert", "(normally r-lab lab-seed (not zoo))", "-t", "release"]);
    e.ok(&["assert", "(normally r-zoo zoo-seed (not lab))", "-t", "release"]);
    e.ok(&["assert", "lab-seed", "-t", "release"]);
    e.ok(&["assert", "zoo-seed", "-t", "release"]);

    // A prefer hypothetical is refused, not silently ignored.
    let out = e
        .cmd()
        .args(["what-if", "(prefer r-lab r-zoo)", "lab", "-t", "release"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "structural hypothetical must be refused");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("must be facts") && err.contains("prefer"),
        "error must explain prefer is not a fact hypothetical: {err}"
    );

    // A plain fact hypothetical (even a typed/parenthesised one) still works.
    let wi = e.json(&["what-if", "(engineered lab)", "lab-seed", "-t", "release"]);
    assert_eq!(wi["provable"], true, "fact hypothetical still evaluated: {wi}");
}

/// #14: retract entries are individually addressable — a stable `entry_id`
/// and a first-class `retracts` target — so an order-independent journal
/// fingerprint no longer collapses every retract onto a null `sid`.
#[test]
fn log_retracts_are_addressable() {
    let e = Env::new();
    let a1 = e.json(&["assert", "one", "-t", "release"]);
    let sid1 = a1["receipt"].as_str().unwrap().to_string();
    let a2 = e.json(&["assert", "two", "-t", "release"]);
    let sid2 = a2["receipt"].as_str().unwrap().to_string();
    e.ok(&["retract", &sid1, "-t", "release"]);
    e.ok(&["retract", &sid2, "-t", "release"]);

    let log = e.json(&["log", "-t", "release"]);
    let entries = log["entries"].as_array().unwrap();

    // Every entry carries a stable, non-null entry_id, and they are distinct
    // even across the two retracts (which both have a null `sid`).
    let ids: Vec<&str> = entries
        .iter()
        .map(|x| {
            let id = x["entry_id"].as_str().expect("every entry has an entry_id");
            assert!(!id.is_empty());
            id
        })
        .collect();
    let mut uniq = ids.clone();
    uniq.sort_unstable();
    uniq.dedup();
    assert_eq!(uniq.len(), ids.len(), "entry_ids must be distinct: {ids:?}");

    // Each retract names its target sid as a first-class field, with sid null.
    let retracts: Vec<(&str, Option<&str>)> = entries
        .iter()
        .filter(|x| x["performative"] == "retract")
        .map(|x| (x["retracts"].as_str().unwrap(), x["sid"].as_str()))
        .collect();
    assert_eq!(retracts.len(), 2);
    assert!(retracts.iter().all(|(_, sid)| sid.is_none()), "retract sid stays null");
    let targets: Vec<&str> = retracts.iter().map(|(t, _)| *t).collect();
    assert!(targets.contains(&sid1.as_str()), "retracts sid1: {targets:?}");
    assert!(targets.contains(&sid2.as_str()), "retracts sid2: {targets:?}");

    // For an assert, entry_id equals its own sentence-id (same derivation).
    let assert1 = entries
        .iter()
        .find(|x| x["sid"].as_str() == Some(sid1.as_str()))
        .unwrap();
    assert_eq!(assert1["entry_id"].as_str(), Some(sid1.as_str()));
}

/// #13: explain on a blocked literal is a self-describing stub, not a bare
/// `null` that reads like a serialization failure.
#[test]
fn explain_blocked_literal_hints_at_why_not() {
    let e = Env::new();
    // A rule with an unmet premise: `blocked` is -D, so explain has nothing
    // to derive.
    e.ok(&["assert", "(normally r-b missing-fact blocked)", "-t", "release"]);
    let ex = e.json(&["explain", "blocked", "-t", "release"]);
    assert!(ex["explanation"].is_null(), "no derivation for a blocked literal");
    assert_eq!(ex["not_provable"], true, "must flag why the explanation is null: {ex}");
    assert!(
        ex["hint"].as_str().unwrap_or_default().contains("why-not"),
        "must point at why-not: {ex}"
    );
}

/// #10: describe --json meta values are plain JSON strings, not Rust Debug
/// (`String("…")`) wrappers. The --json contract is a stable data interface.
#[test]
fn describe_json_meta_values_are_plain_strings() {
    let e = Env::new();
    e.ok(&[
        "assert",
        "(meta deploy-thing (description \"ship it\"))",
        "-t",
        "release",
    ]);
    let d = e.json(&["describe", "deploy-thing", "-t", "release"]);
    let meta = &d["labels"][0]["meta"];
    assert_eq!(
        meta["description"], "ship it",
        "meta value must be a plain JSON string, not a Debug-wrapped String(\"…\"): {meta}"
    );
    // Guard the exact regression: no Rust Debug syntax may survive into JSON.
    assert!(
        !d.to_string().contains("String("),
        "no Debug String(…) wrapper may leak into describe --json: {d}"
    );
}

// ── SPEC-005 TEST-405: docs join in why-not / require ───────────────────

/// Documented and built-in families join why-not/require output; the docs
/// object carries family_kind/description/kind/asserter/built_in and
/// never leaks provenance fields.
#[test]
fn test_405_docs_join() {
    let e = Env::new();
    // Ground-instance rule: spindle's why-not surfaces exact ground missing
    // literals for it. (A variable-headed rule is not unified against a
    // ground goal by spindle's why-not/require today — recorded discovery,
    // IMPL-005; REQ-405 joins whatever those surfaces return.)
    e.ok(&[
        "assert",
        "(normally r-verified (and (ci-green m1) (review-approved m1)) (verified m1))",
        "-t",
        "release",
    ]);
    e.ok(&["assert", "(given (review-approved m1))", "-t", "release"]);
    e.ok(&[
        "define",
        "ci-green/1",
        "--desc",
        "CI pipeline green for task ?t",
        "--kind",
        "evidence",
        "--asserter",
        "role:ci",
        "-t",
        "release",
    ]);

    let w = e.json(&["why-not", "(verified m1)", "-t", "release"]);
    let docs = &w["docs"]["ci-green/1"];
    assert_eq!(docs["family_kind"], "predicate");
    assert_eq!(docs["description"], "CI pipeline green for task ?t");
    assert_eq!(docs["kind"], "evidence");
    assert_eq!(docs["asserter"], "role:ci");
    assert_eq!(docs["built_in"], false);
    assert!(docs.get("documenter").is_none(), "no provenance leak");
    assert!(docs.get("redefined").is_none(), "no provenance leak");

    let r = e.json(&["require", "(verified m1)", "-t", "release"]);
    assert_eq!(
        r["docs"]["ci-green/1"]["description"],
        "CI pipeline green for task ?t"
    );
}

/// Built-in registry entries join too (fixed description, built_in true).
#[test]
fn test_405_builtin_docs_join() {
    let e = Env::new();
    e.ok(&["assert", "(given task-m1)", "-t", "release"]);
    e.ok(&[
        "assert",
        "(normally r-done (and completed-m1 verified-m1) all-done-m1)",
        "-t",
        "release",
    ]);
    let w = e.json(&["why-not", "all-done-m1", "-t", "release"]);
    let docs = &w["docs"];
    assert_eq!(docs["completed"]["built_in"], true);
    assert_eq!(
        docs["completed"]["description"],
        "task is finished (given fact; lifecycle terminal)"
    );
    assert_eq!(docs["verified"]["family_kind"], "legacy");
}

/// Undocumented families leave the output byte-identical to the pre-405
/// shape: no docs key at all.
#[test]
fn test_405_undocumented_shape_unchanged() {
    let e = Env::new();
    e.ok(&[
        "assert",
        "(normally r zz-undocumented-thing yy-goal)",
        "-t",
        "release",
    ]);
    let w = e.json(&["why-not", "yy-goal", "-t", "release"]);
    assert!(
        w.get("docs").is_none(),
        "no docs key when nothing documented"
    );
    let r = e.json(&["require", "yy-goal", "-t", "release"]);
    assert!(r.get("docs").is_none());
}

/// SPEC-005 TEST-407 (CLI surface): same-signer documentation updates are
/// never annotated in the journal (the cross-signer annotation kernel is
/// unit-tested in core::vocab).
#[test]
fn test_407_log_no_annotation_for_same_signer() {
    let e = Env::new();
    e.ok(&[
        "assert",
        "(meta deploy-thing (description \"v1\"))",
        "-t",
        "release",
    ]);
    e.ok(&[
        "assert",
        "(meta deploy-thing (description \"v2\"))",
        "-t",
        "release",
    ]);
    let log = e.json(&["log", "-t", "release"]);
    assert!(
        log["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i.get("redefines").is_none()),
        "same-signer update must not be annotated: {log}"
    );
}
