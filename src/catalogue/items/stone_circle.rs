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

use crate::catalogue::{CatalogueCategory, CatalogueEntry};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, SovereignMaterialSettings, SovereignRockConfig,
    SovereignTextureConfig,
};

use super::util::{cuboid_tapered, glow, id_quat, prim, quat_x, quat_y, solid, sphere};

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
    fn category(&self) -> CatalogueCategory {
        CatalogueCategory::Buildings
    }
    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn megalith_mat() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.48, 0.47, 0.45]),
        roughness: Fp(0.95),
        uv_scale: Fp(1.2),
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
        solid(cuboid_tapered([2.2, altar_h, 1.4], 0.10, megalith_mat())),
        [0.0, altar_h * 0.5, 0.0],
        id_quat(),
    );
    let rel = |ground_y: f32| ground_y - altar_h * 0.5;

    root.children.push(prim(
        sphere(0.30, 3, glow(orb_glow, 4.0)),
        [0.0, altar_h * 0.5 + 0.32, 0.0],
        id_quat(),
    ));

    // Monolith ring: eight tapered slabs facing the centre. The two
    // trilithon pairs (stones 0+1 and 4+5) stand tall to carry their
    // lintels; the rest alternate slightly shorter so the ring reads
    // weathered rather than machine-stamped.
    let ring_r = 6.0;
    let stone_count = 8usize;
    let step = std::f32::consts::TAU / stone_count as f32;
    let tall = 3.4;
    let stone_height = |i: usize| match i {
        0 | 1 | 4 | 5 => tall,
        _ if i.is_multiple_of(2) => 3.0,
        _ => 2.6,
    };
    // Each upright extends `root_depth` below grade — the henge's
    // per-stone "foundation" — so slope-snapped rings never show
    // daylight under a stone. The visible top stays at `height`.
    //
    // Trilithon uprights (pairs 0+1 and 4+5) take their *lintel's* yaw
    // — the chord mid-angle — instead of facing the ring centre:
    // individually-yawed pair stones leave their rotated top corners
    // jutting out from under the lintel ends as little "ears".
    let root_depth = 1.4;
    let pair_yaw = |i: usize| match i {
        0 | 1 => 0.5 * step,
        4 | 5 => 4.5 * step,
        _ => i as f32 * step,
    };
    for i in 0..stone_count {
        let angle = i as f32 * step;
        let height = stone_height(i);
        let full = height + root_depth;
        root.children.push(prim(
            solid(cuboid_tapered([1.15, full, 0.7], 0.22, megalith_mat())),
            [
                angle.sin() * ring_r,
                rel(height - full * 0.5),
                angle.cos() * ring_r,
            ],
            quat_y(pair_yaw(i)),
        ));
    }

    // Ruin flavour: two toppled slabs and one leaning stump scattered
    // outside the ring, half-sunk so they read as casualties of the
    // same centuries that weathered the standing stones.
    let fallen = [
        // (size, position angle, radius, yaw, extra sink)
        ([1.15, 0.55, 2.9], 1.85_f32, 7.6_f32, 0.7_f32, 0.18_f32),
        ([1.0, 0.45, 2.2], 3.55, 8.1, 2.1, 0.14),
    ];
    for (size, angle, radius, yaw, half_h) in fallen {
        root.children.push(prim(
            solid(cuboid_tapered(size, 0.05, megalith_mat())),
            [angle.sin() * radius, rel(half_h), angle.cos() * radius],
            quat_y(yaw),
        ));
    }
    // Leaning stump: a shortened upright caught mid-fall, tilted and
    // buried past its base.
    root.children.push(prim(
        solid(cuboid_tapered([1.05, 2.2, 0.65], 0.20, megalith_mat())),
        [2.1_f32.sin() * 7.0, rel(0.75), 2.1_f32.cos() * 7.0],
        quat_x(0.45),
    ));

    // Lintel capstones bridging *adjacent* tall stones (45° apart) —
    // the trilithon silhouette. The lintel sits at the pair's chord
    // midpoint, yawed to run along the chord, and overhangs each
    // upright by half a stone width.
    let chord = 2.0 * ring_r * (step * 0.5).sin(); // ≈ 4.59 m
    let mid_r = ring_r * (step * 0.5).cos(); // chord midpoint radius
    for pair_start in [0usize, 4] {
        let a = pair_start as f32 * step;
        let b = (pair_start + 1) as f32 * step;
        let mid = (a + b) * 0.5;
        root.children.push(prim(
            solid(cuboid_tapered(
                [chord + 1.2, 0.65, 0.85],
                0.08,
                megalith_mat(),
            )),
            [mid.sin() * mid_r, rel(tall + 0.65 * 0.5), mid.cos() * mid_r],
            // Yawing by `mid` turns local +X into the tangent at the
            // mid-angle — i.e. along the chord between the two
            // uprights. (The old `mid + π/2` pointed the beam down the
            // radius, slicing across the ring interior.)
            quat_y(mid),
        ));
    }

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
        // 1 orb + 8 stones + 3 fallen/leaning + 2 lintels = 14
        // children under the altar root.
        assert_eq!(g.children.len(), 14);
    }
}
