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

use super::admin::Admin;
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
    /// Whose chat it hears, already resolved; `None` is nobody's.
    pub admin: Option<&'a Admin>,
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
    Command::new(program)
        .args(run_args(launch))
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        // A log file is read, not rendered: no colour codes in it.
        .env("NO_COLOR", "1")
        .process_group(0)
        .spawn()
        .map_err(|e| format!("launching the agent: {e}"))
}

/// The daemon's command line: `run`, with what `start` settled. The admin
/// goes as its DID - never the name it was given, which could resolve to
/// someone else the second time it was looked up.
fn run_args(launch: &Launch<'_>) -> Vec<String> {
    let mut args = vec!["run".to_owned()];
    if launch.offline {
        args.push("--offline".to_owned());
        if launch.did != crate::config::agent::OFFLINE_DID {
            args.extend(["--stand-in".to_owned(), launch.did.to_owned()]);
        }
    } else {
        args.extend(["--account".to_owned(), launch.did.to_owned()]);
    }
    if let Some(room) = launch.room {
        args.extend(["--room".to_owned(), room.to_owned()]);
    }
    if let Some(admin) = launch.admin {
        args.extend(["--admin".to_owned(), admin.did.clone()]);
        if let Some(handle) = &admin.handle {
            args.extend(["--admin-handle".to_owned(), handle.clone()]);
        }
    }
    args
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

    /// `start` hands its daemon the admin it resolved - the DID, and the
    /// handle to show beside it - and the daemon's command line parses.
    #[test]
    fn start_forwards_the_admin_as_its_did() {
        use clap::Parser as _;
        let admin = Admin {
            did: "did:plc:admin".into(),
            handle: Some("admin.test".into()),
        };
        let launch = Launch {
            did: "did:plc:agent",
            handle: "agent.test",
            room: None,
            offline: false,
            admin: Some(&admin),
        };

        let args = run_args(&launch);

        let cli = super::super::cli::Cli::try_parse_from(
            std::iter::once("agent".to_owned()).chain(args.iter().cloned()),
        )
        .expect("the daemon's command line parses");
        let super::super::cli::Command::Run(run) = cli.command else {
            panic!("run: {args:?}");
        };
        assert_eq!(run.run.admin.as_deref(), Some("did:plc:admin"));
        assert_eq!(run.admin_handle.as_deref(), Some("admin.test"));
        assert_eq!(run.run.account.name.as_deref(), Some("did:plc:agent"));
    }

    /// No admin: nothing about one on the daemon's command line, so it
    /// hears nobody.
    #[test]
    fn no_admin_is_forwarded_as_none() {
        let launch = Launch {
            did: crate::config::agent::OFFLINE_DID,
            handle: "agent.test",
            room: Some("did:plc:room"),
            offline: true,
            admin: None,
        };
        assert_eq!(
            run_args(&launch),
            ["run", "--offline", "--room", "did:plc:room"]
        );
    }

    /// An offline agent standing in as another identity takes it to the
    /// daemon; the usual stand-in needs no saying.
    #[test]
    fn a_stand_in_is_forwarded_when_it_is_not_the_usual_one() {
        let launch = Launch {
            did: "did:plc:agentofflineboat2222222d",
            handle: "agent.test",
            room: None,
            offline: true,
            admin: None,
        };
        assert_eq!(
            run_args(&launch),
            [
                "run",
                "--offline",
                "--stand-in",
                "did:plc:agentofflineboat2222222d"
            ]
        );
    }
}
