//! Tests for the CI gate script (`scripts/ci.sh`) — the logic the composite
//! action runs. Nothing here needs Lean, so the gate's behaviour is covered on
//! any machine.

use std::path::{Path, PathBuf};
use std::process::Command;

fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/ci.sh")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

struct Gate {
    code: i32,
    stderr: String,
    outputs: String,
    summary: String,
    report_dir: PathBuf,
    /// Kept alive so the reports survive until the assertions have run.
    _dir: tempfile::TempDir,
}

fn run_gate(target: &str, checks: &str, fail_on_mismatch: Option<bool>) -> Gate {
    let dir = tempfile::tempdir().expect("tempdir");
    let out_file = dir.path().join("gh_output");
    let summary_file = dir.path().join("gh_summary");
    let report_dir = dir.path().join("reports");
    std::fs::write(&out_file, "").unwrap();
    std::fs::write(&summary_file, "").unwrap();

    let mut cmd = Command::new("bash");
    cmd.arg(script())
        .env("LEAN_AUDIT_BIN", env!("CARGO_BIN_EXE_lean-audit"))
        .env("LEAN_AUDIT_TARGET", fixture(target))
        .env("LEAN_AUDIT_CHECKS", checks)
        .env("LEAN_AUDIT_REPORT_DIR", &report_dir)
        .env("GITHUB_OUTPUT", &out_file)
        .env("GITHUB_STEP_SUMMARY", &summary_file);
    if let Some(flag) = fail_on_mismatch {
        cmd.env(
            "LEAN_AUDIT_FAIL_ON_MISMATCH",
            if flag { "true" } else { "false" },
        );
    }
    let result = cmd.output().expect("run scripts/ci.sh");

    Gate {
        code: result.status.code().unwrap_or(-1),
        stderr: String::from_utf8_lossy(&result.stderr).to_string(),
        outputs: std::fs::read_to_string(&out_file).unwrap_or_default(),
        summary: std::fs::read_to_string(&summary_file).unwrap_or_default(),
        report_dir,
        _dir: dir,
    }
}

#[test]
fn passes_when_the_only_tokens_are_out_of_scope() {
    // Mirrors the real repositories: a deliberate `Challenge.lean` sorry, with the
    // audit file scoping the gate to the library.
    let gate = run_gate("out-of-scope-sorry", "inspect,scan", None);
    assert_eq!(gate.code, 0, "stderr: {}", gate.stderr);
    assert!(
        gate.outputs.contains("status=ok"),
        "outputs: {}",
        gate.outputs
    );
    assert!(gate.summary.contains("## lean-audit"));
    assert!(gate.summary.contains("| `scan` | 0 | ok |"));
}

#[test]
fn fails_on_an_in_scope_sorry() {
    let gate = run_gate("in-scope-sorry", "scan", None);
    assert_eq!(gate.code, 2, "stderr: {}", gate.stderr);
    assert!(gate.outputs.contains("status=mismatch"));
    assert!(gate.summary.contains("forbidden tokens in scope"));
}

#[test]
fn fail_on_mismatch_false_reports_mismatch_but_exits_zero() {
    let gate = run_gate("in-scope-sorry", "scan", Some(false));
    assert_eq!(gate.code, 0, "stderr: {}", gate.stderr);
    assert!(
        gate.outputs.contains("status=mismatch"),
        "the status must still say mismatch: {}",
        gate.outputs
    );
}

#[test]
fn reports_are_written_for_every_check() {
    let gate = run_gate("out-of-scope-sorry", "inspect,manifest,scan", None);
    assert_eq!(gate.code, 0, "stderr: {}", gate.stderr);
    for name in ["inspect", "manifest", "scan"] {
        let path = gate.report_dir.join(format!("{name}.md"));
        assert!(path.is_file(), "{} was not written", path.display());
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(
            body.contains("does not establish mathematical correctness"),
            "{name}.md is missing the disclaimer"
        );
    }
    assert!(gate.outputs.contains("report-dir="));
}

#[test]
fn an_unknown_check_is_rejected() {
    let gate = run_gate("out-of-scope-sorry", "inspect,nonsense", None);
    assert_eq!(gate.code, 4);
    assert!(gate.stderr.contains("unknown check"));
}

#[test]
fn a_missing_binary_is_an_error_not_a_pass() {
    let dir = tempfile::tempdir().unwrap();
    let out_file = dir.path().join("gh_output");
    let result = Command::new("bash")
        .arg(script())
        .env("LEAN_AUDIT_BIN", dir.path().join("does-not-exist"))
        .env("LEAN_AUDIT_TARGET", fixture("out-of-scope-sorry"))
        .env("LEAN_AUDIT_CHECKS", "scan")
        .env("LEAN_AUDIT_REPORT_DIR", dir.path().join("reports"))
        .env("GITHUB_OUTPUT", &out_file)
        .output()
        .expect("run scripts/ci.sh");
    assert_eq!(result.status.code(), Some(4));
}

#[test]
fn a_check_that_cannot_run_fails_even_when_mismatch_does_not() {
    // `axioms` pointed at a lake that does not exist: the check cannot run, which
    // is not a pass, so the job must fail regardless of fail-on-mismatch.
    let dir = tempfile::tempdir().unwrap();
    let out_file = dir.path().join("gh_output");
    let report_dir = dir.path().join("reports");
    std::fs::write(&out_file, "").unwrap();

    let result = Command::new("bash")
        .arg(script())
        .env("LEAN_AUDIT_BIN", env!("CARGO_BIN_EXE_lean-audit"))
        .env("LEAN_AUDIT_TARGET", fixture("out-of-scope-sorry"))
        .env("LEAN_AUDIT_CHECKS", "axioms")
        .env("LEAN_AUDIT_AXIOMS_ARGS", "--lake /nonexistent-lake-xyz")
        .env("LEAN_AUDIT_FAIL_ON_MISMATCH", "false")
        .env("LEAN_AUDIT_REPORT_DIR", &report_dir)
        .env("GITHUB_OUTPUT", &out_file)
        .output()
        .expect("run scripts/ci.sh");

    assert_eq!(
        result.status.code(),
        Some(4),
        "a check that cannot run must fail the job"
    );
    let outputs = std::fs::read_to_string(&out_file).unwrap();
    assert!(outputs.contains("status=error"), "outputs: {outputs}");
}
