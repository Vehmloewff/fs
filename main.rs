mod edit;
mod find;
mod grep;
mod r#move;
mod output;
mod policy;
mod read;
mod remove;
mod walk;
mod write;

use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};

use policy::Policy;

pub type Res<T> = Result<T, String>;

#[derive(Parser)]
#[command(
    name = "fs",
    version,
    about = "File system utilities for humans and agents",
    after_help = "\
Environment:
  FS_ROOT_DIR            Confine every path to this directory
  FS_DENIED_FILES        Globs that may not be read or written
  FS_DENIED_READ_FILES   Globs that may not be read
  FS_DENIED_WRITE_FILES  Globs that may not be written, moved or removed

Glob lists are comma separated, e.g. FS_DENIED_FILES='.env,*.pem,secrets/**'.
A glob without a '/' matches a file or directory name anywhere; a glob with a
'/' matches the path relative to FS_ROOT_DIR (or the current directory).
Denying a directory denies everything inside it."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read a text file
    #[command(after_help = "\
Output is truncated to 2000 lines or 50KB, whichever comes first. When output is
truncated, a notice says which --offset to use to continue.

Examples:
  fs read main.rs
  fs read main.rs --offset 120 --limit 40")]
    Read {
        /// File to read
        path: String,
        /// Line number to start reading from (1-indexed)
        #[arg(short, long)]
        offset: Option<usize>,
        /// Maximum number of lines to read
        #[arg(short, long)]
        limit: Option<usize>,
        /// Prefix each line with its line number
        #[arg(short = 'n', long)]
        line_numbers: bool,
    },
    /// Create or overwrite a file
    #[command(after_help = "\
Parent directories are created as needed. Content comes from the CONTENT argument,
or from stdin when it is omitted.

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
--old must match exactly one region of the file. If there is no exact match,
trailing whitespace, smart quotes, Unicode dashes and special spaces are ignored.
Line endings and a leading BOM are preserved.

One edit per command: to make several changes, run fs edit once for each. If two
changes touch the same block or nearby lines, make them one edit instead.

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
        #[arg(long = "old", allow_hyphen_values = true)]
        old: String,
        /// Replacement text
        #[arg(long = "new", allow_hyphen_values = true)]
        new: String,
    },
    /// Move or rename a file or directory
    #[command(visible_alias = "mv")]
    Move {
        /// Path to move
        from: String,
        /// Destination path (parent directories are created as needed)
        to: String,
        /// Overwrite the destination file if it exists
        #[arg(short, long)]
        force: bool,
    },
    /// Remove files or directories
    #[command(visible_alias = "rm")]
    Remove {
        /// Paths to remove
        #[arg(required = true)]
        paths: Vec<String>,
        /// Remove directories and their contents
        #[arg(short, long)]
        recursive: bool,
    },
    /// Search file contents for a pattern (respects .gitignore)
    #[command(after_help = "\
Matches print as 'path:line: text'; context lines print as 'path-line- text'.
Output is truncated to --limit matches or 50KB, and long lines to 500 characters.

Examples:
  fs grep 'fn main'
  fs grep -i todo src --glob '*.rs' -C 2
  fs grep -F 'a.b(c)' -- -dash-dir")]
    Grep {
        /// Regex to search for (or literal string with --literal)
        pattern: String,
        /// Directory or file to search
        #[arg(default_value = ".")]
        path: String,
        /// Only search files matching this glob, e.g. '*.rs' or 'src/**/*.ts'
        #[arg(short, long)]
        glob: Option<String>,
        /// Case-insensitive search
        #[arg(short, long)]
        ignore_case: bool,
        /// Treat the pattern as a literal string instead of a regex
        #[arg(short = 'F', long)]
        literal: bool,
        /// Lines of context to show around each match
        #[arg(short = 'C', long, default_value_t = 0)]
        context: usize,
        /// Maximum number of matches
        #[arg(short, long, default_value_t = 100)]
        limit: usize,
    },
    /// Find files by glob pattern (respects .gitignore)
    #[command(after_help = "\
A pattern without a '/' matches file names; a pattern with a '/' matches paths
relative to the search directory. Directories are listed with a trailing '/'.

Examples:
  fs find '*.rs'
  fs find 'src/**/*.test.ts' --limit 50")]
    Find {
        /// Glob to match, e.g. '*.rs', '**/*.json' or 'src/**/*.spec.ts'
        pattern: String,
        /// Directory to search
        #[arg(default_value = ".")]
        path: String,
        /// Maximum number of results
        #[arg(short, long, default_value_t = 1000)]
        limit: usize,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
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
