//! M6 `build` — run `lake build`, collect per-module times, and compare the
//! repository's `receipts/**/build_summary.txt` numbers against a fresh run.
//!
//! This is the expensive milestone: it needs the modules to actually build. The
//! tool stays read-only with respect to sources — `lake build` writes only build
//! artifacts under `.lake`, and `--clean` only removes those.
//!
//! Honesty rule that shapes the design: a **module count is only comparable
//! after a clean build**. On an incremental build lake reports only what it
//! rebuilt, so the count is not the repository's number. When `--clean` is not
//! given, the count comparison is reported as `not_run`, never as a match.

use std::path::Path;
use std::time::Duration;

use crate::model::{
    BuildClaim, BuildLog, BuildReport, DISCLAIMER, ModuleTime, Report, SCHEMA, status,
};
use crate::run::{self, RunOutput};

#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub lake: String,
    pub clean: bool,
    pub timeout: Duration,
    /// Keep only this many slowest modules in the report.
    pub top: usize,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            lake: "lake".to_string(),
            clean: false,
            timeout: Duration::from_secs(3600),
            top: 15,
        }
    }
}

/// Run the build and assemble the report.
pub fn run_build(root: &Path, report: &Report, opts: &BuildOptions) -> BuildReport {
    let mut log = BuildLog {
        run: false,
        clean: false,
        lake: opts.lake.clone(),
        returncode: None,
        timed_out: false,
        wall_ms: 0,
        modules_built: 0,
        errors: 0,
        sorry_warnings: 0,
        slowest: Vec::new(),
        total_module_seconds: 0.0,
        note: None,
        stderr_tail: None,
    };

    if opts.clean {
        match run::run(
            &opts.lake,
            &["clean".to_string()],
            root,
            Duration::from_secs(300),
        ) {
            Ok(out) if out.returncode == Some(0) => log.clean = true,
            Ok(out) => {
                log.note = Some(format!(
                    "`{} clean` exited with {:?}",
                    opts.lake, out.returncode
                ));
            }
            Err(e) => {
                log.note = Some(format!("could not run `{} clean`: {e}", opts.lake));
                return finish(log, report, opts);
            }
        }
    }

    let out = match run::run(&opts.lake, &["build".to_string()], root, opts.timeout) {
        Ok(out) => out,
        Err(e) => {
            log.note = Some(format!("could not run `{} build`: {e}", opts.lake));
            return finish(log, report, opts);
        }
    };

    log.run = true;
    log.returncode = out.returncode;
    log.timed_out = out.timed_out;
    log.wall_ms = out.wall_ms;

    let parsed = parse_build_log(&out.stdout);
    log.modules_built = parsed.modules.len();
    log.errors = parsed.errors;
    log.sorry_warnings = parsed.sorry_warnings;
    log.total_module_seconds = parsed.total_seconds;
    let mut modules = parsed.modules;
    modules.sort_by(|a, b| {
        b.seconds
            .unwrap_or(0.0)
            .total_cmp(&a.seconds.unwrap_or(0.0))
    });
    modules.truncate(opts.top);
    log.slowest = modules;

    if out.returncode != Some(0) || out.timed_out {
        log.stderr_tail = Some(tail_of(&out, 2000));
    }

    finish(log, report, opts)
}

fn finish(mut log: BuildLog, report: &Report, _opts: &BuildOptions) -> BuildReport {
    let claims = compare_claims(&log, report);
    let status = if !log.run || log.timed_out || log.returncode != Some(0) {
        status::ERROR.to_string()
    } else if claims.iter().any(|c| c.matches == Some(false)) {
        status::MISMATCH.to_string()
    } else {
        status::OK.to_string()
    };
    if !log.clean && log.run && status == status::OK {
        log.note = Some(
            "incremental build: module counts reflect only what lake rebuilt, so they are not \
             compared against the receipt; pass --clean for a comparable run"
                .to_string(),
        );
    }
    BuildReport {
        schema: format!("{SCHEMA}/build"),
        generated_at: crate::model::now_iso8601(),
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        project: report.project.clone(),
        result: log,
        claims,
        status,
        disclaimer: DISCLAIMER.to_string(),
    }
}

/// Compare the receipt's numbers against the fresh run. A metric with no
/// published claim is skipped.
fn compare_claims(log: &BuildLog, report: &Report) -> Vec<BuildClaim> {
    let mut claims = Vec::new();
    for summary in &report.receipts.summaries {
        if let Some(claimed) = summary.claimed_modules_built {
            let measured = log.modules_built as i64;
            // Only a clean build is comparable.
            let matches = if log.clean && log.run {
                Some(claimed as i64 == measured)
            } else {
                None
            };
            claims.push(BuildClaim {
                metric: "modules_built".to_string(),
                source: summary.file.clone(),
                claimed: Some(claimed as i64),
                measured,
                matches,
                note: (!log.clean).then(|| "needs --clean".to_string()),
            });
        }
        if let Some(claimed) = summary.claimed_errors {
            claims.push(BuildClaim {
                metric: "errors".to_string(),
                source: summary.file.clone(),
                claimed: Some(claimed as i64),
                measured: log.errors as i64,
                matches: log.run.then_some(claimed as i64 == log.errors as i64),
                note: None,
            });
        }
        if let Some(claimed) = summary.claimed_sorry_warnings {
            claims.push(BuildClaim {
                metric: "sorry_warnings".to_string(),
                source: summary.file.clone(),
                claimed: Some(claimed as i64),
                measured: log.sorry_warnings as i64,
                matches: log
                    .run
                    .then_some(claimed as i64 == log.sorry_warnings as i64),
                note: None,
            });
        }
    }
    claims
}

pub struct ParsedLog {
    pub modules: Vec<ModuleTime>,
    pub errors: usize,
    pub sorry_warnings: usize,
    pub total_seconds: f64,
}

/// Parse `lake build` stdout.
///
/// Progress lines look like `✔ [12/34] Built Foo.Bar (1.2s)` (the leading symbol
/// and the trailing time are both optional). Error and `sorry` warnings are
/// counted from the diagnostics, not from the progress lines.
pub fn parse_build_log(text: &str) -> ParsedLog {
    let mut modules = Vec::new();
    let mut errors = 0;
    let mut sorry_warnings = 0;

    for line in text.lines() {
        if let Some(after) = line.split("] Built ").nth(1) {
            let after = after.trim_end();
            let (module, seconds) = match after.rfind('(') {
                Some(i) if after.ends_with(')') => {
                    let secs = parse_duration(&after[i + 1..after.len() - 1]);
                    (after[..i].trim(), secs)
                }
                _ => (after, None),
            };
            modules.push(ModuleTime {
                module: module.to_string(),
                seconds,
            });
        }
        if line.contains("error:") {
            errors += 1;
        }
        if line.contains("declaration uses 'sorry'") {
            sorry_warnings += 1;
        }
    }

    let total_seconds = modules.iter().filter_map(|m| m.seconds).sum::<f64>();
    ParsedLog {
        modules,
        errors,
        sorry_warnings,
        total_seconds: if total_seconds == 0.0 {
            0.0
        } else {
            total_seconds
        },
    }
}

/// lake prints elapsed time as `(1.8s)`, `(12s)` or `(171ms)`. `ms` must be
/// checked before the bare `s` suffix, because `ms` ends with `s`.
fn parse_duration(raw: &str) -> Option<f64> {
    let raw = raw.trim();
    if let Some(ms) = raw.strip_suffix("ms") {
        return ms.trim().parse::<f64>().ok().map(|v| v / 1000.0);
    }
    if let Some(s) = raw.strip_suffix('s') {
        return s.trim().parse::<f64>().ok();
    }
    raw.parse::<f64>().ok()
}

fn tail_of(out: &RunOutput, n: usize) -> String {
    let combined = out.combined();
    let t = out.tail(n);
    if combined.chars().count() > n {
        format!("…{t}")
    } else {
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_progress_lines_with_and_without_time() {
        let log = parse_build_log(
            "✔ [1/4] Built Foo (1.5s)\n\
             ✔ [2/4] Built Bar.Baz (12s)\n\
             ⚠ [3/4] Built Qux (171ms)\n\
             ✔ [4/4] Built Zap\n",
        );
        assert_eq!(log.modules.len(), 4);
        assert_eq!(log.modules[0].module, "Foo");
        assert_eq!(log.modules[0].seconds, Some(1.5));
        assert_eq!(log.modules[1].module, "Bar.Baz");
        assert_eq!(log.modules[1].seconds, Some(12.0));
        assert_eq!(log.modules[2].seconds, Some(0.171));
        assert_eq!(log.modules[3].seconds, None);
        assert!((log.total_seconds - 13.671).abs() < 1e-9);
    }

    #[test]
    fn parses_millisecond_durations() {
        assert_eq!(parse_duration("171ms"), Some(0.171));
        assert_eq!(parse_duration("1.5s"), Some(1.5));
        assert_eq!(parse_duration("12s"), Some(12.0));
        assert_eq!(parse_duration("nonsense"), None);
    }

    #[test]
    fn counts_errors_and_sorry_warnings() {
        let log = parse_build_log(
            "error: something failed\n\
             warning: declaration uses 'sorry'\n\
             warning: declaration uses 'sorry'\n\
             info: fine\n",
        );
        assert_eq!(log.errors, 1);
        assert_eq!(log.sorry_warnings, 2);
        assert!(log.modules.is_empty());
    }

    #[test]
    fn ignores_unrelated_lines() {
        let log = parse_build_log("info: building\nsome other 'x' depends on axioms: [a]\n");
        assert!(log.modules.is_empty());
        assert_eq!(log.errors, 0);
    }
}
