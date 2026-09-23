//! `agent start` (#1416): launch `agent run` in the background and return
//! once it answers on its control socket.
//!
//! The daemon is this same program, re-run with `run`, in a process group of
//! its own - so the shell or tool that started it can come and go, and a
//! Ctrl+C meant for that shell does not reach the agent. Its output goes to
//! a private log, one per account, emptied at each start.

use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Instant;

use crate::config::agent::{HOME_DIR, LOG_DIR, START_POLL, START_WAIT};

use super::control;
use super::private_fs::{create_private, ensure_private_dir};

/// How many of the log's last lines a failed start shows.
const LOG_TAIL_LINES: usize = 20;

/// A daemon `start` launched.
pub struct Started {
    pub pid: u32,
    pub socket: PathBuf,
    pub log: PathBuf,
}

/// Which daemon to launch.
pub struct Launch<'a> {
    /// The account it plays as - or the offline stand-in's DID.
    pub did: &'a str,
    pub handle: &'a str,
    /// The world to enter, by its owner's DID; `None` is its own.
    pub room: Option<&'a str>,
    /// Stand in offline rather than resume a saved session.
    pub offline: bool,
}

/// Launch the daemon, and wait until it takes commands.
pub fn start(launch: &Launch<'_>) -> Result<Started, String> {
    let socket = control::socket_path(launch.did)?;
    if control::server::is_answering(&socket) {
        return Err(format!(
            "an agent is already running for @{}; `agent stop` ends it",
            launch.handle
        ));
    }
    let log = log_path(launch.did)?;
    let mut child = spawn(launch, &log)?;
    wait_until_answering(&socket, &mut child, &log)?;
    Ok(Started {
        pid: child.id(),
        socket,
        log,
    })
}

/// `<config dir>/agent/logs/<did>.log`, in a private directory.
fn log_path(did: &str) -> Result<PathBuf, String> {
    let dir = crate::prefs::config_dir()
        .ok_or("there is no config directory for the agent's log; set HOME")?
        .join(HOME_DIR)
        .join(LOG_DIR);
    ensure_private_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir.join(format!("{}.log", crate::prefs::account_file_stem(did))))
}

fn spawn(launch: &Launch<'_>, log: &Path) -> Result<Child, String> {
    let program = std::env::current_exe().map_err(|e| format!("finding this program: {e}"))?;
    let out = create_private(log).map_err(|e| format!("{}: {e}", log.display()))?;
    let err = out
        .try_clone()
        .map_err(|e| format!("{}: {e}", log.display()))?;
    let mut command = Command::new(program);
    command.arg("run");
    if launch.offline {
        command.arg("--offline");
    } else {
        command.arg("--account").arg(launch.did);
    }
    if let Some(room) = launch.room {
        command.arg("--room").arg(room);
    }
    command
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        // A log file is read, not rendered: no colour codes in it.
        .env("NO_COLOR", "1")
        .process_group(0)
        .spawn()
        .map_err(|e| format!("launching the agent: {e}"))
}

/// Poll until the daemon answers on `socket`, it exits, or the start is
/// given up on - whichever comes first.
fn wait_until_answering(socket: &Path, child: &mut Child, log: &Path) -> Result<(), String> {
    let deadline = Instant::now() + START_WAIT;
    loop {
        if control::server::is_answering(socket) {
            return Ok(());
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("watching the agent start: {e}"))?
        {
            return Err(format!(
                "the agent stopped as it started ({status}); the end of {}:\n{}",
                log.display(),
                tail(log)
            ));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "the agent (pid {}) did not open its control socket within {} s; its \
                 log is {}",
                child.id(),
                START_WAIT.as_secs(),
                log.display()
            ));
        }
        std::thread::sleep(START_POLL);
    }
}

/// The last lines of `log`, for a failure message.
fn tail(log: &Path) -> String {
    let text = std::fs::read_to_string(log).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(LOG_TAIL_LINES)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_shows_the_end_of_the_log_only() {
        let dir = std::env::temp_dir().join(format!("symbios-agent-tail-{}", std::process::id()));
        ensure_private_dir(&dir).expect("a directory");
        let log = dir.join("agent.log");
        let lines: Vec<String> = (1..=50).map(|n| format!("line {n}")).collect();
        std::fs::write(&log, lines.join("\n")).expect("written");

        let shown = tail(&log);

        assert!(shown.starts_with("line 31"), "{shown}");
        assert!(shown.ends_with("line 50"), "{shown}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
