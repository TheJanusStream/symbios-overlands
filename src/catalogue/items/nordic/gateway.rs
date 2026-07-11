//! Rune Gateway — the Nordic bespoke social gateway (#763). Two carved
//! standing stones flank the walk-through opening, spanned by a heavy
//! carved-timber lintel whose ends rear up into dragon-head finials; a
//! glowing runic serpent ring (the Jelling loop) marks the threshold and
//! glyph columns run down each stone's shore-facing (-Z) front. Replaces
//! the neutral placeholder gate for a Nordic room.
//!
//! The functional element is the single [`GeneratorKind::Gateway`] zone
//! child between the stones — walking into it opens the destination
//! picker. Everything else is Norse set-dressing framing that opening.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, glow, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    DRAGON_EYE, IRON_DARK, STONE_COLD, WOOD_DARK, WOOD_WARM, dragon_head, iron, rough_stone, stone,
    timber,
};

/// Cold rune-light worked into the carved faces and threshold — the same
/// glacial blue as the rune stones, so the gate reads as one steading.
const RUNE_GLOW: [f32; 3] = [0.42, 0.62, 0.92];

pub struct NordicGateway;

impl CatalogueEntry for NordicGateway {
    fn slug(&self) -> &'static str {
        "nordic_gateway"
    }
    fn name(&self) -> &'static str {
        "Rune Gateway"
    }
    fn description(&self) -> &'static str {
        "Rune-carved standing stones spanned by a dragon-headed lintel, a glowing serpent ring marking the way through."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Nordic]
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
    // Layout: two standing stones at x = ±STONE_X leave a 2.6 m opening
    // between their inner faces; a timber lintel bridges their heads.
    let stone_x = 1.7_f32;
    let stone_w = 0.8_f32;
    let stone_d = 0.7_f32;
    let stone_h = 3.8_f32;
    let slab_top = 0.3_f32;
    let stone_top = slab_top + stone_h; // 4.1
    let beam_h = 0.5_f32;
    let beam_y = stone_top + beam_h * 0.5; // 4.35
    let beam_top = beam_y + beam_h * 0.5; // 4.6

    // Flagstone threshold slab — the flat-base root (never tilt a root:
    // every child would spin with it).
    let mut prims = vec![prim(
        solid(cuboid_tapered(
            [5.6, slab_top, 2.4],
            0.0,
            rough_stone(STONE_COLD),
        )),
        [0.0, slab_top * 0.5, 0.0],
        id_quat(),
    )];

    // Two carved standing stones flanking the opening, lightly tapered to a
    // weathered crown, with a glyph column glowing down each shore-facing
    // (-Z) front.
    let front_z = -(stone_d * 0.5 + 0.02);
    for sx in [-1.0_f32, 1.0] {
        let x = sx * stone_x;
        prims.push(prim(
            solid(cuboid_tapered(
                [stone_w, stone_h, stone_d],
                0.12,
                stone(STONE_COLD),
            )),
            [x, slab_top + stone_h * 0.5, 0.0],
            id_quat(),
        ));
        // Rune glyph column carved down the front.
        for k in 0..3 {
            prims.push(prim(
                cuboid_tapered([0.14, 0.42, 0.05], 0.0, glow(RUNE_GLOW, 1.7)),
                [x, slab_top + 1.0 + k as f32 * 0.75, front_z],
                id_quat(),
            ));
        }
    }

    // Heavy carved-timber lintel bridging the stone heads.
    let beam_d = 0.7_f32;
    prims.push(prim(
        solid(cuboid_tapered(
            [4.4, beam_h, beam_d],
            0.0,
            timber(WOOD_WARM),
        )),
        [0.0, beam_y, 0.0],
        id_quat(),
    ));
    // Iron straps wrapping the lintel over each stone head.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.18, beam_h + 0.14, beam_d + 0.08],
                0.0,
                iron(IRON_DARK),
            )),
            [sx * stone_x, beam_y, 0.0],
            id_quat(),
        ));
    }

    // Carved rune panel across the lintel front, holding the serpent ring.
    let panel_z = -(beam_d * 0.5 + 0.03);
    prims.push(prim(
        solid(cuboid_tapered([1.6, 0.42, 0.06], 0.0, timber(WOOD_DARK))),
        [0.0, beam_y, panel_z],
        id_quat(),
    ));
    // Glowing runic serpent ring — the Jelling loop, the hero emblem on the
    // -Z front. Thin trim, so it runs a touch hot without blooming white.
    prims.push(prim(
        torus(0.05, 0.5, glow(RUNE_GLOW, 1.9)),
        [0.0, beam_y, panel_z - 0.04],
        quat_x(FRAC_PI_2),
    ));

    // Dragon-head finials rearing up-and-outward off each lintel end — the
    // Norse signature that turns the span into a guarded gate. Each is a
    // positioned subtree, so its yaw is safe (never the assemble root).
    prims.push(dragon_head(
        [-stone_x, beam_top, 0.0],
        0.65,
        PI, // left head faces -X, outward
        WOOD_DARK,
        DRAGON_EYE,
    ));
    prims.push(dragon_head(
        [stone_x, beam_top, 0.0],
        0.65,
        0.0, // right head faces +X, outward
        WOOD_DARK,
        DRAGON_EYE,
    ));

    // Cold-blue threshold accents echoing the zone veil without bloom: a
    // lintel-underside strip and a floor sill line across the opening.
    prims.push(prim(
        cuboid_tapered([2.6, 0.1, 0.14], 0.0, glow(RUNE_GLOW, 2.4)),
        [0.0, stone_top - 0.15, -0.24],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([2.6, 0.06, 0.2], 0.0, glow(RUNE_GLOW, 2.0)),
        [0.0, slab_top + 0.03, front_z + 0.1],
        id_quat(),
    ));

    // The walk-through zone between the stones: floor at the slab top,
    // headroom up under the lintel. The only functional element.
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
        assert_sanitize_stable(&NordicGateway.build(""), "nordic_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is furniture, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = NordicGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
