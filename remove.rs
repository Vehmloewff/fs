use std::fs;
use std::path::PathBuf;

use crate::policy::{Access, Policy};
use crate::Res;

pub fn run(policy: &Policy, raws: &[String], recursive: bool) -> Res<bool> {
    // Validate everything before removing anything.
    let mut targets = Vec::new();
    for raw in raws {
        let path = policy.resolve(raw)?;
        let meta = fs::symlink_metadata(&path).map_err(|e| format!("{raw}: {e}"))?;
        if meta.is_dir() && !recursive {
            return Err(format!("{raw}: is a directory (use --recursive to remove it)"));
        }
        if policy.root.as_ref() == Some(&path) || path.parent().is_none() {
            return Err(format!("{raw}: refusing to remove the root directory"));
        }
        policy.check_tree(raw, &path, Access::Write)?;
        targets.push((raw, path, meta.is_dir()));
    }
    // Skip paths an earlier target already removed, such as `dir dir/file` or
    // a repeated path, so overlapping arguments don't fail halfway through.
    let mut removed: Vec<PathBuf> = Vec::new();
    for (raw, path, is_dir) in targets {
        if removed.iter().any(|r| path.starts_with(r)) {
            continue;
        }
        let result = if is_dir { fs::remove_dir_all(&path) } else { fs::remove_file(&path) };
        result.map_err(|e| format!("{raw}: {e}"))?;
        removed.push(path);
    }
    Ok(true)
}
