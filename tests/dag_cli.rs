//! `elephant dag` — the whole theory as a layered dependency graph.

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

    fn text(&self, args: &[&str]) -> String {
        let out = self.cmd().args(args).output().unwrap();
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout).to_string()
    }
}

fn seed_release_theory(e: &Env) {
    e.ok(&["assert", "qa-signed", "-t", "release"]);
    e.ok(&["assert", "legal-signed", "-t", "release"]);
    e.ok(&[
        "assert",
        "(normally r-rel (and qa-signed legal-signed) release-ready)",
        "-t",
        "release",
    ]);
    e.ok(&[
        "assert",
        "(normally r-embargo embargo-active (not release-ready))",
        "-t",
        "release",
    ]);
    e.ok(&["assert", "(prefer r-embargo r-rel)", "-t", "release"]);
}

#[test]
fn dag_json_graph_is_complete() {
    let e = Env::new();
    seed_release_theory(&e);
    let v = e.json(&["dag", "-t", "release"]);

    let node = |lit: &str| -> &serde_json::Value {
        v["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["literal"] == lit)
            .unwrap_or_else(|| panic!("no node {lit}"))
    };
    // Facts at layer 0 with definite tags; the derived gate below them.
    assert_eq!(node("qa-signed")["tag"], "+D");
    assert_eq!(node("qa-signed")["layer"], 0);
    assert_eq!(node("release-ready")["tag"], "+d");
    assert_eq!(node("release-ready")["layer"], 1);
    // A negated head lands on the positive atom's node, flagged as attack.
    let edges = v["edges"].as_array().unwrap();
    let attack = edges
        .iter()
        .find(|ed| ed["rule"] == "r-embargo")
        .expect("r-embargo edge");
    assert_eq!(attack["from"], "embargo-active");
    assert_eq!(attack["to"], "release-ready");
    assert_eq!(attack["head_negated"], true);
    // Both r-rel body literals become edges into the head.
    let rel_from: Vec<&str> = edges
        .iter()
        .filter(|ed| ed["rule"] == "r-rel")
        .map(|ed| ed["from"].as_str().unwrap())
        .collect();
    assert!(rel_from.contains(&"qa-signed") && rel_from.contains(&"legal-signed"));
    // Preferences travel with the graph.
    assert_eq!(v["superiorities"][0]["superior"], "r-embargo");
    assert_eq!(v["superiorities"][0]["inferior"], "r-rel");
    assert_eq!(v["cycles"].as_array().unwrap().len(), 0);
}

#[test]
fn dag_text_draws_layers_and_preferences() {
    let e = Env::new();
    seed_release_theory(&e);
    let out = e.text(&["dag", "-t", "release"]);
    assert!(out.contains("[+D qa-signed]"), "facts row missing: {out}");
    assert!(
        out.contains("[+d release-ready]"),
        "derived row missing: {out}"
    );
    assert!(
        out.contains("r-embargo > r-rel"),
        "preferences missing: {out}"
    );
    // Piped output must be pure ASCII (box-drawing is TTY-only).
    assert!(out.is_ascii(), "non-ASCII in piped output: {out}");
    // The facts row must be printed before the derived row.
    assert!(
        out.find("[+D qa-signed]").unwrap() < out.find("[+d release-ready]").unwrap(),
        "layers out of order: {out}"
    );
}

#[test]
fn dag_breaks_cycles_and_reports_them() {
    let e = Env::new();
    e.ok(&["assert", "(normally r-c1 ping pong)", "-t", "release"]);
    e.ok(&["assert", "(normally r-c2 pong ping)", "-t", "release"]);
    let v = e.json(&["dag", "-t", "release"]);
    assert_eq!(v["cycles"].as_array().unwrap().len(), 1);
    // Both nodes still get a layer despite the cycle.
    for n in v["nodes"].as_array().unwrap() {
        assert!(n["layer"].is_number(), "unlayered node: {n}");
    }
    // And the text renderer terminates and mentions the cycle.
    let out = e.text(&["dag", "-t", "release"]);
    assert!(out.contains("cycles (not drawn):"), "{out}");
}

#[test]
fn dag_focus_restricts_to_cone() {
    let e = Env::new();
    seed_release_theory(&e);
    // An unrelated island that must disappear under focus.
    e.ok(&[
        "assert",
        "(normally r-side lamp-on room-lit)",
        "-t",
        "release",
    ]);
    e.ok(&[
        "assert",
        "(normally r-ship release-ready shipped)",
        "-t",
        "release",
    ]);

    let v = e.json(&["dag", "--focus", "release-ready", "-t", "release"]);
    assert_eq!(v["focus"], "release-ready");
    let lits: Vec<&str> = v["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["literal"].as_str().unwrap())
        .collect();
    // Ancestors and descendants stay; the island goes.
    for kept in ["qa-signed", "legal-signed", "embargo-active", "shipped"] {
        assert!(lits.contains(&kept), "missing {kept}: {lits:?}");
    }
    assert!(!lits.contains(&"lamp-on") && !lits.contains(&"room-lit"));
    // Preferences shrink to rules touching the cone.
    assert_eq!(v["superiorities"].as_array().unwrap().len(), 1);

    // Text mode reports the focus and theory-wide totals.
    let out = e.text(&["dag", "--focus", "release-ready", "-t", "release"]);
    assert!(out.contains("focus release-ready:"), "{out}");

    // Unknown focus literal is a clean error.
    e.cmd()
        .args(["dag", "--focus", "no-such-literal", "-t", "release"])
        .assert()
        .failure();
}

#[test]
fn dag_empty_theory() {
    let e = Env::new();
    let out = e.text(&["dag", "-t", "release"]);
    assert!(out.contains("(empty theory)"));
    let v = e.json(&["dag", "-t", "release"]);
    assert_eq!(v["nodes"].as_array().unwrap().len(), 0);
}
