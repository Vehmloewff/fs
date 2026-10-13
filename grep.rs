use std::io;
use std::sync::Arc;

use grep::regex::RegexMatcherBuilder;
use grep::searcher::{BinaryDetection, Searcher, SearcherBuilder, Sink, SinkContext, SinkMatch};
use ignore::overrides::OverrideBuilder;

use crate::output::{format_size, print_with_notices, truncate_bytes, MAX_BYTES, MAX_LINE_CHARS};
use crate::policy::{Access, Policy};
use crate::walk::{display_path, walker};
use crate::Res;

struct GrepSink<'a> {
    display: &'a str,
    out: &'a mut Vec<String>,
    matches: &'a mut usize,
    limit: usize,
    long_lines: &'a mut bool,
}

impl GrepSink<'_> {
    fn push(&mut self, sep: char, line_number: Option<u64>, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);
        let text = text.trim_end_matches(['\n', '\r']);
        let text = match text.char_indices().nth(MAX_LINE_CHARS) {
            Some((cut, _)) => {
                *self.long_lines = true;
                format!("{}... [truncated]", &text[..cut])
            }
            None => text.to_string(),
        };
        let n = line_number.unwrap_or(0);
        self.out.push(format!("{}{sep}{n}{sep} {text}", self.display));
    }
}

impl Sink for GrepSink<'_> {
    type Error = io::Error;

    fn matched(&mut self, _: &Searcher, m: &SinkMatch<'_>) -> Result<bool, io::Error> {
        self.push(':', m.line_number(), m.bytes());
        *self.matches += 1;
        Ok(*self.matches < self.limit)
    }

    fn context(&mut self, _: &Searcher, c: &SinkContext<'_>) -> Result<bool, io::Error> {
        self.push('-', c.line_number(), c.bytes());
        Ok(true)
    }

    fn context_break(&mut self, _: &Searcher) -> Result<bool, io::Error> {
        self.out.push("--".into());
        Ok(true)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    policy: &Arc<Policy>,
    pattern: &str,
    raw: &str,
    glob: Option<String>,
    ignore_case: bool,
    literal: bool,
    context: usize,
    limit: usize,
) -> Res<bool> {
    let root = policy.resolve(raw)?;
    policy.check(raw, &root, Access::Read)?;
    if !root.exists() {
        return Err(format!("{raw}: no such file or directory"));
    }
    let matcher = RegexMatcherBuilder::new()
        .case_insensitive(ignore_case)
        .fixed_strings(literal)
        .build(pattern)
        .map_err(|e| format!("invalid pattern: {e}"))?;
    let mut searcher = SearcherBuilder::new()
        .line_number(true)
        .before_context(context)
        .after_context(context)
        .binary_detection(BinaryDetection::quit(0))
        .build();

    let mut walk = walker(policy, &root, Access::Read);
    if let Some(glob) = &glob {
        let overrides = OverrideBuilder::new(&root)
            .add(glob)
            .and_then(|b| b.build())
            .map_err(|e| format!("invalid --glob: {e}"))?;
        walk.overrides(overrides);
    }

    let limit = limit.max(1);
    let (mut out, mut matches, mut long_lines) = (Vec::new(), 0, false);
    for entry in walk.build().flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        if context > 0 && !out.is_empty() && out.last().is_some_and(|l| l != "--") {
            out.push("--".into());
        }
        let display = display_path(raw, &root, entry.path());
        let before = out.len();
        let sink = GrepSink { display: &display, out: &mut out, matches: &mut matches, limit, long_lines: &mut long_lines };
        // Unreadable files are skipped, as ripgrep does.
        let _ = searcher.search_path(&matcher, entry.path(), sink);
        if out.len() == before && out.last().is_some_and(|l| l == "--") {
            out.pop();
        }
        if matches >= limit {
            break;
        }
    }
    while out.last().is_some_and(|l| l == "--") {
        out.pop();
    }

    if matches == 0 {
        println!("No matches found");
        return Ok(false);
    }
    let (body, truncated) = truncate_bytes(&out);
    let mut notices = Vec::new();
    if matches >= limit {
        notices.push(format!("{limit} matches limit reached. Use --limit {} for more, or refine the pattern", limit * 2));
    }
    if truncated {
        notices.push(format!("{} output limit reached", format_size(MAX_BYTES)));
    }
    if long_lines {
        notices.push(format!("Some lines truncated to {MAX_LINE_CHARS} chars. Use `fs read` to see full lines"));
    }
    print_with_notices(&body, &notices);
    Ok(true)
}
