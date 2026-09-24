//! `agent inventory`, `stash`, `unstash`, `wear` and `take-off` (#1423): the
//! Inventory window's rows and buttons, with no window.
//!
//! The inventory - the stash, in the game's own words - is a record like
//! the world and the avatar: what changes here is unsaved until `agent save
//! inventory`, and `agent revert inventory` goes back to what was saved. The
//! game keeps no undo history for it, so neither does the agent.
//!
//! * `stash` copies something in: a thing in the agent's own world, by the
//!   name `placements` gives it, as the World Editor's "Save to Inventory"
//!   copies one; or a catalogue entry, by its slug, as the catalogue's "Copy
//!   to inventory" does - a wearable entry keeps what it needs to be worn.
//!   A name the stash already holds gets a suffix, as there.
//! * `wear` and `take-off` dress the avatar from the stash through the
//!   avatar editor's own helpers, so each is a step of the avatar's undo
//!   history and is saved with `agent save avatar`.
//! * `unstash` takes an item out, but not one the avatar is wearing: that
//!   comes off first, with `take-off`, as its own step.

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde_json::{Value, json};

use crate::catalogue::by_slug;
use crate::config::state::MAX_INVENTORY_ITEMS;
use crate::pds::InventoryRecord;
use crate::pds::avatar::socket_label;
use crate::pds::inventory::{WearMeta, is_drop_placeable};
use crate::state::{LiveAvatarRecord, LiveInventoryRecord, LiveRoomRecord};
use crate::ui::avatar::{
    AvatarEditorState, attach_record, is_worn_from, record_for_inventory_item, take_off_source,
    wear_blocked_reason, worn_rkeys_from,
};
use crate::ui::room::widgets::unique_key;

/// The inventory, item by item, by name.
pub(super) fn list(world: &mut World) -> Result<Value, String> {
    super::in_world(world)?;
    let unsaved = super::inventory_unsaved(world);
    let inventory = &inventory(world)?.0;
    let worn = world
        .get_resource::<LiveAvatarRecord>()
        .and_then(|live| live.0.body.rigged_ref());
    let mut names: Vec<&String> = inventory.generators.keys().collect();
    names.sort();
    let items: Vec<Value> = names
        .into_iter()
        .map(|name| {
            let generator = &inventory.generators[name];
            json!({
                "name": name,
                "kind": crate::pds::GeneratorKind::display_name(generator.kind_tag()),
                "wearable_on": inventory
                    .wear
                    .get(name)
                    .map(|meta| socket_label(&meta.socket).to_lowercase()),
                "worn": worn.is_some_and(|rig| is_worn_from(rig, name)),
            })
        })
        .collect();
    Ok(json!({
        "count": items.len(),
        "cap": MAX_INVENTORY_ITEMS,
        "unsaved": unsaved,
        "items": items,
    }))
}

fn inventory(world: &World) -> Result<&LiveInventoryRecord, String> {
    world
        .get_resource::<LiveInventoryRecord>()
        .ok_or_else(|| "the agent's inventory has not loaded yet".to_owned())
}

/// Put `what` into the inventory: a thing in the agent's own world, by its
/// name, or a catalogue entry, by its slug.
pub(super) fn stash(world: &mut World, what: &str) -> Result<Value, String> {
    super::in_world(world)?;
    let mut inventory = inventory(world)?.0.clone();
    if inventory.generators.len() >= MAX_INVENTORY_ITEMS {
        return Err(full());
    }
    let own_world = super::owns_room(world);
    let from_world = own_world
        .then(|| world.get_resource::<LiveRoomRecord>())
        .flatten()
        .and_then(|room| room.0.generators.get(what))
        .cloned();
    let (name, from, wearable) = match from_world {
        Some(generator) => {
            let name = unique_key(&inventory.generators, what);
            inventory.put_item(name.clone(), generator, None);
            (name, "world", false)
        }
        None => {
            let entry = by_slug(what).ok_or_else(|| {
                let world_part = if own_world {
                    "this world"
                } else {
                    "the catalogue (only the agent's own world's things can be stashed)"
                };
                format!(
                    "neither {world_part} nor the catalogue has anything called {what:?}; \
                     `agent placements` and `agent catalogue <words>` list them"
                )
            })?;
            let did = world
                .get_resource::<AtprotoSession>()
                .map(|session| session.did.clone())
                .ok_or("the agent is not signed in")?;
            let generator = entry.build(&did);
            if !is_drop_placeable(&generator) {
                return Err(format!("{what} is not something the inventory can hold"));
            }
            let wear = entry
                .wear_socket()
                .map(|socket| WearMeta::for_entry(socket, entry.wear_fit()));
            let name = unique_key(&inventory.generators, entry.name());
            let wearable = wear.is_some();
            inventory.put_item(name.clone(), generator, wear);
            (name, "catalogue", wearable)
        }
    };
    world.resource_mut::<LiveInventoryRecord>().0 = inventory;
    Ok(json!({ "stashed": name, "from": from, "wearable": wearable }))
}

fn full() -> String {
    format!(
        "the inventory is full ({MAX_INVENTORY_ITEMS} of {MAX_INVENTORY_ITEMS}); `agent unstash` \
         something first"
    )
}

/// Take `name` out of the inventory - not while the avatar wears it.
pub(super) fn unstash(world: &mut World, name: &str) -> Result<Value, String> {
    super::in_world(world)?;
    let inventory = &inventory(world)?.0;
    if !inventory.generators.contains_key(name) {
        return Err(no_such(inventory, name));
    }
    let worn = world
        .get_resource::<LiveAvatarRecord>()
        .and_then(|live| live.0.body.rigged_ref())
        .is_some_and(|rig| is_worn_from(rig, name));
    if worn {
        return Err(format!(
            "the avatar is wearing {name:?}; `agent take-off` it first"
        ));
    }
    let mut inventory = inventory.clone();
    inventory.remove_item(name);
    world.resource_mut::<LiveInventoryRecord>().0 = inventory;
    Ok(json!({ "unstashed": name }))
}

fn no_such(inventory: &InventoryRecord, name: &str) -> String {
    format!(
        "the inventory has nothing called {name:?} ({} items); `agent inventory` lists them",
        inventory.generators.len()
    )
}

/// Put the inventory item `name` on the avatar: a step of its undo history.
pub(super) fn wear(world: &mut World, name: &str) -> Result<Value, String> {
    super::in_world(world)?;
    let inventory = &inventory(world)?.0;
    if !inventory.generators.contains_key(name) {
        return Err(no_such(inventory, name));
    }
    let item = record_for_inventory_item(inventory, name)
        .ok_or_else(|| format!("{name:?} is not something that can be worn"))?;
    let socket = socket_label(&item.socket).to_lowercase();
    let mut avatar = world
        .get_resource::<LiveAvatarRecord>()
        .map(|live| live.0.clone())
        .ok_or("the agent has no avatar record yet")?;
    if let Some(reason) = wear_blocked_reason(Some(&avatar), true) {
        return Err(reason);
    }
    let did = world
        .get_resource::<AtprotoSession>()
        .map(|session| session.did.clone())
        .ok_or("the agent is not signed in")?;
    let rig = avatar.body.rigged_mut().ok_or("vehicles wear nothing")?;
    attach_record(rig, item, &did).ok_or("the avatar cannot take another item")?;
    super::write_avatar(world, avatar, format!("wear {name}"))?;
    Ok(json!({ "wearing": name, "on": socket }))
}

/// Take the worn item `name` off the avatar: a step of its undo history.
pub(super) fn take_off(world: &mut World, name: &str) -> Result<Value, String> {
    super::in_world(world)?;
    let mut avatar = world
        .get_resource::<LiveAvatarRecord>()
        .map(|live| live.0.clone())
        .ok_or("the agent has no avatar record yet")?;
    let rig = avatar.body.rigged_mut().ok_or("vehicles wear nothing")?;
    let detached = worn_rkeys_from(rig, name);
    if take_off_source(rig, name) == 0 {
        return Err(format!(
            "the avatar is not wearing anything called {name:?}"
        ));
    }
    super::write_avatar(world, avatar, format!("take off {name}"))?;
    // A gizmo aimed at what came off lets go of it, as when the Inventory
    // window takes it off.
    if let Some(mut editor) = world.get_resource_mut::<AvatarEditorState>() {
        editor.forget_attachments(detached);
    }
    Ok(json!({ "took_off": name }))
}

#[cfg(test)]
mod tests {
    use super::super::harness::{AGENT, OTHER, UNRESOLVABLE, app_as, app_in};
    use super::super::{EditProfile, answer};
    use super::*;
    use crate::agent::control::protocol::{EditRecord, EditRequest};
    use crate::pds::AvatarRecord;
    use crate::state::StoredInventoryRecord;
    use crate::ui::undo::AvatarUndoHistory;

    /// A wearable catalogue entry, by slug, and the name it is stashed as.
    fn wearable() -> (&'static str, &'static str) {
        let entry = crate::catalogue::ENTRIES
            .iter()
            .find(|entry| entry.wear_socket().is_some())
            .expect("a wearable entry");
        (entry.slug(), entry.name())
    }

    /// A seeded avatar with a rigged body in hand: one that can wear.
    fn rigged() -> AvatarRecord {
        (0..200)
            .map(|n| AvatarRecord::default_for_did(&format!("did:plc:inventorybody{n}")))
            .find(|avatar| {
                avatar
                    .body
                    .rigged_ref()
                    .is_some_and(|rig| rig.resolved.is_some())
            })
            .expect("a seeded rigged avatar")
    }

    fn items(app: &App) -> Vec<String> {
        let mut names: Vec<String> = app
            .world()
            .resource::<LiveInventoryRecord>()
            .0
            .generators
            .keys()
            .cloned()
            .collect();
        names.sort();
        names
    }

    /// A catalogue entry is stashed as the catalogue's "Copy to inventory"
    /// stashes it - under its name, wearable if it is - and a second copy
    /// under a name of its own.
    #[test]
    fn a_catalogue_entry_is_stashed_as_copy_to_inventory_does() {
        let (mut app, _) = app_in(AGENT);
        let (slug, name) = wearable();

        let first = stash(app.world_mut(), slug).expect("stashed");
        let second = stash(app.world_mut(), slug).expect("stashed again");

        assert_eq!(first["stashed"], name);
        assert_eq!(
            (first["from"].as_str(), first["wearable"].as_bool()),
            (Some("catalogue"), Some(true))
        );
        assert_ne!(second["stashed"], first["stashed"]);
        let inventory = &app.world().resource::<LiveInventoryRecord>().0;
        assert_eq!(inventory.generators.len(), 2);
        assert!(inventory.is_wearable(name));
    }

    /// A thing in the agent's own world is stashed by the name `placements`
    /// gives it; in someone else's world their things stay theirs.
    #[test]
    fn only_the_agents_own_world_is_stashed_from() {
        let (mut app, _) = app_in(AGENT);
        let own = stash(app.world_mut(), "owner_monument").expect("stashed");
        assert_eq!(
            (own["stashed"].as_str(), own["from"].as_str()),
            (Some("owner_monument"), Some("world"))
        );

        let (mut visiting, _) = app_in(OTHER);
        let theirs = stash(visiting.world_mut(), "owner_monument").expect_err("refused");
        assert!(theirs.contains("only the agent's own world"), "{theirs}");
        assert!(items(&visiting).is_empty());
    }

    /// A full inventory takes nothing more, with the cap in the answer.
    #[test]
    fn a_full_inventory_takes_nothing_more() {
        let (mut app, _) = app_in(AGENT);
        let (slug, _) = wearable();
        {
            let mut live = app.world_mut().resource_mut::<LiveInventoryRecord>();
            let generator = by_slug(slug).expect("an entry").build(AGENT);
            for n in 0..MAX_INVENTORY_ITEMS {
                live.0
                    .put_item(format!("item {n}"), generator.clone(), None);
            }
        }
        let why = stash(app.world_mut(), slug).expect_err("refused");
        assert!(why.contains(&format!("{MAX_INVENTORY_ITEMS} of")), "{why}");
        assert_eq!(items(&app).len(), MAX_INVENTORY_ITEMS);
    }

    /// THE SEQUENCE: stash a wearable, wear it, try to unstash it, take it
    /// off, undo the taking off. Wearing and taking off are each a step of
    /// the avatar's own history; an item on the avatar is not unstashed.
    #[test]
    fn wearing_and_taking_off_are_avatar_steps() {
        let (mut app, _) = app_in(AGENT);
        app.world_mut().insert_resource(LiveAvatarRecord(rigged()));
        app.update();
        let (slug, name) = wearable();
        stash(app.world_mut(), slug).expect("stashed");
        let worn = |app: &App| {
            app.world()
                .resource::<LiveAvatarRecord>()
                .0
                .body
                .rigged_ref()
                .is_some_and(|rig| is_worn_from(rig, name))
        };

        wear(app.world_mut(), name).expect("worn");
        app.update();
        assert!(worn(&app));
        let label = format!("wear {name}");
        assert_eq!(
            app.world().resource::<AvatarUndoHistory>().undo_label(),
            Some(label.as_str())
        );
        let why = unstash(app.world_mut(), name).expect_err("refused");
        assert!(why.contains("take-off"), "{why}");

        take_off(app.world_mut(), name).expect("taken off");
        app.update();
        assert!(!worn(&app));
        answer(app.world_mut(), EditRequest::Undo(EditRecord::Avatar)).expect("undone");
        app.update();
        assert!(worn(&app), "the taking off undone");
        assert!(take_off(app.world_mut(), "nothing worn").is_err());
    }

    /// A vehicle wears nothing, and says so rather than doing nothing.
    #[test]
    fn a_vehicle_wears_nothing() {
        let (mut app, _) = app_in(AGENT);
        let vehicle = (0..200)
            .map(|n| AvatarRecord::default_for_did(&format!("did:plc:inventoryboat{n}")))
            .find(|avatar| avatar.body.visuals().is_some())
            .expect("a seeded vehicle");
        app.world_mut().insert_resource(LiveAvatarRecord(vehicle));
        let (slug, name) = wearable();
        stash(app.world_mut(), slug).expect("stashed");

        let why = wear(app.world_mut(), name).expect_err("refused");

        assert!(why.contains("Vehicles wear nothing"), "{why}");
    }

    /// The inventory is saved and reverted like the other records, and has
    /// no undo history - which the answer says, pointing at revert.
    #[test]
    fn the_inventory_saves_and_reverts_but_has_no_undo() {
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        let (mut app, _) = app_as(UNRESOLVABLE, UNRESOLVABLE);
        let why =
            answer(app.world_mut(), EditRequest::Undo(EditRecord::Inventory)).expect_err("refused");
        assert!(why.contains("revert inventory"), "{why}");
        let (slug, _) = wearable();
        stash(app.world_mut(), slug).expect("stashed");
        assert_eq!(super::super::unsaved(app.world_mut()), ["inventory"]);

        answer(app.world_mut(), EditRequest::Revert(EditRecord::Inventory)).expect("reverted");
        assert!(items(&app).is_empty());
        stash(app.world_mut(), slug).expect("stashed again");
        answer(app.world_mut(), EditRequest::Save(EditRecord::Inventory)).expect("saving");
        let saving = app
            .world_mut()
            .query::<&crate::ui::inventory::PublishInventoryTask>()
            .iter(app.world())
            .map(|task| task.published.generators.len())
            .collect::<Vec<_>>();
        assert_eq!(saving, [1]);
        assert!(
            super::super::unsaved(app.world_mut()).is_empty(),
            "the save covers it"
        );

        app.world_mut().insert_resource(EditProfile {
            allow_save: false,
            offline: false,
        });
        let refused = answer(app.world_mut(), EditRequest::Save(EditRecord::Inventory));
        assert!(refused.unwrap_err().contains("--allow-save"));
        let stored = &app.world().resource::<StoredInventoryRecord>().0;
        assert!(
            stored.generators.is_empty(),
            "only a landed save moves what is saved"
        );
    }

    /// The listing names each item, what it is, where it is worn and
    /// whether it is on the avatar.
    #[test]
    fn the_listing_says_what_is_worn() {
        let (mut app, _) = app_in(AGENT);
        app.world_mut().insert_resource(LiveAvatarRecord(rigged()));
        let (slug, name) = wearable();
        stash(app.world_mut(), slug).expect("stashed");
        stash(app.world_mut(), "owner_monument").expect("stashed");
        wear(app.world_mut(), name).expect("worn");

        let listed = list(app.world_mut()).expect("listed");

        assert_eq!(listed["count"], 2);
        let items = listed["items"].as_array().expect("items");
        let item = |n: &str| items.iter().find(|i| i["name"] == n).expect(n).clone();
        assert_eq!(item(name)["worn"], true);
        assert!(item(name)["wearable_on"].is_string());
        assert_eq!(item("owner_monument")["worn"], false);
        assert!(item("owner_monument")["wearable_on"].is_null());
        assert_eq!(listed["unsaved"], true);
    }
}
