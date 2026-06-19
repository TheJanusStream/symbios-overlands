//! Mudbrick hut — the AncientClassical *poor* landmark. A small sun-baked
//! adobe dwelling with thick walls, a flat mud roof on protruding timber
//! beams, and a dark doorway. The destitute counterpart to the marble
//! kit: a poor classical room grows this instead of a temple or villa.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{ADOBE_TAN, STONE_VOID, adobe};

pub struct MudbrickHut;

impl CatalogueEntry for MudbrickHut {
    fn slug(&self) -> &'static str {
        "mudbrick_hut"
    }
    fn name(&self) -> &'static str {
        "Mudbrick Hut"
    }
    fn description(&self) -> &'static str {
        "Sun-baked adobe dwelling with a flat beamed roof and a dark doorway."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AncientClassical]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ANCIENT_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 28.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Dark weathered roof beam (palm-log).
fn beam() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.32, 0.24, 0.15]),
        roughness: Fp(0.9),
        ..Default::default()
    }
}

fn build_tree() -> Generator {
    let l = 4.6_f32; // along X, door faces +X
    let w = 4.0_f32; // along Z
    let foot_h = 0.3;
    let wall_h = 2.6;
    let wall_top = foot_h + wall_h;

    let mut prims = vec![
        // Low adobe footing — the root.
        prim(
            solid(cuboid_tapered(
                [l + 0.5, foot_h, w + 0.5],
                0.0,
                adobe(ADOBE_TAN),
            )),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
        // Thick adobe walls.
        prim(
            solid(cuboid_tapered([l, wall_h, w], 0.04, adobe(ADOBE_TAN))),
            [0.0, foot_h + wall_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Protruding roof beams spanning the width, set across the walls.
    for bx in [-1.4_f32, 0.0, 1.4] {
        prims.push(prim(
            solid(cylinder_tapered(0.08, w + 1.0, 6, 0.0, beam())),
            [bx, wall_top - 0.1, 0.0],
            crate::catalogue::items::util::quat_x(std::f32::consts::FRAC_PI_2),
        ));
    }
    // Flat mud roof slab over the beams.
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 0.3, 0.3, w + 0.3],
            0.0,
            adobe(ADOBE_TAN),
        )),
        [0.0, wall_top + 0.15, 0.0],
        id_quat(),
    ));

    // Dark doorway in the near gable.
    prims.push(prim(
        cuboid_tapered([0.2, 1.7, 1.0], 0.0, adobe(STONE_VOID)),
        [l * 0.5 + 0.02, foot_h + 0.85, 0.0],
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
        assert_sanitize_stable(&MudbrickHut.build(""), "mudbrick_hut");
    }
}
