//! Barrel stack — a Medieval prop. A cluster of iron-hooped oak barrels of
//! ale and salt, three on the ground and one stacked on top: the stores of
//! a tavern or market.

use crate::catalogue::items::util::{assemble, cylinder_tapered, id_quat, prim, solid, torus};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{IRON_DARK, WOOD_DARK, WOOD_OAK, iron, timber};

pub struct BarrelStack;

impl CatalogueEntry for BarrelStack {
    fn slug(&self) -> &'static str {
        "barrel_stack"
    }
    fn name(&self) -> &'static str {
        "Barrel Stack"
    }
    fn description(&self) -> &'static str {
        "Cluster of iron-hooped oak barrels, three on the ground and one stacked on top."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Medieval]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MEDIEVAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.4,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// One standing oak barrel at `center`: a bellied staved drum with two
/// iron hoops, returned as a [`Generator`] for the assemble list.
fn barrel(center: [f32; 3], tone: [f32; 3]) -> Generator {
    let h = 1.0;
    let mut b = prim(
        solid(cylinder_tapered(0.4, h, 14, -0.12, timber(tone))),
        center,
        id_quat(),
    );
    // Two iron hoops, ringing the local Y axis.
    for dy in [h * 0.3, -h * 0.3] {
        b.children.push(prim(
            torus(0.035, 0.41, iron(IRON_DARK)),
            [0.0, dy, 0.0],
            id_quat(),
        ));
    }
    b
}

fn build_tree() -> Generator {
    // Three barrels on the ground in a tight triangle.
    let ground_y = 0.5;
    let mut prims = vec![barrel([0.0, ground_y, -0.45], WOOD_OAK)];
    prims.push(barrel([0.42, ground_y, 0.25], WOOD_DARK));
    prims.push(barrel([-0.42, ground_y, 0.25], WOOD_OAK));
    // One nestled on top.
    prims.push(barrel([0.0, ground_y + 1.0, 0.0], WOOD_DARK));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&BarrelStack.build(""), "barrel_stack");
    }
}
