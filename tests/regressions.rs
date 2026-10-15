use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
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
