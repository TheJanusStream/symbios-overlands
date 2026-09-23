//! The commands that talk to a running agent (#1416-#1419): each sends one
//! request over the control socket and prints the answer; `walk-to --wait`
//! and `travel --wait` then follow the event log to the end of what they
//! started.

use std::process::ExitCode;

use super::cli::{EventsArgs, TravelArgs, WalkToArgs};
use super::control::protocol::{Request, Response};
use super::{config, control, find_session, print_json, session_file};

pub(super) fn watch_events(args: EventsArgs) -> Result<ExitCode, String> {
    ask(
        args.account.name.as_deref(),
        Request::Events {
            since: args.since,
            wait_secs: args.wait,
        },
    )
}

/// Send `request` to the running agent and print its answer. A refused
/// request exits non-zero, with the reason in the printed answer.
pub(super) fn ask(account: Option<&str>, request: Request) -> Result<ExitCode, String> {
    let response = control::client::call(&socket_for(account)?, &request)?;
    print_response(&response)
}

/// The control socket of the agent `account` names.
///
/// With no saved session at all, the one agent that can be running is the
/// offline one, so that is who is meant.
fn socket_for(account: Option<&str>) -> Result<std::path::PathBuf, String> {
    let did = match find_session(account) {
        Ok((_, session)) => session.did,
        Err(_) if account.is_none() && !has_saved_sessions() => {
            config::agent::OFFLINE_DID.to_owned()
        }
        Err(e) => return Err(e),
    };
    control::socket_path(&did)
}

fn has_saved_sessions() -> bool {
    session_file::SessionStore::platform()
        .and_then(|store| store.list())
        .is_ok_and(|listing| !listing.sessions.is_empty() || !listing.problems.is_empty())
}

fn print_response(response: &Response) -> Result<ExitCode, String> {
    let json = serde_json::to_value(response).map_err(|e| format!("encoding: {e}"))?;
    print_json(&json)?;
    Ok(if response.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Start a walk; with `--wait`, follow the event log from the moment it
/// started until this walk's `movement_ended` arrives, and print that.
pub(super) fn walk_to(args: WalkToArgs) -> Result<ExitCode, String> {
    let socket = socket_for(args.account.name.as_deref())?;
    let started = control::client::call(
        &socket,
        &Request::WalkTo {
            x: args.x,
            z: args.z,
            run: args.run,
        },
    )?;
    let Some(result) = started.result.as_ref().filter(|_| args.wait && started.ok) else {
        return print_response(&started);
    };
    let goal_id = result["goal_id"]
        .as_u64()
        .ok_or("the walk has no goal_id")?;
    let since = result["events_seq"]
        .as_u64()
        .ok_or("the walk has no events_seq")?;
    let ended = wait_for_walk_end(&socket, goal_id, since)?;
    print_response(&Response::success(ended))
}

fn wait_for_walk_end(
    socket: &std::path::Path,
    goal_id: u64,
    since: u64,
) -> Result<serde_json::Value, String> {
    wait_for_event(socket, since, config::agent::WALK_WAIT, |event| {
        event["kind"] == "movement_ended" && event["goal_id"].as_u64() == Some(goal_id)
    })
    .map_err(|waited| {
        format!(
            "the walk had not ended after {} minutes; it goes on, and `agent halt` stops \
             it",
            waited.as_secs() / 60
        )
    })
}

/// Follow the event log from `since` until an event `ends` accepts, and
/// return it - or the wait, if `patience` runs out first.
fn wait_for_event(
    socket: &std::path::Path,
    mut since: u64,
    patience: std::time::Duration,
    ends: impl Fn(&serde_json::Value) -> bool,
) -> Result<serde_json::Value, std::time::Duration> {
    let deadline = std::time::Instant::now() + patience;
    while std::time::Instant::now() < deadline {
        let asked = Request::Events {
            since,
            wait_secs: config::agent::MAX_EVENT_WAIT.as_secs(),
        };
        let Ok(response) = control::client::call(socket, &asked) else {
            break;
        };
        let Some(batch) = response.result else {
            break;
        };
        if batch["restarted"] == true {
            break;
        }
        let found = batch["events"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| ends(e));
        if let Some(found) = found {
            return Ok(found.clone());
        }
        since = batch["next"].as_u64().unwrap_or(since);
    }
    Err(patience)
}

/// Start a trip - to `home`, a DID or a handle - and with `--wait`, follow it
/// to its arrival or failure.
pub(super) fn travel(args: TravelArgs) -> Result<ExitCode, String> {
    let account = args.account.name.as_deref();
    let socket = socket_for(account)?;
    let (room_did, label) = destination(account, &args.to)?;
    let started = control::client::call(
        &socket,
        &Request::Travel {
            room_did: room_did.clone(),
            label,
        },
    )?;
    let Some(result) = started.result.as_ref().filter(|_| args.wait && started.ok) else {
        return print_response(&started);
    };
    let since = result["events_seq"]
        .as_u64()
        .ok_or("the trip has no events_seq")?;
    let ended = wait_for_event(&socket, since, config::agent::TRAVEL_WAIT, |event| {
        (event["kind"] == "entered_world" && event["room_did"] == room_did.as_str())
            || (event["kind"] == "travel_failed" && event["to_did"] == room_did.as_str())
    })
    .map_err(|waited| {
        format!(
            "the trip had not ended after {} minutes",
            waited.as_secs() / 60
        )
    })?;
    print_response(&Response::success(ended))
}

/// Whose world `to` names, as a DID, and what the operator called it.
fn destination(account: Option<&str>, to: &str) -> Result<(String, Option<String>), String> {
    let to = to.trim();
    if to.eq_ignore_ascii_case("home") {
        let did = match find_session(account) {
            Ok((_, session)) => session.did,
            Err(_) if account.is_none() => config::agent::OFFLINE_DID.to_owned(),
            Err(e) => return Err(e),
        };
        return Ok((did, Some("home".to_owned())));
    }
    if to.starts_with("did:") {
        return Ok((to.to_owned(), None));
    }
    let handle = to.trim_start_matches('@').to_ascii_lowercase();
    let did = config::http::block_on(crate::pds::xrpc::resolve_handle(
        &config::http::default_client(),
        &handle,
    ))?;
    Ok((did, Some(format!("@{handle}"))))
}
