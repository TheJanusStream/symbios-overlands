//! Room water-level lookup and the dry-land relocation walk used by
//! water-avoiding placements.

use bevy::prelude::*;

use crate::pds::RoomRecord;
use crate::water::bodies::RoomWater;

/// The room's sea level: the lowest Water child under any Terrain-rooted
/// generator (the canonical homeworld layout puts the room's water plane
/// there, and a higher one is a lake, #1760), or `None` for dry rooms.
/// Water world Y is the child's translation because the terrain anchor
/// sits at the origin unsnapped.
pub(crate) fn room_water_level(record: &RoomRecord) -> Option<f32> {
    crate::water::bodies::sea_level(&crate::water::bodies::terrain_layers(record))
}

/// The room's water as drawn on `heightmap` (#1594): its sea and its lakes
/// (#1760), each reader asking the level at its own point
/// ([`RoomWater::level_at`]). Where a geodata region's core has water
/// (#1586), Berlin's own bodies are the room's water in place of its sea,
/// each at its own level (#1724, [`geo_water`]): the record still decides
/// whether there is water at all - a record with no Water generator draws
/// none, and Berlin's beds lie dry - while Berlin decides where it lies.
/// Every reader of the water that has a heightmap asks this, so the planes,
/// the damp ground, the dry-land walks, the scatter bands and the streets
/// all agree on it.
pub(crate) fn drawn_water(
    record: &RoomRecord,
    heightmap: Option<&crate::terrain::FinishedHeightMap>,
) -> RoomWater {
    RoomWater::of(record, heightmap)
}

/// Berlin's water under `heightmap`, where a geodata region's core has
/// any: its bodies, each at its own level (#1724), which the region draws
/// in place of the record's sea.
pub(crate) fn geo_water(
    heightmap: Option<&crate::terrain::FinishedHeightMap>,
) -> Option<&[crate::water::bodies::WaterBody]> {
    let water = heightmap?.ground()?.water();
    (!water.is_empty()).then_some(water)
}

/// Probe spacing along the bearing of [`relocate_above_water`] (m).
const DRY_STEP: f32 = 6.0;
/// Its probe budget: 30 outward + 30 inward = ±180 m of shoreline hunt.
const DRY_MAX_PROBES: u32 = 60;
/// Required terrain clearance over the water line (m) - enough that a
/// structure's plinth course stays dry.
const FREEBOARD: f32 = 0.75;

/// Slide a water-avoiding anchor along its bearing through the origin -
/// alternating outward / inward in `DRY_STEP`-metre increments - to
/// the first probe where the terrain rises above the room's water
/// there plus a freeboard margin. Bearing-aligned steps keep a
/// spawn-facing yaw valid, and the walk is a pure function of the
/// shared heightmap, so every peer relocates the anchor identically.
/// Gives up after `DRY_MAX_PROBES` probes and leaves the anchor in
/// place (a flooded landmark beats a missing one).
pub(super) fn relocate_above_water(
    hm: &bevy_symbios_ground::HeightMap,
    extent: f32,
    half: f32,
    translation: &mut Vec3,
    water: &RoomWater,
    clearance: f32,
) {
    let dry = |x: f32, z: f32| is_dry(hm, extent, half, (x, z), water, clearance);
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

/// Whether `(x, z)` stands dry over `water`: its centre and (for a
/// non-zero `clearance`) a ring of eight points at that radius all clear
/// the water there by [`FREEBOARD`] - a wide building can't pass on a dry
/// anchor while its far wing floods.
fn is_dry(
    hm: &bevy_symbios_ground::HeightMap,
    extent: f32,
    half: f32,
    (x, z): (f32, f32),
    water: &RoomWater,
    clearance: f32,
) -> bool {
    let sample = |x: f32, z: f32| {
        hm.get_height_at((x + half).clamp(0.0, extent), (z + half).clamp(0.0, extent))
    };
    if water
        .level_at(x, z)
        .is_some_and(|level| sample(x, z) < level + FREEBOARD)
    {
        return false;
    }
    if clearance <= 0.0 {
        return true;
    }
    (0..8).all(|i| {
        let a = i as f32 * std::f32::consts::TAU / 8.0;
        // libm (#1132): same accept/reject shape as the slope walk - a
        // site is dry only if all eight probes clear the freeboard.
        let (px, pz) = (x + libm::sinf(a) * clearance, z + libm::cosf(a) * clearance);
        water
            .level_at(px, pz)
            .is_none_or(|level| sample(px, pz) >= level + FREEBOARD)
    })
}

/// How far round a landing must stand open and dry (m): a body.
const LANDING_CLEARANCE_M: f32 = 1.0;

/// The step of the ring search for open dry ground (m).
const SHORE_STEP_M: f32 = 4.0;

/// Walk a water-avoiding anchor on Berlin's ground (#1589) to where it
/// stands open and dry ([`is_open_and_dry`]): off the water, as
/// [`relocate_above_water`] walks it, and off the street space the land use
/// leaves - a seeded gate is no building to stand across a road. Along its
/// bearing through the origin first, as that walk goes; where that finds
/// nothing - a square centred far out on a lake, or a bearing that runs
/// down a street - to the nearest open dry ground of the walkable ground.
/// Left where it is only where there is none.
pub(super) fn relocate_to_open_ground(
    hm: &bevy_symbios_ground::HeightMap,
    (extent, half): (f32, f32),
    translation: &mut Vec3,
    water: &RoomWater,
    berlin: &crate::terrain::geo::GeoGround,
    clearance: f32,
) {
    let (x0, z0) = (translation.x, translation.z);
    let stands =
        |x: f32, z: f32| is_open_and_dry(hm, (extent, half), berlin, (x, z), water, clearance);
    if stands(x0, z0) {
        return;
    }
    let r0 = x0.hypot(z0);
    if r0 >= 1e-3 {
        let (dx, dz) = (x0 / r0, z0 / r0);
        for i in 1..=DRY_MAX_PROBES {
            let sign = if i % 2 == 1 { 1.0 } else { -1.0 };
            let r = r0 + i.div_ceil(2) as f32 * DRY_STEP * sign;
            if !(4.0..=half).contains(&r) {
                continue;
            }
            if stands(dx * r, dz * r) {
                translation.x = dx * r;
                translation.z = dz * r;
                return;
            }
        }
    }
    // Rings out from the anchor, each probed every SHORE_STEP_M of its arc
    // from due east: the first that stands is the nearest, to within a step.
    let reach = half - clearance.max(LANDING_CLEARANCE_M);
    let rings = (2.0 * half / SHORE_STEP_M).ceil() as u32;
    for ring in 1..=rings {
        let r = ring as f32 * SHORE_STEP_M;
        let probes = ((std::f32::consts::TAU * r / SHORE_STEP_M).ceil() as u32).max(8);
        for i in 0..probes {
            let a = i as f32 * std::f32::consts::TAU / probes as f32;
            let (x, z) = (x0 + r * libm::cosf(a), z0 + r * libm::sinf(a));
            if x.abs() <= reach && z.abs() <= reach && stands(x, z) {
                translation.x = x;
                translation.z = z;
                return;
            }
        }
    }
}

/// Whether `(x, z)` stands open and dry on Berlin's ground, at its centre
/// and at eight points `clearance` round it: the land use names what lies
/// there - no street space - and it is no water, which a lake on the
/// plateau keeps as its flat surface; and, under the water there, it is no
/// lower than the settle keeps dry ground above it
/// ([`geodata::water::FREEBOARD_M`]), the shore's slope being water's.
fn is_open_and_dry(
    hm: &bevy_symbios_ground::HeightMap,
    (extent, half): (f32, f32),
    berlin: &crate::terrain::geo::GeoGround,
    (x, z): (f32, f32),
    water: &RoomWater,
    clearance: f32,
) -> bool {
    let one = |x: f32, z: f32| {
        let named = matches!(
            berlin.cover_at(x, z),
            Some(cover) if cover != geodata::berlin::LandUse::Water
        );
        named
            && water.level_at(x, z).is_none_or(|water_y| {
                let height =
                    hm.get_height_at((x + half).clamp(0.0, extent), (z + half).clamp(0.0, extent));
                // A centimetre's slack: the settled ground is the margin
                // itself, and its interpolation may round below it.
                height >= water_y + geodata::water::FREEBOARD_M - 0.01
            })
    };
    one(x, z)
        && (clearance <= 0.0
            || (0..8).all(|i| {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                one(x + libm::sinf(a) * clearance, z + libm::cosf(a) * clearance)
            }))
}

/// Where a room's landing at `(x, z)` sets a body down on `heightmap`
/// (#1589). On Berlin's ground it is walked to open dry ground by the rule
/// that walks the room's gateway ([`relocate_to_open_ground`]), so a seeded
/// landing in front of its gate comes with it off a lake or a street. On a
/// world's own ground a landing that avoids water - a seeded viewpoint's
/// (#1777) - walks off water and steep ground as its gate does
/// ([`super::pad::relocate_snapped_anchor`]), off what the probe that sited
/// it missed; any other stays where it is, its owner's spot.
pub(crate) fn landing_ashore(
    record: &RoomRecord,
    heightmap: &crate::terrain::FinishedHeightMap,
    (x, z): (f32, f32),
) -> (f32, f32) {
    let Some(berlin) = heightmap.ground() else {
        if !record
            .default_landing
            .is_some_and(|landing| landing.avoid_water)
        {
            return (x, z);
        }
        let water = drawn_water(record, Some(heightmap));
        let mut at = Vec3::new(x, 0.0, z);
        super::pad::relocate_snapped_anchor(
            &super::pad::AnchorGround::new(heightmap, &water),
            &mut at,
            LANDING_CLEARANCE_M,
        );
        return (at.x, at.z);
    };
    let hm = &heightmap.0;
    let extent = (hm.width().saturating_sub(1)) as f32 * hm.scale();
    let mut at = Vec3::new(x, 0.0, z);
    relocate_to_open_ground(
        hm,
        (extent, extent * 0.5),
        &mut at,
        &drawn_water(record, Some(heightmap)),
        berlin,
        LANDING_CLEARANCE_M,
    );
    (at.x, at.z)
}

/// The room's default landing as it sets a body down on `heightmap`: its
/// pose, its spot walked ashore ([`landing_ashore`]) - and turned to face
/// what the derive faced it at, where that stands, once a walk has moved
/// the two apart: a Berlin square's landing its gate, where the landing was
/// walked; a seeded viewpoint's its landmark (#1777), where either was - the
/// probe that sited both misses ground the compile walks them off - while it
/// still [looks at](looks_at) the landmark: one its owner turned keeps the
/// facing they gave it. Any other landing keeps its facing.
pub(crate) fn landing_on(
    record: &RoomRecord,
    heightmap: &crate::terrain::FinishedHeightMap,
) -> Option<crate::pds::DefaultLanding> {
    let given = record.default_landing?;
    let mut landing = given;
    let recorded = landing.pos.0;
    let (x, z) = landing_ashore(record, heightmap, (recorded[0], recorded[1]));
    landing.pos.0 = [x, z];
    let walked = (x, z) != (recorded[0], recorded[1]);
    let water = drawn_water(record, Some(heightmap));
    let ground = super::pad::AnchorGround::new(heightmap, &water);
    let stands = match heightmap.ground() {
        Some(_) if walked => record
            .placements
            .iter()
            .find_map(|placement| match placement {
                crate::pds::Placement::Absolute {
                    generator_ref,
                    transform,
                    snap_to_terrain: true,
                    avoid_water,
                    avoid_water_clearance,
                    ..
                } if generator_ref == GATEWAY => Some(super::pad::snapped_absolute_anchor(
                    &ground,
                    transform,
                    *avoid_water,
                    avoid_water_clearance.0,
                )),
                _ => None,
            }),
        None if landing.avoid_water => seeded_landmark(record, &ground)
            .filter(|&(placed, stands)| {
                // The walks move a spot or leave it bit for bit.
                looks_at(&given, placed) && (walked || [stands.x, stands.z] != placed)
            })
            .map(|(_, stands)| stands),
        _ => None,
    };
    if let Some(stands) = stands {
        let (dx, dz) = (stands.x - x, stands.z - z);
        if dx * dx + dz * dz > 1.0 {
            // The spawn's convention: a pose facing `(dx, dz)` turns by
            // atan2(-dx, -dz).
            landing.yaw_deg.0 = libm::atan2f(-dx, -dz).to_degrees();
        }
    }
    Some(landing)
}

/// Where `record`'s seeded landmark stands as the compile stands it on
/// `ground`, beside where its record placed it, both world XZ (#1777): its
/// [`LANDMARK`](super::scatter::LANDMARK) placement among the seeded
/// structures - snapped, avoiding water - as the scatters keep off it.
pub(super) fn seeded_landmark(
    record: &RoomRecord,
    ground: &super::pad::AnchorGround<'_>,
) -> Option<([f32; 2], Vec3)> {
    record
        .placements
        .iter()
        .find_map(|placement| match placement {
            crate::pds::Placement::Absolute {
                generator_ref,
                transform,
                snap_to_terrain: true,
                avoid_water: true,
                avoid_water_clearance,
                ..
            } if generator_ref == super::scatter::LANDMARK => Some((
                [transform.translation.0[0], transform.translation.0[2]],
                super::pad::snapped_absolute_anchor(
                    ground,
                    transform,
                    true,
                    avoid_water_clearance.0,
                ),
            )),
            _ => None,
        })
}

/// How far a landing's facing may stand off the bearing to its landmark and
/// still look at it (degrees, #1777): wide of the record's own rounding,
/// narrow of any turn an owner gives it.
const LOOKS_AT_DEG: f32 = 1.0;

/// Whether `landing` looks at the point `at` (#1777): its facing within
/// [`LOOKS_AT_DEG`] of the bearing to it. A seeded viewpoint's landing looks
/// at its landmark; one its owner turned does not, and keeps its own facing
/// and its trees.
pub(super) fn looks_at(landing: &crate::pds::DefaultLanding, at: [f32; 2]) -> bool {
    let [x, z] = landing.pos.0;
    let (dx, dz) = (at[0] - x, at[1] - z);
    if dx * dx + dz * dz < 1.0 {
        return false;
    }
    let off = (landing.yaw_deg.0 - libm::atan2f(-dx, -dz).to_degrees()).rem_euclid(360.0);
    off.min(360.0 - off) <= LOOKS_AT_DEG
}

/// The generator the seeded room's gate is placed by.
pub(super) const GATEWAY: &str = "social_gateway";

#[cfg(test)]
mod water_avoidance_tests {
    use super::*;
    use crate::pds::{GeneratorKind, Placement};

    /// #1586, #1724: the record says whether there is water, Berlin where
    /// it lies: its own bodies, each at its own level, in place of the
    /// record's sea.
    #[test]
    fn berlins_water_is_its_own_bodies_where_the_region_has_any() {
        use crate::terrain::FinishedHeightMap;
        use crate::terrain::geo::GeoGround;
        let record = RoomRecord::default_for_did("did:test:water");
        let own = RoomWater::sea_alone(Some(
            room_water_level(&record).expect("seeded rooms carry water"),
        ));
        let map =
            |ground| FinishedHeightMap(bevy_symbios_ground::HeightMap::new(3, 3, 1.0), ground);
        let cover = vec![Some(geodata::berlin::LandUse::Water); 9];
        let wet = GeoGround::from_cover(
            3,
            1.0,
            cover.clone(),
            GeoGround::water_of_cover(
                &bevy_symbios_ground::HeightMap::new(3, 3, 1.0),
                &cover,
                30.5,
            ),
        );
        let dry = GeoGround::from_cover(3, 1.0, vec![None; 9], Vec::new());
        assert_eq!(drawn_water(&record, None), own);
        assert_eq!(drawn_water(&record, Some(&map(None))), own);
        let berlin = drawn_water(&record, Some(&map(Some(wet.clone()))));
        assert_eq!(berlin.sea, None, "no sea spans Berlin");
        assert_eq!(berlin.bodies.as_slice(), wet.water(), "Berlin's own bodies");
        assert_eq!(berlin.level_at(0.0, 0.0), Some(30.5));
        assert_eq!(
            drawn_water(&record, Some(&map(Some(dry)))),
            own,
            "a dry core keeps the record's sea, under Berlin's ground"
        );
        // No Water generator: no water, Berlin's or not.
        let mut without = record.clone();
        for generator in without.generators.values_mut() {
            generator
                .children
                .retain(|c| !matches!(c.kind, GeneratorKind::Water { .. }));
        }
        assert_eq!(
            drawn_water(&without, Some(&map(Some(wet)))),
            RoomWater::default()
        );
    }

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

    /// An 800 m Berlin core, 401 x 401 cells 2 m apart, its water at 30 m:
    /// a lake filling all but a strip of land along the east edge
    /// (x > 300) and a jetty north of the middle (|x| < 6, z < -30), and a
    /// street of the land use's street space running north-south down the
    /// strip (320 < x < 330).
    fn lake_and_street() -> (
        crate::terrain::FinishedHeightMap,
        bevy_symbios_ground::HeightMap,
    ) {
        use crate::terrain::geo::GeoGround;
        use geodata::berlin::LandUse;
        let size = 401usize;
        let mut hm = bevy_symbios_ground::HeightMap::new(size, size, 2.0);
        let mut cover = Vec::with_capacity(size * size);
        for row in 0..size {
            for col in 0..size {
                let (x, z) = (col as f32 * 2.0 - 400.0, row as f32 * 2.0 - 400.0);
                let land = x > 300.0 || (x.abs() < 6.0 && z < -30.0);
                hm.set(col, row, if land { 34.0 } else { 27.0 });
                cover.push(match (land, (320.0..330.0).contains(&x)) {
                    (true, true) => None,
                    (true, false) => Some(LandUse::Park),
                    (false, _) => Some(LandUse::Water),
                });
            }
        }
        let water = GeoGround::water_of_cover(&hm, &cover, 30.0);
        let ground = GeoGround::from_cover(size as u32, 2.0, cover, water);
        (
            crate::terrain::FinishedHeightMap(hm.clone(), Some(ground)),
            hm,
        )
    }

    /// #1589: a landing on Berlin's water or street comes to open dry
    /// ground - along its bearing where the bearing reaches some within the
    /// walk, as the gate does, else to the nearest - and stays put on open
    /// dry ground, and on a procedural room's ground whatever lies under it.
    #[test]
    fn a_landing_on_berlins_water_comes_ashore() {
        let (berlin, hm) = lake_and_street();
        let record = RoomRecord::default_for_did("did:test:ashore");
        // On the jetty's bearing: walked out along it, onto the jetty.
        let (x, z) = landing_ashore(&record, &berlin, (0.0, -10.0));
        assert!(x == 0.0 && z <= -30.0 && z > -40.0, "({x}, {z})");
        // Far out, off any bearing's shore: the nearest open dry ground, the
        // east strip short of its street, to within a probe step.
        let (x, z) = landing_ashore(&record, &berlin, (100.0, 200.0));
        assert!(
            (300.0..312.0).contains(&x) && (z - 200.0).abs() < 40.0,
            "({x}, {z})"
        );
        // On the street: off it, onto the strip's open ground.
        let (x, z) = landing_ashore(&record, &berlin, (325.0, 0.0));
        assert!(!(319.0..331.0).contains(&x) && z.abs() < 10.0, "({x}, {z})");
        // Open dry ground is left alone.
        assert_eq!(landing_ashore(&record, &berlin, (350.0, 0.0)), (350.0, 0.0));
        // A procedural room's landing is its own, wet or not.
        let procedural = crate::terrain::FinishedHeightMap(hm, None);
        assert_eq!(
            landing_ashore(&record, &procedural, (100.0, 200.0)),
            (100.0, 200.0)
        );
        // The record's landing, walked: with no gate to face, its facing
        // kept.
        let mut landed = record.clone();
        landed.placements.retain(|placement| {
            !matches!(placement, crate::pds::Placement::Absolute { generator_ref, .. }
                if generator_ref == GATEWAY)
        });
        landed.default_landing = Some(crate::pds::DefaultLanding {
            pos: crate::pds::Fp2([100.0, 200.0]),
            yaw_deg: crate::pds::Fp(30.0),
            ..Default::default()
        });
        let walked = landing_on(&landed, &berlin).expect("a landing");
        assert!(
            walked.pos.0[0] > 300.0 && walked.yaw_deg.0 == 30.0,
            "no gate to face"
        );

        // With its gate standing on the strip, a walked landing turns to
        // face it; an unwalked one keeps its own turn.
        landed.placements.push(crate::pds::Placement::Absolute {
            generator_ref: GATEWAY.to_owned(),
            transform: crate::pds::TransformData {
                translation: crate::pds::Fp3([350.0, -0.35, 150.0]),
                ..Default::default()
            },
            snap_to_terrain: true,
            avoid_water: true,
            avoid_water_clearance: crate::pds::Fp(3.0),
            seed: None,
            params: Default::default(),
        });
        let walked = landing_on(&landed, &berlin).expect("a landing");
        let at = bevy::math::Vec3::new(walked.pos.0[0], 0.0, walked.pos.0[1]);
        let to_gate = (bevy::math::Vec3::new(350.0, 0.0, 150.0) - at).normalize();
        let facing = bevy::math::Quat::from_rotation_y(walked.yaw_deg.0.to_radians())
            * bevy::math::Vec3::NEG_Z;
        assert!(facing.dot(to_gate) > 0.99, "{facing} vs {to_gate}");
        landed.default_landing = Some(crate::pds::DefaultLanding {
            pos: crate::pds::Fp2([350.0, 0.0]),
            yaw_deg: crate::pds::Fp(30.0),
            ..Default::default()
        });
        let kept = landing_on(&landed, &berlin).expect("a landing");
        assert_eq!((kept.pos.0, kept.yaw_deg.0), ([350.0, 0.0], 30.0));
    }

    /// #1777: a seeded viewpoint's landing - one that avoids water - walks
    /// off a world's own water the probe that sited it missed, and turns to
    /// its landmark where the compile stands it whenever either of the two
    /// was walked; where neither was, it keeps the facing it was given; one
    /// its owner turned off the landmark keeps their facing, walked or not;
    /// and a landing that does not avoid water is never walked.
    #[test]
    fn a_viewpoint_landing_walks_ashore_and_faces_its_landmark_where_it_stands() {
        let record = (0u64..)
            .map(|seed| {
                crate::seeded_defaults::room::build::build_room_with_source(
                    seed,
                    "did:test:viewpoint",
                    None,
                )
            })
            .find(|record| room_water_level(record).is_some())
            .expect("a seeded world with water");
        let sea = room_water_level(&record).expect("its sea");
        // 400 m of dry land 2 m a cell, its ponds - each 12 m round - under
        // the sea.
        let ground = |ponds: &[[f32; 2]]| {
            let mut hm = bevy_symbios_ground::HeightMap::new(201, 201, 2.0);
            for row in 0..201 {
                for col in 0..201 {
                    let at = [col as f32 * 2.0 - 200.0, row as f32 * 2.0 - 200.0];
                    let wet = ponds
                        .iter()
                        .any(|p| (at[0] - p[0]).hypot(at[1] - p[1]) < 12.0);
                    hm.set(col, row, if wet { sea - 3.0 } else { sea + 5.0 });
                }
            }
            crate::terrain::FinishedHeightMap(hm, None)
        };
        let (landmark, landing) = ([40.0, 0.0], [0.0, 100.0]);
        let mut seeded = record.clone();
        for placement in &mut seeded.placements {
            if let Placement::Absolute {
                generator_ref,
                transform,
                avoid_water_clearance,
                ..
            } = placement
                && generator_ref == super::super::scatter::LANDMARK
            {
                transform.translation.0 = [landmark[0], 0.0, landmark[1]];
                transform.scale.0 = [1.0; 3];
                *avoid_water_clearance = crate::pds::Fp(3.0);
            }
        }
        // Looking at the landmark, as the derive faces it, the record's yaw
        // in [0, 360).
        let given = libm::atan2f(-(landmark[0] - landing[0]), -(landmark[1] - landing[1]))
            .to_degrees()
            .rem_euclid(360.0);
        seeded.default_landing = Some(crate::pds::DefaultLanding {
            pos: crate::pds::Fp2(landing),
            yaw_deg: crate::pds::Fp(given),
            pitch_deg: Some(crate::pds::Fp(12.0)),
            avoid_water: true,
            ..Default::default()
        });
        let faces = |landed: crate::pds::DefaultLanding, toward: [f32; 2]| {
            let [x, z] = landed.pos.0;
            let to = Vec3::new(toward[0] - x, 0.0, toward[1] - z).normalize();
            (Quat::from_rotation_y(landed.yaw_deg.0.to_radians()) * Vec3::NEG_Z).dot(to) > 0.999
        };

        // Neither walked: the facing it was given, its pitch kept.
        let kept = landing_on(&seeded, &ground(&[])).expect("a landing");
        assert_eq!((kept.pos.0, kept.yaw_deg.0), (landing, given));
        assert_eq!(kept.pitch_deg, Some(crate::pds::Fp(12.0)));

        // The landing in a pond: ashore along its bearing, facing the
        // landmark from there.
        let walked = landing_on(&seeded, &ground(&[landing])).expect("a landing");
        assert!(walked.pos.0[0] == 0.0 && (walked.pos.0[1] - 100.0).abs() >= 12.0);
        assert!(faces(walked, landmark), "{walked:?}");
        assert_eq!(walked.pitch_deg, Some(crate::pds::Fp(12.0)));

        // The landmark in a pond: the landing stays, facing it where it
        // stands.
        let pond = ground(&[landmark]);
        let water = drawn_water(&seeded, Some(&pond));
        let stands = seeded
            .placements
            .iter()
            .find_map(|placement| match placement {
                Placement::Absolute {
                    generator_ref,
                    transform,
                    ..
                } if generator_ref == super::super::scatter::LANDMARK => {
                    Some(super::super::pad::snapped_absolute_anchor(
                        &super::super::pad::AnchorGround::new(&pond, &water),
                        transform,
                        true,
                        3.0,
                    ))
                }
                _ => None,
            })
            .expect("its landmark");
        assert!((stands.x - landmark[0]).abs() >= 12.0, "{stands}");
        let turned = landing_on(&seeded, &pond).expect("a landing");
        assert_eq!(turned.pos.0, landing);
        assert!(faces(turned, [stands.x, stands.z]), "{turned:?}");

        // Turned by its owner off the landmark: their facing, walked or not.
        let mut own = seeded.clone();
        if let Some(l) = own.default_landing.as_mut() {
            l.yaw_deg = crate::pds::Fp(10.0);
        }
        for ponds in [&[landmark][..], &[landing], &[landing, landmark]] {
            let kept = landing_on(&own, &ground(ponds)).expect("a landing");
            assert_eq!(kept.yaw_deg.0, 10.0, "{ponds:?}");
        }

        // An owner's landing stays where they set it, wet or not.
        let mut owned = seeded.clone();
        if let Some(l) = owned.default_landing.as_mut() {
            l.avoid_water = false;
        }
        let set = landing_on(&owned, &ground(&[landing, landmark])).expect("a landing");
        assert_eq!((set.pos.0, set.yaw_deg.0), (landing, given));
    }

    /// #1589: a seeded structure on Berlin's ground walks off its streets
    /// as well as its water, its whole footprint clear; on procedural
    /// ground the walk is the water walk it always was.
    #[test]
    fn a_seeded_gate_on_berlins_street_steps_off_it() {
        use crate::world_builder::compile::pad::AnchorGround;
        // The water a Berlin region draws: its own bodies, no sea (#1724).
        let berlins = |map: &crate::terrain::FinishedHeightMap| RoomWater {
            sea: None,
            bodies: map.ground().map_or_else(Vec::new, |g| g.water().to_vec()),
        };
        let (berlin, hm) = lake_and_street();
        let berlin_water = berlins(&berlin);
        let gate = crate::pds::TransformData {
            translation: crate::pds::Fp3([325.0, -0.35, 0.0]),
            ..Default::default()
        };
        let ground = AnchorGround::new(&berlin, &berlin_water);
        let stands = crate::world_builder::snapped_absolute_anchor(&ground, &gate, true, 3.0);
        assert!(
            stands.x > 333.0 || (300.0..317.0).contains(&stands.x),
            "a 3 m footprint clear of the street: {stands}"
        );
        // An editor placement, which never walks, stays on the street.
        let hand = crate::world_builder::snapped_absolute_anchor(&ground, &gate, false, 0.0);
        assert_eq!((hand.x, hand.z), (325.0, 0.0));
        // Water on the plateau no body is drawn over - left as the terrain
        // draws it, too narrow to settle: water by its land use, though no
        // level is there to clear.
        let (mut high, _) = lake_and_street();
        let mut cover: Vec<Option<geodata::berlin::LandUse>> = (0..401 * 401)
            .map(|i| {
                let x = (i % 401) as f32 * 2.0 - 400.0;
                Some(if x > 340.0 {
                    geodata::berlin::LandUse::Water
                } else {
                    geodata::berlin::LandUse::Park
                })
            })
            .collect();
        cover.truncate(401 * 401);
        let ground_high = crate::terrain::geo::GeoGround::from_cover(401, 2.0, cover, Vec::new());
        high.1 = Some(ground_high);
        for row in 0..401 {
            for col in 0..401 {
                high.0.set(col, row, 34.0);
            }
        }
        let pond = crate::pds::TransformData {
            translation: crate::pds::Fp3([370.0, -0.35, 0.0]),
            ..Default::default()
        };
        let high_water = berlins(&high);
        let ground = AnchorGround::new(&high, &high_water);
        let ashore = crate::world_builder::snapped_absolute_anchor(&ground, &pond, true, 3.0);
        assert!(ashore.x <= 337.0, "off the plateau lake: {ashore}");

        // Procedural ground: dry there, so it stays.
        let procedural = crate::terrain::FinishedHeightMap(hm, None);
        let sea_30 = RoomWater::sea_alone(Some(30.0));
        let own = AnchorGround::new(&procedural, &sea_30);
        let stays = crate::world_builder::snapped_absolute_anchor(&own, &gate, true, 3.0);
        assert_eq!((stays.x, stays.z), (325.0, 0.0));
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
        let sea = RoomWater::sea_alone(Some(2.0));

        // Submerged anchor at (10, 0), bearing +X: must slide outward
        // past the shoreline without leaving the bearing line.
        let mut t = Vec3::new(10.0, 0.0, 0.0);
        relocate_above_water(&hm, extent, half, &mut t, &sea, 0.0);
        assert!(t.x > 20.0, "anchor should cross the shoreline: {t:?}");
        assert_eq!(t.z, 0.0, "walk must stay on the bearing line");

        // Already-dry anchors stay exactly put.
        let mut dry = Vec3::new(40.0, 0.0, 0.0);
        relocate_above_water(&hm, extent, half, &mut dry, &sea, 0.0);
        assert_eq!(dry.x, 40.0);

        // A fully-drowned bearing gives up and leaves the anchor in
        // place rather than teleporting it somewhere arbitrary.
        let mut hopeless = Vec3::new(0.0, 0.0, -30.0);
        relocate_above_water(&hm, extent, half, &mut hopeless, &sea, 0.0);
        assert_eq!((hopeless.x, hopeless.z), (0.0, -30.0));

        // Clearance ring: an anchor just past the shoreline (x = 22) is
        // dry at its centre but a 10 m footprint ring dips back into
        // the sea - the walk must push it further inland until the
        // whole disc clears.
        let mut wide = Vec3::new(22.0, 0.0, 0.0);
        relocate_above_water(&hm, extent, half, &mut wide, &sea, 10.0);
        assert!(
            wide.x > 30.0,
            "ring-sampled anchor must move until the footprint clears: {wide:?}"
        );
    }
}
