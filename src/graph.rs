//! Import graph metrics over a repository's own Lean modules.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default)]
pub struct GraphMetrics {
    pub internal_edges: usize,
    pub external_imports: Vec<String>,
    pub roots: Vec<String>,
    pub max_depth: Option<usize>,
    /// Each cycle as a path that returns to its first node.
    pub cycles: Vec<Vec<String>>,
    /// Module -> its internal imports (sorted, deduplicated).
    pub adjacency: BTreeMap<String, Vec<String>>,
}

/// `modules` is a list of `(module_name, imports)`. Imports that do not name a
/// module in the repository are reported as external.
pub fn analyze(modules: &[(String, Vec<String>)]) -> GraphMetrics {
    let names: BTreeSet<&str> = modules.iter().map(|(n, _)| n.as_str()).collect();
    let mut adjacency: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut external: BTreeSet<String> = BTreeSet::new();
    let mut incoming: BTreeMap<&str, usize> =
        modules.iter().map(|(n, _)| (n.as_str(), 0)).collect();
    let mut internal_edges = 0usize;

    for (name, imports) in modules {
        let mut list: Vec<String> = Vec::new();
        for imp in imports {
            if names.contains(imp.as_str()) {
                list.push(imp.clone());
                *incoming.get_mut(imp.as_str()).expect("module in map") += 1;
                internal_edges += 1;
            } else {
                external.insert(imp.clone());
            }
        }
        list.sort();
        list.dedup();
        adjacency.insert(name.clone(), list);
    }

    let mut roots: Vec<String> = incoming
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(name, _)| name.to_string())
        .collect();
    roots.sort();

    let mut dfs = Dfs {
        adjacency: &adjacency,
        color: BTreeMap::new(),
        depth: BTreeMap::new(),
        stack: Vec::new(),
        cycles: Vec::new(),
    };
    let mut max_depth = 0usize;
    for name in adjacency.keys() {
        let d = dfs.visit(name);
        max_depth = max_depth.max(d);
    }

    GraphMetrics {
        internal_edges,
        external_imports: external.into_iter().collect(),
        roots,
        max_depth: if adjacency.is_empty() {
            None
        } else {
            Some(max_depth)
        },
        cycles: dfs.cycles,
        adjacency,
    }
}

struct Dfs<'a> {
    adjacency: &'a BTreeMap<String, Vec<String>>,
    /// 0 = unvisited, 1 = on stack, 2 = finished.
    color: BTreeMap<String, u8>,
    depth: BTreeMap<String, usize>,
    stack: Vec<String>,
    cycles: Vec<Vec<String>>,
}

impl Dfs<'_> {
    fn visit(&mut self, node: &str) -> usize {
        match self.color.get(node).copied().unwrap_or(0) {
            2 => return *self.depth.get(node).unwrap_or(&0),
            1 => {
                if let Some(pos) = self.stack.iter().position(|n| n == node) {
                    let mut cycle: Vec<String> = self.stack[pos..].to_vec();
                    cycle.push(node.to_string());
                    if !self.cycles.contains(&cycle) {
                        self.cycles.push(cycle);
                    }
                }
                return 0;
            }
            _ => {}
        }

        self.color.insert(node.to_string(), 1);
        self.stack.push(node.to_string());

        let mut best = 0usize;
        if let Some(children) = self.adjacency.get(node) {
            for child in children {
                let d = self.visit(child);
                best = best.max(d + 1);
            }
        }

        self.stack.pop();
        self.color.insert(node.to_string(), 2);
        self.depth.insert(node.to_string(), best);
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pairs: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
        pairs
            .iter()
            .map(|(n, is)| {
                (
                    n.to_string(),
                    is.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                )
            })
            .collect()
    }

    #[test]
    fn counts_edges_and_external_imports() {
        let modules = m(&[("Main", &["Lib", "Mathlib"]), ("Lib", &["Mathlib"])]);
        let g = analyze(&modules);
        assert_eq!(g.internal_edges, 1);
        assert_eq!(g.external_imports, vec!["Mathlib".to_string()]);
    }

    #[test]
    fn finds_roots() {
        let modules = m(&[("Main", &["Lib"]), ("Lib", &["Base"]), ("Base", &[])]);
        let g = analyze(&modules);
        assert_eq!(g.roots, vec!["Main".to_string()]);
    }

    #[test]
    fn computes_max_depth() {
        let modules = m(&[("A", &["B"]), ("B", &["C"]), ("C", &[])]);
        let g = analyze(&modules);
        assert_eq!(g.max_depth, Some(2));
    }

    #[test]
    fn deduplicates_repeated_imports() {
        let modules = m(&[("A", &["B", "B"]), ("B", &[])]);
        let g = analyze(&modules);
        assert_eq!(
            g.internal_edges, 2,
            "raw edge count still counts each occurrence"
        );
        assert_eq!(g.adjacency["A"], vec!["B".to_string()]);
    }

    #[test]
    fn detects_cycles_without_hanging() {
        let modules = m(&[("A", &["B"]), ("B", &["A"])]);
        let g = analyze(&modules);
        assert!(!g.cycles.is_empty());
        assert!(g.roots.is_empty());
    }

    #[test]
    fn empty_repository_is_handled() {
        let g = analyze(&[]);
        assert_eq!(g.max_depth, None);
        assert_eq!(g.internal_edges, 0);
    }
}
