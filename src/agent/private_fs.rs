//! Files only their owner can read (#1414, #1416): the agent's saved
//! sessions and its daemon logs, which hold an account's keys and the chat
//! it heard.

use std::fs;
use std::io::{self, Write as _};
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::Path;

/// Permission bits that let anyone but the owner in.
pub const GROUP_OR_OTHER: u32 = 0o077;

/// Create `dir` (and its parents) owner-only, and take an existing one back
/// to owner-only if it was looser.
pub fn ensure_private_dir(dir: &Path) -> io::Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    let permissions = fs::metadata(dir)?.permissions();
    if permissions.mode() & GROUP_OR_OTHER != 0 {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Write `bytes` to `path` through a sibling temp file created 0600 and a
/// rename. The temp name carries the process id, so two processes never
/// share one.
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a file path"))?;
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(format!(".{}.tmp", std::process::id()));
    let tmp = path.with_file_name(tmp_name);
    // `create_new` below refuses to reuse a file, so a temp left by a crashed
    // run that had this pid has to go first. A missing one is the usual case.
    match fs::remove_file(&tmp) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let written = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        })
        .and_then(|()| fs::rename(&tmp, path));
    if written.is_err() {
        // INTENTIONAL: best-effort cleanup; the write's own error is the one
        // worth reporting.
        let _ = fs::remove_file(&tmp);
    }
    written
}

/// Create (or empty) `path` for writing, owner-only from the moment it
/// exists. An existing file's permissions are tightened as well, since
/// truncating does not reset them.
pub fn create_private(path: &Path) -> io::Result<fs::File> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reused_log_file_is_taken_back_to_owner_only() {
        let dir = std::env::temp_dir().join(format!("symbios-agent-log-{}", std::process::id()));
        ensure_private_dir(&dir).expect("a private directory");
        let path = dir.join("agent.log");
        fs::write(&path, "old").expect("an old log");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("loosened");

        let mut file = create_private(&path).expect("created");
        file.write_all(b"new").expect("written");

        assert_eq!(
            fs::metadata(&path).expect("exists").permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::read_to_string(&path).expect("read"),
            "new",
            "emptied first"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
