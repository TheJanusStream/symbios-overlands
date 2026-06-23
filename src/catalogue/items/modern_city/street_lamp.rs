//! Street lamp — a Modern-City prop. A tall steel pole with a curved mast
//! arm and a warm glowing luminaire leaning over the roadway.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, LAMP_WARM, STEEL_GREY, concrete, steel};

pub struct StreetLamp;

impl CatalogueEntry for StreetLamp {
    fn slug(&self) -> &'static str {
        "street_lamp"
    }
    fn name(&self) -> &'static str {
        "Street Lamp"
    }
    fn description(&self) -> &'static str {
        "Tall steel pole with a curved arm and a warm glowing luminaire."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::ModernCity]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CITY_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let pole_h = 5.5;

    let mut prims = vec![
        // Concrete footing — the root.
        prim(
            solid(cuboid_tapered(
                [0.6, 0.3, 0.6],
                0.1,
                concrete(CONCRETE_GREY),
            )),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
        // Cast base collar.
        prim(
            solid(cylinder_tapered(
                0.26,
                0.6,
                12,
                0.3,
                steel([0.32, 0.33, 0.35]),
            )),
            [0.0, 0.45, 0.0],
            id_quat(),
        ),
        // Steel pole.
        prim(
            solid(cylinder_tapered(0.13, pole_h, 8, 0.25, steel(STEEL_GREY))),
            [0.0, pole_h * 0.5 + 0.3, 0.0],
            id_quat(),
        ),
    ];

    // Mast arm reaching out over the road toward +X.
    let arm_base = pole_h + 0.1;
    prims.push(prim(
        solid(cylinder_tapered(0.09, 2.6, 6, 0.2, steel(STEEL_GREY))),
        [1.28, arm_base + 0.22, 0.0],
        quat_z(-1.4),
    ));
    // Cobra-head luminaire housing at the arm end, tapering to the tip.
    prims.push(prim(
        solid(cuboid_tapered(
            [1.0, 0.32, 0.46],
            0.4,
            steel([0.3, 0.31, 0.33]),
        )),
        [2.3, arm_base + 0.28, 0.0],
        id_quat(),
    ));
    // Warm glowing lens on the underside of the head.
    prims.push(prim(
        cuboid_tapered([0.62, 0.12, 0.36], 0.1, glow(LAMP_WARM, 3.5)),
        [2.3, arm_base + 0.08, 0.0],
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
        assert_sanitize_stable(&StreetLamp.build(""), "street_lamp");
    }
}
