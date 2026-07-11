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
