//! Berlin's buildings on the walkable ground (#1588): each ALKIS building as
//! the theme's catalogue buildings on its footprint.
//!
//! A building's use and storeys pick its role ([`role_of`]): a church, a
//! large or tall cultural building, a high-rise takes one of the theme's
//! landmarks - at most [`MAX_CORE_LANDMARKS`], no two within
//! [`LANDMARK_SPACING_M`], the most telling first - and every other building
//! takes the theme's secondary buildings. A footprint under
//! [`MIN_AREA_M2`], a kiosk's or a public toilet's or a transformer box's,
//! is left out.
//!
//! A footprint is filled along its long side. Its oriented box - the box
//! along its longest edge - takes rows down its length, one per
//! [`ROW_DEPTH_M`] of its depth. A row turns to the footprint's street
//! side; of several, the outer two turn out, each to its own side, as a
//! block's houses front the streets either side of it. A landmark stands at
//! the box's middle, fitted to its narrower side, and the rows fill the
//! box's length either side of it; one that fits nowhere, whose box's
//! middle is off its footprint - a courtyard - or that would reach what the
//! record keeps gives way to the rows alone, whose slots each keep clear of
//! it. Each slot whose middle lies inside the footprint takes a copy.
//!
//! Where the theme has its street buildings (#1598,
//! [`super::super::streets`]), a row is drawn from those alone, each copy
//! shaped to the footprint ([`fill_street`]): the kind by the building's
//! storeys and use, its storeys Berlin's, its ground floor trading where
//! Berlin's use is a shop's, a workshop's or mixed; the depth its kind's
//! deepest that fits the row, the copy's front on the row's own edge as a
//! Berlin house stands on its street line; and frontages down the row's
//! length, a smaller kind's in a tail its own leaves. A theme without them
//! fills its rows as before ([`fill`]): its secondary buildings set side
//! by side, each fitted to its row's depth, bigger where Berlin's building
//! stands taller.
//!
//! Every copy stands on its building's voxel shell as a collider, and the
//! nearest the landing are drawn near: past the plan's entity budget a copy
//! is drawn as that shell.

use std::collections::HashMap;

use bevy::prelude::*;
use geodata::berlin::BuildingUse;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use crate::catalogue::items::street::{StreetFit, StreetKind};
use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::seeded_defaults::fnv1a_64;
use crate::terrain::geo::street_level::{CoreBuilding, centroid, contains, yaw_towards};
use crate::terrain::lots::{FOUNDATION_SINK_M, fitted_scale, scale_e4};

use super::super::fit::{
    RoomScene, SCALE_MIN, cache_key, footing, pick_landmark, pick_ranked, radius, sized_pool,
    street_cache_key,
};
use super::super::plan::{Grow, Plan, PlannedBuilding, PlannedCopy, Policy, Solid};
use super::super::streets::{
    self, STREET_VARIANTS, StreetKey, Streets, depth_for, kind_of, smaller,
};
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

/// The storeys a building is drawn with where ALKIS has none: a Berlin
/// street's own.
const USUAL_STOREYS: u8 = 5;

/// The most street templates the plan grows (#1598,
/// [`streets::template`]): each a tree, its merged meshes and its shell,
/// grown and baked before its first copy stands.
pub(crate) const MAX_STREET_TEMPLATES: usize = 96;

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

/// The street building `building` is drawn as (#1598): its kind
/// ([`kind_of`]), its storeys - ALKIS's, else its tallest part's, else
/// [`USUAL_STOREYS`] - and whether its ground floor trades: a shop's, a
/// workshop's, a garage's, a utility's or a mixed building's does.
pub(crate) fn street_of(building: &CoreBuilding) -> (StreetKind, u8, bool) {
    let storeys = building
        .storeys
        .or(building.peak_storeys)
        .unwrap_or(USUAL_STOREYS)
        .max(1);
    let works = matches!(
        building.usage,
        BuildingUse::Commercial
            | BuildingUse::Industrial
            | BuildingUse::Parking
            | BuildingUse::Utility
    );
    let trade = works || building.usage == BuildingUse::Mixed;
    (kind_of(storeys, works), storeys, trade)
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
    let edge = edges(outline).max_by(|a, b| a.0.hypot(a.1).total_cmp(&b.0.hypot(b.1)))?;
    box_along(outline, edge)
}

/// How far from square to its street a footprint's front may run, as the
/// sine of the angle: 30 degrees.
const FRONT_SKEW_SIN: f32 = 0.5;

/// The box along `outline`'s street front (#1598): the longest of its edges
/// that run within 30 degrees of square to its street (`street_yaw` faces
/// it) - where Berlin builds on the street line - or its longest edge where
/// none does. A house with a side wing has the wing for its longest edge,
/// and a row down the wing would turn its fronts on its neighbours.
pub(crate) fn street_box(outline: &[(f32, f32)], street_yaw: f32) -> Option<OrientedBox> {
    let to_street = (-libm::sinf(street_yaw), -libm::cosf(street_yaw));
    edges(outline)
        .filter(|e| {
            let length = e.0.hypot(e.1);
            length > 1e-3
                && (e.0 * to_street.0 + e.1 * to_street.1).abs() <= FRONT_SKEW_SIN * length
        })
        .max_by(|a, b| a.0.hypot(a.1).total_cmp(&b.0.hypot(b.1)))
        .map_or_else(|| oriented_box(outline), |edge| box_along(outline, edge))
}

/// `outline`'s edges, each as the step from its start to its end.
fn edges(outline: &[(f32, f32)]) -> impl Iterator<Item = (f32, f32)> + '_ {
    outline
        .windows(2)
        .map(|w| (w[1].0 - w[0].0, w[1].1 - w[0].1))
}

/// The box of `outline` along `edge`'s direction.
fn box_along(outline: &[(f32, f32)], edge: (f32, f32)) -> Option<OrientedBox> {
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

/// Whether, of the two fronts across `bx`'s length, the one nearer
/// `street_yaw` faces the box's `across` (the left of its `along`).
fn fronts_across(bx: &OrientedBox, street_yaw: f32) -> bool {
    let across = (-bx.along.1, bx.along.0);
    libm::cosf(yaw_towards(across) - street_yaw) >= 0.0
}

/// The turn that faces a front across `bx` - to its `across` side or away.
fn facing(bx: &OrientedBox, to_across: bool) -> f32 {
    let across = (-bx.along.1, bx.along.0);
    if to_across {
        yaw_towards(across)
    } else {
        yaw_towards((-across.0, -across.1))
    }
}

/// Of the two fronts across `bx`'s length, the one nearer `street_yaw`.
fn front_yaw(bx: &OrientedBox, street_yaw: f32) -> f32 {
    facing(bx, fronts_across(bx, street_yaw))
}

/// One copy as a fill places it.
#[derive(Clone, Copy)]
pub(crate) struct Slot {
    pub entry: &'static dyn CatalogueEntry,
    pub scale: f32,
    /// Where its middle stands.
    pub at: (f32, f32),
    /// The turn that faces its front.
    pub yaw: f32,
    /// A street building's (#1598): the fit it is built to and which of its
    /// seeds draws it.
    pub street: Option<(StreetFit, u8)>,
}

impl Slot {
    /// A copy of `entry` as it builds itself.
    fn built(entry: &'static dyn CatalogueEntry, scale: f32, at: (f32, f32), yaw: f32) -> Self {
        Slot {
            entry,
            scale,
            at,
            yaw,
            street: None,
        }
    }

    /// How far it reaches from where it stands, turned any way (m).
    pub(crate) fn reach(&self) -> f32 {
        match self.street {
            Some((fit, _)) => fit.reach_m() * self.scale,
            None => radius(self.entry) * self.scale,
        }
    }
}

/// The stretches of a box `half_length` either side of its middle that its
/// rows fill, leaving clear `spared` of its middle: a landmark's.
fn stretches(half_length: f32, spared: f32) -> Vec<(f32, f32)> {
    if spared > 0.0 {
        vec![(-half_length, -spared), (spared, half_length)]
    } else {
        vec![(-half_length, half_length)]
    }
}

/// How many rows a box `half_depth` deep takes.
fn rows_of(half_depth: f32) -> usize {
    ((2.0 * half_depth / ROW_DEPTH_M).round() as usize).clamp(1, MAX_ROWS)
}

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
    let rows = rows_of(bx.half_depth);
    let room = bx.half_depth / rows as f32;
    let mut slots = Vec::new();
    for k in 0..rows {
        let v = -bx.half_depth + room * (2 * k + 1) as f32;
        // Of several rows, the outer two front their own sides.
        let yaw = match k {
            _ if rows == 1 => street,
            0 => facing(&bx, false),
            k if k + 1 == rows => facing(&bx, true),
            _ => street,
        };
        for (from, to) in stretches(bx.half_length, spared) {
            for (entry, scale, u) in row(pool, room, (from, to), rank, rng) {
                let at = (
                    bx.centre.0 + bx.along.0 * u + across.0 * v,
                    bx.centre.1 + bx.along.1 * u + across.1 * v,
                );
                if contains(&building.outline, at) {
                    slots.push(Slot::built(entry, scale, at, yaw));
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

/// Fill `building`'s footprint with rows of the theme's street buildings
/// (#1598, see the module docs), leaving clear the stretch of its box's
/// length within `spared` of its middle: a landmark's.
pub(crate) fn fill_street(
    building: &CoreBuilding,
    streets: &Streets,
    rng: &mut ChaCha8Rng,
    spared: f32,
) -> Vec<Slot> {
    // Its rows run along its street front - or, where a landmark stands at
    // the middle of the box along its longest edge, along that box, so the
    // stretch they spare is the landmark's.
    let bx = if spared > 0.0 {
        oriented_box(&building.outline)
    } else {
        street_box(&building.outline, building.street_yaw)
    };
    let Some(bx) = bx else {
        return Vec::new();
    };
    let across = (-bx.along.1, bx.along.0);
    let street = fronts_across(&bx, building.street_yaw);
    let (kind, storeys, trade) = street_of(building);
    let rows = rows_of(bx.half_depth);
    let band = 2.0 * bx.half_depth / rows as f32;
    let mut slots = Vec::new();
    for k in 0..rows {
        let (v0, v1) = (
            -bx.half_depth + band * k as f32,
            -bx.half_depth + band * (k + 1) as f32,
        );
        // Of several rows, the outer two front their own sides.
        let to_across = match k {
            _ if rows == 1 => street,
            0 => false,
            k if k + 1 == rows => true,
            _ => street,
        };
        let yaw = facing(&bx, to_across);
        for (from, to) in stretches(bx.half_length, spared) {
            for (entry, fit, scale, u) in
                street_row(streets, kind, (storeys, trade), band, (from, to), rng)
            {
                // Its front on the row's own edge: Berlin's street line.
                let depth = fit.depth_m() * scale;
                let v = if to_across {
                    v1 - depth / 2.0
                } else {
                    v0 + depth / 2.0
                };
                let at = (
                    bx.centre.0 + bx.along.0 * u + across.0 * v,
                    bx.centre.1 + bx.along.1 * u + across.1 * v,
                );
                if contains(&building.outline, at) {
                    let variant = (rng.next_u32() % u32::from(STREET_VARIANTS)) as u8;
                    slots.push(Slot {
                        entry,
                        scale,
                        at,
                        yaw,
                        street: Some((fit, variant)),
                    });
                }
            }
        }
    }
    slots
}

/// A row of `streets`' buildings from `from` to `to` along a box's length,
/// in a row `band` deep, centred there: each building's entry, its fit, its
/// scale and its middle along the length. The row is `kind`'s, at `storeys`,
/// trading or not; its frontages are rolled from its kind's, each leaving
/// room for another while the row has it and the widest that fits after,
/// and a tail its kind leaves takes the next kind down.
fn street_row(
    streets: &Streets,
    kind: StreetKind,
    (storeys, trade): (u8, bool),
    band: f32,
    (from, to): (f32, f32),
    rng: &mut ChaCha8Rng,
) -> Vec<(&'static dyn CatalogueEntry, StreetFit, f32, f32)> {
    let mut picks: Vec<(&'static dyn CatalogueEntry, StreetFit, f32, f32)> = Vec::new();
    let mut left = to - from;
    let mut kind = Some(kind);
    while let Some(k) = kind {
        let Some((depth, scale)) = depth_for(k, band) else {
            kind = smaller(k);
            continue;
        };
        let fitting: Vec<(u16, f32)> = k
            .frontages()
            .iter()
            .map(|&f| (f, f32::from(f) * scale))
            .filter(|&(_, width)| width <= left + 1e-3)
            .collect();
        let Some(&(_, narrowest)) = fitting.first() else {
            // A row too short for its kind's narrowest at all: that drawn
            // smaller, so a narrow Berlin house keeps its storeys rather
            // than turn into a low building. A tail takes the kind below.
            let first = f32::from(k.frontages()[0]);
            let smaller_scale = fitted_scale(left / first, SCALE_MIN, scale);
            if picks.is_empty() && k != StreetKind::Low && first * smaller_scale <= left + 1e-3 {
                let fit = StreetFit::new(k.frontages()[0], depth, storeys, trade).snapped(k);
                picks.push((streets.entry(k), fit, smaller_scale, first * smaller_scale));
                left -= first * smaller_scale;
            }
            kind = smaller(k);
            continue;
        };
        let leaving: Vec<(u16, f32)> = fitting
            .iter()
            .copied()
            .filter(|&(_, width)| left - width >= narrowest - 1e-3)
            .collect();
        let (frontage, width) = if leaving.is_empty() {
            *fitting.last().expect("one fits")
        } else {
            leaving[rng.next_u32() as usize % leaving.len()]
        };
        let fit = StreetFit::new(frontage, depth, storeys, trade).snapped(k);
        picks.push((streets.entry(k), fit, scale, width));
        left -= width;
    }
    let mut cursor = from + left / 2.0;
    picks
        .into_iter()
        .map(|(entry, fit, scale, width)| {
            let u = cursor + width / 2.0;
            cursor += width;
            (entry, fit, scale, u)
        })
        .collect()
}

/// The buildings' plan (see the module docs), for `room`, keeping clear of
/// `kept`, standing on `ground`.
pub(crate) fn plan(
    buildings: &[CoreBuilding],
    room: &RoomScene,
    kept: &Kept,
    ground: &dyn Fn(f32, f32) -> f32,
) -> Plan {
    let (theme, character) = room.theme();
    let landmarks = sized_pool(theme, StructureRole::Landmark, character);
    let mut secondaries = sized_pool(theme, StructureRole::Secondary, character);
    let streets = Streets::of(&secondaries);
    // A theme without its street buildings fills its rows with the rest.
    secondaries.retain(|entry| entry.street().is_none());
    let seed = room.seed ^ STREAM_SALT;

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
        did: room.did.clone(),
        character,
        seed,
    };
    let mut by_key: HashMap<(&'static str, i64), usize> = HashMap::new();
    let mut by_street: HashMap<StreetKey, usize> = HashMap::new();
    let mut street_keys: Vec<StreetKey> = Vec::new();
    // Nearest the landing first, so the street templates' budget goes to
    // what a visitor sees first.
    let near: Vec<f32> = buildings
        .iter()
        .map(|b| {
            let c = centroid(&b.outline);
            kept.landing_distance2(c.0, c.1)
        })
        .collect();
    let mut order: Vec<usize> = (0..buildings.len()).collect();
    order.sort_by(|&a, &b| near[a].total_cmp(&near[b]).then(a.cmp(&b)));
    for i in order {
        let building = &buildings[i];
        if role_of(building).is_none() {
            continue;
        }
        // Each building's picks its own, by its uuid: the same on every
        // visit, whatever else the city's data gains or loses.
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ fnv1a_64(&building.id));
        // The rows either side of a landmark, or across the whole box.
        let alone = standing[i];
        let spared = alone.map_or(0.0, |slot| slot.reach());
        let rows = match &streets {
            Some(streets) => fill_street(building, streets, &mut rng, spared),
            None => fill(building, &secondaries, rank[i], &mut rng, spared),
        };
        for slot in alone.into_iter().chain(rows) {
            let (mut x, mut z) = slot.at;
            if !kept.clear(x, z, slot.reach()) {
                continue;
            }
            let slug = slot.entry.slug();
            let index = match slot.street {
                None => {
                    let key = (slug, scale_e4(slot.scale));
                    *by_key.entry(key).or_insert_with(|| {
                        plan.buildings.push(PlannedBuilding::new(
                            slot.entry,
                            slot.scale,
                            Grow::Built,
                            cache_key("core", key.0, key.1),
                        ));
                        plan.buildings.len() - 1
                    })
                }
                Some((fit, variant)) => {
                    let want = (slug, fit, variant, scale_e4(slot.scale));
                    let Some(key) = streets::template(&street_keys, MAX_STREET_TEMPLATES, want)
                    else {
                        continue;
                    };
                    // A shallower template keeps its front on the street
                    // line: it stands back from where the wanted one would.
                    let back = (fit.depth_m() - key.1.depth_m()) * slot.scale / 2.0;
                    let front = Quat::from_rotation_y(slot.yaw) * Vec3::NEG_Z;
                    (x, z) = (x + front.x * back, z + front.z * back);
                    *by_street.entry(key).or_insert_with(|| {
                        street_keys.push(key);
                        let (_, fit, variant, _) = key;
                        plan.buildings.push(PlannedBuilding::new(
                            slot.entry,
                            slot.scale,
                            Grow::Street { fit, variant },
                            street_cache_key("core", key),
                        ));
                        plan.buildings.len() - 1
                    })
                }
            };
            let reach = plan.buildings[index].reach();
            let y = footing(x, z, reach, ground) - FOUNDATION_SINK_M;
            plan.copies.push(PlannedCopy {
                building: index,
                pose: Transform::from_xyz(x, y, z).with_rotation(Quat::from_rotation_y(slot.yaw)),
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
        .then(|| Slot::built(entry, scale, bx.centre, front_yaw(&bx, building.street_yaw)))
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

    /// The modern city's secondary buildings other than its street ones:
    /// a pool as a theme without street buildings fills its rows from.
    fn unstreeted_pool() -> Vec<&'static dyn CatalogueEntry> {
        let did = did_of(ThemeArchetype::ModernCity);
        let (theme, character) = RoomScene::for_did(&did).theme();
        let mut pool = sized_pool(theme, StructureRole::Secondary, character);
        pool.retain(|entry| entry.street().is_none());
        pool
    }

    /// The modern city's street buildings.
    fn city_streets() -> Streets {
        let did = did_of(ThemeArchetype::ModernCity);
        let (theme, character) = RoomScene::for_did(&did).theme();
        Streets::of(&sized_pool(theme, StructureRole::Secondary, character))
            .expect("the modern city has its street buildings")
    }

    /// A terrace 60 m long and 12 deep with its street to the south takes
    /// a row of houses down its length, side by side, each inside it and
    /// fitted to its depth, every one facing south.
    #[test]
    fn a_footprint_takes_a_row_down_its_length_facing_its_street() {
        let pool = unstreeted_pool();
        let terrace = building(
            "terrace",
            rectangle((0.0, 0.0), 60.0, 12.0, 0.0),
            (0.0, 1.0),
        );
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let slots = fill(&terrace, &pool, 0.5, &mut rng, 0.0);
        assert!(slots.len() >= 2, "{} slots", slots.len());
        let mut extent = 0.0;
        for slot in &slots {
            let ((x, z), yaw, reach) = (slot.at, slot.yaw, slot.reach());
            assert!(reach <= 6.0 + 1e-4, "{} reaches {reach}", slot.entry.slug());
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
        for slot in fill(&north, &pool, 0.5, &mut rng, 0.0) {
            let f = Quat::from_rotation_y(slot.yaw) * Vec3::NEG_Z;
            assert!(f.distance(Vec3::NEG_Z) < 1e-5, "{f}");
        }
    }

    /// A block 80 m long and 64 deep takes two rows, each fronting its own
    /// side; with a landmark's 20 m spared at its middle, neither row
    /// reaches into it.
    #[test]
    fn a_deep_footprint_takes_rows_fronting_both_its_sides() {
        let pool = unstreeted_pool();
        let block = building("block", rectangle((0.0, 0.0), 80.0, 64.0, 0.0), (0.0, 1.0));
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        let slots = fill(&block, &pool, 0.5, &mut rng, 0.0);
        let (north, south): (Vec<&Slot>, Vec<&Slot>) =
            slots.iter().partition(|slot| slot.at.1 < 0.0);
        assert!(!north.is_empty() && !south.is_empty());
        for (rows, z, facing) in [(&north, -16.0, Vec3::NEG_Z), (&south, 16.0, Vec3::Z)] {
            for slot in rows.iter() {
                assert!((slot.at.1 - z).abs() < 1e-3, "{}", slot.at.1);
                assert!(slot.reach() <= 16.0 + 1e-4);
                let f = Quat::from_rotation_y(slot.yaw) * Vec3::NEG_Z;
                assert!(f.distance(facing) < 1e-5, "{f} for the row at {z}");
            }
        }
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        for slot in fill(&block, &pool, 0.5, &mut rng, 10.0) {
            assert!(
                slot.at.0.abs() >= 10.0 + slot.reach() - 1e-3,
                "{} in the spared middle",
                slot.at.0
            );
        }
    }

    /// A terrace of five storeys 60 m long and 12 deep, its street to the
    /// south, takes a street of the theme's houses (#1598): each at its
    /// deepest step that fits the row, 11 m, at Berlin's storeys, its front
    /// on the footprint's southern edge and facing south; side by side, the
    /// row's gap less than a house.
    #[test]
    fn a_footprint_takes_a_street_of_its_themes_houses() {
        let streets = city_streets();
        let terrace = building(
            "terrace",
            rectangle((0.0, 0.0), 60.0, 12.0, 0.0),
            (0.0, 1.0),
        );
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let slots = fill_street(&terrace, &streets, &mut rng, 0.0);
        assert!(slots.len() >= 3, "{} houses", slots.len());
        let mut frontage = 0.0;
        for slot in &slots {
            let (fit, variant) = slot.street.expect("a street building");
            assert_eq!(slot.entry.slug(), "city_street_house");
            assert_eq!(
                (fit.depth, fit.storeys, fit.trade),
                (11, 5, false),
                "{fit:?}"
            );
            assert!(variant < STREET_VARIANTS);
            assert_eq!(slot.scale, 1.0);
            assert!(
                (slot.at.1 - (6.0 - 5.5)).abs() < 1e-3,
                "its front on the edge"
            );
            let f = Quat::from_rotation_y(slot.yaw) * Vec3::NEG_Z;
            assert!(f.distance(Vec3::Z) < 1e-5, "faces the street: {f}");
            frontage += fit.frontage_m();
        }
        assert!(
            frontage <= 60.0 + 1e-3 && frontage > 60.0 - 12.0,
            "{frontage} m"
        );
        let mut along: Vec<f32> = slots.iter().map(|slot| slot.at.0).collect();
        along.sort_by(f32::total_cmp);
        for (pair, w) in along.windows(2).zip(slots.windows(2)) {
            let half = |slot: &Slot| slot.street.expect("a street building").0.frontage_m() / 2.0;
            assert!(pair[1] - pair[0] >= half(&w[0]).min(half(&w[1])) * 2.0 - 1e-3);
        }
    }

    /// A building's storeys and use pick its street kind (#1598): a block
    /// of nine storeys takes long blocks - and a house in a tail too short
    /// for one - a workshop of three a trading low building, and a footprint
    /// narrower than its kind's shallowest step a copy drawn smaller.
    #[test]
    fn a_footprint_takes_its_kind_by_its_storeys_and_use() {
        let streets = city_streets();
        let slab = CoreBuilding {
            storeys: Some(9),
            peak_storeys: Some(9),
            ..building("slab", rectangle((0.0, 0.0), 100.0, 14.0, 0.0), (0.0, 1.0))
        };
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let slots = fill_street(&slab, &streets, &mut rng, 0.0);
        let blocks = slots
            .iter()
            .filter(|slot| slot.entry.slug() == "city_street_block")
            .count();
        assert!(blocks >= 2, "{blocks} blocks");
        for slot in &slots {
            let (fit, _) = slot.street.expect("a street building");
            let spec = slot.entry.street().expect("a street building");
            assert_eq!(fit.snapped(spec.kind), fit, "on its kind's steps");
            if spec.kind == StreetKind::Block {
                // Nine storeys: of eight and ten, as near, the lower.
                assert_eq!((fit.depth, fit.storeys), (14, 8), "{fit:?}");
            }
        }
        let works = CoreBuilding {
            usage: BuildingUse::Industrial,
            storeys: Some(3),
            peak_storeys: Some(3),
            ..building("works", rectangle((0.0, 0.0), 30.0, 20.0, 0.0), (0.0, 1.0))
        };
        for slot in fill_street(&works, &streets, &mut rng, 0.0) {
            let (fit, _) = slot.street.expect("a street building");
            assert_eq!(slot.entry.slug(), "city_street_low");
            assert_eq!(
                (fit.depth, fit.storeys, fit.trade),
                (17, 2, true),
                "{fit:?}"
            );
        }
        let wing = building("wing", rectangle((0.0, 0.0), 30.0, 7.0, 0.0), (0.0, 1.0));
        let slots = fill_street(&wing, &streets, &mut rng, 0.0);
        assert!(!slots.is_empty());
        for slot in slots {
            let (fit, _) = slot.street.expect("a street building");
            assert_eq!(fit.depth, 8);
            assert!(slot.scale < 1.0 && fit.depth_m() * slot.scale <= 7.0 + 1e-3);
        }
    }

    /// A house with a side wing (#1598) fronts its street, not its
    /// neighbour: its longest edge is the wing's, run back from the street,
    /// but its row runs along its street front, each copy's front on the
    /// street line.
    #[test]
    fn a_house_with_a_side_wing_fronts_its_street() {
        let streets = city_streets();
        // A front house 20 m along the street (to the south, +z) and 12 m
        // deep, and a wing 8 m wide run 30 m back from its west end.
        let outline = vec![
            (-10.0, 12.0),
            (10.0, 12.0),
            (10.0, 0.0),
            (-2.0, 0.0),
            (-2.0, -30.0),
            (-10.0, -30.0),
            (-10.0, 12.0),
        ];
        let winged = building("winged", outline, (0.0, 1.0));
        let longest = oriented_box(&winged.outline).expect("a box");
        assert!(
            longest.along.0.abs() < 1e-3,
            "the longest edge is the wing's"
        );
        let mut rng = ChaCha8Rng::seed_from_u64(5);
        let slots = fill_street(&winged, &streets, &mut rng, 0.0);
        assert!(!slots.is_empty());
        for slot in &slots {
            let (fit, _) = slot.street.expect("a street building");
            let f = Quat::from_rotation_y(slot.yaw) * Vec3::NEG_Z;
            assert!(f.distance(Vec3::Z) < 1e-5, "faces the street: {f}");
            let front = slot.at.1 + fit.depth_m() * slot.scale / 2.0;
            assert!((front - 12.0).abs() < 1e-3, "front at {front}");
        }
    }

    /// A narrow Berlin house - 10 m along its street, too short for the
    /// theme's narrowest street house - keeps its storeys, drawn smaller,
    /// rather than turn into a low building (#1598).
    #[test]
    fn a_narrow_house_keeps_its_storeys() {
        let streets = city_streets();
        let narrow = building("narrow", rectangle((0.0, 0.0), 10.0, 14.0, 0.0), (0.0, 1.0));
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        let slots = fill_street(&narrow, &streets, &mut rng, 0.0);
        assert_eq!(slots.len(), 1);
        let (fit, _) = slots[0].street.expect("a street building");
        assert_eq!(slots[0].entry.slug(), "city_street_house");
        assert_eq!(fit.storeys, 5);
        assert!(slots[0].scale < 1.0 && fit.frontage_m() * slots[0].scale <= 10.0 + 1e-3);
    }

    /// A block 80 m long and 64 deep takes two rows of street buildings,
    /// each with its front on its own side's edge, facing out.
    #[test]
    fn a_deep_footprint_fronts_both_its_streets_with_street_buildings() {
        let streets = city_streets();
        let block = building("block", rectangle((0.0, 0.0), 80.0, 64.0, 0.0), (0.0, 1.0));
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        let slots = fill_street(&block, &streets, &mut rng, 0.0);
        let (north, south): (Vec<&Slot>, Vec<&Slot>) =
            slots.iter().partition(|slot| slot.at.1 < 0.0);
        assert!(!north.is_empty() && !south.is_empty());
        for (rows, edge, facing) in [(&north, -32.0, Vec3::NEG_Z), (&south, 32.0, Vec3::Z)] {
            for slot in rows.iter() {
                let (fit, _) = slot.street.expect("a street building");
                let front = slot.at.1 + facing.z * fit.depth_m() * slot.scale / 2.0;
                assert!((front - edge).abs() < 1e-3, "front at {front}, not {edge}");
                let f = Quat::from_rotation_y(slot.yaw) * Vec3::NEG_Z;
                assert!(f.distance(facing) < 1e-5, "{f}");
            }
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
        let plan = plan(
            &level.buildings,
            &RoomScene::for_did(&did),
            &kept,
            &|_, _| 30.0,
        );
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
            let reach = planned.reach();
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
            // The modern city has its street buildings (#1598): every
            // secondary copy is one, built to its fit.
            if planned.entry.role() == StructureRole::Secondary {
                assert!(
                    matches!(planned.grow, Grow::Street { .. }),
                    "{} is no street building",
                    planned.key
                );
            }
        }
        let templates = plan
            .buildings
            .iter()
            .filter(|b| matches!(b.grow, Grow::Street { .. }))
            .count();
        assert!(
            (1..=MAX_STREET_TEMPLATES).contains(&templates),
            "{templates} street templates"
        );
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
        let again = super::plan(
            &level.buildings,
            &RoomScene::for_did(&did),
            &kept,
            &|_, _| 30.0,
        );
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
        let plan = plan(
            &[blocked, open],
            &RoomScene::for_did(&did),
            &Kept::of(None, (0.0, 0.0), None, None),
            &|_, _| 30.0,
        );
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
        let plan = plan(
            &level.buildings,
            &RoomScene::for_did(&did),
            &Kept::of(None, (0.0, 0.0), None, None),
            &|_, _| 30.0,
        );
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
            assert!(x.hypot(z) >= super::super::LANDING_CLEAR_M + planned.reach());
        }
    }
}
