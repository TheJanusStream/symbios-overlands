//! The commands that talk to a running agent (#1416-#1419): each sends one
//! request over the control socket and prints the answer; `walk-to --wait`
//! and `travel --wait` then follow the event log to the end of what they
//! started.

use std::process::ExitCode;

use super::cli::{EventsArgs, FaceArgs, FollowArgs, LookArgs, TravelArgs, WalkToArgs};
use super::control::protocol::{LookSpec, Request, Response};
use super::{config, control, find_session, print_json, resolve_name, session_file};

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
/// offline one, so that is who is meant; an offline agent standing in as
/// another identity is named by its DID.
fn socket_for(account: Option<&str>) -> Result<std::path::PathBuf, String> {
    control::socket_path(&agent_did(account)?)
}

/// The DID of the agent `account` names - see [`socket_for`].
fn agent_did(account: Option<&str>) -> Result<String, String> {
    match find_session(account) {
        Ok((_, session)) => Ok(session.did),
        Err(_) if account.is_none() && !has_saved_sessions() => {
            Ok(config::agent::OFFLINE_DID.to_owned())
        }
        Err(_) if account.is_some_and(|name| name.trim().starts_with("did:")) => {
            Ok(account.unwrap_or_default().trim().to_owned())
        }
        Err(e) => Err(e),
    }
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
    let request = Request::WalkTo {
        x: args.x,
        z: args.z,
        run: args.run,
    };
    move_and_wait(args.account.name.as_deref(), &request, args.wait)
}

/// Follow a player - by DID, or a handle looked up here - and with `--wait`,
/// wait for the follow to end.
pub(super) fn follow(args: FollowArgs) -> Result<ExitCode, String> {
    let (did, _) = resolve_name(&args.peer)?;
    let request = Request::Follow {
        did,
        distance: Some(args.distance),
        run: args.run,
    };
    move_and_wait(args.account.name.as_deref(), &request, args.wait)
}

/// Turn toward a point (two numbers) or a player (anything else).
pub(super) fn face(args: FaceArgs) -> Result<ExitCode, String> {
    let request = match args.target.as_slice() {
        [x, z] => {
            let number = |raw: &str| {
                raw.trim()
                    .parse::<f32>()
                    .map_err(|_| format!("{raw:?} is not a number of metres"))
            };
            Request::Face {
                did: None,
                at: Some([number(x)?, number(z)?]),
            }
        }
        [player] => Request::Face {
            did: Some(resolve_name(player)?.0),
            at: None,
        },
        _ => return Err("face takes a player, or a point's x and z".to_owned()),
    };
    move_and_wait(args.account.name.as_deref(), &request, args.wait)
}

/// Start a movement; with `wait`, follow the event log from the moment it
/// started until its `movement_ended` arrives, and print that instead. A
/// movement that had nothing to do (a turn already facing) has no goal to
/// wait for, and its answer is printed as it is.
fn move_and_wait(account: Option<&str>, request: &Request, wait: bool) -> Result<ExitCode, String> {
    let socket = socket_for(account)?;
    let started = control::client::call(&socket, request)?;
    let Some(result) = started.result.as_ref().filter(|_| wait && started.ok) else {
        return print_response(&started);
    };
    let Some(goal_id) = result["goal_id"].as_u64() else {
        return print_response(&started);
    };
    let since = result["events_seq"]
        .as_u64()
        .ok_or("the movement has no events_seq")?;
    let ended = wait_for_event(&socket, since, config::agent::WALK_WAIT, |event| {
        event["kind"] == "movement_ended" && event["goal_id"].as_u64() == Some(goal_id)
    })
    .map_err(|waited| {
        format!(
            "the movement had not ended after {} minutes; it goes on, and `agent halt` \
             stops it",
            waited.as_secs() / 60
        )
    })?;
    print_response(&Response::success(ended))
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

/// Take a picture. `--out` is made absolute here, against this command's own
/// directory: the daemon that writes the file was started from another.
pub(super) fn look(args: LookArgs) -> Result<ExitCode, String> {
    let out = args
        .out
        .map(|path| {
            std::path::absolute(&path).map_err(|e| format!("--out {}: {e}", path.display()))
        })
        .transpose()?;
    let at = args.at.as_deref().map(|at| [at[0], at[1]]);
    ask(
        args.account.name.as_deref(),
        Request::Look(LookSpec {
            view: args.view,
            heading_deg: args.heading,
            at,
            out,
        }),
    )
}

/// Whose world `to` names, as a DID, and what the operator called it.
fn destination(account: Option<&str>, to: &str) -> Result<(String, Option<String>), String> {
    let to = to.trim();
    if to.eq_ignore_ascii_case("home") {
        return Ok((agent_did(account)?, Some("home".to_owned())));
    }
    let (did, handle) = resolve_name(to)?;
    Ok((did, handle.map(|handle| format!("@{handle}"))))
}
