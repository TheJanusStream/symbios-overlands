//! `--first-view` and `--album` (#1768): what a visitor sees on arriving in
//! a seeded world, as numbers and as pictures - the instrument the
//! seeded-world overhaul (#1767) is judged on, before and after each change.
//!
//! **The arrival camera** is the game's own on arrival (#1785): behind the
//! body set down at the landing, facing the way the landing faces, at the
//! rest radius and pitch ([`ORBIT_RADIUS`](crate::config::camera::ORBIT_RADIUS),
//! [`ORBIT_PITCH`](crate::config::camera::ORBIT_PITCH)), looking at the
//! body's origin a metre over the ground
//! ([`SPAWN_HEIGHT_OFFSET`](crate::config::rover::SPAWN_HEIGHT_OFFSET)), and
//! slid in along its ray where rising ground behind the body would hide it,
//! as the player's camera is ([`crate::camera::clear_of_terrain`]). Its lens
//! is the game's ([`crate::camera::perspective_lens`]) on a 16:9 frame. At the
//! rest pitch the frame's top edge is a fraction of a degree under the
//! horizon, so whatever stands above the camera's eye is out of the picture.
//!
//! **`--first-view a..b`** (or a seed, or a DID) prints a row per procedural
//! seed - Berlin's squares are skipped, their ground is fetched - and a
//! summary of the rows:
//!
//! - `hour`: the hour the world is drawn at, `turns` for a day that turns
//!   (read at this machine's clock), `-` for a world with no time of day.
//! - `alt`, `sun`: the key light's altitude, and how far its bearing is
//!   from the view's - 0 looking into it, 180 with it behind the camera.
//!   `moon` says whether the key light is the moon.
//! - `weather`: kind, intensity, share of the time and spell length.
//! - `slope`: the ground's slope under the landing.
//! - `lm_m`, `lm_off`: how far the landmark stands from the landing, and
//!   how far its bearing is from the view's.
//! - `seen`: the share of the landmark's height, from its foot to its drawn
//!   top, that is both in the frame and over the terrain from the camera.
//! - `trees`, `rocks`, `built`: tree-stand trees, boulders and seeded
//!   structures in the frame within [`REACH_M`] of the body, anywhere up
//!   their height - the seeder's names: `tree_scatter_*`, `boulder`, and
//!   every snapped, water-avoiding absolute placement but the landmark.
//! - `in_way`: how many trees, boulders and structures, near or far, stand
//!   within [`IN_WAY_M`] of the line from the camera to the landmark,
//!   between the two and short of the landmark's own ground - the gate
//!   among them wherever it stands on that line, as a seeded one does.
//!
//! **`--album a..b`** draws each procedural seed's arrival frame through this
//! binary's own `--world N --focus arrival`, `--album-jobs` at a time, and
//! lays the frames out in one contact sheet, each tile marked with its seed,
//! written to `--out`. `--hour`, `--weather`, `--effects` and `--grass` pass
//! through to every frame.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::math::Affine3A;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;

use crate::catalogue::items::measure::{is_primitive, transform_of};
use crate::pds::{Environment, Generator, Placement, RoomRecord};
use crate::terrain::FinishedHeightMap;

/// How far ahead of the body the first view's trees, rocks and buildings are
/// counted, metres.
pub(super) const REACH_M: f32 = 40.0;

/// How near the line from the camera to the landmark a thing stands to be in
/// its way, metres.
pub(super) const IN_WAY_M: f32 = 3.0;

/// The frame's width over its height: the game is judged on 16:9.
const ASPECT: f32 = 16.0 / 9.0;

/// How many points up the landmark's height are looked for.
const AXIS_SAMPLES: usize = 12;

/// The heights over its ground a tree, a rock or a building is looked for at
/// in the frame, metres: a trunk, a boulder and a wall reach this high.
const STANDS_UP_M: [f32; 6] = [0.3, 1.0, 2.0, 3.5, 5.5, 8.0];

/// The most frames one album holds.
const MAX_ALBUM: usize = 48;

/// The game's arrival camera over a world (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ArrivalView {
    /// The body's origin, which the camera looks at.
    pub(super) focus: Vec3,
    /// Where the camera stands.
    pub(super) eye: Vec3,
    /// The landing's facing, the spawn path's yaw in degrees.
    pub(super) yaw_deg: f32,
}

impl ArrivalView {
    /// The arrival over `terrain`: the body set down at `record`'s landing
    /// as the compile sets it down ([`crate::world_builder::compile::landing_on`]),
    /// the camera behind it. `None` for a record with no landing.
    pub(super) fn of(record: &RoomRecord, terrain: &FinishedHeightMap) -> Option<Self> {
        let landing =
            crate::world_builder::compile::landing_on(record, terrain).or(record.default_landing)?;
        let [x, z] = landing.pos.0;
        let floor = terrain.world_height_at(x, z) + crate::config::rover::SPAWN_HEIGHT_OFFSET;
        let focus = Vec3::new(x, landing.y.map_or(floor, |y| y.0.max(floor)), z);
        Some(Self::around(focus, landing.yaw_deg.0, terrain))
    }

    /// The camera behind a body at `focus` facing `yaw_deg`, pulled in off
    /// `terrain` as the player's is.
    pub(super) fn around(focus: Vec3, yaw_deg: f32, terrain: &FinishedHeightMap) -> Self {
        let eye = crate::camera::clear_of_terrain(
            focus,
            rest_eye(focus, yaw_deg),
            &crate::state::LocalSettings::default(),
            terrain,
        );
        Self {
            focus,
            eye,
            yaw_deg,
        }
    }

    /// The way the body faces, horizontal and unit: -Z turned by the yaw.
    pub(super) fn facing(&self) -> Vec3 {
        crate::player::spawn_facing(self.yaw_deg) * Vec3::NEG_Z
    }

    /// Whether `point` is in the frame.
    pub(super) fn frames(&self, point: Vec3) -> bool {
        let forward = (self.focus - self.eye).normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(forward);
        let to = point - self.eye;
        let depth = to.dot(forward);
        let half_high = (crate::camera::perspective_lens().fov * 0.5).tan();
        depth > 0.0
            && to.dot(up).abs() <= depth * half_high
            && to.dot(right).abs() <= depth * half_high * ASPECT
    }
}

/// Where the orbit camera stands round `focus` at the rest radius and pitch,
/// turned to look the way a body facing `yaw_deg` does - the orbit's own
/// construction, yaw about the vertical and then pitch - before any terrain
/// pulls it in.
pub(super) fn rest_eye(focus: Vec3, yaw_deg: f32) -> Vec3 {
    // A turn about the vertical always has a heading.
    let yaw = crate::camera::yaw_behind(crate::player::spawn_facing(yaw_deg)).unwrap_or(0.0);
    let rotation =
        Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-crate::config::camera::ORBIT_PITCH);
    focus + rotation * Vec3::new(0.0, 0.0, crate::config::camera::ORBIT_RADIUS)
}

/// Whether the straight line from `from` to `to` keeps over the ground: no
/// point along it, about a metre apart, is under the terrain - the last
/// `short` metres before `to` aside, the ground the target itself stands on.
pub(super) fn in_sight(terrain: &FinishedHeightMap, from: Vec3, to: Vec3, short: f32) -> bool {
    let length = from.distance(to);
    let steps = (length.ceil() as usize).max(1);
    (1..steps).all(|k| {
        let t = k as f32 / steps as f32;
        let point = from.lerp(to, t);
        (1.0 - t) * length < short || point.y >= terrain.world_height_at(point.x, point.z)
    })
}

/// The angle between two directions seen from above, degrees, 0 to 180;
/// `None` where either has no horizontal part.
fn bearing_apart(a: Vec3, b: Vec3) -> Option<f32> {
    let a = Vec2::new(a.x, a.z).try_normalize()?;
    let b = Vec2::new(b.x, b.z).try_normalize()?;
    Some(a.dot(b).clamp(-1.0, 1.0).acos().to_degrees())
}

/// How far `point` stands from the segment `from`..`to` seen from above, and
/// how far along it its nearest point is (0 at `from`, 1 at `to`).
fn off_segment(point: Vec2, from: Vec2, to: Vec2) -> (f32, f32) {
    let along = to - from;
    let t = if along.length_squared() > 0.0 {
        ((point - from).dot(along) / along.length_squared()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (point.distance(from + along * t), t)
}

/// The highest point `root` draws, in its own frame - its root's transform
/// included: every primitive's corners, and every terminal of every shape
/// grammar in it, each terminal's mesh bounds placed as the world places it.
/// `None` for a tree that draws nothing this reads.
pub(super) fn drawn_top(root: &Generator) -> Option<f32> {
    fn visit(node: &Generator, world: Affine3A, top: &mut Option<f32>) {
        let mut raise = |y: f32| *top = Some(top.map_or(y, |t: f32| t.max(y)));
        if is_primitive(&node.kind) {
            for p in crate::world_builder::primitive_points(&node.kind) {
                raise(world.transform_point3(p).y);
            }
        } else if let Some(def) = node.kind.shape_def()
            && let Ok(model) = def.derive()
        {
            // Terminals of one key share a mesh, as the spawner shares them.
            let mut boxes = HashMap::new();
            for terminal in &model.terminals {
                let bake = def.bake(terminal);
                let (lo, hi) = *boxes
                    .entry(bake.cache_key())
                    .or_insert_with(|| mesh_box(&bake.mesh()));
                let placed = world * bake.transform.compute_affine();
                for corner in 0..8 {
                    let pick = |bit: usize, a: f32, b: f32| if corner & bit == 0 { a } else { b };
                    let p = Vec3::new(
                        pick(1, lo.x, hi.x),
                        pick(2, lo.y, hi.y),
                        pick(4, lo.z, hi.z),
                    );
                    raise(placed.transform_point3(p).y);
                }
            }
        }
        for child in &node.children {
            visit(
                child,
                world * transform_of(&child.transform).compute_affine(),
                top,
            );
        }
    }
    let mut top = None;
    visit(
        root,
        transform_of(&root.transform).compute_affine(),
        &mut top,
    );
    top
}

/// A mesh's position bounds; a point at the origin for one with none.
fn mesh_box(mesh: &Mesh) -> (Vec3, Vec3) {
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return (Vec3::ZERO, Vec3::ZERO);
    };
    positions.iter().fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(lo, hi), p| {
            let p = Vec3::from_array(*p);
            (lo.min(p), hi.max(p))
        },
    )
}

/// The key light a world is drawn under at `unix_secs`: the way toward it,
/// whether it is the moon, and the hour, for a world with a time of day.
fn key_light(env: &Environment, unix_secs: f64) -> (Vec3, bool, Option<f32>) {
    match &env.time_of_day {
        Some(time) => {
            let hour = crate::time_of_day::hour_at(time, unix_secs);
            let at = crate::time_of_day::environment_at(env, hour);
            (
                Vec3::from_array(at.env.sun_position.0).normalize_or(Vec3::Y),
                at.moon >= 0.5,
                Some(hour),
            )
        }
        None => (
            Vec3::from_array(env.sun_position.0).normalize_or(Vec3::Y),
            false,
            None,
        ),
    }
}

/// One world's first view (the module docs' columns).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct FirstView {
    pub(super) theme: &'static str,
    pub(super) biome: &'static str,
    /// `None` for no time of day; `Some(None)` for a day that turns.
    pub(super) hour: Option<Option<f32>>,
    pub(super) key_altitude_deg: f32,
    pub(super) key_off_deg: f32,
    pub(super) moon: bool,
    pub(super) weather: Option<String>,
    pub(super) slope_deg: f32,
    /// The landmark's distance, bearing off the view and seen share; `None`
    /// for a world with no landmark.
    pub(super) landmark: Option<(f32, f32, f32)>,
    pub(super) trees: usize,
    pub(super) rocks: usize,
    pub(super) built: usize,
    pub(super) in_way: usize,
}

/// `record`'s first view over `terrain`, with its day read at `unix_secs`.
/// `None` for a record with no landing.
pub(super) fn first_view(
    record: &RoomRecord,
    scene: &crate::seeded_defaults::scene::SceneCharacter,
    terrain: &FinishedHeightMap,
    unix_secs: f64,
) -> Option<FirstView> {
    let view = ArrivalView::of(record, terrain)?;
    let facing = view.facing();
    let water = crate::world_builder::compile::drawn_water(record, Some(terrain));
    let ground = crate::world_builder::AnchorGround::new(terrain, &water);
    let env = &record.environment;

    let (toward, moon, hour) = key_light(env, unix_secs);
    let turns = env
        .time_of_day
        .as_ref()
        .is_some_and(|t| t.day_minutes.0 > 0.0);
    let weather = env.weather.as_ref().map(|w| {
        format!(
            "{} {:.2}/{:.2}/{:.0}m",
            w.kind.label(),
            w.intensity.0,
            w.share.0,
            w.spell_minutes.0
        )
    });
    let normal = terrain.normal_at(view.focus.x, view.focus.z);

    // Where each seeded structure stands, as the compile walks it.
    let mut landmark = None;
    let mut structures = Vec::new();
    for placement in &record.placements {
        let Placement::Absolute {
            generator_ref,
            transform,
            snap_to_terrain: true,
            avoid_water: true,
            avoid_water_clearance,
            ..
        } = placement
        else {
            continue;
        };
        let stands = crate::world_builder::compile::pad::snapped_absolute_anchor(
            &ground,
            transform,
            true,
            avoid_water_clearance.0,
        );
        if generator_ref == "landmark" {
            let top = record
                .generators
                .get(generator_ref)
                .map(|g| match placement.shape_seed() {
                    Some(seed) => g.with_shape_seed(seed),
                    None => g.clone(),
                })
                .and_then(|g| drawn_top(&g))
                .unwrap_or(0.0);
            landmark = Some((stands, top, avoid_water_clearance.0));
        } else {
            structures.push(stands);
        }
    }

    // In the frame anywhere up its height: the frame's foot is 45 degrees
    // down, so the thing nearest the camera - the one filling the frame - is
    // in it only from its middle up.
    let near = |at: Vec3| {
        Vec2::new(at.x - view.focus.x, at.z - view.focus.z).length() <= REACH_M
            && STANDS_UP_M.iter().any(|&up| view.frames(at + Vec3::Y * up))
    };
    let sightline =
        landmark.map(|(stands, _, ground_m)| (Vec2::new(view.eye.x, view.eye.z), stands, ground_m));
    let in_way = |at: Vec3| {
        sightline.is_some_and(|(from, stands, ground_m)| {
            let to = Vec2::new(stands.x, stands.z);
            let (off, along) = off_segment(Vec2::new(at.x, at.z), from, to);
            off <= IN_WAY_M
                && along > 0.0
                && along < 1.0
                && to.distance(Vec2::new(at.x, at.z)) > ground_m
        })
    };
    let (mut trees, mut rocks, mut in_the_way) = (0, 0, 0);
    let points = crate::world_builder::compile::scatter_points(record, terrain);
    for (placement, placed) in record.placements.iter().zip(points) {
        let (Placement::Scatter { generator_ref, .. }, Some(placed)) = (placement, placed) else {
            continue;
        };
        let tree = generator_ref.starts_with("tree_scatter_");
        if !tree && generator_ref != "boulder" {
            continue;
        }
        for (x, z) in placed {
            let at = Vec3::new(x, terrain.world_height_at(x, z), z);
            if near(at) {
                if tree {
                    trees += 1;
                } else {
                    rocks += 1;
                }
            }
            in_the_way += usize::from(in_way(at));
        }
    }
    let built = structures.iter().filter(|at| near(**at)).count();
    in_the_way += structures.iter().filter(|at| in_way(**at)).count();

    let landmark = landmark.map(|(stands, top, ground_m)| {
        let distance = Vec2::new(stands.x - view.focus.x, stands.z - view.focus.z).length();
        let off = bearing_apart(facing, stands - view.eye).unwrap_or(0.0);
        let seen = (0..AXIS_SAMPLES)
            .filter(|&k| {
                let rise = 1.0 + (top - 1.0).max(0.0) * k as f32 / (AXIS_SAMPLES - 1) as f32;
                let point = stands + Vec3::Y * rise;
                view.frames(point) && in_sight(terrain, view.eye, point, ground_m)
            })
            .count();
        (distance, off, seen as f32 / AXIS_SAMPLES as f32)
    });

    Some(FirstView {
        theme: scene.theme.label(),
        biome: scene.biome.label(),
        hour: env
            .time_of_day
            .as_ref()
            .map(|_| if turns { None } else { hour }),
        key_altitude_deg: toward.y.clamp(-1.0, 1.0).asin().to_degrees(),
        key_off_deg: bearing_apart(facing, toward).unwrap_or(0.0),
        moon,
        weather,
        slope_deg: normal.y.clamp(-1.0, 1.0).acos().to_degrees(),
        landmark,
        trees,
        rocks,
        built,
        in_way: in_the_way,
    })
}

/// The seeds `--first-view` and `--album` read: `a..b`, or one seed.
fn seeds(what: &str) -> Result<Vec<u64>, String> {
    match what.split_once("..") {
        Some((a, b)) => {
            let a: u64 = a
                .trim()
                .parse()
                .map_err(|_| format!("{what:?}: {a:?} is not a seed"))?;
            let b: u64 = b
                .trim()
                .parse()
                .map_err(|_| format!("{what:?}: {b:?} is not a seed"))?;
            Ok((a..b).collect())
        }
        None => what
            .trim()
            .parse()
            .map(|seed| vec![seed])
            .map_err(|_| format!("{what:?} is not a seed or A..B")),
    }
}

/// The seeded world `--world N` draws for seed `seed`, its scene, and its
/// own ground - `None` where the world is a Berlin square, whose ground is
/// fetched.
fn procedural(
    seed: u64,
) -> Option<(
    RoomRecord,
    crate::seeded_defaults::scene::SceneCharacter,
    FinishedHeightMap,
)> {
    let record = RoomRecord::default_for_seed(seed, &format!("did:render:{seed}"));
    if record
        .geo_source
        .as_ref()
        .and_then(crate::pds::GeoSource::berlin_square)
        .is_some()
    {
        return None;
    }
    let terrain = FinishedHeightMap(crate::terrain::rebuild_heightmap_for_record(&record), None);
    Some((
        record,
        crate::seeded_defaults::scene::SceneCharacter::for_seed(seed),
        terrain,
    ))
}

fn now_unix_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// `--first-view`: the table and its summary (see the module docs).
pub(super) fn print_first_view(what: &str) {
    let seeds = seeds(what).unwrap_or_else(|e| panic!("--first-view {e}"));
    let now = now_unix_secs();
    println!(
        "{:>5}  {:<14} {:<10} {:>5} {:>5} {:>4} {:<4} {:<22} {:>5} {:>5} {:>6} {:>5} {:>5} {:>5} {:>5} {:>6}",
        "seed",
        "theme",
        "biome",
        "hour",
        "alt",
        "sun",
        "key",
        "weather",
        "slope",
        "lm_m",
        "lm_off",
        "seen",
        "trees",
        "rocks",
        "built",
        "in_way"
    );
    let mut rows = Vec::new();
    let mut berlin = 0usize;
    for seed in seeds {
        let Some((record, scene, terrain)) = procedural(seed) else {
            berlin += 1;
            continue;
        };
        let Some(row) = first_view(&record, &scene, &terrain, now) else {
            println!("{seed:>5}  no landing");
            continue;
        };
        let hour = match row.hour {
            None => "-".to_string(),
            Some(None) => "turns".to_string(),
            Some(Some(h)) => format!("{h:.1}"),
        };
        let (lm_m, lm_off, seen) = row.landmark.map_or_else(
            || ("-".to_string(), "-".to_string(), "-".to_string()),
            |(d, off, seen)| (format!("{d:.0}"), format!("{off:.0}"), format!("{seen:.2}")),
        );
        println!(
            "{:>5}  {:<14} {:<10} {:>5} {:>5.1} {:>4.0} {:<4} {:<22} {:>5.1} {:>5} {:>6} {:>5} {:>5} {:>5} {:>5} {:>6}",
            seed,
            truncate(row.theme, 14),
            truncate(row.biome, 10),
            hour,
            row.key_altitude_deg,
            row.key_off_deg,
            if row.moon { "moon" } else { "sun" },
            row.weather.as_deref().unwrap_or("-"),
            row.slope_deg,
            lm_m,
            lm_off,
            seen,
            row.trees,
            row.rocks,
            row.built,
            row.in_way
        );
        rows.push(row);
    }
    print_summary(&rows, berlin);
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// The `p`th of `values`, sorted in place; 0 for none.
fn percentile(values: &mut [f32], p: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f32::total_cmp);
    values[((values.len() - 1) as f32 * p).round() as usize]
}

fn print_summary(rows: &[FirstView], berlin: usize) {
    let n = rows.len();
    println!("\n{n} procedural worlds ({berlin} Berlin squares skipped)");
    if n == 0 {
        return;
    }
    let count = |f: &dyn Fn(&FirstView) -> bool| rows.iter().filter(|r| f(r)).count();
    let mut distances: Vec<f32> = rows
        .iter()
        .filter_map(|r| r.landmark.map(|l| l.0))
        .collect();
    let seen = |lo: f32, hi: f32| count(&|r| r.landmark.is_some_and(|l| l.2 >= lo && l.2 < hi));
    println!(
        "  landmark  distance median {:.0} m (p10 {:.0}, p90 {:.0}); seen whole (>= 0.75) {}, partly {}, barely (< 0.25) {}, none {}",
        percentile(&mut distances, 0.5),
        percentile(&mut distances, 0.1),
        percentile(&mut distances, 0.9),
        seen(0.75, 2.0),
        seen(0.25, 0.75),
        seen(f32::MIN_POSITIVE, 0.25),
        count(&|r| r.landmark.is_none_or(|l| l.2 == 0.0)),
    );
    let mut altitudes: Vec<f32> = rows.iter().map(|r| r.key_altitude_deg).collect();
    println!(
        "  light     moon {}; key altitude median {:.0} deg (p10 {:.0}, p90 {:.0}); off the view: into it (< 45) {}, across, ahead (45-90) {}, across, behind (90-135) {}, behind the camera (>= 135) {}",
        count(&|r| r.moon),
        percentile(&mut altitudes, 0.5),
        percentile(&mut altitudes, 0.1),
        percentile(&mut altitudes, 0.9),
        count(&|r| r.key_off_deg < 45.0),
        count(&|r| (45.0..90.0).contains(&r.key_off_deg)),
        count(&|r| (90.0..135.0).contains(&r.key_off_deg)),
        count(&|r| r.key_off_deg >= 135.0),
    );
    let mut hours: Vec<f32> = rows.iter().filter_map(|r| r.hour.flatten()).collect();
    let mut kinds: Vec<(String, usize)> = Vec::new();
    for row in rows {
        let kind = row
            .weather
            .as_deref()
            .and_then(|w| w.split(' ').next())
            .unwrap_or("none")
            .to_string();
        match kinds.iter_mut().find(|(k, _)| *k == kind) {
            Some((_, n)) => *n += 1,
            None => kinds.push((kind, 1)),
        }
    }
    kinds.sort();
    println!(
        "  sky       no time of day {}, a standing hour {} (median {:.1}), a turning day {}; weather {}",
        count(&|r| r.hour.is_none()),
        hours.len(),
        percentile(&mut hours, 0.5),
        count(&|r| matches!(r.hour, Some(None))),
        kinds
            .iter()
            .map(|(k, n)| format!("{k} {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut slopes: Vec<f32> = rows.iter().map(|r| r.slope_deg).collect();
    println!(
        "  first {REACH_M:.0} m  a tree in view {}, a rock {}, a structure {}; something in the landmark's way {}; landing slope median {:.1} deg (p90 {:.1})",
        count(&|r| r.trees > 0),
        count(&|r| r.rocks > 0),
        count(&|r| r.built > 0),
        count(&|r| r.in_way > 0),
        percentile(&mut slopes, 0.5),
        percentile(&mut slopes, 0.9),
    );
}

/// What `--album` passes through to every frame it draws.
pub(super) struct AlbumRequest<'a> {
    pub(super) seeds: &'a str,
    pub(super) out: PathBuf,
    pub(super) jobs: usize,
    /// Flags handed to each frame as they were given: `--hour`, `--weather`,
    /// `--effects`, `--grass` and their values.
    pub(super) pass: Vec<String>,
}

/// The flags of `args` (this binary's own, its name left off) that `--album`
/// hands every frame as they were given: `--hour`, `--weather`, `--effects`
/// and `--grass`, each with its value, written either way clap reads them.
pub(super) fn passthrough(args: impl IntoIterator<Item = String>) -> Vec<String> {
    const PASS: [&str; 4] = ["--hour", "--weather", "--effects", "--grass"];
    let mut args = args.into_iter();
    let mut out = Vec::new();
    while let Some(arg) = args.next() {
        let named = |flag: &&str| {
            arg.strip_prefix(*flag)
                .is_some_and(|rest| rest.starts_with('='))
        };
        if PASS.iter().any(named) {
            out.push(arg);
        } else if PASS.contains(&arg.as_str()) {
            out.push(arg);
            out.extend(args.next());
        }
    }
    out
}

/// A tile's size in the sheet: half a frame's.
const TILE: (u32, u32) = (448, 252);
/// The frame each tile is drawn from.
const FRAME: (u32, u32) = (896, 504);

/// `--album`: draw each procedural seed's arrival frame and lay the frames
/// out as one contact sheet (see the module docs). Exits non-zero when a
/// frame fails.
pub(super) fn album(request: &AlbumRequest<'_>) {
    let seeds: Vec<u64> = seeds(request.seeds)
        .unwrap_or_else(|e| panic!("--album {e}"))
        .into_iter()
        .filter(|&seed| procedural(seed).is_some())
        .take(MAX_ALBUM)
        .collect();
    if seeds.is_empty() {
        eprintln!("--album {}: no procedural seed in it", request.seeds);
        std::process::exit(1);
    }
    let frames = request.out.with_extension("frames");
    std::fs::create_dir_all(&frames)
        .unwrap_or_else(|e| panic!("cannot make {}: {e}", frames.display()));
    let exe = std::env::current_exe().expect("this binary's own path");
    let next = std::sync::atomic::AtomicUsize::new(0);
    let failed = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..request.jobs.clamp(1, 4) {
            scope.spawn(|| {
                while let Some(&seed) =
                    seeds.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
                {
                    if let Err(e) =
                        draw_frame(&exe, seed, &frame_path(&frames, seed), &request.pass)
                    {
                        failed.lock().expect("one writer at a time").push((seed, e));
                    }
                }
            });
        }
    });
    let failed = failed.into_inner().expect("the frames are drawn");
    for (seed, e) in &failed {
        eprintln!("--album: seed {seed}: {e}");
    }
    let columns = (seeds.len() as f32).sqrt().ceil().clamp(1.0, 8.0) as u32;
    let rows = (seeds.len() as u32).div_ceil(columns);
    let mut sheet = image::RgbImage::new(columns * TILE.0, rows * TILE.1);
    for (i, &seed) in seeds.iter().enumerate() {
        let (left, top) = ((i as u32 % columns) * TILE.0, (i as u32 / columns) * TILE.1);
        if let Ok(frame) = image::open(frame_path(&frames, seed)) {
            let tile = image::imageops::resize(
                &frame.to_rgb8(),
                TILE.0,
                TILE.1,
                image::imageops::FilterType::Triangle,
            );
            image::imageops::replace(&mut sheet, &tile, i64::from(left), i64::from(top));
        }
        super::terrain_report::draw_number(
            i64::from(left) + 6,
            i64::from(top) + 6,
            seed as usize,
            [255, 255, 255],
            &mut |x, y, rgb| {
                if x >= 0 && y >= 0 && (x as u32) < sheet.width() && (y as u32) < sheet.height() {
                    sheet.put_pixel(x as u32, y as u32, image::Rgb(rgb));
                }
            },
        );
    }
    sheet
        .save(&request.out)
        .unwrap_or_else(|e| panic!("cannot write {}: {e}", request.out.display()));
    println!(
        "album: {} ({} seeds, {columns} columns, seeds left to right and top to bottom; frames in {})",
        request.out.display(),
        seeds.len(),
        frames.display()
    );
    println!(
        "seeds: {}",
        seeds
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    );
    if !failed.is_empty() {
        std::process::exit(1);
    }
}

fn frame_path(frames: &Path, seed: u64) -> PathBuf {
    frames.join(format!("seed-{seed}.png"))
}

/// One arrival frame, drawn by this binary in a process of its own. A frame
/// whose log says a shader or an asset failed to load is refused: the tool
/// still writes a plausible picture without them (#1320).
fn draw_frame(exe: &Path, seed: u64, out: &Path, pass: &[String]) -> Result<(), String> {
    let output = std::process::Command::new(exe)
        .args(["--world", &seed.to_string(), "--focus", "arrival"])
        .args([
            "--width",
            &FRAME.0.to_string(),
            "--height",
            &FRAME.1.to_string(),
        ])
        .arg("--out")
        .arg(out)
        .args(pass)
        .output()
        .map_err(|e| format!("cannot run {}: {e}", exe.display()))?;
    let log = String::from_utf8_lossy(&output.stderr);
    if let Some(line) = log
        .lines()
        .find(|l| l.contains("Path not found") || l.contains("failed to process shader"))
    {
        return Err(format!("the frame drew without its assets: {line}"));
    }
    if !output.status.success() {
        return Err(format!(
            "exit {}: {}",
            output.status,
            log.lines().last().unwrap_or("")
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A heightmap of `cells` a side, `scale` metres a cell, its height
    /// `height(x, z)` in world metres about the map's centre.
    fn terrain(cells: usize, scale: f32, height: impl Fn(f32, f32) -> f32) -> FinishedHeightMap {
        let mut map = bevy_symbios_ground::HeightMap::new(cells, cells, scale);
        let half = (cells - 1) as f32 * scale * 0.5;
        for z in 0..cells {
            for x in 0..cells {
                map.data_mut()[z * cells + x] =
                    height(x as f32 * scale - half, z as f32 * scale - half);
            }
        }
        FinishedHeightMap(map, None)
    }

    /// The arrival camera stands behind the body, the rest radius back and
    /// the rest pitch up, and looks the way the landing faces: a facing of
    /// 90 looks along -X (the spawn path's turn), so the camera stands on
    /// +X of the body.
    #[test]
    fn the_arrival_camera_stands_behind_the_way_the_landing_faces() {
        let flat = terrain(65, 2.0, |_, _| 0.0);
        let focus = Vec3::new(0.0, 1.0, 0.0);
        for yaw in [0.0f32, 90.0, 180.0, 233.0] {
            let view = ArrivalView::around(focus, yaw, &flat);
            let back = view.eye - view.focus;
            let pitch = crate::config::camera::ORBIT_PITCH;
            let radius = crate::config::camera::ORBIT_RADIUS;
            assert!((back.length() - radius).abs() < 1e-3, "yaw {yaw}: {back}");
            assert!(
                (back.y - radius * pitch.sin()).abs() < 1e-3,
                "yaw {yaw}: {back}"
            );
            let looks = Vec2::new(-back.x, -back.z).normalize();
            let facing = view.facing();
            assert!(
                looks.distance(Vec2::new(facing.x, facing.z)) < 1e-4,
                "yaw {yaw}: looks {looks}, faces {facing}"
            );
        }
        let east = ArrivalView::around(focus, 90.0, &flat);
        assert!(east.eye.x > 11.0 && east.eye.z.abs() < 1e-3, "{}", east.eye);
    }

    /// Rising ground behind the body pulls the camera in along its ray, as
    /// the player's camera is pulled in: on a slope climbing toward +Z the
    /// camera of a body facing -Z stands nearer than the rest radius.
    #[test]
    fn rising_ground_behind_the_body_pulls_the_camera_in() {
        let ramp = terrain(65, 2.0, |_, z| (z * 0.8).max(0.0));
        let focus = Vec3::new(0.0, 1.0, 0.0);
        let view = ArrivalView::around(focus, 0.0, &ramp);
        let reach = view.eye.distance(view.focus);
        assert!(
            reach < crate::config::camera::ORBIT_RADIUS - 1.0,
            "the camera stayed {reach} m out, in the hill"
        );
        assert!(view.eye.y >= ramp.world_height_at(view.eye.x, view.eye.z));
    }

    /// The frame at the rest pitch: the ground ahead is in it, a point
    /// straight above the body's head is not, nor is anything behind the
    /// camera or off to the side past the lens's width - and the horizon is
    /// a fraction of a degree under its top edge, so a point at the
    /// camera's own height far ahead is out of it while one a little under
    /// is in.
    #[test]
    fn the_frame_holds_the_ground_ahead_and_not_what_stands_above_the_eye() {
        let flat = terrain(65, 2.0, |_, _| 0.0);
        let view = ArrivalView::around(Vec3::new(0.0, 1.0, 0.0), 0.0, &flat);
        assert!(view.frames(Vec3::new(0.0, 0.0, -10.0)), "the ground ahead");
        assert!(!view.frames(Vec3::new(0.0, 20.0, 0.0)), "overhead");
        assert!(!view.frames(Vec3::new(0.0, 0.0, 30.0)), "behind the camera");
        assert!(!view.frames(Vec3::new(60.0, 0.0, -10.0)), "far to the side");
        let far = 400.0;
        assert!(
            !view.frames(Vec3::new(0.0, view.eye.y, -far)),
            "eye height far ahead"
        );
        assert!(view.frames(Vec3::new(0.0, view.eye.y - 0.05 * far, -far)));
    }

    /// The sightline: over flat ground a line between two raised points is
    /// in sight, a ridge between them hides it, and the ground the target
    /// stands on (`short`) does not count against it.
    #[test]
    fn a_ridge_between_hides_the_far_point_and_the_targets_own_ground_does_not() {
        let flat = terrain(65, 2.0, |_, _| 0.0);
        let (from, to) = (Vec3::new(0.0, 5.0, 30.0), Vec3::new(0.0, 5.0, -30.0));
        assert!(in_sight(&flat, from, to, 0.0));
        let ridge = terrain(65, 2.0, |_, z| if z.abs() < 4.0 { 9.0 } else { 0.0 });
        assert!(!in_sight(&ridge, from, to, 0.0));
        let mound = terrain(
            65,
            2.0,
            |_, z| if (z + 30.0).abs() < 6.0 { 9.0 } else { 0.0 },
        );
        assert!(!in_sight(&mound, from, to, 0.0));
        assert!(in_sight(&mound, from, to, 8.0));
    }

    /// A shape grammar's top is what its terminals draw, not its node's
    /// footprint: a 6 m extrusion on a grammar node lifted 2 m reads 8 m, and
    /// a primitive child above it raises the top again.
    #[test]
    fn the_drawn_top_reads_a_grammar_s_terminals_and_the_primitives() {
        let grammar: Generator = serde_json::from_value(serde_json::json!({
            "$type": "network.symbios.gen.shape",
            "grammar_source": "Lot --> Extrude(6) I(\"Block\")",
            "root_rule": "Lot",
            "footprint": [40000, 0, 40000],
            "seed": "7",
            "transform": { "translation": [0, 20000, 0] }
        }))
        .expect("a grammar node");
        let top = drawn_top(&grammar).expect("it draws");
        assert!((top - 8.0).abs() < 1e-3, "top {top}");

        let mut tower = grammar.clone();
        let spire: Generator = serde_json::from_value(serde_json::json!({
            "$type": "network.symbios.gen.cuboid",
            "size": [10000, 40000, 10000],
            "solid": true,
            "transform": { "translation": [0, 100000, 0] }
        }))
        .expect("a cuboid");
        tower.children.push(spire);
        let top = drawn_top(&tower).expect("it draws");
        assert!((top - 14.0).abs() < 1e-3, "top {top}");
    }

    /// An album hands each frame the look flags it was given, either way
    /// clap reads them, and nothing else of its own line.
    #[test]
    fn an_album_passes_its_look_flags_to_every_frame() {
        let line = [
            "--album",
            "0..8",
            "--hour",
            "17.5",
            "--weather=rain:0.4",
            "--album-jobs",
            "3",
            "--effects",
            "reduced",
            "--grass=low",
            "--out",
            "x.png",
        ]
        .map(String::from);
        assert_eq!(
            passthrough(line),
            [
                "--hour",
                "17.5",
                "--weather=rain:0.4",
                "--effects",
                "reduced",
                "--grass=low"
            ]
        );
    }

    /// The seeds a survey reads: a range, or one.
    #[test]
    fn a_survey_reads_a_range_or_one_seed() {
        assert_eq!(seeds("3..6"), Ok(vec![3, 4, 5]));
        assert_eq!(seeds(" 7 "), Ok(vec![7]));
        assert!(seeds("x..4").is_err());
        assert!(seeds("did:plc:abc").is_err());
    }
}
