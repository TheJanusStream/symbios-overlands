//! Spell circle — a High-Fantasy prop. A glowing double-ring sigil inscribed
//! on the ground with floating glyph marks. Scatter clutter of the arcane
//! quarter; it is emissive trim the ruin pass can darken.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, torus};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{ARCANE_PURPLE, RUNE_GOLD};

pub struct SpellCircle;

impl CatalogueEntry for SpellCircle {
    fn slug(&self) -> &'static str {
        "spell_circle"
    }
    fn name(&self) -> &'static str {
        "Spell Circle"
    }
    fn description(&self) -> &'static str {
        "Glowing double-ring sigil inscribed on the ground with floating glyph marks."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Fantasy]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FANTASY_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.5,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Outer glowing ring — the root.
        prim(
            torus(0.05, 1.4, glow(ARCANE_PURPLE, 2.2)),
            [0.0, 0.05, 0.0],
            id_quat(),
        ),
    ];
    // Inner glowing ring.
    prims.push(prim(
        torus(0.04, 0.9, glow(ARCANE_PURPLE, 2.2)),
        [0.0, 0.05, 0.0],
        id_quat(),
    ));

    // Glyph marks floating just above the rings.
    for i in 0..6 {
        let a = i as f32 / 6.0 * TAU;
        prims.push(prim(
            cuboid_tapered([0.18, 0.05, 0.18], 0.0, glow(RUNE_GOLD, 2.5)),
            [a.cos() * 1.15, 0.12, a.sin() * 1.15],
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
        assert_sanitize_stable(&SpellCircle.build(""), "spell_circle");
    }
}
