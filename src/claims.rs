//! Published axiom claims, gathered from the artifacts a repository ships.
//!
//! Two sources are used, both already produced by the repository itself:
//!
//! 1. `receipts/**/build_summary.txt` — `AUDIT axioms <name>: [...]` lines
//! 2. `**/AXIOM_AUDIT.json` — `general_declarations_exact_axioms` and
//!    `small_native_regressions_exact_axioms` maps
//!
//! Nothing here is inferred from the Lean sources: these are the numbers the
//! repository itself published, and they are what `axioms` compares against.

use std::collections::BTreeMap;
use std::path::Path;

use walkdir::WalkDir;

use crate::model::Report;
use crate::util;

#[derive(Debug, Clone)]
pub struct PublishedClaim {
    pub axioms: Vec<String>,
    pub source: String,
}

/// Map of declaration name -> published axiom list.
///
/// When two artifacts disagree, the first source wins and the conflict is
/// reported through `conflicts`.
pub struct Claims {
    pub by_name: BTreeMap<String, PublishedClaim>,
    /// Names published with two different axiom lists.
    pub conflicts: Vec<ClaimConflict>,
}

pub struct ClaimConflict {
    pub name: String,
    pub first: (Vec<String>, String),
    pub second: (Vec<String>, String),
}

impl Claims {
    pub fn get(&self, name: &str) -> Option<&PublishedClaim> {
        self.by_name.get(name)
    }

    pub fn len(&self) -> usize {
        self.by_name.len()
    }
}

/// Gather claims from the receipts already parsed by `inspect`, plus every
/// `AXIOM_AUDIT.json` in the tree.
pub fn collect(root: &Path, report: &Report) -> Claims {
    let mut by_name: BTreeMap<String, PublishedClaim> = BTreeMap::new();
    let mut conflicts = Vec::new();

    // 1. receipts/build_summary.txt
    for summary in &report.receipts.summaries {
        for claim in &summary.claimed_axioms {
            insert(
                &mut by_name,
                &mut conflicts,
                claim.name.clone(),
                claim.axioms.clone(),
                summary.file.clone(),
            );
        }
    }

    // 2. AXIOM_AUDIT.json
    for path in find_axiom_audits(root) {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let rel = util::relative(root, &path).unwrap_or_else(|| path.display().to_string());
        for key in [
            "general_declarations_exact_axioms",
            "small_native_regressions_exact_axioms",
            "declarations_exact_axioms",
        ] {
            let Some(map) = value.get(key).and_then(|m| m.as_object()) else {
                continue;
            };
            for (name, axioms) in map {
                let Some(list) = axioms.as_array() else {
                    continue;
                };
                let list: Vec<String> = list
                    .iter()
                    .filter_map(|a| a.as_str().map(str::to_string))
                    .collect();
                insert(
                    &mut by_name,
                    &mut conflicts,
                    name.clone(),
                    list,
                    format!("{rel}#{key}"),
                );
            }
        }
    }

    Claims { by_name, conflicts }
}

fn insert(
    by_name: &mut BTreeMap<String, PublishedClaim>,
    conflicts: &mut Vec<ClaimConflict>,
    name: String,
    axioms: Vec<String>,
    source: String,
) {
    match by_name.get(&name) {
        None => {
            by_name.insert(name, PublishedClaim { axioms, source });
        }
        Some(existing) => {
            if existing.axioms != axioms {
                conflicts.push(ClaimConflict {
                    name,
                    first: (existing.axioms.clone(), existing.source.clone()),
                    second: (axioms, source),
                });
            }
        }
    }
}

fn find_axiom_audits(root: &Path) -> Vec<std::path::PathBuf> {
    WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(util::keep_entry)
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.file_name() == "AXIOM_AUDIT.json")
        .map(|e| e.path().to_path_buf())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_source_wins_and_conflicts_are_recorded() {
        let mut by_name = BTreeMap::new();
        let mut conflicts = Vec::new();
        insert(
            &mut by_name,
            &mut conflicts,
            "A".into(),
            vec!["propext".into()],
            "receipts/x".into(),
        );
        insert(
            &mut by_name,
            &mut conflicts,
            "A".into(),
            vec!["propext".into()],
            "data/AXIOM_AUDIT.json".into(),
        );
        assert!(conflicts.is_empty(), "identical claims are not a conflict");

        insert(
            &mut by_name,
            &mut conflicts,
            "A".into(),
            vec!["sorryAx".into()],
            "data/AXIOM_AUDIT.json".into(),
        );
        assert_eq!(conflicts.len(), 1);
        assert_eq!(by_name["A"].axioms, vec!["propext".to_string()]);
    }
}
