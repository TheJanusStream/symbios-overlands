//! `agent travel` (#1419): the agent goes to another player's world through
//! the door a person uses - the unsaved-edits guard - rather than around it.
//!
//! The guard proceeds by itself when nothing unsaved stands to be lost,
//! which is every trip an agent that has not edited anything makes. When it
//! does stop to ask, nobody is there to answer the dialog it draws, so the
//! agent is told why the trip did not start instead of waiting forever.

use bevy::prelude::*;
use serde_json::{Value, json};

use crate::config::agent::GUARD_WAIT_SECS;
use crate::state::{AppState, CurrentRoomDid, TravelingTo};
use crate::ui::unsaved_guard::{GuardPhase, GuardedAction, TravelVia, UnsavedGuard};

use super::super::control::events::EventKind;
use super::observe::EventSink;

/// Start travelling to `room_did`'s world. `label` is what the operator
/// called it, for the world's name while the trip is under way.
pub(super) fn travel(
    world: &mut World,
    room_did: String,
    label: Option<String>,
) -> Result<Value, String> {
    if *world.resource::<State<AppState>>().get() != AppState::InGame {
        return Err("the agent is not in a world yet".to_owned());
    }
    if let Some(trip) = world.get_resource::<TravelingTo>() {
        return Err(format!(
            "the agent is already travelling to {}",
            trip.target_did
        ));
    }
    if world.contains_resource::<UnsavedGuard>() {
        return Err("the agent is already on its way somewhere".to_owned());
    }
    if world
        .get_resource::<CurrentRoomDid>()
        .is_some_and(|room| room.0 == room_did)
    {
        return Err("the agent is already in that world".to_owned());
    }
    world.insert_resource(UnsavedGuard::new(GuardedAction::PortalTravel {
        // A destination chosen by name, as from the account menu.
        via: TravelVia::Menu,
        target_did: room_did.clone(),
        target_label: label,
        target_pos: None,
    }));
    let events_seq = world.resource::<EventSink>().0.last_seq();
    Ok(json!({ "travelling_to": room_did, "events_seq": events_seq }))
}

/// Each trip ends as an arrival - the room is the destination - or as a
/// failure, when the travel marker goes with the agent still where it was.
///
/// A trip never passes through `Loading`, so the `OnEnter(InGame)` arrival
/// the sign-in raises does not fire for it; this is where a trip's arrival
/// is announced.
pub(super) fn record_travel(
    traveling: Option<Res<TravelingTo>>,
    room: Option<Res<CurrentRoomDid>>,
    mut underway: Local<Option<String>>,
    sink: Res<EventSink>,
) {
    match (traveling.as_deref(), underway.as_ref()) {
        (Some(trip), None) => *underway = Some(trip.target_did.clone()),
        (None, Some(target)) => {
            let target = target.clone();
            *underway = None;
            if room.is_some_and(|room| room.0 == target) {
                sink.0.push(EventKind::EnteredWorld { room_did: target });
            } else {
                sink.0.push(EventKind::TravelFailed {
                    to_did: target,
                    reason: "the destination could not be reached".to_owned(),
                });
            }
        }
        _ => {}
    }
}

/// A trip the guard has stopped to ask about - unsaved edits, or an edit
/// still publishing - is withdrawn after a moment, and the agent told why.
pub(super) fn withdraw_unanswered_guard(
    mut commands: Commands,
    guard: Option<Res<UnsavedGuard>>,
    time: Res<Time<Real>>,
    mut asking_since: Local<Option<f64>>,
    sink: Res<EventSink>,
) {
    let Some(guard) = guard else {
        *asking_since = None;
        return;
    };
    let GuardedAction::PortalTravel { target_did, .. } = &guard.action else {
        return;
    };
    let now = time.elapsed_secs_f64();
    let since = *asking_since.get_or_insert(now);
    if !unanswered(guard.phase, now - since) {
        return;
    }
    sink.0.push(EventKind::TravelFailed {
        to_did: target_did.clone(),
        reason: "the agent's world has unsaved edits, and leaving would lose them".to_owned(),
    });
    commands.remove_resource::<UnsavedGuard>();
    *asking_since = None;
}

/// Has the guard been asking, rather than publishing, for long enough that
/// nobody is going to answer? A publish the guard waits on is its own
/// progress and is left to finish.
fn unanswered(phase: GuardPhase, asking_for_secs: f64) -> bool {
    phase == GuardPhase::Prompt && asking_for_secs >= GUARD_WAIT_SECS
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;
    use crate::agent::control::events::EventLog;

    fn app() -> (App, Arc<EventLog>) {
        let log = Arc::new(EventLog::new(16, "test".into()));
        let mut app = App::new();
        app.insert_resource(EventSink(Arc::clone(&log)))
            .insert_resource(CurrentRoomDid("did:plc:home".into()))
            .add_systems(Update, record_travel);
        (app, log)
    }

    fn trip(to: &str) -> TravelingTo {
        TravelingTo {
            target_did: to.into(),
            target_pos: None,
            target_label: None,
            phase: crate::state::TravelPhase::Fetching,
        }
    }

    fn kinds(log: &EventLog) -> Vec<EventKind> {
        log.after(0, Duration::ZERO)
            .events
            .into_iter()
            .map(|e| e.what)
            .collect()
    }

    /// THE SEQUENCE of a trip that lands: the marker goes up, the room
    /// changes under it, the marker comes down. One arrival.
    #[test]
    fn a_trip_that_lands_is_an_arrival() {
        let (mut app, log) = app();
        app.world_mut().insert_resource(trip("did:plc:bob"));
        app.update();
        app.world_mut()
            .insert_resource(CurrentRoomDid("did:plc:bob".into()));
        app.update();
        app.world_mut().remove_resource::<TravelingTo>();
        app.update();

        assert_eq!(
            kinds(&log),
            [EventKind::EnteredWorld {
                room_did: "did:plc:bob".into()
            }]
        );
    }

    /// And one that does not: the marker comes down with the room unchanged.
    #[test]
    fn a_trip_that_comes_back_down_where_it_started_failed() {
        let (mut app, log) = app();
        app.world_mut().insert_resource(trip("did:plc:gone"));
        app.update();
        app.world_mut().remove_resource::<TravelingTo>();
        app.update();

        assert!(
            matches!(
                kinds(&log).as_slice(),
                [EventKind::TravelFailed { to_did, .. }] if to_did == "did:plc:gone"
            ),
            "{:?}",
            kinds(&log)
        );
    }

    #[test]
    fn only_a_question_left_unanswered_is_withdrawn() {
        assert!(!unanswered(GuardPhase::Prompt, 0.0));
        assert!(unanswered(GuardPhase::Prompt, GUARD_WAIT_SECS));
        assert!(
            !unanswered(GuardPhase::Publishing, 10.0 * GUARD_WAIT_SECS),
            "a publish under way is waited out"
        );
    }
}
