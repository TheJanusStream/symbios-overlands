//! `agent placements`, `catalogue`, `place`, `move` and `remove` (#1422):
//! the World Editor's placement gestures, with no mouse.
//!
//! A placement is named by its index in the world's record - the handle the
//! game's own editor uses - so removing one moves every later one down by
//! one, and `placements` is the place to look before acting on an index.
//! Each is listed where it is drawn, which is not always where its record
//! puts it: a seeded landmark is moved off water before it is set down.
//!
//! * `place` is a drop with no mouse: a catalogue entry is built for the
//!   agent, and an inventory thing (#1855) copied from the stash, each
//!   keyed the way a drop keys it - the same entry or item dropped twice
//!   shares one generator, and one that differs from a namesake the world
//!   holds is keyed apart from it - and set on the ground at its point,
//!   snapped to it, as the editor's "place at my position" sets one. A
//!   name the world already holds places another of that thing instead;
//!   `--inventory` asks the stash alone.
//! * `move` is a sideways drag: the placement's new pose is written back by
//!   the drag's own rule from where it is drawn, and it keeps its height
//!   above the ground wherever it lands.
//! * `remove` takes one placement out, as the scene menu's "Delete
//!   placement" does; the thing it placed stays in the world's record, and
//!   `place` with its name puts it back.
//!
//! Names are the world owner's words, so every listing says whose they
//! are (`named_by`).

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde_json::{Value, json};

use crate::catalogue::by_slug;
use crate::editor_gizmo::copy::InstanceCopyProxy;
use crate::pds::inventory::is_drop_placeable;
use crate::pds::{
    Fp4, Generator, GeneratorKind, InventoryRecord, Placement, RoomRecord, ScatterBounds,
};
use crate::state::{CurrentRoomDid, LiveInventoryRecord, LiveRoomRecord};
use crate::terrain::FinishedHeightMap;
use crate::ui::room::caps::Cap;
use crate::world_builder::{PlacementMarker, snap_footprint_radius, snapped_ground_y};

use super::super::status::{Pose, local_pose, thing_kind};
use super::super::{hundredths, hundredths3};

/// Every placement in the world the agent stands in, by index, with where
/// each is drawn - those within `within_m` of the agent, when given.
pub(super) fn list(world: &mut World, within_m: Option<f32>) -> Result<Value, String> {
    super::in_world(world)?;
    let pose = local_pose(world);
    let drawn = drawn_anchors(world);
    let own = super::owns_room(world);
    let named_by = world
        .get_resource::<CurrentRoomDid>()
        .map(|room| room.0.clone());
    let record = &world
        .get_resource::<LiveRoomRecord>()
        .ok_or("the world has no record yet")?
        .0;
    let placements: Vec<Value> = record
        .placements
        .iter()
        .enumerate()
        .filter_map(|(index, placement)| {
            let at = drawn.get(&index).map(|anchor| anchor.translation);
            let distance = at
                .zip(pose.as_ref())
                .map(|(at, pose)| at.distance(pose.position));
            if within_m.is_some_and(|within| distance.is_none_or(|d| d > within)) {
                return None;
            }
            Some(row(record, index, placement, at, pose.as_ref()))
        })
        .collect();
    Ok(json!({
        "named_by": named_by,
        "own_world": own,
        "count": record.placements.len(),
        "placements": placements,
    }))
}

/// One placement, as `placements` lists it.
fn row(
    record: &RoomRecord,
    index: usize,
    placement: &Placement,
    drawn: Option<Vec3>,
    pose: Option<&Pose>,
) -> Value {
    let name = generator_ref(placement);
    let kind = name
        .and_then(|name| record.generators.get(name))
        .map_or("missing", kind_word);
    let mut row = json!({
        "index": index,
        "name": name,
        "kind": kind,
        "position": drawn.map(hundredths3),
    });
    if let (Some(at), Some(pose)) = (drawn, pose) {
        let (ahead, right) = pose.frame_of(at);
        row["distance_m"] = json!(hundredths(at.distance(pose.position)));
        row["ahead_m"] = json!(hundredths(ahead));
        row["right_m"] = json!(hundredths(right));
    }
    match placement {
        Placement::Absolute {
            transform,
            snap_to_terrain,
            ..
        } => {
            row["layout"] = json!("single");
            row["yaw_deg"] = json!(hundredths(yaw_of(quat(&transform.rotation))));
            row["on_ground"] = json!(snap_to_terrain);
            // Where the record puts it, when that is not where it is drawn:
            // a seeded landmark moved off water or steep ground.
            let recorded = Vec2::new(transform.translation.0[0], transform.translation.0[2]);
            if drawn.is_some_and(|at| at.xz().distance(recorded) > MOVED_OFF_M) {
                row["recorded_at"] = json!([hundredths(recorded.x), hundredths(recorded.y)]);
            }
        }
        Placement::Scatter { count, bounds, .. } => {
            row["layout"] = json!("scatter");
            row["count"] = json!(count);
            // How far from its centre it spreads, as the census reads it.
            row["radius_m"] = json!(hundredths(match bounds {
                ScatterBounds::Circle { radius, .. } => radius.0,
                ScatterBounds::Rect { extents, .. } => extents.0[0].max(extents.0[1]),
            }));
        }
        Placement::Grid {
            transform, counts, ..
        } => {
            row["layout"] = json!("grid");
            row["count"] = json!(counts.iter().product::<u32>());
            row["yaw_deg"] = json!(hundredths(yaw_of(quat(&transform.rotation))));
        }
        Placement::Instances {
            transform,
            copies,
            variants,
            solid,
            ..
        } => {
            row["layout"] = json!("instances");
            row["count"] = json!(copies.len());
            row["variants"] = json!(variants.len().max(1));
            row["solid"] = json!(solid.word());
            row["yaw_deg"] = json!(hundredths(yaw_of(quat(&transform.rotation))));
        }
        Placement::Unknown => row["layout"] = json!("unknown"),
    }
    row
}

/// How far a placement may be drawn from where its record puts it before
/// the listing says so (m): past rounding, short of any real move.
const MOVED_OFF_M: f32 = 0.05;

/// What a placed tree is, in a word: a thing to walk into, as `status`
/// names it, or the ground, the water, the roads or the weather.
fn kind_word(generator: &Generator) -> &'static str {
    match &generator.kind {
        GeneratorKind::Terrain(_) => "terrain",
        GeneratorKind::Water { .. } => "water",
        GeneratorKind::RoadNetwork(_) => "roads",
        GeneratorKind::ParticleSystem(_) => "particles",
        _ => thing_kind(generator).unwrap_or("structure"),
    }
}

/// The generator a placement puts in the world, by its key.
fn generator_ref(placement: &Placement) -> Option<&str> {
    placement.generator_ref()
}

/// The pose each placement is drawn at, by index: the anchor every drawn
/// placement hangs from, as the gizmo finds it - not a picked copy's
/// stand-in, which carries its set's marker too (#1644).
fn drawn_anchors(world: &mut World) -> HashMap<usize, Transform> {
    world
        .query_filtered::<(&PlacementMarker, &Transform), Without<InstanceCopyProxy>>()
        .iter(world)
        .map(|(marker, anchor)| (marker.0, *anchor))
        .collect()
}

/// The catalogue's entries - each one whose slug, name, section and
/// description between them hold every word of `search`, when given
/// ([`crate::catalogue::search`], which the render tool's
/// `--catalogue-sizes` sizes by too).
pub(super) fn catalogue(search: Option<&str>) -> Value {
    let words = search.unwrap_or_default();
    let entries: Vec<Value> = crate::catalogue::search(words)
        .map(|entry| {
            json!({
                "slug": entry.slug(),
                "name": entry.name(),
                "section": entry.category().label(),
                "description": entry.description(),
                "wearable": entry.wear_socket().is_some(),
            })
        })
        .collect();
    // The Catalogue's avatars and worlds (#1843) beside its things, found by
    // what its own search box finds them by - every word somewhere, as for
    // a thing. They are worn or loaded, never placed or sized, so they list
    // apart and `entries` stays the things `render --catalogue-sizes` sizes.
    let words: Vec<String> = words.split_whitespace().map(str::to_lowercase).collect();
    let found = |matches: &dyn Fn(&str) -> bool| words.iter().all(|word| matches(word));
    let avatars: Vec<Value> = crate::catalogue::avatars::AVATARS
        .iter()
        .filter(|entry| found(&|word| entry.matches(word)))
        .map(|entry| {
            json!({
                "slug": entry.slug,
                "name": entry.name,
                "description": entry.description,
                "family": entry.family_label(),
            })
        })
        .collect();
    let worlds: Vec<Value> = crate::catalogue::worlds::WORLDS
        .iter()
        .filter(|entry| found(&|word| entry.matches(word)))
        .map(|entry| {
            json!({
                "slug": entry.slug,
                "name": entry.name,
                "description": entry.description,
                "group": entry.group(),
                "theme": entry.theme.label(),
            })
        })
        .collect();
    json!({
        "count": entries.len(),
        "entries": entries,
        "avatars": avatars,
        "worlds": worlds,
    })
}

/// Why a catalogue avatar or world named by its slug is not placed, if it
/// is one (#1843): it is worn, or loaded, instead.
fn not_placed(slug: &str) -> Option<String> {
    if let Some(entry) = crate::catalogue::avatars::avatar_by_slug(slug) {
        return Some(format!(
            "the catalogue's {slug} is an avatar ({}), which is worn, not placed: `agent wear \
             {slug}`, or `agent stash {slug}` to keep a copy",
            entry.name
        ));
    }
    crate::catalogue::worlds::world_by_slug(slug).map(|entry| {
        format!(
            "the catalogue's {slug} is a world ({}), which is loaded over the agent's own, not \
             placed: `agent load {slug}`, or `agent stash {slug}` to keep a copy",
            entry.name
        )
    })
}

/// Put `what` down in the agent's world at `at` - or ahead of the agent -
/// turned to `yaw_deg`: a thing the world already holds, by its name; else
/// an inventory thing, by its name (#1855), as a drop from the Inventory
/// window places it; else a catalogue entry, by its slug. With
/// `inventory_only`, the inventory alone is asked - for an item whose name
/// the world already holds.
pub(super) fn place(
    world: &mut World,
    what: &str,
    at: Option<[f32; 2]>,
    yaw_deg: Option<f32>,
    inventory_only: bool,
) -> Result<Value, String> {
    super::own_room(world)?;
    let pose = local_pose(world);
    let mut record = super::room_for_edit(world)?;
    if Cap::Placements.is_full(record.placements.len()) {
        return Err(Cap::Placements.full_reason());
    }
    let Found {
        from,
        generator,
        clearance,
    } = find(world, &record, what, inventory_only)?;
    // A drop's keying: a tree the world holds under the name, or under a
    // suffix of it, is shared; one that differs is never overwritten.
    let (key, added) = match generator {
        None => (what.to_owned(), false),
        Some(generator) => {
            if !is_drop_placeable(&generator) {
                return Err(format!("{what} is not something that can stand in a world"));
            }
            let generator = settled(what, generator);
            let key = crate::ui::inventory::choose_room_generator_key(
                &record.generators,
                what,
                &generator,
            );
            let added = !record.generators.contains_key(&key);
            if added {
                if Cap::Generators.is_full(record.generators.len()) {
                    return Err(Cap::Generators.full_reason());
                }
                if let Some(why) = past_the_ceiling(what, &key, &generator) {
                    return Err(why);
                }
                record.generators.insert(key.clone(), generator);
            }
            (key, added)
        }
    };
    let point = match at {
        Some(at) => Vec2::from_array(at),
        None => {
            let pose = pose.ok_or("the agent has no body yet, so there is no ahead")?;
            let ahead = clearance + crate::config::agent::PLACE_AHEAD_M;
            (pose.position + pose.forward * ahead).xz()
        }
    };
    if !point.is_finite() {
        return Err("the point is not a number".to_owned());
    }
    let mut placement = crate::ui::room::new_absolute_placement(key.clone(), point.to_array());
    if let (Some(yaw), Placement::Absolute { transform, .. }) = (yaw_deg, &mut placement) {
        transform.rotation = Fp4(yaw_rotation(yaw).to_array());
    }
    record.placements.push(placement);
    let index = record.placements.len() - 1;
    super::write_room(world, record, format!("place of {key}"))?;
    let mut answer = json!({
        "placed": {
            "index": index,
            "pointer": format!("/placements/{index}"),
            "name": key,
            "generator": format!("/generators/{}", key.replace('~', "~0").replace('/', "~1")),
            "from": from,
            "at": [hundredths(point.x), hundredths(point.y)],
            "yaw_deg": hundredths(yaw_deg.map_or(0.0, |yaw| yaw.rem_euclid(360.0))),
        },
    });
    if from != "world" {
        if let Some(adjusted) = keyed_apart(what, &key) {
            answer["adjusted"] = json!(adjusted);
        }
    } else if let Some(differs) = inventory_differs(world, what) {
        answer["inventory_differs"] = json!(differs);
    }
    // What the tree it added weighs, as `room set` says it (#1455, #1500).
    if added {
        let live = &world.resource::<LiveRoomRecord>().0;
        answer["record_size"] = super::size::room(live);
        let live_sync = super::size::live_sync(live);
        if live_sync.get("refused").is_some() {
            answer["live_sync"] = live_sync;
        }
    }
    Ok(answer)
}

/// What [`place`] found to put down: where it came from (`world`,
/// `inventory` or `catalogue`), the tree to copy into the world - none for
/// a thing the world already holds - and the room to leave round it.
struct Found {
    from: &'static str,
    generator: Option<Generator>,
    clearance: f32,
}

/// Look `what` up for [`place`]: the world's things, then the inventory's,
/// then the catalogue - or, `inventory_only`, the inventory alone.
fn find(
    world: &World,
    record: &RoomRecord,
    what: &str,
    inventory_only: bool,
) -> Result<Found, String> {
    let inventory = world
        .get_resource::<LiveInventoryRecord>()
        .map(|live| &live.0);
    if inventory_only {
        return from_inventory(inventory.ok_or_else(super::inventory::not_loaded)?, what);
    }
    if record.generators.contains_key(what) {
        return Ok(Found {
            from: "world",
            generator: None,
            clearance: PLACED_CLEARANCE_M,
        });
    }
    if let Some(inventory) = inventory.filter(|inventory| inventory.has_name(what)) {
        return from_inventory(inventory, what);
    }
    let entry = by_slug(what).ok_or_else(|| {
        not_placed(what).unwrap_or_else(|| nothing_called(what, record, inventory))
    })?;
    let did = world
        .get_resource::<AtprotoSession>()
        .map(|session| session.did.clone())
        .ok_or("the agent is not signed in")?;
    Ok(Found {
        from: "catalogue",
        generator: Some(entry.build(&did)),
        clearance: entry.footprint().clearance,
    })
}

/// The inventory item `name`, if it is a thing (#1855). A whole avatar or
/// world is refused with the verb it is for, as the Inventory window's
/// drag of one places nothing.
fn from_inventory(inventory: &InventoryRecord, name: &str) -> Result<Found, String> {
    if let Some(generator) = inventory.generators.get(name) {
        return Ok(Found {
            from: "inventory",
            generator: Some(generator.clone()),
            clearance: PLACED_CLEARANCE_M,
        });
    }
    if inventory.avatars.contains_key(name) {
        return Err(format!(
            "the inventory's {name:?} is an avatar, which is worn, not placed: `agent wear \
             {name:?}`"
        ));
    }
    if inventory.worlds.contains_key(name) {
        return Err(format!(
            "the inventory's {name:?} is a world, which is loaded over the agent's own, not \
             placed: `agent load {name:?}`"
        ));
    }
    Err(super::inventory::no_such(inventory, name))
}

/// Why nothing called `what` is placed: no world thing, inventory item or
/// catalogue entry has the name - with the names it could have meant.
fn nothing_called(what: &str, record: &RoomRecord, inventory: Option<&InventoryRecord>) -> String {
    let mut near = super::inventory::near_names(record.generators.keys().map(String::as_str), what);
    if let Some(inventory) = inventory {
        near.extend(super::inventory::near_names(
            inventory.names().map(String::as_str),
            what,
        ));
    }
    near.sort_unstable();
    near.dedup();
    near.truncate(4);
    let near = if near.is_empty() {
        String::new()
    } else {
        format!(" (near: {})", super::inventory::quoted(&near))
    };
    let unloaded = if inventory.is_none() {
        " - and the inventory has not loaded yet, so it was not asked"
    } else {
        ""
    };
    format!(
        "neither this world, the inventory nor the catalogue has anything called {what:?}{near}\
         {unloaded}; `agent placements` lists the world's things, `agent inventory` the \
         inventory's, `agent catalogue <words>` searches the catalogue"
    )
}

/// `generator` as the world holds it once written - through the record's
/// wire form and the sanitiser - so that it is compared with the world's
/// own trees, each written that way, as like with like: an inventory item
/// stashed live and not yet saved can sit off the wire's grid, and its
/// second placement would otherwise be keyed apart from its first.
fn settled(name: &str, generator: Generator) -> Generator {
    let mut alone = RoomRecord::default();
    alone.generators.clear();
    alone.placements.clear();
    alone.generators.insert(name.to_owned(), generator.clone());
    super::settle_room(alone)
        .ok()
        .and_then(|room| room.generators.into_values().next())
        .unwrap_or(generator)
}

/// Why the tree `what` brings in, kept as `key`, is not placed, if it is
/// not: alone it would be a record past the ceiling a save can write
/// (`record_size::HARD_RECORD_CEILING_BYTES`), so the world could never be
/// saved with it. A world saves a record per generator (#1455), so the tree
/// is weighed as its own record. Past the soft budget it is placed, and the
/// answer's `record_size` says so, as a `room set` does.
fn past_the_ceiling(what: &str, key: &str, generator: &Generator) -> Option<String> {
    use crate::pds::record_size::{HARD_RECORD_CEILING_BYTES, human_bytes};
    let mut alone = RoomRecord::default();
    alone.generators.clear();
    alone.placements.clear();
    alone.generators.insert(key.to_owned(), generator.clone());
    let bytes = crate::pds::room::measure_publish(&alone).bytes?;
    (bytes > HARD_RECORD_CEILING_BYTES).then(|| {
        format!(
            "{what:?} would be a record of {} in this world, past the {} a save can write, so \
             it was not placed",
            human_bytes(bytes),
            human_bytes(HARD_RECORD_CEILING_BYTES)
        )
    })
}

/// What a placement of `what` kept as `key` says about the key, if the two
/// differ: the world holds a different tree under the name, so this one
/// was keyed apart, as a drop keys it - or the sanitiser cleaned the name.
fn keyed_apart(what: &str, key: &str) -> Option<String> {
    use crate::pds::{limits::MAX_GENERATOR_NAME_CHARS, sanitize::names::with_suffix};
    if key == what {
        return None;
    }
    let clean = with_suffix(what, "", MAX_GENERATOR_NAME_CHARS);
    Some(if key == clean {
        format!("the name is kept as {key:?}, as the world's record keeps names")
    } else {
        format!(
            "the world already holds a different thing called {clean:?}, so this one is kept \
             as {key:?}: `agent place {key:?}` places another of it"
        )
    })
}

/// When `what` placed from the world also names an inventory thing whose
/// tree differs from the world's (#1855): the world's was placed, and
/// `--inventory` places the inventory's beside it.
fn inventory_differs(world: &World, what: &str) -> Option<String> {
    let item = world
        .get_resource::<LiveInventoryRecord>()?
        .0
        .generators
        .get(what)?;
    let held = world.resource::<LiveRoomRecord>().0.generators.get(what)?;
    let item = settled(what, item.clone());
    (serde_json::to_value(&item).ok() != serde_json::to_value(held).ok()).then(|| {
        format!(
            "the world's own {what:?} was placed; the inventory's {what:?} is a different \
             tree, which `agent place {what:?} --inventory` places"
        )
    })
}

/// Room left around a thing placed again from the world's own record, which
/// has no catalogue footprint to say how big it is (m).
const PLACED_CLEARANCE_M: f32 = 2.0;

/// Move placement `index` to `to`, keeping its height above the ground, and
/// turn it to `yaw_deg` when given - a sideways drag of its gizmo.
pub(super) fn move_to(
    world: &mut World,
    index: usize,
    to: Vec2,
    yaw_deg: Option<f32>,
) -> Result<Value, String> {
    super::own_room(world)?;
    if !to.is_finite() {
        return Err("the point is not a number".to_owned());
    }
    let start = drawn_anchors(world).remove(&index);
    let mut record = super::room_for_edit(world)?;
    let count = record.placements.len();
    let placement = record
        .placements
        .get_mut(index)
        .ok_or_else(|| no_such(index, count))?;
    let name = generator_ref(placement)
        .ok_or_else(|| format!("placement {index} is of a kind this build cannot move"))?
        .to_owned();
    let start = start.ok_or_else(|| {
        format!(
            "placement {index} is not drawn yet, so there is nothing to move; try again in a moment"
        )
    })?;
    let heightmap = world
        .get_resource::<FinishedHeightMap>()
        .ok_or("the ground is still being built; try again in a moment")?;
    let rotation = yaw_deg.map_or(start.rotation, |yaw| turned_to(start.rotation, yaw));
    let target = Transform {
        translation: Vec3::new(to.x, moved_height(placement, &start, to, heightmap), to.y),
        rotation,
        scale: start.scale,
    };
    crate::editor_gizmo::write_transform_into_placement(
        placement,
        &target,
        &start,
        Some(heightmap),
    );
    let moved = super::write_room(world, record, format!("move of {name}"))?;
    Ok(json!({
        "moved": {
            "index": index,
            "name": name,
            "to": [hundredths(to.x), hundredths(to.y)],
            "yaw_deg": yaw_deg.map(|yaw| hundredths(yaw.rem_euclid(360.0))),
        },
        "changed": moved,
    }))
}

/// The world height a placement is dragged to at `to`, so that it keeps its
/// height above the ground. A snapped placement keeps it by the drag's own
/// rule - its height is an offset from the ground, and a sideways drag
/// leaves the offset alone - and a scatter or a grid is set on the ground
/// wherever it goes; one that stands at a height of its own, as a
/// catalogue drop does, is raised or lowered by how much the ground under
/// it rises or falls.
fn moved_height(
    placement: &Placement,
    start: &Transform,
    to: Vec2,
    heightmap: &FinishedHeightMap,
) -> f32 {
    match placement {
        Placement::Absolute {
            snap_to_terrain: false,
            ..
        } => {
            let radius = snap_footprint_radius(placement);
            let ground = |x: f32, z: f32| snapped_ground_y(&heightmap.0, x, z, radius);
            start.translation.y + ground(to.x, to.y)
                - ground(start.translation.x, start.translation.z)
        }
        _ => start.translation.y,
    }
}

/// Take placement `index` out of the agent's world.
pub(super) fn remove(world: &mut World, index: usize) -> Result<Value, String> {
    let mut record = super::room_for_edit(world)?;
    if index >= record.placements.len() {
        return Err(no_such(index, record.placements.len()));
    }
    let removed = record.placements.remove(index);
    let name = generator_ref(&removed).map(str::to_owned);
    let still_placed = record
        .placements
        .iter()
        .filter(|placement| generator_ref(placement) == name.as_deref())
        .count();
    let label = format!("delete of {}", name.as_deref().unwrap_or("a placement"));
    super::write_room(world, record, label)?;
    Ok(json!({
        "removed": { "index": index, "name": name },
        "still_placed_elsewhere": still_placed,
    }))
}

/// Move copy `copy` of the instance set `index` to (`to.x`, `to.y`) in the
/// world (#1644): read in the set's anchor, keeping its own lift - above
/// the ground under it, where the set snaps - and turned to `yaw_deg`
/// clockwise when given.
pub(super) fn move_copy(
    world: &mut World,
    (index, copy): (usize, usize),
    to: Vec2,
    yaw_deg: Option<f32>,
) -> Result<Value, String> {
    super::own_room(world)?;
    if !to.is_finite() || yaw_deg.is_some_and(|yaw| !yaw.is_finite()) {
        return Err("the point is not a number".to_owned());
    }
    let mut record = super::room_for_edit(world)?;
    let (name, numbers) = copy_of(&mut record, index, copy)?;
    let Some(Placement::Instances { transform, .. }) = record.placements.get(index) else {
        return Err(format!("placement {index} is no instance set"));
    };
    let anchor = Transform::from(transform).with_scale(Vec3::ONE);
    let local = anchor.rotation.inverse() * (Vec3::new(to.x, 0.0, to.y) - anchor.translation);
    let mut moved = numbers;
    // As far from the anchor as the sanitiser keeps a copy, and reported
    // where it was written to.
    let reach = crate::pds::sanitize::limits::MAX_INSTANCE_OFFSET_CM;
    moved.x_cm = crate::pds::instances::centimetres(local.x).clamp(-reach, reach);
    moved.z_cm = crate::pds::instances::centimetres(local.z).clamp(-reach, reach);
    let [x, _, z] = moved.position();
    let to = anchor.translation + anchor.rotation * Vec3::new(x, 0.0, z);
    if let Some(yaw) = yaw_deg {
        let front = anchor.rotation.inverse() * yaw_rotation(yaw) * Vec3::NEG_Z;
        moved.yaw_ddeg =
            crate::pds::instances::tenths_of_degree(front.x.atan2(-front.z).to_degrees());
    }
    if let Some(Placement::Instances { copies, .. }) = record.placements.get_mut(index) {
        copies[copy] = moved;
    }
    let changed = super::write_room(world, record, format!("move of copy {copy} of {name}"))?;
    Ok(json!({
        "moved": {
            "index": index,
            "copy": copy,
            "name": name,
            "to": [hundredths(to.x), hundredths(to.z)],
            "yaw_deg": yaw_deg.map(|yaw| hundredths(yaw.rem_euclid(360.0))),
        },
        "changed": changed,
    }))
}

/// Take copy `copy` out of the instance set `index` (#1644): the copies
/// after it move down one.
pub(super) fn remove_copy(
    world: &mut World,
    (index, copy): (usize, usize),
) -> Result<Value, String> {
    let mut record = super::room_for_edit(world)?;
    let (name, _) = copy_of(&mut record, index, copy)?;
    let left = match record.placements.get_mut(index) {
        Some(Placement::Instances { copies, .. }) => {
            copies.remove(copy);
            copies.len()
        }
        _ => 0,
    };
    super::write_room(world, record, format!("delete of copy {copy} of {name}"))?;
    Ok(json!({
        "removed": { "index": index, "copy": copy, "name": name },
        "copies_left": left,
    }))
}

/// Detach copy `copy` of the instance set `index` into an absolute
/// placement of its own, drawn as it was (#1644,
/// [`crate::pds::instances::detach_copy`]).
pub(super) fn detach(world: &mut World, (index, copy): (usize, usize)) -> Result<Value, String> {
    let mut record = super::room_for_edit(world)?;
    let (name, _) = copy_of(&mut record, index, copy)?;
    let placed = crate::pds::instances::detach_copy(&mut record, index, copy)?;
    let item = record.placements[placed]
        .generator_ref()
        .unwrap_or_default()
        .to_owned();
    super::write_room(world, record, format!("detach of copy {copy} of {name}"))?;
    Ok(json!({
        "detached": { "index": index, "copy": copy, "name": name },
        "placement": { "index": placed, "name": item },
    }))
}

/// The item the instance set `index` places, and its copy `copy`'s
/// numbers - or why there is no such copy.
fn copy_of(
    record: &mut crate::pds::RoomRecord,
    index: usize,
    copy: usize,
) -> Result<(String, crate::pds::InstanceCopy), String> {
    let count = record.placements.len();
    match record.placements.get(index) {
        None => Err(no_such(index, count)),
        Some(Placement::Instances {
            generator_ref,
            copies,
            ..
        }) => copies
            .get(copy)
            .map(|numbers| (generator_ref.clone(), *numbers))
            .ok_or_else(|| {
                format!(
                    "set {index} has no copy {copy}: it has {}, numbered from 0; `agent room get \
                     /placements/{index}/copies` lists them",
                    copies.len()
                )
            }),
        Some(_) => Err(format!(
            "placement {index} is no instance set, so it has no copies; `agent move {index}` \
             moves it whole"
        )),
    }
}

fn no_such(index: usize, count: usize) -> String {
    format!(
        "there is no placement {index}: the world has {count}, numbered from 0; \
         `agent placements` lists them"
    )
}

fn quat(rotation: &Fp4) -> Quat {
    Quat::from_array(rotation.0).normalize()
}

/// Which way a rotation turns a thing, in degrees clockwise seen from above:
/// 0 when its front (-Z) faces -Z, 90 when it faces +X.
fn yaw_of(rotation: Quat) -> f32 {
    let front = rotation * Vec3::NEG_Z;
    front.x.atan2(-front.z).to_degrees().rem_euclid(360.0)
}

/// The rotation that turns a thing's front to `yaw_deg` (see [`yaw_of`]).
fn yaw_rotation(yaw_deg: f32) -> Quat {
    Quat::from_rotation_y(-yaw_deg.to_radians())
}

/// `rotation`, turned about the vertical until its front faces `yaw_deg`,
/// any tilt it has kept.
fn turned_to(rotation: Quat, yaw_deg: f32) -> Quat {
    Quat::from_rotation_y(-(yaw_deg - yaw_of(rotation)).to_radians()) * rotation
}

#[cfg(test)]
mod tests {
    use super::super::harness::{AGENT, app_in, placeable_slug};
    use super::*;
    use crate::catalogue::ENTRIES;
    use crate::pds::{Fp, Fp3, TransformData};
    use crate::state::LocalPlayer;

    fn record(app: &App) -> &RoomRecord {
        &app.world().resource::<LiveRoomRecord>().0
    }

    fn translation(placement: &Placement) -> [f32; 3] {
        match placement {
            Placement::Absolute { transform, .. } | Placement::Grid { transform, .. } => {
                transform.translation.0
            }
            other => panic!("not a posed placement: {other:?}"),
        }
    }

    fn rotation(placement: &Placement) -> Quat {
        match placement {
            Placement::Absolute { transform, .. } => quat(&transform.rotation),
            other => panic!("not a single placement: {other:?}"),
        }
    }

    /// A catalogue drop with no mouse: built for the agent, keyed by its
    /// slug, set on the ground at its point (snapped, no offset), and
    /// turned the way asked - 90 degrees clockwise faces its front to +X.
    #[test]
    fn place_puts_a_catalogue_entry_on_the_ground_where_asked() {
        let (mut app, _) = app_in(AGENT);
        let slug = placeable_slug();
        let generators = record(&app).generators.len();

        let placed = place(
            app.world_mut(),
            slug,
            Some([12.5, -3.25]),
            Some(90.0),
            false,
        )
        .expect("placed");

        let record = record(&app);
        let index = placed["placed"]["index"].as_u64().expect("an index") as usize;
        assert_eq!(index, record.placements.len() - 1);
        assert_eq!(placed["placed"]["name"], slug);
        assert_eq!(placed["placed"]["from"], "catalogue");
        assert_eq!(record.generators.len(), generators + 1);
        let placement = &record.placements[index];
        assert!(matches!(
            placement,
            Placement::Absolute { generator_ref, snap_to_terrain: true, .. } if generator_ref == slug
        ));
        assert_eq!(translation(placement), [12.5, 0.0, -3.25]);
        let front = rotation(placement) * Vec3::NEG_Z;
        assert!(front.distance(Vec3::X) < 1e-3, "front {front}");
        assert!((yaw_of(rotation(placement)) - 90.0).abs() < 0.01);
    }

    /// Dropped twice, one entry is one generator placed twice - the drop's
    /// own keying - rather than two copies of the same tree to save.
    #[test]
    fn the_same_entry_placed_twice_shares_one_generator() {
        let (mut app, _) = app_in(AGENT);
        let slug = placeable_slug();
        let generators = record(&app).generators.len();

        place(app.world_mut(), slug, Some([1.0, 1.0]), None, false).expect("placed");
        place(app.world_mut(), slug, Some([5.0, 1.0]), None, false).expect("placed again");

        let record = record(&app);
        assert_eq!(record.generators.len(), generators + 1);
        let of_it = record
            .placements
            .iter()
            .filter(|p| generator_ref(p) == Some(slug))
            .count();
        assert_eq!(of_it, 2);
    }

    /// A name the world already holds places another of that very thing -
    /// how a removed placement goes back - and nothing new is added.
    #[test]
    fn a_name_the_world_holds_places_another_of_it() {
        let (mut app, _) = app_in(AGENT);
        let generators = record(&app).generators.len();
        let placements = record(&app).placements.len();

        let placed = place(
            app.world_mut(),
            "owner_monument",
            Some([2.0, 2.0]),
            None,
            false,
        )
        .expect("placed");

        assert_eq!(placed["placed"]["from"], "world");
        assert_eq!(record(&app).generators.len(), generators);
        assert_eq!(record(&app).placements.len(), placements + 1);
        let unknown = place(app.world_mut(), "no_such_thing_anywhere", None, None, false);
        let why = unknown.expect_err("refused");
        assert!(why.contains("agent catalogue"), "{why}");
    }

    /// With no point, a thing lands ahead of the agent, clear of its body
    /// by the thing's own footprint.
    #[test]
    fn with_no_point_a_thing_lands_ahead_of_the_agent() {
        let (mut app, _) = app_in(AGENT);
        app.world_mut().spawn((
            LocalPlayer,
            GlobalTransform::from(
                Transform::from_xyz(10.0, 2.0, -20.0).looking_to(Vec3::Z, Vec3::Y),
            ),
        ));
        let slug = placeable_slug();
        let clearance = by_slug(slug).expect("an entry").footprint().clearance;

        place(app.world_mut(), slug, None, None, false).expect("placed");

        let placement = record(&app).placements.last().expect("placed");
        let [x, _, z] = translation(placement);
        let ahead = clearance + crate::config::agent::PLACE_AHEAD_M;
        assert!(
            (x - 10.0).abs() < 1e-3 && (z - (-20.0 + ahead)).abs() < 1e-3,
            "({x}, {z})"
        );
    }

    /// A full world refuses the next placement with the editor's own
    /// sentence, and is left as it was.
    #[test]
    fn a_full_world_refuses_another_placement() {
        let (mut app, _) = app_in(AGENT);
        {
            let mut live = app.world_mut().resource_mut::<LiveRoomRecord>();
            let one = live.0.placements[0].clone();
            live.0.placements.resize(Cap::Placements.max(), one);
        }
        let refused = place(
            app.world_mut(),
            placeable_slug(),
            Some([0.0, 0.0]),
            None,
            false,
        );
        assert_eq!(refused.unwrap_err(), Cap::Placements.full_reason());
        assert_eq!(record(&app).placements.len(), Cap::Placements.max());
    }

    /// 129 x 129 at 1 m, world -64..64, the ground rising 0.25 m per metre
    /// of x - the gizmo tests' map.
    fn slope() -> FinishedHeightMap {
        let mut hm = bevy_symbios_ground::HeightMap::new(129, 129, 1.0);
        for z in 0..129 {
            for x in 0..129 {
                hm.set(x, z, 0.25 * (x as f32 - 64.0));
            }
        }
        FinishedHeightMap(hm, None)
    }

    /// A placement of `generator_ref` at `at` with its anchor drawn there
    /// on the slope, `lift` above the ground, as the compile would draw it.
    fn drawn(app: &mut App, placement: Placement, lift: f32) -> usize {
        let at = Vec3::from_array(translation(&placement));
        let radius = snap_footprint_radius(&placement);
        let ground = snapped_ground_y(&slope().0, at.x, at.z, radius);
        let world = app.world_mut();
        let mut live = world.resource_mut::<LiveRoomRecord>();
        live.0.placements.push(placement);
        let index = live.0.placements.len() - 1;
        world.spawn((
            PlacementMarker(index),
            Transform::from_xyz(at.x, ground + lift, at.z),
        ));
        index
    }

    fn single(at: [f32; 3], on_ground: bool) -> Placement {
        Placement::Absolute {
            generator_ref: "owner_monument".into(),
            transform: TransformData {
                translation: Fp3(at),
                ..TransformData::default()
            },
            snap_to_terrain: on_ground,
            avoid_water: false,
            avoid_water_clearance: Fp(0.0),
            seed: None,
            params: Default::default(),
        }
    }

    /// A move up the slope keeps a thing's height above the ground, both
    /// ways it can have one: snapped, its record keeps the same offset;
    /// standing at a height of its own, as a catalogue drop stands, it is
    /// raised by how far the ground rose.
    #[test]
    fn a_move_keeps_the_height_above_the_ground() {
        let (mut app, _) = app_in(AGENT);
        app.world_mut().insert_resource(slope());
        let snapped = drawn(&mut app, single([0.0, 0.5, 0.0], true), 0.5);
        let standing = drawn(&mut app, single([4.0, 1.0, 0.0], false), 0.0);

        move_to(app.world_mut(), snapped, Vec2::new(8.0, 3.0), None).expect("moved");
        let moved = move_to(app.world_mut(), standing, Vec2::new(12.0, 3.0), None).expect("moved");
        assert_eq!(moved["changed"], true);

        let record = record(&app);
        assert_eq!(translation(&record.placements[snapped]), [8.0, 0.5, 3.0]);
        // Drawn at 1.0 on ground 1.0 at x 4: at x 12 the ground is 3.0.
        let [x, y, z] = translation(&record.placements[standing]);
        assert_eq!((x, z), (12.0, 3.0));
        assert!((y - 3.0).abs() < 1e-3, "{y}");
    }

    /// Turning keeps whatever tilt a thing has, and a move asks nothing of
    /// a placement the compile has not drawn yet.
    #[test]
    fn a_turn_keeps_the_tilt_and_an_undrawn_thing_is_not_moved() {
        let (mut app, _) = app_in(AGENT);
        app.world_mut().insert_resource(slope());
        let tilt = Quat::from_rotation_x(0.2);
        let mut tilted = single([0.0, 0.0, 0.0], true);
        if let Placement::Absolute { transform, .. } = &mut tilted {
            transform.rotation = Fp4(tilt.to_array());
        }
        let index = drawn(&mut app, tilted, 0.0);
        app.world_mut()
            .query::<(&PlacementMarker, &mut Transform)>()
            .iter_mut(app.world_mut())
            .for_each(|(marker, mut anchor)| {
                if marker.0 == index {
                    anchor.rotation = tilt;
                }
            });

        move_to(app.world_mut(), index, Vec2::new(1.0, 0.0), Some(135.0)).expect("moved");

        let turned = rotation(&record(&app).placements[index]);
        assert!((yaw_of(turned) - 135.0).abs() < 0.05, "{}", yaw_of(turned));
        let up = |q: Quat| (q * Vec3::Y).y;
        assert!((up(turned) - up(tilt)).abs() < 1e-4, "the tilt kept");

        let mut undrawn = record(&app).clone();
        undrawn.placements.push(single([5.0, 0.0, 5.0], true));
        let last = undrawn.placements.len() - 1;
        app.world_mut().resource_mut::<LiveRoomRecord>().0 = undrawn;
        let refused = move_to(app.world_mut(), last, Vec2::new(6.0, 6.0), None);
        assert!(refused.unwrap_err().contains("not drawn yet"));
    }

    /// Remove takes that index out, and says whether what it placed still
    /// stands elsewhere; an index past the end is refused, naming the count.
    #[test]
    fn remove_takes_one_placement_out() {
        let (mut app, _) = app_in(AGENT);
        let slug = placeable_slug();
        place(app.world_mut(), slug, Some([1.0, 1.0]), None, false).expect("placed");
        place(app.world_mut(), slug, Some([2.0, 1.0]), None, false).expect("placed");
        let count = record(&app).placements.len();

        let removed = remove(app.world_mut(), count - 2).expect("removed");

        assert_eq!(removed["removed"]["name"], slug);
        assert_eq!(removed["still_placed_elsewhere"], 1);
        assert_eq!(record(&app).placements.len(), count - 1);
        assert_eq!(translation(record(&app).placements.last().unwrap())[0], 2.0);
        let why = remove(app.world_mut(), count).expect_err("refused");
        assert!(why.contains(&format!("has {}", count - 1)), "{why}");
    }

    /// #1644: a copy of an instance set is moved in the world's frame - read
    /// in its set's turned anchor, its lift kept - removed, or detached into
    /// a placement of its own; a copy the set lacks is refused, naming how
    /// many it has.
    #[test]
    fn a_sets_copy_is_moved_removed_and_detached() {
        use crate::pds::{InstanceCopy, InstanceSolid};
        let (mut app, _) = app_in(AGENT);
        let slug = placeable_slug();
        place(app.world_mut(), slug, Some([1.0, 1.0]), None, false).expect("placed");
        let index = record(&app).placements.len() - 1;
        let name = generator_ref(&record(&app).placements[index])
            .expect("an item")
            .to_owned();
        app.world_mut()
            .resource_mut::<LiveRoomRecord>()
            .0
            .placements[index] = Placement::Instances {
            generator_ref: name.clone(),
            transform: TransformData {
                translation: Fp3([10.0, 0.0, 0.0]),
                rotation: Fp4(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2).to_array()),
                ..TransformData::default()
            },
            snap_to_terrain: true,
            solid: InstanceSolid::Shell,
            variants: Vec::new(),
            copies: vec![
                InstanceCopy {
                    y_cm: 20,
                    ..InstanceCopy::at(0.0, 0.0, 0.0)
                },
                InstanceCopy::at(2.0, 0.0, 0.0),
                InstanceCopy::at(4.0, 0.0, 0.0),
            ],
        };
        let copies = |app: &App| match &record(app).placements[index] {
            Placement::Instances { copies, .. } => copies.clone(),
            other => panic!("a set, not {other:?}"),
        };

        move_copy(
            app.world_mut(),
            (index, 0),
            Vec2::new(10.0, 3.0),
            Some(90.0),
        )
        .expect("moved");
        let moved = copies(&app)[0];
        assert_eq!((moved.x_cm, moved.z_cm), (300, 0), "read in the anchor");
        assert_eq!(moved.yaw_ddeg, 0, "the anchor's own turn");
        assert_eq!(moved.y_cm, 20, "its lift kept");

        let removed = remove_copy(app.world_mut(), (index, 1)).expect("removed");
        assert_eq!(removed["copies_left"], 2);
        let detached = detach(app.world_mut(), (index, 0)).expect("detached");
        let placed = detached["placement"]["index"].as_u64().expect("an index") as usize;
        assert_eq!(placed, record(&app).placements.len() - 1);
        assert_eq!(copies(&app).len(), 1);
        assert!(matches!(
            &record(&app).placements[placed],
            Placement::Absolute { generator_ref, .. } if *generator_ref == name
        ));

        let why = move_copy(app.world_mut(), (index, 9), Vec2::ZERO, None).expect_err("refused");
        assert!(why.contains("has 1"), "{why}");
        let why = remove_copy(app.world_mut(), (placed, 0)).expect_err("refused");
        assert!(why.contains("no instance set"), "{why}");
    }

    /// The listing names each placement where it is drawn - and, where the
    /// compile moved one off its recorded spot, that spot too.
    #[test]
    fn the_listing_shows_where_things_are_drawn() {
        let (mut app, _) = app_in(AGENT);
        app.world_mut().insert_resource(slope());
        let here = drawn(&mut app, single([2.0, 0.0, 2.0], true), 0.0);
        let moved_off = drawn(&mut app, single([10.0, 0.0, 10.0], true), 0.0);
        app.world_mut()
            .query::<(&PlacementMarker, &mut Transform)>()
            .iter_mut(app.world_mut())
            .for_each(|(marker, mut anchor)| {
                if marker.0 == moved_off {
                    anchor.translation.x += 3.0;
                }
            });

        let listed = list(app.world_mut(), None).expect("listed");

        let rows = listed["placements"].as_array().expect("rows");
        assert_eq!(rows.len(), record(&app).placements.len());
        assert_eq!(rows[here]["index"], here);
        assert_eq!(rows[here]["position"][0], 2.0);
        assert!(rows[here].get("recorded_at").is_none());
        assert_eq!(rows[moved_off]["position"][0], 13.0);
        assert_eq!(
            rows[moved_off]["recorded_at"],
            serde_json::json!([10.0, 10.0])
        );
        assert_eq!(listed["named_by"], AGENT);
    }

    /// Every word of a search has to be somewhere in an entry.
    #[test]
    fn a_catalogue_search_matches_every_word() {
        let everything = catalogue(None);
        let slug = placeable_slug();
        let one = catalogue(Some(slug));
        let none = catalogue(Some(&format!("{slug} zzqxunlikely")));

        assert_eq!(everything["count"], ENTRIES.len());
        assert!(
            one["entries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["slug"] == slug)
        );
        assert_eq!(none["count"], 0);
    }

    /// The Catalogue's avatars and worlds list beside its things (#1843),
    /// every word of a search somewhere in each, as the Catalogue's own
    /// search box finds them - and never among the things.
    #[test]
    fn a_catalogue_search_lists_its_avatars_and_worlds_apart() {
        use crate::catalogue::avatars::AVATARS;
        use crate::catalogue::worlds::WORLDS;
        let everything = catalogue(None);
        assert_eq!(
            everything["avatars"].as_array().unwrap().len(),
            AVATARS.len()
        );
        assert_eq!(everything["worlds"].as_array().unwrap().len(), WORLDS.len());
        assert!(
            everything["entries"]
                .as_array()
                .unwrap()
                .iter()
                .all(|e| !e["slug"].as_str().unwrap().starts_with("avatar_")
                    && !e["slug"].as_str().unwrap().starts_with("worlds_"))
        );

        let world = &WORLDS[0];
        let found = catalogue(Some(&format!("{} WORLD", world.slug)));
        assert_eq!(
            found["worlds"].as_array().unwrap()[0]["slug"],
            world.slug,
            "{found}"
        );
        assert!(found["avatars"].as_array().unwrap().is_empty());
        let avatar = &AVATARS[0];
        let found = catalogue(Some(avatar.slug));
        assert_eq!(found["avatars"].as_array().unwrap()[0]["slug"], avatar.slug);
        let none = catalogue(Some(&format!("{} zzqxunlikely", avatar.slug)));
        assert!(none["avatars"].as_array().unwrap().is_empty());

        // Neither is placed: each names the verb it is for.
        let (mut app, _) = super::super::harness::app_in(AGENT);
        let why =
            place(app.world_mut(), avatar.slug, Some([1.0, 1.0]), None, false).expect_err("worn");
        assert!(why.contains("`agent wear"), "{why}");
        let why =
            place(app.world_mut(), world.slug, Some([1.0, 1.0]), None, false).expect_err("loaded");
        assert!(why.contains("`agent load"), "{why}");
    }

    /// `render --catalogue-sizes <words>` sizes what this lists for the same
    /// words, in the same order (#1466): an agent sizes a search's hits in
    /// one run, so a second matcher there - by name alone, say - would size
    /// a different set than the one it is choosing from.
    #[test]
    fn the_render_tools_size_search_selects_this_listing() {
        let slugs = |listing: &Value| -> Vec<String> {
            listing["entries"]
                .as_array()
                .expect("entries")
                .iter()
                .map(|e| e["slug"].as_str().expect("a slug").to_owned())
                .collect()
        };
        for words in [
            "lantern",
            "scrap",
            "Rust corrugated",
            "tree",
            "",
            "zzqxunlikely",
        ] {
            let split: Vec<String> = words.split_whitespace().map(str::to_owned).collect();
            let sized: Vec<&str> = crate::render_tool::sizes::select(&split)
                .iter()
                .map(|entry| entry.slug())
                .collect();
            assert_eq!(sized, slugs(&catalogue(Some(words))), "{words:?}");
        }
        // Not vacuous: `scrap` finds entries by their descriptions, which a
        // matcher by name would miss.
        let by_name = ENTRIES
            .iter()
            .filter(|entry| entry.name().to_lowercase().contains("scrap"))
            .count();
        assert!(slugs(&catalogue(Some("scrap"))).len() > by_name);
    }

    /// An inventory thing to place (#1855): a cottage-shaped cuboid with a
    /// door child, from wire JSON - the form `room set` takes - each decimal
    /// off the wire's grid in its float form only through the sanitiser.
    fn cottage() -> Generator {
        serde_json::from_value(serde_json::json!({
            "$type": "network.symbios.gen.cuboid", "size": [40000, 30000, 50000], "solid": true,
            "children": [{
                "$type": "network.symbios.gen.cuboid", "size": [6200, 9000, 1000],
                "solid": false, "transform": {"translation": [0, 4500, 25000]}
            }]
        }))
        .expect("wire JSON")
    }

    const COTTAGE: &str = "Cat Cottage";

    /// The agent standing in `room`, its inventory holding [`cottage`] as
    /// "Cat Cottage", the way a gift accepted from the owner leaves it.
    fn given_a_cottage(room: &str) -> App {
        let (mut app, _) = super::super::harness::app_in(room);
        let given = app
            .world_mut()
            .resource_mut::<LiveInventoryRecord>()
            .0
            .put_item(COTTAGE.into(), cottage(), None);
        assert!(given);
        app
    }

    fn value(generator: &Generator) -> Value {
        serde_json::to_value(generator).expect("serialises")
    }

    /// THE CASE (#1855, live session 20): the owner gave Catoid the Cat
    /// Cottage, and `place "Cat Cottage"` answered that neither the world
    /// nor the catalogue had it. Now it lands as a drop from the Inventory
    /// window lands it: the item's tree copied into the record under the
    /// item's name, an absolute placement of it on the ground where asked,
    /// turned as asked - written, settled and read back, every placement
    /// naming a generator the record holds - and the stash keeps the item,
    /// since a drop copies it. `--inventory` places it the same way.
    #[test]
    fn an_inventory_thing_is_placed_as_a_drop_places_it() {
        let mut app = given_a_cottage(AGENT);
        let generators = record(&app).generators.len();
        assert!(!record(&app).generators.contains_key(COTTAGE));

        let placed = place(
            app.world_mut(),
            COTTAGE,
            Some([3.0, 4.0]),
            Some(160.0),
            false,
        )
        .expect("placed");

        let held = record(&app);
        let index = held.placements.len() - 1;
        assert_eq!(placed["placed"]["index"], index);
        assert_eq!(placed["placed"]["pointer"], format!("/placements/{index}"));
        assert_eq!(placed["placed"]["name"], COTTAGE);
        assert_eq!(placed["placed"]["generator"], "/generators/Cat Cottage");
        assert_eq!(placed["placed"]["from"], "inventory");
        assert!(placed.get("adjusted").is_none(), "{placed}");
        assert!(placed["record_size"]["bytes"].is_u64(), "{placed}");
        assert_eq!(held.generators.len(), generators + 1);
        assert_eq!(
            value(&held.generators[COTTAGE]),
            value(&cottage()),
            "the item's own tree, children and all"
        );
        let placement = &held.placements[index];
        assert!(matches!(
            placement,
            Placement::Absolute { generator_ref, snap_to_terrain: true, .. } if generator_ref == COTTAGE
        ));
        assert_eq!(translation(placement), [3.0, 0.0, 4.0]);
        assert!((yaw_of(rotation(placement)) - 160.0).abs() < 0.01);
        assert!(super::super::json::missing_generators(held).is_empty());
        assert!(
            app.world()
                .resource::<LiveInventoryRecord>()
                .0
                .generators
                .contains_key(COTTAGE),
            "a drop copies; the item stays in the stash"
        );

        let mut app = given_a_cottage(AGENT);
        let placed = place(app.world_mut(), COTTAGE, Some([3.0, 4.0]), None, true).expect("placed");
        assert_eq!(placed["placed"]["from"], "inventory");
        assert_eq!(
            record(&app).placements.last().and_then(generator_ref),
            Some(COTTAGE)
        );
    }

    /// Placed twice, an inventory thing is one generator placed twice, as two
    /// drops of it are. A different tree the world already holds under its
    /// name is never overwritten: the item is keyed apart (`_2`), the answer
    /// says so, and its second placement shares that `_2` rather than adding
    /// a `_3` copy. Without `--inventory`, the world's own namesake is the
    /// one placed, and the answer says the inventory's differs.
    #[test]
    fn a_second_placement_of_an_inventory_thing_shares_its_generator() {
        let mut app = given_a_cottage(AGENT);
        let generators = record(&app).generators.len();
        place(app.world_mut(), COTTAGE, Some([1.0, 1.0]), None, false).expect("placed");
        let again = place(app.world_mut(), COTTAGE, Some([9.0, 1.0]), None, false).expect("again");
        assert_eq!(again["placed"]["name"], COTTAGE);
        assert_eq!(record(&app).generators.len(), generators + 1);
        let of_it = |app: &App, key: &str| {
            record(app)
                .placements
                .iter()
                .filter(|p| generator_ref(p) == Some(key))
                .count()
        };
        assert_eq!(of_it(&app, COTTAGE), 2);

        // A namesake that differs: the owner's own "Cat Cottage", a shed.
        let mut app = given_a_cottage(AGENT);
        let shed = Generator::default_cuboid();
        app.world_mut()
            .resource_mut::<LiveRoomRecord>()
            .0
            .generators
            .insert(COTTAGE.into(), shed.clone());
        let generators = record(&app).generators.len();
        let apart = place(app.world_mut(), COTTAGE, Some([1.0, 1.0]), None, true).expect("placed");
        let kept_as = "Cat Cottage_2";
        assert_eq!(apart["placed"]["name"], kept_as);
        assert!(
            apart["adjusted"]
                .as_str()
                .is_some_and(|why| why.contains("different thing") && why.contains(kept_as)),
            "{apart}"
        );
        let again = place(app.world_mut(), COTTAGE, Some([9.0, 1.0]), None, true).expect("again");
        assert_eq!(again["placed"]["name"], kept_as, "{again}");
        {
            let held = record(&app);
            assert_eq!(held.generators.len(), generators + 1, "no _3 copy");
            assert_eq!(value(&held.generators[kept_as]), value(&cottage()));
            assert_eq!(
                value(&held.generators[COTTAGE]),
                value(&settled(COTTAGE, shed)),
                "the world's own namesake untouched"
            );
        }
        assert_eq!(of_it(&app, kept_as), 2);

        let worlds =
            place(app.world_mut(), COTTAGE, Some([5.0, 5.0]), None, false).expect("placed");
        assert_eq!(worlds["placed"]["from"], "world");
        assert!(
            worlds["inventory_differs"]
                .as_str()
                .is_some_and(|why| why.contains("--inventory")),
            "{worlds}"
        );
    }

    /// Each refusal says why, in a sentence that names what to do instead,
    /// and leaves the world as it was: a whole avatar or world in the
    /// inventory (worn or loaded, never placed), a name nothing holds (with
    /// the item it could have meant), an inventory not loaded yet, a thing
    /// that cannot stand in a world, and anyone else's world.
    #[test]
    fn placing_from_the_inventory_refuses_what_a_drop_would_not_place() {
        use crate::pds::AvatarRecord;
        use crate::pds::inventory::{AvatarItem, WorldItem};
        let mut app = given_a_cottage(AGENT);
        {
            let mut live = app.world_mut().resource_mut::<LiveInventoryRecord>();
            let body =
                AvatarItem::from_record(&AvatarRecord::default_for_did(AGENT)).expect("copies");
            assert!(live.0.put_avatar("Spare Body".into(), body));
            let harbour = WorldItem::snapshot(&RoomRecord::default_for_did(AGENT)).expect("copies");
            assert!(live.0.put_world("Harbour".into(), harbour));
            let ground =
                Generator::from_kind(crate::ui::room::construct::make_default_for_kind("Terrain"));
            assert!(live.0.put_item("Hill".into(), ground, None));
        }
        let before = value_of(&app);
        let ask = |app: &mut App, what: &str, only: bool| {
            place(app.world_mut(), what, Some([1.0, 1.0]), None, only).expect_err("refused")
        };

        for only in [false, true] {
            let why = ask(&mut app, "Spare Body", only);
            assert!(
                why.contains("is an avatar") && why.contains("`agent wear"),
                "{why}"
            );
            let why = ask(&mut app, "Harbour", only);
            assert!(
                why.contains("is a world") && why.contains("`agent load"),
                "{why}"
            );
            let why = ask(&mut app, "Hill", only);
            assert!(why.contains("not something that can stand"), "{why}");
            let why = ask(&mut app, "cat_cottage", only);
            assert!(why.contains("\"Cat Cottage\""), "the near name: {why}");
        }
        let why = ask(&mut app, "cat_cottage", true);
        assert!(why.contains("the inventory has nothing called"), "{why}");
        let why = ask(&mut app, "cat_cottage", false);
        assert!(
            why.contains("agent inventory") && why.contains("agent catalogue"),
            "{why}"
        );

        app.world_mut().remove_resource::<LiveInventoryRecord>();
        let why = ask(&mut app, COTTAGE, true);
        assert!(why.contains("has not loaded yet"), "{why}");
        let why = ask(&mut app, COTTAGE, false);
        assert!(why.contains("has not loaded yet"), "{why}");
        assert_eq!(value_of(&app), before, "nothing written");

        let mut visiting = given_a_cottage(super::super::harness::OTHER);
        let before = value_of(&visiting);
        let why = ask(&mut visiting, COTTAGE, true);
        assert!(why.contains("not the agent's world"), "{why}");
        assert_eq!(value_of(&visiting), before);
    }

    fn value_of(app: &App) -> Value {
        serde_json::to_value(record(app)).expect("serialises")
    }

    /// A tree that, alone, would be a record past what a save can write is
    /// not placed - the world could never be saved with it - and the world
    /// is left as it was. Weighed as the world would hold it, sanitised (a
    /// tree of more nodes than a generator may hold is cut to fit, so the
    /// weight here is in grammar text each node may carry), and measured,
    /// so the test cannot pass on a tree that fits.
    #[test]
    fn an_inventory_thing_past_the_save_ceiling_is_not_placed() {
        use crate::pds::record_size::HARD_RECORD_CEILING_BYTES;
        let mut grammar =
            Generator::from_kind(crate::ui::room::construct::make_default_for_kind("Shape"));
        if let GeneratorKind::Shape { grammar_source, .. } = &mut grammar.kind {
            *grammar_source = format!("// {}", "a".repeat(16_000));
        }
        let mut heavy = Generator::default_cuboid();
        heavy.children = vec![grammar; 70];
        let mut alone = RoomRecord::default();
        alone.generators.clear();
        alone
            .generators
            .insert("Mountain".into(), settled("Mountain", heavy.clone()));
        let bytes = crate::pds::room::measure_publish(&alone)
            .bytes
            .expect("measured");
        assert!(bytes > HARD_RECORD_CEILING_BYTES, "{bytes} bytes fits");
        let mut app = given_a_cottage(AGENT);
        let given = app
            .world_mut()
            .resource_mut::<LiveInventoryRecord>()
            .0
            .put_item("Mountain".into(), heavy, None);
        assert!(given);
        let before = value_of(&app);

        let why =
            place(app.world_mut(), "Mountain", Some([1.0, 1.0]), None, true).expect_err("refused");

        assert!(why.contains("past the") && why.contains("save"), "{why}");
        assert_eq!(value_of(&app), before);
        // And the cottage beside it still goes in.
        place(app.world_mut(), COTTAGE, Some([1.0, 1.0]), None, true).expect("placed");
    }
}
