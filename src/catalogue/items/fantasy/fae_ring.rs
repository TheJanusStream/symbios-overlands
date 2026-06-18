//! Fae ring — a High-Fantasy secondary. A mossy circle of little standing
//! stones and glowing mushrooms around a softly-lit spell mark, mana motes
//! rising from its centre. The fairy ring of the arcane quarter; its glow is
//! emissive trim the ruin pass can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the mossy floor.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{ARCANE_PURPLE, MUSH_GLOW, STONE_MOSS, fx, mossy};

pub struct FaeRing;

impl CatalogueEntry for FaeRing {
    fn slug(&self) -> &'static str {
        "fae_ring"
    }
    fn name(&self) -> &'static str {
        "Fae Ring"
    }
    fn description(&self) -> &'static str {
        "Mossy circle of standing stones and glowing mushrooms around a lit spell mark."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Fantasy]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FANTASY_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let ring_r = 3.5_f32;

    let mut prims = vec![
        // Mossy floor disc — the root.
        prim(
            solid(cylinder_tapered(4.0, 0.2, 24, 0.0, mossy(STONE_MOSS))),
            [0.0, 0.1, 0.0],
            id_quat(),
        ),
    ];

    // Glowing spell mark at the centre — emissive.
    prims.push(prim(
        cylinder_tapered(1.4, 0.08, 20, 0.0, glow(ARCANE_PURPLE, 2.2)),
        [0.0, 0.22, 0.0],
        id_quat(),
    ));

    // Ring of little standing stones with glowing mushrooms between them.
    for i in 0..8 {
        let a = i as f32 / 8.0 * TAU;
        if i % 2 == 0 {
            prims.push(prim(
                solid(cuboid_tapered([0.5, 1.2, 0.4], 0.1, mossy(STONE_MOSS))),
                [a.cos() * ring_r, 0.7, a.sin() * ring_r],
                id_quat(),
            ));
        } else {
            // Glowing mushroom: a pale stem + a glowing cap.
            prims.push(prim(
                solid(cylinder_tapered(0.08, 0.5, 6, 0.0, mossy(STONE_MOSS))),
                [a.cos() * ring_r, 0.25, a.sin() * ring_r],
                id_quat(),
            ));
            prims.push(prim(
                cone(0.3, 0.4, 8, glow(MUSH_GLOW, 2.0)),
                [a.cos() * ring_r, 0.6, a.sin() * ring_r],
                id_quat(),
            ));
        }
    }

    let mut root = assemble(prims);
    // Signature life: mana motes rising from the ring centre.
    root.children
        .push(fx::mana_motes([0.0, 0.5, 0.0], 0x0A1A_FA12));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&FaeRing.build(""), "fae_ring");
    }
}
