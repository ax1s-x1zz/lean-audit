//! Rendering: JSON and Markdown.

use anyhow::Result;
use serde::Serialize;

use crate::model::*;

pub fn to_json<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string_pretty(value)?)
}

pub fn inspect_markdown(r: &Report) -> String {
    let mut s = String::new();

    s.push_str(&format!("# lean-audit report — {}\n\n", r.project));
    s.push_str(&format!("> **{}**\n\n", r.disclaimer));
    s.push_str(&format!(
        "schema `{}` · tool `{}` · generated `{}`\n\n",
        r.schema, r.tool_version, r.generated_at
    ));

    // Source
    s.push_str("## Source\n\n");
    s.push_str(&format!("- path: `{}`\n", r.source.path));
    match (&r.source.git_commit, r.source.git_dirty) {
        (Some(c), Some(d)) => s.push_str(&format!(
            "- commit: `{}`{}\n",
            &c[..c.len().min(12)],
            if d { " (dirty)" } else { "" }
        )),
        _ => s.push_str(&format!(
            "- commit: unavailable ({})\n",
            r.source.git_note.as_deref().unwrap_or("unknown")
        )),
    }

    // Toolchain / mathlib
    s.push_str("\n## Toolchain\n\n");
    s.push_str("| field | value | status |\n|---|---|---|\n");
    s.push_str(&format!(
        "| lean-toolchain | {} | {} |\n",
        code_or_dash(r.toolchain.lean_toolchain.as_deref()),
        r.toolchain.status
    ));
    s.push_str(&format!(
        "| lean version | {} | |\n",
        code_or_dash(r.toolchain.lean_version.as_deref())
    ));
    s.push_str(&format!(
        "| mathlib (lakefile rev) | {} | {} |\n",
        code_or_dash(r.mathlib.tag.as_deref()),
        r.mathlib.status
    ));
    s.push_str(&format!(
        "| mathlib rev | {} | |\n",
        code_or_dash(r.mathlib.rev.as_deref())
    ));

    // Lakefile
    s.push_str("\n## Lakefile\n\n");
    s.push_str(&format!(
        "- kind: `{}` · status: `{}`\n",
        r.lakefile.kind, r.lakefile.status
    ));
    if !r.lakefile.default_targets.is_empty() {
        s.push_str(&format!(
            "- defaultTargets: {}\n",
            r.lakefile
                .default_targets
                .iter()
                .map(|t| format!("`{t}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !r.lakefile.requires.is_empty() {
        s.push_str("\n| require | git | rev |\n|---|---|---|\n");
        for req in &r.lakefile.requires {
            s.push_str(&format!(
                "| `{}` | {} | {} |\n",
                req.name,
                code_or_dash(req.git.as_deref()),
                code_or_dash(req.rev.as_deref())
            ));
        }
    }

    // Modules
    s.push_str("\n## Modules\n\n");
    s.push_str(&format!(
        "- count: **{}** · internal import edges: **{}** · max import depth: {}\n",
        r.modules.count,
        r.modules.import_edges_internal,
        r.modules
            .max_import_depth
            .map(|d| d.to_string())
            .unwrap_or_else(|| "n/a".to_string())
    ));
    s.push_str(&format!(
        "- external imports: {} distinct ({})\n",
        r.modules.external_imports.len(),
        preview(&r.modules.external_imports, 12)
    ));
    if !r.modules.roots.is_empty() {
        s.push_str(&format!("- roots: {}\n", preview(&r.modules.roots, 20)));
    }
    if !r.modules.cycles.is_empty() {
        s.push_str(&format!("- ⚠ import cycles: {}\n", r.modules.cycles.len()));
    }
    if !r.modules.by_directory.is_empty() {
        s.push_str("\n| directory | .lean files |\n|---|---|\n");
        for d in r.modules.by_directory.iter().take(12) {
            s.push_str(&format!("| `{}` | {} |\n", d.dir, d.files));
        }
    }

    // Audit implementation
    s.push_str("\n## Audit implementation\n\n");
    s.push_str(&format!("- kind: `{}`\n", r.audit_impl.kind));
    if !r.audit_impl.files.is_empty() {
        s.push_str(&format!(
            "- files: {}\n",
            r.audit_impl
                .files
                .iter()
                .map(|f| format!("`{f}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if let Some(cmd) = &r.audit_impl.command_names.first() {
        if r.audit_impl.command_names.len() == 1 {
            s.push_str(&format!("- command: `{cmd}`\n"));
        } else {
            s.push_str(&format!(
                "- commands: {}\n",
                r.audit_impl
                    .command_names
                    .iter()
                    .map(|c| format!("`{c}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    s.push_str(&format!(
        "- heads: {} (source: `{}`)\n",
        match &r.audit_impl.heads {
            Some(h) => format!("**{}**", h.len()),
            None => "not extracted".to_string(),
        },
        r.audit_impl.heads_source
    ));
    if let Some(h) = &r.audit_impl.heads {
        for name in h.iter().take(40) {
            s.push_str(&format!("  - `{name}`\n"));
        }
        if h.len() > 40 {
            s.push_str(&format!("  - … and {} more\n", h.len() - 40));
        }
    }
    if let Some(e) = &r.audit_impl.expected_direct_sorry {
        s.push_str(&format!("- expected direct `sorry` users: {}\n", e.len()));
    }
    if !r.audit_impl.allowed_axioms.is_empty() {
        s.push_str(&format!(
            "- allowed axioms: {}\n",
            r.audit_impl
                .allowed_axioms
                .iter()
                .map(|a| format!("`{a}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    // Manifests
    s.push_str("\n## Manifests (presence only)\n\n");
    if r.manifests.is_empty() {
        s.push_str("_none found_\n");
    } else {
        s.push_str("| file | entries | status |\n|---|---|---|\n");
        for m in &r.manifests {
            s.push_str(&format!(
                "| `{}` | {} | {} |\n",
                m.file,
                m.entries
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "?".to_string()),
                m.status
            ));
        }
        s.push_str("\n_Hash verification is not run yet (see `NEXT.md`, M2)._\n");
    }

    // Receipts
    s.push_str("\n## Receipts\n\n");
    match &r.receipts.dir {
        Some(dir) => {
            s.push_str(&format!(
                "- `{dir}/` — {} file(s)\n",
                r.receipts.files.len()
            ));
            if r.receipts.summaries.is_empty() {
                s.push_str("- no `build_summary.txt` found\n");
            } else {
                s.push_str(
                    "\n| build_summary | modules built | errors | sorry warnings | axiom claims | placeholders |\n|---|---|---|---|---|---|\n",
                );
                for sum in &r.receipts.summaries {
                    s.push_str(&format!(
                        "| `{}` | {} | {} | {} | {} | {} |\n",
                        sum.file,
                        opt_num(sum.claimed_modules_built),
                        opt_num(sum.claimed_errors),
                        opt_num(sum.claimed_sorry_warnings),
                        sum.claimed_axioms.len(),
                        opt_num(sum.claimed_placeholders),
                    ));
                }
                if let Some(first) = r.receipts.summaries.first() {
                    if !first.claimed_axioms.is_empty() {
                        s.push_str(&format!(
                            "\n**Published axiom claims** (from `{}`):\n\n",
                            first.file
                        ));
                        for claim in first.claimed_axioms.iter().take(40) {
                            s.push_str(&format!(
                                "- `{}`: [{}]\n",
                                claim.name,
                                claim.axioms.join(", ")
                            ));
                        }
                        if first.claimed_axioms.len() > 40 {
                            s.push_str(&format!(
                                "- … and {} more\n",
                                first.claimed_axioms.len() - 40
                            ));
                        }
                    }
                    s.push_str(&format!(
                        "- direct `sorry` users claimed: {}\n",
                        if first.claimed_direct_sorry.is_empty() {
                            "none".to_string()
                        } else {
                            first.claimed_direct_sorry.join(", ")
                        }
                    ));
                }
            }
            s.push_str("\n_These are claims parsed from `receipts/`. Re-computation is M6._\n");
        }
        None => s.push_str("_no `receipts/` directory_\n"),
    }

    // Scan
    s.push_str("\n## Forbidden-token scan\n\n");
    s.push_str(&format!(
        "- files scanned: {} · flags: **{}**\n",
        r.scan.files_scanned,
        r.scan.flags.len()
    ));
    if !r.scan.flags.is_empty() {
        s.push_str("\n| file | line | token |\n|---|---|---|\n");
        for f in r.scan.flags.iter().take(50) {
            s.push_str(&format!("| `{}` | {} | `{}` |\n", f.path, f.line, f.token));
        }
        if r.scan.flags.len() > 50 {
            s.push_str(&format!(
                "\n_… and {} more flags._\n",
                r.scan.flags.len() - 50
            ));
        }
    }
    if let Some(note) = &r.scan.note {
        s.push_str(&format!("\n_{note}_\n"));
    }

    if !r.warnings.is_empty() {
        s.push_str("\n## Warnings\n\n");
        for w in &r.warnings {
            s.push_str(&format!("- {w}\n"));
        }
    }

    s.push_str(&format!("\n---\n\n_{}_\n", r.disclaimer));
    s
}

pub fn batch_markdown(b: &BatchReport) -> String {
    let mut s = String::new();
    s.push_str("# lean-audit batch report\n\n");
    s.push_str(&format!("> **{}**\n\n", b.disclaimer));
    s.push_str(&format!(
        "root `{}` · {} repositories · tool `{}` · generated `{}`\n\n",
        b.root,
        b.repos.len(),
        b.tool_version,
        b.generated_at
    ));

    s.push_str("| project | lean | modules | edges | depth | audit | commands | heads | flags | manifests | claims |\n");
    s.push_str("|---|---|---|---|---|---|---|---|---|---|---|\n");
    for r in &b.repos {
        let claims: usize = r
            .receipts
            .summaries
            .iter()
            .map(|x| x.claimed_axioms.len())
            .sum();
        s.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | `{}` | {} | {} | {} | {} | {} |\n",
            r.project,
            r.toolchain.lean_version.as_deref().unwrap_or("—"),
            r.modules.count,
            r.modules.import_edges_internal,
            r.modules
                .max_import_depth
                .map(|d| d.to_string())
                .unwrap_or_else(|| "—".to_string()),
            r.audit_impl.kind,
            if r.audit_impl.command_names.is_empty() {
                "—".to_string()
            } else {
                format!("`{}`", r.audit_impl.command_names.join("` `"))
            },
            r.audit_impl
                .heads
                .as_ref()
                .map(|h| h.len().to_string())
                .unwrap_or_else(|| "—".to_string()),
            r.scan.flags.len(),
            r.manifests.len(),
            claims,
        ));
    }

    s.push_str("\n## Notes\n\n");
    s.push_str(
        "- `heads` is the number of head theorems found in the repository's own audit file.\n",
    );
    s.push_str("- `claims` is the number of published `AUDIT axioms <name>: [...]` lines parsed from `receipts/`. Re-computing them is M3.\n");
    s.push_str(
        "- `manifests` counts manifest files discovered; their hashes are not verified yet (M2).\n",
    );
    s.push_str("- `flags` counts forbidden tokens (`sorry`, `admit`, `axiom`, `unsafe`, `implemented_by`, `extern`) in comment- and string-stripped source.\n");
    s.push_str(&format!("\n---\n\n_{}_\n", b.disclaimer));
    s
}

#[derive(Serialize)]
pub struct ScanReport {
    pub schema: String,
    pub generated_at: String,
    pub tool_version: String,
    pub project: String,
    pub scan: ScanSummary,
    pub disclaimer: String,
}

pub fn scan_markdown(r: &ScanReport) -> String {
    let mut s = String::new();
    s.push_str(&format!("# lean-audit scan — {}\n\n", r.project));
    s.push_str(&format!("> **{}**\n\n", r.disclaimer));
    s.push_str(&format!(
        "files scanned: {} · flags: **{}**\n\n",
        r.scan.files_scanned,
        r.scan.flags.len()
    ));
    if r.scan.flags.is_empty() {
        s.push_str("No forbidden tokens found in comments-stripped source.\n");
    } else {
        s.push_str("| file | line | token |\n|---|---|---|\n");
        for f in &r.scan.flags {
            s.push_str(&format!("| `{}` | {} | `{}` |\n", f.path, f.line, f.token));
        }
    }
    if let Some(note) = &r.scan.note {
        s.push_str(&format!("\n_{note}_\n"));
    }
    s.push_str(&format!("\n---\n\n_{}_\n", r.disclaimer));
    s
}

fn code_or_dash(v: Option<&str>) -> String {
    match v {
        Some(v) => format!("`{v}`"),
        None => "—".to_string(),
    }
}

fn opt_num(v: Option<usize>) -> String {
    v.map(|n| n.to_string()).unwrap_or_else(|| "?".to_string())
}

fn preview(items: &[String], n: usize) -> String {
    if items.is_empty() {
        return "none".to_string();
    }
    let shown: Vec<String> = items.iter().take(n).map(|s| format!("`{s}`")).collect();
    if items.len() > n {
        format!("{}, … (+{})", shown.join(", "), items.len() - n)
    } else {
        shown.join(", ")
    }
}
