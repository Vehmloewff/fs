use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fs-regression-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fs_cmd(args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_fs"));
    command.env_clear().args(args);
    command
}

fn output(args: &[&str]) -> Output {
    fs_cmd(args).output().unwrap()
}

#[cfg(unix)]
#[test]
fn write_rejects_dangling_symlink_escape_from_root() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new();
    let root = temp.path().join("root");
    let outside = temp.path().join("outside");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();
    symlink(outside.join("missing"), root.join("link")).unwrap();

    let result = fs_cmd(&["write", root.join("link").to_str().unwrap(), "payload"])
        .env("FS_ROOT_DIR", &root)
        .output()
        .unwrap();

    assert!(!result.status.success());
    assert!(!outside.join("missing").exists());
}

#[cfg(unix)]
#[test]
fn write_rejects_dangling_symlink_parent_escape_from_root() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new();
    let root = temp.path().join("root");
    let outside = temp.path().join("outside");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();
    symlink(outside.join("missing"), root.join("link")).unwrap();

    let result = fs_cmd(&["write", root.join("link/file").to_str().unwrap(), "payload"])
        .env("FS_ROOT_DIR", &root)
        .output()
        .unwrap();

    assert!(!result.status.success());
    assert!(!outside.join("missing/file").exists());
}

#[cfg(unix)]
#[test]
fn move_rejects_dangling_symlink_escape_before_creating_parents() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new();
    let root = temp.path().join("root");
    let outside = temp.path().join("outside");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(root.join("source"), "payload").unwrap();
    symlink(root.join("missing/../../outside/escape"), root.join("link")).unwrap();

    let result = fs_cmd(&[
        "move",
        root.join("source").to_str().unwrap(),
        root.join("link/sub/file").to_str().unwrap(),
    ])
    .env("FS_ROOT_DIR", &root)
    .output()
    .unwrap();

    assert!(!result.status.success());
    assert!(root.join("source").exists());
    assert!(!outside.join("escape").exists());
}

#[cfg(unix)]
#[test]
fn deny_globs_apply_to_symlink_targets() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new();
    let root = temp.path().join("root");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("secret"), "classified").unwrap();
    symlink("secret", root.join("alias")).unwrap();

    let result = fs_cmd(&["read", root.join("alias").to_str().unwrap()])
        .env("FS_ROOT_DIR", &root)
        .env("FS_DENIED_FILES", "secret")
        .output()
        .unwrap();

    assert!(!result.status.success());
    assert!(!String::from_utf8_lossy(&result.stdout).contains("classified"));
}

#[cfg(unix)]
#[test]
fn deny_globs_apply_to_dangling_symlink_targets() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new();
    let target = temp.path().join("secret");
    let alias = temp.path().join("alias");
    symlink(&target, &alias).unwrap();

    let result = fs_cmd(&["write", alias.to_str().unwrap(), "payload"])
        .env("FS_DENIED_WRITE_FILES", "secret")
        .output()
        .unwrap();

    assert!(!result.status.success());
    assert!(!target.exists());
}

#[test]
fn move_into_source_directory_fails_without_creating_parents() {
    let temp = TempDir::new();
    let source = temp.path().join("dir");
    fs::create_dir(&source).unwrap();

    let result = output(&[
        "move",
        source.to_str().unwrap(),
        source.join("sub/new").to_str().unwrap(),
    ]);

    assert!(!result.status.success());
    assert!(!source.join("sub").exists());
}

#[test]
fn fuzzy_edit_preserves_unmatched_unicode_bytes() {
    let temp = TempDir::new();
    let file = temp.path().join("file.txt");
    fs::write(&file, "ﬁnished smart—dash tail\n").unwrap();

    let result = output(&[
        "edit",
        file.to_str().unwrap(),
        "--old",
        "smart-dash",
        "--new",
        "done",
    ]);

    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read_to_string(file).unwrap(), "ﬁnished done tail\n");
}

#[test]
fn fuzzy_edit_still_matches_nfkc_compatibility_characters() {
    let temp = TempDir::new();
    let file = temp.path().join("file.txt");
    fs::write(&file, "ﬁnished cafe\u{301} value\n").unwrap();

    let result = output(&[
        "edit",
        file.to_str().unwrap(),
        "--old",
        "finished café",
        "--new",
        "complete",
    ]);

    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read_to_string(file).unwrap(), "complete value\n");
}

#[cfg(target_os = "linux")]
#[test]
fn cross_device_move_preserves_symlinks() {
    use std::os::unix::fs::{symlink, MetadataExt};

    let shared_memory = Path::new("/dev/shm");
    if !shared_memory.is_dir() {
        return;
    }

    let temp = TempDir::new();
    let target = temp.path().join("target");
    let link = temp.path().join("link");
    fs::write(&target, "target contents").unwrap();
    symlink(&target, &link).unwrap();

    let shared_dir = shared_memory.join(format!(
        "fs-regression-{}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&shared_dir).unwrap();
    let destination = shared_dir.join("link");

    if fs::metadata(temp.path()).unwrap().dev() == fs::metadata(shared_memory).unwrap().dev() {
        fs::remove_dir_all(shared_dir).unwrap();
        return;
    }

    let result = output(&[
        "move",
        link.to_str().unwrap(),
        destination.to_str().unwrap(),
    ]);

    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(fs::symlink_metadata(&destination)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read_link(&destination).unwrap(), target);
    assert!(!link.exists());
    fs::remove_dir_all(shared_dir).unwrap();
}

#[test]
fn edit_does_not_refuse_unique_exact_match_that_is_fuzzy_ambiguous() {
    let temp = TempDir::new();
    let file = temp.path().join("file.txt");
    fs::write(&file, "foo \u{2014} bar\nx\nfoo - bar\n").unwrap();

    let result = output(&[
        "edit",
        file.to_str().unwrap(),
        "--old",
        "foo - bar",
        "--new",
        "REPLACED",
    ]);

    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read_to_string(file).unwrap(), "foo \u{2014} bar\nx\nREPLACED\n");
}

#[test]
fn edit_preserves_mixed_line_endings() {
    let temp = TempDir::new();
    let file = temp.path().join("file.txt");
    fs::write(&file, b"a\nb\r\nc\n").unwrap();

    let result = output(&[
        "edit",
        file.to_str().unwrap(),
        "--old",
        "b",
        "--new",
        "B",
    ]);

    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read(&file).unwrap(), b"a\nB\r\nc\n");
}

#[test]
fn edit_keeps_crlf_when_replacement_spans_lines() {
    let temp = TempDir::new();
    let file = temp.path().join("file.txt");
    fs::write(&file, b"a\r\nb\r\nc\r\n").unwrap();

    let result = output(&[
        "edit",
        file.to_str().unwrap(),
        "--old",
        "b\nc",
        "--new",
        "x",
    ]);

    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read(&file).unwrap(), b"a\r\nx\r\n");
}

#[test]
fn edit_gives_inserted_lines_the_first_matched_line_ending() {
    let temp = TempDir::new();
    let file = temp.path().join("file.txt");
    fs::write(&file, b"a\r\nb\r\nc\r\n").unwrap();

    let result = output(&[
        "edit",
        file.to_str().unwrap(),
        "--old",
        "b",
        "--new",
        "x\ny",
    ]);

    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read(&file).unwrap(), b"a\r\nx\r\ny\r\nc\r\n");
}

#[test]
fn edit_keeps_uniform_crlf_unchanged() {
    let temp = TempDir::new();
    let file = temp.path().join("file.txt");
    fs::write(&file, b"a\r\nb\r\nc\r\n").unwrap();

    let result = output(&[
        "edit",
        file.to_str().unwrap(),
        "--old",
        "b",
        "--new",
        "B",
    ]);

    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read(&file).unwrap(), b"a\r\nB\r\nc\r\n");
}

#[test]
fn edit_inserting_lines_on_unterminated_last_line_keeps_line_breaks() {
    let temp = TempDir::new();
    for (before, after) in [("a\nb", "a\nb\nc"), ("a\r\nb", "a\r\nb\r\nc"), ("b", "b\nc")] {
        let file = temp.path().join("file.txt");
        fs::write(&file, before).unwrap();

        let result = output(&["edit", file.to_str().unwrap(), "--old", "b", "--new", "b\nc"]);

        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), after);
    }
}

#[test]
fn move_requires_read_access_to_source() {
    let temp = TempDir::new();
    fs::write(temp.path().join("key.pem"), "secret").unwrap();

    let result = fs_cmd(&["move", "key.pem", "key.txt"])
        .current_dir(temp.path())
        .env("FS_DENIED_READ_FILES", "*.pem")
        .output()
        .unwrap();

    assert!(!result.status.success());
    assert!(temp.path().join("key.pem").exists());
    assert!(!temp.path().join("key.txt").exists());
}
