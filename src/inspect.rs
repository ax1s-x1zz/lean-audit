//! Repository inspection: everything decidable from files alone, with no Lean
//! execution. See `docs/requirements.md` §7.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use walkdir::WalkDir;

use crate::graph;
use crate::lean_src::{self, FORBIDDEN_TOKENS};
use crate::model::*;
use crate::util;

const STANDARD_AXIOMS: [&str; 3] = ["propext", "Classical.choice", "Quot.sound"];

pub struct InspectOptions {
    pub emit_graph: Option<PathBuf>,
    /// Recompute manifest SHA-256 hashes. Cheap enough to leave on by default.
    pub verify_hashes: bool,
}

/// Inspect every immediate subdirectory of `root` that looks like a Lean project.
pub fn batch(root: &Path, verify_hashes: bool) -> Result<Vec<Report>> {
    if !root.is_dir() {
        anyhow::bail!("not a directory: {}", root.display());
    }

    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .with_context(|| format!("reading {}", root.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir() && is_lean_project(p))
        .collect();
    dirs.sort();

    let mut reports = Vec::new();
    for dir in dirs {
        reports.push(inspect(
            &dir,
            &InspectOptions {
                emit_graph: None,
                verify_hashes,
            },
        )?);
    }
    Ok(reports)
}

fn is_lean_project(dir: &Path) -> bool {
    ["lean-toolchain", "lakefile.toml", "lakefile.lean"]
        .iter()
        .any(|f| dir.join(f).is_file())
}

pub fn inspect(root: &Path, opts: &InspectOptions) -> Result<Report> {
    if !root.is_dir() {
        anyhow::bail!("not a directory: {}", root.display());
    }

    let mut warnings: Vec<String> = Vec::new();

    let (modules, module_warnings) = discover_modules(root);
    warnings.extend(module_warnings);

    let lakefile = parse_lakefile(root);
    let manifest = parse_lake_manifest(root);
    let mathlib = mathlib_pin(&lakefile, &manifest);
    let toolchain = parse_toolchain(root);

    let module_pairs: Vec<(String, Vec<String>)> = modules
        .iter()
        .map(|m| (m.name.clone(), m.imports.clone()))
        .collect();
    let metrics = graph::analyze(&module_pairs);

    if let Some(path) = &opts.emit_graph {
        let json = serde_json::json!({
            "schema": "lean-audit/graph/v1",
            "modules": metrics.adjacency,
        });
        std::fs::write(path, serde_json::to_string_pretty(&json)?)
            .with_context(|| format!("writing graph to {}", path.display()))?;
    }

    if !metrics.cycles.is_empty() {
        warnings.push(format!("import cycles detected: {}", metrics.cycles.len()));
    }

    let mut by_dir: BTreeMap<String, usize> = BTreeMap::new();
    for m in &modules {
        *by_dir.entry(m.dir.clone()).or_insert(0) += 1;
    }
    let mut by_directory: Vec<DirCount> = by_dir
        .into_iter()
        .map(|(dir, files)| DirCount { dir, files })
        .collect();
    by_directory.sort_by(|a, b| b.files.cmp(&a.files).then(a.dir.cmp(&b.dir)));

    let scan = scan_sources(&modules);

    let report = Report {
        schema: SCHEMA.to_string(),
        generated_at: now_iso8601(),
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        project: project_name(root, &lakefile),
        source: git_info(root),
        toolchain,
        mathlib,
        lakefile,
        manifest,
        modules: Modules {
            count: modules.len(),
            roots: metrics.roots.clone(),
            import_edges_internal: metrics.internal_edges,
            external_imports: metrics.external_imports.clone(),
            max_import_depth: metrics.max_depth,
            cycles: metrics.cycles.clone(),
            by_directory,
            status: if modules.is_empty() {
                status::UNSUPPORTED.to_string()
            } else {
                status::OK.to_string()
            },
        },
        audit_impl: detect_audit_impl(root),
        manifests: crate::manifest::discover_and_verify(root, opts.verify_hashes),
        receipts: parse_receipts(root),
        scan,
        warnings,
        disclaimer: DISCLAIMER.to_string(),
    };

    Ok(report)
}

// ── modules ────────────────────────────────────────────────────────────────

pub struct Module {
    pub name: String,
    pub dir: String,
    pub path: PathBuf,
    pub imports: Vec<String>,
    /// Stripped source, kept for the forbidden-token scan.
    pub stripped: String,
}

fn discover_modules(root: &Path) -> (Vec<Module>, Vec<String>) {
    let mut modules = Vec::new();
    let mut warnings = Vec::new();

    let walker = WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(util::keep_entry);

    for entry in walker.filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("lean") {
            continue;
        }
        let rel = match path.strip_prefix(root) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let name = module_name(rel);
        if name.is_empty() {
            continue;
        }
        let dir = rel
            .parent()
            .and_then(|p| p.components().next())
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .unwrap_or_else(|| "(root)".to_string());

        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                warnings.push(format!("could not read {}: {e}", rel.display()));
                continue;
            }
        };
        let stripped = match lean_src::strip_comments_and_strings(&text) {
            Ok(s) => s,
            Err(e) => {
                warnings.push(format!("{}: {e}", rel.display()));
                continue;
            }
        };
        let imports = lean_src::extract_imports(&stripped);
        modules.push(Module {
            name,
            dir,
            path: path.to_path_buf(),
            imports,
            stripped,
        });
    }

    (modules, warnings)
}

fn module_name(rel: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for c in rel.components() {
        parts.push(c.as_os_str().to_string_lossy().to_string());
    }
    if let Some(last) = parts.last_mut()
        && let Some(stem) = last.strip_suffix(".lean")
    {
        *last = stem.to_string();
    }
    parts.join(".")
}

fn scan_sources(modules: &[Module]) -> ScanSummary {
    let mut flags = Vec::new();
    for m in modules {
        for (line, token) in lean_src::scan_forbidden(&m.stripped) {
            flags.push(Flag {
                path: m.path.to_string_lossy().to_string(),
                line,
                token,
            });
        }
    }
    ScanSummary {
        files_scanned: modules.len(),
        flags,
        status: status::OK.to_string(),
        note: Some(format!(
            "tokens checked: {}; comments and string literals are stripped first",
            FORBIDDEN_TOKENS.join(", ")
        )),
    }
}

// ── toolchain ──────────────────────────────────────────────────────────────

fn parse_toolchain(root: &Path) -> Toolchain {
    let path = root.join("lean-toolchain");
    let raw = std::fs::read_to_string(&path).ok();
    let lean_toolchain = raw.as_ref().map(|s| s.trim().to_string());
    let lean_version = lean_toolchain.as_deref().and_then(parse_lean_version);
    let status = match &lean_toolchain {
        Some(_) if lean_version.is_some() => status::OK,
        Some(_) => status::PARSED_ONLY,
        None => status::ABSENT,
    };
    Toolchain {
        lean_toolchain,
        lean_version,
        status: status.to_string(),
    }
}

/// `leanprover/lean4:v4.33.1` -> `4.33.1`. Returns `None` for nightly/commit pins.
fn parse_lean_version(toolchain: &str) -> Option<String> {
    let tag = toolchain.rsplit(':').next()?;
    let version = tag.strip_prefix('v').unwrap_or(tag);
    if version.starts_with(|c: char| c.is_ascii_digit())
        && version.chars().all(|c| c.is_ascii_digit() || c == '.')
    {
        Some(version.to_string())
    } else {
        None
    }
}

// ── lakefile ───────────────────────────────────────────────────────────────

fn parse_lakefile(root: &Path) -> Lakefile {
    let toml_path = root.join("lakefile.toml");
    let lean_path = root.join("lakefile.lean");

    if toml_path.is_file() {
        let text = std::fs::read_to_string(&toml_path).unwrap_or_default();
        return match text.parse::<toml::Value>() {
            Ok(v) => lakefile_from_toml(&toml_path, &v),
            Err(e) => Lakefile {
                path: Some(toml_path.to_string_lossy().to_string()),
                kind: "toml".to_string(),
                name: None,
                version: None,
                default_targets: Vec::new(),
                requires: Vec::new(),
                lean_libs: Vec::new(),
                status: format!("{}: {e}", status::ERROR),
            },
        };
    }

    if lean_path.is_file() {
        return Lakefile {
            path: Some(lean_path.to_string_lossy().to_string()),
            kind: "lean".to_string(),
            name: None,
            version: None,
            default_targets: Vec::new(),
            requires: Vec::new(),
            lean_libs: Vec::new(),
            status: status::UNSUPPORTED.to_string(),
        };
    }

    Lakefile {
        path: None,
        kind: "none".to_string(),
        name: None,
        version: None,
        default_targets: Vec::new(),
        requires: Vec::new(),
        lean_libs: Vec::new(),
        status: status::ABSENT.to_string(),
    }
}

fn lakefile_from_toml(path: &Path, v: &toml::Value) -> Lakefile {
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);

    let default_targets = v
        .get("defaultTargets")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let requires = v
        .get("require")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|r| {
                    let name = r.get("name").and_then(|x| x.as_str())?.to_string();
                    Some(Require {
                        name,
                        git: r.get("git").and_then(|x| x.as_str()).map(str::to_string),
                        rev: r.get("rev").and_then(|x| x.as_str()).map(str::to_string),
                        path: r.get("path").and_then(|x| x.as_str()).map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let lean_libs = v
        .get("lean_lib")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|l| l.get("name").and_then(|x| x.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    Lakefile {
        path: Some(path.to_string_lossy().to_string()),
        kind: "toml".to_string(),
        name: s("name"),
        version: s("version"),
        default_targets,
        requires,
        lean_libs,
        status: status::OK.to_string(),
    }
}

// ── lake-manifest.json ─────────────────────────────────────────────────────

fn parse_lake_manifest(root: &Path) -> ManifestPins {
    let path = root.join("lake-manifest.json");
    if !path.is_file() {
        return ManifestPins {
            path: None,
            packages: Vec::new(),
            status: status::ABSENT.to_string(),
        };
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            return ManifestPins {
                path: Some(path.to_string_lossy().to_string()),
                packages: Vec::new(),
                status: format!("{}: {e}", status::ERROR),
            };
        }
    };
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            return ManifestPins {
                path: Some(path.to_string_lossy().to_string()),
                packages: Vec::new(),
                status: format!("{}: {e}", status::ERROR),
            };
        }
    };

    let packages = value
        .get("packages")
        .and_then(|p| p.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    let name = p.get("name").and_then(|x| x.as_str())?.to_string();
                    let url = p
                        .get("url")
                        .or_else(|| p.get("git"))
                        .and_then(|x| x.as_str())
                        .map(str::to_string);
                    let kind = if p.get("type").and_then(|x| x.as_str()) == Some("path") {
                        "path"
                    } else if url.is_some() {
                        "git"
                    } else {
                        "unknown"
                    };
                    Some(PackagePin {
                        name,
                        rev: p.get("rev").and_then(|x| x.as_str()).map(str::to_string),
                        url,
                        kind: kind.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    ManifestPins {
        path: Some(path.to_string_lossy().to_string()),
        packages,
        status: status::OK.to_string(),
    }
}

fn mathlib_pin(lakefile: &Lakefile, manifest: &ManifestPins) -> MathlibPin {
    let rev = manifest
        .packages
        .iter()
        .find(|p| p.name == "mathlib")
        .and_then(|p| p.rev.clone());
    let tag = lakefile
        .requires
        .iter()
        .find(|r| r.name == "mathlib")
        .and_then(|r| r.rev.clone());
    let status = if rev.is_some() || tag.is_some() {
        status::OK
    } else {
        status::ABSENT
    };
    MathlibPin {
        rev,
        tag,
        source: "lakefile require + lake-manifest.json".to_string(),
        status: status.to_string(),
    }
}

// ── audit implementation detection ─────────────────────────────────────────

fn detect_audit_impl(root: &Path) -> AuditImpl {
    let candidates = [
        (root.join("Audit.lean"), "lean_audit"),
        (root.join("AxiomCheck.lean"), "lean_axiomcheck"),
        (root.join("scripts/audit_axioms.py"), "python_script"),
        (root.join("audit_axioms.py"), "python_script"),
    ];

    let present: Vec<(&Path, &str)> = candidates
        .iter()
        .filter(|(p, _)| p.is_file())
        .map(|(p, k)| (p.as_path(), *k))
        .collect();

    if present.is_empty() {
        return AuditImpl {
            kind: "none".to_string(),
            files: Vec::new(),
            command_names: Vec::new(),
            heads: None,
            heads_source: "n/a".to_string(),
            expected_direct_sorry: None,
            allowed_axioms: Vec::new(),
            in_default_targets: None,
            status: status::ABSENT.to_string(),
        };
    }

    let kinds: std::collections::BTreeSet<&str> = present.iter().map(|(_, k)| *k).collect();
    let kind = if kinds.len() > 1 {
        "multiple".to_string()
    } else {
        kinds.iter().next().unwrap().to_string()
    };

    let lean_file = present
        .iter()
        .find(|(_, k)| k.starts_with("lean_"))
        .map(|(p, _)| *p);

    let mut command_names = Vec::new();
    let mut heads = None;
    let mut expected = None;
    let mut allowed: Vec<String> = Vec::new();
    let mut heads_source = "n/a".to_string();

    if let Some(path) = lean_file {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        if let Ok(stripped) = lean_src::strip_comments_and_strings(&text) {
            command_names = extract_elab_command_names(&stripped, &text);
            heads = extract_heads(&stripped);
            heads_source = if heads.is_some() {
                "parsed"
            } else {
                "unparsed"
            }
            .to_string();
            expected = extract_bracket_names(&stripped, "let expected");
            allowed = extract_bracket_names(&stripped, "let std").unwrap_or_default();
            // The three standard axioms are the contract, but only report them
            // when the audit file actually mentions them.
            if allowed.is_empty() && text.contains("propext") {
                allowed = STANDARD_AXIOMS.iter().map(|s| s.to_string()).collect();
            }
        }
    } else if kind == "python_script" || kind == "multiple" {
        heads_source = "cli".to_string();
    }

    AuditImpl {
        kind,
        files: present
            .iter()
            .map(|(p, _)| p.to_string_lossy().to_string())
            .collect(),
        command_names,
        heads,
        heads_source,
        expected_direct_sorry: expected,
        allowed_axioms: allowed,
        in_default_targets: None,
        status: status::OK.to_string(),
    }
}

/// Extract every elaborator command name, e.g. `#ang_audit`, `#ang_large_audit`.
///
/// The name lives inside a string literal, so it is read from `raw` at the
/// position located in `stripped` (which preserves character offsets).
fn extract_elab_command_names(stripped: &str, raw: &str) -> Vec<String> {
    let s: Vec<char> = stripped.chars().collect();
    let r: Vec<char> = raw.chars().collect();
    let needle = ['e', 'l', 'a', 'b'];
    let mut out: Vec<String> = Vec::new();

    let mut i = 0usize;
    while i + needle.len() <= s.len() {
        let is_match = s[i..i + needle.len()] == needle && (i == 0 || !is_name_char(s[i - 1]));
        if is_match {
            let mut j = i + needle.len();
            while j < r.len() && r[j].is_whitespace() {
                j += 1;
            }
            if j < r.len() && r[j] == '"' {
                let mut k = j + 1;
                let mut name = String::new();
                while k < r.len() && r[k] != '"' {
                    name.push(r[k]);
                    k += 1;
                }
                if !name.is_empty() && !out.contains(&name) {
                    out.push(name);
                }
            }
        }
        i += 1;
    }
    out
}

/// Collect head theorem names from an audit file.
///
/// Three shapes are supported, because TEAM42 uses all three:
/// - `let heads : List Name := [``A, ``B]`
/// - `for h in [``A, ``B] do`
/// - `Lean.collectAxioms ``A` (e.g. `feigenbaum-scalar-duality-lean`, which uses
///   `run_cmd` instead of a custom `elab` command)
fn extract_heads(stripped: &str) -> Option<Vec<String>> {
    let mut names: Vec<String> = Vec::new();
    let mut push = |candidate: Vec<String>| {
        for name in candidate {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    };

    if let Some(region) = region_after(stripped, "let heads") {
        push(double_backtick_names(&region));
    }
    for region in for_in_bracket_regions(stripped) {
        push(double_backtick_names(&region));
    }
    push(collect_axioms_targets(stripped));

    if names.is_empty() { None } else { Some(names) }
}

/// Names written as `` ``Name `` immediately after `collectAxioms`.
fn collect_axioms_targets(stripped: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let marker = "collectAxioms";
    let mut start = 0usize;
    while let Some(idx) = stripped[start..].find(marker) {
        let abs = start + idx + marker.len();
        let rest = stripped[abs..].trim_start();
        if let Some(after) = rest.strip_prefix("``") {
            let name: String = after.chars().take_while(|c| is_name_char(*c)).collect();
            if !name.is_empty() && !out.contains(&name) {
                out.push(name);
            }
        }
        start = abs;
    }
    out
}

/// Return the first top-level `[...]` that follows `marker`.
fn region_after(stripped: &str, marker: &str) -> Option<String> {
    let idx = stripped.find(marker)?;
    bracket_region(&stripped[idx..])
}

/// Contents of every `for … in [ … ]` bracket region.
fn for_in_bracket_regions(stripped: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0usize;
    while let Some(idx) = stripped[start..].find(" in [") {
        let abs = start + idx;
        let lookback = &stripped[abs.saturating_sub(40)..abs];
        if lookback.contains("for ")
            && let Some(region) = bracket_region(&stripped[abs..])
        {
            out.push(region);
        }
        start = abs + 4;
    }
    out
}

/// Find `marker` and return the names written as ``` ``Name ``` inside the first
/// bracketed region that follows it.
fn extract_bracket_names(stripped: &str, marker: &str) -> Option<Vec<String>> {
    let idx = stripped.find(marker)?;
    let region = bracket_region(&stripped[idx..])?;
    let mut names = Vec::new();
    for name in double_backtick_names(&region) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    Some(names)
}

/// Return the contents of the first top-level `[...]` in `text`.
fn bracket_region(text: &str) -> Option<String> {
    let start = text.find('[')?;
    let mut depth = 0usize;
    for (off, c) in text[start..].char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(text[start + 1..start + off].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Extract every ``` ``Name ``` occurrence.
fn double_backtick_names(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 1 < chars.len() {
        if chars[i] == '`' && chars[i + 1] == '`' {
            let mut j = i + 2;
            let mut name = String::new();
            while j < chars.len() && is_name_char(chars[j]) {
                name.push(chars[j]);
                j += 1;
            }
            if !name.is_empty() {
                out.push(name);
                i = j;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\'' || c == '.'
}

// ── manifests ──────────────────────────────────────────────────────────────

// ── receipts ───────────────────────────────────────────────────────────────

fn parse_receipts(root: &Path) -> Receipts {
    let dir = root.join("receipts");
    if !dir.is_dir() {
        return Receipts {
            dir: None,
            files: Vec::new(),
            summaries: Vec::new(),
            status: status::ABSENT.to_string(),
        };
    }

    // Receipts may be nested per architecture (e.g. receipts/aarch64/,
    // receipts/x86_64/), so walk recursively.
    let mut files: Vec<String> = WalkDir::new(&dir)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| {
            e.path()
                .strip_prefix(root)
                .unwrap_or(e.path())
                .to_string_lossy()
                .to_string()
        })
        .collect();
    files.sort();

    let mut summaries = Vec::new();
    for file in &files {
        if !file.ends_with("build_summary.txt") {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(root.join(file)) {
            let mut s = ReceiptSummary {
                file: file.clone(),
                claimed_modules_built: None,
                claimed_errors: None,
                claimed_sorry_warnings: None,
                claimed_axioms: Vec::new(),
                claimed_placeholders: None,
                claimed_direct_sorry: Vec::new(),
            };
            parse_build_summary(&text, &mut s);
            summaries.push(s);
        }
    }

    Receipts {
        dir: Some("receipts".to_string()),
        files,
        summaries,
        status: status::PARSED_ONLY.to_string(),
    }
}

/// Parse a `receipts/**/build_summary.txt`.
///
/// The label formats differ between repositories — `sierksma33` writes
/// `modules built (lines '[k/n] Built'): 3683` while
/// `ang-subset-products-lean` writes `lines '[k/n] Built <module>' in
/// build.log (Mathlib files come from the cache): 25` — so the matcher keys on
/// the stable parts (`[k/n] Built`, `error lines`, `sorry' warnings`) and always
/// requires a trailing integer.
fn parse_build_summary(text: &str, s: &mut ReceiptSummary) {
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        let trailing = line
            .rsplit(':')
            .next()
            .map(str::trim)
            .and_then(|n| n.parse::<usize>().ok());

        if (lower.contains("[k/n] built") || lower.starts_with("modules built"))
            && let Some(n) = trailing
        {
            s.claimed_modules_built = Some(n);
        } else if lower.starts_with("error lines")
            && let Some(n) = trailing
        {
            s.claimed_errors = Some(n);
        } else if lower.contains("sorry")
            && lower.contains("warnings")
            && lower.contains("declaration uses")
            && let Some(n) = trailing
        {
            s.claimed_sorry_warnings = Some(n);
        }

        let Some(pos) = line.find("AUDIT ") else {
            continue;
        };
        let rest = &line[pos + "AUDIT ".len()..];
        if let Some(r) = rest.strip_prefix("axioms ") {
            if let Some(colon) = r.find(':') {
                s.claimed_axioms.push(ClaimedAxioms {
                    name: r[..colon].trim().to_string(),
                    axioms: parse_bracket_list(&r[colon..]),
                });
            }
        } else if let Some(r) = rest.strip_prefix("placeholders ") {
            if let Some(open) = r.find('(')
                && let Some(close) = r[open..].find(')')
            {
                s.claimed_placeholders = r[open + 1..open + close].trim().parse().ok();
            }
        } else if let Some(r) = rest.strip_prefix("declarations using sorry directly:") {
            s.claimed_direct_sorry = parse_bracket_list(r);
        }
    }
}

/// Contents of the first `[...]` in `text`, split on commas.
fn parse_bracket_list(text: &str) -> Vec<String> {
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

// ── misc ───────────────────────────────────────────────────────────────────

fn project_name(root: &Path, lakefile: &Lakefile) -> String {
    if let Some(name) = &lakefile.name {
        return name.clone();
    }
    root.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.display().to_string())
}

fn git_info(root: &Path) -> Source {
    let run = |args: &[&str]| -> Option<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    };

    let commit = run(&["rev-parse", "HEAD"]).filter(|s| !s.is_empty());
    let dirty = run(&["status", "--porcelain"]).map(|s| !s.is_empty());
    let note = if commit.is_none() {
        Some("git not available or not a repository".to_string())
    } else {
        None
    };

    Source {
        path: root.to_string_lossy().to_string(),
        git_commit: commit,
        git_dirty: dirty,
        git_note: note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_release_toolchain() {
        assert_eq!(
            parse_lean_version("leanprover/lean4:v4.33.1"),
            Some("4.33.1".to_string())
        );
        assert_eq!(
            parse_lean_version("leanprover/lean4:v4.35.0"),
            Some("4.35.0".to_string())
        );
    }

    #[test]
    fn nightly_toolchain_has_no_version() {
        assert_eq!(
            parse_lean_version("leanprover/lean4:nightly-2024-01-01"),
            None
        );
    }

    #[test]
    fn module_name_from_path() {
        assert_eq!(
            module_name(Path::new("SierksmaLean/Main.lean")),
            "SierksmaLean.Main"
        );
        assert_eq!(module_name(Path::new("Main.lean")), "Main");
        assert_eq!(module_name(Path::new("A/B/C.lean")), "A.B.C");
    }

    #[test]
    fn extracts_elab_command_names() {
        let src = "open Lean Elab Command\nelab \"#ang_audit\" : command => do\n  pure ()\nelab \"#ang_large_audit\" : command => do\n  pure ()\n";
        let stripped = lean_src::strip_comments_and_strings(src).unwrap();
        assert_eq!(
            extract_elab_command_names(&stripped, src),
            vec!["#ang_audit".to_string(), "#ang_large_audit".to_string()]
        );
    }

    #[test]
    fn elab_inside_a_comment_is_not_a_command() {
        let src = "-- elab \"#fake\" : command => do\nelab \"#real\" : command => do\n  pure ()\n";
        let stripped = lean_src::strip_comments_and_strings(src).unwrap();
        assert_eq!(
            extract_elab_command_names(&stripped, src),
            vec!["#real".to_string()]
        );
    }

    #[test]
    fn extracts_heads_from_for_in_shape() {
        // The shape used by ang-subset-products-lean/Audit.lean.
        let src = "  for h in [``oeis_60957_conjecture_0.disproof, ``Ang.disproof, ``Ang.counterexample] do\n    pure ()\n";
        assert_eq!(
            extract_heads(src),
            Some(vec![
                "oeis_60957_conjecture_0.disproof".to_string(),
                "Ang.disproof".to_string(),
                "Ang.counterexample".to_string(),
            ])
        );
    }

    #[test]
    fn extract_heads_ignores_for_over_plain_bindings() {
        let src = "  for n in localDecls do\n    pure ()\n";
        assert_eq!(extract_heads(src), None);
    }

    #[test]
    fn extracts_heads_from_run_cmd_shape() {
        // The shape used by feigenbaum-scalar-duality-lean/Audit.lean.
        let src = "open Lean Elab Command in\nrun_cmd do\n  let found := (← liftCoreM <| Lean.collectAxioms ``AffineWordLog.realIntervalDuality).toList\n  let expected := [``propext, ``Classical.choice, ``Quot.sound]\n";
        assert_eq!(
            extract_heads(src),
            Some(vec!["AffineWordLog.realIntervalDuality".to_string()])
        );
    }

    #[test]
    fn collect_axioms_on_a_variable_yields_no_heads() {
        // sierksma33 / ang / rubik pass a bound variable, not a literal.
        let src =
            "  for h in heads do\n    let axs := (← liftCoreM <| Lean.collectAxioms h).toList\n";
        assert_eq!(collect_axioms_targets(src), Vec::<String>::new());
    }

    #[test]
    fn extracts_heads_from_audit_lean_shape() {
        let src = "  let expected : List Name := []\n  let heads : List Name := [``SierksmaLean.main_theorem, ``SierksmaLean.sierksma_three_three, ``sierksma_three_three]\n";
        assert_eq!(
            extract_bracket_names(src, "let heads"),
            Some(vec![
                "SierksmaLean.main_theorem".to_string(),
                "SierksmaLean.sierksma_three_three".to_string(),
                "sierksma_three_three".to_string(),
            ])
        );
        assert_eq!(extract_bracket_names(src, "let expected"), Some(vec![]));
    }

    #[test]
    fn extracts_heads_from_axiomcheck_shape() {
        let src = "  let heads : List (Name × Bool) := [(``RubikSTM.rubik_stm_diameter_entropy, false),\n    (``rubik_stm_diameter_entropy, false)]\n";
        assert_eq!(
            extract_bracket_names(src, "let heads"),
            Some(vec![
                "RubikSTM.rubik_stm_diameter_entropy".to_string(),
                "rubik_stm_diameter_entropy".to_string(),
            ])
        );
    }

    #[test]
    fn parses_sierksma_style_build_summary() {
        let text = "modules built (lines '[k/n] Built'): 3683\nerror lines: 0\n'declaration uses sorry' warnings: 1\nwarning: Challenge.lean:10:8: declaration uses `sorry`\ninfo: Audit.lean:43:0: AUDIT axioms SierksmaLean.main_theorem: [propext, Classical.choice, Quot.sound]\ninfo: Audit.lean:43:0: AUDIT placeholders (0): []\n";
        let mut s = ReceiptSummary {
            file: "receipts/build_summary.txt".to_string(),
            claimed_modules_built: None,
            claimed_errors: None,
            claimed_sorry_warnings: None,
            claimed_axioms: Vec::new(),
            claimed_placeholders: None,
            claimed_direct_sorry: Vec::new(),
        };
        parse_build_summary(text, &mut s);
        assert_eq!(s.claimed_modules_built, Some(3683));
        assert_eq!(s.claimed_errors, Some(0));
        assert_eq!(s.claimed_sorry_warnings, Some(1));
        assert_eq!(s.claimed_placeholders, Some(0));
        assert_eq!(s.claimed_axioms.len(), 1);
        assert_eq!(s.claimed_axioms[0].name, "SierksmaLean.main_theorem");
        assert_eq!(s.claimed_axioms[0].axioms.len(), 3);
    }

    #[test]
    fn parses_ang_style_build_summary() {
        // The label format differs from sierksma33's, and the individual warning
        // lines must not be mistaken for the count line.
        let text = "lines '[k/n] Built <module>' in build.log (Mathlib files come from the cache): 25\nerror lines: 0\n'declaration uses sorry' warnings: 3\nwarning: Challenge.lean:34:8: declaration uses `sorry`\ninfo: Audit.lean:38:0: AUDIT axioms Ang.disproof: [propext, Classical.choice, Quot.sound]\ninfo: Audit.lean:38:0: AUDIT declarations using sorry directly: []\ninfo: Audit.lean:68:0: AUDIT placeholders (0): []\n";
        let mut s = ReceiptSummary {
            file: "receipts/aarch64/build_summary.txt".to_string(),
            claimed_modules_built: None,
            claimed_errors: None,
            claimed_sorry_warnings: None,
            claimed_axioms: Vec::new(),
            claimed_placeholders: None,
            claimed_direct_sorry: Vec::new(),
        };
        parse_build_summary(text, &mut s);
        assert_eq!(s.claimed_modules_built, Some(25));
        assert_eq!(
            s.claimed_sorry_warnings,
            Some(3),
            "must not be overwritten by warning lines"
        );
        assert_eq!(s.claimed_axioms.len(), 1);
        assert!(s.claimed_direct_sorry.is_empty());
    }

    #[test]
    fn bracket_region_handles_nesting() {
        assert_eq!(
            bracket_region("x := [a, [b, c], d] tail"),
            Some("a, [b, c], d".to_string())
        );
        assert_eq!(bracket_region("no brackets"), None);
    }
}
