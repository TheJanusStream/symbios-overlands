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
//! - Every other lot takes a secondary building, a bigger one where
//!   Berlin's stands taller: the pool is ranked by size, and a lot aims at
//!   the place in it that its height has among the ring's lots, give or
//!   take one for variety.
//!
//! A kilometre of central Berlin is about 4,000 buildings. Within
//! [`NEAR_RING_M`] of the walls, while the copies stay inside
//! [`RING_ENTITY_BUDGET`] entities, a building is drawn near, its parts
//! merged per material; past that, as its voxel shell, at most
//! [`MAX_FAR_COPIES`] of them ([`super::bake`]). No one walks to the ring:
//! its copies stand on no collider.

use std::collections::HashMap;

use bevy::prelude::*;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::catalogue::StructureRole;
use crate::seeded_defaults::fnv1a_64;
use crate::terrain::geo::ring::RingLot;
use crate::terrain::lots::{FOUNDATION_SINK_M, scale_e4};

use super::fit::{cache_key, footing, pick_landmark, pick_ranked, radius, room_theme, sized_pool};
use super::plan::{Grow, Plan, PlannedBuilding, PlannedCopy, Policy, Solid};
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

/// How far past the walls a lot's building is drawn near (m).
pub(crate) const NEAR_RING_M: f32 = 200.0;

/// The most entities the ring's near copies may be: a near copy past it is
/// drawn far.
pub(crate) const RING_ENTITY_BUDGET: u32 = 9_000;

/// The most far copies the ring draws, one entity each.
pub(crate) const MAX_FAR_COPIES: usize = 4_000;

/// The salt of the ring's own random stream.
const RING_STREAM_SALT: u64 = 0x5249_4E47_B011_D1E5;

/// The ring's plan: its drawing policy.
const RING_POLICY: Policy = Policy {
    near_entities: RING_ENTITY_BUDGET,
    far_copies: MAX_FAR_COPIES,
    cut: false,
};

/// Draw the ring's buildings on `lots` (see the module docs), for the room
/// `did`, standing on `ground` - the ground's height at a world point.
pub(crate) fn draw_ring(lots: &[RingLot], did: &str, ground: &dyn Fn(f32, f32) -> f32) -> Plan {
    let (theme, character) = room_theme(did);
    let landmarks = sized_pool(theme, StructureRole::Landmark, character);
    let secondaries = sized_pool(theme, StructureRole::Secondary, character);

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

    let seed = fnv1a_64(did) ^ RING_STREAM_SALT;
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut plan = Plan {
        label: "ring",
        policy: RING_POLICY,
        buildings: Vec::new(),
        copies: Vec::new(),
        did: did.to_owned(),
        character,
        seed,
    };
    let mut by_key: HashMap<(&'static str, i64), usize> = HashMap::new();
    for (i, lot) in lots.iter().enumerate() {
        let landmark = if landmark_lots[i] {
            pick_landmark(&landmarks, lot.room, &mut rng)
        } else {
            None
        };
        let Some((entry, scale)) =
            landmark.or_else(|| pick_ranked(&secondaries, lot.room, rank[i], &mut rng))
        else {
            continue;
        };
        let key = (entry.slug(), scale_e4(scale));
        let building = *by_key.entry(key).or_insert_with(|| {
            plan.buildings.push(PlannedBuilding::new(
                entry,
                scale,
                Grow::Built,
                cache_key("ring", key.0, key.1),
            ));
            plan.buildings.len() - 1
        });
        let y = footing(lot.x, lot.z, radius(entry) * scale, ground) - FOUNDATION_SINK_M;
        plan.copies.push(PlannedCopy {
            building,
            pose: Transform::from_xyz(lot.x, y, lot.z)
                .with_rotation(Quat::from_rotation_y(lot.yaw)),
            near: lot.beyond <= NEAR_RING_M,
            source: SourceId::new(SourceLayer::RingLot, format!("{:.0},{:.0}", lot.x, lot.z)),
            height: None,
            solid: Solid::None,
        });
    }
    plan
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

    use super::super::fit::{SCALE_MIN, did_of};

    /// A lot at `(x, z)`, `beyond` past the walls, with the whole of its
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
        let plan = draw_ring(&lots, &did, &ground);
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
            let reach = radius(building.entry) * building.scale;
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
            // No one walks to the ring; a lot is named by its place.
            assert_eq!((copy.solid, copy.height), (Solid::None, None));
            assert_eq!(copy.source.layer, SourceLayer::RingLot);
        }
        // One building per entry and scale, shared by its copies.
        assert!(plan.buildings.len() < plan.copies.len());
        let mut keys: Vec<&str> = plan.buildings.iter().map(|b| b.key.as_str()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), plan.buildings.len());
        // And the same room draws the same ring.
        let again = draw_ring(&lots, &did, &ground);
        let picks = |p: &Plan| -> Vec<(String, PlannedCopy)> {
            p.copies
                .iter()
                .map(|c| (p.buildings[c.building].key.clone(), c.clone()))
                .collect()
        };
        assert_eq!(picks(&plan), picks(&again));
    }

    #[test]
    fn a_taller_lot_takes_a_bigger_building() {
        let did = did_of(ThemeArchetype::ModernCity);
        let lots: Vec<RingLot> = (0..300)
            .map(|i| {
                let x = (i % 20) as f32 * 30.0;
                lot(x, (i / 20) as f32 * 30.0, 3.0 + 0.1 * i as f32, x)
            })
            .collect();
        let plan = draw_ring(&lots, &did, &|_, _| 0.0);
        let reach = |range: std::ops::Range<usize>| {
            let n = range.len() as f32;
            plan.copies[range]
                .iter()
                .map(|c| radius(plan.buildings[c.building].entry))
                .sum::<f32>()
                / n
        };
        assert!(
            reach(200..300) > reach(0..100),
            "{} > {}",
            reach(200..300),
            reach(0..100)
        );
    }
}
