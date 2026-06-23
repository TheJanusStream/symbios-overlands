//! Road sign — a Roadside prop. A green highway guide panel on twin steel
//! posts, white-bordered with a blank legend block. Scatter clutter for the
//! shoulder.

use std::f32::consts::FRAC_PI_4;

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_z, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{ROAD_GREEN, SIGN_WHITE, STEEL_GREY, enamel, steel};

pub struct RoadSign;

impl CatalogueEntry for RoadSign {
    fn slug(&self) -> &'static str {
        "road_sign"
    }
    fn name(&self) -> &'static str {
        "Road Sign"
    }
    fn description(&self) -> &'static str {
        "Green highway guide panel on twin steel posts."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Roadside]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ROADSIDE_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.2,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let py = 2.4_f32; // panel centre height

    let mut prims = vec![
        // Left post — the root.
        prim(
            solid(cuboid_tapered([0.13, 2.9, 0.13], 0.0, steel(STEEL_GREY))),
            [-0.75, 1.45, 0.0],
            id_quat(),
        ),
    ];
    // Right post.
    prims.push(prim(
        solid(cuboid_tapered([0.13, 2.9, 0.13], 0.0, steel(STEEL_GREY))),
        [0.75, 1.45, 0.0],
        id_quat(),
    ));

    // White border behind, green panel proud of it — never flush.
    prims.push(prim(
        solid(cuboid_tapered([2.62, 1.32, 0.06], 0.0, enamel(SIGN_WHITE))),
        [0.0, py, 0.05],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([2.44, 1.14, 0.1], 0.0, enamel(ROAD_GREEN))),
        [0.0, py, 0.0],
        id_quat(),
    ));

    // Legend on the −Z face: a route shield, two destination bars and a down
    // arrow, each proud of the green panel.
    // Route shield (a white badge with a green inset number field).
    prims.push(prim(
        cuboid_tapered([0.46, 0.56, 0.07], 0.0, enamel(SIGN_WHITE)),
        [-0.82, py + 0.1, -0.07],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.32, 0.4, 0.05], 0.0, enamel(ROAD_GREEN)),
        [-0.82, py + 0.1, -0.11],
        id_quat(),
    ));
    // Two destination legend bars.
    prims.push(prim(
        cuboid_tapered([1.3, 0.2, 0.06], 0.0, enamel(SIGN_WHITE)),
        [0.28, py + 0.28, -0.07],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([1.05, 0.16, 0.06], 0.0, enamel(SIGN_WHITE)),
        [0.15, py - 0.04, -0.07],
        id_quat(),
    ));
    // Down arrow: a shaft plus a 45°-rotated square head.
    prims.push(prim(
        cuboid_tapered([0.12, 0.36, 0.06], 0.0, enamel(SIGN_WHITE)),
        [0.5, py - 0.32, -0.07],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.26, 0.26, 0.06], 0.0, enamel(SIGN_WHITE)),
        [0.5, py - 0.5, -0.07],
        quat_z(FRAC_PI_4),
    ));

    // Green EXIT tab perched on the panel's top corner.
    prims.push(prim(
        solid(cuboid_tapered([1.0, 0.44, 0.1], 0.0, enamel(ROAD_GREEN))),
        [0.78, py + 0.86, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.74, 0.22, 0.06], 0.0, enamel(SIGN_WHITE)),
        [0.78, py + 0.86, -0.07],
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
        assert_sanitize_stable(&RoadSign.build(""), "road_sign");
    }
}
