//! The commands that sign an agent in and bring it up (#1414-#1416): `login`,
//! `accounts`, `start` and `run`.

use std::process::ExitCode;

use super::admin::Admin;
use super::cli::{DaemonArgs, LoginArgs, RunArgs};
use super::{config, daemon, find_session, launch, login, print_json, session_file};

pub(super) fn run_login(args: LoginArgs) -> Result<(), String> {
    let store = session_file::SessionStore::platform().map_err(|e| e.to_string())?;
    let request = login::LoginRequest {
        pds_url: args.pds,
        relay_host: args.relay,
        account: args.account,
        open_browser: !args.no_browser,
    };
    let signed_in = login::login(&request, &store)?;
    eprintln!(
        "Signed in as @{} ({}); the session is saved in {}.",
        signed_in.handle,
        signed_in.did,
        signed_in.session_file.display()
    );
    print_json(&serde_json::json!({
        "did": signed_in.did,
        "handle": signed_in.handle,
        "session_file": signed_in.session_file,
    }))
}

pub(super) fn list_accounts() -> Result<(), String> {
    let store = session_file::SessionStore::platform().map_err(|e| e.to_string())?;
    let listing = store.list().map_err(|e| e.to_string())?;
    let accounts: Vec<_> = listing
        .sessions
        .iter()
        .map(|(path, session)| {
            serde_json::json!({
                "did": session.did,
                "handle": session.handle,
                "pds_url": session.pds_url,
                "relay_host": session.relay_host,
                "session_file": path,
            })
        })
        .collect();
    let problems: Vec<String> = listing.problems.iter().map(|p| p.to_string()).collect();
    print_json(&serde_json::json!({ "accounts": accounts, "problems": problems }))
}

pub(super) fn start_daemon(args: RunArgs) -> Result<(), String> {
    let (did, handle) = if args.offline {
        (
            args.stand_in
                .clone()
                .unwrap_or_else(|| config::agent::OFFLINE_DID.to_owned()),
            config::agent::OFFLINE_HANDLE.to_owned(),
        )
    } else {
        let (_, session) = find_session(args.account.name.as_deref())?;
        (session.did, session.handle)
    };
    // Resolved here, once, before anything is launched: the daemon is handed
    // the DID, and an admin that cannot be found stops the start (#1427).
    let admin = args
        .admin
        .as_deref()
        .map(|name| Admin::resolve(name, &did))
        .transpose()
        .map_err(|e| format!("the agent was not started: {e}"))?;
    let started = launch::start(&launch::Launch {
        did: &did,
        handle: &handle,
        room: args.room.as_deref(),
        offline: args.offline,
        admin: admin.as_ref(),
    })?;
    print_json(&serde_json::json!({
        "did": did,
        "handle": handle,
        "offline": args.offline,
        "admin": admin.as_ref().map(Admin::to_json),
        "pid": started.pid,
        "socket": started.socket,
        "log": started.log,
    }))
}

pub(super) fn run_daemon(args: DaemonArgs) -> Result<ExitCode, String> {
    let DaemonArgs {
        run: args,
        admin_handle,
    } = args;
    let identity = if args.offline {
        daemon::Identity::Offline {
            did: args
                .stand_in
                .clone()
                .unwrap_or_else(|| config::agent::OFFLINE_DID.to_owned()),
        }
    } else {
        let (session_file, session) = find_session(args.account.name.as_deref())?;
        daemon::Identity::Saved {
            session_file,
            session: Box::new(session),
        }
    };
    let admin = args
        .admin
        .as_deref()
        .map(|name| {
            let mut admin = Admin::resolve(name, identity.did())?;
            // `start` hands over a DID it has already resolved, which needs
            // no lookup, and the handle it resolved it from, to show.
            if admin.handle.is_none() {
                admin.handle = admin_handle;
            }
            Ok::<_, String>(admin)
        })
        .transpose()
        .map_err(|e| format!("the agent was not started: {e}"))?;
    daemon::run(daemon::RunRequest {
        identity,
        room_did: args.room,
        admin,
    })
}
