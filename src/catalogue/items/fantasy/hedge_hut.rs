//! Hedge hut — the High-Fantasy *poor* landmark. A hedge-witch's daub-and-
//! timber hut under a shaggy thatch roof, a crooked chimney and a single
//! softly-glowing window, charms hung at the door. The hedge-magic
//! counterpart to the [`wizard_tower`](super::wizard_tower): same craft,
//! opposite end of the prosperity axis (`Poor`), so a destitute fantasy room
//! grows the witch's holding instead of the mage's seat.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the daub wall.

use crate::catalogue::items::nordic::gable_roof;
use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    ARCANE_GLASS, STONE_MOSS, THATCH_STRAW, TIMBER_DARK, glass, matte, mossy, thatch, timber,
};

/// Pale daub plaster of the hut walls.
const DAUB: [f32; 3] = [0.74, 0.70, 0.58];
/// Dried-herb bundle colour.
const HERB: [f32; 3] = [0.42, 0.5, 0.3];

pub struct HedgeHut;

impl CatalogueEntry for HedgeHut {
    fn slug(&self) -> &'static str {
        "hedge_hut"
    }
    fn name(&self) -> &'static str {
        "Hedge Hut"
    }
    fn description(&self) -> &'static str {
        "Hedge-witch's daub-and-timber hut under shaggy thatch with a glowing window."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Fantasy]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FANTASY_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 34.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let wall_h = 2.6_f32;
    let wall_top = wall_h;

    let mut prims = vec![
        // Daub walls — the root.
        prim(
            solid(cuboid_tapered([4.5, wall_h, 3.8], 0.0, matte(DAUB))),
            [0.0, wall_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Timber corner frame.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.2, wall_h, 0.2], 0.0, timber(TIMBER_DARK))),
                [sx * 2.2, wall_h * 0.5, sz * 1.85],
                id_quat(),
            ));
        }
    }

    // Shaggy steep A-frame thatch roof (ridge ‖ X over the long walls).
    let ridge_y = wall_top + 2.1;
    prims.push(gable_roof(
        [5.4, 2.1, 4.6],
        [0.0, wall_top + 1.05, 0.0],
        thatch(THATCH_STRAW),
    ));
    // Ridge beam seated proud above the apex (never grazing it).
    prims.push(prim(
        solid(cuboid_tapered([5.5, 0.14, 0.14], 0.0, timber(TIMBER_DARK))),
        [0.0, ridge_y + 0.06, 0.0],
        id_quat(),
    ));

    // Timber door + a softly-glowing window on the −Z (front) face.
    prims.push(prim(
        solid(cuboid_tapered([0.9, 1.9, 0.2], 0.0, timber(TIMBER_DARK))),
        [-1.0, 0.95, -1.95],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.8, 0.8, 0.15], 0.0, glass(ARCANE_GLASS, 1.2)),
        [1.2, 1.5, -1.95],
        id_quat(),
    ));

    // Crooked mossy-stone chimney poking up past the ridge at the gable end.
    prims.push(prim(
        solid(cuboid_tapered([0.68, 3.0, 0.68], 0.1, mossy(STONE_MOSS))),
        [1.8, wall_top + 1.0, -0.9],
        quat_x(0.08),
    ));

    // Dried-herb bundles hung beside the door, tapering to a tied tip.
    for (cy, len) in [(1.7_f32, 0.5_f32), (1.4, 0.42), (1.15, 0.46)] {
        prims.push(prim(
            solid(cuboid_tapered([0.14, len, 0.14], 0.7, matte(HERB))),
            [-1.9, cy, -1.92],
            quat_x(std::f32::consts::PI),
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
        assert_sanitize_stable(&HedgeHut.build(""), "hedge_hut");
    }
}
