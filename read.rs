use std::fs;

use crate::output::{format_size, print_with_notices, MAX_BYTES, MAX_LINES};
use crate::policy::{Access, Policy};
use crate::Res;

pub fn run(policy: &Policy, raw: &str, offset: Option<usize>, limit: Option<usize>, line_numbers: bool) -> Res<bool> {
    let path = policy.resolve(raw)?;
    policy.check(raw, &path, Access::Read)?;
    let meta = fs::metadata(&path).map_err(|e| format!("{raw}: {e}"))?;
    if meta.is_dir() {
        return Err(format!("{raw}: is a directory (use `fs find '*' {raw}` to list it)"));
    }
    let bytes = fs::read(&path).map_err(|e| format!("{raw}: {e}"))?;
    if bytes[..bytes.len().min(8192)].contains(&0) {
        return Err(format!("{raw}: binary file ({}), not shown", format_size(bytes.len())));
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<&str> = text.split('\n').collect();
    if text.is_empty() || text.ends_with('\n') {
        lines.pop();
    }
    let total = lines.len();
    let start = offset.unwrap_or(1).max(1) - 1;
    if total == 0 {
        return Ok(true);
    }
    if start >= total {
        return Err(format!("{raw}: offset {} is beyond end of file ({total} lines total)", start + 1));
    }
    let end = limit.map_or(total, |l| (start + l).min(total));

    let mut out = String::new();
    let mut shown = 0;
    let mut by_bytes = false;
    for (i, line) in lines[start..end].iter().enumerate() {
        let line = if line_numbers { format!("{:>6}\t{line}", start + i + 1) } else { line.to_string() };
        if shown == MAX_LINES {
            break;
        }
        if out.len() + line.len() + 1 > MAX_BYTES {
            if shown == 0 {
                // A single enormous line: show its head rather than nothing.
                let mut cut = MAX_BYTES;
                while !line.is_char_boundary(cut) {
                    cut -= 1;
                }
                out.push_str(&line[..cut]);
                out.push('\n');
                let next = start + 2;
                let notice = format!(
                    "Line {} is {}, truncated to {}. Use --offset {next} to continue",
                    start + 1,
                    format_size(line.len()),
                    format_size(MAX_BYTES)
                );
                print_with_notices(out.trim_end_matches('\n'), &[notice]);
                return Ok(true);
            }
            by_bytes = true;
            break;
        }
        out.push_str(&line);
        out.push('\n');
        shown += 1;
    }

    let last = start + shown;
    let mut notices = Vec::new();
    if last < end {
        let why = if by_bytes { format!(" ({} limit)", format_size(MAX_BYTES)) } else { String::new() };
        notices.push(format!("Showing lines {}-{last} of {total}{why}. Use --offset {} to continue", start + 1, last + 1));
    } else if end < total {
        notices.push(format!("{} more lines in file. Use --offset {} to continue", total - end, end + 1));
    }
    if notices.is_empty() {
        print!("{out}");
    } else {
        print_with_notices(out.trim_end_matches('\n'), &notices);
    }
    Ok(true)
}
