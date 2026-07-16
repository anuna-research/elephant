//! CLI observability follow-ups from the grounded two-machine run:
//!   #20 canonical closure fingerprint + compare
//!   #22 journal accounting (`theory inspect`) + `log` filters
//!   #26 canonical plain-language proof states
//! Driven end-to-end through the binary, direct-store mode (no daemon).

use assert_cmd::Command;

struct Env {
    dir: tempfile::TempDir,
    home: String,
}

impl Env {
    fn named(name: &str) -> Env {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_string_lossy().to_string();
        let e = Env { dir, home };
        e.cmd()
            .args(["id", "create", "--name", name])
            .assert()
            .success();
        e.cmd()
            .args(["theory", "create", "release"])
            .assert()
            .success();
        e
    }
    fn new() -> Env {
        Env::named("alice")
    }
    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("elephant").unwrap();
        c.env("ELEPHANT_HOME", &self.home);
        c
    }
    fn ok(&self, args: &[&str]) {
        self.cmd().args(args).assert().success();
    }
    fn json(&self, args: &[&str]) -> serde_json::Value {
        let out = self.cmd().args(args).arg("--json").output().unwrap();
        assert!(
            out.status.success(),
            "command {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).expect("stdout must be JSON")
    }
    /// Write `status --json` to a file under the temp dir and return its path.
    fn status_file(&self, name: &str) -> std::path::PathBuf {
        let out = self
            .cmd()
            .args(["status", "-t", "release", "--json"])
            .output()
            .unwrap();
        assert!(out.status.success());
        let p = self.dir.path().join(name);
        std::fs::write(&p, &out.stdout).unwrap();
        p
    }
}

/// A small penguin theory: a defeasible flies-rule, a superior penguin-rule,
/// and the two facts, so the closure has a mix of positive and negative tags.
fn seed_penguin(e: &Env) {
    e.ok(&["assert", "bird", "-t", "release"]);
    e.ok(&["assert", "penguin", "-t", "release"]);
    e.ok(&["assert", "(normally r-fly bird flies)", "-t", "release"]);
    e.ok(&[
        "assert",
        "(normally r-peng penguin (not flies))",
        "-t",
        "release",
    ]);
    e.ok(&["assert", "(prefer r-peng r-fly)", "-t", "release"]);
}

fn row<'a>(v: &'a serde_json::Value, lit: &str) -> Option<&'a serde_json::Value> {
    v["conclusions"]
        .as_array()?
        .iter()
        .find(|c| c["literal"] == lit)
}

// ── #26 canonical proof states ──────────────────────────────────────────

#[test]
fn status_carries_canonical_proof_states() {
    let e = Env::new();
    seed_penguin(&e);
    let s = e.json(&["status", "-t", "release"]);
    assert_eq!(s["proof_state_map"], 1, "status carries the map version");

    // bird is a strict fact: +D → definitely_provable.
    let bird = row(&s, "bird").expect("bird present");
    assert_eq!(bird["tag"], "+D");
    assert_eq!(bird["proof_state"], "definitely_provable");
    assert_eq!(bird["positive"], true);
    assert_eq!(bird["level"], "definite");

    // (not (flies)) wins by preference: +d → defeasibly_provable.
    let nf = row(&s, "(not (flies))").expect("(not (flies)) present");
    assert_eq!(nf["tag"], "+d");
    assert_eq!(nf["proof_state"], "defeasibly_provable");
    assert_eq!(nf["positive"], true);
    assert_eq!(nf["level"], "defeasible");

    // flies is defeated: not positive.
    let flies = row(&s, "flies").expect("flies present");
    assert_eq!(flies["positive"], false);
}

#[test]
fn verbose_text_shows_the_name() {
    let e = Env::new();
    e.ok(&["assert", "bird", "-t", "release"]);
    let out = e
        .cmd()
        .args(["-v", "status", "-t", "release"])
        .output()
        .unwrap();
    let txt = String::from_utf8_lossy(&out.stdout);
    assert!(
        txt.contains("definitely_provable"),
        "verbose text should name the state, got: {txt}"
    );
    // Default (non-verbose) text must NOT be noisy with the name.
    let out2 = e.cmd().args(["status", "-t", "release"]).output().unwrap();
    let txt2 = String::from_utf8_lossy(&out2.stdout);
    assert!(!txt2.contains("definitely_provable"));
}

// ── #20 closure fingerprint + compare ───────────────────────────────────

#[test]
fn fingerprint_agrees_across_surfaces() {
    let e = Env::new();
    seed_penguin(&e);
    let a = e.json(&["closure", "fingerprint", "-t", "release"]);
    let b = e.json(&["status", "-t", "release", "--fingerprint"]);
    assert_eq!(a["algorithm"], "elephant.closure.v1");
    assert_eq!(a["hash"], "sha256");
    assert!(a["included"].as_u64().unwrap() >= 3);
    assert_eq!(
        a["digest"], b["fingerprint"]["digest"],
        "closure fingerprint and status --fingerprint must agree"
    );
}

#[test]
fn fingerprint_is_identity_independent() {
    // Same content statements under two different identities/theory-ids must
    // fingerprint the same: membership, signers, and theory id are excluded.
    let alice = Env::named("alice");
    let bob = Env::named("bob");
    seed_penguin(&alice);
    seed_penguin(&bob);
    let fa = alice.json(&["closure", "fingerprint", "-t", "release"]);
    let fb = bob.json(&["closure", "fingerprint", "-t", "release"]);
    assert_eq!(
        fa["digest"], fb["digest"],
        "same corpus under different identity must match"
    );
}

#[test]
fn compare_match_and_mismatch_exit_codes() {
    let e = Env::new();
    seed_penguin(&e);
    let f1 = e.status_file("a.json");
    let f2 = e.status_file("b.json");

    // Identical closures → match, exit 0.
    let m = e.json(&[
        "closure",
        "compare",
        f1.to_str().unwrap(),
        f2.to_str().unwrap(),
    ]);
    assert_eq!(m["match"], true);
    e.cmd()
        .args([
            "closure",
            "compare",
            f1.to_str().unwrap(),
            f2.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Mutate one tag in the second file → mismatch, exit 10 (Predicate), and
    // the command itself did not error.
    let mut v: serde_json::Value = serde_json::from_slice(&std::fs::read(&f2).unwrap()).unwrap();
    v["conclusions"][0]["tag"] = serde_json::json!("-D");
    std::fs::write(&f2, serde_json::to_vec(&v).unwrap()).unwrap();
    e.cmd()
        .args([
            "closure",
            "compare",
            f1.to_str().unwrap(),
            f2.to_str().unwrap(),
        ])
        .assert()
        .code(10);
}

#[test]
fn compare_reports_bad_input() {
    let e = Env::new();
    let bad = e.dir.path().join("nope.json");
    std::fs::write(&bad, b"{\"not\":\"status\"}").unwrap();
    // No `conclusions` array → parse error (exit 3), distinct from a mismatch.
    e.cmd()
        .args([
            "closure",
            "compare",
            bad.to_str().unwrap(),
            bad.to_str().unwrap(),
        ])
        .assert()
        .code(3);
}

// ── #22 journal accounting + log filters ────────────────────────────────

#[test]
fn theory_inspect_counts_active_retracted_and_setup() {
    let e = Env::new();
    // Two content facts; retract one. Genesis meta is the single setup entry.
    e.ok(&["assert", "qa-signed", "-t", "release"]);
    let out = e
        .cmd()
        .args(["assert", "legal-signed", "-t", "release", "--json"])
        .output()
        .unwrap();
    let asserted: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let sid = asserted["receipt"].as_str().unwrap().to_string();
    e.ok(&["retract", &sid, "-t", "release"]);

    let insp = e.json(&["theory", "inspect", "release"]);
    // Genesis (meta theory …) is the one setup assertion; qa-signed is content.
    assert_eq!(insp["setup_assertions"], 1);
    assert_eq!(insp["content_assertions"], 1); // legal-signed retracted, qa-signed active
    assert_eq!(insp["active_assertions"], 2); // genesis + qa-signed
    assert_eq!(insp["retracted_assertions"], 1); // legal-signed
    assert_eq!(insp["retractions"], 1); // the retract entry
    // journal total = 2 active asserts + 1 retracted assert + 1 retract = 4.
    assert_eq!(insp["journal_entries"], 4);
}

#[test]
fn log_filters_compose_with_and() {
    let e = Env::new();
    e.ok(&["assert", "qa-signed", "-t", "release"]);
    let out = e
        .cmd()
        .args(["assert", "legal-signed", "-t", "release", "--json"])
        .output()
        .unwrap();
    let sid = serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()["receipt"]
        .as_str()
        .unwrap()
        .to_string();
    e.ok(&["retract", &sid, "-t", "release"]);

    // Only active asserts: genesis + qa-signed (legal-signed is retracted).
    let active_asserts = e.json(&[
        "log",
        "-t",
        "release",
        "--status",
        "active",
        "--performative",
        "assert",
    ]);
    let n = active_asserts["entries"].as_array().unwrap().len();
    assert_eq!(n, 2, "genesis + qa-signed are the active asserts");

    // Retracts only.
    let retracts = e.json(&["log", "-t", "release", "--performative", "retract"]);
    let r = retracts["entries"].as_array().unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0]["retracts"], serde_json::json!(sid));

    // Filter by retraction target.
    let by_target = e.json(&["log", "-t", "release", "--retracts", &sid]);
    assert_eq!(by_target["entries"].as_array().unwrap().len(), 1);

    // An impossible AND is a valid empty result, not an error.
    let empty = e.json(&[
        "log",
        "-t",
        "release",
        "--status",
        "retracted",
        "--performative",
        "retract",
    ]);
    assert_eq!(empty["entries"].as_array().unwrap().len(), 0);
}
