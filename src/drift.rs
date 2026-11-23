//! M5 `drift` — compare two `lean-audit` JSON reports and report what changed.
//!
//! The point is to run the *same* cheap checks on a repository before and after
//! a tag, and see only the differences: a changed toolchain pin, a new or
//! removed head theorem, a manifest entry that stopped matching, a `sorry` that
//! appeared. It does not judge whether a change is good or bad.
//!
//! Both `inspect` reports and `batch` reports are accepted (a `batch` report is
//! a list of `inspect` reports). They are matched by project name. A field that
//! changed is emitted once, with the old and new values side by side.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::model::{DISCLAIMER, ManifestInfo, Report, SCHEMA};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub field: String,
    pub old: String,
    pub new: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDrift {
    pub project: String,
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftReport {
    pub schema: String,
    pub generated_at: String,
    pub tool_version: String,
    pub old: String,
    pub new: String,
    pub projects: Vec<ProjectDrift>,
    /// Projects present only in the old (resp. new) report.
    pub only_in_old: Vec<String>,
    pub only_in_new: Vec<String>,
    pub disclaimer: String,
}

impl DriftReport {
    pub fn has_drift(&self) -> bool {
        !self.only_in_old.is_empty()
            || !self.only_in_new.is_empty()
            || self.projects.iter().any(|p| !p.changes.is_empty())
    }

    pub fn change_count(&self) -> usize {
        self.only_in_old.len()
            + self.only_in_new.len()
            + self.projects.iter().map(|p| p.changes.len()).sum::<usize>()
    }
}

pub fn run(old_path: &Path, new_path: &Path) -> Result<DriftReport> {
    let old = load(old_path)?;
    let new = load(new_path)?;

    let mut projects = Vec::new();
    for (name, o) in &old {
        if let Some(n) = new.get(name) {
            let changes = diff_reports(o, n);
            if !changes.is_empty() {
                projects.push(ProjectDrift {
                    project: name.clone(),
                    changes,
                });
            }
        }
    }

    let only_in_old: Vec<String> = old
        .keys()
        .filter(|k| !new.contains_key(*k))
        .cloned()
        .collect();
    let only_in_new: Vec<String> = new
        .keys()
        .filter(|k| !old.contains_key(*k))
        .cloned()
        .collect();

    Ok(DriftReport {
        schema: format!("{SCHEMA}/drift"),
        generated_at: crate::model::now_iso8601(),
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        old: old_path.to_string_lossy().to_string(),
        new: new_path.to_string_lossy().to_string(),
        projects,
        only_in_old,
        only_in_new,
        disclaimer: DISCLAIMER.to_string(),
    })
}

/// Load a report file into `project name -> Report`, accepting either a single
/// report object or a batch report with a `repos` array.
fn load(path: &Path) -> Result<BTreeMap<String, Report>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("parsing {} as JSON", path.display()))?;

    let mut map = BTreeMap::new();
    if let Some(repos) = value.get("repos").and_then(|v| v.as_array()) {
        for item in repos {
            let report: Report = serde_json::from_value(item.clone()).with_context(|| {
                format!(
                    "{}: a `repos` entry is not a lean-audit report",
                    path.display()
                )
            })?;
            map.insert(report.project.clone(), report);
        }
    } else {
        let report: Report = serde_json::from_value(value)
            .with_context(|| format!("{} is not a lean-audit report", path.display()))?;
        map.insert(report.project.clone(), report);
    }
    Ok(map)
}

fn diff_reports(old: &Report, new: &Report) -> Vec<Change> {
    let mut c = Vec::new();

    // Source / toolchain / mathlib pins.
    push(
        &mut c,
        "source.git_commit",
        opt(&old.source.git_commit),
        opt(&new.source.git_commit),
    );
    push(
        &mut c,
        "source.git_dirty",
        opt(&old.source.git_dirty),
        opt(&new.source.git_dirty),
    );
    push(
        &mut c,
        "toolchain.lean_toolchain",
        opt(&old.toolchain.lean_toolchain),
        opt(&new.toolchain.lean_toolchain),
    );
    push(
        &mut c,
        "toolchain.lean_version",
        opt(&old.toolchain.lean_version),
        opt(&new.toolchain.lean_version),
    );
    push(
        &mut c,
        "mathlib.rev",
        opt(&old.mathlib.rev),
        opt(&new.mathlib.rev),
    );
    push(
        &mut c,
        "mathlib.tag",
        opt(&old.mathlib.tag),
        opt(&new.mathlib.tag),
    );

    // Lakefile.
    push(
        &mut c,
        "lakefile.default_targets",
        joined(&old.lakefile.default_targets),
        joined(&new.lakefile.default_targets),
    );
    push(
        &mut c,
        "lakefile.requires",
        requires(&old.lakefile.requires),
        requires(&new.lakefile.requires),
    );

    // Modules.
    push(
        &mut c,
        "modules.count",
        old.modules.count.to_string(),
        new.modules.count.to_string(),
    );
    push(
        &mut c,
        "modules.import_edges_internal",
        old.modules.import_edges_internal.to_string(),
        new.modules.import_edges_internal.to_string(),
    );
    push(
        &mut c,
        "modules.max_import_depth",
        opt(&old.modules.max_import_depth),
        opt(&new.modules.max_import_depth),
    );
    push(
        &mut c,
        "modules.cycles",
        old.modules.cycles.len().to_string(),
        new.modules.cycles.len().to_string(),
    );

    // Audit implementation.
    push(
        &mut c,
        "audit_impl.kind",
        old.audit_impl.kind.clone(),
        new.audit_impl.kind.clone(),
    );
    push(
        &mut c,
        "audit_impl.command_names",
        joined(&old.audit_impl.command_names),
        joined(&new.audit_impl.command_names),
    );
    push(
        &mut c,
        "audit_impl.heads",
        opt_list(&old.audit_impl.heads),
        opt_list(&new.audit_impl.heads),
    );
    push(
        &mut c,
        "audit_impl.allowed_axioms",
        joined(&old.audit_impl.allowed_axioms),
        joined(&new.audit_impl.allowed_axioms),
    );
    push(
        &mut c,
        "audit_impl.expected_direct_sorry",
        opt_list(&old.audit_impl.expected_direct_sorry),
        opt_list(&new.audit_impl.expected_direct_sorry),
    );

    // Forbidden-token scan.
    push(
        &mut c,
        "scan.files_scanned",
        old.scan.files_scanned.to_string(),
        new.scan.files_scanned.to_string(),
    );
    push(
        &mut c,
        "scan.flags_in_scope",
        old.scan.flags_in_scope.to_string(),
        new.scan.flags_in_scope.to_string(),
    );
    push(
        &mut c,
        "scan.flags",
        flags(&old.scan.flags),
        flags(&new.scan.flags),
    );

    // Manifests, per file.
    let om: BTreeMap<&str, &ManifestInfo> =
        old.manifests.iter().map(|m| (m.file.as_str(), m)).collect();
    let nm: BTreeMap<&str, &ManifestInfo> =
        new.manifests.iter().map(|m| (m.file.as_str(), m)).collect();
    for (file, o) in &om {
        match nm.get(file) {
            Some(n) => {
                push(
                    &mut c,
                    &format!("manifest[{file}].status"),
                    o.status.clone(),
                    n.status.clone(),
                );
                push(
                    &mut c,
                    &format!("manifest[{file}].checked"),
                    opt(&o.checked),
                    opt(&n.checked),
                );
                push(
                    &mut c,
                    &format!("manifest[{file}].matched"),
                    opt(&o.matched),
                    opt(&n.matched),
                );
                push(
                    &mut c,
                    &format!("manifest[{file}].mismatched"),
                    o.mismatched.len().to_string(),
                    n.mismatched.len().to_string(),
                );
                push(
                    &mut c,
                    &format!("manifest[{file}].missing"),
                    o.missing.len().to_string(),
                    n.missing.len().to_string(),
                );
            }
            None => push(
                &mut c,
                &format!("manifest[{file}]"),
                "present".into(),
                "removed".into(),
            ),
        }
    }
    for file in nm.keys() {
        if !om.contains_key(file) {
            push(
                &mut c,
                &format!("manifest[{file}]"),
                "absent".into(),
                "added".into(),
            );
        }
    }

    // Published axiom claims from receipts.
    let oc = claimed_axioms(old);
    let nc = claimed_axioms(new);
    for (name, o) in &oc {
        match nc.get(name) {
            Some(n) => push(
                &mut c,
                &format!("claim[{name}].axioms"),
                o.join(", "),
                n.join(", "),
            ),
            None => push(
                &mut c,
                &format!("claim[{name}]"),
                "present".into(),
                "removed".into(),
            ),
        }
    }
    for name in nc.keys() {
        if !oc.contains_key(name) {
            push(
                &mut c,
                &format!("claim[{name}]"),
                "absent".into(),
                "added".into(),
            );
        }
    }

    c
}

fn claimed_axioms(r: &Report) -> BTreeMap<String, Vec<String>> {
    let mut map = BTreeMap::new();
    for s in &r.receipts.summaries {
        for c in &s.claimed_axioms {
            map.insert(c.name.clone(), c.axioms.clone());
        }
    }
    map
}

fn push(c: &mut Vec<Change>, field: &str, old: String, new: String) {
    if old != new {
        c.push(Change {
            field: field.to_string(),
            old,
            new,
        });
    }
}

fn opt<T: std::fmt::Display>(v: &Option<T>) -> String {
    v.as_ref()
        .map(|x| x.to_string())
        .unwrap_or_else(|| "—".to_string())
}

fn opt_list(v: &Option<Vec<String>>) -> String {
    match v {
        Some(list) => joined(list),
        None => "—".to_string(),
    }
}

fn joined(v: &[String]) -> String {
    if v.is_empty() {
        "—".to_string()
    } else {
        v.join(", ")
    }
}

fn requires(v: &[crate::model::Require]) -> String {
    if v.is_empty() {
        return "—".to_string();
    }
    v.iter()
        .map(|r| {
            let rev = r.rev.clone().or_else(|| r.git.clone()).unwrap_or_default();
            format!("{}@{}", r.name, rev)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn flags(v: &[crate::model::Flag]) -> String {
    if v.is_empty() {
        return "—".to_string();
    }
    v.iter()
        .map(|f| format!("{}:{}:{}", f.path, f.line, f.token))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joined_handles_empty() {
        assert_eq!(joined(&[]), "—");
        assert_eq!(joined(&["a".into(), "b".into()]), "a, b");
    }

    #[test]
    fn push_skips_equal_values() {
        let mut c = Vec::new();
        push(&mut c, "f", "x".into(), "x".into());
        assert!(c.is_empty());
        push(&mut c, "f", "x".into(), "y".into());
        assert_eq!(c.len(), 1);
    }
}
