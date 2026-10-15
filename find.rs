use std::sync::Arc;

use globset::GlobBuilder;

use crate::output::{format_size, print_with_notices, truncate_bytes, MAX_BYTES};
use crate::policy::{Access, Policy};
use crate::walk::{display_path, walker};
use crate::Res;

pub fn run(policy: &Arc<Policy>, pattern: &str, raw: &str, limit: usize) -> Res<bool> {
    let root = policy.resolve(raw)?;
    policy.check(raw, &root, Access::List)?;
    if !root.is_dir() {
        return Err(format!("{raw}: not a directory"));
    }
    // A trailing '/' or '/.' would never match an entry's name; drop it.
    let pattern = pattern.trim_start_matches("./").trim_end_matches('/').trim_end_matches("/.");
    let matcher = GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .map_err(|e| format!("invalid pattern: {e}"))?
        .compile_matcher();
    let full_path = pattern.contains('/');

    let limit = limit.max(1);
    let mut out = Vec::new();
    for entry in walker(policy, &root, Access::List).build().flatten() {
        if entry.depth() == 0 {
            continue;
        }
        let rel = entry.path().strip_prefix(&root).unwrap_or(entry.path());
        let candidate = if full_path { rel.to_string_lossy().replace('\\', "/") } else { entry.file_name().to_string_lossy().into_owned() };
        if !matcher.is_match(&candidate) {
            continue;
        }
        let mut display = display_path(raw, &root, entry.path());
        if entry.file_type().is_some_and(|t| t.is_dir()) {
            display.push('/');
        }
        out.push(display);
        if out.len() >= limit {
            break;
        }
    }

    if out.is_empty() {
        println!("No files found matching pattern");
        return Ok(false);
    }
    let (body, truncated) = truncate_bytes(&out);
    let mut notices = Vec::new();
    if out.len() >= limit {
        notices.push(format!("{limit} results limit reached. Use --limit {} for more, or refine the pattern", limit * 2));
    }
    if truncated {
        notices.push(format!("{} output limit reached", format_size(MAX_BYTES)));
    }
    print_with_notices(&body, &notices);
    Ok(true)
}
