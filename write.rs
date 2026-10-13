use std::fs;
use std::io::{self, IsTerminal, Read};

use crate::policy::{Access, Policy};
use crate::Res;

pub fn run(policy: &Policy, raw: &str, content: Option<String>) -> Res<bool> {
    let path = policy.resolve(raw)?;
    policy.check(raw, &path, Access::Write)?;
    let content = match content {
        Some(content) => content,
        None if io::stdin().is_terminal() => {
            return Err("no content: pass it as an argument or pipe it on stdin".into());
        }
        None => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf).map_err(|e| format!("reading stdin: {e}"))?;
            buf
        }
    };
    if path.is_dir() {
        return Err(format!("{raw}: is a directory"));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    fs::write(&path, &content).map_err(|e| format!("{raw}: {e}"))?;
    Ok(true)
}
