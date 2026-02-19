use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent dirs");
    }
    fs::write(path, content).expect("write file");
}

#[test]
fn pack_is_deterministic_for_same_tree() {
    let dir = tempdir().expect("temp dir");
    write(&dir.path().join("b.yml"), "z: 2\na: 1\n");
    write(&dir.path().join("a.yml"), "v: 3\n");

    let output_1 = Command::cargo_bin("fyaml")
        .expect("binary")
        .args(["pack", dir.path().to_str().expect("utf8 path"), "--no-header"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_2 = Command::cargo_bin("fyaml")
        .expect("binary")
        .args(["pack", dir.path().to_str().expect("utf8 path"), "--no-header"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert_eq!(output_1, output_2);
}

#[test]
fn validate_json_reports_collision() {
    let dir = tempdir().expect("temp dir");
    write(&dir.path().join("auth.yml"), "kind: file\n");
    write(&dir.path().join("auth/provider.yml"), "kind: dir\n");

    let output = Command::cargo_bin("fyaml")
        .expect("binary")
        .args(["validate", dir.path().to_str().expect("utf8 path"), "--json"])
        .assert()
        .failure()
        .code(2)
        .get_output()
        .stdout
        .clone();

    let diagnostics: Value =
        serde_json::from_slice(&output).expect("validate --json should return JSON diagnostics");
    let list = diagnostics.as_array().expect("diagnostics array");
    assert!(list
        .iter()
        .any(|d| d.get("code").and_then(Value::as_str) == Some("E001")));
}

#[test]
fn explain_lists_ignored_entries() {
    let dir = tempdir().expect("temp dir");
    write(&dir.path().join("a.yml"), "x: 1\n");
    write(&dir.path().join("notes.txt"), "ignore me\n");

    Command::cargo_bin("fyaml")
        .expect("binary")
        .args(["explain", dir.path().to_str().expect("utf8 path")])
        .assert()
        .success()
        .stdout(predicate::str::contains("Ignored Entries"))
        .stdout(predicate::str::contains("notes.txt"));
}

#[test]
fn diff_reports_equal_for_semantically_identical_trees() {
    let left = tempdir().expect("left temp dir");
    let right = tempdir().expect("right temp dir");

    write(&left.path().join("env/prod/database.yml"), "host: db\nport: 5432\n");
    write(&left.path().join("env/prod/cache.yml"), "ttl: 60\n");

    write(&right.path().join("env.yml"), "prod:\n  cache:\n    ttl: 60\n  database:\n    host: db\n    port: 5432\n");

    Command::cargo_bin("fyaml")
        .expect("binary")
        .args([
            "diff",
            left.path().to_str().expect("utf8 path"),
            right.path().to_str().expect("utf8 path"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("equal"));
}

#[test]
fn reserved_word_filename_fails_by_default() {
    let dir = tempdir().expect("temp dir");
    write(&dir.path().join("true.yml"), "x: 1\n");

    Command::cargo_bin("fyaml")
        .expect("binary")
        .args(["validate", dir.path().to_str().expect("utf8 path")])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("reserved YAML key"));
}

#[test]
fn reserved_word_filename_allowed_with_flag() {
    let dir = tempdir().expect("temp dir");
    write(&dir.path().join("true.yml"), "x: 1\n");

    Command::cargo_bin("fyaml")
        .expect("binary")
        .args([
            "validate",
            dir.path().to_str().expect("utf8 path"),
            "--allow-reserved-keys",
        ])
        .assert()
        .success();
}

#[test]
fn scaffold_then_pack_keeps_semantics() {
    let input_root = tempdir().expect("input temp dir");
    let scaffold_root = tempdir().expect("scaffold temp dir");
    let input = input_root.path().join("input.yml");
    let scaffold_dir = scaffold_root.path().join("scaffold");

    write(
        &input,
        "name: app\nsteps:\n  - extract\n  - transform\n  - load\n",
    );

    Command::cargo_bin("fyaml")
        .expect("binary")
        .args([
            "scaffold",
            input.to_str().expect("utf8 path"),
            scaffold_dir.to_str().expect("utf8 path"),
        ])
        .assert()
        .success();

    let packed_scaffold = Command::cargo_bin("fyaml")
        .expect("binary")
        .args([
            "pack",
            scaffold_dir.to_str().expect("utf8 path"),
            "--no-header",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let packed_input = Command::cargo_bin("fyaml")
        .expect("binary")
        .args([
            "pack",
            input_root.path().to_str().expect("utf8 path"),
            "--root-mode",
            "file-root",
            "--root-file",
            "input.yml",
            "--no-header",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert_eq!(packed_scaffold, packed_input);
}
