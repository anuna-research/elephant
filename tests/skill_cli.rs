use assert_cmd::Command;
use std::fs;

fn cmd(dir: &tempfile::TempDir) -> Command {
    let mut cmd = Command::cargo_bin("elephant").unwrap();
    cmd.current_dir(dir.path())
        .env("ELEPHANT_HOME", dir.path().join("unused-store"));
    cmd
}

#[test]
fn installs_offline_and_preserves_existing_edits() {
    let dir = tempfile::tempdir().unwrap();
    let args = ["skill", "init", "--json"];
    let out = cmd(&dir)
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let result: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(result["status"], "installed");
    let path = dir.path().join(".agents/skills/elephant/SKILL.md");
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains(elephant::VERSION));
    assert!(!content.contains("{{ELEPHANT_VERSION}}"));
    assert!(!dir.path().join("unused-store").exists());
    cmd(&dir)
        .args(args)
        .assert()
        .success()
        .stdout(predicates::str::contains("unchanged"));
    fs::write(&path, "local edits").unwrap();
    cmd(&dir).args(args).assert().code(2);
    assert_eq!(fs::read_to_string(path).unwrap(), "local edits");
}

#[test]
fn preview_matches_custom_install_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    let out = cmd(&dir)
        .args(["skill", "init", "--dry-run", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let preview: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(preview["status"], "preview");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    cmd(&dir)
        .args(["skill", "init", "--path", ".claude/skills/elephant"])
        .assert()
        .success();
    let installed =
        fs::read_to_string(dir.path().join(".claude/skills/elephant/SKILL.md")).unwrap();
    assert_eq!(installed, preview["content"].as_str().unwrap());
    cmd(&dir)
        .args(["skill", "init", "--dry-run"])
        .assert()
        .success()
        .stdout(installed);
}

#[test]
fn rejects_theory_flags_and_unwritable_destination() {
    let dir = tempfile::tempdir().unwrap();
    for flags in [["--at", "2026-09-18T00:00:00Z"], ["-t", "demo"]] {
        cmd(&dir)
            .args(["skill", "init"])
            .args(flags)
            .assert()
            .code(1);
    }
    fs::write(dir.path().join("file"), "keep").unwrap();
    cmd(&dir)
        .args(["skill", "init", "--path", "file/elephant"])
        .assert()
        .code(2);
    assert_eq!(fs::read_to_string(dir.path().join("file")).unwrap(), "keep");
}
