//! Output limits and formatting shared by commands, matching pi's tools.

pub const MAX_LINES: usize = 2000;
pub const MAX_BYTES: usize = 50 * 1024;
pub const MAX_LINE_CHARS: usize = 500;

pub fn format_size(bytes: usize) -> String {
    match bytes {
        b if b < 1024 => format!("{b}B"),
        b if b < 1024 * 1024 => format!("{:.1}KB", b as f64 / 1024.0),
        b => format!("{:.1}MB", b as f64 / (1024.0 * 1024.0)),
    }
}

/// Keep whole lines from the front of `lines` until MAX_BYTES is reached.
/// Returns the kept text and whether anything was dropped.
pub fn truncate_bytes(lines: &[String]) -> (String, bool) {
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        if out.len() + line.len() + usize::from(i > 0) > MAX_BYTES {
            return (out, true);
        }
        if i > 0 {
            out.push('\n');
        }
        out.push_str(line);
    }
    (out, false)
}

pub fn print_with_notices(body: &str, notices: &[String]) {
    print!("{body}");
    if !notices.is_empty() {
        print!("\n\n[{}]", notices.join(". "));
    }
    println!();
}
