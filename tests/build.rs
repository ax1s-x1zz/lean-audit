//! M6 `build` integration tests.
//!
//! These run `lake build` for real against `tests/fixtures/buildclaim`, a
//! one-module project with a `receipts/build_summary.txt`. Skipped when no
//! `lake` is available, so the suite still runs without Lean.

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
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/buildclaim")
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() == ".lake" {
            continue;
        }
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn run_build(dir: &Path, lake: &Path) -> (serde_json::Value, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_lean-audit"))
        .arg("build")
        .arg(dir)
        .arg("--clean")
        .arg("--lake")
        .arg(lake)
        .args(["--format", "json"])
        .output()
        .expect("run lean-audit build");
    let value: serde_json::Value =
        serde_json::from_slice(&out.stdout).unwrap_or(serde_json::Value::Null);
    (value, out.status.code().unwrap_or(-1))
}

#[test]
fn build_matches_the_receipt_claim() {
    let Some(lake) = lake_path() else {
        eprintln!("skipping: no lake on PATH or in ~/.elan/bin");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&fixture(), dir.path());

    let (v, code) = run_build(dir.path(), &lake);
    assert_eq!(code, 0, "expected ok, got {code}: {v}");
    assert_eq!(v["status"], "ok");
    assert_eq!(v["result"]["clean"], true);
    assert_eq!(v["result"]["modules_built"], 1);
    let claims = v["claims"].as_array().unwrap();
    assert!(!claims.is_empty(), "no claims were parsed: {v}");
    assert!(
        claims.iter().all(|c| c["matches"] == true),
        "every claim should match: {claims:?}"
    );
}

#[test]
fn build_reports_a_claim_mismatch() {
    let Some(lake) = lake_path() else {
        eprintln!("skipping: no lake on PATH or in ~/.elan/bin");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&fixture(), dir.path());
    std::fs::write(
        dir.path().join("receipts/build_summary.txt"),
        "modules built (lines '[k/n] Built'): 999\n",
    )
    .unwrap();

    let (v, code) = run_build(dir.path(), &lake);
    assert_eq!(code, 3, "expected mismatch exit 3, got {code}: {v}");
    assert_eq!(v["status"], "mismatch");
    assert_eq!(v["claims"][0]["metric"], "modules_built");
    assert_eq!(v["claims"][0]["matches"], false);
}
