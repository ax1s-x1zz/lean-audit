//! Manifest discovery and SHA-256 verification.
//!
//! TEAM42 repositories publish three different manifest shapes:
//!
//! - `SOURCE_MANIFEST.json` with a `files: [{path, bytes, sha256}]` array
//!   (`site-percolation-square-lattice-lean`)
//! - `SOURCE_MANIFEST.json` with a `source_files_sha256: {path: sha256}` map
//!   (`rubik-stm-entropy-lean`)
//! - `*.sha256` / `SHA256SUMS` in `sha256sum` format, with paths relative to the
//!   manifest's own directory
//! - `data/AXIOM_AUDIT.json`, which binds Lean source hashes *and* the recorded
//!   axiom lists
//!
//! All four are handled here. Nothing is inferred: an entry that cannot be
//! resolved is reported as `missing`, never silently skipped.

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::hash;
use crate::model::*;
use crate::util;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    SourceManifest,
    Sha256Sum,
    AxiomAudit,
    ManuscriptBinding,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::SourceManifest => "source_manifest",
            Kind::Sha256Sum => "sha256sum",
            Kind::AxiomAudit => "axiom_audit",
            Kind::ManuscriptBinding => "manuscript_binding",
        }
    }
}

#[derive(Debug, Clone)]
struct Entry {
    path: String,
    sha256: String,
    bytes: Option<u64>,
}

struct Manifest {
    rel: String,
    kind: Kind,
    /// Directory that entry paths are resolved against.
    base: PathBuf,
    entries: Vec<Entry>,
    axiom_claims: Option<usize>,
    note: Option<String>,
}

/// Find every manifest under `root` and, when `verify`, recompute its hashes.
pub fn discover_and_verify(root: &Path, verify: bool) -> Vec<ManifestInfo> {
    let mut out = Vec::new();
    for manifest in discover(root) {
        out.push(verify_one(&manifest, root, verify));
    }
    out.sort_by(|a, b| a.file.cmp(&b.file));
    out
}

fn discover(root: &Path) -> Vec<Manifest> {
    let mut found: Vec<Manifest> = Vec::new();

    let walker = WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(util::keep_entry);

    for entry in walker.filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(rel) = util::relative(root, path) else {
            continue;
        };

        let kind = match name {
            "SOURCE_MANIFEST.json" => Kind::SourceManifest,
            "AXIOM_AUDIT.json" => Kind::AxiomAudit,
            "MANUSCRIPT_BINDING.json" => Kind::ManuscriptBinding,
            "SHA256SUMS" => Kind::Sha256Sum,
            _ if name.ends_with(".sha256") => Kind::Sha256Sum,
            _ => continue,
        };

        let base = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.to_path_buf());

        if let Some(m) = parse(path, rel, kind, base) {
            found.push(m);
        }
    }

    found
}

fn parse(path: &Path, rel: String, kind: Kind, base: PathBuf) -> Option<Manifest> {
    let mut manifest = Manifest {
        rel,
        kind,
        base,
        entries: Vec::new(),
        axiom_claims: None,
        note: None,
    };

    match kind {
        Kind::Sha256Sum => {
            let text = std::fs::read_to_string(path).ok()?;
            manifest.entries = parse_sha256sum(&text);
        }
        _ => {
            let text = std::fs::read_to_string(path).ok()?;
            let value: serde_json::Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(e) => {
                    manifest.note = Some(format!("json parse error: {e}"));
                    return Some(manifest);
                }
            };
            manifest.entries = json_hash_entries(&value);
            if manifest.entries.is_empty() {
                manifest.note = Some("no recognised hash table".to_string());
            }
            if kind == Kind::AxiomAudit {
                manifest.axiom_claims = json_axiom_claim_count(&value);
                if manifest.axiom_claims.is_some() {
                    manifest.note =
                        Some("also binds recorded axiom lists; re-computation is M3".to_string());
                }
            }
        }
    }

    Some(manifest)
}

/// `sha256sum` format: `<64 hex><space><path>`, with an optional `*` before the
/// path for binary mode. Paths run to the end of the line, so names may contain
/// spaces.
fn parse_sha256sum(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((digest, rest)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let digest = digest.trim();
        if !hash::is_sha256(digest) {
            continue;
        }
        let path = rest.trim_start().trim_start_matches('*').trim();
        if path.is_empty() {
            continue;
        }
        out.push(Entry {
            path: path.to_string(),
            sha256: digest.to_ascii_lowercase(),
            bytes: None,
        });
    }
    out
}

/// Recognise the hash tables TEAM42 uses.
fn json_hash_entries(value: &serde_json::Value) -> Vec<Entry> {
    // Shape A: {"files": [{"path", "bytes", "sha256"}]}
    if let Some(files) = value.get("files").and_then(|f| f.as_array()) {
        return files
            .iter()
            .filter_map(|f| {
                let path = f.get("path").and_then(|p| p.as_str())?.to_string();
                let sha256 = f
                    .get("sha256")
                    .or_else(|| f.get("hash"))
                    .and_then(|h| h.as_str())?
                    .to_ascii_lowercase();
                Some(Entry {
                    path,
                    sha256,
                    bytes: f.get("bytes").and_then(|b| b.as_u64()),
                })
            })
            .collect();
    }

    // Shape B: {"<table>": {"path": "sha256"}}
    for key in [
        "source_files_sha256",
        "Lean_source_hashes",
        "hashes",
        "sha256",
    ] {
        if let Some(map) = value.get(key).and_then(|m| m.as_object()) {
            return map
                .iter()
                .filter_map(|(path, digest)| {
                    let digest = digest.as_str()?.to_ascii_lowercase();
                    Some(Entry {
                        path: path.clone(),
                        sha256: digest,
                        bytes: None,
                    })
                })
                .collect();
        }
    }

    Vec::new()
}

fn json_axiom_claim_count(value: &serde_json::Value) -> Option<usize> {
    let mut total = 0usize;
    let mut found = false;
    for key in [
        "general_declarations_exact_axioms",
        "small_native_regressions_exact_axioms",
        "declarations_exact_axioms",
    ] {
        if let Some(map) = value.get(key).and_then(|m| m.as_object()) {
            total += map.len();
            found = true;
        }
    }
    found.then_some(total)
}

fn verify_one(m: &Manifest, root: &Path, verify: bool) -> ManifestInfo {
    let mut info = ManifestInfo {
        file: m.rel.clone(),
        kind: m.kind.as_str().to_string(),
        entries: Some(m.entries.len()),
        checked: None,
        matched: None,
        mismatched: Vec::new(),
        missing: Vec::new(),
        axiom_claims: m.axiom_claims,
        status: status::PARSED_ONLY.to_string(),
        note: m.note.clone(),
    };

    if !verify {
        return info;
    }
    if m.entries.is_empty() {
        info.status = if m.note.is_some() {
            status::UNSUPPORTED.to_string()
        } else {
            status::PARSED_ONLY.to_string()
        };
        return info;
    }

    let mut checked = 0usize;
    let mut matched = 0usize;
    let mut from_root = 0usize;

    for entry in &m.entries {
        // Two conventions coexist: `sha256sum` files are usually relative to their
        // own directory, but `checker/sharded/PATCH.sha256` and the JSON manifests
        // use repository-root-relative paths. A file only exists in one place, so
        // resolving against both is not a guess; which base was used is recorded.
        let self_relative = m.base.join(&entry.path);
        let root_relative = root.join(&entry.path);
        let resolved = if self_relative.is_file() {
            Some(self_relative)
        } else if root_relative.is_file() {
            from_root += 1;
            Some(root_relative)
        } else {
            None
        };

        let Some(resolved) = resolved else {
            info.missing.push(entry.path.clone());
            continue;
        };

        checked += 1;
        let actual = match hash::sha256_file(&resolved) {
            Ok(h) => h,
            Err(e) => {
                info.mismatched.push(HashMismatch {
                    path: entry.path.clone(),
                    expected: entry.sha256.clone(),
                    actual: format!("<error: {e}>"),
                    bytes_expected: entry.bytes,
                    bytes_actual: None,
                });
                continue;
            }
        };
        let actual_bytes = std::fs::metadata(&resolved).ok().map(|md| md.len());
        let bytes_differ = entry.bytes.is_some_and(|b| Some(b) != actual_bytes);

        if actual == entry.sha256 && !bytes_differ {
            matched += 1;
        } else {
            info.mismatched.push(HashMismatch {
                path: entry.path.clone(),
                expected: entry.sha256.clone(),
                actual,
                bytes_expected: entry.bytes,
                bytes_actual: actual_bytes,
            });
        }
    }

    info.checked = Some(checked);
    info.matched = Some(matched);
    if from_root > 0 {
        let extra = format!("{from_root} entries resolved relative to the repository root");
        info.note = Some(match info.note.take() {
            Some(existing) => format!("{existing}; {extra}"),
            None => extra,
        });
    }
    info.status = if info.mismatched.is_empty() && info.missing.is_empty() {
        status::OK.to_string()
    } else {
        status::MISMATCH.to_string()
    };
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sha256sum_lines() {
        let text = "# comment\n8e40f48b484af14f48fbcde465209954e3616a201497eeb4cb60a309fbef5821  enum.in\ne07e36a978f07bc1c57f1089bfbcf6e9327f7db76c04066c7123574637387484 *laws.bin\nnot a hash  x\n\n";
        let entries = parse_sha256sum(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "enum.in");
        assert_eq!(entries[1].path, "laws.bin");
    }

    #[test]
    fn sha256sum_paths_may_contain_spaces() {
        let text = "8e40f48b484af14f48fbcde465209954e3616a201497eeb4cb60a309fbef5821  a file with spaces.bin\n";
        let entries = parse_sha256sum(text);
        assert_eq!(entries[0].path, "a file with spaces.bin");
    }

    #[test]
    fn parses_files_array_shape() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"schema":1,"files":[{"path":".gitignore","bytes":436,"sha256":"ff88848fb619c5cfe8f76e4eac07d0c4518bde42695851a2190527bdc6a9f6d6"}]}"#,
        )
        .unwrap();
        let e = json_hash_entries(&v);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].path, ".gitignore");
        assert_eq!(e[0].bytes, Some(436));
    }

    #[test]
    fn parses_source_files_sha256_map_shape() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"schema":"rubik-public-scope-v1","source_files_sha256":{"AxiomCheck.lean":"379697ad253ae2b60a9d200f6be3a12882652cbc396eaa2269d9919a5411247e"}}"#,
        )
        .unwrap();
        let e = json_hash_entries(&v);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].path, "AxiomCheck.lean");
        assert_eq!(e[0].bytes, None);
    }

    #[test]
    fn counts_axiom_claims_from_both_tables() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"general_declarations_exact_axioms":{"A":["propext"],"B":["propext"]},"small_native_regressions_exact_axioms":{"C":["x"]}}"#,
        )
        .unwrap();
        assert_eq!(json_axiom_claim_count(&v), Some(3));
    }

    #[test]
    fn unknown_json_shape_is_not_guessed() {
        let v: serde_json::Value = serde_json::from_str(r#"{"unrelated":true}"#).unwrap();
        assert!(json_hash_entries(&v).is_empty());
        assert_eq!(json_axiom_claim_count(&v), None);
    }
}
