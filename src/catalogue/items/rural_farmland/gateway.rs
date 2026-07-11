//! Farm Gate — the Rural/Farmland social gateway (#766). A weathered timber
//! ranch entrance: two squared gate posts on fieldstone piers, a stout header
//! beam under a peaked barn roof, and a red-and-white name board hung out
//! front, lit warm by a pair of post lanterns as the crickets start up. It
//! replaces the neutral placeholder gateway for the theme.
//!
//! The only functional element is the [`GeneratorKind::Gateway`] zone child in
//! the walk-through opening — stepping into it opens the destination picker of
//! the room owner's mutual follows. Everything else frames that opening so it
//! reads as a farm gate you pass through.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, foundation_mat, glow, id_quat, prim, quat_z,
    solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BARN_RED, LAMP_WARM, ROOF_GREY, STONE_GREY, TRIM_WHITE, WOOD_GREY, barn_board, enamel, fx,
    metal_roof, stone, weathered,
};

/// Blackened wrought-iron for the lantern brackets and sign hangers.
const IRON: [f32; 3] = [0.14, 0.14, 0.16];

pub struct RuralFarmlandGateway;

impl CatalogueEntry for RuralFarmlandGateway {
    fn slug(&self) -> &'static str {
        "rural_farmland_gateway"
    }
    fn name(&self) -> &'static str {
        "Farm Gate"
    }
    fn description(&self) -> &'static str {
        "Weathered ranch gate on fieldstone piers, a red name board lit warm under a peaked barn roof."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::RuralFarmland]
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
    let gap_half = 1.8_f32; // X offset of each gate post from centre
    let apron_h = 0.3_f32; // forecourt slab thickness
    let pier_h = 0.7_f32;
    let pier_top = apron_h + pier_h; // 1.0 — timber posts spring from here
    let post_h = 3.4_f32;
    let front = -0.36_f32; // −Z hero face: the name board hangs out here

    // Forecourt apron — the flat-base root (never tilt a root: every child
    // would spin with it).
    let mut prims = vec![prim(
        solid(cuboid_tapered([5.6, apron_h, 3.0], 0.0, foundation_mat())),
        [0.0, apron_h * 0.5, 0.0],
        id_quat(),
    )];

    for sx in [-1.0_f32, 1.0] {
        // Fieldstone pier — the mass the timber post stands on.
        prims.push(prim(
            solid(cuboid_tapered(
                [0.72, pier_h, 0.72],
                0.05,
                stone(STONE_GREY),
            )),
            [sx * gap_half, apron_h + pier_h * 0.5, 0.0],
            id_quat(),
        ));
        // Squared, lightly tapered gate post.
        prims.push(prim(
            solid(cuboid_tapered(
                [0.42, post_h, 0.42],
                0.06,
                weathered(WOOD_GREY),
            )),
            [sx * gap_half, pier_top + post_h * 0.5, 0.0],
            id_quat(),
        ));
    }

    // Header beam bridging the posts.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.0 * gap_half + 1.0, 0.5, 0.62],
            0.0,
            weathered(WOOD_GREY),
        )),
        [0.0, 4.1, 0.0],
        id_quat(),
    ));

    // Peaked barn roof crowning the gate — ridge running along the span (X),
    // gable ends facing the traveller front and back. The posts poke a touch
    // into its underside so nothing sits coplanar with the eave.
    prims.push(prim(
        solid(cuboid_tapered_xz(
            [4.9, 0.8, 1.0],
            [0.08, 0.85],
            metal_roof(ROOF_GREY),
        )),
        [0.0, 4.7, 0.0],
        id_quat(),
    ));

    // Knee braces stiffening each post into the header (a safe child rotation).
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.12, 1.0, 0.12], 0.0, weathered(WOOD_GREY)),
            [sx * 1.45, 3.72, 0.0],
            quat_z(sx * -0.6),
        ));
    }

    // Name board hung out on the −Z front: a pale trim board behind a painted
    // red field, so the cream frame shows all round.
    prims.push(prim(
        cuboid_tapered([3.24, 1.13, 0.08], 0.0, barn_board(TRIM_WHITE)),
        [0.0, 3.55, front + 0.06],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([3.0, 0.95, 0.12], 0.0, barn_board(BARN_RED)),
        [0.0, 3.55, front],
        id_quat(),
    ));
    // Warm-lit sign band across the red field — low strength so it reads as a
    // lamp-lit painted board at dusk, not a white lightbox.
    prims.push(prim(
        cuboid_tapered([2.4, 0.5, 0.06], 0.0, glow(LAMP_WARM, 2.0)),
        [0.0, 3.58, front - 0.08],
        id_quat(),
    ));
    // Iron hanger straps carrying the board off the header.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.1, 0.55, 0.1], 0.0, enamel(IRON)),
            [sx * 1.1, 4.05, front + 0.04],
            id_quat(),
        ));
    }

    // A wrought-iron post lantern flanking each side of the opening — the hot
    // threshold accent. Small deep-amber orbs run bright without blooming
    // white the way a broad lit face would.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.1, 0.1, 0.36], 0.0, enamel(IRON))),
            [sx * (gap_half - 0.24), 3.05, -0.16],
            id_quat(),
        ));
        prims.push(prim(
            sphere(0.15, 4, glow([1.0, 0.6, 0.24], 6.0)),
            [sx * 1.5, 2.85, -0.3],
            id_quat(),
        ));
    }

    // The walk-in zone centred in the opening: bottom at the apron, headroom
    // clearing the header.
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.6, 3.2, 1.4]),
        },
        [0.0, 1.85, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: crickets in the field at dusk.
    root.audio = fx::crickets();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&RuralFarmlandGateway.build(""), "rural_farmland_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is set-dressing, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = RuralFarmlandGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
