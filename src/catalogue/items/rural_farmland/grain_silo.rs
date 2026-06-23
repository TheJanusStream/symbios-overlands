//! Grain silo — a Rural/Farmland secondary. A tall galvanised-steel storage
//! silo with ribbed walls, a conical roof and vent cap, and an external fill
//! chute. The vertical landmark of the farmyard.

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, id_quat, prim, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{ROOF_GREY, SILO_STEEL, STONE_GREY, enamel, metal_roof, silo_metal, stone};

pub struct GrainSilo;

impl CatalogueEntry for GrainSilo {
    fn slug(&self) -> &'static str {
        "grain_silo"
    }
    fn name(&self) -> &'static str {
        "Grain Silo"
    }
    fn description(&self) -> &'static str {
        "Tall ribbed-steel grain silo with a conical roof and a fill chute."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::RuralFarmland]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FARM_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 32.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 0.5;
    let body_h = 13.0;
    let r = 2.5_f32;

    let mut prims = vec![
        // Concrete pad — the root.
        prim(
            solid(cylinder_tapered(
                r + 0.4,
                base_h,
                24,
                0.0,
                stone(STONE_GREY),
            )),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
        // Ribbed steel body.
        prim(
            solid(cylinder_tapered(r, body_h, 24, 0.0, silo_metal(SILO_STEEL))),
            [0.0, base_h + body_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Conical roof and vent cap.
    let roof_y = base_h + body_h;
    prims.push(prim(
        solid(cone(r + 0.3, 2.2, 24, metal_roof(ROOF_GREY))),
        [0.0, roof_y + 1.1, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.5, 0.6, 12, 0.0, enamel(SILO_STEEL))),
        [0.0, roof_y + 2.4, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cone(0.55, 0.5, 12, metal_roof(ROOF_GREY))),
        [0.0, roof_y + 2.9, 0.0],
        id_quat(),
    ));

    // External fill chute running up one side.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.6, body_h + 1.5, 0.5],
            0.0,
            enamel([0.5, 0.5, 0.52]),
        )),
        [r + 0.25, base_h + (body_h + 1.5) * 0.5, 0.0],
        id_quat(),
    ));
    // Round hoop bands hugging the body (a torus, so no square corners jut
    // past the cylinder wall).
    for k in 1..5 {
        let y = base_h + body_h * (k as f32 / 5.0);
        prims.push(prim(
            torus(0.07, r + 0.05, enamel([0.5, 0.52, 0.54])),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }

    // Access ladder up the −X side (opposite the fill chute).
    let lad_x = -r - 0.05;
    for sz in [-0.22_f32, 0.22] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.06, body_h, 0.06],
                0.0,
                enamel([0.5, 0.5, 0.52]),
            )),
            [lad_x, base_h + body_h * 0.5, sz],
            id_quat(),
        ));
    }
    for k in 0..8 {
        let y = base_h + 0.7 + k as f32 * (body_h - 1.4) / 7.0;
        prims.push(prim(
            cuboid_tapered([0.12, 0.05, 0.5], 0.0, enamel([0.5, 0.5, 0.52])),
            [lad_x, y, 0.0],
            id_quat(),
        ));
    }

    // Man-door at the base on the −Z front.
    prims.push(prim(
        solid(cuboid_tapered(
            [1.0, 1.8, 0.15],
            0.0,
            enamel([0.42, 0.44, 0.46]),
        )),
        [0.0, base_h + 0.9, -r + 0.05],
        id_quat(),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&GrainSilo.build(""), "grain_silo");
    }
}
