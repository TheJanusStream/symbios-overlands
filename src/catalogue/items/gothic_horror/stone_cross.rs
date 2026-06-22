//! Stone cross — a Gothic-Horror prop. A weathered ringed cross on a stepped
//! base, lichened with age. Scatter clutter marking the graves.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{STONE_MOSS, mossy};

pub struct StoneCross;

impl CatalogueEntry for StoneCross {
    fn slug(&self) -> &'static str {
        "stone_cross"
    }
    fn name(&self) -> &'static str {
        "Stone Cross"
    }
    fn description(&self) -> &'static str {
        "Weathered ringed cross on a stepped base, lichened with age."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::GOTHIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let ms = || mossy(STONE_MOSS);
    let mut prims = vec![
        // Lowest Calvary step — the root.
        prim(
            solid(cuboid_tapered([1.5, 0.32, 1.5], 0.05, ms())),
            [0.0, 0.16, 0.0],
            id_quat(),
        ),
    ];
    // Middle + upper steps.
    prims.push(prim(
        solid(cuboid_tapered([1.15, 0.3, 1.15], 0.05, ms())),
        [0.0, 0.47, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.85, 0.3, 0.85], 0.05, ms())),
        [0.0, 0.77, 0.0],
        id_quat(),
    ));
    // Socket stone.
    prims.push(prim(
        solid(cuboid_tapered([0.52, 0.42, 0.52], 0.1, ms())),
        [0.0, 1.1, 0.0],
        id_quat(),
    ));

    // Tapered shaft.
    prims.push(prim(
        solid(cylinder_tapered(0.19, 2.3, 8, 0.2, ms())),
        [0.0, 2.45, 0.0],
        id_quat(),
    ));

    // Cross head: upright + transom arms.
    let head_y = 3.55_f32;
    prims.push(prim(
        solid(cuboid_tapered([0.28, 0.9, 0.26], 0.0, ms())),
        [0.0, head_y + 0.05, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([1.3, 0.3, 0.26], 0.0, ms())),
        [0.0, head_y, 0.0],
        id_quat(),
    ));
    // Celtic ring, standing in the cross plane.
    prims.push(prim(
        solid(torus(0.11, 0.46, ms())),
        [0.0, head_y, 0.0],
        quat_x(FRAC_PI_2),
    ));
    // Carved central boss at the crossing.
    prims.push(prim(
        solid(sphere(0.13, 6, ms())),
        [0.0, head_y, -0.16],
        id_quat(),
    ));
    // Small gabled finial crowning the head.
    prims.push(prim(
        solid(cone(0.17, 0.36, 4, ms())),
        [0.0, head_y + 0.62, 0.0],
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
        assert_sanitize_stable(&StoneCross.build(""), "stone_cross");
    }
}
