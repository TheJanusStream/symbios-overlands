//! Stone circle — a ring of eight tapered monoliths with two lintel
//! capstones and a low central altar carrying a faintly glowing orb.
//! The wilderness landmark: no walls, no roof, just megaliths that
//! read at any scale and fit every biome from tundra to volcanic.
//!
//! Frame convention mirrors the lighthouse: the root is the altar
//! block whose base sits at the generator origin; the monolith ring is
//! positioned relative to it. The placement's terrain snap puts the
//! altar on the ground — on slopes the outer stones float or sink a
//! little, which suits a ruin.

use crate::catalogue::items::util::{tile, tiles_per_metre};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, SovereignMaterialSettings, SovereignRockConfig,
    SovereignTextureConfig,
};
use crate::seeded_defaults::ThemeArchetype;

use crate::catalogue::items::util::{
    cuboid_tapered, glow, id_quat, prim, quat_x, quat_y, solid, sphere,
};

pub struct StoneCircle;

impl CatalogueEntry for StoneCircle {
    fn slug(&self) -> &'static str {
        "stone_circle"
    }
    fn name(&self) -> &'static str {
        "Stone Circle"
    }
    fn description(&self) -> &'static str {
        "Ring of eight tapered monoliths with lintel capstones and a glowing altar orb."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }

    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AncientClassical, ThemeArchetype::Nordic]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 10.0,
            min_spawn_dist: 40.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn megalith_mat() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.48, 0.47, 0.45]),
        roughness: Fp(0.95),
        uv_scale: tiles_per_metre(tile::ROCK),
        texture: SovereignTextureConfig::Rock(SovereignRockConfig {
            scale: Fp64(6.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn build_tree() -> Generator {
    let orb_glow = [0.55, 0.85, 1.0];

    // Altar block — the root; base at the generator origin.
    let altar_h = 0.9;
    let mut root = prim(
        solid(cuboid_tapered([2.4, altar_h, 1.6], 0.10, megalith_mat())),
        [0.0, altar_h * 0.5, 0.0],
        id_quat(),
    );
    let rel = |ground_y: f32| ground_y - altar_h * 0.5;

    root.children.push(prim(
        sphere(0.32, 3, glow(orb_glow, 4.0)),
        [0.0, altar_h * 0.5 + 0.34, 0.0],
        id_quat(),
    ));

    // Monolith ring: eight massive tapered sarsens on a tight ring, three
    // adjacent pairs raised to carry trilithon lintels (a horseshoe of
    // capped uprights) with two plainer stones closing the back. Chunky
    // proportions on a close ring read as an imposing henge instead of the
    // scatter of thin slabs the old wide, slender ring produced.
    let ring_r = 5.0;
    let stone_count = 8usize;
    let step = std::f32::consts::TAU / stone_count as f32;
    let pairs = [0usize, 2, 4]; // trilithon upright pairs
    let is_pair = |i: usize| pairs.iter().any(|&p| i == p || i == p + 1);
    let tall = 4.0;
    let stone_height = |i: usize| if is_pair(i) { tall } else { 3.0 };
    // Each upright extends `root_depth` below grade — the henge's per-stone
    // "foundation" — so slope-snapped rings never show daylight under a
    // stone. Trilithon uprights take their *lintel's* yaw (the chord
    // mid-angle) so their rotated top corners tuck under the lintel ends
    // instead of jutting out as "ears"; the plain stones face the centre.
    let pair_yaw = |i: usize| {
        for &p in &pairs {
            if i == p || i == p + 1 {
                return (p as f32 + 0.5) * step;
            }
        }
        i as f32 * step
    };
    let root_depth = 1.4;
    for i in 0..stone_count {
        let angle = i as f32 * step;
        let height = stone_height(i);
        let full = height + root_depth;
        root.children.push(prim(
            solid(cuboid_tapered([1.7, full, 1.15], 0.12, megalith_mat())),
            [
                angle.sin() * ring_r,
                rel(height - full * 0.5),
                angle.cos() * ring_r,
            ],
            quat_y(pair_yaw(i)),
        ));
    }

    // Lintel capstones bridging each trilithon pair (adjacent uprights 45°
    // apart) — the trilithon silhouette. Each sits at its pair's chord
    // midpoint, yawed to run along the chord, overhanging both uprights and
    // sunk a touch so its underside overlaps the upright tops rather than
    // resting coplanar with them.
    let chord = 2.0 * ring_r * (step * 0.5).sin();
    let mid_r = ring_r * (step * 0.5).cos();
    for &p in &pairs {
        let a = p as f32 * step;
        let b = (p + 1) as f32 * step;
        let mid = (a + b) * 0.5;
        root.children.push(prim(
            solid(cuboid_tapered(
                [chord + 1.3, 0.75, 1.15],
                0.06,
                megalith_mat(),
            )),
            [
                mid.sin() * mid_r,
                rel(tall + 0.75 * 0.5 - 0.08),
                mid.cos() * mid_r,
            ],
            quat_y(mid),
        ));
    }

    // Ruin flavour: two toppled slabs and one leaning stump just outside the
    // ring, half-sunk so they read as casualties of the same centuries that
    // weathered the standing stones.
    let fallen = [
        // (size, position angle, radius, yaw, extra sink)
        ([1.6, 0.6, 2.9], 1.85_f32, 6.4_f32, 0.7_f32, 0.18_f32),
        ([1.4, 0.5, 2.3], 5.6, 6.6, 2.1, 0.14),
    ];
    for (size, angle, radius, yaw, half_h) in fallen {
        root.children.push(prim(
            solid(cuboid_tapered(size, 0.05, megalith_mat())),
            [angle.sin() * radius, rel(half_h), angle.cos() * radius],
            quat_y(yaw),
        ));
    }
    // Leaning stump: a shortened upright caught mid-fall, tilted and buried
    // past its base.
    root.children.push(prim(
        solid(cuboid_tapered([1.5, 2.6, 1.0], 0.16, megalith_mat())),
        [6.5_f32.sin() * 6.2, rel(0.9), 6.5_f32.cos() * 6.2],
        quat_x(0.42),
    ));

    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&StoneCircle.build(""), "stone_circle");
    }

    #[test]
    fn ring_has_eight_stones_plus_lintels_orb_and_ruins() {
        let g = StoneCircle.build("");
        // 1 orb + 8 stones + 3 lintels + 3 fallen/leaning = 15
        // children under the altar root.
        assert_eq!(g.children.len(), 15);
    }
}
