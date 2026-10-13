mod edit;
mod find;
mod grep;
mod help;
mod r#move;
mod output;
mod policy;
mod read;
mod remove;
mod walk;
mod write;

use std::process::ExitCode;
use std::sync::Arc;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};

use policy::Policy;

pub type Res<T> = Result<T, String>;

#[derive(Parser)]
#[command(
    name = "fs",
    version,
    about = "File system utilities for humans and agents",
    disable_help_subcommand = true,
    after_help = "Run 'fs --help' for the full reference of every command, or 'fs <command> --help' for one."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read a text file
    #[command(after_help = "\
Prints the file as is. Output is truncated to 2000 lines or 50KB,
whichever comes first; a notice at the end then says which --offset
continues. Directories and binary files are refused.

Examples:
  fs read main.rs
  fs read main.rs --offset 120 --limit 40")]
    Read {
        /// File to read
        path: String,
        /// Line number to start reading from (1-indexed)
        #[arg(short, long, value_name = "LINE")]
        offset: Option<usize>,
        /// Maximum number of lines to read
        #[arg(short, long, value_name = "LINES")]
        limit: Option<usize>,
        /// Prefix each line with its line number
        #[arg(short = 'n', long)]
        line_numbers: bool,
    },
    /// Create or overwrite a file
    #[command(after_help = "\
Writes CONTENT, or stdin when CONTENT is omitted, replacing the whole
file. Parent directories are created as needed. Use this for new files
and full rewrites; use fs edit to change part of a file.

Examples:
  fs write notes.txt 'hello world'
  fs write main.rs <<'EOF'
  fn main() {}
  EOF")]
    Write {
        /// File to write
        path: String,
        /// Content to write (read from stdin when omitted)
        #[arg(allow_hyphen_values = true)]
        content: Option<String>,
    },
    /// Replace one exact, unique piece of text in a file
    #[command(after_help = "\
--old must match exactly one place in the file, including whitespace and
newlines; keep it short but unique. If there is no exact match, trailing
whitespace, smart quotes, Unicode dashes and special spaces are ignored.
Line endings and a leading BOM are preserved.

One edit per command: to make several changes, run fs edit once for
each. If two changes touch the same block or nearby lines, make them one
edit instead.

Examples:
  fs edit main.rs --old 'let x = 1;' --new 'let x = 2;'
  fs edit main.rs --old 'if ok {
      run();
  }' --new 'if ok {
      run_twice();
  }'")]
    Edit {
        /// File to edit
        path: String,
        /// Exact text to replace; must be unique in the file
        #[arg(long = "old", value_name = "TEXT", allow_hyphen_values = true)]
        old: String,
        /// Replacement text (may be empty to delete)
        #[arg(long = "new", value_name = "TEXT", allow_hyphen_values = true)]
        new: String,
    },
    /// Move or rename a file or directory
    #[command(
        visible_alias = "mv",
        after_help = "\
TO is the full destination path, not a directory to move into. Parent
directories are created as needed. Fails if TO exists unless --force is
given, and never overwrites a directory.

Examples:
  fs move old.rs new.rs
  fs move draft.md docs/guide.md --force"
    )]
    Move {
        /// Path to move
        from: String,
        /// Destination path
        to: String,
        /// Overwrite the destination file if it exists
        #[arg(short, long)]
        force: bool,
    },
    /// Remove files or directories
    #[command(
        visible_alias = "rm",
        after_help = "\
Directories need --recursive. Every path is checked before anything is
removed, so a bad path removes nothing.

Examples:
  fs remove notes.txt old.txt
  fs remove build --recursive"
    )]
    Remove {
        /// Paths to remove
        #[arg(required = true)]
        paths: Vec<String>,
        /// Remove directories and their contents
        #[arg(short, long)]
        recursive: bool,
    },
    /// Search file contents for a regex (respects .gitignore)
    #[command(after_help = "\
Searches recursively, skipping .gitignored files, binary files and .git,
but including hidden files. Matches print as 'path:line: text' and
context lines as 'path-line- text', with '--' between separate groups.
Output is truncated to --limit matches or 50KB, and long lines to 500
characters. Prints 'No matches found' and exits 1 when nothing matches.

Examples:
  fs grep 'fn main'
  fs grep -i todo src --glob '*.rs' -C 2
  fs grep -F 'a.b(c)'
  fs grep -- '-starts-with-dash'")]
    Grep {
        /// Regex to search for (literal text with -F)
        pattern: String,
        /// Directory or file to search
        #[arg(default_value = ".")]
        path: String,
        /// Only search files matching this glob, e.g. '*.rs'
        #[arg(short, long)]
        glob: Option<String>,
        /// Case-insensitive search
        #[arg(short, long)]
        ignore_case: bool,
        /// Treat the pattern as literal text, not a regex
        #[arg(short = 'F', long)]
        literal: bool,
        /// Lines to show around each match
        #[arg(short = 'C', long, value_name = "LINES", default_value_t = 0)]
        context: usize,
        /// Maximum number of matches
        #[arg(short, long, value_name = "MATCHES", default_value_t = 100)]
        limit: usize,
    },
    /// Find files and directories by glob (respects .gitignore)
    #[command(after_help = "\
A pattern without a '/' matches names at any depth; a pattern with a '/'
matches paths relative to PATH, where '**' spans directories. Skips
.gitignored files and .git, but includes hidden files. Directories are
listed with a trailing '/'. Prints 'No files found matching pattern' and
exits 1 when nothing matches.

Examples:
  fs find '*.rs'
  fs find 'src/**/*.test.ts' --limit 50
  fs find '*' docs")]
    Find {
        /// Glob to match, e.g. '*.rs' or 'src/**/*.ts'
        pattern: String,
        /// Directory to search
        #[arg(default_value = ".")]
        path: String,
        /// Maximum number of results
        #[arg(short, long, value_name = "RESULTS", default_value_t = 1000)]
        limit: usize,
    },
}

fn main() -> ExitCode {
    let command = Cli::command();
    let reference = help::reference(&command);
    let matches = command.after_long_help(reference).get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|err| err.exit());
    let result = Policy::from_env().and_then(|policy| {
        let policy = Arc::new(policy);
        match cli.command {
            Command::Read { path, offset, limit, line_numbers } => read::run(&policy, &path, offset, limit, line_numbers),
            Command::Write { path, content } => write::run(&policy, &path, content),
            Command::Edit { path, old, new } => edit::run(&policy, &path, &old, &new),
            Command::Move { from, to, force } => r#move::run(&policy, &from, &to, force),
            Command::Remove { paths, recursive } => remove::run(&policy, &paths, recursive),
            Command::Grep { pattern, path, glob, ignore_case, literal, context, limit } => {
                grep::run(&policy, &pattern, &path, glob, ignore_case, literal, context, limit)
            }
            Command::Find { pattern, path, limit } => find::run(&policy, &pattern, &path, limit),
        }
    });
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("fs: {err}");
            ExitCode::FAILURE
        }
    }
}
