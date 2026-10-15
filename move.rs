use std::fs;
use std::io;

use crate::policy::{Access, Policy};
use crate::Res;

pub fn run(policy: &Policy, raw_from: &str, raw_to: &str, force: bool) -> Res<bool> {
    let from = policy.resolve(raw_from)?;
    let to = policy.resolve(raw_to)?;
    let from_meta = fs::symlink_metadata(&from).map_err(|e| format!("{raw_from}: {e}"))?;
    if to != from && to.starts_with(&from) && from_meta.is_dir() {
        return Err(format!("{raw_to}: cannot move a directory into itself"));
    }
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
        if err.kind() != io::ErrorKind::CrossesDevices {
            return Err(format!("{raw_from} -> {raw_to}: {err}"));
        }

        // Cross-device fallback applies to regular files and symlinks. Never
        // dereference a symlink or follow a destination symlink while copying.
        if !from_meta.file_type().is_symlink() && !from_meta.is_file() {
            return Err(format!("{raw_from} -> {raw_to}: {err}"));
        }
        // Copy next to the destination, then rename over it, so a failed copy
        // never destroys an existing file that --force would replace.
        let name = to.file_name().unwrap_or_default().to_string_lossy();
        let tmp = to.with_file_name(format!(".{name}.fs-move-{}", std::process::id()));
        let copied = if from_meta.file_type().is_symlink() {
            recreate_symlink(&from, &tmp)
        } else {
            fs::copy(&from, &tmp).map(|_| ())
        };
        if let Err(e) = copied.and_then(|_| fs::rename(&tmp, &to)) {
            let _ = fs::remove_file(&tmp);
            return Err(format!("{raw_to}: {e}"));
        }
        fs::remove_file(&from).map_err(|e| format!("{raw_from}: {e}"))?;
    }
    Ok(true)
}

fn recreate_symlink(from: &std::path::Path, to: &std::path::Path) -> io::Result<()> {
    let target = fs::read_link(from)?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, to)
    }
    #[cfg(windows)]
    {
        if fs::metadata(from).is_ok_and(|meta| meta.is_dir()) {
            std::os::windows::fs::symlink_dir(target, to)
        } else {
            std::os::windows::fs::symlink_file(target, to)
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (target, to);
        Err(io::Error::new(io::ErrorKind::Unsupported, "symlink moves are unsupported on this platform"))
    }
}
