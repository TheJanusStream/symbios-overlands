//! The agent daemon's control channel (#1416): how `agent status`, `agent
//! events`, `agent stop` and the rest reach a running daemon.
//!
//! A Unix socket per account, in a directory only the user can enter - the
//! boundary ssh-agent draws: any process running as this user can drive the
//! agent, and nothing else can. One request per connection: the client
//! writes one line of JSON, the daemon answers with one line and closes.
//!
//! This module is transport only - no ECS. A request that needs the world is
//! handed over a channel to the daemon, which answers it on its next frame.
//! `events` is answered here, from the shared [`events::EventLog`], so a
//! client can wait for something to happen without holding a frame up.

pub mod client;
pub mod events;
pub mod protocol;
pub mod server;

use std::path::PathBuf;

/// The longest path a Unix socket address holds on Linux (`sun_path`), less
/// its terminating NUL.
const MAX_SOCKET_PATH: usize = 107;

/// Where the daemon for `did` listens:
/// `$XDG_RUNTIME_DIR/symbios-overlands-agent/<did>.sock`, or the agent's own
/// `run/` directory on a system with no runtime directory.
pub fn socket_path(did: &str) -> Result<PathBuf, String> {
    use crate::config::agent::{HOME_DIR, RUN_DIR, RUNTIME_DIR_NAME};
    let dir = match std::env::var_os("XDG_RUNTIME_DIR").filter(|dir| !dir.is_empty()) {
        Some(runtime) => PathBuf::from(runtime).join(RUNTIME_DIR_NAME),
        None => crate::prefs::config_dir()
            .ok_or(
                "there is no directory for the agent's control socket; set XDG_RUNTIME_DIR or HOME",
            )?
            .join(HOME_DIR)
            .join(RUN_DIR),
    };
    let path = dir.join(format!("{}.sock", crate::prefs::account_file_stem(did)));
    if path.as_os_str().len() > MAX_SOCKET_PATH {
        return Err(format!(
            "the control socket's path is {} bytes, longer than a Unix socket allows \
             ({MAX_SOCKET_PATH}): {}; point XDG_RUNTIME_DIR at a shorter directory",
            path.as_os_str().len(),
            path.display()
        ));
    }
    Ok(path)
}

/// A test's control socket in a directory of its own under the system's
/// temporary one, `sa-<name>-<pid>/agent.sock`, the directory removed when
/// the guard drops - when the test ends, passed or failed. A listener's
/// [`server::ControlSocket`] takes its socket file away but not the
/// directory, and every test run left one per test in `/tmp` (#1517: 902 of
/// them on 2026-09-28). It derefs to the socket's path.
#[cfg(test)]
pub(crate) struct TestSocket {
    dir: PathBuf,
    path: PathBuf,
}

#[cfg(test)]
impl TestSocket {
    /// The directory is cleared first, in case one of the same name and
    /// process id is there already.
    pub(crate) fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("sa-{name}-{}", std::process::id()));
        // INTENTIONAL: usually there is nothing to remove.
        let _ = std::fs::remove_dir_all(&dir);
        Self {
            path: dir.join("agent.sock"),
            dir,
        }
    }
}

#[cfg(test)]
impl std::ops::Deref for TestSocket {
    type Target = std::path::Path;

    fn deref(&self) -> &std::path::Path {
        &self.path
    }
}

#[cfg(test)]
impl AsRef<std::path::Path> for TestSocket {
    fn as_ref(&self) -> &std::path::Path {
        &self.path
    }
}

#[cfg(test)]
impl Drop for TestSocket {
    fn drop(&mut self) {
        // INTENTIONAL: best effort - a directory this fails to remove (a
        // run killed mid-test leaves its own) stays until /tmp is wiped:
        // process ids differ between runs, so no later `new` clears it.
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test's socket directory is gone once its guard drops, and a
    /// directory left by an earlier run is cleared when the guard is made.
    #[test]
    fn a_test_socket_takes_its_directory_with_it() {
        let stale = std::env::temp_dir().join(format!("sa-guard-{}", std::process::id()));
        std::fs::create_dir_all(&stale).expect("a stale directory");
        std::fs::write(stale.join("agent.sock"), b"").expect("a stale socket file");
        let socket = TestSocket::new("guard");
        assert!(!stale.exists(), "the stale directory is cleared");
        std::fs::create_dir_all(socket.parent().expect("a directory")).expect("made");
        std::fs::write(&socket, b"").expect("a socket file");
        drop(socket);
        assert!(!stale.exists(), "the directory goes with the guard");
    }

    #[test]
    fn a_did_plc_socket_fits_under_the_usual_runtime_directory() {
        let did = "did:plc:abcdefghijklmnopqrstuvwx";
        let path = PathBuf::from("/run/user/1000")
            .join(crate::config::agent::RUNTIME_DIR_NAME)
            .join(format!("{}.sock", crate::prefs::account_file_stem(did)));
        assert!(
            path.as_os_str().len() <= MAX_SOCKET_PATH,
            "{} is {} bytes",
            path.display(),
            path.as_os_str().len()
        );
    }
}
