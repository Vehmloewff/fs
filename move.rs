use std::fs;

use crate::policy::{Access, Policy};
use crate::Res;

pub fn run(policy: &Policy, raw_from: &str, raw_to: &str, force: bool) -> Res<bool> {
    let from = policy.resolve(raw_from)?;
    let to = policy.resolve(raw_to)?;
    fs::symlink_metadata(&from).map_err(|e| format!("{raw_from}: {e}"))?;
    // Moving exposes the contents under a new name, so the source must be readable too.
    policy.check_tree(raw_from, &from, Access::Read)?;
    policy.check_tree(raw_from, &from, Access::Write)?;
    policy.check_tree(raw_to, &to, Access::Write)?;
    if let Ok(meta) = fs::symlink_metadata(&to) {
        if meta.is_dir() {
            return Err(format!("{raw_to}: already exists and is a directory"));
        }
        if !force {
            return Err(format!("{raw_to}: already exists (use --force to overwrite)"));
        }
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    if let Err(err) = fs::rename(&from, &to) {
        // rename fails across file systems; fall back to copy + remove for files.
        if from.is_file() {
            fs::copy(&from, &to).map_err(|e| format!("{raw_to}: {e}"))?;
            fs::remove_file(&from).map_err(|e| format!("{raw_from}: {e}"))?;
        } else {
            return Err(format!("{raw_from} -> {raw_to}: {err}"));
        }
    }
    Ok(true)
}
