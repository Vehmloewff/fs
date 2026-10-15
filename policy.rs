//! FS_ROOT_DIR and the FS_DENIED_* globs.

use std::path::{Component, Path, PathBuf};
use std::{env, fs};

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;

use crate::walk::display_path;
use crate::Res;

pub struct Policy {
    /// Canonical FS_ROOT_DIR, if set.
    pub root: Option<PathBuf>,
    /// Directory relative paths resolve against.
    base: PathBuf,
    denied: GlobSet,
    denied_read: GlobSet,
    denied_write: GlobSet,
}

#[derive(Clone, Copy)]
pub enum Access {
    Read,
    Write,
    /// Listing names only: just FS_DENIED_FILES applies.
    List,
}

impl Policy {
    pub fn from_env() -> Res<Policy> {
        let cwd = env::current_dir().map_err(|e| format!("cannot determine current directory: {e}"))?;
        let cwd = cwd.canonicalize().unwrap_or(cwd);
        let root = match env::var("FS_ROOT_DIR") {
            Ok(dir) if !dir.is_empty() => {
                let dir = expand_tilde(&dir);
                let dir = if dir.is_absolute() { dir } else { cwd.join(dir) };
                Some(dir.canonicalize().map_err(|e| format!("FS_ROOT_DIR {}: {e}", dir.display()))?)
            }
            _ => None,
        };
        let base = match &root {
            Some(root) if !cwd.starts_with(root) => root.clone(),
            _ => cwd,
        };
        Ok(Policy {
            root,
            base,
            denied: globs_from_env("FS_DENIED_FILES")?,
            denied_read: globs_from_env("FS_DENIED_READ_FILES")?,
            denied_write: globs_from_env("FS_DENIED_WRITE_FILES")?,
        })
    }

    /// Resolve a user-supplied path to an absolute path, enforcing FS_ROOT_DIR.
    /// The parent directory is canonicalized but the final component is kept, so
    /// operations on a symlink act on the link itself.
    pub fn resolve(&self, raw: &str) -> Res<PathBuf> {
        let path = expand_tilde(raw);
        let path = normalize(&if path.is_absolute() { path } else { self.base.join(path) });
        let path = match (path.parent(), path.file_name()) {
            (Some(parent), Some(name)) => canonicalize_existing(parent).join(name),
            _ => canonicalize_existing(&path),
        };
        if let Some(root) = &self.root {
            match canonicalize_target(&path, 40) {
                Some(real) if !real.starts_with(root) => {
                    return Err(format!("{raw}: outside of FS_ROOT_DIR ({})", root.display()));
                }
                Some(_) => {}
                None => {
                    return Err(format!("{raw}: cannot verify path is inside FS_ROOT_DIR ({})", root.display()));
                }
            }
        }
        Ok(path)
    }

    pub fn is_denied(&self, path: &Path, access: Access) -> bool {
        let empty = GlobSet::empty();
        let extra = match access {
            Access::Read => &self.denied_read,
            Access::Write => &self.denied_write,
            Access::List => &empty,
        };
        if self.denied.is_empty() && extra.is_empty() {
            return false;
        }
        let deny_base = self.root.as_ref().unwrap_or(&self.base);
        let mut paths = vec![path.to_path_buf()];
        if !matches!(access, Access::List) {
            match canonicalize_target(path, 40) {
                Some(real) if real != path => paths.push(real),
                Some(_) => {}
                None if path.ancestors().any(|ancestor| {
                    fs::symlink_metadata(ancestor).is_ok_and(|meta| meta.file_type().is_symlink())
                }) => return true,
                None => {}
            }
        }
        // Check both the supplied path and its resolved target. Check ancestors
        // too, so denying a directory denies everything inside it.
        for path in paths {
            for p in path.ancestors() {
                if p == deny_base {
                    break;
                }
                let mut candidates = vec![p.to_string_lossy().replace('\\', "/")];
                if let Some(name) = p.file_name() {
                    candidates.push(name.to_string_lossy().into_owned());
                }
                if let Ok(rel) = p.strip_prefix(deny_base) {
                    candidates.push(rel.to_string_lossy().replace('\\', "/"));
                }
                if candidates.iter().any(|c| self.denied.is_match(c) || extra.is_match(c)) {
                    return true;
                }
            }
        }
        false
    }

    pub fn check(&self, raw: &str, path: &Path, access: Access) -> Res<()> {
        if self.is_denied(path, access) {
            let what = match access {
                Access::Read => "reading",
                Access::Write => "writing",
                Access::List => "listing",
            };
            return Err(format!("{raw}: {what} is denied by policy"));
        }
        Ok(())
    }

    /// Check a path and, if it is a directory, everything inside it.
    pub fn check_tree(&self, raw: &str, path: &Path, access: Access) -> Res<()> {
        self.check(raw, path, access)?;
        if fs::symlink_metadata(path).is_ok_and(|m| m.is_dir()) {
            for entry in WalkBuilder::new(path).standard_filters(false).build().flatten() {
                if self.is_denied(entry.path(), access) {
                    let inner = display_path(raw, path, entry.path());
                    return Err(format!("{raw}: contains {inner}, which is denied by policy"));
                }
            }
        }
        Ok(())
    }
}

/// Read a comma separated list from an environment variable, ignoring commas
/// inside `{a,b}` alternations.
pub fn env_list(var: &str) -> Vec<String> {
    let value = env::var(var).unwrap_or_default();
    let mut items = Vec::new();
    let (mut current, mut depth) = (String::new(), 0usize);
    for ch in value.chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' | '\n' if depth == 0 => {
                items.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(ch);
    }
    items.push(current);
    items.iter().map(|i| i.trim().to_string()).filter(|i| !i.is_empty()).collect()
}

fn globs_from_env(var: &str) -> Res<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in env_list(var) {
        let pattern = pattern.trim_end_matches('/');
        let glob = GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .map_err(|e| format!("{var}: invalid glob {pattern:?}: {e}"))?;
        builder.add(glob);
    }
    builder.build().map_err(|e| format!("{var}: {e}"))
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(home) = env::var_os("HOME") {
        if path == "~" {
            return PathBuf::from(home);
        }
        if let Some(rest) = path.strip_prefix("~/") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

/// Lexically remove `.` and `..` components.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Canonicalize the longest existing prefix of a path and append the rest.
fn canonicalize_existing(path: &Path) -> PathBuf {
    let mut existing = path;
    let mut rest = Vec::new();
    loop {
        if let Ok(real) = existing.canonicalize() {
            return rest.iter().rev().fold(real, |acc, name| acc.join(name));
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_owned());
                existing = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Resolve existing symlinks, including dangling links, then append any
/// nonexistent suffix to its canonical parent. `None` means a link cycle,
/// unreadable link target, or unresolved `..`, so callers enforcing policy
/// must fail closed.
fn canonicalize_target(path: &Path, links_left: usize) -> Option<PathBuf> {
    if let Ok(real) = path.canonicalize() {
        return Some(real);
    }
    if links_left == 0 {
        return None;
    }
    for ancestor in path.ancestors() {
        if !fs::symlink_metadata(ancestor).is_ok_and(|meta| meta.file_type().is_symlink()) {
            continue;
        }
        let target = fs::read_link(ancestor).ok()?;
        let target = if target.is_absolute() {
            target
        } else {
            ancestor.parent()?.join(target)
        };
        let suffix = path.strip_prefix(ancestor).ok()?;
        return canonicalize_target(&target.join(suffix), links_left - 1);
    }
    let unresolved = canonicalize_existing(path);
    if unresolved.components().any(|component| matches!(component, Component::ParentDir)) {
        None
    } else {
        Some(unresolved)
    }
}
