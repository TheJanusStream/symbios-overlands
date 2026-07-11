//! Torii Gateway — the Feudal-Japan bespoke social gateway (#757), the
//! themed replacement for the neutral placeholder arch. A vermilion torii
//! read as a walk-through: two lacquered pillars on stone footings carry a
//! pierced nuki tie beam and the upswept Myōjin crown (shimaki + kasagi),
//! with a shrine plaque on the hero face and warm paper lanterns lighting
//! the threshold.
//!
//! The only functional element is the single [`GeneratorKind::Gateway`]
//! zone centred in the opening — walking into it opens the destination
//! picker of the room owner's mutual follows. Everything else frames that
//! zone so it reads as a sacred threshold you pass through.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{LACQUER_RED, LANTERN_GLOW, STONE_GREY, TIMBER_DARK, lacquer, stone, timber};

pub struct FeudalJapanGateway;

impl CatalogueEntry for FeudalJapanGateway {
    fn slug(&self) -> &'static str {
        "feudal_japan_gateway"
    }
    fn name(&self) -> &'static str {
        "Torii Gateway"
    }
    fn description(&self) -> &'static str {
        "Vermilion torii threshold whose lantern-lit span opens the way to the owner's mutual follows."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::FeudalJapan]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.5,
            min_spawn_dist: 8.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let span = 1.7_f32; // half the pillar spacing — inner faces bracket a ~2.6 m opening
    let pillar_r = 0.3_f32;
    let pillar_h = 4.6_f32;
    let base_top = 0.4_f32; // top of the stone footing strip the pillars stand on
    let nuki_y = base_top + pillar_h * 0.72; // pierced tie beam height
    let top = base_top + pillar_h; // pillar tops / crown springing line

    // Slightly darkened lacquer for the crown, so the kasagi reads apart from
    // the pillars.
    let crown_mat = || {
        lacquer([
            LACQUER_RED[0] * 0.85,
            LACQUER_RED[1] * 0.85,
            LACQUER_RED[2] * 0.85,
        ])
    };

    // Stone footing strip — the FLAT-BASE ROOT (never tilt a root: every
    // child would spin with it).
    let mut prims = vec![prim(
        solid(cuboid_tapered(
            [2.0 * span + 1.4, 0.4, 1.2],
            0.0,
            stone(STONE_GREY),
        )),
        [0.0, 0.2, 0.0],
        id_quat(),
    )];

    // Two lacquered pillars on stone footings, tapering up.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.8, 0.5, 0.8], 0.0, stone(STONE_GREY))),
            [sx * span, base_top + 0.05, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cylinder_tapered(
                pillar_r,
                pillar_h,
                12,
                0.12,
                lacquer(LACQUER_RED),
            )),
            [sx * span, base_top + pillar_h * 0.5, 0.0],
            id_quat(),
        ));
    }

    // Nuki tie beam pierced through the pillars.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.0 * span + 0.8, 0.42, 0.55],
            0.0,
            lacquer(LACQUER_RED),
        )),
        [0.0, nuki_y, 0.0],
        id_quat(),
    ));

    // Shimaki (lower crown beam) hugging the pillar tops.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.0 * span + 1.2, 0.5, 0.7],
            0.0,
            lacquer(LACQUER_RED),
        )),
        [0.0, top + 0.25, 0.0],
        id_quat(),
    ));
    // Central kasagi crown beam.
    let kasagi_w = 2.0 * span + 0.6;
    let kasagi_y = top + 0.78;
    prims.push(prim(
        solid(cuboid_tapered([kasagi_w, 0.55, 0.95], 0.0, crown_mat())),
        [0.0, kasagi_y, 0.0],
        id_quat(),
    ));
    // Two upswept tips angling out and up from the crown ends — the Myōjin
    // curve that names the gate.
    let tip_len = 1.5_f32;
    let phi = 0.32_f32;
    let dx = kasagi_w * 0.5 + tip_len * 0.5 * phi.cos();
    let dy = tip_len * 0.5 * phi.sin();
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([tip_len, 0.5, 0.9], 0.2, crown_mat())),
            [sx * dx, kasagi_y + dy, 0.0],
            quat_z(sx * phi),
        ));
    }

    // Gakuzuka strut carrying the shrine plaque, between nuki and shimaki.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.45, top - nuki_y - 0.2, 0.25],
            0.0,
            timber(TIMBER_DARK),
        )),
        [0.0, (nuki_y + top) * 0.5, 0.0],
        id_quat(),
    ));
    // Shrine plaque (gaku) hung on the strut, facing the hero front (−Z).
    prims.push(prim(
        solid(cuboid_tapered([0.85, 0.7, 0.1], 0.0, lacquer(LACQUER_RED))),
        [0.0, nuki_y + 0.7, -0.28],
        id_quat(),
    ));

    // Threshold accent: a warm lantern-glow sill spanning the opening just
    // under the nuki — a thin edge strip, so it runs hotter than a broad face
    // without blooming white.
    prims.push(prim(
        cuboid_tapered([2.6, 0.1, 0.14], 0.0, glow(LANTERN_GLOW, 4.0)),
        [0.0, nuki_y - 0.34, 0.0],
        id_quat(),
    ));
    // Two hanging paper lanterns flanking the opening on the hero face, each
    // on a short timber cord. Broad lit lantern bodies stay at low strength so
    // they read as warm-lit paper, not white orbs.
    for sx in [-1.0_f32, 1.0] {
        let lx = sx * (span - 0.5);
        prims.push(prim(
            solid(cylinder_tapered(0.02, 0.5, 6, 0.0, timber(TIMBER_DARK))),
            [lx, nuki_y - 0.45, -0.34],
            id_quat(),
        ));
        prims.push(prim(
            cylinder_tapered(0.16, 0.42, 12, 0.1, glow(LANTERN_GLOW, 2.2)),
            [lx, nuki_y - 0.9, -0.34],
            id_quat(),
        ));
    }

    // The walk-in zone between the pillars: bottom near the footing top,
    // headroom under the nuki.
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.6, 3.2, 1.4]),
        },
        [0.0, 1.9, 0.0],
        id_quat(),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&FeudalJapanGateway.build(""), "feudal_japan_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is set-dressing, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = FeudalJapanGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
