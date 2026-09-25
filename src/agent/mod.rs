//! The agent client (#1413): a headless Overlands client an AI agent drives
//! from the command line, signed in as its own account.
//!
//! To everyone else in a world the agent is a player like any other: it runs
//! the game's own client, sends exactly what that client sends, and looks
//! exactly like it. What differs is who is at the controls. A person signs
//! the agent's account in once, in a browser ([`login`]); from then on the
//! agent's session is resumed from a private file ([`session_file`]) with
//! nobody at the keyboard, and the tool never sees a password.
//!
//! Unix-only: the credentials live in owner-only files.
//!
//! ## Sub-module map
//!
//! * [`cli`] - the command line, one JSON object per command on stdout.
//! * [`admin`] - the one account whose chat the agent hears.
//! * [`login`] - the one-time browser sign-in.
//! * [`session_file`] - the saved sessions, one private file per account.
//! * [`daemon`] - the headless client itself, resumed from a saved session.
//! * [`control`] - the socket a running daemon takes commands on.
//! * [`launch`] - `agent start`, the daemon in the background.
//! * [`private_fs`] - the owner-only files the rest keep secrets in.
//! * [`lifecycle`] - the commands that sign an agent in and bring it up.
//! * [`requests`] - the commands that talk to a running agent.

mod admin;
mod cli;
mod control;
mod daemon;
mod launch;
mod lifecycle;
mod login;
mod private_fs;
mod requests;
mod session_file;

use std::process::ExitCode;

use clap::Parser as _;

use cli::{Cli, Command};
use control::protocol::Request;
use lifecycle::{list_accounts, run_daemon, run_login, start_daemon};
use requests::{
    JsonRecord, ask, catalogue, face, follow, gift, look, move_placement, place, placements,
    record_json, remove, save, travel, ui, walk_to, watch_events,
};

use crate::config;

/// Run the agent command line, returning the process's exit code.
pub fn run() -> ExitCode {
    let cli = Cli::parse();
    execute(cli.command).unwrap_or_else(|message| {
        eprintln!("agent: {message}");
        ExitCode::FAILURE
    })
}

fn execute(command: Command) -> Result<ExitCode, String> {
    match command {
        Command::Login(args) => run_login(args).map(|()| ExitCode::SUCCESS),
        Command::Accounts => list_accounts().map(|()| ExitCode::SUCCESS),
        Command::Start(args) => start_daemon(args).map(|()| ExitCode::SUCCESS),
        Command::Run(args) => run_daemon(args),
        Command::Status(account) => ask(account.name.as_deref(), Request::Status),
        Command::Events(args) => watch_events(args),
        Command::Stop(account) => ask(account.name.as_deref(), Request::Stop),
        Command::Say(args) => ask(
            args.account.name.as_deref(),
            Request::Say { text: args.text },
        ),
        Command::WalkTo(args) => walk_to(args),
        Command::Halt(account) => ask(account.name.as_deref(), Request::Halt),
        Command::Travel(args) => travel(args),
        Command::Look(args) => look(args),
        Command::Follow(args) => follow(args),
        Command::Face(args) => face(args),
        Command::Placements(args) => placements(args),
        Command::Catalogue(args) => catalogue(args),
        Command::Place(args) => place(args),
        Command::Move(args) => move_placement(args),
        Command::Remove(args) => remove(args),
        Command::Room(args) => record_json(JsonRecord::Room, args),
        Command::Avatar(args) => record_json(JsonRecord::Avatar, args),
        Command::Undo(args) => ask(
            args.account.name.as_deref(),
            Request::Undo {
                record: args.record,
            },
        ),
        Command::Redo(args) => ask(
            args.account.name.as_deref(),
            Request::Redo {
                record: args.record,
            },
        ),
        Command::Revert(args) => ask(
            args.account.name.as_deref(),
            Request::Revert {
                record: args.record,
            },
        ),
        Command::Save(args) => save(args),
        Command::Inventory(account) => ask(account.name.as_deref(), Request::Inventory),
        Command::Stash(args) => ask(
            args.account.name.as_deref(),
            Request::Stash {
                what: args.what,
                from_avatar: args.from_avatar,
            },
        ),
        Command::Unstash(args) => ask(
            args.account.name.as_deref(),
            Request::Unstash { name: args.item },
        ),
        Command::Wear(args) => ask(
            args.account.name.as_deref(),
            Request::Wear { name: args.item },
        ),
        Command::TakeOff(args) => ask(
            args.account.name.as_deref(),
            Request::TakeOff { name: args.item },
        ),
        Command::Gift(args) => gift(args),
        Command::Ui(args) => ui(args),
    }
}

/// The saved session a command names - or the only one there is.
pub(super) fn find_session(
    account: Option<&str>,
) -> Result<(std::path::PathBuf, session_file::AgentSession), String> {
    session_file::SessionStore::platform()
        .and_then(|store| store.find(account))
        .map_err(|e| e.to_string())
}

/// The DID `name` stands for - a DID as given, or a handle (with or without
/// its @) looked up - and the handle, when it was one.
pub(super) fn resolve_name(name: &str) -> Result<(String, Option<String>), String> {
    let name = name.trim();
    if name.starts_with("did:") {
        return Ok((name.to_owned(), None));
    }
    let handle = name.trim_start_matches('@').to_ascii_lowercase();
    let did = config::http::block_on(crate::pds::xrpc::resolve_handle(
        &config::http::default_client(),
        &handle,
    ))?;
    Ok((did, Some(handle)))
}

/// Print one command's result as a single line of JSON.
pub(super) fn print_json(value: &serde_json::Value) -> Result<(), String> {
    write_json_line(&mut std::io::stdout().lock(), value)
}

/// Write `value` to `out` as one line of JSON. A reader that has gone - the
/// far end of a pipe closed early, as `head` closes it - is no failure: the
/// command has run, and nobody is left to read what it would say (#1451).
/// `println!` panicked there instead.
fn write_json_line(out: &mut impl std::io::Write, value: &serde_json::Value) -> Result<(), String> {
    let line = serde_json::to_string(value).map_err(|e| format!("encoding the result: {e}"))?;
    match writeln!(out, "{line}").and_then(|()| out.flush()) {
        Err(e) if e.kind() != std::io::ErrorKind::BrokenPipe => {
            Err(format!("printing the result: {e}"))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};

    use serde_json::json;

    use super::write_json_line;

    /// A writer that refuses every write with `kind`.
    struct Refusing(io::ErrorKind);

    impl Write for Refusing {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(self.0.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// #1451: `agent status | head -1`, or any reader that closes before the
    /// answer lands, made the command panic and abort (exit 134) after it
    /// had done its work.
    #[test]
    fn a_reader_that_has_gone_is_no_failure() {
        let closed = &mut Refusing(io::ErrorKind::BrokenPipe);
        assert_eq!(write_json_line(closed, &json!({ "ok": true })), Ok(()));
    }

    #[test]
    fn any_other_failure_to_print_is_reported() {
        let refusing = &mut Refusing(io::ErrorKind::PermissionDenied);
        let error = write_json_line(refusing, &json!({ "ok": true })).unwrap_err();
        assert!(error.starts_with("printing the result: "), "{error}");
    }

    #[test]
    fn a_result_is_one_line_of_json() {
        let mut out = Vec::new();
        write_json_line(&mut out, &json!({ "ok": true, "result": { "n": 1 } })).unwrap();
        assert_eq!(out, b"{\"ok\":true,\"result\":{\"n\":1}}\n");
    }
}
