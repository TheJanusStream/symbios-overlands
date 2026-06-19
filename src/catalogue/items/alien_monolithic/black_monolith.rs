//! Black monolith — the Alien-Monolithic landmark and the kit's lit hero. A
//! tall polished obsidian slab hovering a hand's breadth above a glowing base
//! ring, its face inscribed with luminous glyph lines. ~10 m tall, so it
//! anchors the site and reads as the monolith from across the home region. Its
//! glyphs and base ring are the trim escalation's ruin pass snuffs to a dead,
//! lightless slab.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the base ring.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{ENERGY_BLUE, GLYPH_CYAN, OBSIDIAN, fx, obsidian};

pub struct BlackMonolith;

impl CatalogueEntry for BlackMonolith {
    fn slug(&self) -> &'static str {
        "black_monolith"
    }
    fn name(&self) -> &'static str {
        "Black Monolith"
    }
    fn description(&self) -> &'static str {
        "Polished obsidian slab hovering over a glowing base ring, inscribed with glyph lines."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienMonolithic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MONOLITH_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 50.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let slab_h = 10.0_f32;
    let slab_w = 2.4_f32;
    let slab_d = 0.8_f32;
    let lift = 0.7_f32; // hover gap above the base
    let slab_cy = lift + slab_h * 0.5;

    let mut prims = vec![
        // Obsidian base disc — the root.
        prim(
            solid(cylinder_tapered(2.6, 0.3, 24, 0.0, obsidian(OBSIDIAN))),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];
    // Glowing base ring — emissive.
    prims.push(prim(
        torus(0.12, 2.2, glow(ENERGY_BLUE, 2.6)),
        [0.0, 0.34, 0.0],
        id_quat(),
    ));

    // Hovering obsidian slab.
    prims.push(prim(
        solid(cuboid_tapered(
            [slab_w, slab_h, slab_d],
            0.04,
            obsidian(OBSIDIAN),
        )),
        [0.0, slab_cy, 0.0],
        id_quat(),
    ));

    // Glowing glyph lines on the +Z face — emissive.
    prims.push(prim(
        cuboid_tapered([0.14, slab_h - 1.5, 0.86], 0.0, glow(GLYPH_CYAN, 2.8)),
        [0.0, slab_cy, 0.0],
        id_quat(),
    ));
    for k in 0..4 {
        let y = lift + 1.5 + k as f32 * 2.2;
        prims.push(prim(
            cuboid_tapered([1.4, 0.16, 0.86], 0.0, glow(GLYPH_CYAN, 2.6)),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }

    let mut root = assemble(prims);
    // Signature life: the monolith's hum, energy motes rising in the gap.
    root.audio = fx::monolith_hum();
    root.children
        .push(fx::energy_motes([0.0, 0.5, 0.0], 0x0A30_8112));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&BlackMonolith.build(""), "black_monolith");
    }

    #[test]
    fn has_glyphs() {
        assert!(crate::catalogue::items::util::has_emissive(
            &BlackMonolith.build("")
        ));
    }
}
