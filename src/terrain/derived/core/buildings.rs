//! Berlin's buildings on the walkable ground (#1588): each ALKIS building as
//! the theme's catalogue buildings on its footprint.
//!
//! A building's use and storeys pick its role ([`role_of`]): a church, a
//! large or tall cultural building, a high-rise takes one of the theme's
//! landmarks - at most [`MAX_CORE_LANDMARKS`], no two within
//! [`LANDMARK_SPACING_M`], the most telling first - and every other building
//! takes the theme's secondary buildings, bigger where Berlin's stands
//! taller. A footprint under [`MIN_AREA_M2`] - a kiosk, a public toilet, a
//! transformer box - is left out.
//!
//! A footprint is filled along its long side ([`fill`]). Its oriented box -
//! the box along its longest edge - takes rows of entries down its length,
//! one per [`ROW_DEPTH_M`] of its depth, each entry fitted to its row's
//! depth and set side by side, each slot whose middle lies inside the
//! footprint taking a copy. A row turns to the footprint's street side;
//! of several, the outer two turn out, each to its own side, as a block's
//! houses front the streets either side of it. A landmark stands at the
//! box's middle, fitted to its narrower side, and the rows fill the box's
//! length either side of it; one that fits nowhere, whose box's middle is
//! off its footprint - a courtyard - or that would reach what the record
//! keeps gives way to the rows alone, whose slots each keep clear of it.
//!
//! Every copy stands on its building's voxel shell as a collider, and the
//! nearest the landing are drawn near: past the plan's entity budget a copy
//! is drawn as that shell.

use std::collections::HashMap;

use bevy::prelude::*;
use geodata::berlin::BuildingUse;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::seeded_defaults::fnv1a_64;
use crate::terrain::geo::street_level::{CoreBuilding, centroid, contains, yaw_towards};
use crate::terrain::lots::{FOUNDATION_SINK_M, scale_e4};

use super::super::fit::{
    cache_key, footing, pick_landmark, pick_ranked, radius, room_theme, sized_pool,
};
use super::super::plan::{Grow, Plan, PlannedBuilding, PlannedCopy, Policy, Solid};
use super::super::{SourceId, SourceLayer};
use super::Kept;

/// The most landmarks the walkable ground draws.
pub(crate) const MAX_CORE_LANDMARKS: usize = 6;

/// How far apart two landmarks stand at least (m).
pub(crate) const LANDMARK_SPACING_M: f32 = 150.0;

/// The smallest footprint that takes a building (m2).
pub(crate) const MIN_AREA_M2: f32 = 30.0;

/// The depth one row of a footprint's buildings takes (m): a deeper
/// footprint takes rows side by side across it.
pub(crate) const ROW_DEPTH_M: f32 = 30.0;

/// The most rows a footprint takes.
const MAX_ROWS: usize = 4;

/// The storeys past which a building is a high-rise.
const HIGH_RISE_STOREYS: u8 = 12;

/// The area past which a cultural or public building is a landmark (m2).
const LANDMARK_AREA_M2: f32 = 1_200.0;

/// The most entities the near copies may be; a near copy past it is drawn
/// as its shell.
pub(crate) const CORE_ENTITY_BUDGET: u32 = 12_000;

/// The most shells the plan draws.
pub(crate) const CORE_FAR_COPIES: usize = 3_000;

/// The salt of the buildings' own random stream.
const STREAM_SALT: u64 = 0xA1C1_5B01_D1E5_0002;

/// The salt of a landmark's own pick, apart from its building's rows.
const LANDMARK_SALT: u64 = 0x1A4D_3A2C_0000_0001;

/// What a building takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    /// One of the theme's landmarks, standing alone.
    Landmark,
    /// A row of the theme's secondary buildings.
    Secondary,
}

/// The role `building` takes, or `None` where it is left out: too small to
/// hold a building.
pub(crate) fn role_of(building: &CoreBuilding) -> Option<Role> {
    if building.area < MIN_AREA_M2 {
        return None;
    }
    let tall = building
        .peak_storeys
        .is_some_and(|storeys| storeys >= HIGH_RISE_STOREYS);
    let large = building.area >= LANDMARK_AREA_M2;
    Some(match building.usage {
        BuildingUse::Religious => Role::Landmark,
        BuildingUse::Cultural | BuildingUse::Public if large => Role::Landmark,
        _ if tall => Role::Landmark,
        _ => Role::Secondary,
    })
}

/// How telling a landmark is, for choosing among them: places of worship,
/// then the large cultural buildings, then the tallest, then the largest.
fn landmark_rank(building: &CoreBuilding) -> (u8, u8, i64) {
    let kind = match building.usage {
        BuildingUse::Religious => 0,
        BuildingUse::Cultural => 1,
        _ => 2,
    };
    (
        kind,
        u8::MAX - building.peak_storeys.unwrap_or(0),
        -(building.area as i64),
    )
}

/// A footprint's oriented box: its middle, the unit direction of its
/// longest edge, and its half length along that and half depth across it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OrientedBox {
    pub centre: (f32, f32),
    pub along: (f32, f32),
    pub half_length: f32,
    pub half_depth: f32,
}

/// The box along `outline`'s longest edge.
pub(crate) fn oriented_box(outline: &[(f32, f32)]) -> Option<OrientedBox> {
    let edge = outline
        .windows(2)
        .map(|w| (w[1].0 - w[0].0, w[1].1 - w[0].1))
        .max_by(|a, b| a.0.hypot(a.1).total_cmp(&b.0.hypot(b.1)))?;
    let length = edge.0.hypot(edge.1);
    if length < 1e-3 {
        return None;
    }
    let along = (edge.0 / length, edge.1 / length);
    let across = (-along.1, along.0);
    let project = |axis: (f32, f32)| {
        outline.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
            let t = p.0 * axis.0 + p.1 * axis.1;
            (lo.min(t), hi.max(t))
        })
    };
    let ((u0, u1), (v0, v1)) = (project(along), project(across));
    let (u, v) = ((u0 + u1) / 2.0, (v0 + v1) / 2.0);
    Some(OrientedBox {
        centre: (along.0 * u + across.0 * v, along.1 * u + across.1 * v),
        along,
        half_length: (u1 - u0) / 2.0,
        half_depth: (v1 - v0) / 2.0,
    })
}

/// Of the two fronts across `bx`'s length, the one nearer `street_yaw`.
fn front_yaw(bx: &OrientedBox, street_yaw: f32) -> f32 {
    let across = (-bx.along.1, bx.along.0);
    let yaw = yaw_towards(across);
    if libm::cosf(yaw - street_yaw) >= 0.0 {
        yaw
    } else {
        yaw_towards((-across.0, -across.1))
    }
}

/// One copy as the fill places it: its entry, scale, place and turn.
pub(crate) type Slot = (&'static dyn CatalogueEntry, f32, (f32, f32), f32);

/// Fill `building`'s footprint with rows (see the module docs) from `pool`
/// at `rank`, leaving clear the stretch of its box's length within
/// `spared` of the box's middle: a landmark's.
pub(crate) fn fill(
    building: &CoreBuilding,
    pool: &[&'static dyn CatalogueEntry],
    rank: f32,
    rng: &mut ChaCha8Rng,
    spared: f32,
) -> Vec<Slot> {
    let Some(bx) = oriented_box(&building.outline) else {
        return Vec::new();
    };
    let across = (-bx.along.1, bx.along.0);
    let street = front_yaw(&bx, building.street_yaw);
    let rows = ((2.0 * bx.half_depth / ROW_DEPTH_M).round() as usize).clamp(1, MAX_ROWS);
    let room = bx.half_depth / rows as f32;
    let stretches = if spared > 0.0 {
        vec![(-bx.half_length, -spared), (spared, bx.half_length)]
    } else {
        vec![(-bx.half_length, bx.half_length)]
    };
    let mut slots = Vec::new();
    for k in 0..rows {
        let v = -bx.half_depth + room * (2 * k + 1) as f32;
        // Of several rows, the outer two front their own sides.
        let yaw = match k {
            _ if rows == 1 => street,
            0 => yaw_towards((-across.0, -across.1)),
            k if k + 1 == rows => yaw_towards(across),
            _ => street,
        };
        for &(from, to) in &stretches {
            for (entry, scale, u) in row(pool, room, (from, to), rank, rng) {
                let at = (
                    bx.centre.0 + bx.along.0 * u + across.0 * v,
                    bx.centre.1 + bx.along.1 * u + across.1 * v,
                );
                if contains(&building.outline, at) {
                    slots.push((entry, scale, at, yaw));
                }
            }
        }
    }
    slots
}

/// A row of entries from `pool` at `rank`, each fitted to `room`, set side
/// by side from `from` to `to` along a box's length and centred there:
/// each entry, its scale, and its middle along the length.
fn row(
    pool: &[&'static dyn CatalogueEntry],
    room: f32,
    (from, to): (f32, f32),
    rank: f32,
    rng: &mut ChaCha8Rng,
) -> Vec<(&'static dyn CatalogueEntry, f32, f32)> {
    let mut row = Vec::new();
    let mut cursor = from;
    while let Some((entry, scale)) = pick_ranked(pool, room, rank, rng) {
        let width = 2.0 * radius(entry) * scale;
        if cursor + width > to + 1e-3 {
            break;
        }
        row.push((entry, scale, cursor + width / 2.0));
        cursor += width;
    }
    let shift = (to - cursor) / 2.0;
    row.into_iter()
        .map(|(entry, scale, u)| (entry, scale, u + shift))
        .collect()
}

/// The buildings' plan (see the module docs), for the room `did`, keeping
/// clear of `kept`, standing on `ground`.
pub(crate) fn plan(
    buildings: &[CoreBuilding],
    did: &str,
    kept: &Kept,
    ground: &dyn Fn(f32, f32) -> f32,
) -> Plan {
    let (theme, character) = room_theme(did);
    let landmarks = sized_pool(theme, StructureRole::Landmark, character);
    let secondaries = sized_pool(theme, StructureRole::Secondary, character);
    let seed = fnv1a_64(did) ^ STREAM_SALT;

    // The landmarks: the most telling, spaced, each counted only once it
    // can stand - one that gives way takes no landmark's place.
    let mut candidates: Vec<usize> = (0..buildings.len())
        .filter(|&i| role_of(&buildings[i]) == Some(Role::Landmark))
        .collect();
    candidates.sort_by_key(|&i| (landmark_rank(&buildings[i]), i));
    let mut landmark_at: Vec<(f32, f32)> = Vec::new();
    let mut standing: Vec<Option<Slot>> = vec![None; buildings.len()];
    for i in candidates {
        if landmark_at.len() == MAX_CORE_LANDMARKS {
            break;
        }
        let building = &buildings[i];
        let c = centroid(&building.outline);
        let apart = landmark_at
            .iter()
            .all(|&(x, z)| (x - c.0).hypot(z - c.1) >= LANDMARK_SPACING_M);
        if !apart {
            continue;
        }
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ fnv1a_64(&building.id) ^ LANDMARK_SALT);
        if let Some(slot) = landmark_slot(building, &landmarks, kept, &mut rng) {
            landmark_at.push(c);
            standing[i] = Some(slot);
        }
    }
    // Every building's place by height among them all, in (0, 1).
    let mut ranked: Vec<usize> = (0..buildings.len())
        .filter(|&i| role_of(&buildings[i]).is_some())
        .collect();
    ranked.sort_by_key(|&i| (buildings[i].peak_storeys.unwrap_or(0), i));
    let mut rank = vec![0.5; buildings.len()];
    for (place, &i) in ranked.iter().enumerate() {
        rank[i] = (place as f32 + 0.5) / ranked.len() as f32;
    }

    let mut plan = Plan {
        label: "core buildings",
        policy: Policy {
            near_entities: CORE_ENTITY_BUDGET,
            far_copies: CORE_FAR_COPIES,
            cut: false,
        },
        buildings: Vec::new(),
        copies: Vec::new(),
        did: did.to_owned(),
        character,
        seed,
    };
    let mut by_key: HashMap<(&'static str, i64), usize> = HashMap::new();
    for (i, building) in buildings.iter().enumerate() {
        if role_of(building).is_none() {
            continue;
        }
        // Each building's picks its own, by its uuid: the same on every
        // visit, whatever else the city's data gains or loses.
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ fnv1a_64(&building.id));
        // The rows either side of a landmark, or across the whole box.
        let alone = standing[i];
        let spared = alone.map_or(0.0, |(entry, scale, ..)| radius(entry) * scale);
        let slots: Vec<Slot> = alone
            .into_iter()
            .chain(fill(building, &secondaries, rank[i], &mut rng, spared))
            .collect();
        for (entry, scale, (x, z), yaw) in slots {
            let reach = radius(entry) * scale;
            if !kept.clear(x, z, reach) {
                continue;
            }
            let key = (entry.slug(), scale_e4(scale));
            let index = *by_key.entry(key).or_insert_with(|| {
                plan.buildings.push(PlannedBuilding::new(
                    entry,
                    scale,
                    Grow::Built,
                    cache_key("core", key.0, key.1),
                ));
                plan.buildings.len() - 1
            });
            let y = footing(x, z, reach, ground) - FOUNDATION_SINK_M;
            plan.copies.push(PlannedCopy {
                building: index,
                pose: Transform::from_xyz(x, y, z).with_rotation(Quat::from_rotation_y(yaw)),
                near: true,
                source: SourceId::new(SourceLayer::Building, building.id.clone()),
                height: None,
                solid: Solid::Shell,
            });
        }
    }
    // Nearest the landing first: the near budget keeps those.
    let from_landing =
        |c: &PlannedCopy| kept.landing_distance2(c.pose.translation.x, c.pose.translation.z);
    plan.copies
        .sort_by(|a, b| from_landing(a).total_cmp(&from_landing(b)));
    plan
}

/// Where `building`'s landmark stands, alone at its box's middle, fitted to
/// the box's narrower side: `None` where that middle is off its footprint,
/// no landmark fits, or the one picked would reach what the record keeps.
fn landmark_slot(
    building: &CoreBuilding,
    landmarks: &[&'static dyn CatalogueEntry],
    kept: &Kept,
    rng: &mut ChaCha8Rng,
) -> Option<Slot> {
    let bx = oriented_box(&building.outline)?;
    if !contains(&building.outline, bx.centre) {
        return None;
    }
    let (entry, scale) = pick_landmark(landmarks, bx.half_length.min(bx.half_depth), rng)?;
    kept.clear(bx.centre.0, bx.centre.1, radius(entry) * scale)
        .then(|| (entry, scale, bx.centre, front_yaw(&bx, building.street_yaw)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeded_defaults::ThemeArchetype;

    use super::super::super::fit::did_of;

    /// A closed rectangle `length` x `depth` about `centre`, its length
    /// along `angle` (radians from world +x towards +z).
    fn rectangle(centre: (f32, f32), length: f32, depth: f32, angle: f32) -> Vec<(f32, f32)> {
        let (c, s) = (angle.cos(), angle.sin());
        let corner = |u: f32, v: f32| (centre.0 + u * c - v * s, centre.1 + u * s + v * c);
        let (l, d) = (length / 2.0, depth / 2.0);
        vec![
            corner(-l, -d),
            corner(l, -d),
            corner(l, d),
            corner(-l, d),
            corner(-l, -d),
        ]
    }

    fn building(id: &str, outline: Vec<(f32, f32)>, street: (f32, f32)) -> CoreBuilding {
        CoreBuilding {
            id: id.into(),
            area: crate::terrain::geo::street_level::outline_area(&outline),
            outline,
            usage: BuildingUse::Residential,
            storeys: Some(5),
            peak_storeys: Some(5),
            street_yaw: yaw_towards(street),
        }
    }

    #[test]
    fn a_footprints_box_lies_along_its_longest_edge() {
        let angle = 0.5f32;
        let bx = oriented_box(&rectangle((5.0, -3.0), 40.0, 10.0, angle)).expect("a box");
        assert!((bx.centre.0 - 5.0).abs() < 1e-3 && (bx.centre.1 + 3.0).abs() < 1e-3);
        assert!((bx.half_length - 20.0).abs() < 1e-3 && (bx.half_depth - 5.0).abs() < 1e-3);
        let dot = bx.along.0 * angle.cos() + bx.along.1 * angle.sin();
        assert!((dot.abs() - 1.0).abs() < 1e-5, "{:?}", bx.along);
        assert!(oriented_box(&[(1.0, 1.0), (1.0, 1.0)]).is_none());
    }

    /// A terrace 60 m long and 12 deep with its street to the south takes
    /// a row of houses down its length, side by side, each inside it and
    /// fitted to its depth, every one facing south.
    #[test]
    fn a_footprint_takes_a_row_down_its_length_facing_its_street() {
        let did = did_of(ThemeArchetype::ModernCity);
        let (theme, character) = room_theme(&did);
        let pool = sized_pool(theme, StructureRole::Secondary, character);
        let terrace = building(
            "terrace",
            rectangle((0.0, 0.0), 60.0, 12.0, 0.0),
            (0.0, 1.0),
        );
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let slots = fill(&terrace, &pool, 0.5, &mut rng, 0.0);
        assert!(slots.len() >= 2, "{} slots", slots.len());
        let mut extent = 0.0;
        for &(entry, scale, (x, z), yaw) in &slots {
            let reach = radius(entry) * scale;
            assert!(reach <= 6.0 + 1e-4, "{} reaches {reach}", entry.slug());
            assert!(contains(&terrace.outline, (x, z)));
            assert!(z.abs() < 1e-3, "on the row's line");
            let f = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
            assert!(f.distance(Vec3::Z) < 1e-5, "faces the street: {f}");
            extent += 2.0 * reach;
        }
        assert!(extent <= 60.0 + 1e-3, "side by side: {extent} m");
        // The street on the north turns the row round.
        let north = building("terrace", terrace.outline.clone(), (0.0, -1.0));
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        for (.., yaw) in fill(&north, &pool, 0.5, &mut rng, 0.0) {
            let f = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
            assert!(f.distance(Vec3::NEG_Z) < 1e-5, "{f}");
        }
    }

    /// A block 80 m long and 64 deep takes two rows, each fronting its own
    /// side; with a landmark's 20 m spared at its middle, neither row
    /// reaches into it.
    #[test]
    fn a_deep_footprint_takes_rows_fronting_both_its_sides() {
        let did = did_of(ThemeArchetype::ModernCity);
        let (theme, character) = room_theme(&did);
        let pool = sized_pool(theme, StructureRole::Secondary, character);
        let block = building("block", rectangle((0.0, 0.0), 80.0, 64.0, 0.0), (0.0, 1.0));
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        let slots = fill(&block, &pool, 0.5, &mut rng, 0.0);
        let (north, south): (Vec<&Slot>, Vec<&Slot>) =
            slots.iter().partition(|(.., (_, z), _)| *z < 0.0);
        assert!(!north.is_empty() && !south.is_empty());
        for (rows, z, facing) in [(&north, -16.0, Vec3::NEG_Z), (&south, 16.0, Vec3::Z)] {
            for &&(entry, scale, (_, at_z), yaw) in rows {
                assert!((at_z - z).abs() < 1e-3, "{at_z}");
                assert!(radius(entry) * scale <= 16.0 + 1e-4);
                let f = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
                assert!(f.distance(facing) < 1e-5, "{f} for the row at {z}");
            }
        }
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        for (entry, scale, (x, _), _) in fill(&block, &pool, 0.5, &mut rng, 10.0) {
            assert!(
                x.abs() >= 10.0 + radius(entry) * scale - 1e-3,
                "{x} in the spared middle"
            );
        }
    }

    /// The Museumsinsel's buildings: the cathedral and a few museums take
    /// landmarks, spaced, every other building a row; every copy stands on
    /// its own footprint, clear of the landing, on its shell.
    #[test]
    fn the_museumsinsel_takes_landmarks_and_rows_on_its_footprints() {
        let level = crate::terrain::geo::tests::museum_level();
        let did = did_of(ThemeArchetype::ModernCity);
        // Landing on the square's southern edge, clear of the cathedral.
        let landing = (0.0, 280.0);
        let kept = Kept {
            discs: vec![(landing.0, landing.1, super::super::LANDING_CLEAR_M)],
        };
        let plan = plan(&level.buildings, &did, &kept, &|_, _| 30.0);
        let by_id: HashMap<&str, &CoreBuilding> =
            level.buildings.iter().map(|b| (&*b.id, b)).collect();
        assert!(plan.copies.len() > level.buildings.len(), "rows of several");
        let mut landmarks: Vec<&CoreBuilding> = Vec::new();
        for copy in &plan.copies {
            let building = by_id[&*copy.source.key];
            let planned = &plan.buildings[copy.building];
            let (x, z) = (copy.pose.translation.x, copy.pose.translation.z);
            assert!(
                contains(&building.outline, (x, z)),
                "{} off its footprint",
                copy.source
            );
            let reach = radius(planned.entry) * planned.scale;
            assert!(
                (x - landing.0).hypot(z - landing.1) >= super::super::LANDING_CLEAR_M + reach,
                "{} at the landing",
                copy.source
            );
            assert_eq!(copy.source.layer, SourceLayer::Building);
            assert_eq!(
                (copy.solid, copy.height, copy.near),
                (Solid::Shell, None, true)
            );
            if planned.entry.role() == StructureRole::Landmark
                && !landmarks.iter().any(|b| b.id == building.id)
            {
                landmarks.push(building);
            }
        }
        assert!(
            (1..=MAX_CORE_LANDMARKS).contains(&landmarks.len()),
            "{} landmarks",
            landmarks.len()
        );
        assert!(
            landmarks.iter().any(|b| b.usage == BuildingUse::Religious),
            "the cathedral is one"
        );
        for (i, a) in landmarks.iter().enumerate() {
            for b in &landmarks[i + 1..] {
                let (ca, cb) = (centroid(&a.outline), centroid(&b.outline));
                assert!((ca.0 - cb.0).hypot(ca.1 - cb.1) >= LANDMARK_SPACING_M);
            }
        }
        // Nearest the landing first, which the near budget keeps.
        let from_landing =
            |c: &PlannedCopy| kept.landing_distance2(c.pose.translation.x, c.pose.translation.z);
        assert!(
            plan.copies
                .windows(2)
                .all(|w| from_landing(&w[0]) <= from_landing(&w[1]))
        );
        // The same room draws the same city.
        let again = super::plan(&level.buildings, &did, &kept, &|_, _| 30.0);
        let picks = |p: &Plan| -> Vec<(String, PlannedCopy)> {
            p.copies
                .iter()
                .map(|c| (p.buildings[c.building].key.clone(), c.clone()))
                .collect()
        };
        assert_eq!(picks(&plan), picks(&again));
    }

    /// A landmark that cannot stand takes no landmark's place: the first of
    /// two churches 100 m apart is at the landing and gives way, so the
    /// second - within the first's spacing - stands as one.
    #[test]
    fn a_landmark_that_gives_way_takes_no_landmarks_place() {
        let did = did_of(ThemeArchetype::ModernCity);
        let church = |id: &str, x: f32, area_scale: f32| CoreBuilding {
            usage: BuildingUse::Religious,
            ..building(
                id,
                rectangle((x, 0.0), 60.0 * area_scale, 40.0, 0.0),
                (0.0, 1.0),
            )
        };
        let (blocked, open) = (church("first", 0.0, 1.1), church("second", 100.0, 1.0));
        assert!(
            landmark_rank(&blocked) < landmark_rank(&open),
            "the blocked one comes first"
        );
        let plan = plan(&[blocked, open], &did, &Kept::of(None), &|_, _| 30.0);
        let roles = |id: &str| -> Vec<StructureRole> {
            plan.copies
                .iter()
                .filter(|c| &*c.source.key == id)
                .map(|c| plan.buildings[c.building].entry.role())
                .collect()
        };
        assert!(
            roles("first")
                .iter()
                .all(|r| *r == StructureRole::Secondary)
        );
        assert!(
            roles("second").contains(&StructureRole::Landmark),
            "{:?}",
            roles("second")
        );
    }

    /// The default landing is the square's middle, 25 m from the
    /// cathedral's: its landmark would reach the landing, so the cathedral
    /// takes a row, less the slots at the landing - not nothing.
    #[test]
    fn a_landmark_reaching_the_landing_gives_way_to_a_row() {
        let level = crate::terrain::geo::tests::museum_level();
        let did = did_of(ThemeArchetype::ModernCity);
        let cathedral = level
            .buildings
            .iter()
            .find(|b| b.usage == BuildingUse::Religious)
            .expect("the Berliner Dom");
        let plan = plan(&level.buildings, &did, &Kept::of(None), &|_, _| 30.0);
        let on_it: Vec<&PlannedCopy> = plan
            .copies
            .iter()
            .filter(|c| c.source.key == cathedral.id)
            .collect();
        assert!(
            !on_it.is_empty(),
            "the cathedral's footprint is not left bare"
        );
        for copy in on_it {
            let planned = &plan.buildings[copy.building];
            assert_eq!(planned.entry.role(), StructureRole::Secondary);
            let (x, z) = (copy.pose.translation.x, copy.pose.translation.z);
            assert!(
                x.hypot(z) >= super::super::LANDING_CLEAR_M + radius(planned.entry) * planned.scale
            );
        }
    }
}
