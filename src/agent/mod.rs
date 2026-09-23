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
//! * [`login`] - the one-time browser sign-in.
//! * [`session_file`] - the saved sessions, one private file per account.
//! * [`daemon`] - the headless client itself, resumed from a saved session.
//! * [`control`] - the socket a running daemon takes commands on.
//! * [`launch`] - `agent start`, the daemon in the background.
//! * [`private_fs`] - the owner-only files the rest keep secrets in.
//! * [`lifecycle`] - the commands that sign an agent in and bring it up.
//! * [`requests`] - the commands that talk to a running agent.

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
use requests::{ask, travel, walk_to, watch_events};

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

/// Print one command's result as a single line of JSON.
pub(super) fn print_json(value: &serde_json::Value) -> Result<(), String> {
    let line = serde_json::to_string(value).map_err(|e| format!("encoding the result: {e}"))?;
    println!("{line}");
    Ok(())
}
