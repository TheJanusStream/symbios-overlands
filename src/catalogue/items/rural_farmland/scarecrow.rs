//! Scarecrow — a Rural/Farmland prop. A straw-stuffed figure on a cross
//! frame, with a burlap head and a floppy hat, standing watch over the field
//! as the crickets start up.

use crate::catalogue::items::solarpunk::{CROP_GREEN, crop_tufts, foliage};
use crate::catalogue::items::util::{assemble, cone, cuboid_tapered, id_quat, prim, solid, sphere};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{HAY_GOLD, WOOD_GREY, fx, weathered};

/// Faded denim trousers.
const DENIM: [f32; 3] = [0.28, 0.30, 0.40];
/// Crow black.
const CROW: [f32; 3] = [0.08, 0.08, 0.09];

/// Burlap / sackcloth tan.
const BURLAP: [f32; 3] = [0.62, 0.50, 0.32];
/// Faded plaid shirt.
const SHIRT: [f32; 3] = [0.44, 0.20, 0.18];

pub struct Scarecrow;

impl CatalogueEntry for Scarecrow {
    fn slug(&self) -> &'static str {
        "scarecrow"
    }
    fn name(&self) -> &'static str {
        "Scarecrow"
    }
    fn description(&self) -> &'static str {
        "Straw-stuffed figure on a cross frame with a burlap head and floppy hat."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::RuralFarmland]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FARM_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // Cross-frame post — the root.
    let mut prims = vec![prim(
        solid(cuboid_tapered([0.1, 2.3, 0.1], 0.0, weathered(WOOD_GREY))),
        [0.0, 1.15, 0.0],
        id_quat(),
    )];
    // Arm crossbar.
    prims.push(prim(
        solid(cuboid_tapered([1.9, 0.1, 0.1], 0.0, weathered(WOOD_GREY))),
        [0.0, 1.7, 0.0],
        id_quat(),
    ));

    // Stuffed shirt body.
    prims.push(prim(
        solid(cuboid_tapered([0.8, 1.0, 0.45], 0.1, weathered(SHIRT))),
        [0.0, 1.45, 0.0],
        id_quat(),
    ));
    // Straw cuffs at the hands.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.22, 0.18, 0.18], 0.3, weathered(HAY_GOLD)),
            [sx * 0.9, 1.7, 0.0],
            id_quat(),
        ));
    }

    // Burlap head.
    prims.push(prim(
        solid(sphere(0.28, 3, weathered(BURLAP))),
        [0.0, 2.15, 0.0],
        id_quat(),
    ));
    // Floppy hat: brim + crown.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.7, 0.06, 0.7],
            0.0,
            weathered([0.4, 0.3, 0.18]),
        )),
        [0.0, 2.35, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cone(0.22, 0.3, 8, weathered([0.4, 0.3, 0.18]))),
        [0.0, 2.4, 0.0],
        id_quat(),
    ));
    // A wisp of straw out the collar.
    prims.push(prim(
        cuboid_tapered([0.5, 0.2, 0.4], 0.5, weathered(HAY_GOLD)),
        [0.0, 1.95, 0.0],
        id_quat(),
    ));

    // Stitched burlap face on the −Z front (the camera face).
    for ex in [-0.1_f32, 0.1] {
        prims.push(prim(
            solid(sphere(0.045, 3, weathered(CROW))),
            [ex, 2.2, -0.26],
            id_quat(),
        ));
    }
    prims.push(prim(
        cuboid_tapered([0.18, 0.03, 0.04], 0.0, weathered(CROW)),
        [0.0, 2.05, -0.27],
        id_quat(),
    ));

    // Straw-stuffed denim legs hanging below the shirt, with straw cuffs.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.2, 0.85, 0.22], 0.0, weathered(DENIM))),
            [sx * 0.2, 0.5, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([0.22, 0.16, 0.24], 0.3, weathered(HAY_GOLD)),
            [sx * 0.2, 0.08, 0.0],
            id_quat(),
        ));
    }

    // A crow perched on the outstretched arm.
    prims.push(prim(
        solid(sphere(0.12, 4, weathered(CROW))),
        [0.85, 1.85, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(sphere(0.07, 3, weathered(CROW))),
        [0.78, 1.96, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.22, 0.06, 0.08], 0.4, weathered(CROW)),
        [0.98, 1.86, 0.0],
        id_quat(),
    ));

    // The crop rows it stands watch over, out front.
    prims.extend(crop_tufts(
        [0.0, 0.0, -0.7],
        [1.8, 0.7],
        5,
        2,
        0.3,
        foliage(CROP_GREEN),
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
        assert_sanitize_stable(&Scarecrow.build(""), "scarecrow");
    }
}
