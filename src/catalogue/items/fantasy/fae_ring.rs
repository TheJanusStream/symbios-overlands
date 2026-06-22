//! Fae ring — a High-Fantasy secondary. A mossy circle of little standing
//! stones and glowing mushrooms around a softly-lit spell mark, mana motes
//! rising from its centre. The fairy ring of the arcane quarter; its glow is
//! emissive trim the ruin pass can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the mossy floor.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_z, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{ARCANE_PURPLE, MUSH_GLOW, RUNE_GOLD, STONE_MOSS, fx, mossy, toadstool};

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
    let ring_r = 3.4_f32;

    let mut prims = vec![
        // Mossy floor disc — the root.
        prim(
            solid(cylinder_tapered(4.0, 0.2, 24, 0.0, mossy(STONE_MOSS))),
            [0.0, 0.1, 0.0],
            id_quat(),
        ),
    ];

    // Glowing sigil inscribed at the centre — twin rune rings (not a flat
    // glowing puddle) with little glyph marks between them.
    for major in [1.35_f32, 0.85] {
        prims.push(prim(
            torus(0.05, major, glow(ARCANE_PURPLE, 1.9)),
            [0.0, 0.2, 0.0],
            id_quat(),
        ));
    }
    for i in 0..6 {
        let a = i as f32 / 6.0 * TAU + 0.3;
        prims.push(prim(
            cuboid_tapered([0.14, 0.05, 0.14], 0.0, glow(RUNE_GOLD, 2.0)),
            [a.cos() * 1.1, 0.21, a.sin() * 1.1],
            id_quat(),
        ));
    }

    // Ring of little leaning menhirs with glowing toadstools between them.
    for i in 0..8 {
        let a = i as f32 / 8.0 * TAU;
        let (x, z) = (a.cos() * ring_r, a.sin() * ring_r);
        if i % 2 == 0 {
            // A weathered little standing stone, tapered to a blunt point and
            // leaning a touch outward.
            let lean = 0.12 * if i % 4 == 0 { 1.0 } else { -1.0 };
            prims.push(prim(
                solid(cuboid_tapered([0.42, 1.5, 0.34], 0.3, mossy(STONE_MOSS))),
                [x, 0.78, z],
                quat_z(lean),
            ));
        } else {
            // Glowing domed toadstool.
            prims.push(toadstool([x, 0.18, z], 0.85, glow(MUSH_GLOW, 1.5), false));
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
