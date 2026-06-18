//! Toadstool ring — a High-Fantasy *poor* prop. A humble fairy ring of plain
//! red-capped toadstools in the moss. The everyday magic of the hedge-witch's
//! holding.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{assemble, cone, cylinder_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{STONE_MOSS, matte, mossy};

/// Plain toadstool colours.
const STEM: [f32; 3] = [0.84, 0.82, 0.74];
const CAP_RED: [f32; 3] = [0.62, 0.18, 0.16];

pub struct ToadstoolRing;

impl CatalogueEntry for ToadstoolRing {
    fn slug(&self) -> &'static str {
        "toadstool_ring"
    }
    fn name(&self) -> &'static str {
        "Toadstool Ring"
    }
    fn description(&self) -> &'static str {
        "Humble fairy ring of plain red-capped toadstools in the moss."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Fantasy]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FANTASY_POOR
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
    let ring_r = 0.8_f32;

    let mut prims = vec![
        // Mossy floor patch — the root.
        prim(
            solid(cylinder_tapered(1.0, 0.08, 16, 0.0, mossy(STONE_MOSS))),
            [0.0, 0.04, 0.0],
            id_quat(),
        ),
    ];

    // A small ring of plain toadstools.
    for i in 0..6 {
        let a = i as f32 / 6.0 * TAU;
        let scale = 0.7 + (i % 2) as f32 * 0.4;
        let stem_h = 0.4 * scale;
        let x = a.cos() * ring_r;
        let z = a.sin() * ring_r;
        prims.push(prim(
            solid(cylinder_tapered(0.06 * scale, stem_h, 6, 0.0, matte(STEM))),
            [x, stem_h * 0.5 + 0.08, z],
            id_quat(),
        ));
        prims.push(prim(
            solid(cone(0.22 * scale, 0.26 * scale, 8, matte(CAP_RED))),
            [x, stem_h + 0.18 * scale, z],
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
        assert_sanitize_stable(&ToadstoolRing.build(""), "toadstool_ring");
    }
}
