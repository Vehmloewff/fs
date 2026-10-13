//! Directory walking and result paths shared by grep and find.

use std::path::Path;
use std::sync::Arc;

use ignore::WalkBuilder;

use crate::policy::{Access, Policy};

/// Build the path to print for a walked entry: the user's path argument joined
/// with the entry's path relative to the search root.
pub fn display_path(raw: &str, search_root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(search_root).unwrap_or(path).to_string_lossy().replace('\\', "/");
    let raw = raw.trim_end_matches('/');
    match (raw, rel.is_empty()) {
        (_, true) => raw.to_string(),
        ("." | "", false) => rel,
        _ => format!("{raw}/{rel}"),
    }
}

/// Walk a directory the same way ripgrep does with --hidden: .gitignore is
/// respected, hidden files are included, and .git is skipped. Entries denied by
/// policy are pruned.
pub fn walker(policy: &Arc<Policy>, root: &Path, access: Access) -> WalkBuilder {
    let mut builder = WalkBuilder::new(root);
    let policy = Arc::clone(policy);
    builder
        .hidden(false)
        .require_git(false)
        .sort_by_file_name(|a, b| a.cmp(b))
        .filter_entry(move |entry| entry.file_name() != ".git" && !policy.is_denied(entry.path(), access));
    builder
}
