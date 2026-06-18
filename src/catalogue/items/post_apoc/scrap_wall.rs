//! Scrap wall — a Post-apocalyptic prop. A barrier of mismatched corrugated
//! and plate metal welded to leaning posts. Scatter clutter fencing the
//! holdout.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CORRUGATED_RUST, RUST_BROWN, STEEL_GREY, rusted, sheet};

pub struct ScrapWall;

impl CatalogueEntry for ScrapWall {
    fn slug(&self) -> &'static str {
        "scrap_wall"
    }
    fn name(&self) -> &'static str {
        "Scrap Wall"
    }
    fn description(&self) -> &'static str {
        "Barrier of mismatched corrugated and plate metal welded to leaning posts."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::PostApoc]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::POSTAPOC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.0,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Tallest corrugated panel — the root.
        prim(
            solid(cuboid_tapered(
                [1.4, 2.4, 0.12],
                0.0,
                sheet(CORRUGATED_RUST),
            )),
            [-1.2, 1.2, 0.0],
            id_quat(),
        ),
    ];
    // Mismatched panels of varying height welded alongside.
    prims.push(prim(
        solid(cuboid_tapered([1.4, 2.0, 0.14], 0.0, rusted(STEEL_GREY))),
        [0.2, 1.0, 0.05],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([1.2, 1.6, 0.12], 0.0, sheet(RUST_BROWN))),
        [1.4, 0.8, -0.04],
        id_quat(),
    ));

    // Leaning support posts.
    for x in [-1.8_f32, 0.0, 1.9] {
        prims.push(prim(
            solid(cuboid_tapered([0.12, 2.2, 0.12], 0.0, rusted(STEEL_GREY))),
            [x, 1.1, -0.12],
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
        assert_sanitize_stable(&ScrapWall.build(""), "scrap_wall");
    }
}
