//! Standing stone — a High-Fantasy *poor* secondary. A lone moss-covered
//! menhir leaning at an angle, its old glyphs only faintly aglow. The wayside
//! marker of the hedge-magic holding.
//!
//! The menhir leans with a single [`quat_x`].

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{RUNE_GOLD, STONE_MOSS, mossy, rune_marks};

pub struct StandingStone;

impl CatalogueEntry for StandingStone {
    fn slug(&self) -> &'static str {
        "standing_stone"
    }
    fn name(&self) -> &'static str {
        "Standing Stone"
    }
    fn description(&self) -> &'static str {
        "Lone moss-covered menhir leaning at an angle, its old glyphs faintly aglow."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Fantasy]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FANTASY_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.0,
            min_spawn_dist: 26.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Mossy base mound — the root.
        prim(
            solid(cylinder_tapered(1.0, 0.3, 12, 0.2, mossy(STONE_MOSS))),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];

    // Leaning menhir, tapered to a weathered blunt crown.
    let mut menhir = prim(
        solid(cuboid_tapered([0.9, 3.2, 0.6], 0.22, mossy(STONE_MOSS))),
        [0.0, 1.7, 0.0],
        quat_x(0.16),
    );
    // Faintly-glowing rune strokes carved into the −Z face — children of the
    // menhir so they lean with it (a hint of old magic, near the glow
    // threshold). Local frame: origin at the menhir centre, front face −Z.
    for stroke in rune_marks([0.0, 0.15, -0.32], 0.95, glow(RUNE_GOLD, 0.95)) {
        menhir.children.push(stroke);
    }
    prims.push(menhir);

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&StandingStone.build(""), "standing_stone");
    }
}
