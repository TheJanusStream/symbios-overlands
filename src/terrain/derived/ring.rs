//! The middle ring's buildings (#1587): the region's catalogue buildings on
//! the lots a geodata region's ring cuts round its walkable ground
//! ([`crate::terrain::geo::ring`]).
//!
//! They are the theme's own, as the road layer grows its lots
//! ([`super::fit`]). Each lot takes one:
//!
//! - The tallest of Berlin's buildings - [`LANDMARK_STANDING_M`] or more,
//!   no two within [`LANDMARK_SPACING_M`], at most [`MAX_RING_LANDMARKS`] -
//!   take the theme's landmarks: a church tower, a dome, a high-rise.
//! - Every other lot takes one of the theme's street buildings (#1598,
//!   [`super::streets`]) where it has them, shaped to the lot
//!   ([`ring_street`]): its kind and storeys by Berlin's height there, its
//!   frontage and depth the biggest that keeps to the lot.
//! - A theme without them takes a secondary building, a bigger one where
//!   Berlin's stands taller: the pool is ranked by size, and a lot aims at
//!   the place in it that its height has among the ring's lots, give or
//!   take one for variety.
//!
//! A kilometre of central Berlin is about 4,000 buildings. Within
//! [`NEAR_RING_M`] of the core's edge, while the copies stay inside
//! [`RING_ENTITY_BUDGET`] entities, a building is drawn near, its parts
//! merged per material; past that, as its voxel shell, at most
//! [`MAX_FAR_COPIES`] of them ([`super::bake`]). The ring is walked (P4.1,
//! #1596): each copy drawn, near or far, stands on its voxel shell, as the
//! walkable ground's buildings do.

use std::collections::HashMap;

use bevy::prelude::*;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use crate::catalogue::items::street::{StreetFit, StreetKind};
use crate::catalogue::{CatalogueEntry, StructureRole};
use crate::terrain::geo::ring::RingLot;
use crate::terrain::lots::{FOUNDATION_SINK_M, fitted_scale, scale_e4};

use super::fit::{
    RoomScene, SCALE_MIN, cache_key, footing, pick_landmark, pick_ranked, radius, sized_pool,
    street_cache_key,
};
use super::plan::{Grow, Plan, PlannedBuilding, PlannedCopy, Policy, Solid};
use super::streets::{self, STREET_VARIANTS, StreetKey, Streets, kind_of, smaller};
use super::{SourceId, SourceLayer};

/// How high over the ground Berlin's building on a lot must rise for the
/// lot to take a landmark (m): past the city's eaves, which stand at about
/// 22 m, and past the museums and offices of its centre.
pub(crate) const LANDMARK_STANDING_M: f32 = 40.0;

/// How far apart two landmarks of the ring stand at least (m): a cluster of
/// high-rises takes one.
pub(crate) const LANDMARK_SPACING_M: f32 = 200.0;

/// The most landmarks the ring draws.
pub(crate) const MAX_RING_LANDMARKS: usize = 8;

/// How far past the core's edge a lot's building is drawn near (m).
pub(crate) const NEAR_RING_M: f32 = 200.0;

/// The most entities the ring's near copies may be: a near copy past it is
/// drawn far.
pub(crate) const RING_ENTITY_BUDGET: u32 = 9_000;

/// The most far copies the ring draws, one entity each.
pub(crate) const MAX_FAR_COPIES: usize = 4_000;

/// The salt of the ring's own random stream.
const RING_STREAM_SALT: u64 = 0x5249_4E47_B011_D1E5;

/// What Berlin's height on a lot says of its storeys: that height is the
/// ninetieth percentile of the surface model over the lot's building, near
/// its ridge ([`RingLot::standing`]), so a crown of [`RING_CROWN_M`] comes
/// off it and each storey takes [`RING_STOREY_M`] of the rest. A pitched
/// Altbau's 24 m reads six storeys, a flat slab's 31 m eight, a cottage's
/// 9 m two.
const RING_CROWN_M: f32 = 2.5;

/// See [`RING_CROWN_M`] (m).
const RING_STOREY_M: f32 = 3.4;

/// One lot in this many takes a street building whose ground floor trades:
/// the ring's lots know no use.
const RING_TRADE_ONE_IN: u32 = 3;

/// The most street templates the ring grows (#1598,
/// [`streets::template`]).
pub(crate) const MAX_RING_STREET_TEMPLATES: usize = 64;

/// The ring's plan: its drawing policy.
const RING_POLICY: Policy = Policy {
    near_entities: RING_ENTITY_BUDGET,
    far_copies: MAX_FAR_COPIES,
    cut: false,
};

/// Draw the ring's buildings on `lots` (see the module docs), for `room`,
/// keeping clear of what the record keeps (`kept`) - its landing and its
/// absolute placements, as the walkable ground's plans do, the ring being
/// walked too (P4.1, #1596) - standing on `ground`, the ground's height at
/// a world point.
pub(crate) fn draw_ring(
    lots: &[RingLot],
    room: &RoomScene,
    kept: &super::core::Kept,
    ground: &dyn Fn(f32, f32) -> f32,
) -> Plan {
    let (theme, character) = room.theme();
    let landmarks = sized_pool(theme, StructureRole::Landmark, character);
    let mut secondaries = sized_pool(theme, StructureRole::Secondary, character);
    let streets = Streets::of(&secondaries);
    // A theme without its street buildings draws the rest.
    secondaries.retain(|entry| entry.street().is_none());

    let landmark_lots = landmark_lots(lots);
    // Each other lot's place by height among the other lots, in (0, 1).
    let mut others: Vec<usize> = (0..lots.len()).filter(|&i| !landmark_lots[i]).collect();
    others.sort_by(|&a, &b| {
        lots[a]
            .standing
            .total_cmp(&lots[b].standing)
            .then(a.cmp(&b))
    });
    let mut rank = vec![0.0; lots.len()];
    for (place, &i) in others.iter().enumerate() {
        rank[i] = (place as f32 + 0.5) / others.len() as f32;
    }

    let seed = room.seed ^ RING_STREAM_SALT;
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut plan = Plan {
        label: "ring",
        policy: RING_POLICY,
        buildings: Vec::new(),
        copies: Vec::new(),
        did: room.did.clone(),
        character,
        seed,
    };
    let mut by_key: HashMap<(&'static str, i64), usize> = HashMap::new();
    let mut by_street: HashMap<StreetKey, usize> = HashMap::new();
    let mut street_keys: Vec<StreetKey> = Vec::new();
    for (i, lot) in lots.iter().enumerate() {
        let landmark = if landmark_lots[i] {
            pick_landmark(&landmarks, lot.room, &mut rng)
        } else {
            None
        };
        let pick = match (landmark, &streets) {
            (Some((entry, scale)), _) => Some((entry, scale, None)),
            (None, Some(streets)) => ring_street(streets, lot, &mut rng)
                .map(|(entry, fit, scale, variant)| (entry, scale, Some((fit, variant)))),
            (None, None) => pick_ranked(&secondaries, lot.room, rank[i], &mut rng)
                .map(|(entry, scale)| (entry, scale, None)),
        };
        let Some((entry, scale, street)) = pick else {
            continue;
        };
        let reach = match street {
            Some((fit, _)) => fit.reach_m() * scale,
            None => radius(entry) * scale,
        };
        // Picked whether it stands or not, so a lot given up for the
        // record leaves the picks of the lots after it as they were.
        if !kept.clear(lot.x, lot.z, reach) {
            continue;
        }
        let building = match street {
            None => {
                let key = (entry.slug(), scale_e4(scale));
                *by_key.entry(key).or_insert_with(|| {
                    plan.buildings.push(PlannedBuilding::new(
                        entry,
                        scale,
                        Grow::Built,
                        cache_key("ring", key.0, key.1),
                    ));
                    plan.buildings.len() - 1
                })
            }
            Some((fit, variant)) => {
                let want = (entry.slug(), fit, variant, scale_e4(scale));
                let Some(key) = streets::template(&street_keys, MAX_RING_STREET_TEMPLATES, want)
                else {
                    continue;
                };
                *by_street.entry(key).or_insert_with(|| {
                    street_keys.push(key);
                    let (_, fit, variant, _) = key;
                    plan.buildings.push(PlannedBuilding::new(
                        entry,
                        scale,
                        Grow::Street { fit, variant },
                        street_cache_key("ring", key),
                    ));
                    plan.buildings.len() - 1
                })
            }
        };
        let reach = plan.buildings[building].reach();
        let y = footing(lot.x, lot.z, reach, ground) - FOUNDATION_SINK_M;
        plan.copies.push(PlannedCopy {
            building,
            pose: Transform::from_xyz(lot.x, y, lot.z)
                .with_rotation(Quat::from_rotation_y(lot.yaw)),
            near: lot.beyond <= NEAR_RING_M,
            source: SourceId::new(SourceLayer::RingLot, format!("{:.0},{:.0}", lot.x, lot.z)),
            height: None,
            solid: Solid::Shell,
        });
    }
    plan
}

/// The street building `lot` takes (#1598), and its seed variant: its kind
/// and storeys by Berlin's height on the lot ([`RING_CROWN_M`]),
/// trading on one lot in [`RING_TRADE_ONE_IN`], at the biggest of its
/// kind's fits that keeps to the lot's room - the next kind down's where
/// none of its own does - or else a low building's smallest drawn smaller,
/// down to [`SCALE_MIN`]. `None` where even that outgrows the lot.
fn ring_street(
    streets: &Streets,
    lot: &RingLot,
    rng: &mut ChaCha8Rng,
) -> Option<(&'static dyn CatalogueEntry, StreetFit, f32, u8)> {
    let storeys = ((lot.standing - RING_CROWN_M) / RING_STOREY_M)
        .round()
        .clamp(1.0, 30.0) as u8;
    let trade = rng.next_u32().is_multiple_of(RING_TRADE_ONE_IN);
    let variant = (rng.next_u32() % u32::from(STREET_VARIANTS)) as u8;
    let mut kind = Some(kind_of(storeys, false));
    while let Some(k) = kind {
        let biggest = k
            .frontages()
            .iter()
            .flat_map(|&f| k.depths().iter().map(move |&d| (f, d)))
            .map(|(f, d)| StreetFit::new(f, d, storeys, trade).snapped(k))
            .filter(|fit| fit.reach_m() <= lot.room)
            .max_by_key(|fit| (u32::from(fit.frontage) * u32::from(fit.depth), fit.depth));
        if let Some(fit) = biggest {
            return Some((streets.entry(k), fit, 1.0, variant));
        }
        kind = smaller(k);
    }
    let low = StreetKind::Low;
    let fit = StreetFit::new(low.frontages()[0], low.depths()[0], storeys, trade).snapped(low);
    let scale = fitted_scale(lot.room / fit.reach_m(), SCALE_MIN, 1.0);
    (scale * fit.reach_m() <= lot.room).then_some((streets.entry(low), fit, scale, variant))
}

/// Which lots take a landmark: the tallest, tallest first, at least
/// [`LANDMARK_STANDING_M`] high and [`LANDMARK_SPACING_M`] from every
/// landmark taken before, at most [`MAX_RING_LANDMARKS`].
fn landmark_lots(lots: &[RingLot]) -> Vec<bool> {
    let mut tall: Vec<usize> = (0..lots.len())
        .filter(|&i| lots[i].standing >= LANDMARK_STANDING_M)
        .collect();
    tall.sort_by(|&a, &b| {
        lots[b]
            .standing
            .total_cmp(&lots[a].standing)
            .then(a.cmp(&b))
    });
    let mut taken: Vec<usize> = Vec::new();
    for i in tall {
        if taken.len() == MAX_RING_LANDMARKS {
            break;
        }
        let apart = taken.iter().all(|&j| {
            let (dx, dz) = (lots[i].x - lots[j].x, lots[i].z - lots[j].z);
            dx * dx + dz * dz >= LANDMARK_SPACING_M * LANDMARK_SPACING_M
        });
        if apart {
            taken.push(i);
        }
    }
    let mut landmark = vec![false; lots.len()];
    for i in taken {
        landmark[i] = true;
    }
    landmark
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeded_defaults::ThemeArchetype;
    use crate::terrain::derived::core::Kept;

    use super::super::fit::{SCALE_MIN, did_of};

    /// A lot at `(x, z)`, `beyond` past the core's edge, with the whole of its
    /// 30 m to itself, Berlin's building on it `standing` high.
    fn lot(x: f32, z: f32, standing: f32, beyond: f32) -> RingLot {
        RingLot {
            x,
            z,
            yaw: 0.0,
            room: 15.0,
            standing,
            beyond,
        }
    }

    #[test]
    fn every_lot_takes_a_building_that_fits_it_and_the_tallest_take_landmarks() {
        let did = did_of(ThemeArchetype::ModernCity);
        // Two rows of lots; four stand tall, two of them within a landmark's
        // spacing of each other.
        let mut lots: Vec<RingLot> = (0..40)
            .map(|i| {
                let x = (i % 20) as f32 * 30.0;
                lot(x, (i / 20) as f32 * 30.0, 10.0 + (i % 7) as f32, x)
            })
            .collect();
        for (i, standing) in [(0, 80.0), (1, 70.0), (15, 60.0), (39, 45.0)] {
            lots[i].standing = standing;
        }
        lots[7].room = 8.0;
        let ground = |x: f32, z: f32| 30.0 + 0.01 * x - 0.02 * z;
        let plan = draw_ring(&lots, &RoomScene::for_did(&did), &Kept::nothing(), &ground);
        assert_eq!(plan.copies.len(), lots.len(), "a building on every lot");
        let landmarks: Vec<usize> = plan
            .copies
            .iter()
            .enumerate()
            .filter(|(_, c)| plan.buildings[c.building].entry.role() == StructureRole::Landmark)
            .map(|(i, _)| i)
            .collect();
        // Lot 1 is 30 m from lot 0, and lot 39 is 30 m from lot 15's row
        // mate but 360 m from lot 0.
        assert_eq!(landmarks, vec![0, 15], "the tallest, spaced");
        for (copy, lot) in plan.copies.iter().zip(&lots) {
            let building = &plan.buildings[copy.building];
            let reach = building.reach();
            assert!(
                reach <= lot.room,
                "{} at {} on a lot of room {}",
                building.entry.slug(),
                building.scale,
                lot.room
            );
            assert!((SCALE_MIN..=1.0).contains(&building.scale));
            assert_eq!(
                (copy.pose.translation.x, copy.pose.translation.z),
                (lot.x, lot.z)
            );
            // The footing is the lowest ground under the building, sunk.
            let under = footing(lot.x, lot.z, reach, &ground);
            assert_eq!(copy.pose.translation.y, under - FOUNDATION_SINK_M);
            assert!(under <= ground(lot.x, lot.z));
            assert_eq!(copy.near, lot.beyond <= NEAR_RING_M);
            // The ring is walked, on its shells (P4.1); a lot is named by
            // its place.
            assert_eq!((copy.solid, copy.height), (Solid::Shell, None));
            assert_eq!(copy.source.layer, SourceLayer::RingLot);
        }
        // The modern city's lots take its street buildings (#1598).
        let streets = plan
            .copies
            .iter()
            .filter(|c| matches!(plan.buildings[c.building].grow, Grow::Street { .. }))
            .count();
        assert_eq!(streets, lots.len() - 2, "every lot but the landmarks'");
        // One building per entry, fit and scale, shared by its copies.
        assert!(plan.buildings.len() < plan.copies.len());
        let mut keys: Vec<&str> = plan.buildings.iter().map(|b| b.key.as_str()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), plan.buildings.len());
        // And the same room draws the same ring.
        let again = draw_ring(&lots, &RoomScene::for_did(&did), &Kept::nothing(), &ground);
        let picks = |p: &Plan| -> Vec<(String, PlannedCopy)> {
            p.copies
                .iter()
                .map(|c| (p.buildings[c.building].key.clone(), c.clone()))
                .collect()
        };
        assert_eq!(picks(&plan), picks(&again));
    }

    /// A taller lot takes a taller building: its street building's walls
    /// (#1598) stand higher, its storeys Berlin's there.
    #[test]
    fn a_taller_lot_takes_a_taller_building() {
        let did = did_of(ThemeArchetype::ModernCity);
        let lots: Vec<RingLot> = (0..300)
            .map(|i| {
                let x = (i % 20) as f32 * 30.0;
                lot(x, (i / 20) as f32 * 30.0, 3.0 + 0.1 * i as f32, x)
            })
            .collect();
        let plan = draw_ring(
            &lots,
            &RoomScene::for_did(&did),
            &Kept::nothing(),
            &|_, _| 0.0,
        );
        let walls = |range: std::ops::Range<usize>| {
            let n = range.len() as f32;
            plan.copies[range]
                .iter()
                .map(|c| {
                    let building = &plan.buildings[c.building];
                    let Grow::Street { fit, .. } = building.grow else {
                        panic!("{} is no street building", building.key);
                    };
                    let spec = building.entry.street().expect("a street building");
                    spec.walls_m(fit.storeys) * building.scale
                })
                .sum::<f32>()
                / n
        };
        assert!(
            walls(200..300) > walls(0..100) + 10.0,
            "{} > {}",
            walls(200..300),
            walls(0..100)
        );
    }

    /// P4.1 (#1596): the ring is walked, so it keeps clear of what the
    /// record keeps - a landing out there, an owner's placement - as the
    /// walkable ground's plans do, and the lots after one given up keep
    /// their picks.
    #[test]
    fn the_ring_keeps_clear_of_what_the_record_keeps() {
        let did = super::super::fit::did_of(crate::seeded_defaults::ThemeArchetype::ModernCity);
        let lots: Vec<RingLot> = (0..6)
            .map(|i| RingLot {
                x: 600.0 + 40.0 * i as f32,
                z: 0.0,
                yaw: 0.0,
                room: 15.0,
                standing: 12.0,
                beyond: 100.0,
            })
            .collect();
        let room = RoomScene::for_did(&did);
        let all = draw_ring(&lots, &room, &Kept::nothing(), &|_, _| 30.0);
        assert_eq!(all.copies.len(), 6);
        // A landing on the third lot.
        let kept = Kept::of(None, (680.0, 0.0), None, None);
        let around = draw_ring(&lots, &room, &kept, &|_, _| 30.0);
        let at = |plan: &Plan| -> Vec<f32> {
            plan.copies.iter().map(|c| c.pose.translation.x).collect()
        };
        assert!(!at(&around).contains(&680.0), "{:?}", at(&around));
        // The others stand as they did, the same buildings on them.
        let key = |plan: &Plan, x: f32| {
            plan.copies
                .iter()
                .find(|c| c.pose.translation.x == x)
                .map(|c| plan.buildings[c.building].key.clone())
        };
        for x in [600.0, 760.0, 800.0] {
            assert_eq!(key(&around, x), key(&all, x), "lot at {x}");
        }
    }
}
