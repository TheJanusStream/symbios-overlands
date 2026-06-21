//! Amphitheatre — an AncientClassical secondary. A small open theatre:
//! concentric tiers of sandstone seating curved in a semicircle around a
//! marble orchestra floor, with a low scaenae backdrop wall and two stub
//! columns on the stage.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, torus, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{MARBLE_WHITE, SANDSTONE_GOLD, SANDSTONE_WEATHERED, STONE_VOID, marble, sandstone};

pub struct Amphitheatre;

impl CatalogueEntry for Amphitheatre {
    fn slug(&self) -> &'static str {
        "amphitheatre"
    }
    fn name(&self) -> &'static str {
        "Amphitheatre"
    }
    fn description(&self) -> &'static str {
        "Curved tiers of sandstone seating around a marble orchestra with a scaenae wall."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AncientClassical]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ANCIENT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 38.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // Marble orchestra floor — the root.
    let mut prims = vec![prim(
        solid(cylinder_tapered(4.0, 0.2, 32, 0.0, marble(MARBLE_WHITE))),
        [0.0, 0.1, 0.0],
        id_quat(),
    )];

    // Continuous tiered cavea: concentric path-cut hollow-cylinder steps
    // sweeping the −Z hemisphere and opening to +Z. Each tier is a curved
    // annular arc rising from the ground; its top annulus is the seat tread
    // and the exposed inner wall of the tier behind it is the riser — proper
    // raked seating where the back rows sit higher and further out, instead
    // of scattered blocks. `path_cut [0.47,1.03]` wraps ≈200° with the horns
    // curling past the ±X line toward the stage; `hollow` bores each step to
    // its inner radius. The masonry alternates gold / weathered for depth.
    // (path_cut stays within [0,1] — the sanitiser clamps wider ranges, so a
    // clean semicircle is the widest symmetric wrap about −Z that round-trips.)
    let n_tiers = 7;
    for t in 0..n_tiers {
        let inner_r = 4.3 + t as f32 * 1.0;
        let outer_r = inner_r + 1.15;
        let h = 0.55 + t as f32 * 0.5;
        let mat = if t % 2 == 0 {
            sandstone(SANDSTONE_GOLD)
        } else {
            sandstone(SANDSTONE_WEATHERED)
        };
        prims.push(prim(
            solid(with_cut(
                cylinder_tapered(outer_r, h, 40, 0.0, mat),
                [0.5, 1.0],
                [0.0, 1.0],
                inner_r / outer_r,
            )),
            [0.0, h * 0.5, 0.0],
            id_quat(),
        ));
    }

    // Scaenae frons — a tall sandstone stage wall closing the +Z opening,
    // faced toward the bowl with engaged marble columns and a central arched
    // doorway (porta regia), capped by an oversailing marble cornice.
    let scaenae_z = 4.8_f32;
    let wall_h = 4.6_f32;
    prims.push(prim(
        solid(cuboid_tapered(
            [9.5, wall_h, 0.7],
            0.0,
            sandstone(SANDSTONE_WEATHERED),
        )),
        [0.0, wall_h * 0.5, scaenae_z],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([10.1, 0.4, 1.0], 0.0, marble(MARBLE_WHITE))),
        [0.0, wall_h + 0.2, scaenae_z],
        id_quat(),
    ));
    // Four engaged columns standing proud of the audience (−Z) face.
    for sx in [-3.4_f32, -1.8, 1.8, 3.4] {
        prims.push(prim(
            solid(cylinder_tapered(
                0.32,
                wall_h - 0.4,
                16,
                0.08,
                marble(MARBLE_WHITE),
            )),
            [sx, (wall_h - 0.4) * 0.5, scaenae_z - 0.45],
            id_quat(),
        ));
    }
    // Central arched doorway: a dark recess read through a marble arch.
    prims.push(prim(
        cuboid_tapered([2.0, 3.0, 0.45], 0.0, marble(STONE_VOID)),
        [0.0, 1.5, scaenae_z - 0.15],
        id_quat(),
    ));
    prims.push(prim(
        with_cut(
            torus(0.22, 1.05, marble(MARBLE_WHITE)),
            [0.0, 0.5],
            [0.0, 1.0],
            0.0,
        ),
        [0.0, 3.0, scaenae_z - 0.45],
        quat_x(-FRAC_PI_2),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Amphitheatre.build(""), "amphitheatre");
    }
}
