//! `agent save` (#1422): the agent's world or avatar, saved to its account
//! through the pipeline the editors' Save buttons use.
//!
//! Refused unless the operator allowed saving when the agent was started;
//! then refused for the Save button's own reasons, in its own words - a
//! save already under way, an expired session, a record too big to write,
//! nothing to save - and, where the editor would stop to ask a person, for
//! a record whose saved copy could not be read when the agent arrived:
//! saving over it could overwrite work the agent never saw.
//!
//! A save is a round trip to the account's server, so the answer says only
//! that it started; how it ends arrives as a `saved` or `save_failed`
//! event, which `agent save --wait` waits for.

use bevy::ecs::world::CommandQueue;
use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde_json::{Value, json};

use crate::oauth::OauthRefreshCtx;
use crate::pds::avatar::avatar_is_dirty;
use crate::pds::record_size::{SizeClass, SizeReadout};
use crate::pds::{AvatarRecord, InventoryRecord, RoomRecord};
use crate::state::{
    AvatarRecordRecovery, CurrentRoomDid, InventoryRecordRecovery, LiveAvatarRecord,
    LiveInventoryRecord, LiveRoomRecord, PublishFeedback, PublishStatus, RoomRecordRecovery,
    StoredAvatarRecord, StoredInventoryRecord, StoredRoomRecord, records_differ,
};
use crate::ui::editable::save_refusal;

use super::super::super::control::events::EventKind;
use super::super::super::control::protocol::EditRecord;
use super::super::observe::EventSink;
use super::EditProfile;

/// Start saving `record` to the agent's account.
pub(in crate::agent::daemon) fn save(
    world: &mut World,
    record: EditRecord,
) -> Result<Value, String> {
    let profile = world
        .get_resource::<EditProfile>()
        .copied()
        .unwrap_or_default();
    if let Some(why) = profile.save_refused() {
        return Err(why.to_owned());
    }
    let size = match record {
        EditRecord::Room => save_room(world)?,
        EditRecord::Avatar => save_avatar(world)?,
        EditRecord::Inventory => save_inventory(world)?,
    };
    let events_seq = world.resource::<EventSink>().0.last_seq();
    Ok(json!({
        "saving": record.word(),
        "events_seq": events_seq,
        "largest_record": size.largest,
        "largest_bytes": size.bytes,
        "over_soft_budget": size.class() == Some(SizeClass::OverSoftBudget),
    }))
}

fn save_room(world: &mut World) -> Result<SizeReadout, String> {
    super::own_room(world)?;
    if let Some(recovery) = world.get_resource::<RoomRecordRecovery>() {
        return Err(recovery_refusal("world", &recovery.reason));
    }
    let saving = super::room_saving(world);
    let (Some(live), Some(stored)) = (
        world.get_resource::<LiveRoomRecord>(),
        world.get_resource::<StoredRoomRecord>(),
    ) else {
        return Err("the agent's world has no record yet".to_owned());
    };
    let dirty = records_differ(&live.0, &stored.0);
    let size = crate::pds::room::measure_publish(&live.0);
    let status = &world.resource::<PublishFeedback<RoomRecord>>().status;
    if let Some(why) = save_refusal(dirty, true, &size, saving, status) {
        return Err(why);
    }
    let record = live.0.clone();
    let room_did = world
        .get_resource::<CurrentRoomDid>()
        .map(|room| room.0.clone())
        .unwrap_or_default();
    let now = world.resource::<Time>().elapsed_secs_f64();
    with_session(world, |commands, session, refresh| {
        crate::ui::room::spawn_room_publish_task(commands, session, refresh, record, room_did, now);
    })?;
    world.resource_mut::<PublishFeedback<RoomRecord>>().status =
        PublishStatus::Publishing { since_secs: now };
    Ok(size)
}

fn save_avatar(world: &mut World) -> Result<SizeReadout, String> {
    super::in_world(world)?;
    if let Some(recovery) = world.get_resource::<AvatarRecordRecovery>() {
        return Err(recovery_refusal("avatar", &recovery.reason));
    }
    let saving = super::avatar_saving(world);
    let (Some(live), Some(stored)) = (
        world.get_resource::<LiveAvatarRecord>(),
        world.get_resource::<StoredAvatarRecord>(),
    ) else {
        return Err("the agent has no avatar record yet".to_owned());
    };
    let dirty = avatar_is_dirty(&live.0, &stored.0);
    let size = crate::pds::avatar::wardrobe::measure_publish(&live.0);
    let status = &world.resource::<PublishFeedback<AvatarRecord>>().status;
    if let Some(why) = save_refusal(dirty, true, &size, saving, status) {
        return Err(why);
    }
    let record = live.0.clone();
    // What the save takes off is what the saved avatar wears and this one
    // does not, as the editor's own Save derives it (#1110).
    let stored_attachments = crate::pds::avatar::wardrobe::attachment_rkeys(&stored.0);
    let now = world.resource::<Time>().elapsed_secs_f64();
    with_session(world, |commands, session, refresh| {
        crate::ui::avatar::spawn_publish_avatar_task(
            commands,
            session,
            refresh,
            record,
            stored_attachments,
            now,
        );
    })?;
    world.resource_mut::<PublishFeedback<AvatarRecord>>().status =
        PublishStatus::Publishing { since_secs: now };
    Ok(size)
}

fn save_inventory(world: &mut World) -> Result<SizeReadout, String> {
    super::in_world(world)?;
    if let Some(recovery) = world.get_resource::<InventoryRecordRecovery>() {
        return Err(recovery_refusal("inventory", &recovery.reason));
    }
    let saving = super::inventory_saving(world);
    let (Some(live), Some(stored)) = (
        world.get_resource::<LiveInventoryRecord>(),
        world.get_resource::<StoredInventoryRecord>(),
    ) else {
        return Err("the agent's inventory has not loaded yet".to_owned());
    };
    // The Inventory window's Save stands down past the cap too (#841).
    let cap = crate::config::state::MAX_INVENTORY_ITEMS;
    if live.0.generators.len() > cap {
        return Err(format!(
            "the inventory holds {} items, past the {cap} it may save; `agent unstash` some",
            live.0.generators.len()
        ));
    }
    let dirty = records_differ(&live.0, &stored.0);
    let size = crate::pds::inventory::measure_publish(&live.0);
    let status = &world.resource::<PublishFeedback<InventoryRecord>>().status;
    if let Some(why) = save_refusal(dirty, true, &size, saving, status) {
        return Err(why);
    }
    let (record, stored) = (live.0.clone(), stored.0.clone());
    let now = world.resource::<Time>().elapsed_secs_f64();
    with_session(world, |commands, session, refresh| {
        crate::ui::inventory::spawn_publish_inventory_task(
            commands, session, refresh, record, stored, now,
        );
    })?;
    world
        .resource_mut::<PublishFeedback<InventoryRecord>>()
        .status = PublishStatus::Publishing { since_secs: now };
    Ok(size)
}

/// Run `spawn` with the session a save writes as, and apply what it
/// spawned.
fn with_session(
    world: &mut World,
    spawn: impl FnOnce(&mut Commands, &AtprotoSession, &OauthRefreshCtx),
) -> Result<(), String> {
    let mut queue = CommandQueue::default();
    {
        let (Some(session), Some(refresh)) = (
            world.get_resource::<AtprotoSession>(),
            world.get_resource::<OauthRefreshCtx>(),
        ) else {
            return Err("the agent's session cannot write right now".to_owned());
        };
        let mut commands = Commands::new(&mut queue, world);
        spawn(&mut commands, session, refresh);
    }
    queue.apply(world);
    Ok(())
}

/// Why a record whose saved copy never loaded is not saved over. The
/// editor asks a person first; nobody is here to ask.
fn recovery_refusal(what: &str, reason: &str) -> String {
    format!(
        "the agent's saved {what} could not be read when it arrived ({reason}), so saving \
         now could overwrite it with what the agent has instead; a person can check it \
         in the game's editor"
    )
}

/// Each save that ends - the agent's own, or the one a trip made before
/// leaving - becomes a `saved` or `save_failed` event, read off the status
/// line the editors' Save rows show. A status only moves on a frame its
/// resource changed in, so a frame where neither did is not looked at.
pub(in crate::agent::daemon) fn record_saves(
    room: Option<Res<PublishFeedback<RoomRecord>>>,
    avatar: Option<Res<PublishFeedback<AvatarRecord>>>,
    inventory: Option<Res<PublishFeedback<InventoryRecord>>>,
    mut last: Local<[PublishStatus; 3]>,
    sink: Res<EventSink>,
) {
    let room_moved = room.as_ref().is_some_and(|feedback| feedback.is_changed());
    let avatar_moved = avatar
        .as_ref()
        .is_some_and(|feedback| feedback.is_changed());
    let inventory_moved = inventory
        .as_ref()
        .is_some_and(|feedback| feedback.is_changed());
    if !room_moved && !avatar_moved && !inventory_moved {
        return;
    }
    let statuses = [
        (EditRecord::Room, room.map(|f| f.status.clone())),
        (EditRecord::Avatar, avatar.map(|f| f.status.clone())),
        (EditRecord::Inventory, inventory.map(|f| f.status.clone())),
    ];
    for ((record, status), last) in statuses.into_iter().zip(last.iter_mut()) {
        let Some(status) = status.filter(|status| status != last) else {
            continue;
        };
        match &status {
            PublishStatus::Success { .. } => sink.0.push(EventKind::Saved {
                record: record.word().to_owned(),
            }),
            PublishStatus::Failed {
                message, terminal, ..
            } => sink.0.push(EventKind::SaveFailed {
                record: record.word().to_owned(),
                reason: message.clone(),
                terminal: *terminal,
            }),
            PublishStatus::Idle | PublishStatus::Publishing { .. } => {}
        }
        *last = status;
    }
}

#[cfg(test)]
mod tests {
    use super::super::harness::{OTHER, UNRESOLVABLE, app_as, app_in, placeable_slug};
    use super::*;
    use crate::agent::control::events::EventLog;
    use crate::agent::control::protocol::EditRequest;
    use crate::ui::room::PublishRoomTask;

    fn place(world: &mut World) {
        super::super::answer(
            world,
            EditRequest::Place {
                slug: placeable_slug().to_owned(),
                at: Some([3.0, 4.0]),
                yaw_deg: None,
            },
        )
        .expect("placed");
    }

    fn published(app: &mut App) -> Vec<RoomRecord> {
        app.world_mut()
            .query::<&PublishRoomTask>()
            .iter(app.world())
            .map(|task| task.published.clone())
            .collect()
    }

    /// What the operator did not allow is refused before anything else is
    /// looked at: offline there is no account, and without --allow-save
    /// the agent may not write to it.
    #[test]
    fn a_save_the_operator_did_not_allow_is_refused() {
        for (profile, says) in [
            (
                EditProfile {
                    allow_save: false,
                    offline: false,
                },
                "--allow-save",
            ),
            (
                EditProfile {
                    allow_save: true,
                    offline: true,
                },
                "offline",
            ),
        ] {
            // A save that got past the gate would start, with nowhere to go.
            bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default);
            let (mut app, _) = app_as(UNRESOLVABLE, UNRESOLVABLE);
            place(app.world_mut());
            app.world_mut().insert_resource(profile);
            for record in [EditRecord::Room, EditRecord::Avatar] {
                let why = save(app.world_mut(), record).expect_err("refused");
                assert!(why.contains(says), "{why}");
            }
            assert!(published(&mut app).is_empty());
        }
    }

    /// THE SEQUENCE: an edit, a save - which writes the live record and
    /// says so on the Save row's own status - and a second save while the
    /// first is under way, refused in the Save button's words.
    #[test]
    fn a_save_writes_the_live_record_once() {
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        let (mut app, _) = app_as(UNRESOLVABLE, UNRESOLVABLE);
        let nothing = save(app.world_mut(), EditRecord::Room).expect_err("refused");
        assert!(nothing.contains("nothing to save"), "{nothing}");
        place(app.world_mut());

        let started = save(app.world_mut(), EditRecord::Room).expect("saving");

        assert_eq!(started["saving"], "room");
        let live = app.world().resource::<LiveRoomRecord>().0.clone();
        let written = published(&mut app);
        assert_eq!(written.len(), 1);
        assert!(!records_differ(&written[0], &live));
        assert!(matches!(
            app.world().resource::<PublishFeedback<RoomRecord>>().status,
            PublishStatus::Publishing { .. }
        ));
        let again = save(app.world_mut(), EditRecord::Room).expect_err("refused");
        assert!(again.contains("already in flight"), "{again}");
    }

    /// Where the editor would stop to ask a person - a saved record that
    /// could not be read on arrival - the agent does not save over it.
    #[test]
    fn a_record_that_never_loaded_is_not_saved_over() {
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        let (mut app, _) = app_as(UNRESOLVABLE, UNRESOLVABLE);
        place(app.world_mut());
        app.world_mut().insert_resource(RoomRecordRecovery {
            cause: crate::state::RecoveryCause::Unreachable,
            reason: "the fetch failed".into(),
        });

        let why = save(app.world_mut(), EditRecord::Room).expect_err("refused");

        assert!(
            why.contains("the fetch failed") && why.contains("overwrite"),
            "{why}"
        );
        assert!(published(&mut app).is_empty());
    }

    /// A save writes its saver's own account whatever world they stand in,
    /// so a world that is not the agent's is never saved.
    #[test]
    fn a_world_that_is_not_the_agents_is_never_saved() {
        let (mut app, _) = app_in(OTHER);
        let why = save(app.world_mut(), EditRecord::Room).expect_err("refused");
        assert!(why.contains("not the agent's world"), "{why}");
    }

    /// Each save's end is one event: landing, or failing - with whether a
    /// retry can work - and a status that does not change says nothing.
    #[test]
    fn each_save_that_ends_is_one_event() {
        let log = std::sync::Arc::new(EventLog::new(16, "test".into()));
        let mut app = App::new();
        app.insert_resource(EventSink(std::sync::Arc::clone(&log)))
            .init_resource::<PublishFeedback<RoomRecord>>()
            .init_resource::<PublishFeedback<AvatarRecord>>()
            .init_resource::<PublishFeedback<InventoryRecord>>()
            .add_systems(Update, record_saves);
        let set = |app: &mut App, status: PublishStatus| {
            app.world_mut()
                .resource_mut::<PublishFeedback<RoomRecord>>()
                .status = status;
            app.update();
            app.update();
        };

        set(&mut app, PublishStatus::Publishing { since_secs: 1.0 });
        set(&mut app, PublishStatus::Success { at_secs: 2.0 });
        set(&mut app, PublishStatus::Publishing { since_secs: 3.0 });
        set(
            &mut app,
            PublishStatus::Failed {
                at_secs: 4.0,
                message: "refused".into(),
                terminal: true,
            },
        );
        app.world_mut()
            .resource_mut::<PublishFeedback<AvatarRecord>>()
            .status = PublishStatus::Success { at_secs: 5.0 };
        app.update();
        app.world_mut()
            .resource_mut::<PublishFeedback<InventoryRecord>>()
            .status = PublishStatus::Success { at_secs: 6.0 };
        app.update();

        let events: Vec<EventKind> = log
            .after(0, std::time::Duration::ZERO)
            .events
            .into_iter()
            .map(|e| e.what)
            .collect();
        assert_eq!(
            events,
            [
                EventKind::Saved {
                    record: "room".into()
                },
                EventKind::SaveFailed {
                    record: "room".into(),
                    reason: "refused".into(),
                    terminal: true
                },
                EventKind::Saved {
                    record: "avatar".into()
                },
                EventKind::Saved {
                    record: "inventory".into()
                },
            ]
        );
    }
}
