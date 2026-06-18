//! Guardrail — a Roadside prop. A short run of galvanised W-beam highway
//! barrier on steel posts. Scatter clutter that lines the shoulder.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CORRUGATED_GREY, STEEL_GREY, corrugated, steel};

pub struct Guardrail;

impl CatalogueEntry for Guardrail {
    fn slug(&self) -> &'static str {
        "guardrail"
    }
    fn name(&self) -> &'static str {
        "Guardrail"
    }
    fn description(&self) -> &'static str {
        "Short run of galvanised W-beam highway barrier on steel posts."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Roadside]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ROADSIDE_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.2,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Galvanised W-beam rail — the root.
        prim(
            solid(cuboid_tapered(
                [4.4, 0.35, 0.1],
                0.0,
                corrugated(CORRUGATED_GREY),
            )),
            [0.0, 0.7, 0.0],
            id_quat(),
        ),
    ];

    // Three steel posts.
    for x in [-1.8_f32, 0.0, 1.8] {
        prims.push(prim(
            solid(cuboid_tapered([0.12, 0.9, 0.12], 0.0, steel(STEEL_GREY))),
            [x, 0.45, 0.02],
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
        assert_sanitize_stable(&Guardrail.build(""), "guardrail");
    }
}
