//! Shared filesystem helpers.

use std::path::Path;

use walkdir::DirEntry;

/// Directories that never belong to a Lean project's own sources.
pub const SKIP_DIRS: [&str; 6] = [
    "lake-packages",
    ".lake",
    "target",
    "build",
    ".git",
    "node_modules",
];

/// WalkDir filter: prune the directories above.
pub fn keep_entry(e: &DirEntry) -> bool {
    !(e.file_type().is_dir()
        && e.file_name()
            .to_str()
            .is_some_and(|n| SKIP_DIRS.contains(&n)))
}

/// Path relative to `root`, using forward slashes, or `None` if not under `root`.
pub fn relative(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    Some(
        rel.components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/"),
    )
}
