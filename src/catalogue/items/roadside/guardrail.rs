//! Guardrail — a Roadside prop. A short run of galvanised W-beam highway
//! barrier on steel posts. Scatter clutter that lines the shoulder.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CORRUGATED_GREY, SIGN_AMBER, STEEL_GREY, corrugated, steel};

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
    let rail_y = 0.7_f32;

    let mut prims = vec![
        // Galvanised W-beam rail — the root.
        prim(
            solid(cuboid_tapered(
                [4.4, 0.34, 0.1],
                0.0,
                corrugated(CORRUGATED_GREY),
            )),
            [0.0, rail_y, 0.0],
            id_quat(),
        ),
    ];
    // Centre bolt-rib down the beam valley + top/bottom edge lips, each proud
    // of the corrugated face so the W-profile reads (never flush).
    prims.push(prim(
        solid(cuboid_tapered([4.4, 0.07, 0.06], 0.0, steel(STEEL_GREY))),
        [0.0, rail_y, -0.07],
        id_quat(),
    ));
    for sy in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([4.42, 0.05, 0.13], 0.0, steel(STEEL_GREY))),
            [0.0, rail_y + sy * 0.17, 0.0],
            id_quat(),
        ));
    }

    // Three steel posts with flat caps + blockout spacers behind the rail.
    for x in [-1.8_f32, 0.0, 1.8] {
        prims.push(prim(
            solid(cuboid_tapered([0.12, 0.9, 0.12], 0.0, steel(STEEL_GREY))),
            [x, 0.45, 0.06],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.17, 0.05, 0.17], 0.0, steel(STEEL_GREY))),
            [x, 0.92, 0.06],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.1, 0.22, 0.08], 0.0, steel(STEEL_GREY))),
            [x, rail_y, 0.02],
            id_quat(),
        ));
    }

    // Amber reflectors marching along the −Z face.
    for x in [-1.8_f32, 0.0, 1.8] {
        prims.push(prim(
            cuboid_tapered([0.1, 0.1, 0.04], 0.0, glow(SIGN_AMBER, 1.8)),
            [x, rail_y, -0.11],
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
