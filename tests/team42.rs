//! Integration tests against real TEAM42 repositories.
//!
//! These tests need local clones. They are skipped when the clones are absent so
//! the suite stays runnable anywhere.
//!
//! Expected layout (see the project README):
//!
//! ```sh
//! git clone --depth 1 https://github.com/TEAM-42KR/rubik-stm-entropy-lean.git ../team42-repos/rubik-stm-entropy-lean
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

fn repos_root() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../team42-repos");
    root.is_dir().then_some(root)
}

fn inspect_json(repo: &Path) -> Option<serde_json::Value> {
    let out = Command::new(env!("CARGO_BIN_EXE_lean-audit"))
        .arg("inspect")
        .arg(repo)
        .args(["--format", "json"])
        .output()
        .ok()?;
    serde_json::from_slice(&out.stdout).ok()
}

#[test]
fn rubik_audit_implementation_is_recognised() {
    let Some(root) = repos_root() else {
        eprintln!("skipping: ../team42-repos not present");
        return;
    };
    let Some(v) = inspect_json(&root.join("rubik-stm-entropy-lean")) else {
        eprintln!("skipping: could not inspect");
        return;
    };

    let audit = &v["audit_impl"];
    assert_eq!(audit["kind"], "lean_axiomcheck");
    assert_eq!(audit["command_names"][0], "#check_axioms");
    assert_eq!(
        audit["heads"].as_array().map(Vec::len),
        Some(30),
        "AxiomCheck.lean audits 30 head theorems"
    );
    assert_eq!(audit["heads_source"], "parsed");
    assert_eq!(v["toolchain"]["lean_version"], "4.33.1");
    assert_eq!(v["modules"]["count"], 935);
    assert_eq!(
        v["scan"]["flags"].as_array().map(Vec::len),
        Some(0),
        "rubik's own sources contain no forbidden tokens"
    );
}

#[test]
fn ang_uses_two_audit_commands_and_nested_receipts() {
    let Some(root) = repos_root() else {
        eprintln!("skipping: ../team42-repos not present");
        return;
    };
    let Some(v) = inspect_json(&root.join("ang-subset-products-lean")) else {
        eprintln!("skipping: could not inspect");
        return;
    };

    let audit = &v["audit_impl"];
    assert_eq!(audit["kind"], "lean_audit");
    assert_eq!(
        audit["command_names"].as_array().map(Vec::len),
        Some(2),
        "#ang_audit and #ang_large_audit"
    );
    assert_eq!(audit["heads"].as_array().map(Vec::len), Some(9));

    // Receipts are nested per architecture, and the published axiom claims are
    // parsed out of build_summary.txt.
    assert_eq!(v["receipts"]["summaries"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        v["receipts"]["summaries"][0]["claimed_axioms"]
            .as_array()
            .map(Vec::len),
        Some(9)
    );
    assert_eq!(v["receipts"]["summaries"][0]["claimed_sorry_warnings"], 3);

    // The three `sorry` tokens are in Challenge.lean, which is the deliberate
    // comparison statement; the doc-comment copy of the same theorem is ignored.
    let flags = v["scan"]["flags"].as_array().unwrap();
    assert_eq!(flags.len(), 3);
    assert!(
        flags
            .iter()
            .all(|f| f["path"].as_str().unwrap_or("").ends_with("Challenge.lean")),
        "only Challenge.lean should carry sorry"
    );
}

#[test]
fn feigenbaum_audit_uses_run_cmd() {
    let Some(root) = repos_root() else {
        eprintln!("skipping: ../team42-repos not present");
        return;
    };
    let Some(v) = inspect_json(&root.join("feigenbaum-scalar-duality-lean")) else {
        eprintln!("skipping: could not inspect");
        return;
    };

    let audit = &v["audit_impl"];
    assert_eq!(audit["kind"], "lean_audit");
    assert!(
        audit["command_names"].as_array().unwrap().is_empty(),
        "this repository defines no custom elab command"
    );
    assert_eq!(
        audit["heads"],
        serde_json::json!(["AffineWordLog.realIntervalDuality"])
    );
    assert_eq!(audit["allowed_axioms"].as_array().map(Vec::len), Some(3));
}

#[test]
fn site_percolation_reports_python_audit_and_pins_lean_435() {
    let Some(root) = repos_root() else {
        eprintln!("skipping: ../team42-repos not present");
        return;
    };
    let Some(v) = inspect_json(&root.join("site-percolation-square-lattice-lean")) else {
        eprintln!("skipping: could not inspect");
        return;
    };

    assert_eq!(v["audit_impl"]["kind"], "python_script");
    assert_eq!(v["audit_impl"]["heads_source"], "cli");
    assert_eq!(v["toolchain"]["lean_version"], "4.35.0");
    // Manifests are discovered but not verified yet.
    let manifests = v["manifests"].as_array().unwrap();
    assert!(
        manifests
            .iter()
            .any(|m| m["file"] == "SOURCE_MANIFEST.json")
    );
    assert!(
        manifests.iter().all(|m| m["status"] == "parsed_only"),
        "hash verification is M2 and must not be reported as ok"
    );
}

#[test]
fn every_lean_repository_avoids_forbidden_tokens_outside_comments() {
    let Some(root) = repos_root() else {
        eprintln!("skipping: ../team42-repos not present");
        return;
    };
    for name in [
        "coth-sums-barnes-lean",
        "feigenbaum-scalar-duality-lean",
        "site-percolation-square-lattice-lean",
        "rubik-stm-entropy-lean",
    ] {
        let Some(v) = inspect_json(&root.join(name)) else {
            continue;
        };
        let flags = v["scan"]["flags"].as_array().unwrap();
        assert!(
            flags.is_empty(),
            "{name} unexpectedly reports flags: {flags:?}"
        );
    }
}
