//! Room water-level lookup and the dry-land relocation walk used by
//! water-avoiding placements.

use bevy::prelude::*;

use crate::pds::{GeneratorKind, RoomRecord};

/// The room's sea level: the highest Water child under any
/// Terrain-rooted generator (the canonical homeworld layout puts the
/// room's water plane there), or `None` for dry rooms. Water world Y
/// is the child's translation because the terrain anchor sits at the
/// origin unsnapped.
pub(crate) fn room_water_level(record: &RoomRecord) -> Option<f32> {
    record
        .generators
        .values()
        .filter(|g| matches!(g.kind, GeneratorKind::Terrain(_)))
        .flat_map(|g| g.children.iter())
        .filter(|c| matches!(c.kind, GeneratorKind::Water { .. }))
        .map(|c| c.transform.translation.0[1])
        .fold(None, |acc: Option<f32>, y| {
            Some(acc.map_or(y, |a| a.max(y)))
        })
}

/// Slide a water-avoiding anchor along its bearing through the origin
/// — alternating outward / inward in `DRY_STEP`-metre increments — to
/// the first probe where the terrain rises above the room's water
/// line plus a freeboard margin. Bearing-aligned steps keep a
/// spawn-facing yaw valid, and the walk is a pure function of the
/// shared heightmap, so every peer relocates the anchor identically.
/// Gives up after `DRY_MAX_PROBES` probes and leaves the anchor in
/// place (a flooded landmark beats a missing one).
pub(super) fn relocate_above_water(
    hm: &bevy_symbios_ground::HeightMap,
    extent: f32,
    half: f32,
    translation: &mut Vec3,
    water_y: f32,
    clearance: f32,
) {
    /// Probe spacing along the bearing (m).
    const DRY_STEP: f32 = 6.0;
    /// Probe budget: 30 outward + 30 inward = ±180 m of shoreline hunt.
    const DRY_MAX_PROBES: u32 = 60;
    /// Required terrain clearance over the water line (m) — enough
    /// that a structure's plinth course stays dry.
    const FREEBOARD: f32 = 0.75;

    let sample = |x: f32, z: f32| {
        hm.get_height_at((x + half).clamp(0.0, extent), (z + half).clamp(0.0, extent))
    };
    // A candidate is dry when its centre and (for non-zero clearance) a
    // ring of eight points at the clearance radius all clear the water
    // line — a wide building can't pass on a dry anchor while its far
    // wing floods.
    let dry = |x: f32, z: f32| {
        if sample(x, z) < water_y + FREEBOARD {
            return false;
        }
        if clearance <= 0.0 {
            return true;
        }
        (0..8).all(|i| {
            let a = i as f32 * std::f32::consts::TAU / 8.0;
            sample(x + a.sin() * clearance, z + a.cos() * clearance) >= water_y + FREEBOARD
        })
    };
    let (x0, z0) = (translation.x, translation.z);
    if dry(x0, z0) {
        return;
    }
    let r0 = (x0 * x0 + z0 * z0).sqrt();
    if r0 < 1e-3 {
        // Anchored on the origin: no bearing to walk.
        return;
    }
    let (dx, dz) = (x0 / r0, z0 / r0);
    for i in 1..=DRY_MAX_PROBES {
        // Alternate +1, -1, +2, -2, … steps along the bearing.
        let sign = if i % 2 == 1 { 1.0 } else { -1.0 };
        let k = i.div_ceil(2) as f32 * DRY_STEP * sign;
        let r = r0 + k;
        // Inward probes stop short of the spawn square; outward ones
        // stay inside the heightmap.
        if !(4.0..=half).contains(&r) {
            continue;
        }
        let (x, z) = (dx * r, dz * r);
        if dry(x, z) {
            translation.x = x;
            translation.z = z;
            return;
        }
    }
}

#[cfg(test)]
mod water_avoidance_tests {
    use super::*;
    use crate::pds::Placement;

    #[test]
    fn room_water_level_reads_seeded_record() {
        let record = RoomRecord::default_for_did("did:test:water");
        let level = room_water_level(&record).expect("seeded rooms always carry water");
        assert!(
            level >= 0.0,
            "seeded water sits at or above the terrain base"
        );
    }

    #[test]
    fn landmark_placement_opts_into_water_avoidance() {
        let record = RoomRecord::default_for_did("did:test:water");
        let landmark_avoids = record.placements.iter().any(|p| {
            matches!(
                p,
                Placement::Absolute {
                    generator_ref,
                    avoid_water: true,
                    snap_to_terrain: true,
                    ..
                } if generator_ref == "landmark"
            )
        });
        assert!(landmark_avoids, "seeded landmark must carry avoid_water");
    }

    #[test]
    fn dry_land_walk_slides_along_bearing_to_shore() {
        // Synthetic 129×129 heightmap, scale 1.0 → world X/Z in
        // [-64, 64]. Dry plateau (y = 5) where world X > 20, seabed
        // (y = 0) elsewhere; water line at y = 2.
        let mut hm = bevy_symbios_ground::HeightMap::new(129, 129, 1.0);
        for z in 0..129 {
            for x in 0..129 {
                let world_x = x as f32 - 64.0;
                hm.set(x, z, if world_x > 20.0 { 5.0 } else { 0.0 });
            }
        }
        let (extent, half) = (128.0, 64.0);

        // Submerged anchor at (10, 0), bearing +X: must slide outward
        // past the shoreline without leaving the bearing line.
        let mut t = Vec3::new(10.0, 0.0, 0.0);
        relocate_above_water(&hm, extent, half, &mut t, 2.0, 0.0);
        assert!(t.x > 20.0, "anchor should cross the shoreline: {t:?}");
        assert_eq!(t.z, 0.0, "walk must stay on the bearing line");

        // Already-dry anchors stay exactly put.
        let mut dry = Vec3::new(40.0, 0.0, 0.0);
        relocate_above_water(&hm, extent, half, &mut dry, 2.0, 0.0);
        assert_eq!(dry.x, 40.0);

        // A fully-drowned bearing gives up and leaves the anchor in
        // place rather than teleporting it somewhere arbitrary.
        let mut hopeless = Vec3::new(0.0, 0.0, -30.0);
        relocate_above_water(&hm, extent, half, &mut hopeless, 2.0, 0.0);
        assert_eq!((hopeless.x, hopeless.z), (0.0, -30.0));

        // Clearance ring: an anchor just past the shoreline (x = 22) is
        // dry at its centre but a 10 m footprint ring dips back into
        // the sea — the walk must push it further inland until the
        // whole disc clears.
        let mut wide = Vec3::new(22.0, 0.0, 0.0);
        relocate_above_water(&hm, extent, half, &mut wide, 2.0, 10.0);
        assert!(
            wide.x > 30.0,
            "ring-sampled anchor must move until the footprint clears: {wide:?}"
        );
    }
}
