use std::fs;

use unicode_normalization::UnicodeNormalization;

use crate::policy::{Access, Policy};
use crate::Res;

/// Normalize text for fuzzy edit matching: NFKC, strip trailing whitespace
/// from each line, and map smart quotes, Unicode dashes and special spaces to
/// ASCII.
fn normalize_fuzzy(text: &str) -> String {
    text.nfkc()
        .collect::<String>()
        .split('\n')
        .map(|line| {
            line.trim_end()
                .chars()
                .map(|c| match c {
                    '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => '\'',
                    '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' => '"',
                    '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
                    '\u{00A0}' | '\u{2002}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}' => ' ',
                    c => c,
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Byte spans of each line, including its trailing newline.
fn line_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = 0;
    for (i, _) in text.match_indices('\n') {
        spans.push((start, i + 1));
        start = i + 1;
    }
    if start < text.len() {
        spans.push((start, text.len()));
    }
    spans
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
    let crlf = text.find('\n').is_some_and(|i| i > 0 && text.as_bytes()[i - 1] == b'\r');
    let content = text.replace("\r\n", "\n").replace('\r', "\n");
    let old = old.replace("\r\n", "\n");
    let new = new.replace("\r\n", "\n");
    if old.is_empty() {
        return Err("--old must not be empty".into());
    }

    let fuzzy_content = normalize_fuzzy(&content);
    let fuzzy_old = normalize_fuzzy(&old);
    // Whitespace-only text normalizes to nothing, so it can only match exactly.
    let count = if fuzzy_old.is_empty() { content.matches(&old).count() } else { fuzzy_content.matches(&fuzzy_old).count() };
    if count > 1 {
        return Err(format!("found {count} occurrences of --old in {raw}. Add more context to make it unique"));
    }

    let result = if let Some(start) = content.find(&old) {
        format!("{}{new}{}", &content[..start], &content[start + old.len()..])
    } else if let Some(start) = fuzzy_content.find(&fuzzy_old).filter(|_| !fuzzy_old.is_empty()) {
        // Rewrite only the lines the match touches from the normalized text and
        // copy every other line from the original byte for byte.
        let base_lines = line_spans(&fuzzy_content);
        let orig_lines = line_spans(&content);
        if base_lines.len() != orig_lines.len() {
            return Err(format!("could not find --old in {raw}. It must match exactly, including whitespace and newlines"));
        }
        let end = start + fuzzy_old.len();
        let line_of = |offset: usize| base_lines.iter().position(|&(s, e)| offset >= s && offset < e).unwrap_or(base_lines.len() - 1);
        let (first, last) = (line_of(start), line_of(end - 1));
        let (gs, ge) = (base_lines[first].0, base_lines[last].1);
        format!(
            "{}{}{new}{}{}",
            &content[..orig_lines[first].0],
            &fuzzy_content[gs..start],
            &fuzzy_content[end..ge],
            &content[orig_lines[last].1..]
        )
    } else {
        return Err(format!("could not find --old in {raw}. It must match exactly, including whitespace and newlines"));
    };

    if result == content {
        return Err(format!("no changes made to {raw}: the replacement produced identical content"));
    }
    let result = if crlf { result.replace('\n', "\r\n") } else { result };
    fs::write(&path, format!("{bom}{result}")).map_err(|e| format!("{raw}: {e}"))?;
    Ok(true)
}
