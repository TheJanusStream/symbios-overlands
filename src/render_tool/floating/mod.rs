//! `--floating-report` (#1477): every part of a world that floats, named.
//!
//! Two kinds of float were found by eye in the Understory, and nothing
//! measured either. A dead tree, placed forty times, tapered its trunk to
//! half its width at the top and kept its limbs where the untapered trunk's
//! surface would have been, so its top limbs hung in the air beside it: a
//! child does not follow its parent's torture, which reshapes the parent's
//! own mesh and nothing else. And the Puffball Meadow, one placement snapped
//! at its centre, had its small puffballs authored at one height in its own
//! frame, so where the real ground fell away they floated up to 2.14 m.
//! Asking of each part whether its bottom is on the ground named 280 of 441
//! parts, because most parts rest on other parts, as a cap rests on its stem:
//! the rule has to know what touches what.
//!
//! # The rules
//!
//! Each placed generator's primitives are meshed by the real mesher in the
//! generator's own frame ([`body`]), and two parts touch when their surfaces
//! come within [`body::CONTACT_M`] of each other or one lies wholly inside
//! the other's closed solid. The ground touches a part too: a part reaching
//! down to the generator's ground plane (its y = 0, where a snapped
//! placement stands it on the terrain), or down to the real ground where the
//! game stands one of its absolute placements - snapped, at the terrain
//! report's `stands_y`, by the same heightmap and footprint rule, or
//! unsnapped, where it says. Water holds a part only where it holds it at
//! every absolute placement, meeting its surface as a raft does: the ghost
//! snag stands ten of its sixteen trees in a pond, and a bracket two metres
//! up its trunk, under water there, is held by nothing at the other six.
//!
//! - **Class a, free of its generator**: a part in a group of touching parts
//!   that touches nothing the generator stands by - the ground, where any of
//!   its parts reaches the ground; else its first part's group. `gap_m` is
//!   how far the group is from the body it left, or from the ground under
//!   it.
//! - **Class b, over falling ground**: a part resting on the generator's
//!   ground - its ground plane, or the real ground at one of its placements -
//!   whose underside, where the game stands an absolute placement of it, is
//!   somewhere more than [`FLOAT_M`] above what is under it there: the real
//!   ground, the water's surface, or another part. Each point of the
//!   underside is asked on its own, so a long spine one end of which dips
//!   into rising ground still floats along the rest; `gap_m` is the largest
//!   such clearance. A part buried deep is not a float.
//!
//! A scatter's or a grid's copies each stand at their own point, so class b
//! is not asked of them: a part can float there only by its reach from the
//! generator's centre times the slope, and each scattered generator's reach
//! is listed so a wide one shows.

mod body;
mod geometry;

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use bevy::math::Affine3A;
use bevy::prelude::*;
use bevy_symbios_ground::HeightMap;
use serde_json::{Value, json};

use self::body::{Body, CONTACT_M, Part, distance};
use super::terrain_report::Ground;
use crate::pds::{GeneratorKind, Placement, RoomRecord};
use crate::world_builder::compile::pad;
use crate::world_builder::compile::water::room_water_level;

/// A part resting on its generator's ground that clears the real ground
/// by more than this floats (m).
pub(super) const FLOAT_M: f32 = 0.15;

/// What each class means, in the report's own words.
const CLASSES: [(&str, &str); 2] = [
    (
        "a",
        "free of its generator: touches no part of the body the generator stands by - \
         its ground, or its first part where nothing reaches the ground; gap_m is how far \
         it is from that body or from the ground under it",
    ),
    (
        "b",
        "over falling ground: rests on its generator's ground (its ground plane, or the \
         real ground at one of its placements), but where the game stands this placement \
         the real ground (or water, or a part) under some point of its underside is gap_m \
         lower",
    ),
];

/// What the report does not look at, in its own words.
const NOT_CHECKED: &str = "L-systems, shapes and signs, which are not meshed here \
     (a part resting on one would read as floating: `unmeshed` names each generator \
     that holds one); particle emitters, portals, gateways and water, which are not \
     parts; anything under a terrain or particle-system root, whose generator is not \
     read at all; class b for scatters and grids, whose copies each snap at their own \
     point (`scatters` gives each one's reach from its centre, which times the slope \
     is the most such a copy can float)";

/// Print the report for `record`, the world `world` names.
pub(super) fn print_floating_report(world: &str, record: &RoomRecord) {
    // The heightmap job and the meshing are the two slow halves, and
    // neither reads the other.
    let (map, bodies) = std::thread::scope(|scope| {
        let map = scope.spawn(|| crate::terrain::rebuild_heightmap_for_record(record));
        let bodies = bodies_of(record);
        (map.join().expect("the heightmap rebuild panicked"), bodies)
    });
    println!("{}", one_row_a_line(&report(world, record, &map, &bodies)));
}

/// Every placed generator's body, by name, meshed on every core.
fn bodies_of(record: &RoomRecord) -> BTreeMap<String, Body> {
    let mut names: Vec<&String> = record
        .placements
        .iter()
        .filter_map(generator_ref)
        .filter_map(|name| record.generators.get_key_value(name).map(|(k, _)| k))
        .filter(|name| {
            !matches!(
                record.generators[*name].kind,
                GeneratorKind::Terrain(_) | GeneratorKind::ParticleSystem(_)
            )
        })
        .collect();
    names.sort();
    names.dedup();
    let next = AtomicUsize::new(0);
    let done = Mutex::new(BTreeMap::new());
    let workers = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .clamp(1, names.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                while let Some(name) = names.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let body = Body::of(&record.generators[*name]);
                    done.lock()
                        .expect("a meshing worker panicked")
                        .insert((*name).clone(), body);
                }
            });
        }
    });
    done.into_inner().expect("a meshing worker panicked")
}

/// The generator a placement plants, if it plants one.
fn generator_ref(placement: &Placement) -> Option<&String> {
    match placement {
        Placement::Absolute { generator_ref, .. }
        | Placement::Scatter { generator_ref, .. }
        | Placement::Grid { generator_ref, .. } => Some(generator_ref),
        Placement::Unknown => None,
    }
}

/// Where a generator is placed.
#[derive(Default)]
struct Uses {
    /// Each absolute placement: its index, and where the game stands the
    /// generator's frame for it.
    stands: Vec<(usize, Affine3A)>,
    /// Its scatters and grids, by index.
    elsewhere: Vec<usize>,
    /// Whether any placement stands its frame on the ground, which makes
    /// its y = 0 the ground's.
    snapped: bool,
    /// Whether any of those is a scatter or a grid.
    scattered: bool,
}

fn uses_of<'a>(
    record: &'a RoomRecord,
    map: &HeightMap,
    water: Option<f32>,
) -> HashMap<&'a str, Uses> {
    let mut uses: HashMap<&str, Uses> = HashMap::new();
    for (index, placement) in record.placements.iter().enumerate() {
        match placement {
            Placement::Absolute {
                generator_ref,
                transform,
                snap_to_terrain,
                avoid_water,
                avoid_water_clearance,
            } => {
                let used = uses.entry(generator_ref).or_default();
                // As the executor stands it: the placement's rotation and no
                // scale, snapped at the anchor the terrain report prints, or
                // unsnapped where it says - where the real ground under it
                // is just as real.
                let at = if *snap_to_terrain {
                    pad::snapped_absolute_anchor(
                        map,
                        transform,
                        *avoid_water,
                        avoid_water_clearance.0,
                        water,
                    )
                } else {
                    Vec3::from_array(transform.translation.0)
                };
                let pose = Transform::from_translation(at)
                    .with_rotation(Quat::from_array(transform.rotation.0));
                used.stands.push((index, pose.compute_affine()));
                used.snapped |= *snap_to_terrain;
            }
            Placement::Scatter {
                generator_ref,
                snap_to_terrain,
                ..
            }
            | Placement::Grid {
                generator_ref,
                snap_to_terrain,
                ..
            } => {
                let used = uses.entry(generator_ref).or_default();
                used.elsewhere.push(index);
                used.snapped |= *snap_to_terrain;
                used.scattered = true;
            }
            Placement::Unknown => {}
        }
    }
    uses
}

/// The report's fields in print order.
fn report(
    world: &str,
    record: &RoomRecord,
    map: &HeightMap,
    bodies: &BTreeMap<String, Body>,
) -> Vec<(&'static str, Value)> {
    let ground = Ground::new(map);
    let height = |x: f32, z: f32| ground.height(x, z);
    let water = room_water_level(record);
    let uses = uses_of(record, map, water);
    let mut rows: Vec<Row> = Vec::new();
    let mut scatters = Vec::new();
    let mut unmeshed = Vec::new();
    let (mut meshed, mut parts, mut stands) = (0, 0, 0);
    for (name, body) in bodies {
        let Some(used) = uses.get(name.as_str()) else {
            continue;
        };
        meshed += usize::from(!body.parts.is_empty());
        parts += body.parts.len();
        stands += used.stands.len();
        if body.unmeshed > 0 {
            unmeshed.push(json!(name));
        }
        let check = Check {
            name,
            body,
            used,
            ground: &height,
            water,
        };
        rows.extend(check.rows());
        if used.scattered && !body.parts.is_empty() {
            scatters.push(json!({
                "generator": name,
                "placements": used.elsewhere,
                "ground_reach_m": round2(ground_reach(body)),
            }));
        }
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(b.1)).then(b.2.total_cmp(&a.2)));
    vec![
        ("world", json!(world)),
        (
            "checked",
            json!({
                "generators": meshed,
                "parts": parts,
                "absolute_placements": stands,
            }),
        ),
        (
            "rules",
            json!({ "contact_m": round2(CONTACT_M), "float_m": round2(FLOAT_M) }),
        ),
        (
            "classes",
            Value::Object(
                CLASSES
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), json!(v)))
                    .collect(),
            ),
        ),
        ("not_checked", json!(NOT_CHECKED)),
        ("unmeshed", Value::Array(unmeshed)),
        ("scatters", Value::Array(scatters)),
        (
            "floating",
            Value::Array(rows.into_iter().map(|r| r.3).collect()),
        ),
    ]
}

/// How far out from its origin, across the level, a generator meets its
/// ground plane (m).
fn ground_reach(body: &Body) -> f32 {
    body.parts
        .iter()
        .flat_map(|p| p.points.iter())
        .filter(|p| p.y <= CONTACT_M)
        .map(|p| p.x.hypot(p.z))
        .fold(0.0, f32::max)
}

/// A floating part's row, and what the rows sort by: its generator, its
/// class, and its gap (the largest first).
type Row = (String, &'static str, f32, Value);

/// One generator, checked where it is placed.
struct Check<'a, G: Fn(f32, f32) -> f32> {
    name: &'a str,
    body: &'a Body,
    used: &'a Uses,
    /// The real ground's height at a world `(x, z)`.
    ground: &'a G,
    /// The room's water level, if it has water.
    water: Option<f32>,
}

impl<G: Fn(f32, f32) -> f32> Check<'_, G> {
    /// Its floating parts as (generator, class, gap, row).
    fn rows(&self) -> Vec<Row> {
        let parts = &self.body.parts;
        if parts.is_empty() {
            return Vec::new();
        }
        let grounded: Vec<bool> = parts.iter().map(|part| self.grounded(part)).collect();
        let component = self.body.components();
        let by_ground = grounded.iter().any(|g| *g);
        let anchored: Vec<bool> = {
            let mut held = vec![false; parts.len()];
            for (i, &c) in component.iter().enumerate() {
                if (by_ground && grounded[i]) || (!by_ground && i == 0) {
                    held[c] = true;
                }
            }
            component.iter().map(|&c| held[c]).collect()
        };
        let mut rows = self.free(&component, &anchored, by_ground);
        for (index, stand) in &self.used.stands {
            for (i, part) in parts.iter().enumerate() {
                if grounded[i] && anchored[i] {
                    rows.extend(self.over_falling_ground(i, part, *index, stand));
                }
            }
        }
        rows
    }

    /// Class a: every part of a group that touches nothing anchored.
    fn free(&self, component: &[usize], anchored: &[bool], by_ground: bool) -> Vec<Row> {
        let parts = &self.body.parts;
        let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (i, &c) in component.iter().enumerate() {
            if !anchored[i] {
                groups.entry(c).or_default().push(i);
            }
        }
        let placements: Vec<usize> = self
            .used
            .stands
            .iter()
            .map(|(index, _)| *index)
            .chain(self.used.elsewhere.iter().copied())
            .collect();
        let mut rows = Vec::new();
        for members in groups.values() {
            // How far the group is from the ground holding the body: its
            // ground plane, or the real ground where it is stood.
            let mut gap = f32::INFINITY;
            if by_ground {
                for &i in members {
                    if self.used.snapped {
                        gap = gap.min(parts[i].min.y.max(0.0));
                    }
                    for (_, stand) in &self.used.stands {
                        gap = gap.min(self.clearance(&parts[i], stand).max(0.0));
                    }
                }
            }
            for &i in members {
                for (j, other) in parts.iter().enumerate() {
                    if anchored[j] {
                        gap = distance(&parts[i], other, gap);
                    }
                }
            }
            for &i in members {
                let part = &parts[i];
                let centre = (part.min + part.max) * 0.5;
                let mut row = json!({
                    "generator": self.name,
                    "part": pointer(self.name, &part.path),
                    "kind": part.kind,
                    "class": "a",
                    "gap_m": round2(gap),
                    "local": round3v(centre),
                    "placements": placements,
                });
                if let Some((_, stand)) = self.used.stands.first() {
                    row["at"] = round3v(stand.transform_point3(centre));
                }
                if members.len() > 1 {
                    // Free together: the group's first part, and its size.
                    row["group"] = json!(pointer(self.name, &parts[members[0]].path));
                    row["group_parts"] = json!(members.len());
                }
                rows.push((self.name.to_owned(), "a", gap, row));
            }
        }
        rows
    }

    /// Whether the ground holds `part`: it reaches the generator's ground
    /// plane where a placement snaps it, or the real ground where the game
    /// stands one of its absolute placements - or the water holds it.
    fn grounded(&self, part: &Part) -> bool {
        (self.used.snapped && part.min.y <= CONTACT_M)
            || self
                .used
                .stands
                .iter()
                .any(|(_, stand)| self.clearance(part, stand) <= CONTACT_M)
            || self.afloat(part)
    }

    /// Whether the water holds `part`: it meets the water's surface at
    /// every absolute placement, as a raft laid on a pond does. A part
    /// under water at one placement and in the air at another is held at
    /// neither - the water's surface is not the ground, and what reaches
    /// down into it is not resting on it.
    fn afloat(&self, part: &Part) -> bool {
        let Some(level) = self.water else {
            return false;
        };
        !self.used.stands.is_empty()
            && self.used.stands.iter().all(|(_, stand)| {
                let (low, high) = part.points.iter().fold(
                    (f32::INFINITY, f32::NEG_INFINITY),
                    |(low, high), p| {
                        let y = stand.transform_point3(*p).y;
                        (low.min(y), high.max(y))
                    },
                );
                low <= level + CONTACT_M && high >= level - CONTACT_M
            })
    }

    /// What a part can rest on at a world `(x, z)`, but for other parts:
    /// the ground, or the water over it.
    fn support(&self, x: f32, z: f32) -> f32 {
        let height = (self.ground)(x, z);
        self.water.map_or(height, |level| height.max(level))
    }

    /// Class b: `part`, which rests on the generator's ground, where
    /// placement `index` stands it - if some point of its underside clears
    /// what is under it by more than [`FLOAT_M`].
    fn over_falling_ground(
        &self,
        i: usize,
        part: &Part,
        index: usize,
        stand: &Affine3A,
    ) -> Option<Row> {
        // Its underside where the game stands it: the samples within a
        // contact's width of its lowest, which is what it rests on. Each is
        // asked on its own - a spine laid at one height over falling ground
        // floats along its length though one end dips into the rise.
        let placed: Vec<(Vec3, Vec3)> = part
            .samples()
            .map(|p| (p, stand.transform_point3(p)))
            .collect();
        let low = placed.iter().fold(f32::INFINITY, |m, (_, w)| m.min(w.y));
        let underside: Vec<(Vec3, Vec3)> = placed
            .into_iter()
            .filter(|(_, w)| w.y <= low + CONTACT_M)
            .collect();
        // Parts under it can only raise what it rests on, so an underside
        // nowhere that far over the ground or water needs no rays.
        if underside
            .iter()
            .all(|(_, w)| w.y - self.support(w.x, w.z) <= FLOAT_M)
        {
            return None;
        }
        // The parts under it hold it up as the ground would: a ray straight
        // down from each sample, in the generator's frame - cast from a
        // contact's width above it, so a sample lying in the face it rests
        // on meets that face and not the one beneath.
        let down = stand.inverse().transform_vector3(Vec3::NEG_Y);
        let level = (down - Vec3::NEG_Y).length() < 1.0e-4;
        let mut worst = (f32::NEG_INFINITY, Vec3::ZERO, 0.0);
        for (p, w) in underside {
            let mut under = self.support(w.x, w.z);
            for (j, other) in self.body.parts.iter().enumerate() {
                if j == i
                    || (level
                        && (p.x < other.min.x
                            || p.x > other.max.x
                            || p.z < other.min.z
                            || p.z > other.max.z
                            || other.min.y > p.y + CONTACT_M))
                {
                    continue;
                }
                if other.contains(p) {
                    under = w.y;
                } else if let Some(drop) = other.below(p - down * CONTACT_M, down) {
                    under = under.max(w.y - (drop - CONTACT_M));
                }
            }
            if w.y - under > worst.0 {
                worst = (w.y - under, w, under);
            }
        }
        let (gap, at, under) = worst;
        (gap > FLOAT_M).then(|| {
            let row = json!({
                "generator": self.name,
                "part": pointer(self.name, &part.path),
                "kind": part.kind,
                "class": "b",
                "gap_m": round2(gap),
                "placement": index,
                "at": round3v(at),
                "under_m": round2(under),
            });
            (self.name.to_owned(), "b", gap, row)
        })
    }

    /// The least height of `part` over the real ground under it where
    /// `stand` stands it - the ground alone, not the water over it.
    fn clearance(&self, part: &Part, stand: &Affine3A) -> f32 {
        part.samples()
            .map(|p| {
                let w = stand.transform_point3(p);
                w.y - (self.ground)(w.x, w.z)
            })
            .fold(f32::INFINITY, f32::min)
    }
}

/// A part's JSON pointer into the record: its generator's name escaped as
/// a pointer escapes it, then each child index.
fn pointer(name: &str, path: &[usize]) -> String {
    let base = format!("/generators/{}", name.replace('~', "~0").replace('/', "~1"));
    path.iter().fold(base, |p, i| format!("{p}/children/{i}"))
}

fn round2(v: f32) -> f64 {
    (f64::from(v) * 100.0).round() / 100.0
}

fn round3v(v: Vec3) -> Value {
    json!(
        v.to_array()
            .map(|c| (f64::from(c) * 1000.0).round() / 1000.0)
    )
}

/// `fields` as one JSON object with each field on a line of its own and
/// each row of a list on a line of its own (the triangle report's layout),
/// so `grep` finds a generator's rows whole.
fn one_row_a_line(fields: &[(&str, Value)]) -> String {
    let mut out = String::from("{\n");
    for (i, (key, value)) in fields.iter().enumerate() {
        let comma = if i + 1 < fields.len() { "," } else { "" };
        let key = Value::from(*key);
        match value {
            Value::Array(rows) if !rows.is_empty() => {
                out += &format!("  {key}: [\n");
                for (j, row) in rows.iter().enumerate() {
                    let sep = if j + 1 < rows.len() { "," } else { "" };
                    out += &format!("    {row}{sep}\n");
                }
                out += &format!("  ]{comma}\n");
            }
            _ => out += &format!("  {key}: {value}{comma}\n"),
        }
    }
    out.push('}');
    out
}

#[cfg(test)]
mod tests;
