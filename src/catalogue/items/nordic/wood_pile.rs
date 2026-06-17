//! Wood pile — a Nordic *poor* prop. A neat stack of split firewood beside a
//! chopping stump with the axe still buried in it: the winter fuel of a
//! croft. The cut top of the stump shows its end-grain rings.

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{IRON_DARK, WOOD_DARK, WOOD_WARM, iron, log_end, timber};

pub struct WoodPile;

impl CatalogueEntry for WoodPile {
    fn slug(&self) -> &'static str {
        "wood_pile"
    }
    fn name(&self) -> &'static str {
        "Wood Pile"
    }
    fn description(&self) -> &'static str {
        "Stacked split firewood beside a chopping stump with a buried axe."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Nordic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::NORDIC_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.2,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // Chopping stump (root) — its sawn top shows end-grain.
    let mut prims = vec![prim(
        solid(cylinder_tapered(0.42, 0.7, 12, 0.05, log_end(WOOD_WARM))),
        [0.9, 0.35, 0.0],
        id_quat(),
    )];

    // Stacked split billets, offset row to row, alternating tone.
    let rows = 3;
    let cols = 4;
    for r in 0..rows {
        let y = 0.18 + r as f32 * 0.34;
        let shove = if r % 2 == 0 { 0.0 } else { 0.16 };
        for c in 0..cols {
            let z = -0.9 + c as f32 * 0.6 + shove;
            let tone = if (r + c) % 2 == 0 {
                WOOD_WARM
            } else {
                WOOD_DARK
            };
            prims.push(prim(
                solid(cuboid_tapered([0.9, 0.3, 0.55], 0.05, timber(tone))),
                [-0.6, y, z],
                id_quat(),
            ));
        }
    }

    // Axe buried in the stump: a leaning haft and an iron head.
    prims.push(prim(
        solid(cylinder_tapered(0.04, 1.0, 6, 0.0, timber(WOOD_WARM))),
        [0.9, 1.0, 0.0],
        quat_x(0.35),
    ));
    prims.push(prim(
        solid(cone(0.1, 0.3, 6, iron(IRON_DARK))),
        [0.9, 0.62, 0.18],
        quat_x(1.2),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&WoodPile.build(""), "wood_pile");
    }
}
