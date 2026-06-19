//! Amphitheatre — an AncientClassical secondary. A small open theatre:
//! concentric tiers of sandstone seating curved in a semicircle around a
//! marble orchestra floor, with a low scaenae backdrop wall and two stub
//! columns on the stage.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_y, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{MARBLE_WHITE, SANDSTONE_GOLD, SANDSTONE_WEATHERED, marble, sandstone};

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
        solid(cylinder_tapered(4.0, 0.2, 28, 0.0, marble(MARBLE_WHITE))),
        [0.0, 0.1, 0.0],
        id_quat(),
    )];

    // Three curved seating tiers sweeping the −Z hemisphere, opening to +Z.
    let seats = 9;
    for t in 0..3 {
        let radius = 4.6 + t as f32 * 1.5;
        let y = 0.5 + t as f32 * 0.9;
        for i in 0..seats {
            let a = -1.3 + 2.6 * (i as f32) / ((seats - 1) as f32);
            let x = radius * a.sin();
            let z = -radius * a.cos();
            prims.push(prim(
                solid(cuboid_tapered(
                    [1.35, 0.6, 1.1],
                    0.0,
                    sandstone(SANDSTONE_GOLD),
                )),
                [x, y, z],
                quat_y(a),
            ));
        }
    }

    // Low scaenae backdrop wall on the open (+Z) stage side, with stubs.
    prims.push(prim(
        solid(cuboid_tapered(
            [8.0, 3.0, 0.6],
            0.0,
            sandstone(SANDSTONE_WEATHERED),
        )),
        [0.0, 1.5, 4.6],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cylinder_tapered(0.3, 2.6, 16, 0.1, marble(MARBLE_WHITE))),
            [sx * 2.4, 1.3, 4.2],
            id_quat(),
        ));
    }

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
