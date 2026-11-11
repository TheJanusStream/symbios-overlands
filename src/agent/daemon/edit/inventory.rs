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
//!   A name the stash already holds gets a suffix, as there. And a part of
//!   the agent's own body, by its JSON pointer, under a name of the agent's
//!   choosing (#1444): the avatar is the one record the agent may edit in
//!   anyone's world, so a thing built on its body can be stashed - and
//!   gifted - wherever it is.
//! * `wear` and `take-off` dress the avatar from the stash through the
//!   avatar editor's own helpers, so each is a step of the avatar's undo
//!   history and is saved with `agent save avatar`.
//! * `unstash` takes an item out, but not one the avatar is wearing: that
//!   comes off first, with `take-off`, as its own step.
//!
//! Since #1843 the inventory holds whole avatars and whole worlds beside
//! things, in one namespace, and the listing says which each item is. Their
//! own commands live in `stash.rs`: `stash-avatar`, `stash-world`, `load`,
//! and `wear` of an avatar item. `stash` and `wear` given a catalogue
//! avatar's or world's slug go there too, as the Catalogue's Copy to
//! inventory and Wear do.

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde_json::{Value, json};

use crate::catalogue::by_slug;
use crate::config::state::MAX_INVENTORY_ITEMS;
use crate::pds::InventoryRecord;
use crate::pds::avatar::socket_label;
use crate::pds::inventory::{AvatarItem, WearMeta, is_drop_placeable};
use crate::state::{LiveAvatarRecord, LiveInventoryRecord, LiveRoomRecord};
use crate::ui::avatar::{
    AvatarEditorState, attach_record, is_worn_from, record_for_inventory_item, take_off_source,
    wear_blocked_reason, wear_refused_reason, worn_rkeys_from,
};

/// The inventory, item by item, by name: every kind in one list, each
/// saying what it `is` - a `thing`, an `avatar` or a `world` (#1843).
pub(super) fn list(world: &mut World) -> Result<Value, String> {
    super::in_world(world)?;
    let unsaved = super::inventory_unsaved(world);
    let inventory = &inventory(world)?.0;
    let avatar = world.get_resource::<LiveAvatarRecord>().map(|live| &live.0);
    let worn = avatar.and_then(|avatar| avatar.body.rigged_ref());
    // What the avatar worn would be as an item: an avatar item equal to it
    // is the one worn, as the game's Wear tells it is already on.
    let wearing = avatar.and_then(|avatar| AvatarItem::from_record(avatar).ok());
    let mut names: Vec<&String> = inventory.names().collect();
    names.sort();
    let items: Vec<Value> = names
        .into_iter()
        .filter_map(|name| {
            if let Some(generator) = inventory.generators.get(name) {
                return Some(json!({
                    "name": name,
                    "is": "thing",
                    "kind": crate::pds::GeneratorKind::display_name(generator.kind_tag()),
                    "wearable_on": inventory
                        .wear
                        .get(name)
                        .map(|meta| socket_label(&meta.socket).to_lowercase()),
                    "worn": worn.is_some_and(|rig| is_worn_from(rig, name)),
                }));
            }
            if let Some(item) = inventory.avatars.get(name) {
                let mut listed = super::stash::describe_avatar(item);
                listed["name"] = json!(name);
                listed["worn"] = json!(wearing.as_ref() == Some(item));
                return Some(listed);
            }
            inventory.worlds.get(name).map(|item| {
                let mut listed = super::stash::describe_world(item);
                listed["name"] = json!(name);
                listed
            })
        })
        .collect();
    Ok(json!({
        "count": items.len(),
        "cap": MAX_INVENTORY_ITEMS,
        "things": inventory.generators.len(),
        "avatars": inventory.avatars.len(),
        "worlds": inventory.worlds.len(),
        "unsaved": unsaved,
        "items": items,
    }))
}

fn inventory(world: &World) -> Result<&LiveInventoryRecord, String> {
    world
        .get_resource::<LiveInventoryRecord>()
        .ok_or_else(not_loaded)
}

/// Why nothing can be taken from the inventory yet.
pub(super) fn not_loaded() -> String {
    "the agent's inventory has not loaded yet".to_owned()
}

/// Put `what` into the inventory: a thing in the agent's own world, by its
/// name, or a catalogue entry, by its slug - or, given `from_avatar`, the
/// part of the agent's body at that pointer, named `what`.
pub(super) fn stash(
    world: &mut World,
    what: &str,
    from_avatar: Option<&str>,
) -> Result<Value, String> {
    super::in_world(world)?;
    let own_world = super::owns_room(world);
    let in_own_world = |what: &str| {
        own_world
            && world
                .get_resource::<LiveRoomRecord>()
                .is_some_and(|room| room.0.generators.contains_key(what))
    };
    // A catalogue avatar or world, by its slug, as the Catalogue's Copy to
    // inventory copies it (#1843) - refused, the inventory full among the
    // rest, in its own words. A thing of the agent's own world by that
    // name comes first, as below.
    if from_avatar.is_none() && !in_own_world(what) {
        if let Some(entry) = crate::catalogue::avatars::avatar_by_slug(what) {
            return super::stash::copy_avatar(world, entry);
        }
        if let Some(entry) = crate::catalogue::worlds::world_by_slug(what) {
            return super::stash::copy_world(world, entry);
        }
    }
    let mut inventory = inventory(world)?.0.clone();
    // Every kind of item counts against the cap (#1838).
    if inventory.is_full() {
        return Err(full());
    }
    if let Some(pointer) = from_avatar {
        let name = what.trim();
        if name.is_empty() {
            return Err("a part of the body is stashed under a name: give one".to_owned());
        }
        // No `is_drop_placeable` gate, unlike a catalogue entry: the avatar
        // sanitiser has already turned any terrain, water, portal or gateway
        // node into a plain cuboid, and a kind this build cannot name never
        // serialises far enough to be read here.
        let generator = super::avatar::body_part(world, pointer)?;
        let name = inventory.free_name(name);
        inventory.put_item(name.clone(), generator, None);
        world.resource_mut::<LiveInventoryRecord>().0 = inventory;
        return Ok(json!({ "stashed": name, "from": "avatar", "wearable": false }));
    }
    let from_world = own_world
        .then(|| world.get_resource::<LiveRoomRecord>())
        .flatten()
        .and_then(|room| room.0.generators.get(what))
        .cloned();
    let (name, from, wearable) = match from_world {
        Some(generator) => {
            let name = inventory.free_name(what);
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
            let name = inventory.free_name(entry.name());
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
    // An item of any kind (#1843): an avatar or a world comes out as a
    // thing does.
    if !inventory.has_name(name) {
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

pub(super) fn no_such(inventory: &InventoryRecord, name: &str) -> String {
    // Items go by name, not by slug: `stash satchel` answers that it stashed
    // "Traveler's Satchel", and `wear satchel` found nothing (#1484). A slug
    // is named back as the items it was stashed as.
    let stashed_as = by_slug(name).map(|entry| {
        let mut names: Vec<&String> = inventory
            .generators
            .keys()
            .filter(|item| stashed_from(item, entry.name()))
            .collect();
        names.sort();
        names
    });
    match stashed_as {
        Some(names) if !names.is_empty() => format!(
            "the inventory has nothing called {name:?}: items go by name, and the catalogue's \
             {name} was stashed as {} - `agent inventory` lists them",
            names
                .iter()
                .take(4)
                .map(|n| format!("{n:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => {
            let near = near_names(inventory.names().map(String::as_str), name);
            let near = if near.is_empty() {
                String::new()
            } else {
                format!(" - it has {}", quoted(&near))
            };
            format!(
                "the inventory has nothing called {name:?} ({} items){near}; `agent inventory` \
                 lists them",
                inventory.item_count()
            )
        }
    }
}

/// The names among `names` that `asked` could have meant, sorted, at most
/// four (#1855): the same words in another case, or written with `_` or
/// `-` for a space - `cat_cottage` for the "Cat Cottage" an owner gave -
/// or one holding the other's words whole.
pub(super) fn near_names<'a>(names: impl Iterator<Item = &'a str>, asked: &str) -> Vec<&'a str> {
    let words = |name: &str| {
        name.to_lowercase()
            .replace(['_', '-'], " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let asked = words(asked);
    if asked.is_empty() {
        return Vec::new();
    }
    let mut near: Vec<&str> = names
        .filter(|name| {
            let name = words(name);
            name == asked
                || (asked.len() >= 3 && name.contains(&asked))
                || (name.len() >= 3 && asked.contains(&name))
        })
        .collect();
    near.sort_unstable();
    near.dedup();
    near.truncate(4);
    near
}

/// `names`, each quoted, joined with commas.
pub(super) fn quoted(names: &[&str]) -> String {
    names
        .iter()
        .map(|n| format!("{n:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Whether `item` is a name a catalogue entry called `entry` is stashed
/// under: the name itself, or the name and the `_N` a further copy takes.
/// Not every name that starts the same - "Banner Pole" was named back as
/// what the catalogue's "Banner" was stashed as (the session 879 review).
fn stashed_from(item: &str, entry: &str) -> bool {
    item == entry
        || item
            .strip_prefix(entry)
            .and_then(|rest| rest.strip_prefix('_'))
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Why nothing called `name` is worn: a catalogue slug is named back as the
/// worn items it was stashed as, as `no_such` does for the inventory
/// (#1484: `take-off satchel` said only that nothing was called "satchel").
fn not_worn(rig: &crate::pds::avatar::RiggedBody, name: &str) -> String {
    let worn_as: Vec<String> = by_slug(name)
        .map(|entry| {
            let mut sources: Vec<String> = rig
                .resolved
                .iter()
                .flat_map(|resolved| &resolved.attachments)
                .filter_map(|worn| worn.record.source.clone())
                .filter(|source| stashed_from(source, entry.name()))
                .collect();
            sources.sort();
            sources.dedup();
            sources
        })
        .unwrap_or_default();
    if worn_as.is_empty() {
        format!("the avatar is not wearing anything called {name:?}")
    } else {
        format!(
            "the avatar is not wearing anything called {name:?}: worn items go by the name they \
             were stashed as, and the catalogue's {name} is worn as {}",
            worn_as
                .iter()
                .take(4)
                .map(|n| format!("{n:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// Put the inventory item `name` on the avatar: a step of its undo history.
/// An avatar item, or a catalogue avatar by its slug, is worn whole instead
/// (#1843).
pub(super) fn wear(world: &mut World, name: &str) -> Result<Value, String> {
    super::in_world(world)?;
    let inventory = &inventory(world)?.0;
    if let Some(item) = inventory.avatars.get(name) {
        let item = item.clone();
        return super::stash::wear_item(world, name, &item);
    }
    if inventory.worlds.contains_key(name) {
        return Err(format!(
            "{name:?} is a world, which is loaded over the agent's own world, not worn: `agent \
             load {name:?}`"
        ));
    }
    if !inventory.generators.contains_key(name) {
        if let Some(entry) = crate::catalogue::avatars::avatar_by_slug(name) {
            return super::stash::wear_entry(world, entry);
        }
        return Err(no_such(inventory, name));
    }
    let item = record_for_inventory_item(inventory, name)
        .ok_or_else(|| format!("{name:?} is not something that can be worn"))?;
    let socket = socket_label(&item.socket).to_lowercase();
    let mut avatar = world
        .get_resource::<LiveAvatarRecord>()
        .map(|live| live.0.clone())
        .ok_or("the agent has no avatar record yet")?;
    if let Some(reason) = wear_blocked_reason(Some(&avatar), true)
        .or_else(|| wear_refused_reason(Some(&avatar), &item.socket, item.fit_beast_mm))
    {
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
        return Err(not_worn(rig, name));
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

    /// A seeded avatar with a person's rigged body in hand, which can wear
    /// what is cut for a person: since #1882 a seeded rigged body may be
    /// winged, and a winged body is refused a prop on its back.
    fn rigged() -> AvatarRecord {
        use crate::seeded_defaults::ChassisFamily;
        (0..200)
            .map(|n| format!("did:plc:inventorybody{n}"))
            .filter(|did| ChassisFamily::for_did(did) == ChassisFamily::Humanoid)
            .map(|did| AvatarRecord::default_for_did(&did))
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

        let first = stash(app.world_mut(), slug, None).expect("stashed");
        let second = stash(app.world_mut(), slug, None).expect("stashed again");

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

    /// #1484, met dressing Reeve live: `stash satchel` answered that it
    /// stashed "Traveler's Satchel", and `wear satchel` then said only that
    /// nothing was called "satchel". A slug is named back as what it was
    /// stashed as.
    #[test]
    fn a_slug_is_named_back_as_what_it_was_stashed_as() {
        let (mut app, _) = app_in(AGENT);
        let (slug, name) = wearable();
        stash(app.world_mut(), slug, None).expect("stashed");

        let why = wear(app.world_mut(), slug).expect_err("refused");

        assert!(why.contains(&format!("stashed as {name:?}")), "{why}");
        assert!(!why.contains("  "), "{why}");
        let plain = wear(app.world_mut(), "no such thing").expect_err("refused");
        assert!(plain.contains("(1 items)"), "{plain}");
    }

    /// A thing in the agent's own world is stashed by the name `placements`
    /// gives it; in someone else's world their things stay theirs.
    #[test]
    fn only_the_agents_own_world_is_stashed_from() {
        let (mut app, _) = app_in(AGENT);
        let own = stash(app.world_mut(), "owner_monument", None).expect("stashed");
        assert_eq!(
            (own["stashed"].as_str(), own["from"].as_str()),
            (Some("owner_monument"), Some("world"))
        );

        let (mut visiting, _) = app_in(OTHER);
        let theirs = stash(visiting.world_mut(), "owner_monument", None).expect_err("refused");
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
        let why = stash(app.world_mut(), slug, None).expect_err("refused");
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
        stash(app.world_mut(), slug, None).expect("stashed");
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

        // #1484's take-off half (the session 879 review): with the item
        // worn again by the undo, its slug names what it is worn as
        let why = take_off(app.world_mut(), slug).expect_err("refused");
        assert!(why.contains(&format!("worn as {name:?}")), "{why}");
        assert!(!why.contains("  "), "{why}");
    }

    /// The session 879 review: stashed names were matched by what they
    /// start with, so the catalogue's "Banner" was named back as stashed as
    /// "Banner Pole". Only the name itself and its `_N` copies count.
    #[test]
    fn a_slug_is_not_named_back_as_another_entry_that_starts_the_same() {
        let (mut app, _) = app_in(AGENT);
        let pole = by_slug("banner_pole").expect("an entry").name();
        let banner = by_slug("banner").expect("an entry").name();
        assert!(
            pole.starts_with(banner),
            "the premise: {pole:?} starts with {banner:?}"
        );
        stash(app.world_mut(), "banner_pole", None).expect("stashed");

        let why = unstash(app.world_mut(), "banner").expect_err("refused");

        assert!(!why.contains("stashed as"), "{why}");
        assert!(stashed_from(&format!("{banner}_2"), banner));
        assert!(!stashed_from(&format!("{banner}_x"), banner));
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
        stash(app.world_mut(), slug, None).expect("stashed");

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
        stash(app.world_mut(), slug, None).expect("stashed");
        assert_eq!(super::super::unsaved(app.world_mut()), ["inventory"]);

        answer(app.world_mut(), EditRequest::Revert(EditRecord::Inventory)).expect("reverted");
        assert!(items(&app).is_empty());
        stash(app.world_mut(), slug, None).expect("stashed again");
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

    /// #1845 (end review), THE REAL PATH: a save fails without a known
    /// outcome - it may have landed - and the agent reverts. `live` equals
    /// `stored` again, but the data server may hold the edit, so the
    /// inventory stays unsaved and `save inventory` is taken, not refused
    /// as "nothing to save": the save that follows plans against what the
    /// data server holds and puts the reverted stash back. A landed save
    /// settles it.
    #[test]
    fn a_revert_after_a_failed_save_can_still_be_saved() {
        use bevy::ecs::system::RunSystemOnce;
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        let (mut app, _) = app_as(UNRESOLVABLE, UNRESOLVABLE);
        let world = app.world_mut();
        world.init_resource::<crate::diagnostics::SessionLog>();
        world.init_resource::<crate::diagnostics::MetricsRegistry>();
        world.init_resource::<crate::ui::toolbar::UiPanels>();
        world.init_resource::<crate::notify::Toasts>();
        let land = |app: &mut App, result: Result<(), String>| {
            let world = app.world_mut();
            let mut tasks = world.query::<&mut crate::ui::inventory::PublishInventoryTask>();
            for mut task in tasks.iter_mut(world) {
                let result = result.clone();
                task.task = bevy::tasks::IoTaskPool::get().spawn(async move { result });
            }
            for _ in 0..2_000 {
                world
                    .run_system_once(crate::ui::inventory::poll_publish_inventory_tasks)
                    .expect("the poll runs");
                if world
                    .query::<&crate::ui::inventory::PublishInventoryTask>()
                    .iter(world)
                    .next()
                    .is_none()
                {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            panic!("the save never landed");
        };
        let (slug, _) = wearable();
        stash(app.world_mut(), slug, None).expect("stashed");
        answer(app.world_mut(), EditRequest::Save(EditRecord::Inventory)).expect("saving");
        land(
            &mut app,
            Err(String::from("Saving your inventory timed out after 30s")),
        );

        answer(app.world_mut(), EditRequest::Revert(EditRecord::Inventory)).expect("reverted");
        assert!(items(&app).is_empty(), "live is stored again");
        assert_eq!(
            super::super::unsaved(app.world_mut()),
            ["inventory"],
            "the data server may hold the edit"
        );
        answer(app.world_mut(), EditRequest::Save(EditRecord::Inventory))
            .expect("taken, not refused as nothing to save");

        land(&mut app, Ok(()));
        assert!(super::super::unsaved(app.world_mut()).is_empty(), "settled");
        let refused = answer(app.world_mut(), EditRequest::Save(EditRecord::Inventory))
            .expect_err("nothing to save now");
        assert!(refused.contains("nothing to save"), "{refused}");
    }

    /// The listing names each item, what it is, where it is worn and
    /// whether it is on the avatar.
    #[test]
    fn the_listing_says_what_is_worn() {
        let (mut app, _) = app_in(AGENT);
        app.world_mut().insert_resource(LiveAvatarRecord(rigged()));
        let (slug, name) = wearable();
        stash(app.world_mut(), slug, None).expect("stashed");
        stash(app.world_mut(), "owner_monument", None).expect("stashed");
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

    /// The listing names every kind in one list, in name order, each saying
    /// what it `is` (#1843): a thing as before, an avatar with its kind and
    /// whether it is the one worn, a world with its kind and size - and the
    /// count and the cap count them all.
    #[test]
    fn the_listing_shows_every_kind() {
        let (mut app, _) = app_in(AGENT);
        let worn = vehicle();
        app.world_mut()
            .insert_resource(LiveAvatarRecord(worn.clone()));
        {
            let mut live = app.world_mut().resource_mut::<LiveInventoryRecord>();
            live.0.put_item(
                "b crate".into(),
                crate::pds::Generator::default_cuboid(),
                None,
            );
            live.0.put_avatar(
                "a boat".into(),
                AvatarItem::from_record(&worn).expect("copies"),
            );
            live.0.put_avatar(
                "d person".into(),
                AvatarItem::from_record(&rigged()).expect("copies"),
            );
            live.0.put_world(
                "c harbour".into(),
                crate::pds::inventory::WorldItem::snapshot(
                    &crate::pds::RoomRecord::default_for_did(AGENT),
                )
                .expect("copies"),
            );
        }

        let listed = list(app.world_mut()).expect("listed");

        assert_eq!(listed["count"], 4);
        assert_eq!(
            (&listed["things"], &listed["avatars"], &listed["worlds"]),
            (&json!(1), &json!(2), &json!(1))
        );
        let items = listed["items"].as_array().expect("items");
        let names: Vec<&str> = items.iter().map(|i| i["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["a boat", "b crate", "c harbour", "d person"]);
        let is: Vec<&str> = items.iter().map(|i| i["is"].as_str().unwrap()).collect();
        assert_eq!(is, ["avatar", "thing", "world", "avatar"]);
        assert_eq!(items[0]["worn"], true, "the avatar on is the one worn");
        assert_eq!(items[3]["worn"], false);
        assert_eq!(
            items[3]["kind"],
            AvatarItem::from_record(&rigged())
                .expect("copies")
                .kind()
                .label()
        );
        assert_eq!(items[1]["kind"], "Box", "a thing's kind as before");
        assert!(
            items[2]["kind"]
                .as_str()
                .is_some_and(|kind| kind.starts_with("a ")),
            "{}",
            items[2]
        );
        assert!(items[2]["bytes"].as_u64().is_some_and(|bytes| bytes > 0));
        assert_eq!(items[2]["held"], true);
    }

    /// A seeded avatar with a generator body - a vehicle, whose drawn parts
    /// are nodes of its own tree.
    fn vehicle() -> AvatarRecord {
        (0..200)
            .map(|n| AvatarRecord::default_for_did(&format!("did:plc:inventorycraft{n}")))
            .find(|avatar| avatar.body.visuals().is_some())
            .expect("a seeded vehicle")
    }

    /// Build a thing on the body the way the agent does - `avatar set` of
    /// a new last child of the body's root - and give its pointer.
    fn build_on_body(app: &mut App) -> String {
        let built = serde_json::json!({
            "$type": "network.symbios.gen.cuboid",
            "size": [4000, 3000, 4000],
            "solid": false,
            "transform": {
                "translation": [3000, 150, -2000],
                "rotation": [0, 7071, 0, 7071],
            },
            "children": [{
                "$type": "network.symbios.gen.sphere",
                "radius": 1000,
                "resolution": 2,
                "solid": false,
                "transform": { "translation": [0, 2500, 0] },
            }],
        });
        super::super::avatar::set(app.world_mut(), "/record/body/visuals/children/-", built)
            .expect("built on the body");
        let children = app
            .world()
            .resource::<LiveAvatarRecord>()
            .0
            .body
            .visuals()
            .expect("a generator body")
            .children
            .len();
        format!("/record/body/visuals/children/{}", children - 1)
    }

    /// A part of the agent's own body is stashed under the name it is
    /// given (#1444), the nodes under it with it, standing on its own
    /// origin: where it sat on the body is dropped, its turn is kept. The
    /// body keeps it - a stash is a copy - and a second one gets a name of
    /// its own.
    #[test]
    fn a_part_of_the_body_is_stashed_under_the_name_given() {
        let (mut app, _) = app_in(AGENT);
        app.world_mut().insert_resource(LiveAvatarRecord(vehicle()));
        app.update();
        let pointer = build_on_body(&mut app);
        let parts_before = app.world().resource::<LiveAvatarRecord>().0.clone();

        let first = stash(app.world_mut(), "Birdhouse", Some(&pointer)).expect("stashed");
        let second = stash(app.world_mut(), "Birdhouse", Some(&pointer)).expect("again");

        assert_eq!(
            (
                first["stashed"].as_str(),
                first["from"].as_str(),
                first["wearable"].as_bool()
            ),
            (Some("Birdhouse"), Some("avatar"), Some(false))
        );
        assert_ne!(second["stashed"], first["stashed"]);
        let inventory = &app.world().resource::<LiveInventoryRecord>().0;
        let item = &inventory.generators["Birdhouse"];
        assert!(
            matches!(item.kind, crate::pds::GeneratorKind::Cuboid { .. }),
            "{:?}",
            item.kind
        );
        assert_eq!(item.transform.translation.0, [0.0, 0.0, 0.0]);
        // Its turn, in the wire form it was built with (a quarter turn).
        assert_eq!(
            serde_json::to_value(&item.transform).expect("serialises")["rotation"],
            serde_json::json!([0, 7071, 0, 7071])
        );
        assert_eq!(item.children.len(), 1, "the nodes under it come too");
        assert_eq!(item.children[0].transform.translation.0, [0.0, 0.25, 0.0]);
        assert_eq!(
            app.world().resource::<LiveAvatarRecord>().0,
            parts_before,
            "the body keeps what was stashed from it"
        );
    }

    /// Only a node of the body's own tree is a part: not another piece of
    /// the record, not a name that merely starts like the tree's, not a
    /// value inside a node, not a node that is not there - and never
    /// without a name. Nothing reaches the inventory from any of them.
    #[test]
    fn only_a_node_of_the_bodys_tree_is_stashed() {
        let (mut app, _) = app_in(AGENT);
        app.world_mut().insert_resource(LiveAvatarRecord(vehicle()));
        app.update();
        let pointer = build_on_body(&mut app);
        let refused = |app: &mut App, name: &str, pointer: &str| {
            stash(app.world_mut(), name, Some(pointer)).expect_err(pointer)
        };

        let elsewhere = refused(&mut app, "x", "/record/locomotion");
        let lookalike = refused(&mut app, "x", "/record/body/visualsx");
        let a_value = refused(&mut app, "x", &format!("{pointer}/size"));
        let missing = refused(&mut app, "x", "/record/body/visuals/children/999");
        let unnamed = refused(&mut app, "  ", &pointer);

        assert!(
            elsewhere.contains("not a part of the avatar's body"),
            "{elsewhere}"
        );
        assert!(
            lookalike.contains("not a part of the avatar's body"),
            "{lookalike}"
        );
        assert!(
            a_value.contains("is not a node of the body's tree"),
            "{a_value}"
        );
        assert!(missing.contains("nothing is at"), "{missing}");
        assert!(unnamed.contains("give one"), "{unnamed}");
        assert!(items(&app).is_empty());
    }

    /// A rigged body is a sculpt, not a tree of parts: nothing to stash.
    #[test]
    fn a_rigged_body_has_no_parts_to_stash() {
        let (mut app, _) = app_in(AGENT);
        app.world_mut().insert_resource(LiveAvatarRecord(rigged()));
        app.update();

        let why = stash(app.world_mut(), "x", Some("/record/body/visuals")).expect_err("refused");

        assert!(
            why.contains("a rigged body keeps its shape in a sculpt"),
            "{why}"
        );
        assert!(items(&app).is_empty());
    }
}
