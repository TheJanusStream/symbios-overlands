//! Idol — a Mesoamerican prop. A squat carved stone deity on a plinth: a
//! blocky seated figure with a gold headdress, a jade collar, and faintly
//! glowing jade eyes that watch the precinct.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BONE_WHITE, GOLD_WARM, JADE_GREEN, STONE_GREY, cobble, gold, jade, painted};

/// Cold jade glow worked into the carved eyes.
const EYE_GLOW: [f32; 3] = [0.35, 0.85, 0.55];

pub struct Idol;

impl CatalogueEntry for Idol {
    fn slug(&self) -> &'static str {
        "idol"
    }
    fn name(&self) -> &'static str {
        "Idol"
    }
    fn description(&self) -> &'static str {
        "Carved stone deity with a gold headdress and glowing jade eyes."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Mesoamerican]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MESO_BAND
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
    // A squat, frontal seated deity — broad blocky shoulders (wide in X,
    // shallow in Z) and knees thrust forward break the axial symmetry so the
    // stone reads as a figure, not a finial. Face features and headdress all
    // sit on the front (−Z).
    let mut prims = vec![
        // Plinth — the root.
        prim(
            solid(cuboid_tapered([1.6, 0.4, 1.4], 0.0, cobble(STONE_GREY))),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
        // Seated lap / folded legs (kept blocky, no taper).
        prim(
            solid(cuboid_tapered([1.4, 0.65, 1.2], 0.0, cobble(STONE_GREY))),
            [0.0, 0.725, 0.0],
            id_quat(),
        ),
        // Broad shoulders torso — wide in X, shallow in Z.
        prim(
            solid(cuboid_tapered([1.3, 0.9, 0.55], 0.0, cobble(STONE_GREY))),
            [0.0, 1.5, 0.0],
            id_quat(),
        ),
        // Blocky head.
        prim(
            solid(cuboid_tapered([0.68, 0.64, 0.6], 0.0, cobble(STONE_GREY))),
            [0.0, 2.42, 0.0],
            id_quat(),
        ),
    ];

    // Knees thrust forward, and forearms resting on them — the seated pose.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.42, 0.5, 0.5], 0.0, cobble(STONE_GREY))),
            [sx * 0.38, 0.8, -0.45],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.26, 0.9, 0.4], 0.0, cobble(STONE_GREY))),
            [sx * 0.55, 1.15, -0.14],
            id_quat(),
        ));
    }

    // Jade collar / necklace across the shoulders.
    prims.push(prim(
        cuboid_tapered([1.08, 0.2, 0.62], 0.0, jade(JADE_GREEN)),
        [0.0, 1.98, 0.0],
        id_quat(),
    ));

    // Carved face: a brow ridge, deep-set glowing jade eyes, a jutting nose,
    // and a bared-fang mouth.
    prims.push(prim(
        cuboid_tapered([0.62, 0.12, 0.1], 0.0, cobble(STONE_GREY)),
        [0.0, 2.58, -0.28],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.16, 0.13, 0.08], 0.0, glow(EYE_GLOW, 1.8)),
            [sx * 0.16, 2.46, -0.30],
            id_quat(),
        ));
    }
    prims.push(prim(
        cuboid_tapered([0.12, 0.3, 0.18], 0.0, cobble(STONE_GREY)),
        [0.0, 2.36, -0.33],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.36, 0.13, 0.07], 0.0, painted(BONE_WHITE)),
        [0.0, 2.18, -0.31],
        id_quat(),
    ));

    // Stepped gold headdress: a brow band, a central crest plume, and ear
    // flares hanging at the temples.
    prims.push(prim(
        solid(cuboid_tapered([0.86, 0.26, 0.64], 0.0, gold(GOLD_WARM))),
        [0.0, 2.86, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.36, 0.6, 0.22], 0.25, gold(GOLD_WARM))),
        [0.0, 3.25, -0.05],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.18, 0.42, 0.16], 0.1, gold(GOLD_WARM)),
            [sx * 0.42, 2.64, -0.06],
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
        assert_sanitize_stable(&Idol.build(""), "idol");
    }
}
