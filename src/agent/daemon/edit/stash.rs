//! Whole avatars and whole worlds in the inventory (#1843): `agent
//! stash-avatar`, `stash-world` and `load`, the whole-avatar half of `wear`,
//! and the Catalogue's avatars and worlds - each through the game's own path,
//! so it changes the live records exactly as a person's click does.
//!
//! * `stash-avatar` copies the avatar the agent wears - unsaved edits
//!   included - as the Avatar editor's Save to Inventory does
//!   ([`crate::ui::avatar::save_avatar_to_inventory_as`]); `stash-world`
//!   copies its own world as the World Editor's Save to Inventory does
//!   ([`crate::ui::room::stash::save_world_to_inventory_as`]). Each lands in
//!   the live inventory, unsaved until `agent save inventory`, as the
//!   agent's other stashes do. A world item's contents are records of their
//!   own, a collection a sign-in made before it was added cannot write: such
//!   a sign-in is refused with the game's sign-in-again sentence.
//! * `wear` of an avatar item, or of a catalogue avatar by its slug, puts it
//!   on through the Inventory's and the Catalogue's one wear path
//!   ([`crate::ui::avatar::wear_avatar_on_live`]): one write, one step of the
//!   avatar's undo history, unsaved until `agent save avatar`. Any world.
//! * `load` of a world item lands it as the Inventory's Load into my world
//!   does ([`crate::ui::inventory::world::spawn_world_load_task`]): the
//!   parts are joined - fetched from the account when this session no
//!   longer holds them - and the game's own poll replaces the world through
//!   the re-roll's path. So the answer says only that it started, and how it
//!   ended arrives as a `world_loaded` or `world_load_failed` event, which
//!   `agent load --wait` waits for. A catalogue world is built and landed in
//!   the frame, as its Load into my world does
//!   ([`crate::ui::catalogue::load_world_entry`]). Either way it is one step
//!   of the world's undo history, unsaved until `agent save`, and only in
//!   the agent's own world.
//!
//! Every refusal is the game's own sentence: the one its toast gives.

use bevy::ecs::system::SystemState;
use bevy::ecs::world::CommandQueue;
use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde_json::{Value, json};

use crate::catalogue::avatars::AvatarEntry;
use crate::catalogue::worlds::WorldEntry;
use crate::notify::Toasts;
use crate::pds::InventoryRecord;
use crate::pds::inventory::{AvatarItem, WorldItem};
use crate::state::{CurrentRoomDid, LiveAvatarRecord, LiveInventoryRecord, LiveRoomRecord};
use crate::ui::inventory::{LoadWorldTask, WearSurface, WorldLoadsEnded};
use crate::ui::room::RoomEditorState;
use crate::ui::undo::PendingUndoLabels;

use super::super::super::control::events::EventKind;
use super::super::observe::EventSink;

/// The agent's DID, as the game's paths take it.
fn did(world: &World) -> Result<String, String> {
    world
        .get_resource::<AtprotoSession>()
        .map(|session| session.did.clone())
        .ok_or_else(|| "the agent is not signed in".to_owned())
}

/// Whether the agent's sign-in may write a world item's parts - the
/// question the World Editor's and the Catalogue's Save to Inventory ask.
fn parts_granted(world: &World) -> bool {
    world
        .get_resource::<AtprotoSession>()
        .is_some_and(|session| {
            crate::pds::inventory::world::grant_covers_parts(&session.session.token_set().scope)
        })
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

/// A copy of the live inventory, to change and hand back with [`keep`] -
/// the live record is written only once a change is certain (#1322).
fn inventory_copy(world: &World) -> Result<InventoryRecord, String> {
    world
        .get_resource::<LiveInventoryRecord>()
        .map(|live| live.0.clone())
        .ok_or_else(|| "the agent's inventory has not loaded yet".to_owned())
}

fn keep(world: &mut World, inventory: InventoryRecord) {
    world.resource_mut::<LiveInventoryRecord>().0 = inventory;
}

/// Run `f` with the game's toast queue, as a click's handler has it.
fn with_toasts<R>(world: &mut World, f: impl FnOnce(&mut World, &mut Toasts) -> R) -> R {
    if !world.contains_resource::<Toasts>() {
        world.init_resource::<Toasts>();
    }
    world.resource_scope(|world, mut toasts: Mut<Toasts>| f(world, &mut toasts))
}

/// What an avatar item is, as the inventory's listing and the answers say.
pub(super) fn describe_avatar(item: &AvatarItem) -> Value {
    json!({
        "is": "avatar",
        "kind": item.kind().label(),
        "wearables": item.wearable_count(),
        "readable": item.is_readable(),
    })
}

/// What a world item is, as the inventory's listing and the answers say:
/// its head's summary - the world itself is read only when it is loaded.
pub(super) fn describe_world(item: &WorldItem) -> Value {
    let head = item.head();
    let summary = &head.summary;
    json!({
        "is": "world",
        "kind": summary.kind_label(),
        "world_name": (!summary.name.is_empty()).then_some(summary.name.as_str()),
        "assets": summary.generators,
        "placements": summary.placements,
        "bytes": head.bytes,
        "parts": head.parts.len(),
        "held": item.slices().is_some(),
    })
}

/// Merge `extra`'s members into `answer`.
fn with(mut answer: Value, extra: Value) -> Value {
    if let (Some(answer), Value::Object(extra)) = (answer.as_object_mut(), extra) {
        answer.extend(extra);
    }
    answer
}

/// Why a typed name is no name (critic of #1843): blank or invisible, it
/// would be stashed and listed, saved under the hash of nothing, and dropped
/// by the sanitiser at the next login - a world's parts left behind on the
/// account. The game never types one; the agent's `stash --from-avatar`
/// refuses one too.
pub(super) const NO_NAME: &str =
    "a name of only spaces or invisible characters is no name: give one, or none for the default";

/// A typed name, refused when it is no name ([`NO_NAME`]).
fn typed_name(name: Option<&str>) -> Result<Option<&str>, String> {
    match name {
        Some(typed)
            if crate::pds::sanitize::names::clean_name(
                typed,
                crate::config::state::MAX_INVENTORY_NAME_CHARS,
            )
            .is_none() =>
        {
            Err(NO_NAME.to_owned())
        }
        other => Ok(other),
    }
}

/// `agent stash-avatar [name]`: the avatar the agent wears, as the Avatar
/// editor's Save to Inventory copies it. Any world.
pub(super) fn stash_avatar(world: &mut World, name: Option<&str>) -> Result<Value, String> {
    super::in_world(world)?;
    let name = typed_name(name)?;
    let avatar = world
        .get_resource::<LiveAvatarRecord>()
        .map(|live| live.0.clone())
        .ok_or("the agent has no avatar record yet")?;
    let mut inventory = inventory_copy(world)?;
    let name = crate::ui::avatar::save_avatar_to_inventory_as(&avatar, name, &mut inventory)?;
    let described = describe_avatar(&inventory.avatars[&name]);
    keep(world, inventory);
    Ok(with(json!({ "stashed": name, "from": "worn" }), described))
}

/// `agent stash-world [name]`: the agent's own world, as the World Editor's
/// Save to Inventory copies it. Only there - the World Editor is its
/// owner's alone.
pub(super) fn stash_world(world: &mut World, name: Option<&str>) -> Result<Value, String> {
    let name = typed_name(name)?;
    let room = super::room_for_edit(world)?;
    let mut inventory = inventory_copy(world)?;
    let name = crate::ui::room::stash::save_world_to_inventory_as(
        &room,
        name,
        &mut inventory,
        parts_granted(world),
    )?;
    let described = describe_world(&inventory.worlds[&name]);
    keep(world, inventory);
    Ok(with(
        json!({ "stashed": name, "from": "own_world" }),
        described,
    ))
}

/// `agent stash <avatar slug>`: the Catalogue's Copy to inventory of an
/// avatar entry ([`crate::ui::catalogue::copy_avatar_entry`]).
pub(super) fn copy_avatar(world: &mut World, entry: &AvatarEntry) -> Result<Value, String> {
    let mut inventory = inventory_copy(world)?;
    let now = now(world);
    let name = with_toasts(world, |_, toasts| {
        crate::ui::catalogue::copy_avatar_entry(entry, &mut inventory, toasts, now)
    })?;
    let described = describe_avatar(&inventory.avatars[&name]);
    keep(world, inventory);
    Ok(with(
        json!({ "stashed": name, "from": "catalogue" }),
        described,
    ))
}

/// `agent stash <world slug>`: the Catalogue's Copy to inventory of a world
/// entry ([`crate::ui::catalogue::copy_world_entry`]) - built for the
/// agent, as a re-roll builds. Any world, as the Catalogue's is.
pub(super) fn copy_world(world: &mut World, entry: &WorldEntry) -> Result<Value, String> {
    let mut inventory = inventory_copy(world)?;
    let (did, granted, now) = (did(world)?, parts_granted(world), now(world));
    let name = with_toasts(world, |_, toasts| {
        crate::ui::catalogue::copy_world_entry(entry, &did, &mut inventory, granted, toasts, now)
    })?;
    let described = describe_world(&inventory.worlds[&name]);
    keep(world, inventory);
    Ok(with(
        json!({ "stashed": name, "from": "catalogue" }),
        described,
    ))
}

/// Run `wear` with the surface the Inventory's and the Catalogue's Wear
/// buttons hand the game's wear path.
fn with_wear_surface(
    world: &mut World,
    wear: impl FnOnce(&mut WearSurface, &str, f64) -> Result<(), String>,
) -> Result<(), String> {
    let (did, now) = (did(world)?, now(world));
    if !world.contains_resource::<Toasts>() {
        world.init_resource::<Toasts>();
    }
    let mut state = SystemState::<WearSurface>::new(world);
    let worn = {
        let mut surface = state
            .get_mut(world)
            .map_err(|e| format!("the avatar cannot be dressed right now: {e}"))?;
        wear(&mut surface, &did, now)
    };
    state.apply(world);
    worn
}

/// What a wear that landed answers: what is worn, and the step `agent undo`
/// takes back. `kind` is the Inventory's and the Catalogue's word for it,
/// where every generator body on the car preset is a `skiff`; `locomotion`
/// is how the body now worn moves, in `status.locomotion`'s words - `car`
/// for that skiff (#1866: live 21 read `skiff` for Kairos's Hexapod and
/// Roadfactory, car-preset bodies since they were built, and took it for a
/// chassis they had not been built on).
fn worn(world: &World, name: &str, kind: &str) -> Value {
    let locomotion = world
        .get_resource::<LiveAvatarRecord>()
        .map(|live| super::super::status::locomotion_word(&live.0));
    json!({
        "wearing": name,
        "is": "avatar",
        "kind": kind,
        "locomotion": locomotion,
        "undo": format!("wear {name}"),
        "unsaved_until": "agent save avatar",
    })
}

/// `agent wear <avatar item>`: the Inventory's Wear on an avatar row.
pub(super) fn wear_item(world: &mut World, name: &str, item: &AvatarItem) -> Result<Value, String> {
    super::in_world(world)?;
    with_wear_surface(world, |surface, did, now| {
        let stored = surface.stored_avatar.as_deref().map(|stored| &stored.0);
        let Some(live) = surface.live_avatar.as_mut() else {
            return Err("No avatar loaded yet.".to_owned());
        };
        crate::ui::avatar::wear_avatar_on_live(
            item,
            name,
            live.reborrow(),
            stored,
            &mut surface.avatar_editor,
            did,
            &mut surface.undo_labels,
            &mut surface.toasts,
            now,
        )
    })?;
    Ok(worn(world, name, item.kind().label()))
}

/// `agent wear <avatar slug>`: the Catalogue's Wear on an avatar entry.
pub(super) fn wear_entry(world: &mut World, entry: &AvatarEntry) -> Result<Value, String> {
    super::in_world(world)?;
    with_wear_surface(world, |surface, did, now| {
        crate::ui::catalogue::wear_avatar_entry(entry, surface, did, now)
    })?;
    let kind = entry
        .built()
        .and_then(|built| built.item.as_ref().ok())
        .map_or("avatar", |item| item.kind().label());
    Ok(worn(world, entry.name, kind))
}

/// Whether a world load is under way - the one-at-a-time rule.
fn loading(world: &mut World) -> bool {
    world
        .query_filtered::<(), With<LoadWorldTask>>()
        .iter(world)
        .next()
        .is_some()
}

/// `agent load <name>`: a world item by its name, else a catalogue world
/// by its slug, over the agent's own world.
pub(super) fn load(world: &mut World, name: &str) -> Result<Value, String> {
    super::in_world(world)?;
    let inventory = inventory_copy(world)?;
    if let Some(item) = inventory.worlds.get(name) {
        return load_item(world, name, item);
    }
    if inventory.has_name(name) {
        return Err(format!(
            "{name:?} is not a world, so it cannot be loaded; `agent inventory` says what it is"
        ));
    }
    match crate::catalogue::worlds::world_by_slug(name) {
        Some(entry) => load_entry(world, entry),
        None => Err(format!(
            "neither the inventory nor the catalogue has a world called {name:?}; `agent \
             inventory` lists the inventory's, `agent catalogue worlds` the catalogue's"
        )),
    }
}

/// The Inventory's Load into my world, confirmed: refused for the reasons
/// it is disabled for, else the load task it starts.
fn load_item(world: &mut World, name: &str, item: &WorldItem) -> Result<Value, String> {
    let busy = loading(world);
    if let Some(reason) =
        crate::ui::inventory::world::world_load_blocked_reason(super::owns_room(world), busy)
    {
        return Err(reason.to_owned());
    }
    let (did, now) = (did(world)?, now(world));
    let mut queue = CommandQueue::default();
    {
        let mut commands = Commands::new(&mut queue, world);
        crate::ui::inventory::world::spawn_world_load_task(&mut commands, &did, name, item, now);
    }
    queue.apply(world);
    with_toasts(world, |_, toasts| {
        toasts.info(format!("Loading \"{name}\" into your world\u{2026}"), now);
    });
    Ok(json!({
        "loading": name,
        "from": "inventory",
        "fetching": item.slices().is_none(),
        "events_seq": world.resource::<EventSink>().0.last_seq(),
        "undo": format!("load {name}"),
        "unsaved_until": "agent save",
    }))
}

/// The Catalogue's Load into my world, confirmed: built and landed in the
/// frame.
fn load_entry(world: &mut World, entry: &WorldEntry) -> Result<Value, String> {
    let busy = loading(world);
    let (did, now) = (did(world)?, now(world));
    let room_did = world
        .get_resource::<CurrentRoomDid>()
        .map(|room| room.0.clone());
    if !world.contains_resource::<Toasts>() {
        world.init_resource::<Toasts>();
    }
    let mut state = SystemState::<(
        Option<ResMut<LiveRoomRecord>>,
        Option<ResMut<RoomEditorState>>,
        ResMut<PendingUndoLabels>,
        ResMut<Toasts>,
    )>::new(world);
    let landed = {
        let (mut live, mut editor, mut labels, mut toasts) = state
            .get_mut(world)
            .map_err(|e| format!("the world cannot be loaded right now: {e}"))?;
        crate::ui::catalogue::load_world_entry(
            entry,
            &did,
            busy,
            room_did.as_deref(),
            live.as_mut().map(|live| live.reborrow()),
            editor.as_mut().map(|editor| editor.reborrow()),
            &mut labels,
            &mut toasts,
            now,
        )
    };
    state.apply(world);
    landed?;
    // The Catalogue's click says the rebuild to come, and so does this.
    world.insert_resource(crate::world_builder::RebuildAnnounce {
        what: format!("Loaded \"{}\"", entry.name),
        since_secs: now,
    });
    Ok(json!({
        "loaded": entry.name,
        "from": "catalogue",
        "undo": format!("load {}", entry.name),
        "unsaved": super::unsaved(world),
        "unsaved_until": "agent save",
    }))
}

/// Each world load that ended - the agent's own, or one a click in the
/// game's interface started - becomes a `world_loaded` or
/// `world_load_failed` event, read off the outcomes the game's load poll
/// leaves ([`WorldLoadsEnded`], which the daemon inserts).
pub(in crate::agent::daemon) fn record_world_loads(
    ended: Option<ResMut<WorldLoadsEnded>>,
    sink: Res<EventSink>,
) {
    // Read through `Deref` first: draining an empty list would still stamp
    // its tick.
    let Some(mut ended) = ended.filter(|ended| !ended.0.is_empty()) else {
        return;
    };
    for (name, outcome) in ended.0.drain(..) {
        sink.0.push(match outcome {
            Ok(()) => EventKind::WorldLoaded { name },
            Err(reason) => EventKind::WorldLoadFailed { name, reason },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::harness::{AGENT, OTHER, app_in};
    use super::super::{EditRecord, EditRequest, answer, unsaved};
    use super::*;
    use crate::pds::{AvatarRecord, RoomRecord};
    use crate::state::StoredAvatarRecord;
    use crate::ui::undo::{AvatarUndoHistory, RoomUndoHistory};

    /// A seeded avatar with a rigged body in hand.
    fn rigged() -> AvatarRecord {
        (0..200)
            .map(|n| AvatarRecord::default_for_did(&format!("did:plc:stashbody{n}")))
            .find(|avatar| {
                avatar
                    .body
                    .rigged_ref()
                    .is_some_and(|rig| rig.resolved.is_some())
            })
            .expect("a seeded rigged avatar")
    }

    fn value<T: serde::Serialize>(record: &T) -> Value {
        serde_json::to_value(record).expect("serialises")
    }

    fn room(app: &App) -> Value {
        value(&app.world().resource::<LiveRoomRecord>().0)
    }

    fn ask(app: &mut App, request: EditRequest) -> Result<Value, String> {
        answer(app.world_mut(), request)
    }

    /// A procedural catalogue world, by its entry.
    fn a_world_entry() -> &'static WorldEntry {
        crate::catalogue::worlds::WORLDS
            .iter()
            .find(|entry| entry.borough().is_none())
            .expect("a procedural world entry")
    }

    /// A vehicle catalogue avatar, by its entry.
    fn a_vehicle_entry() -> &'static AvatarEntry {
        crate::catalogue::avatars::AVATARS
            .iter()
            .find(|entry| !entry.is_rigged())
            .expect("a vehicle entry")
    }

    /// THE AVATAR'S ROUND TRIP: `stash-avatar` copies the avatar as worn -
    /// its unsaved edit included - into the live inventory, unsaved; a
    /// different avatar is put on; `wear` of the item puts the copy back in
    /// ONE undo step, unsaved, that one `undo` takes back.
    #[test]
    fn a_stashed_avatar_is_worn_back_in_one_undo_step() {
        let (mut app, _) = app_in(AGENT);
        let mut sculpted = rigged();
        if let Some(resolved) = sculpted
            .body
            .rigged_mut()
            .and_then(|rig| rig.resolved.as_mut())
        {
            resolved.body.name = String::from("Sunday best");
        }
        app.world_mut()
            .insert_resource(StoredAvatarRecord(rigged()));
        app.world_mut()
            .insert_resource(LiveAvatarRecord(sculpted.clone()));
        app.update();

        let stashed = ask(&mut app, EditRequest::StashAvatar { name: None }).expect("stashed");
        assert_eq!(stashed["stashed"], "Sunday best", "the body's own name");
        assert_eq!(
            (stashed["is"].as_str(), stashed["kind"].as_str()),
            (
                Some("avatar"),
                Some(
                    AvatarItem::from_record(&sculpted)
                        .expect("copies")
                        .kind()
                        .label()
                )
            )
        );
        let item = app.world().resource::<LiveInventoryRecord>().0.avatars["Sunday best"].clone();
        assert_eq!(
            Ok(&item),
            AvatarItem::from_record(&sculpted).as_ref(),
            "the avatar as worn, its unsaved edit with it"
        );
        assert!(unsaved(app.world_mut()).contains(&"inventory"));

        // Something else on, then the item back.
        let vehicle = (0..200)
            .map(|n| AvatarRecord::default_for_did(&format!("did:plc:stashcraft{n}")))
            .find(|avatar| avatar.body.visuals().is_some())
            .expect("a seeded vehicle");
        app.world_mut()
            .insert_resource(LiveAvatarRecord(vehicle.clone()));
        app.update();
        let before = app.world().resource::<AvatarUndoHistory>().len();

        let worn = ask(
            &mut app,
            EditRequest::Wear {
                name: "Sunday best".into(),
            },
        )
        .expect("worn");
        app.update();

        assert_eq!(worn["wearing"], "Sunday best");
        assert_eq!(worn["undo"], "wear Sunday best");
        let live = app.world().resource::<LiveAvatarRecord>().0.clone();
        assert_eq!(AvatarItem::from_record(&live).as_ref(), Ok(&item));
        assert!(
            live.body
                .rigged_ref()
                .is_some_and(|rig| rig.resolved.is_some()),
            "resolved at once, with no fetch"
        );
        let history = app.world().resource::<AvatarUndoHistory>();
        assert_eq!(history.len(), before + 1, "one undo step");
        assert_eq!(history.undo_label(), Some("wear Sunday best"));
        assert!(unsaved(app.world_mut()).contains(&"avatar"));

        // Worn already: the game's sentence, and nothing written.
        let tick = app
            .world()
            .resource_ref::<LiveAvatarRecord>()
            .last_changed();
        app.world_mut().increment_change_tick();
        let again = ask(
            &mut app,
            EditRequest::Wear {
                name: "Sunday best".into(),
            },
        )
        .expect_err("already on");
        assert_eq!(again, "You are already wearing \"Sunday best\".");
        assert_eq!(
            app.world()
                .resource_ref::<LiveAvatarRecord>()
                .last_changed(),
            tick,
            "a refusal stamps no tick"
        );

        ask(&mut app, EditRequest::Undo(EditRecord::Avatar)).expect("undone");
        app.update();
        assert_eq!(
            value(&app.world().resource::<LiveAvatarRecord>().0),
            value(&vehicle),
            "one undo takes it off again"
        );
    }

    /// A name given is the item's, made free across every kind; a body
    /// with nothing to copy is refused in the Avatar editor's words, and
    /// nothing reaches the inventory.
    #[test]
    fn stash_avatar_takes_a_name_and_refuses_in_the_games_words() {
        let (mut app, _) = app_in(OTHER);
        app.world_mut()
            .resource_mut::<LiveInventoryRecord>()
            .0
            .put_item("Me".into(), crate::pds::Generator::default_cuboid(), None);

        let named = ask(
            &mut app,
            EditRequest::StashAvatar {
                name: Some("Me".into()),
            },
        )
        .expect("stashed in someone else's world, as the avatar goes anywhere");
        assert_eq!(named["stashed"], "Me_1");

        let mut unresolved = rigged();
        unresolved.body.rigged_mut().expect("rigged").resolved = None;
        app.world_mut()
            .insert_resource(LiveAvatarRecord(unresolved));
        let why = ask(&mut app, EditRequest::StashAvatar { name: None }).expect_err("refused");
        assert_eq!(why, crate::pds::inventory::avatar::UNRESOLVED_BODY);
        // A name that is no name is refused before anything is copied: blank
        // or invisible, the sanitiser would drop the item at the next login
        // (critic of #1843).
        for blank in ["   ", "\u{200B}"] {
            let why = ask(
                &mut app,
                EditRequest::StashAvatar {
                    name: Some(blank.into()),
                },
            )
            .expect_err("no name");
            assert_eq!(why, super::NO_NAME, "{blank:?}");
            let why = ask(
                &mut app,
                EditRequest::StashWorld {
                    name: Some(blank.into()),
                },
            )
            .expect_err("no name");
            assert_eq!(why, super::NO_NAME, "{blank:?}");
        }
        assert_eq!(
            app.world().resource::<LiveInventoryRecord>().0.item_count(),
            2
        );
    }

    /// The cap counts every kind: a stash full of things refuses an avatar,
    /// a world and a catalogue avatar alike, in the game's words.
    #[test]
    fn a_full_inventory_refuses_avatars_and_worlds() {
        let (mut app, _) = app_in(AGENT);
        {
            let mut live = app.world_mut().resource_mut::<LiveInventoryRecord>();
            for n in 0..crate::config::state::MAX_INVENTORY_ITEMS {
                live.0.put_item(
                    format!("g{n}"),
                    crate::pds::Generator::default_cuboid(),
                    None,
                );
            }
        }
        let full = |why: Result<Value, String>| {
            let why = why.expect_err("refused");
            assert!(why.starts_with("Inventory full"), "{why}");
        };
        full(ask(&mut app, EditRequest::StashAvatar { name: None }));
        full(ask(&mut app, EditRequest::StashWorld { name: None }));
        full(ask(
            &mut app,
            EditRequest::Stash {
                what: a_vehicle_entry().slug.into(),
                from_avatar: None,
            },
        ));
    }

    /// THE WORLD'S ROUND TRIP: `stash-world` copies the world as it stands,
    /// its unsaved edit included, and `load` of the item, once the world
    /// has moved on, starts the Inventory's own load; the game's poll lands
    /// it as ONE undo step, unsaved, and says so as an event.
    #[test]
    fn a_stashed_world_loads_back_in_one_undo_step() {
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        let (mut app, log) = app_in(AGENT);
        app.init_resource::<WorldLoadsEnded>().add_systems(
            Update,
            (
                crate::ui::inventory::poll_world_load_tasks,
                record_world_loads.after(crate::ui::inventory::poll_world_load_tasks),
            ),
        );
        ask(
            &mut app,
            EditRequest::Set {
                record: EditRecord::Room,
                pointer: "/environment/sun_color".into(),
                value: json!([9000, 1000, 1000]),
            },
        )
        .expect("an unsaved edit");
        app.update();
        let as_stashed = room(&app);

        let stashed = ask(
            &mut app,
            EditRequest::StashWorld {
                name: Some("Harbour".into()),
            },
        )
        .expect("stashed");
        assert_eq!(
            (stashed["stashed"].as_str(), stashed["is"].as_str()),
            (Some("Harbour"), Some("world"))
        );
        assert_eq!(stashed["held"], true, "its parts held for the save");
        assert!(unsaved(app.world_mut()).contains(&"inventory"));

        // The world moves on.
        ask(
            &mut app,
            EditRequest::Set {
                record: EditRecord::Room,
                pointer: "/environment/sun_color".into(),
                value: json!([1000, 9000, 1000]),
            },
        )
        .expect("another edit");
        app.update();
        let moved_on = room(&app);
        assert_ne!(moved_on, as_stashed);
        let before = app.world().resource::<RoomUndoHistory>().len();

        let started = ask(
            &mut app,
            EditRequest::Load {
                name: "Harbour".into(),
            },
        )
        .expect("loading");
        assert_eq!(started["loading"], "Harbour");
        assert_eq!(started["fetching"], false);
        let since = started["events_seq"].as_u64().expect("a seq");
        // A second load while the first is in flight: the game's sentence.
        let busy = ask(
            &mut app,
            EditRequest::Load {
                name: "Harbour".into(),
            },
        )
        .expect_err("one at a time");
        assert_eq!(busy, "A world is loading - wait for it to land.");

        for _ in 0..200 {
            app.update();
            if app
                .world_mut()
                .query::<&LoadWorldTask>()
                .iter(app.world())
                .next()
                .is_none()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        app.update();

        let mut settled: RoomRecord = serde_json::from_value(as_stashed).expect("reads");
        crate::ui::room::settle_edit(&mut settled);
        assert_eq!(room(&app), value(&settled), "the world as it was stashed");
        let history = app.world().resource::<RoomUndoHistory>();
        assert_eq!(history.len(), before + 1, "one undo step");
        assert_eq!(history.undo_label(), Some("load Harbour"));
        assert!(unsaved(app.world_mut()).contains(&"room"));
        let events: Vec<EventKind> = log
            .after(since, std::time::Duration::ZERO)
            .events
            .into_iter()
            .map(|event| event.what)
            .collect();
        assert_eq!(
            events,
            [EventKind::WorldLoaded {
                name: "Harbour".into()
            }]
        );

        ask(&mut app, EditRequest::Undo(EditRecord::Room)).expect("undone");
        app.update();
        assert_eq!(room(&app), moved_on, "one undo takes it back");
    }

    /// A world loads into the agent's own world only, and is copied from
    /// there only: elsewhere each is refused before anything is touched.
    #[test]
    fn worlds_load_and_stash_in_the_agents_own_world_only() {
        let (mut app, _) = app_in(OTHER);
        app.world_mut()
            .resource_mut::<LiveInventoryRecord>()
            .0
            .put_world(
                "Harbour".into(),
                WorldItem::snapshot(&RoomRecord::default_for_did(AGENT)).expect("copies"),
            );
        let before = room(&app);

        let item = ask(
            &mut app,
            EditRequest::Load {
                name: "Harbour".into(),
            },
        )
        .expect_err("refused");
        let entry = ask(
            &mut app,
            EditRequest::Load {
                name: a_world_entry().slug.into(),
            },
        )
        .expect_err("refused");
        let stash = ask(&mut app, EditRequest::StashWorld { name: None }).expect_err("refused");

        let elsewhere =
            crate::ui::inventory::world::world_load_blocked_reason(false, false).expect("a reason");
        assert_eq!(item, elsewhere);
        assert_eq!(entry, elsewhere);
        assert!(stash.contains("not the agent's world"), "{stash}");
        assert_eq!(room(&app), before);
        assert!(
            app.world_mut()
                .query::<&LoadWorldTask>()
                .iter(app.world())
                .next()
                .is_none(),
            "no load started"
        );
        assert_eq!(
            app.world().resource::<LiveInventoryRecord>().0.item_count(),
            1
        );
    }

    /// A catalogue world loads in the frame, as its Load into my world
    /// does: the world it builds for the agent, one undo step, unsaved -
    /// which the answer says.
    #[test]
    fn a_catalogue_world_loads_at_once_in_one_undo_step() {
        let (mut app, _) = app_in(AGENT);
        let entry = a_world_entry();
        let before = app.world().resource::<RoomUndoHistory>().len();
        let old = room(&app);

        let loaded = ask(
            &mut app,
            EditRequest::Load {
                name: entry.slug.into(),
            },
        )
        .expect("loaded");
        app.update();

        assert_eq!(loaded["loaded"], entry.name);
        assert_eq!(loaded["unsaved"], json!(["room"]));
        let built = crate::ui::catalogue::built_world(entry, AGENT).expect("builds");
        assert_eq!(room(&app), value(&built), "the world the entry builds");
        let history = app.world().resource::<RoomUndoHistory>();
        assert_eq!(history.len(), before + 1, "one undo step");
        let label = format!("load {}", entry.name);
        assert_eq!(history.undo_label(), Some(label.as_str()));
        assert_eq!(loaded["undo"], label.as_str());

        ask(&mut app, EditRequest::Undo(EditRecord::Room)).expect("undone");
        app.update();
        assert_eq!(room(&app), old);
    }

    /// The Catalogue's avatars and worlds by slug: `stash` copies each as
    /// Copy to inventory does, and `wear` puts an avatar on as the
    /// Catalogue's Wear does - one undo step, unsaved, anywhere.
    #[test]
    fn catalogue_avatars_and_worlds_are_copied_and_worn_by_slug() {
        let (mut app, _) = app_in(OTHER);
        let vehicle = a_vehicle_entry();
        let world = a_world_entry();

        let avatar = ask(
            &mut app,
            EditRequest::Stash {
                what: vehicle.slug.into(),
                from_avatar: None,
            },
        )
        .expect("copied");
        let copied = ask(
            &mut app,
            EditRequest::Stash {
                what: world.slug.into(),
                from_avatar: None,
            },
        )
        .expect("copied anywhere, as the Catalogue's Copy is");
        assert_eq!(
            (avatar["stashed"].as_str(), avatar["is"].as_str()),
            (Some(vehicle.name), Some("avatar"))
        );
        assert_eq!(
            (copied["stashed"].as_str(), copied["is"].as_str()),
            (Some(world.name), Some("world"))
        );
        let inventory = app.world().resource::<LiveInventoryRecord>().0.clone();
        assert!(inventory.avatars.contains_key(vehicle.name));
        assert!(inventory.worlds.contains_key(world.name));

        let before = app.world().resource::<AvatarUndoHistory>().len();
        let worn = ask(
            &mut app,
            EditRequest::Wear {
                name: vehicle.slug.into(),
            },
        )
        .expect("worn");
        app.update();
        assert_eq!(worn["wearing"], vehicle.name);
        let live = &app.world().resource::<LiveAvatarRecord>().0;
        assert_eq!(live.body.visuals(), vehicle.visuals(), "the entry's tree");
        let history = app.world().resource::<AvatarUndoHistory>();
        assert_eq!(history.len(), before + 1, "one undo step");
        assert!(unsaved(app.world_mut()).contains(&"avatar"));
    }

    /// #1866: a body on car physics wears as a `skiff` - the Inventory's
    /// and the Catalogue's word for every generator body on the car preset,
    /// after the Catalogue's Land-skiffs - and the answer says how it moves
    /// in `status.locomotion`'s words: `car`. Live 21 read `skiff` for
    /// Kairos's Hexapod and Roadfactory, two car-preset bodies since they
    /// were built, and took it for a chassis they had not been built on.
    /// An inventory item and a Catalogue entry answer alike.
    #[test]
    fn a_worn_avatar_says_how_it_moves() {
        use crate::seeded_defaults::{AvatarPins, CraftType, SkiffType};
        let (mut app, _) = app_in(AGENT);
        app.world_mut().insert_resource(LiveAvatarRecord(rigged()));
        let mut pins = AvatarPins::default();
        pins.lock_craft(Some(CraftType::Skiff(SkiffType::Wagon)));
        let car = AvatarRecord::default_for_seed(pins.find_seed(0).expect("a skiff seed"));
        assert!(matches!(
            car.locomotion,
            crate::pds::LocomotionConfig::Car(_)
        ));
        let item = AvatarItem::from_record(&car).expect("copies");
        app.world_mut()
            .resource_mut::<LiveInventoryRecord>()
            .0
            .put_avatar("Kairos Hexapod".into(), item);

        let worn = ask(
            &mut app,
            EditRequest::Wear {
                name: "Kairos Hexapod".into(),
            },
        )
        .expect("worn");
        assert_eq!(
            (worn["kind"].as_str(), worn["locomotion"].as_str()),
            (Some("skiff"), Some("car")),
            "{worn}"
        );

        let entry = crate::catalogue::avatars::avatar_by_slug("avatar_skiff_rover")
            .expect("the rover Kairos's cars were built from");
        let worn = ask(
            &mut app,
            EditRequest::Wear {
                name: entry.slug.into(),
            },
        )
        .expect("worn");
        assert_eq!(
            (worn["kind"].as_str(), worn["locomotion"].as_str()),
            (Some("skiff"), Some("car")),
            "{worn}"
        );
    }

    /// A world item needs a grant newer than a sign-in made before #1839:
    /// such a sign-in is refused with the game's sign-in-again sentence,
    /// whether it copies its world or a catalogue world, and so is a save
    /// that would write one - before anything is written.
    #[test]
    fn a_sign_in_from_before_world_items_is_told_to_sign_in_again() {
        use crate::pds::inventory::world::SIGN_IN_AGAIN;
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        let (mut app, _) = app_in(AGENT);
        let old_grant = "atproto repo:network.symbios.overlands.room \
             repo:network.symbios.overlands.inventory.item";
        app.world_mut().insert_resource(
            crate::oauth::stand_in::stand_in_session_scoped(AGENT, "agent.test", old_grant)
                .expect("a session"),
        );

        let own = ask(&mut app, EditRequest::StashWorld { name: None }).expect_err("refused");
        let entry = ask(
            &mut app,
            EditRequest::Stash {
                what: a_world_entry().slug.into(),
                from_avatar: None,
            },
        )
        .expect_err("refused");
        assert_eq!(own, SIGN_IN_AGAIN);
        assert_eq!(entry, SIGN_IN_AGAIN);
        assert!(
            app.world()
                .resource::<LiveInventoryRecord>()
                .0
                .worlds
                .is_empty()
        );

        // A world item already held - from before, or a gift of the game -
        // is not saved past the grant either.
        app.world_mut()
            .resource_mut::<LiveInventoryRecord>()
            .0
            .put_world(
                "Harbour".into(),
                WorldItem::snapshot(&RoomRecord::default_for_did(AGENT)).expect("copies"),
            );
        let save = ask(&mut app, EditRequest::Save(EditRecord::Inventory)).expect_err("refused");
        assert_eq!(save, SIGN_IN_AGAIN);
        assert!(
            app.world_mut()
                .query::<&crate::ui::inventory::PublishInventoryTask>()
                .iter(app.world())
                .next()
                .is_none(),
            "nothing written"
        );
        // A world needs no new grant to be loaded: its parts are read, not
        // written.
        assert!(
            ask(
                &mut app,
                EditRequest::Load {
                    name: "Harbour".into()
                }
            )
            .is_ok()
        );
    }

    /// Avatars and worlds are not mixed up with each other or with things:
    /// a world is not worn, a thing or an avatar is not loaded, and a name
    /// nothing holds is said as such.
    #[test]
    fn each_kind_answers_only_its_own_verbs() {
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        let (mut app, _) = app_in(AGENT);
        {
            let mut live = app.world_mut().resource_mut::<LiveInventoryRecord>();
            live.0.put_world(
                "Harbour".into(),
                WorldItem::snapshot(&RoomRecord::default_for_did(AGENT)).expect("copies"),
            );
            live.0.put_item(
                "Crate".into(),
                crate::pds::Generator::default_cuboid(),
                None,
            );
        }
        let worn = ask(
            &mut app,
            EditRequest::Wear {
                name: "Harbour".into(),
            },
        )
        .expect_err("a world is not worn");
        assert!(worn.contains("agent load"), "{worn}");
        let thing = ask(
            &mut app,
            EditRequest::Load {
                name: "Crate".into(),
            },
        )
        .expect_err("a thing is not loaded");
        assert!(thing.contains("is not a world"), "{thing}");
        let nothing = ask(
            &mut app,
            EditRequest::Load {
                name: "Atlantis".into(),
            },
        )
        .expect_err("nothing");
        assert!(nothing.contains("has a world called"), "{nothing}");

        // `unstash` takes an avatar or a world out like any item.
        ask(
            &mut app,
            EditRequest::Unstash {
                name: "Harbour".into(),
            },
        )
        .expect("unstashed");
        assert!(
            app.world()
                .resource::<LiveInventoryRecord>()
                .0
                .worlds
                .is_empty()
        );
    }

    /// The outcomes the game's load poll leaves become events, one per load,
    /// a failure with the game's reason; an empty mailbox is not touched.
    #[test]
    fn each_load_that_ends_is_one_event() {
        let log = std::sync::Arc::new(crate::agent::control::events::EventLog::new(
            16,
            "test".into(),
        ));
        let mut app = App::new();
        app.insert_resource(EventSink(std::sync::Arc::clone(&log)))
            .init_resource::<WorldLoadsEnded>()
            .add_systems(Update, record_world_loads);
        app.update();
        let tick = app.world().resource_ref::<WorldLoadsEnded>().last_changed();
        app.world_mut().increment_change_tick();
        app.update();
        assert_eq!(
            app.world().resource_ref::<WorldLoadsEnded>().last_changed(),
            tick
        );
        app.world_mut().resource_mut::<WorldLoadsEnded>().0 = vec![
            ("Harbour".into(), Ok(())),
            ("Atlantis".into(), Err("the world did not arrive".into())),
        ];
        app.update();
        let events: Vec<EventKind> = log
            .after(0, std::time::Duration::ZERO)
            .events
            .into_iter()
            .map(|event| event.what)
            .collect();
        assert_eq!(
            events,
            [
                EventKind::WorldLoaded {
                    name: "Harbour".into()
                },
                EventKind::WorldLoadFailed {
                    name: "Atlantis".into(),
                    reason: "the world did not arrive".into()
                },
            ]
        );
        assert!(app.world().resource::<WorldLoadsEnded>().0.is_empty());
    }
}
