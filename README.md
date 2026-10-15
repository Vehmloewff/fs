# fs

File system utilities that work just as well for agents as they do for humans.

Coding agents spend most of their time reading, searching and editing files, usually through `cat`, `sed`, `grep` and `find`. Those tools were never designed for it: output floods the context window, edits are fragile, and nothing stops an agent from wandering into `.env` or out of the project. `fs` is one small binary with a plain CLI that fixes that.

- **Output that fits.** Long files and big searches are truncated with a note explaining the limit and how to continue when possible. A single overlong line is clipped; line offsets cannot resume within it.
- **Safe edits.** `fs edit` replaces one exact, unique piece of text, and refuses when the match is missing or ambiguous.
- **Guardrails.** Confine every command to a directory, and deny reads or writes to files you choose.
- **Quiet.** Commands that change things print nothing when they succeed.
- **Self-contained.** Search is built on ripgrep's libraries, so there is nothing else to install.

## Install

```sh
nix profile install github:Vehmloewff/fs
```

## Usage

```sh
fs read main.rs --offset 120 --limit 40
fs write notes.txt 'hello world'
fs edit main.rs --old 'let x = 1;' --new 'let x = 2;'
fs move old.rs new.rs
fs remove build --recursive
fs grep 'TODO' src --glob '*.rs' --context 2
fs find '**/*.json'
```

Run `fs <command> --help` for every option.

## Configuration

| Variable | Effect |
| --- | --- |
| `FS_ROOT_DIR` | Every path must stay inside this directory |
| `FS_DENIED_FILES` | Globs that can't be read or written |
| `FS_DENIED_READ_FILES` | Globs that can't be read |
| `FS_DENIED_WRITE_FILES` | Globs that can't be written, moved or removed |
| `FS_GREP_IGNORE` | Globs `fs grep` skips, on top of `.gitignore` |

```sh
export FS_ROOT_DIR=~/code/my-project
export FS_DENIED_FILES='.env,*.pem,secrets'
```

## License

[MIT](LICENSE)
