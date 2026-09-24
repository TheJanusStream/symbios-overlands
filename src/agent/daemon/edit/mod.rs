//! The agent's edit commands (#1422): its own world, thing by thing or as
//! JSON, and its avatar as JSON - undone, reverted and saved the way a
//! person's edits are.
//!
//! Every edit goes through the door the game's own editors write through:
//! the live record, changed once and sanitised. So the world recompiles
//! what changed, the change streams to everyone in the world as a person's
//! edit does, and the game's undo history takes one step for it - the same
//! history Ctrl+Z steps. Nothing an edit does reaches the agent's account
//! until it is saved, and saving is the operator's to allow (`agent start
//! --allow-save`); without it the agent edits freely, and what it changes
//! is gone when it stops.
//!
//! Three rules hold for every write:
//!
//! * **Only the agent's own world.** The game lets a world's owner edit it
//!   and nobody else, and a save writes to the saver's own account whatever
//!   world they stand in - so an edit or a save anywhere else is refused
//!   here, before anything is touched. The avatar goes where the agent
//!   goes, and is its own to edit anywhere.
//! * **What is written is what a save would write.** An edited record is
//!   passed through its wire form - where every decimal is a whole number
//!   of ten-thousandths, since atproto has no floats - and sanitised, as a
//!   record fetched from an account is. So the live record is what saving
//!   it and reading it back would give. The world compiler and the terrain
//!   compare wire forms, so the round trip itself rebuilds nothing.
//! * **One edit a frame.** The undo history records one step for each
//!   frame a record changed in, so the daemon answers at most one editing
//!   command a frame (`serve`), and each command is a step of its own.

mod avatar;
mod inventory;
mod json;
mod placements;
mod save;

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde_json::{Value, json};

use crate::pds::avatar::avatar_is_dirty;
use crate::pds::{AvatarRecord, RoomRecord};
use crate::state::{
    AppState, CurrentRoomDid, LiveAvatarRecord, LiveInventoryRecord, LiveRoomRecord,
    StoredAvatarRecord, StoredInventoryRecord, StoredRoomRecord, TravelingTo, records_differ,
};
use crate::ui::avatar::{AvatarEditorState, PublishAvatarTask};
use crate::ui::inventory::PublishInventoryTask;
use crate::ui::room::{PublishRoomTask, ResetRoomTask, RoomEditorState};
use crate::ui::undo::{
    AvatarUndoHistory, PendingUndoLabels, RoomUndoHistory, StepKind, step_avatar, step_room,
};

use super::super::control::protocol::{EditRecord, EditRequest};

pub(super) use save::{record_saves, save};

/// What the operator allowed when the agent was started (#1422).
#[derive(Resource, Clone, Copy, Debug, Default)]
pub(super) struct EditProfile {
    /// `--allow-save`: the agent may save its world and its avatar.
    pub allow_save: bool,
    /// The offline stand-in has no account to save to.
    pub offline: bool,
}

impl EditProfile {
    /// Why `save` is refused whatever is asked, if it is.
    pub(super) fn save_refused(self) -> Option<&'static str> {
        if self.offline {
            Some("the agent is offline, so there is no account to save to")
        } else if !self.allow_save {
            Some("the agent was started without --allow-save, so it may not save")
        } else {
            None
        }
    }
}

/// Answer one edit command.
pub(super) fn answer(world: &mut World, request: EditRequest) -> Result<Value, String> {
    match request {
        EditRequest::Placements { within_m } => placements::list(world, within_m),
        EditRequest::Catalogue { search } => Ok(placements::catalogue(search.as_deref())),
        EditRequest::Place { slug, at, yaw_deg } => placements::place(world, &slug, at, yaw_deg),
        EditRequest::Move {
            index,
            x,
            z,
            yaw_deg,
        } => placements::move_to(world, index, Vec2::new(x, z), yaw_deg),
        EditRequest::Remove { index } => placements::remove(world, index),
        EditRequest::Get {
            record: EditRecord::Room,
            pointer,
        } => json::room_get(world, &pointer),
        EditRequest::Set {
            record: EditRecord::Room,
            pointer,
            value,
        } => json::room_set(world, &pointer, value),
        EditRequest::Get {
            record: EditRecord::Avatar,
            pointer,
        } => avatar::get(world, &pointer),
        EditRequest::Set {
            record: EditRecord::Avatar,
            pointer,
            value,
        } => avatar::set(world, &pointer, value),
        EditRequest::Get {
            record: EditRecord::Inventory,
            ..
        }
        | EditRequest::Set {
            record: EditRecord::Inventory,
            ..
        } => Err(
            "the inventory is read with `agent inventory`, and changed with \
             `agent stash` and `agent unstash`"
                .to_owned(),
        ),
        EditRequest::Undo(record) => step(world, record, StepKind::Undo),
        EditRequest::Redo(record) => step(world, record, StepKind::Redo),
        EditRequest::Revert(record) => revert(world, record),
        EditRequest::Save(record) => save(world, record),
        EditRequest::Inventory => inventory::list(world),
        EditRequest::Stash { what } => inventory::stash(world, &what),
        EditRequest::Unstash { name } => inventory::unstash(world, &name),
        EditRequest::Wear { name } => inventory::wear(world, &name),
        EditRequest::TakeOff { name } => inventory::take_off(world, &name),
    }
}

/// Refused unless the agent is standing in a world - not signing in,
/// loading, or on its way somewhere.
fn in_world(world: &World) -> Result<(), String> {
    if *world.resource::<State<AppState>>().get() != AppState::InGame {
        return Err("the agent is not in a world yet".to_owned());
    }
    if world.contains_resource::<TravelingTo>() {
        return Err("the agent is travelling".to_owned());
    }
    Ok(())
}

/// Whether the world the agent stands in is its own: the only one the game
/// lets it edit, and the only one a save of a world can write.
pub(super) fn owns_room(world: &World) -> bool {
    match (
        world.get_resource::<AtprotoSession>(),
        world.get_resource::<CurrentRoomDid>(),
    ) {
        (Some(session), Some(room)) => session.did == room.0,
        _ => false,
    }
}

/// Refused unless the agent stands in its own world.
fn own_room(world: &World) -> Result<(), String> {
    in_world(world)?;
    if !owns_room(world) {
        return Err(
            "this is not the agent's world, and only a world's owner can edit it; \
             `agent travel home` goes back to its own"
                .to_owned(),
        );
    }
    Ok(())
}

/// A copy of the agent's own world's live record, to edit and hand to
/// [`write_room`].
fn room_for_edit(world: &World) -> Result<RoomRecord, String> {
    own_room(world)?;
    world
        .get_resource::<LiveRoomRecord>()
        .map(|live| live.0.clone())
        .ok_or_else(|| "the agent's world has no record yet".to_owned())
}

/// Make `record` the agent's world, as the World Editor makes an edit:
/// through its wire form and the sanitiser, as one change the undo history
/// names `label`. `false` when that changes nothing, and then nothing is
/// touched: a record merely borrowed mutably still recompiles, broadcasts
/// and takes an undo step.
fn write_room(world: &mut World, record: RoomRecord, label: String) -> Result<bool, String> {
    let record = settle_room(record)?;
    if !records_differ(&world.resource::<LiveRoomRecord>().0, &record) {
        return Ok(false);
    }
    world.resource_mut::<LiveRoomRecord>().0 = record;
    world.resource_mut::<PendingUndoLabels>().set_room(label);
    Ok(true)
}

/// `record` as saving it and reading it back would give: its wire form,
/// read back and sanitised as a record fetched from an account is.
fn settle_room(record: RoomRecord) -> Result<RoomRecord, String> {
    let wire = serde_json::to_value(&record)
        .map_err(|e| format!("the world's record does not serialise: {e}"))?;
    let mut settled: RoomRecord = serde_json::from_value(wire)
        .map_err(|e| format!("the world's record does not read back: {e}"))?;
    settled.sanitize();
    Ok(settled)
}

/// Make `record` the agent's avatar - see [`write_room`]. The avatar's
/// edits are its own undo history's.
fn write_avatar(world: &mut World, record: AvatarRecord, label: String) -> Result<bool, String> {
    if !avatar_is_dirty(&world.resource::<LiveAvatarRecord>().0, &record) {
        return Ok(false);
    }
    world.resource_mut::<LiveAvatarRecord>().0 = record;
    world.resource_mut::<PendingUndoLabels>().set_avatar(label);
    Ok(true)
}

/// Whether the agent's world has edits no save has taken: unsaved, and
/// not what a save now under way is writing. Only while it stands in its
/// own world - another's world is never the agent's to save.
pub(super) fn room_unsaved(world: &mut World) -> bool {
    if !owns_room(world) {
        return false;
    }
    let (Some(live), Some(stored)) = (
        world.get_resource::<LiveRoomRecord>(),
        world.get_resource::<StoredRoomRecord>(),
    ) else {
        return false;
    };
    if !records_differ(&live.0, &stored.0) {
        return false;
    }
    let live = live.0.clone();
    !world
        .query::<&PublishRoomTask>()
        .iter(world)
        .any(|task| !records_differ(&task.published, &live))
}

/// Whether the avatar has edits no save has taken - see [`room_unsaved`].
fn avatar_unsaved(world: &mut World) -> bool {
    let (Some(live), Some(stored)) = (
        world.get_resource::<LiveAvatarRecord>(),
        world.get_resource::<StoredAvatarRecord>(),
    ) else {
        return false;
    };
    if !avatar_is_dirty(&live.0, &stored.0) {
        return false;
    }
    let live = live.0.clone();
    !world
        .query::<&PublishAvatarTask>()
        .iter(world)
        .any(|task| !avatar_is_dirty(&task.published, &live))
}

/// Whether the inventory has edits no save has taken - see
/// [`room_unsaved`].
fn inventory_unsaved(world: &mut World) -> bool {
    let (Some(live), Some(stored)) = (
        world.get_resource::<LiveInventoryRecord>(),
        world.get_resource::<StoredInventoryRecord>(),
    ) else {
        return false;
    };
    if !records_differ(&live.0, &stored.0) {
        return false;
    }
    let live = live.0.clone();
    !world
        .query::<&PublishInventoryTask>()
        .iter(world)
        .any(|task| !records_differ(&task.published, &live))
}

/// Whether a save of the inventory is under way.
fn inventory_saving(world: &mut World) -> bool {
    world
        .query_filtered::<(), With<PublishInventoryTask>>()
        .iter(world)
        .next()
        .is_some()
}

/// Whether a save of the agent's world is under way.
fn room_saving(world: &mut World) -> bool {
    world
        .query_filtered::<(), Or<(With<PublishRoomTask>, With<ResetRoomTask>)>>()
        .iter(world)
        .next()
        .is_some()
}

/// Whether a save of the avatar is under way.
fn avatar_saving(world: &mut World) -> bool {
    world
        .query_filtered::<(), With<PublishAvatarTask>>()
        .iter(world)
        .next()
        .is_some()
}

/// Which records hold edits no save has taken, by name.
pub(super) fn unsaved(world: &mut World) -> Vec<&'static str> {
    let mut names = Vec::new();
    if room_unsaved(world) {
        names.push(EditRecord::Room.word());
    }
    if avatar_unsaved(world) {
        names.push(EditRecord::Avatar.word());
    }
    if inventory_unsaved(world) {
        names.push(EditRecord::Inventory.word());
    }
    names
}

/// Which records have a save under way, by name.
pub(super) fn saving(world: &mut World) -> Vec<&'static str> {
    let mut names = Vec::new();
    if room_saving(world) {
        names.push(EditRecord::Room.word());
    }
    if avatar_saving(world) {
        names.push(EditRecord::Avatar.word());
    }
    if inventory_saving(world) {
        names.push(EditRecord::Inventory.word());
    }
    names
}

/// What `status` says about editing: whether the agent may edit where it
/// stands and save at all, what is unsaved or saving, and what an undo or
/// a redo would step.
pub(super) fn describe(world: &mut World) -> Value {
    let profile = world
        .get_resource::<EditProfile>()
        .copied()
        .unwrap_or_default();
    let steps = |world: &World| {
        let room = world
            .get_resource::<RoomUndoHistory>()
            .filter(|_| owns_room(world));
        let avatar = world.get_resource::<AvatarUndoHistory>();
        json!({
            "undo": {
                "room": room.and_then(|h| h.undo_label()),
                "avatar": avatar.and_then(|h| h.undo_label()),
            },
            "redo": {
                "room": room.and_then(|h| h.redo_label()),
                "avatar": avatar.and_then(|h| h.redo_label()),
            },
        })
    };
    let mut answer = json!({
        "own_world": owns_room(world),
        "may_save": profile.save_refused().is_none(),
        "why_no_save": profile.save_refused(),
        "unsaved": unsaved(world),
        "saving": saving(world),
    });
    if let (Some(answer), Value::Object(steps)) = (answer.as_object_mut(), steps(world)) {
        answer.extend(steps);
    }
    answer
}

/// Undo or redo one step of `record`, through the history Ctrl+Z steps.
fn step(world: &mut World, record: EditRecord, kind: StepKind) -> Result<Value, String> {
    let (verb, noun) = match kind {
        StepKind::Undo => ("undid", "undo"),
        StepKind::Redo => ("redid", "redo"),
    };
    let label = match record {
        EditRecord::Room => {
            own_room(world)?;
            step_room_history(world, kind)
        }
        EditRecord::Avatar => {
            in_world(world)?;
            step_avatar_history(world, kind)
        }
        EditRecord::Inventory => {
            return Err("the game keeps no undo history for the inventory; \
                 `agent revert inventory` goes back to what was last saved"
                .to_owned());
        }
    }?
    .ok_or_else(|| format!("there is nothing to {noun} in the {}", record.word()))?;
    Ok(json!({ verb: label, "record": record.word() }))
}

fn step_room_history(world: &mut World, kind: StepKind) -> Result<Option<String>, String> {
    let missing = || "the world's undo history is not there yet".to_owned();
    if !world.contains_resource::<RoomUndoHistory>()
        || !world.contains_resource::<RoomEditorState>()
        || !world.contains_resource::<LiveRoomRecord>()
    {
        return Err(missing());
    }
    Ok(
        world.resource_scope(|world, mut history: Mut<RoomUndoHistory>| {
            world.resource_scope(|world, mut editor: Mut<RoomEditorState>| {
                let mut live = world.resource_mut::<LiveRoomRecord>();
                step_room(kind, &mut history, &mut live, &mut editor)
            })
        }),
    )
}

fn step_avatar_history(world: &mut World, kind: StepKind) -> Result<Option<String>, String> {
    if !world.contains_resource::<AvatarUndoHistory>()
        || !world.contains_resource::<AvatarEditorState>()
        || !world.contains_resource::<LiveAvatarRecord>()
    {
        return Err("the avatar's undo history is not there yet".to_owned());
    }
    Ok(
        world.resource_scope(|world, mut history: Mut<AvatarUndoHistory>| {
            world.resource_scope(|world, mut editor: Mut<AvatarEditorState>| {
                let mut live = world.resource_mut::<LiveAvatarRecord>();
                step_avatar(kind, &mut history, &mut live, &mut editor)
            })
        }),
    )
}

/// Throw `record`'s unsaved edits away: back to what was last saved, as
/// "Revert to saved" does - and, like it, a step the history can undo.
pub(super) fn revert(world: &mut World, record: EditRecord) -> Result<Value, String> {
    match record {
        EditRecord::Room => {
            own_room(world)?;
            let saving = room_saving(world);
            let (Some(live), Some(stored)) = (
                world.get_resource::<LiveRoomRecord>(),
                world.get_resource::<StoredRoomRecord>(),
            ) else {
                return Err("the agent's world has no record yet".to_owned());
            };
            let dirty = records_differ(&live.0, &stored.0);
            if let Some(why) = crate::ui::editable::revert_refusal(dirty, saving) {
                return Err(why.to_owned());
            }
            let stored = stored.0.clone();
            world.resource_mut::<LiveRoomRecord>().0 = stored;
            world
                .resource_mut::<PendingUndoLabels>()
                .set_room("revert to saved");
        }
        EditRecord::Avatar => {
            in_world(world)?;
            let saving = avatar_saving(world);
            let (Some(live), Some(stored)) = (
                world.get_resource::<LiveAvatarRecord>(),
                world.get_resource::<StoredAvatarRecord>(),
            ) else {
                return Err("the agent has no avatar record yet".to_owned());
            };
            let dirty = avatar_is_dirty(&live.0, &stored.0);
            if let Some(why) = crate::ui::editable::revert_refusal(dirty, saving) {
                return Err(why.to_owned());
            }
            let stored = stored.0.clone();
            world.resource_mut::<LiveAvatarRecord>().0 = stored;
            world
                .resource_mut::<PendingUndoLabels>()
                .set_avatar("revert to saved");
        }
        EditRecord::Inventory => {
            in_world(world)?;
            let saving = inventory_saving(world);
            let (Some(live), Some(stored)) = (
                world.get_resource::<LiveInventoryRecord>(),
                world.get_resource::<StoredInventoryRecord>(),
            ) else {
                return Err("the agent's inventory has not loaded yet".to_owned());
            };
            let dirty = records_differ(&live.0, &stored.0);
            if let Some(why) = crate::ui::editable::revert_refusal(dirty, saving) {
                return Err(why.to_owned());
            }
            let stored = stored.0.clone();
            world.resource_mut::<LiveInventoryRecord>().0 = stored;
        }
    }
    Ok(json!({ "reverted": record.word() }))
}

#[cfg(test)]
pub(super) mod harness {
    //! The agent standing in a world, for the edit commands' tests: the
    //! records and the undo history as the game registers them, and the
    //! history's capture where the game runs it, after each frame's update.

    use std::sync::Arc;

    use bevy::prelude::*;

    use bevy_symbios_multiuser::prelude::{Broadcast, SendTo};

    use crate::agent::control::events::EventLog;
    use crate::pds::{AvatarRecord, InventoryRecord, RoomRecord};
    use crate::protocol::OverlandsMessage;
    use crate::state::{
        AppState, CurrentRoomDid, LiveAvatarRecord, LiveInventoryRecord, LiveRoomRecord,
        PublishFeedback, RoomWriteSignals, StoredAvatarRecord, StoredInventoryRecord,
        StoredRoomRecord,
    };
    use crate::ui::avatar::AvatarEditorState;
    use crate::ui::room::RoomEditorState;
    use crate::ui::undo::{
        AvatarUndoHistory, PendingUndoLabels, RoomUndoHistory, capture_avatar_history,
        capture_room_history,
    };

    use super::super::observe::EventSink;
    use super::EditProfile;

    /// The agent, whose own world is the one seeded from this DID.
    pub const AGENT: &str = "did:plc:agenteditharness22222222";
    /// Somebody else, whose world the agent can visit.
    pub const OTHER: &str = "did:plc:someoneelsesworld2222222";
    /// An agent whose identity no directory could be asked about: a save
    /// it starts fails at the first step, before anything leaves the
    /// machine.
    pub const UNRESOLVABLE: &str = "did:key:zagentwhosesavesgonowhere";

    /// The agent standing in `room`'s world - its own when `room` is
    /// [`AGENT`] - allowed to save, one frame in so the undo history has
    /// its baseline.
    pub fn app_in(room: &str) -> (App, Arc<EventLog>) {
        app_as(AGENT, room)
    }

    /// [`app_in`], playing as `agent`.
    pub fn app_as(agent: &str, room: &str) -> (App, Arc<EventLog>) {
        let log = Arc::new(EventLog::new(64, "test".into()));
        let record = RoomRecord::default_for_did(room);
        let avatar = AvatarRecord::default_for_did(agent);
        let mut app = App::new();
        app.insert_resource(State::new(AppState::InGame))
            .insert_resource(Time::<()>::default())
            .insert_resource(
                crate::oauth::stand_in::stand_in_session(agent, "agent.test").expect("a session"),
            )
            .insert_resource(crate::oauth::stand_in::stand_in_refresh_ctx().expect("a context"))
            .insert_resource(CurrentRoomDid(room.to_owned()))
            .insert_resource(LiveRoomRecord(record.clone()))
            .insert_resource(StoredRoomRecord(record))
            .insert_resource(LiveAvatarRecord(avatar.clone()))
            .insert_resource(StoredAvatarRecord(avatar))
            .insert_resource(EditProfile {
                allow_save: true,
                offline: false,
            })
            .insert_resource(EventSink(Arc::clone(&log)))
            .init_resource::<PublishFeedback<RoomRecord>>()
            .init_resource::<PublishFeedback<AvatarRecord>>()
            .init_resource::<RoomUndoHistory>()
            .init_resource::<AvatarUndoHistory>()
            .init_resource::<RoomWriteSignals>()
            .init_resource::<PendingUndoLabels>()
            .init_resource::<RoomEditorState>()
            .init_resource::<AvatarEditorState>()
            // The inventory, and what answering and offering gifts touch
            // (#1423): the game's own resources, as its plugins make them.
            .insert_resource(LiveInventoryRecord(InventoryRecord::default()))
            .insert_resource(StoredInventoryRecord(InventoryRecord::default()))
            .init_resource::<PublishFeedback<InventoryRecord>>()
            .init_resource::<crate::diagnostics::SessionLog>()
            .init_resource::<crate::diagnostics::MetricsRegistry>()
            .init_resource::<crate::state::BusyAutoDeclines>()
            .init_resource::<crate::state::MutedDids>()
            .init_resource::<crate::state::PendingOutgoingOffers>()
            .init_resource::<crate::notify::Toasts>()
            .init_resource::<crate::network::chunk::OutboundChunkSeq>()
            .init_resource::<crate::network::LinkState>()
            .add_message::<Broadcast<OverlandsMessage>>()
            .add_message::<SendTo<OverlandsMessage>>()
            .add_systems(PostUpdate, (capture_room_history, capture_avatar_history));
        app.update();
        (app, log)
    }

    /// A catalogue entry that can stand in a world, by its slug - one the
    /// agent's seeded world holds nothing under, so placing it adds it.
    pub fn placeable_slug() -> &'static str {
        static SLUG: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
        SLUG.get_or_init(|| {
            let seeded = RoomRecord::default_for_did(AGENT);
            crate::catalogue::ENTRIES
                .iter()
                .find(|entry| {
                    !seeded.generators.contains_key(entry.slug())
                        && crate::pds::inventory::is_drop_placeable(&entry.build(AGENT))
                })
                .expect("a placeable catalogue entry the seeded world lacks")
                .slug()
        })
    }

    /// A task that never finishes: a save still under way.
    pub fn never_lands() -> bevy::tasks::Task<Result<(), String>> {
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default)
            .spawn(std::future::pending())
    }
}

#[cfg(test)]
mod tests {
    use super::harness::{AGENT, OTHER, app_in, never_lands, placeable_slug};
    use super::*;
    use crate::pds::{DefaultLanding, Fp, Fp2};

    fn place(world: &mut World, at: [f32; 2]) -> Result<Value, String> {
        answer(
            world,
            EditRequest::Place {
                slug: placeable_slug().to_owned(),
                at: Some(at),
                yaw_deg: None,
            },
        )
    }

    fn placements(app: &App) -> usize {
        app.world().resource::<LiveRoomRecord>().0.placements.len()
    }

    /// An edit is what saving and reading it back would give: a decimal
    /// the wire cannot hold comes out on its grid of ten-thousandths, and
    /// the written record is its own round trip, value for value.
    #[test]
    fn a_written_record_is_what_a_save_would_read_back() {
        let (mut app, _) = app_in(AGENT);
        let world = app.world_mut();
        let mut record = room_for_edit(world).expect("its own world");
        record.default_landing = Some(DefaultLanding {
            pos: Fp2([1.234_567_9, -0.000_04]),
            y: None,
            yaw_deg: Fp(0.0),
        });

        assert!(write_room(world, record, "landing".into()).expect("written"));

        let live = &world.resource::<LiveRoomRecord>().0;
        let pos = live.default_landing.expect("a landing").pos.0;
        assert_eq!(pos[0], 12346.0 / 10_000.0, "on the grid: {}", pos[0]);
        assert_eq!(pos[1], 0.0, "under half a step rounds to nothing");
        let reread = settle_room(live.clone()).expect("reads back");
        assert_eq!(
            serde_json::to_value(live).unwrap(),
            serde_json::to_value(&reread).unwrap()
        );
    }

    /// A write that changes nothing leaves the record alone - no change
    /// tick, so no recompile, no broadcast and no undo step.
    #[test]
    fn a_write_that_changes_nothing_touches_nothing() {
        let (mut app, _) = app_in(AGENT);
        let before = app.world().resource::<RoomUndoHistory>().len();
        let world = app.world_mut();
        let ticked = world.resource_ref::<LiveRoomRecord>().last_changed();
        let record = room_for_edit(world).expect("its own world");

        assert!(!write_room(world, record, "nothing".into()).expect("answered"));
        app.update();

        let world = app.world();
        assert_eq!(
            world.resource_ref::<LiveRoomRecord>().last_changed(),
            ticked
        );
        assert_eq!(world.resource::<RoomUndoHistory>().len(), before);
    }

    /// THE SEQUENCE: place, undo, redo. Each is one step of the history
    /// Ctrl+Z steps, named for the agent's command.
    #[test]
    fn an_edit_is_one_undo_step_named_for_it() {
        let (mut app, _) = app_in(AGENT);
        let baseline = placements(&app);

        place(app.world_mut(), [3.0, 4.0]).expect("placed");
        app.update();
        let label = format!("place of {}", placeable_slug());
        assert_eq!(
            app.world().resource::<RoomUndoHistory>().undo_label(),
            Some(label.as_str())
        );

        let undid = answer(app.world_mut(), EditRequest::Undo(EditRecord::Room)).expect("undone");
        app.update();
        assert_eq!(undid["undid"], label.as_str());
        assert_eq!(placements(&app), baseline);

        let redid = answer(app.world_mut(), EditRequest::Redo(EditRecord::Room)).expect("redone");
        app.update();
        assert_eq!(redid["redid"], label.as_str());
        assert_eq!(placements(&app), baseline + 1);

        let nothing = answer(app.world_mut(), EditRequest::Redo(EditRecord::Room));
        assert!(nothing.unwrap_err().contains("nothing to redo"));
    }

    /// The game lets a world's owner edit it and nobody else, and a save
    /// writes the saver's own account wherever they stand: every write is
    /// refused in someone else's world, and reading is not.
    #[test]
    fn nothing_is_written_in_someone_elses_world() {
        let (mut app, _) = app_in(OTHER);
        let before = serde_json::to_value(&app.world().resource::<LiveRoomRecord>().0).unwrap();
        let world = app.world_mut();

        let writes = [
            place(world, [1.0, 1.0]),
            answer(world, EditRequest::Remove { index: 0 }),
            answer(
                world,
                EditRequest::Move {
                    index: 0,
                    x: 1.0,
                    z: 1.0,
                    yaw_deg: None,
                },
            ),
            answer(
                world,
                EditRequest::Set {
                    record: EditRecord::Room,
                    pointer: "/environment/fog_visibility".into(),
                    value: serde_json::json!(1_000_000),
                },
            ),
            answer(world, EditRequest::Undo(EditRecord::Room)),
            answer(world, EditRequest::Revert(EditRecord::Room)),
            answer(world, EditRequest::Save(EditRecord::Room)),
        ];
        for refused in writes {
            let why = refused.expect_err("refused");
            assert!(why.contains("not the agent's world"), "{why}");
        }
        assert_eq!(
            serde_json::to_value(&world.resource::<LiveRoomRecord>().0).unwrap(),
            before
        );
        for read in [
            EditRequest::Placements { within_m: None },
            EditRequest::Get {
                record: EditRecord::Room,
                pointer: "/environment".into(),
            },
        ] {
            assert!(answer(world, read).is_ok());
        }
    }

    /// Revert is back to the saved record exactly, a step the history can
    /// undo - and refused with nothing to revert, or with a save in flight
    /// whose landing would make the saved record something else.
    #[test]
    fn revert_goes_back_to_what_was_saved_and_can_be_undone() {
        let (mut app, _) = app_in(AGENT);
        let baseline = placements(&app);
        place(app.world_mut(), [3.0, 4.0]).expect("placed");
        app.update();

        let reverted = answer(app.world_mut(), EditRequest::Revert(EditRecord::Room));
        app.update();
        assert_eq!(reverted.expect("reverted")["reverted"], "room");
        assert_eq!(placements(&app), baseline);
        assert!(!room_unsaved(app.world_mut()));
        let again = answer(app.world_mut(), EditRequest::Revert(EditRecord::Room));
        assert!(again.unwrap_err().contains("Nothing to revert"));

        answer(app.world_mut(), EditRequest::Undo(EditRecord::Room)).expect("undone");
        app.update();
        assert_eq!(placements(&app), baseline + 1, "the revert undone");

        let published = app.world().resource::<LiveRoomRecord>().0.clone();
        app.world_mut().spawn(PublishRoomTask {
            task: never_lands(),
            did: AGENT.into(),
            spawned_at: 0.0,
            record_bytes: None,
            published,
        });
        let saving = answer(app.world_mut(), EditRequest::Revert(EditRecord::Room));
        assert!(saving.unwrap_err().contains("Wait for the save"));
    }

    /// Unsaved is what no save has taken: a save under way that is writing
    /// the live record counts it as saved; one writing an older record
    /// does not.
    #[test]
    fn a_save_under_way_of_the_live_record_is_not_unsaved() {
        let (mut app, _) = app_in(AGENT);
        assert!(!room_unsaved(app.world_mut()), "the world as it arrived");
        let older = app.world().resource::<LiveRoomRecord>().0.clone();
        place(app.world_mut(), [3.0, 4.0]).expect("placed");
        assert!(room_unsaved(app.world_mut()));

        let task = |published| PublishRoomTask {
            task: never_lands(),
            did: AGENT.into(),
            spawned_at: 0.0,
            record_bytes: None,
            published,
        };
        let stale = app.world_mut().spawn(task(older)).id();
        assert!(
            room_unsaved(app.world_mut()),
            "the save is of the older record"
        );
        app.world_mut().despawn(stale);
        let live = app.world().resource::<LiveRoomRecord>().0.clone();
        app.world_mut().spawn(task(live));
        assert!(!room_unsaved(app.world_mut()));
        assert_eq!(saving(app.world_mut()), ["room"]);
    }

    /// The avatar is the agent's wherever it is: its undo works in someone
    /// else's world too, as its own history.
    #[test]
    fn the_avatar_is_edited_anywhere_as_its_own_history() {
        let (mut app, _) = app_in(OTHER);
        let before = app.world().resource::<LiveAvatarRecord>().0.clone();
        let pointer = "/record/locomotion";
        let got = answer(
            app.world_mut(),
            EditRequest::Get {
                record: EditRecord::Avatar,
                pointer: pointer.into(),
            },
        )
        .expect("read");
        let mut locomotion = got["value"].clone();
        let changed = bump_first_number(&mut locomotion).expect("a number to change");
        let set = answer(
            app.world_mut(),
            EditRequest::Set {
                record: EditRecord::Avatar,
                pointer: pointer.into(),
                value: locomotion,
            },
        )
        .expect("set");
        app.update();
        assert_eq!(set["changed"], true, "{set} ({changed})");
        assert_eq!(set["others_see_it"], "now");

        answer(app.world_mut(), EditRequest::Undo(EditRecord::Avatar)).expect("undone");
        app.update();
        assert!(!avatar_is_dirty(
            &app.world().resource::<LiveAvatarRecord>().0,
            &before
        ));
    }

    /// Nudge the first whole number in `value` (depth first) by one step,
    /// and say where it was.
    fn bump_first_number(value: &mut Value) -> Option<String> {
        match value {
            Value::Number(n) if n.is_i64() => {
                let was = n.as_i64()?;
                *value = serde_json::json!(was + 100);
                Some(format!("{was}"))
            }
            Value::Object(members) => members
                .iter_mut()
                .find_map(|(key, v)| bump_first_number(v).map(|was| format!("{key}={was}"))),
            Value::Array(items) => items.iter_mut().find_map(bump_first_number),
            _ => None,
        }
    }
}

#[cfg(test)]
mod describe_tests {
    use super::harness::{AGENT, OTHER, app_in, placeable_slug};
    use super::*;

    /// `status.editing`: where the agent may edit and whether it may save
    /// at all, and why not; what is unsaved; what an undo would step.
    #[test]
    fn status_says_what_editing_can_do_here() {
        let (mut app, _) = app_in(AGENT);
        answer(
            app.world_mut(),
            EditRequest::Place {
                slug: placeable_slug().to_owned(),
                at: Some([1.0, 1.0]),
                yaw_deg: None,
            },
        )
        .expect("placed");
        app.update();

        let editing = describe(app.world_mut());

        assert_eq!(editing["own_world"], true);
        assert_eq!(editing["may_save"], true);
        assert!(editing["why_no_save"].is_null());
        assert_eq!(editing["unsaved"], json!(["room"]));
        let placed = format!("place of {}", placeable_slug());
        assert_eq!(editing["undo"]["room"], placed.as_str());
        assert!(editing["redo"]["room"].is_null());

        let (mut visiting, _) = app_in(OTHER);
        visiting.world_mut().insert_resource(EditProfile {
            allow_save: false,
            offline: false,
        });
        let editing = describe(visiting.world_mut());
        assert_eq!(editing["own_world"], false);
        assert_eq!(editing["may_save"], false);
        assert!(
            editing["why_no_save"]
                .as_str()
                .is_some_and(|why| why.contains("--allow-save")),
            "{editing}"
        );
        assert!(editing["undo"]["room"].is_null(), "not its history here");
    }
}
