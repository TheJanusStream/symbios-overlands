//! The commands that talk to a running agent (#1416-#1422): each sends one
//! request over the control socket and prints the answer; `walk-to --wait`,
//! `travel --wait` and `save --wait` then follow the event log to the end of
//! what they started.

use std::process::ExitCode;

use super::cli::{
    CatalogueArgs, EventsArgs, FaceArgs, FollowArgs, GiftAction, GiftArgs, JsonAction, LookArgs,
    MoveArgs, PlaceArgs, PlacementsArgs, RecordJsonArgs, RemoveArgs, SaveArgs, TravelArgs,
    UiAction, UiArgs, WalkToArgs,
};
use super::control::protocol::{LookSpec, Request, Response, UnsavedEdits};
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
    let unsaved = if args.discard_edits {
        UnsavedEdits::Discard
    } else if args.save_edits {
        UnsavedEdits::Save
    } else {
        UnsavedEdits::Refuse
    };
    let started = control::client::call(
        &socket,
        &Request::Travel {
            room_did: room_did.clone(),
            label,
            unsaved,
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

/// The placements in the agent's world.
pub(super) fn placements(args: PlacementsArgs) -> Result<ExitCode, String> {
    ask(
        args.account.name.as_deref(),
        Request::Placements {
            within_m: args.within,
        },
    )
}

/// The catalogue, or the entries matching a search.
pub(super) fn catalogue(args: CatalogueArgs) -> Result<ExitCode, String> {
    ask(
        args.account.name.as_deref(),
        Request::Catalogue {
            search: args.search,
        },
    )
}

pub(super) fn place(args: PlaceArgs) -> Result<ExitCode, String> {
    ask(
        args.account.name.as_deref(),
        Request::Place {
            slug: args.slug,
            at: args.at.as_deref().map(|at| [at[0], at[1]]),
            yaw_deg: args.yaw,
        },
    )
}

pub(super) fn move_placement(args: MoveArgs) -> Result<ExitCode, String> {
    ask(
        args.account.name.as_deref(),
        Request::Move {
            index: args.index,
            x: args.x,
            z: args.z,
            yaw_deg: args.yaw,
        },
    )
}

pub(super) fn remove(args: RemoveArgs) -> Result<ExitCode, String> {
    ask(
        args.account.name.as_deref(),
        Request::Remove { index: args.index },
    )
}

/// The records with JSON commands.
#[derive(Clone, Copy, Debug)]
pub(super) enum JsonRecord {
    Room,
    Avatar,
}

/// `room get|set` and `avatar get|set`. A value is JSON - given on the
/// command line, or read from a file here - and goes to the agent as JSON,
/// so a value that is not JSON never leaves this command.
pub(super) fn record_json(record: JsonRecord, args: RecordJsonArgs) -> Result<ExitCode, String> {
    match args.action {
        JsonAction::Get(get) => {
            let pointer = get.pointer.unwrap_or_default();
            let request = match record {
                JsonRecord::Room => Request::RoomGet { pointer },
                JsonRecord::Avatar => Request::AvatarGet { pointer },
            };
            ask(get.account.name.as_deref(), request)
        }
        JsonAction::Set(set) => {
            let text = match (set.value, set.file) {
                (Some(value), _) => value,
                (None, Some(path)) => std::fs::read_to_string(&path)
                    .map_err(|e| format!("{}: {e}", path.display()))?,
                (None, None) => return Err("set takes a value, or --file".to_owned()),
            };
            let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
                format!("the value is not JSON ({e}); a string needs its quotes, as in '\"text\"'")
            })?;
            let pointer = set.pointer;
            let request = match record {
                JsonRecord::Room => Request::RoomSet { pointer, value },
                JsonRecord::Avatar => Request::AvatarSet { pointer, value },
            };
            ask(set.account.name.as_deref(), request)
        }
    }
}

/// Start a save; with `--wait`, follow the event log from the moment it
/// started until it lands or fails, and print that.
pub(super) fn save(args: SaveArgs) -> Result<ExitCode, String> {
    let record = args.record.record;
    let socket = socket_for(args.record.account.name.as_deref())?;
    let started = control::client::call(&socket, &Request::Save { record })?;
    let Some(result) = started.result.as_ref().filter(|_| args.wait && started.ok) else {
        return print_response(&started);
    };
    let since = result["events_seq"]
        .as_u64()
        .ok_or("the save has no events_seq")?;
    let ended = wait_for_event(&socket, since, config::agent::SAVE_WAIT, |event| {
        (event["kind"] == "saved" || event["kind"] == "save_failed")
            && event["record"] == record.word()
    })
    .map_err(|waited| {
        format!(
            "the save had not landed after {} s; it may yet, and `agent status` says",
            waited.as_secs()
        )
    })?;
    print_response(&save_ended(ended))
}

/// What `save --wait` prints once the save has ended, wrapped as every
/// other `--wait` wraps its ending (#1442): a `saved` event as the result;
/// a `save_failed` one as a refusal, its reason the error - and the event
/// still the result, so `result.kind` reads the same either way. A refusal
/// exits non-zero, as a failed save always has.
fn save_ended(event: serde_json::Value) -> Response {
    if event["kind"] != "save_failed" {
        return Response::success(event);
    }
    let reason = event["reason"]
        .as_str()
        .unwrap_or("the save did not land")
        .to_owned();
    Response {
        ok: false,
        result: Some(event),
        error: Some(reason),
    }
}

/// `gift give|accept|decline`. A gift goes to a player named by DID or by
/// handle, looked up here; with `--wait` its answer is followed on the event
/// log - a lapsed offer answers too, so the wait ends.
pub(super) fn gift(args: GiftArgs) -> Result<ExitCode, String> {
    match args.action {
        GiftAction::Give(give) => {
            let (to_did, _) = resolve_name(&give.player)?;
            let socket = socket_for(give.account.name.as_deref())?;
            let started = control::client::call(
                &socket,
                &Request::GiftGive {
                    to_did,
                    item: give.item,
                },
            )?;
            let Some(result) = started.result.as_ref().filter(|_| give.wait && started.ok) else {
                return print_response(&started);
            };
            let (Some(offer_id), Some(since)) =
                (result["offered"].as_u64(), result["events_seq"].as_u64())
            else {
                return print_response(&started);
            };
            let answered = wait_for_event(&socket, since, config::agent::GIFT_WAIT, |event| {
                event["kind"] == "gift_answered" && event["offer_id"].as_u64() == Some(offer_id)
            })
            .map_err(|waited| {
                format!(
                    "no answer to the gift after {} s; `agent events` says when it comes",
                    waited.as_secs()
                )
            })?;
            print_response(&Response::success(answered))
        }
        GiftAction::Accept(offer) => ask(
            offer.account.name.as_deref(),
            Request::GiftAccept {
                offer_id: offer.offer_id,
            },
        ),
        GiftAction::Decline(offer) => ask(
            offer.account.name.as_deref(),
            Request::GiftDecline {
                offer_id: offer.offer_id,
            },
        ),
    }
}

/// `ui` and its verbs (#1424): each one request, answered once the
/// interface has done it.
pub(super) fn ui(args: UiArgs) -> Result<ExitCode, String> {
    let request = match args.action {
        None => Request::Ui {
            picture: args.picture,
        },
        Some(_) if args.picture => {
            return Err(
                "--picture goes with `agent ui` alone, or `agent ui show <window> --picture`"
                    .to_owned(),
            );
        }
        Some(UiAction::Show { window, picture }) => Request::UiShow { window, picture },
        Some(UiAction::Open { window }) => Request::UiOpen { window },
        Some(UiAction::Close { window }) => Request::UiClose { window },
        Some(UiAction::Click { path }) => Request::UiClick { path },
        Some(UiAction::Type { path, text, enter }) => Request::UiType { path, text, enter },
        Some(UiAction::Set { path, value }) => Request::UiSet { path, value },
        Some(UiAction::Choose { path, option }) => Request::UiChoose { path, option },
        Some(UiAction::Scroll { window, points }) => Request::UiScroll { window, points },
    };
    ask(args.account.as_deref(), request)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::save_ended;

    /// `save --wait` prints its ending as `walk-to --wait` and the rest do
    /// (#1442), where it used to print the bare event: an agent reading
    /// `result` met a missing key on this one command alone.
    #[test]
    fn a_saves_ending_is_wrapped_like_every_other_wait() {
        let saved = json!({ "kind": "saved", "record": "avatar", "seq": 6, "at": 1 });
        let failed = json!({
            "kind": "save_failed",
            "record": "room",
            "reason": "the record is past its ceiling",
            "terminal": false,
            "seq": 7,
            "at": 2,
        });

        let landed = serde_json::to_value(save_ended(saved.clone())).unwrap();
        let refused = serde_json::to_value(save_ended(failed.clone())).unwrap();

        assert_eq!(landed, json!({ "ok": true, "result": saved }));
        assert_eq!(
            refused,
            json!({
                "ok": false,
                "result": failed,
                "error": "the record is past its ceiling",
            })
        );
    }
}
