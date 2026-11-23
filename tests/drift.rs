//! `drift` compares two report JSON files. These tests drive the real binary.

use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_lean-audit")
}

fn fixture() -> String {
    format!("{}/tests/fixtures/leanfix", env!("CARGO_MANIFEST_DIR"))
}

/// Produce an `inspect --format json` report for the fixture.
///
/// `inspect` exits 2 when it finds forbidden tokens in scope; the fixture has a
/// deliberate `sorry` stub, so accept 0 or 2. The report is written either way.
fn inspect_json(out: &std::path::Path) {
    let status = Command::new(bin())
        .args(["inspect", &fixture(), "--format", "json", "--out"])
        .arg(out)
        .status()
        .expect("run inspect");
    assert!(
        matches!(status.code(), Some(0) | Some(2)),
        "inspect exited with {status}"
    );
}

#[test]
fn no_drift_on_identical_reports() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.json");
    let b = dir.path().join("b.json");
    inspect_json(&a);
    std::fs::copy(&a, &b).unwrap();

    let out = Command::new(bin())
        .arg("drift")
        .arg(&a)
        .arg(&b)
        .output()
        .expect("run drift");
    assert!(out.status.success(), "expected exit 0, got {}", out.status);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("No drift"), "unexpected output:\n{text}");
}

#[test]
fn drift_is_reported_with_exit_3() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.json");
    let b = dir.path().join("b.json");
    inspect_json(&a);

    // Change one compared field.
    let text = std::fs::read_to_string(&a).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    let count = value["modules"]["count"].as_u64().unwrap();
    value["modules"]["count"] = serde_json::json!(count + 1);
    std::fs::write(&b, serde_json::to_string_pretty(&value).unwrap()).unwrap();

    let out = Command::new(bin())
        .arg("drift")
        .arg(&a)
        .arg(&b)
        .output()
        .expect("run drift");
    assert_eq!(out.status.code(), Some(3), "expected exit 3");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("modules.count"), "unexpected output:\n{text}");
}
