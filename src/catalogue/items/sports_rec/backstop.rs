//! Backstop — a Sports/Recreation *poor* secondary. A tall chain-link
//! baseball backstop on steel posts with a forward overhang. The rusting
//! edge of the municipal rec ground.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_y, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CHAIN_GREY, CONCRETE_GREY, STEEL_GREY, chainlink, concrete, steel};

pub struct Backstop;

impl CatalogueEntry for Backstop {
    fn slug(&self) -> &'static str {
        "backstop"
    }
    fn name(&self) -> &'static str {
        "Backstop"
    }
    fn description(&self) -> &'static str {
        "Tall chain-link baseball backstop on steel posts with a forward overhang."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SportsRec]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SPORTS_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.5,
            min_spawn_dist: 26.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let pad_h = 0.15_f32;
    let post_h = 3.8_f32;
    let panel_y = pad_h + 1.85;

    let mut prims = vec![
        // Dirt-rimmed concrete pad — the root.
        prim(
            solid(cuboid_tapered(
                [7.0, pad_h, 3.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, pad_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Central chain-link panel.
    prims.push(prim(
        cuboid_tapered([4.0, 3.5, 0.05], 0.0, chainlink(CHAIN_GREY)),
        [0.0, panel_y, 0.0],
        id_quat(),
    ));
    // Two wing panels hinged at the centre and angled forward toward the −Z
    // plate, giving the wrap-around baseball-backstop shape.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([2.4, 3.5, 0.05], 0.0, chainlink(CHAIN_GREY)),
            [sx * 2.91, panel_y, -0.77],
            quat_y(sx * 0.7),
        ));
    }

    // Round steel posts: centre, the two hinges and the two wing tips. Rusting.
    let posts = [
        [0.0_f32, 0.0_f32],
        [-2.0, 0.0],
        [2.0, 0.0],
        [-3.82, -1.54],
        [3.82, -1.54],
    ];
    for [x, z] in posts {
        prims.push(prim(
            solid(cylinder_tapered(0.1, post_h, 8, 0.0, steel(STEEL_GREY))),
            [x, pad_h + post_h * 0.5, z],
            id_quat(),
        ));
    }
    // Top and bottom rails across the central panel.
    for y in [pad_h + 0.3, pad_h + 3.4] {
        prims.push(prim(
            solid(cuboid_tapered([4.2, 0.1, 0.1], 0.0, steel(STEEL_GREY))),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }

    // Forward overhang tilted in over the plate (toward the −Z front).
    prims.push(prim(
        cuboid_tapered([4.0, 0.05, 1.4], 0.0, chainlink(CHAIN_GREY)),
        [0.0, pad_h + 3.4, -0.7],
        quat_x(-0.5),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Backstop.build(""), "backstop");
    }
}
