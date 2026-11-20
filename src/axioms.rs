//! M3 — re-compute the head theorems' axiom dependencies with Lean itself.
//!
//! The generated file calls `Lean.collectAxioms` (the same kernel-level API the
//! repositories' own `Audit.lean` / `AxiomCheck.lean` use) and prints one line per
//! head, plus the set of declarations that use `sorryAx` directly, obtained by
//! traversing `env.constants`. Nothing is decided by string search.
//!
//! See `docs/method.md` §3.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::claims::Claims;
use crate::inspect;
use crate::lean_src;
use crate::model::*;
use crate::run;

pub const STANDARD_AXIOMS: [&str; 3] = ["propext", "Classical.choice", "Quot.sound"];

const MARK_AXIOMS: &str = "LEANAUDIT_AXIOMS";
const MARK_MISSING: &str = "LEANAUDIT_MISSING";
const MARK_SORRY: &str = "LEANAUDIT_SORRY";

pub struct AxiomsOptions {
    /// Explicit heads. When `None`, they are taken from the repository's audit
    /// file, then from its published claims.
    pub heads: Option<Vec<String>>,
    /// `collect` (Lean `collectAxioms`, structured) or `print` (the
    /// `#print axioms` path used by `scripts/audit_axioms.py`).
    pub mode: AxiomsMode,
    pub lake: String,
    pub timeout: Duration,
    pub build: bool,
    pub build_timeout: Duration,
    pub keep_generated: Option<PathBuf>,
    pub check_sorry: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxiomsMode {
    /// Generate an `elab` command that calls `Lean.collectAxioms` and prints one
    /// structured line per head, plus the `sorryAx` traversal.
    Collect,
    /// Generate `#print axioms <name>` lines, exactly as the reference Python
    /// implementation does, and parse Lean's message output.
    ///
    /// This cannot report an unknown head as `missing`: `#print axioms` fails
    /// elaboration for a name that does not resolve, so the run is reported as
    /// `error` instead. It exists to cross-check against the reference
    /// implementation's code path.
    Print,
}

impl AxiomsMode {
    pub fn as_str(self) -> &'static str {
        match self {
            AxiomsMode::Collect => "collect",
            AxiomsMode::Print => "print",
        }
    }
}

impl Default for AxiomsOptions {
    fn default() -> Self {
        Self {
            heads: None,
            mode: AxiomsMode::Collect,
            lake: "lake".to_string(),
            timeout: Duration::from_secs(600),
            build: false,
            build_timeout: Duration::from_secs(3600),
            keep_generated: None,
            check_sorry: true,
        }
    }
}

pub fn run_axioms(
    root: &Path,
    report: &Report,
    claims: &Claims,
    opts: &AxiomsOptions,
) -> AxiomsResult {
    let allowed: Vec<String> = if report.audit_impl.allowed_axioms.is_empty() {
        STANDARD_AXIOMS.iter().map(|s| s.to_string()).collect()
    } else {
        report.audit_impl.allowed_axioms.clone()
    };

    let mut result = AxiomsResult {
        run: false,
        status: status::NOT_RUN.to_string(),
        mode: opts.mode.as_str().to_string(),
        lake: opts.lake.clone(),
        heads_source: "none".to_string(),
        imports: Vec::new(),
        allowed_axioms: allowed.clone(),
        returncode: None,
        timed_out: false,
        wall_ms: None,
        build: None,
        generated_file: None,
        heads: Vec::new(),
        missing: Vec::new(),
        unexpected_axioms: Vec::new(),
        direct_sorry: None,
        expected_direct_sorry: report.audit_impl.expected_direct_sorry.clone(),
        sorry_scope: Vec::new(),
        sorry_scope_source: "none".to_string(),
        claim_conflicts: claims
            .conflicts
            .iter()
            .map(|c| format!("{}: {} vs {}", c.name, c.first.1, c.second.1))
            .collect(),
        stderr_tail: None,
        note: None,
    };

    // 1. Which heads?
    let (heads, heads_source) = match resolve_heads(report, claims, opts) {
        Some(v) => v,
        None => {
            result.status = status::UNSUPPORTED.to_string();
            result.note = Some(
                "no head theorems: none given with --heads, none parsed from the audit file, \
                 and none published in receipts or AXIOM_AUDIT.json"
                    .to_string(),
            );
            return result;
        }
    };
    result.heads_source = heads_source;

    // 2. Which modules to import?
    let imports = resolve_imports(root, report);
    result.imports = imports.clone();

    // 3. Optional build.
    if opts.build {
        match run::run(&opts.lake, &["build".to_string()], root, opts.build_timeout) {
            Ok(out) => {
                let outcome = BuildOutcome {
                    ran: true,
                    returncode: out.returncode,
                    timed_out: out.timed_out,
                    wall_ms: out.wall_ms,
                };
                let failed = out.timed_out || out.returncode != Some(0);
                result.build = Some(outcome);
                if failed {
                    result.status = status::ERROR.to_string();
                    result.stderr_tail = Some(out.tail(2000));
                    result.note = Some("lake build did not succeed".to_string());
                    return result;
                }
            }
            Err(e) => {
                result.status = status::ERROR.to_string();
                result.note = Some(format!("could not run `{} build`: {e}", opts.lake));
                return result;
            }
        }
    }

    // 4. Generate the audit program.
    let (scope, scope_source) = sorry_scope(root, &report.audit_impl);
    result.sorry_scope = scope.clone();
    result.sorry_scope_source = scope_source;
    if opts.mode == AxiomsMode::Print {
        result.note = Some(
            "print mode: `#print axioms` output only; no env.constants sorry traversal".to_string(),
        );
    }
    let check_sorry = opts.check_sorry && opts.mode == AxiomsMode::Collect;
    let program = match opts.mode {
        AxiomsMode::Collect => generate(&imports, &heads, &scope, check_sorry),
        AxiomsMode::Print => generate_print(&imports, &heads),
    };

    let generated_path = match &opts.keep_generated {
        Some(p) => {
            if let Some(parent) = p.parent()
                && !parent.as_os_str().is_empty()
            {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::write(p, &program) {
                Ok(()) => p.clone(),
                Err(e) => {
                    result.status = status::ERROR.to_string();
                    result.note = Some(format!("could not write {}: {e}", p.display()));
                    return result;
                }
            }
        }
        None => {
            let mut p = std::env::temp_dir();
            p.push(format!("lean-audit-{}.lean", std::process::id()));
            match std::fs::write(&p, &program) {
                Ok(()) => p,
                Err(e) => {
                    result.status = status::ERROR.to_string();
                    result.note = Some(format!("could not write {}: {e}", p.display()));
                    return result;
                }
            }
        }
    };
    result.generated_file = Some(generated_path.to_string_lossy().to_string());

    // 5. Run `lake env lean <generated>`.
    let args = vec![
        "env".to_string(),
        "lean".to_string(),
        generated_path.to_string_lossy().to_string(),
    ];
    let out = match run::run(&opts.lake, &args, root, opts.timeout) {
        Ok(out) => out,
        Err(e) => {
            result.status = status::ERROR.to_string();
            result.note = Some(format!("could not run `{} env lean`: {e}", opts.lake));
            cleanup(&generated_path, opts);
            return result;
        }
    };
    result.run = true;
    result.returncode = out.returncode;
    result.timed_out = out.timed_out;
    result.wall_ms = Some(out.wall_ms);

    let combined = out.combined();
    let parsed = match opts.mode {
        AxiomsMode::Collect => parse_output(&combined),
        AxiomsMode::Print => parse_print_output(&combined, &heads),
    };

    result.missing = parsed.missing.clone();
    result.direct_sorry = parsed.sorry.clone();

    for name in &heads {
        let axioms = parsed.axioms.get(name).cloned();
        let claim = claims.get(name);
        let claimed = claim.map(|c| c.axioms.clone());
        let claimed_source = claim.map(|c| c.source.clone());

        let disclosed = axioms.as_ref().is_none_or(|axs| {
            axs.iter()
                .all(|a| allowed.contains(a) || claimed.as_ref().is_some_and(|c| c.contains(a)))
        });

        if let Some(axs) = &axioms {
            for a in axs {
                let is_standard = allowed.contains(a);
                let is_claimed = claimed.as_ref().is_some_and(|c| c.contains(a));
                if !is_standard && !is_claimed {
                    result.unexpected_axioms.push(UnexpectedAxiom {
                        head: name.clone(),
                        axiom: a.clone(),
                    });
                }
            }
        }

        let matches_claim = match (&axioms, &claimed) {
            (Some(a), Some(c)) => Some(a == c),
            _ => None,
        };

        result.heads.push(HeadResult {
            name: name.clone(),
            axioms,
            claimed,
            claimed_source,
            matches_claim,
            disclosed,
        });
    }

    // 6. Verdict.
    let unresolved = result.heads.iter().filter(|h| h.axioms.is_none()).count();
    let claim_mismatch = result.heads.iter().any(|h| h.matches_claim == Some(false));
    let sorry_mismatch = match (&result.direct_sorry, &result.expected_direct_sorry) {
        (Some(found), Some(expected)) => sorted(found) != sorted(expected),
        _ => false,
    };

    result.status = if out.timed_out || out.returncode != Some(0) {
        status::ERROR.to_string()
    } else if unresolved > 0
        || !result.unexpected_axioms.is_empty()
        || claim_mismatch
        || sorry_mismatch
    {
        status::MISMATCH.to_string()
    } else {
        status::OK.to_string()
    };

    if result.status == status::ERROR {
        result.stderr_tail = Some(out.tail(2000));
        if out.timed_out {
            result.note = Some(format!("timed out after {}s", opts.timeout.as_secs()));
        } else {
            result.note =
                Some("Lean exited non-zero; imports are probably not built yet".to_string());
        }
    }
    if out.output_abandoned {
        let extra = "output was abandoned: a process kept the pipe open after the run ended";
        result.note = Some(match result.note.take() {
            Some(existing) => format!("{existing}; {extra}"),
            None => extra.to_string(),
        });
    }

    cleanup(&generated_path, opts);
    result
}

fn cleanup(path: &Path, opts: &AxiomsOptions) {
    if opts.keep_generated.is_none() {
        let _ = std::fs::remove_file(path);
    }
}

/// Heads, in priority order: `--heads`, the audit file, published claims.
fn resolve_heads(
    report: &Report,
    claims: &Claims,
    opts: &AxiomsOptions,
) -> Option<(Vec<String>, String)> {
    if let Some(h) = &opts.heads
        && !h.is_empty()
    {
        return Some((h.clone(), "cli".to_string()));
    }
    if let Some(h) = &report.audit_impl.heads
        && !h.is_empty()
    {
        return Some((h.clone(), "audit_file".to_string()));
    }
    if claims.len() > 0 {
        return Some((
            claims.by_name.keys().cloned().collect(),
            "published_claims".to_string(),
        ));
    }
    None
}

/// Modules to import: whatever the repository's own audit file imports, plus its
/// lake libraries. `Lean` is always included.
fn resolve_imports(root: &Path, report: &Report) -> Vec<String> {
    let mut imports: Vec<String> = Vec::new();
    let mut push = |name: String| {
        if !imports.contains(&name) {
            imports.push(name);
        }
    };

    for file in &report.audit_impl.files {
        let path = Path::new(file);
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        if let Ok(stripped) = lean_src::strip_comments_and_strings(&text) {
            for imp in lean_src::extract_imports(&stripped) {
                push(imp);
            }
        }
    }

    // A repository without an audit file (e.g. coth-sums-barnes-lean) still has
    // lake libraries to import.
    if report.audit_impl.heads.is_none() {
        for lib in &report.lakefile.lean_libs {
            push(lib.clone());
        }
    }

    push("Lean".to_string());
    let _ = root;
    imports
}

/// The module roots used to scope the `sorry` traversal and the token gate.
///
/// Only a scope the repository itself declares narrows the check. Without one,
/// every local module is in scope: narrowing on our own judgement could hide a
/// real `sorry`, whereas reporting a deliberate `Challenge.lean` `sorry` as a
/// discrepancy is the lesser error when the repository has said nothing.
pub fn sorry_scope(root: &Path, audit: &AuditImpl) -> (Vec<String>, String) {
    let declared = declared_roots(audit);
    if !declared.is_empty() {
        return (declared, "audit_file".to_string());
    }
    let modules = inspect::local_module_names(root);
    (local_roots(&modules), "all_local_modules".to_string())
}

/// Top-level name components of the repository's own modules.
fn local_roots(modules: &[String]) -> Vec<String> {
    let mut roots: BTreeSet<String> = BTreeSet::new();
    for m in modules {
        if let Some(root) = m.split('.').next() {
            roots.insert(root.to_string());
        }
    }
    roots.into_iter().collect()
}

/// Roots the repository's own audit file scopes its `sorry` scan to, read from
/// comparisons like `m.getRoot == \`SierksmaLean` or `m == \`Solution`.
///
/// Using the repository's declared scope matters: `Challenge.lean` deliberately
/// contains `sorry`, and the repositories exclude it on purpose. Scanning
/// everything would report a discrepancy that is not one.
fn declared_roots(audit: &AuditImpl) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for file in &audit.files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        let Ok(stripped) = lean_src::strip_comments_and_strings(&text) else {
            continue;
        };
        for name in declared_roots_from_text(&stripped) {
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

fn declared_roots_from_text(stripped: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cursor = 0usize;
    while let Some(idx) = stripped[cursor..].find("==") {
        let abs = cursor + idx;
        let after = stripped[abs + 2..].trim_start();
        if let Some(rest) = after.strip_prefix('`') {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '\'' || *c == '.')
                .collect();
            let lookback = &stripped[abs.saturating_sub(40)..abs];
            let last = lookback.split_whitespace().last().unwrap_or("");
            if (last == "m" || last.ends_with("getRoot"))
                && !name.is_empty()
                && !out.contains(&name)
            {
                out.push(name);
            }
        }
        cursor = abs + 2;
    }
    out
}

fn generate(imports: &[String], heads: &[String], roots: &[String], check_sorry: bool) -> String {
    let mut s = String::new();
    s.push_str("import Lean\n");
    for imp in imports {
        if imp != "Lean" {
            s.push_str(&format!("import {imp}\n"));
        }
    }
    s.push_str("\nopen Lean Elab Command\n\n");
    s.push_str("elab \"#lean_audit\" : command => do\n");
    s.push_str("  let env ← getEnv\n");

    // Heads are emitted as raw `Name` literals (single backtick). A double
    // backtick quotation resolves the name in the environment and fails
    // elaboration outright for a name that does not exist, which would hide the
    // very case we want to report: a head that is not in the environment.
    s.push_str("  let heads : List Name := [");
    s.push_str(
        &heads
            .iter()
            .map(|h| format!("`{h}"))
            .collect::<Vec<_>>()
            .join(", "),
    );
    s.push_str("]\n");
    s.push_str("  for h in heads do\n");
    s.push_str("    if env.contains h then\n");
    s.push_str("      let axs := (← liftCoreM <| Lean.collectAxioms h).toList\n");
    s.push_str(&format!(
        "      logInfo m!\"{MARK_AXIOMS} {{h}} {{axs}}\"\n"
    ));
    s.push_str("    else\n");
    s.push_str(&format!("      logInfo m!\"{MARK_MISSING} {{h}}\"\n"));

    if check_sorry {
        s.push_str("  let localRoots : List Name := [");
        s.push_str(
            &roots
                .iter()
                .map(|r| format!("`{r}"))
                .collect::<Vec<_>>()
                .join(", "),
        );
        s.push_str("]\n");
        s.push_str("  let rootsArr := localRoots.toArray\n");
        s.push_str("  let mods := env.header.moduleNames\n");
        s.push_str("  let direct := env.constants.map₁.fold (init := (#[] : Array Name)) fun acc n ci =>\n");
        s.push_str("    match env.getModuleIdxFor? n with\n");
        s.push_str("    | some idx =>\n");
        s.push_str(
            "      if rootsArr.contains mods[idx.toNat]!.getRoot && ci.getUsedConstantsAsSet.contains ``sorryAx then\n",
        );
        s.push_str("        acc.push n\n");
        s.push_str("      else acc\n");
        s.push_str("    | none => acc\n");
        s.push_str(&format!("  logInfo m!\"{MARK_SORRY} {{direct.toList}}\"\n"));
    }

    s.push_str("\n#lean_audit\n");
    s
}

#[derive(Default)]
struct Parsed {
    axioms: std::collections::BTreeMap<String, Vec<String>>,
    missing: Vec<String>,
    sorry: Option<Vec<String>>,
}

/// The `#print axioms` path, matching `scripts/audit_axioms.py`: import the
/// modules, then one `#print axioms <name>` line per head.
fn generate_print(imports: &[String], heads: &[String]) -> String {
    let mut s = String::new();
    for imp in imports {
        s.push_str(&format!("import {imp}\n"));
    }
    s.push('\n');
    for h in heads {
        s.push_str(&format!("#print axioms {h}\n"));
    }
    s
}

/// Parse Lean's `#print axioms` messages.
///
/// `#print axioms X` fails elaboration when `X` does not resolve, so a head with
/// no output line is reported as missing by absence — the same rule the reference
/// implementation uses.
fn parse_print_output(text: &str, heads: &[String]) -> Parsed {
    let mut parsed = Parsed::default();
    for line in text.lines() {
        let line = line.trim();
        if let Some(idx) = line.find("' does not depend on any axioms") {
            if let Some(name) = trailing_quoted(&line[..idx]) {
                parsed.axioms.insert(name, Vec::new());
            }
        } else if let Some(idx) = line.find("' depends on axioms:")
            && let Some(name) = trailing_quoted(&line[..idx])
        {
            parsed.axioms.insert(name, bracket_list(&line[idx..]));
        }
    }
    for h in heads {
        if !parsed.axioms.contains_key(h) {
            parsed.missing.push(h.clone());
        }
    }
    parsed
}

/// The name before a closing quote, e.g. `'Ang.disproof` -> `Ang.disproof`.
/// A name may itself end in `'`, which is why the closing quote is located by
/// the text that follows it rather than by searching for the first quote.
fn trailing_quoted(s: &str) -> Option<String> {
    let name = s.trim_start().strip_prefix('\'')?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn parse_output(text: &str) -> Parsed {
    let mut parsed = Parsed::default();
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(MARK_AXIOMS) {
            let rest = rest.trim_start();
            if let Some((name, list)) = rest.split_once(' ') {
                parsed
                    .axioms
                    .insert(name.trim().to_string(), bracket_list(list));
            } else if !rest.is_empty() {
                parsed.axioms.insert(rest.trim().to_string(), Vec::new());
            }
        } else if let Some(rest) = line.strip_prefix(MARK_MISSING) {
            let name = rest.trim();
            if !name.is_empty() {
                parsed.missing.push(name.to_string());
            }
        } else if let Some(rest) = line.strip_prefix(MARK_SORRY) {
            parsed.sorry = Some(bracket_list(rest));
        }
    }
    parsed
}

/// Order-insensitive copy, for comparing recomputed and published lists.
fn sorted(items: &[String]) -> Vec<String> {
    let mut v = items.to_vec();
    v.sort();
    v
}

fn bracket_list(text: &str) -> Vec<String> {
    let Some(open) = text.find('[') else {
        return Vec::new();
    };
    let Some(close) = text[open..].find(']') else {
        return Vec::new();
    };
    text[open + 1..open + close]
        .split(',')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_a_collect_axioms_program() {
        let program = generate(
            &["Solution".to_string(), "Lean".to_string()],
            &["Ang.disproof".to_string(), "Ang.counterexample".to_string()],
            &["AngCounterexample".to_string(), "Solution".to_string()],
            true,
        );
        assert!(program.contains("import Lean\n"));
        assert!(program.contains("import Solution\n"));
        assert_eq!(
            program.matches("import Lean").count(),
            1,
            "Lean must not be imported twice"
        );
        assert!(program.contains("`Ang.disproof, `Ang.counterexample"));
        assert!(program.contains("Lean.collectAxioms h"));
        assert!(program.contains("`AngCounterexample, `Solution"));
        assert!(program.contains("getUsedConstantsAsSet.contains ``sorryAx"));
        assert!(program.contains("``sorryAx"));
        assert!(program.trim_end().ends_with("#lean_audit"));
    }

    #[test]
    fn generation_can_skip_the_sorry_traversal() {
        let program = generate(&["Lean".to_string()], &["A.b".to_string()], &[], false);
        assert!(!program.contains(MARK_SORRY));
        assert!(program.contains(MARK_AXIOMS));
    }

    #[test]
    fn parses_lean_audit_output() {
        let text = "\
LEANAUDIT_AXIOMS Ang.disproof [propext, Classical.choice, Quot.sound]
LEANAUDIT_AXIOMS Ang.counterexample []
LEANAUDIT_MISSING Ang.nonexistent
LEANAUDIT_SORRY []
info: some unrelated line
";
        let parsed = parse_output(text);
        assert_eq!(parsed.axioms["Ang.disproof"].len(), 3);
        assert!(parsed.axioms["Ang.counterexample"].is_empty());
        assert_eq!(parsed.missing, vec!["Ang.nonexistent".to_string()]);
        assert_eq!(parsed.sorry, Some(vec![]));
    }

    #[test]
    fn parses_sorry_users_and_native_axioms() {
        let text = "LEANAUDIT_SORRY [Foo.bar, Baz.qux]\nLEANAUDIT_AXIOMS T.x [propext, T.x._native.native_decide.ax_1_1]\n";
        let parsed = parse_output(text);
        assert_eq!(parsed.sorry, Some(vec!["Foo.bar".into(), "Baz.qux".into()]));
        assert_eq!(parsed.axioms["T.x"].len(), 2);
    }

    #[test]
    fn ignores_lines_from_other_tools() {
        let parsed =
            parse_output("AUDIT axioms X: [propext]\nsome LEANAUDIT_AXIOMS_NOT_A_MARK X\n");
        assert!(parsed.axioms.is_empty());
    }

    #[test]
    fn generates_a_print_axioms_program_like_the_reference() {
        let program = generate_print(
            &["Solution".to_string(), "Lean".to_string()],
            &["Ang.disproof".to_string(), "Ang.counterexample".to_string()],
        );
        assert_eq!(
            program,
            "import Solution\nimport Lean\n\n#print axioms Ang.disproof\n#print axioms Ang.counterexample\n"
        );
    }

    #[test]
    fn parses_print_axioms_output() {
        let text = "\
'Ang.disproof' depends on axioms: [propext, Classical.choice, Quot.sound]
'head_true' does not depend on any axioms
'foo'' depends on axioms: [propext]
";
        let heads = vec![
            "Ang.disproof".to_string(),
            "head_true".to_string(),
            "foo'".to_string(),
            "absent".to_string(),
        ];
        let parsed = parse_print_output(text, &heads);
        assert_eq!(parsed.axioms["Ang.disproof"].len(), 3);
        assert!(parsed.axioms["head_true"].is_empty());
        assert_eq!(parsed.axioms["foo'"], vec!["propext".to_string()]);
        assert_eq!(parsed.missing, vec!["absent".to_string()]);
        assert_eq!(parsed.sorry, None, "print mode has no sorry traversal");
    }

    #[test]
    fn print_mode_parser_ignores_unrelated_output() {
        let parsed = parse_print_output(
            "info: building\nsome other 'x' depends on axioms: [a]\n",
            &["x".to_string()],
        );
        // The line does not start with a quote, so it is not mistaken for output.
        assert!(parsed.axioms.is_empty());
        assert_eq!(parsed.missing, vec!["x".to_string()]);
    }

    #[test]
    fn trailing_quoted_handles_names_ending_in_a_quote() {
        assert_eq!(trailing_quoted("'Ang.x").as_deref(), Some("Ang.x"));
        assert_eq!(trailing_quoted("'foo'").as_deref(), Some("foo'"));
        assert_eq!(trailing_quoted("no quote"), None);
    }

    #[test]
    fn local_roots_are_the_first_name_components() {
        let roots = local_roots(&[
            "RubikSTM.A.B".to_string(),
            "RubikSTM.C".to_string(),
            "Main".to_string(),
            "AxiomCheck".to_string(),
        ]);
        assert_eq!(
            roots,
            vec![
                "AxiomCheck".to_string(),
                "Main".to_string(),
                "RubikSTM".to_string()
            ]
        );
    }

    #[test]
    fn declared_roots_reads_getroot_and_m_comparisons() {
        let src = "if (m.getRoot == `AngCounterexample || m == `Solution) && ci.getUsedConstantsAsSet.contains ``sorryAx then\n  acc.push n\nelse acc\n";
        assert_eq!(
            declared_roots_from_text(src),
            vec!["AngCounterexample".to_string(), "Solution".to_string()]
        );
    }

    #[test]
    fn declared_roots_ignores_unrelated_comparisons() {
        // feigenbaum-scalar-duality-lean compares axiom lists, not roots.
        let src = "unless found.toArray.qsort Name.lt == expected.toArray.qsort Name.lt do\n  throwError m!\"x\"\n";
        assert!(declared_roots_from_text(src).is_empty());
    }

    #[test]
    fn bracket_list_handles_empty_and_spaces() {
        assert_eq!(bracket_list(" []"), Vec::<String>::new());
        assert_eq!(
            bracket_list(" [a, b]"),
            vec!["a".to_string(), "b".to_string()]
        );
    }
}
