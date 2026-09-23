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

#[cfg(test)]
mod tests {
    use super::*;

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
