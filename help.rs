//! The full reference shown by `fs --help`, rendered from the clap definitions
//! so it always matches the real arguments.

use clap::{Arg, ArgAction, Command};

const CONVENTIONS: &str = "\
Conventions:
  Paths are relative to the current directory; '~' expands to your home.
  write, edit, move and remove print nothing when they succeed.
  Errors print to stderr as 'fs: <message>' and exit with code 1.
  Truncated output ends with a [bracketed notice] saying how to get the rest.
  Put '--' before a pattern or path that starts with '-'.";

const ENVIRONMENT: &str = "\
Environment:
  FS_ROOT_DIR            Every path must stay inside this directory. Relative
                         paths resolve against it when the current directory
                         is outside it.
  FS_DENIED_FILES        Globs that may not be read or written
  FS_DENIED_READ_FILES   Globs that may not be read
  FS_DENIED_WRITE_FILES  Globs that may not be written, edited, moved or removed
  FS_GREP_IGNORE         Extra paths grep skips, on top of .gitignore

  Lists are comma separated, e.g. FS_DENIED_FILES='.env,*.pem,secrets'.

  In the FS_DENIED_* lists, a glob without a '/' matches a file or directory
  name anywhere, and a glob with a '/' matches the path relative to FS_ROOT_DIR
  (or the current directory). Denying a directory denies everything inside it.
  grep skips denied files and find hides files in FS_DENIED_FILES.

  FS_GREP_IGNORE takes .gitignore-style patterns relative to the search path,
  e.g. FS_GREP_IGNORE='dist,*.min.js,vendor/'.";

pub fn reference(cli: &Command) -> String {
    let mut out = format!("{CONVENTIONS}\n\nCommand reference:\n");
    for cmd in cli.get_subcommands() {
        out.push('\n');
        out.push_str(&command_reference(cmd));
    }
    out.push('\n');
    out.push_str(ENVIRONMENT);
    out
}

fn command_reference(cmd: &Command) -> String {
    let args: Vec<&Arg> = cmd.get_arguments().filter(|a| !matches!(a.get_id().as_str(), "help" | "version")).collect();

    let mut usage = format!("  fs {}", cmd.get_name());
    for arg in args.iter().filter(|a| a.is_positional()) {
        usage.push(' ');
        usage.push_str(&positional_name(arg));
    }
    for arg in args.iter().filter(|a| !a.is_positional() && a.is_required_set()) {
        usage.push(' ');
        usage.push_str(arg_name(arg).trim());
    }
    if args.iter().any(|a| !a.is_positional() && !a.is_required_set()) {
        usage.push_str(" [options]");
    }
    let aliases: Vec<&str> = cmd.get_visible_aliases().collect();
    if !aliases.is_empty() {
        usage.push_str(&format!("   (alias: {})", aliases.join(", ")));
    }

    let about = cmd.get_about().map(|a| a.to_string()).unwrap_or_default();
    let mut out = format!("{usage}\n      {about}\n\n");

    let rows: Vec<(String, String)> = args.iter().map(|a| (arg_name(a), arg_help(a))).collect();
    let width = rows.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
    for (name, help) in rows {
        out.push_str(&format!("      {name:width$}  {help}\n"));
    }

    if let Some(notes) = cmd.get_after_help() {
        out.push('\n');
        for line in notes.to_string().lines() {
            if line.is_empty() {
                out.push('\n');
            } else {
                out.push_str(&format!("      {line}\n"));
            }
        }
    }
    out
}

fn positional_name(arg: &Arg) -> String {
    let name = arg.get_id().as_str().to_uppercase();
    let name = if matches!(arg.get_action(), ArgAction::Append) { format!("{name}...") } else { name };
    if arg.is_required_set() {
        format!("<{name}>")
    } else {
        format!("[{name}]")
    }
}

fn arg_name(arg: &Arg) -> String {
    if arg.is_positional() {
        return positional_name(arg);
    }
    let mut name = match (arg.get_short(), arg.get_long()) {
        (Some(s), Some(l)) => format!("-{s}, --{l}"),
        (None, Some(l)) => format!("    --{l}"),
        (Some(s), None) => format!("-{s}"),
        (None, None) => String::new(),
    };
    if arg.get_action().takes_values() {
        let value = arg.get_value_names().and_then(|v| v.first()).map(|v| v.to_string());
        name.push_str(&format!(" <{}>", value.unwrap_or_else(|| arg.get_id().as_str().to_uppercase())));
    }
    name
}

fn arg_help(arg: &Arg) -> String {
    let mut help = arg.get_help().map(|h| h.to_string()).unwrap_or_default();
    let defaults: Vec<String> = arg.get_default_values().iter().map(|v| v.to_string_lossy().into_owned()).collect();
    if !defaults.is_empty() && !matches!(arg.get_action(), ArgAction::SetTrue) {
        help.push_str(&format!(" [default: {}]", defaults.join(", ")));
    }
    help
}
