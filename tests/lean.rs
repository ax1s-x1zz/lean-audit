//! Real-Lean integration tests.
//!
//! These run `lake env lean` for real, against the small fixture project in
//! `tests/fixtures/leanfix`. They are skipped when no `lake` is available, so the
//! suite still runs on a machine without Lean; `tests/axioms.rs` covers the same
//! logic with a stub.
//!
//! The fixture is deliberately shaped like a TEAM42 repository: a library
//! directory (`LeanFix/`), a `Challenge.lean` whose `sorry` is intentional, and an
//! `Audit.lean` that scopes its `sorry` scan with `getRoot == \`LeanFix`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn lake_path() -> Option<PathBuf> {
    if let Ok(out) = Command::new("lake").arg("--version").output()
        && out.status.success()
    {
        return Some(PathBuf::from("lake"));
    }
    let home = std::env::var("HOME").ok()?;
    let candidate = PathBuf::from(home).join(".elan/bin/lake");
    candidate.is_file().then_some(candidate)
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/leanfix")
}

fn run_fixture(lake: &Path, extra: &[&str]) -> (serde_json::Value, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_lean-audit"))
        .arg("axioms")
        .arg(fixture())
        .arg("--lake")
        .arg(lake)
        .args(["--format", "json"])
        .args(extra)
        .output()
        .expect("run lean-audit");
    let value: serde_json::Value =
        serde_json::from_slice(&out.stdout).unwrap_or(serde_json::Value::Null);
    (value, out.status.code().unwrap_or(-1))
}

#[test]
fn computes_axioms_with_real_lean() {
    let Some(lake) = lake_path() else {
        eprintln!("skipping: no lake on PATH or in ~/.elan/bin");
        return;
    };

    // --build is needed on a fresh checkout; it is a no-op afterwards.
    let (v, code) = run_fixture(&lake, &["--build"]);
    let r = &v["result"];

    assert_eq!(r["run"], true, "Lean should have been executed: {r}");
    assert_eq!(r["heads_source"], "audit_file");
    assert_eq!(
        r["returncode"], 0,
        "lake env lean failed: {}",
        r["stderr_tail"]
    );

    let heads = r["heads"].as_array().unwrap();
    assert_eq!(heads.len(), 2);

    // `theorem head_true : True := trivial` depends on nothing.
    assert_eq!(heads[0]["name"], "head_true");
    assert_eq!(heads[0]["axioms"], serde_json::json!([]));

    // `Classical.em` pulls in exactly the three standard axioms.
    assert_eq!(heads[1]["name"], "head_classical");
    assert_eq!(
        heads[1]["axioms"],
        serde_json::json!(["propext", "Classical.choice", "Quot.sound"])
    );

    assert!(r["unexpected_axioms"].as_array().unwrap().is_empty());
    assert!(r["missing"].as_array().unwrap().is_empty());

    // The library's `sorry` user is found; Challenge.lean's deliberate `sorry` is
    // outside the scope the repository declares, so it must not appear here.
    assert_eq!(r["sorry_scope"], serde_json::json!(["LeanFix"]));
    assert_eq!(r["sorry_scope_source"], "audit_file");
    assert_eq!(r["direct_sorry"], serde_json::json!(["stub_unproved"]));

    // The fixture's audit publishes an expectation of zero, so this differs.
    assert_eq!(r["status"], "mismatch");
    assert_eq!(code, 3);
}

#[test]
fn reports_a_missing_head_instead_of_guessing() {
    let Some(lake) = lake_path() else {
        eprintln!("skipping: no lake on PATH or in ~/.elan/bin");
        return;
    };
    let (v, code) = run_fixture(&lake, &["--build", "--heads", "head_true,does_not_exist"]);
    let r = &v["result"];
    assert_eq!(r["missing"], serde_json::json!(["does_not_exist"]));
    assert_eq!(r["status"], "mismatch");
    assert_eq!(code, 3);
}

#[test]
fn an_unbuilt_repository_is_reported_as_error_not_ok() {
    let Some(lake) = lake_path() else {
        eprintln!("skipping: no lake on PATH or in ~/.elan/bin");
        return;
    };
    // Copy the fixture without its build output so imports cannot resolve.
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("unbuilt");
    copy_tree(&fixture(), &dst);
    std::fs::remove_dir_all(dst.join(".lake")).ok();

    let out = Command::new(env!("CARGO_BIN_EXE_lean-audit"))
        .arg("axioms")
        .arg(&dst)
        .arg("--lake")
        .arg(&lake)
        .args(["--format", "json"])
        .output()
        .expect("run lean-audit");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["result"]["status"], "error",
        "an unbuilt repository must not be reported as ok"
    );
    assert_eq!(out.status.code(), Some(4));
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().filter_map(Result::ok) {
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            if entry.file_name() == ".lake" {
                continue;
            }
            copy_tree(&src, &dst);
        } else {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}
