use std::fs;

use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

use crate::policy::{Access, Policy};
use crate::Res;

fn normalize_char(c: char) -> char {
    match c {
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => '\'',
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' => '"',
        '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
        '\u{00A0}' | '\u{2002}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}' => ' ',
        c => c,
    }
}

/// Normalize only the documented fuzzy differences. Keep a source span for
/// every normalized character so replacement preserves all unmatched bytes.
fn normalize_fuzzy_with_spans(text: &str) -> (String, Vec<Span>) {
    let mut normalized = String::new();
    let mut spans = Vec::new();
    let mut original_start = 0;
    let mut lines = text.split('\n').peekable();

    while let Some(line) = lines.next() {
        let normalized_line_start = normalized.len();
        for (offset, grapheme) in line.grapheme_indices(true) {
            let original_end = original_start + offset + grapheme.len();
            for normalized_char in grapheme.nfkc().map(normalize_char) {
                let normalized_start = normalized.len();
                normalized.push(normalized_char);
                spans.push(Span {
                    normalized_start,
                    normalized_end: normalized.len(),
                    original_start: original_start + offset,
                    original_end,
                });
            }
        }
        let trimmed_len = normalized[normalized_line_start..].trim_end().len();
        normalized.truncate(normalized_line_start + trimmed_len);
        while spans
            .last()
            .is_some_and(|span| span.normalized_end > normalized.len())
        {
            spans.pop();
        }

        if lines.peek().is_some() {
            let normalized_start = normalized.len();
            normalized.push('\n');
            spans.push(Span {
                normalized_start,
                normalized_end: normalized.len(),
                original_start: original_start + line.len(),
                original_end: original_start + line.len() + 1,
            });
            original_start += line.len() + 1;
        }
    }
    (normalized, spans)
}

fn normalize_line(text: &str) -> String {
    text.graphemes(true)
        .flat_map(|grapheme| grapheme.nfkc())
        .map(normalize_char)
        .collect::<String>()
        .trim_end()
        .to_string()
}

fn normalize_fuzzy(text: &str) -> String {
    text.split('\n').map(normalize_line).collect::<Vec<_>>().join("\n")
}

struct Span {
    normalized_start: usize,
    normalized_end: usize,
    original_start: usize,
    original_end: usize,
}

fn original_match_span(spans: &[Span], start: usize, end: usize) -> Option<(usize, usize)> {
    let first = spans.iter().find(|span| span.normalized_start == start)?;
    let last = spans.iter().find(|span| span.normalized_end == end)?;
    Some((first.original_start, last.original_end))
}

pub fn run(policy: &Policy, raw: &str, old: &str, new: &str) -> Res<bool> {
    let path = policy.resolve(raw)?;
    policy.check(raw, &path, Access::Read)?;
    policy.check(raw, &path, Access::Write)?;
    let bytes = fs::read(&path).map_err(|e| format!("{raw}: {e}"))?;
    if bytes[..bytes.len().min(8192)].contains(&0) {
        return Err(format!("{raw}: binary file, cannot edit"));
    }
    let raw_text = String::from_utf8(bytes).map_err(|_| format!("{raw}: not valid UTF-8, cannot edit"))?;

    // Match against BOM-less, LF-only content; restore both when writing.
    let (bom, text) = match raw_text.strip_prefix('\u{FEFF}') {
        Some(rest) => ("\u{FEFF}", rest),
        None => ("", raw_text.as_str()),
    };
    let content = text.replace("\r\n", "\n").replace('\r', "\n");
    let old = old.replace("\r\n", "\n").replace('\r', "\n");
    let new = new.replace("\r\n", "\n").replace('\r', "\n");
    if old.is_empty() {
        return Err("--old must not be empty".into());
    }

    let (fuzzy_content, spans) = normalize_fuzzy_with_spans(&content);
    let fuzzy_old = normalize_fuzzy(&old);
    // An exact match wins: only count fuzzy matches when --old does not
    // appear exactly, so normalization can never make a unique exact match
    // look ambiguous.
    let exact_count = content.matches(&old).count();
    let count = if exact_count > 0 || fuzzy_old.is_empty() { exact_count } else { fuzzy_content.matches(&fuzzy_old).count() };
    if count > 1 {
        return Err(format!("found {count} occurrences of --old in {raw}. Add more context to make it unique"));
    }

    // The match region in content (LF-only) coordinates.
    let (result, region) = if let Some(start) = content.find(&old) {
        (format!("{}{new}{}", &content[..start], &content[start + old.len()..]), (start, start + old.len()))
    } else if let Some(start) = fuzzy_content.find(&fuzzy_old).filter(|_| !fuzzy_old.is_empty()) {
        let end = start + fuzzy_old.len();
        let (original_start, original_end) = original_match_span(&spans, start, end)
            .ok_or_else(|| format!("could not map --old back to {raw}"))?;
        (format!("{}{new}{}", &content[..original_start], &content[original_end..]), (original_start, original_end))
    } else {
        return Err(format!("could not find --old in {raw}. It must match exactly, including whitespace and newlines"));
    };

    if result == content {
        return Err(format!("no changes made to {raw}: the replacement produced identical content"));
    }
    let result = restore_endings(
        &result,
        &line_endings(text),
        region,
        new.len(),
        content[..region.0].matches('\n').count(),
        content[..region.1].matches('\n').count(),
    );
    fs::write(&path, format!("{bom}{result}")).map_err(|e| format!("{raw}: {e}"))?;
    Ok(true)
}

/// The terminator of each line, in order, plus a final "" for a last line
/// without one: "\n", "\r\n", "\r" or "".
fn line_endings(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut endings = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' if bytes.get(i + 1) == Some(&b'\n') => {
                endings.push("\r\n");
                i += 2;
            }
            b'\r' => {
                endings.push("\r");
                i += 1;
            }
            b'\n' => {
                endings.push("\n");
                i += 1;
            }
            _ => i += 1,
        }
    }
    endings.push("");
    endings
}

/// Put the original terminators back into LF-only `result`, where the
/// replacement spans content bytes `region` and occupies `region.0
/// ..region.0 + new_len`. A terminator that survives from the content keeps
/// its own original ending, so untouched lines keep their exact bytes; a
/// terminator introduced by the replacement takes the ending of the first
/// line the match touched.
fn restore_endings(
    result: &str,
    endings: &[&str],
    region: (usize, usize),
    new_len: usize,
    first_region_line: usize,
    suffix_first_line: usize,
) -> String {
    let region_end_in_result = region.0 + new_len;
    // The last line of a file without a trailing newline has no ending of its
    // own; lines inserted there take the previous line's ending instead.
    let inserted = match endings[first_region_line] {
        "" => first_region_line.checked_sub(1).map_or("\n", |i| endings[i]),
        ending => ending,
    };
    let mut out = String::with_capacity(result.len() + 16);
    let mut last = 0;
    let (mut prefix_line, mut suffix_line) = (0usize, suffix_first_line);
    for (pos, _) in result.match_indices('\n') {
        out.push_str(&result[last..pos]);
        if pos < region.0 {
            out.push_str(endings[prefix_line]);
            prefix_line += 1;
        } else if pos < region_end_in_result {
            out.push_str(inserted);
        } else {
            out.push_str(endings[suffix_line]);
            suffix_line += 1;
        }
        last = pos + 1;
    }
    out.push_str(&result[last..]);
    out
}
