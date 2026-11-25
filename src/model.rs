//! Report data model (JSON schema `lean-audit/v1`).
//!
//! Honesty rules are encoded in the `status` string fields. See `AGENTS.md`:
//! `not_run` must never be reported as `ok`.

use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "lean-audit/v1";

pub const DISCLAIMER: &str = "This report verifies reproducibility of published claims. It does not establish mathematical correctness.";

/// Status values used across the report. Kept as string constants so the JSON
/// stays readable and forward-compatible.
///
/// `MISMATCH` and `NOT_RUN` are part of the documented schema and are consumed by
/// the later milestones (manifest verification, axiom re-computation).
#[allow(dead_code)]
pub mod status {
    /// Recomputed value matches the published claim.
    pub const OK: &str = "ok";
    /// Recomputed value differs from the published claim.
    pub const MISMATCH: &str = "mismatch";
    /// The check was not executed. Never report this as `ok`.
    pub const NOT_RUN: &str = "not_run";
    /// The file was read but nothing was recomputed.
    pub const PARSED_ONLY: &str = "parsed_only";
    /// The repository structure is not what this tool expects. Do not guess.
    pub const UNSUPPORTED: &str = "unsupported";
    /// The tool itself failed.
    pub const ERROR: &str = "error";
    /// Nothing to check (e.g. no audit file present).
    pub const ABSENT: &str = "absent";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub schema: String,
    pub generated_at: String,
    pub tool_version: String,
    pub project: String,
    pub source: Source,
    pub toolchain: Toolchain,
    pub mathlib: MathlibPin,
    pub lakefile: Lakefile,
    pub manifest: ManifestPins,
    pub modules: Modules,
    pub audit_impl: AuditImpl,
    pub manifests: Vec<ManifestInfo>,
    pub receipts: Receipts,
    pub scan: ScanSummary,
    pub warnings: Vec<String>,
    pub disclaimer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub path: String,
    pub git_commit: Option<String>,
    pub git_dirty: Option<bool>,
    /// Why git information is missing, when it is.
    pub git_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Toolchain {
    /// Raw contents of `lean-toolchain`, trimmed.
    pub lean_toolchain: Option<String>,
    /// Parsed release version, e.g. `4.33.1` for `leanprover/lean4:v4.33.1`.
    pub lean_version: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MathlibPin {
    /// Commit pin from `lake-manifest.json`.
    pub rev: Option<String>,
    /// Tag/rev requested in the lakefile `require`, e.g. `v4.33.1`.
    pub tag: Option<String>,
    pub source: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lakefile {
    pub path: Option<String>,
    /// `toml`, `lean`, or `none`.
    pub kind: String,
    pub name: Option<String>,
    pub version: Option<String>,
    pub default_targets: Vec<String>,
    pub requires: Vec<Require>,
    pub lean_libs: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Require {
    pub name: String,
    pub git: Option<String>,
    pub rev: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestPins {
    pub path: Option<String>,
    pub packages: Vec<PackagePin>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackagePin {
    pub name: String,
    pub rev: Option<String>,
    pub url: Option<String>,
    /// `git`, `path`, or `unknown`.
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Modules {
    pub count: usize,
    pub roots: Vec<String>,
    pub import_edges_internal: usize,
    pub external_imports: Vec<String>,
    pub max_import_depth: Option<usize>,
    pub cycles: Vec<Vec<String>>,
    /// Count of `.lean` files per top-level directory, descending.
    pub by_directory: Vec<DirCount>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirCount {
    pub dir: String,
    pub files: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditImpl {
    /// `lean_audit`, `lean_axiomcheck`, `python_script`, `none`, or `multiple`.
    pub kind: String,
    pub files: Vec<String>,
    /// Elaborator command names found in the audit file, e.g. `#ang_audit`.
    pub command_names: Vec<String>,
    /// Head theorem names, when they could be extracted. Never guessed.
    pub heads: Option<Vec<String>>,
    /// `parsed`, `cli`, `unparsed`, or `n/a`.
    pub heads_source: String,
    /// Direct `sorry` users the audit expects. `None` when the file does not
    /// state an explicit list.
    pub expected_direct_sorry: Option<Vec<String>>,
    pub allowed_axioms: Vec<String>,
    pub in_default_targets: Option<bool>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestInfo {
    pub file: String,
    /// `source_manifest`, `sha256sum`, `axiom_audit`, `manuscript_binding`, `unknown`.
    pub kind: String,
    /// Number of hash entries the manifest declares.
    pub entries: Option<usize>,
    /// Number of entries actually hashed. `None` when verification did not run.
    pub checked: Option<usize>,
    pub matched: Option<usize>,
    pub mismatched: Vec<HashMismatch>,
    /// Entries declared by the manifest that are not present in the checkout.
    pub missing: Vec<String>,
    /// `AUDIT axioms` claims bound by an axiom-audit file. Re-computation is M3.
    pub axiom_claims: Option<usize>,
    pub status: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashMismatch {
    pub path: String,
    pub expected: String,
    pub actual: String,
    pub bytes_expected: Option<u64>,
    pub bytes_actual: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipts {
    pub dir: Option<String>,
    /// All files under `receipts/`, recursively, relative to the repository root.
    pub files: Vec<String>,
    /// Every `build_summary.txt` found, with the numbers it claims.
    pub summaries: Vec<ReceiptSummary>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptSummary {
    pub file: String,
    pub claimed_modules_built: Option<usize>,
    pub claimed_errors: Option<usize>,
    pub claimed_sorry_warnings: Option<usize>,
    /// `AUDIT axioms <name>: [...]` lines: the axiom list the run published for
    /// each head theorem. This is the claim M3 re-computes.
    pub claimed_axioms: Vec<ClaimedAxioms>,
    pub claimed_placeholders: Option<usize>,
    /// `AUDIT declarations using sorry directly: [...]`.
    pub claimed_direct_sorry: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimedAxioms {
    pub name: String,
    pub axioms: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSummary {
    pub files_scanned: usize,
    /// Module roots the scan was scoped to.
    pub scope: Vec<String>,
    /// `audit_file` or `all_local_modules`.
    pub scope_source: String,
    /// Every forbidden token found, in or out of scope.
    pub flags: Vec<Flag>,
    /// How many of `flags` fall inside the scope.
    pub flags_in_scope: usize,
    pub status: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flag {
    pub path: String,
    pub line: usize,
    pub token: String,
    /// Whether the module is inside the repository's declared library scope.
    pub in_scope: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchReport {
    pub schema: String,
    pub generated_at: String,
    pub tool_version: String,
    pub root: String,
    pub repos: Vec<Report>,
    pub disclaimer: String,
}

// ── M3: axiom re-computation ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxiomsResult {
    /// Whether Lean was actually executed.
    pub run: bool,
    pub status: String,
    /// `collect` (Lean `collectAxioms`) — see `docs/method.md` §3.
    pub mode: String,
    pub lake: String,
    /// `cli`, `audit_file`, `published_claims`, or `none`.
    pub heads_source: String,
    pub imports: Vec<String>,
    pub allowed_axioms: Vec<String>,
    pub returncode: Option<i32>,
    pub timed_out: bool,
    pub wall_ms: Option<u64>,
    pub build: Option<BuildOutcome>,
    pub generated_file: Option<String>,
    pub heads: Vec<HeadResult>,
    /// Heads that the Lean environment does not contain.
    pub missing: Vec<String>,
    /// Axioms that are neither standard nor disclosed by a published claim.
    pub unexpected_axioms: Vec<UnexpectedAxiom>,
    /// Declarations of this repository using `sorryAx`, from `env.constants`.
    pub direct_sorry: Option<Vec<String>>,
    pub expected_direct_sorry: Option<Vec<String>>,
    /// Module roots the `sorry` traversal was scoped to.
    pub sorry_scope: Vec<String>,
    /// `audit_file`, `library_directories`, or `all_local_modules`.
    pub sorry_scope_source: String,
    pub claim_conflicts: Vec<String>,
    pub stderr_tail: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildOutcome {
    pub ran: bool,
    pub returncode: Option<i32>,
    pub timed_out: bool,
    pub wall_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadResult {
    pub name: String,
    /// Recomputed axiom list. `None` when the head was not resolved.
    pub axioms: Option<Vec<String>>,
    pub claimed: Option<Vec<String>>,
    pub claimed_source: Option<String>,
    /// `None` when there is nothing to compare against.
    pub matches_claim: Option<bool>,
    /// Every axiom is either standard or disclosed in the published claim.
    pub disclosed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnexpectedAxiom {
    pub head: String,
    pub axiom: String,
}

// ── M6: build + receipts re-computation ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleTime {
    pub module: String,
    /// Elaboration seconds, when lake printed them.
    pub seconds: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildLog {
    /// Whether `lake build` was actually executed.
    pub run: bool,
    /// Whether `lake clean` ran first (needed for a comparable module count).
    pub clean: bool,
    pub lake: String,
    pub returncode: Option<i32>,
    pub timed_out: bool,
    pub wall_ms: u64,
    pub modules_built: usize,
    pub errors: usize,
    pub sorry_warnings: usize,
    /// Slowest modules, capped by `--top`.
    pub slowest: Vec<ModuleTime>,
    /// Sum of the per-module times lake printed.
    pub total_module_seconds: f64,
    pub note: Option<String>,
    pub stderr_tail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildClaim {
    /// `modules_built`, `errors` or `sorry_warnings`.
    pub metric: String,
    /// The receipt that published the number.
    pub source: String,
    pub claimed: Option<i64>,
    pub measured: i64,
    /// `None` when the comparison could not be made (e.g. incremental build).
    pub matches: Option<bool>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildReport {
    pub schema: String,
    pub generated_at: String,
    pub tool_version: String,
    pub project: String,
    pub result: BuildLog,
    pub claims: Vec<BuildClaim>,
    pub status: String,
    pub disclaimer: String,
}

/// A minimal UTC ISO-8601 timestamp, avoiding a `chrono` dependency.
pub fn now_iso8601() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    iso8601_utc(secs)
}

pub fn iso8601_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// Howard Hinnant's `civil_from_days`.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso8601_known_values() {
        assert_eq!(iso8601_utc(0), "1970-01-01T00:00:00Z");
        // 2026-11-17T12:34:56Z
        assert_eq!(iso8601_utc(1_763_382_896), "2025-11-17T12:34:56Z");
        assert_eq!(
            iso8601_utc(1_763_382_896 + 365 * 86_400),
            "2026-11-17T12:34:56Z"
        );
    }

    #[test]
    fn civil_handles_epoch_and_leap() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
    }
}
