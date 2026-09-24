//! `agent travel` (#1419): the agent goes to another player's world through
//! the door a person uses - the unsaved-edits guard - rather than around it.
//!
//! The guard proceeds by itself when nothing unsaved stands to be lost,
//! which is every trip an agent that has not edited its world makes. With
//! unsaved edits it would stop to ask, and nobody is there to answer the
//! dialog it draws - so the agent is asked here instead, before the trip
//! starts (#1422): unless told what to do with them, the trip is refused and
//! says so; told to discard them, it drops them as "Discard & travel" does;
//! told to save them, it saves as "Save & travel" does, and the guard waits
//! for the save to land before it leaves. Should the guard stop to ask all
//! the same - a save before leaving that failed - the trip is withdrawn
//! after a moment, and the agent told why.

use bevy::prelude::*;
use serde_json::{Value, json};

use crate::config::agent::GUARD_WAIT_SECS;
use crate::state::{AppState, CurrentRoomDid, TravelingTo};
use crate::ui::unsaved_guard::{GuardNotice, GuardPhase, GuardedAction, TravelVia, UnsavedGuard};

use super::super::control::events::EventKind;
use super::super::control::protocol::{EditRecord, UnsavedEdits};
use super::edit;
use super::observe::EventSink;

/// Start travelling to `room_did`'s world. `label` is what the operator
/// called it, for the world's name while the trip is under way; `unsaved`
/// is what to do with unsaved edits to the agent's own world, which leaving
/// it would lose.
pub(super) fn travel(
    world: &mut World,
    room_did: String,
    label: Option<String>,
    unsaved: UnsavedEdits,
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
    let edits = if edit::room_unsaved(world) {
        match unsaved {
            UnsavedEdits::Refuse => {
                return Err(concat!(
                    "the agent's world has unsaved edits, and leaving would lose them: ",
                    "travel with --save-edits or --discard-edits, or first `agent save` ",
                    "or `agent revert` them"
                )
                .to_owned());
            }
            UnsavedEdits::Discard => {
                edit::revert(world, EditRecord::Room)?;
                "discarded"
            }
            UnsavedEdits::Save => {
                edit::save(world, EditRecord::Room)?;
                "saving"
            }
        }
    } else {
        "none"
    };
    world.insert_resource(UnsavedGuard::new(GuardedAction::PortalTravel {
        // A destination chosen by name, as from the account menu.
        via: TravelVia::Menu,
        target_did: room_did.clone(),
        target_label: label,
        target_pos: None,
    }));
    let events_seq = world.resource::<EventSink>().0.last_seq();
    Ok(json!({
        "travelling_to": room_did,
        "events_seq": events_seq,
        "unsaved_edits": edits,
    }))
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
    let reason = match &guard.notice {
        Some(GuardNotice::PublishFailed(error)) => {
            format!("saving the agent's world before leaving failed ({error}), so it stayed")
        }
        _ => "the agent's world has unsaved edits, and leaving would lose them".to_owned(),
    };
    sink.0.push(EventKind::TravelFailed {
        to_did: target_did.clone(),
        reason,
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

#[cfg(test)]
mod unsaved_edit_tests {
    use super::super::edit::harness::{AGENT, OTHER, UNRESOLVABLE, app_as, app_in, placeable_slug};
    use super::*;
    use crate::agent::control::events::EventLog;
    use crate::agent::control::protocol::EditRequest;
    use crate::state::{LiveRoomRecord, StoredRoomRecord, records_differ};

    fn edited(agent: &str) -> App {
        let (mut app, _) = app_as(agent, agent);
        super::super::edit::answer(
            app.world_mut(),
            EditRequest::Place {
                slug: placeable_slug().to_owned(),
                at: Some([3.0, 4.0]),
                yaw_deg: None,
            },
        )
        .expect("placed");
        app
    }

    fn go(app: &mut App, unsaved: UnsavedEdits) -> Result<Value, String> {
        travel(app.world_mut(), OTHER.into(), None, unsaved)
    }

    fn clean(app: &App) -> bool {
        let world = app.world();
        !records_differ(
            &world.resource::<LiveRoomRecord>().0,
            &world.resource::<StoredRoomRecord>().0,
        )
    }

    /// Unsaved edits to its own world keep the agent there unless it is
    /// told what to do with them; the answer says both ways out. A world
    /// with nothing unsaved is left as ever.
    #[test]
    fn unsaved_edits_refuse_the_trip_unless_told() {
        let mut app = edited(AGENT);

        let why = go(&mut app, UnsavedEdits::Refuse).expect_err("refused");

        assert!(
            why.contains("--save-edits") && why.contains("--discard-edits"),
            "{why}"
        );
        assert!(!why.contains("  "), "one sentence, spaced once: {why}");
        assert!(!app.world().contains_resource::<UnsavedGuard>());
        let (mut untouched, _) = app_in(AGENT);
        let went = go(&mut untouched, UnsavedEdits::Refuse).expect("travelling");
        assert_eq!(went["unsaved_edits"], "none");
        assert!(untouched.world().contains_resource::<UnsavedGuard>());
    }

    /// Told to discard them, the agent drops them as "Discard & travel"
    /// does - back to the saved world - and leaves through the guard.
    #[test]
    fn told_to_discard_them_it_reverts_and_goes() {
        let mut app = edited(AGENT);

        let went = go(&mut app, UnsavedEdits::Discard).expect("travelling");

        assert_eq!(went["unsaved_edits"], "discarded");
        assert!(clean(&app));
        assert!(app.world().contains_resource::<UnsavedGuard>());
    }

    /// Told to save them, the agent saves first - only if it may - and
    /// the guard leaves once the save has landed.
    #[test]
    fn told_to_save_them_it_saves_first_if_it_may() {
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        let mut refused = edited(UNRESOLVABLE);
        refused
            .world_mut()
            .insert_resource(super::super::edit::EditProfile {
                allow_save: false,
                offline: false,
            });
        let why = go(&mut refused, UnsavedEdits::Save).expect_err("refused");
        assert!(why.contains("--allow-save"), "{why}");
        assert!(!refused.world().contains_resource::<UnsavedGuard>());

        let mut app = edited(UNRESOLVABLE);
        let went = go(&mut app, UnsavedEdits::Save).expect("travelling");

        assert_eq!(went["unsaved_edits"], "saving");
        let saves = app
            .world_mut()
            .query::<&crate::ui::room::PublishRoomTask>()
            .iter(app.world())
            .count();
        assert_eq!(saves, 1);
        assert!(app.world().contains_resource::<UnsavedGuard>());
    }

    /// A save before leaving that failed brings the guard back to its
    /// question; the trip is withdrawn, and says the save is why.
    #[test]
    fn a_failed_save_before_leaving_is_the_reason_given() {
        let log = std::sync::Arc::new(EventLog::new(16, "test".into()));
        let mut app = App::new();
        app.insert_resource(EventSink(std::sync::Arc::clone(&log)))
            .init_resource::<Time<Real>>()
            .add_systems(Update, withdraw_unanswered_guard);
        let mut guard = UnsavedGuard::new(GuardedAction::PortalTravel {
            via: TravelVia::Menu,
            target_did: OTHER.into(),
            target_label: None,
            target_pos: None,
        });
        guard.notice = Some(GuardNotice::PublishFailed("the PDS said no".into()));
        app.world_mut().insert_resource(guard);
        app.update();
        let mut time = app.world_mut().resource_mut::<Time<Real>>();
        // A real clock's first reading only starts it; the second moves it.
        let start = time.startup();
        time.update_with_instant(start);
        time.update_with_instant(start + std::time::Duration::from_secs_f64(GUARD_WAIT_SECS + 1.0));
        app.update();

        let events: Vec<EventKind> = log
            .after(0, std::time::Duration::ZERO)
            .events
            .into_iter()
            .map(|e| e.what)
            .collect();
        assert!(
            matches!(
                events.as_slice(),
                [EventKind::TravelFailed { reason, .. }] if reason.contains("the PDS said no")
            ),
            "{events:?}"
        );
        assert!(!app.world().contains_resource::<UnsavedGuard>());
    }
}
