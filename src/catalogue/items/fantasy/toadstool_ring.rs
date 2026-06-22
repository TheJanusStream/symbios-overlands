//! Toadstool ring — a High-Fantasy *poor* prop. A humble fairy ring of plain
//! red-capped toadstools in the moss. The everyday magic of the hedge-witch's
//! holding.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{assemble, cylinder_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{STONE_MOSS, matte, mossy, toadstool};

/// Plain red toadstool cap colour.
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
    let ring_r = 0.85_f32;

    let mut prims = vec![
        // Mossy floor patch — the root.
        prim(
            solid(cylinder_tapered(1.0, 0.08, 16, 0.0, mossy(STONE_MOSS))),
            [0.0, 0.04, 0.0],
            id_quat(),
        ),
    ];

    // A small ring of plain red-capped, white-spotted toadstools.
    for i in 0..6 {
        let a = i as f32 / 6.0 * TAU;
        let scale = 0.75 + (i % 2) as f32 * 0.45;
        prims.push(toadstool(
            [a.cos() * ring_r, 0.07, a.sin() * ring_r],
            scale,
            matte(CAP_RED),
            true,
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
