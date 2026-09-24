//! `agent placements`, `catalogue`, `place`, `move` and `remove` (#1422):
//! the World Editor's placement gestures, with no mouse.
//!
//! A placement is named by its index in the world's record - the handle the
//! game's own editor uses - so removing one moves every later one down by
//! one, and `placements` is the place to look before acting on an index.
//! Each is listed where it is drawn, which is not always where its record
//! puts it: a seeded landmark is moved off water before it is set down.
//!
//! * `place` is a catalogue drop: the entry is built for the agent, keyed
//!   the way a drop keys it - the same entry dropped twice shares one
//!   generator - and set on the ground at its point, snapped to it, as the
//!   editor's "place at my position" sets one. A name the world already
//!   holds places another of that thing instead.
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

use crate::catalogue::{ENTRIES, by_slug};
use crate::pds::inventory::is_drop_placeable;
use crate::pds::{Fp4, Generator, GeneratorKind, Placement, RoomRecord, ScatterBounds};
use crate::state::{CurrentRoomDid, LiveRoomRecord};
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
    match placement {
        Placement::Absolute { generator_ref, .. }
        | Placement::Scatter { generator_ref, .. }
        | Placement::Grid { generator_ref, .. } => Some(generator_ref),
        Placement::Unknown => None,
    }
}

/// The pose each placement is drawn at, by index: the anchor every drawn
/// placement hangs from, as the gizmo finds it.
fn drawn_anchors(world: &mut World) -> HashMap<usize, Transform> {
    world
        .query::<(&PlacementMarker, &Transform)>()
        .iter(world)
        .map(|(marker, anchor)| (marker.0, *anchor))
        .collect()
}

/// The catalogue's entries - each one whose slug, name, section and
/// description between them hold every word of `search`, when given.
pub(super) fn catalogue(search: Option<&str>) -> Value {
    let words: Vec<String> = search
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    let entries: Vec<Value> = ENTRIES
        .iter()
        .filter(|entry| {
            let text = format!(
                "{} {} {} {}",
                entry.slug(),
                entry.name(),
                entry.category().label(),
                entry.description()
            )
            .to_lowercase();
            words.iter().all(|word| text.contains(word))
        })
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
    json!({ "count": entries.len(), "entries": entries })
}

/// Put `what` down in the agent's world at `at` - or ahead of the agent -
/// turned to `yaw_deg`: a thing the world already holds, by its name, or a
/// catalogue entry, by its slug.
pub(super) fn place(
    world: &mut World,
    what: &str,
    at: Option<[f32; 2]>,
    yaw_deg: Option<f32>,
) -> Result<Value, String> {
    super::own_room(world)?;
    let pose = local_pose(world);
    let mut record = super::room_for_edit(world)?;
    if Cap::Placements.is_full(record.placements.len()) {
        return Err(Cap::Placements.full_reason());
    }
    let (key, from, clearance) = if record.generators.contains_key(what) {
        (what.to_owned(), "world", PLACED_CLEARANCE_M)
    } else {
        let entry = by_slug(what).ok_or_else(|| {
            format!(
                "neither this world nor the catalogue has anything called {what:?}; \
                 `agent placements` lists the world's things, `agent catalogue <words>` \
                 searches the catalogue"
            )
        })?;
        if Cap::Generators.is_full(record.generators.len()) {
            return Err(Cap::Generators.full_reason());
        }
        let did = world
            .get_resource::<AtprotoSession>()
            .map(|session| session.did.clone())
            .ok_or("the agent is not signed in")?;
        let generator = entry.build(&did);
        if !is_drop_placeable(&generator) {
            return Err(format!("{what} is not something that can stand in a world"));
        }
        let key =
            crate::ui::inventory::choose_room_generator_key(&record.generators, what, &generator);
        record.generators.entry(key.clone()).or_insert(generator);
        (key, "catalogue", entry.footprint().clearance)
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
    Ok(json!({
        "placed": {
            "index": index,
            "name": key,
            "from": from,
            "at": [hundredths(point.x), hundredths(point.y)],
            "yaw_deg": hundredths(yaw_deg.map_or(0.0, |yaw| yaw.rem_euclid(360.0))),
        },
    }))
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

        let placed = place(app.world_mut(), slug, Some([12.5, -3.25]), Some(90.0)).expect("placed");

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

        place(app.world_mut(), slug, Some([1.0, 1.0]), None).expect("placed");
        place(app.world_mut(), slug, Some([5.0, 1.0]), None).expect("placed again");

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

        let placed =
            place(app.world_mut(), "owner_monument", Some([2.0, 2.0]), None).expect("placed");

        assert_eq!(placed["placed"]["from"], "world");
        assert_eq!(record(&app).generators.len(), generators);
        assert_eq!(record(&app).placements.len(), placements + 1);
        let unknown = place(app.world_mut(), "no_such_thing_anywhere", None, None);
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

        place(app.world_mut(), slug, None, None).expect("placed");

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
        let refused = place(app.world_mut(), placeable_slug(), Some([0.0, 0.0]), None);
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
        FinishedHeightMap(hm)
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
        place(app.world_mut(), slug, Some([1.0, 1.0]), None).expect("placed");
        place(app.world_mut(), slug, Some([2.0, 1.0]), None).expect("placed");
        let count = record(&app).placements.len();

        let removed = remove(app.world_mut(), count - 2).expect("removed");

        assert_eq!(removed["removed"]["name"], slug);
        assert_eq!(removed["still_placed_elsewhere"], 1);
        assert_eq!(record(&app).placements.len(), count - 1);
        assert_eq!(translation(record(&app).placements.last().unwrap())[0], 2.0);
        let why = remove(app.world_mut(), count).expect_err("refused");
        assert!(why.contains(&format!("has {}", count - 1)), "{why}");
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
}
