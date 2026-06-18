//! Street lamp — a Modern-City prop. A tall steel pole with a curved mast
//! arm and a warm glowing luminaire leaning over the roadway.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid,
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
        // Steel pole.
        prim(
            solid(cylinder_tapered(0.13, pole_h, 8, 0.25, steel(STEEL_GREY))),
            [0.0, pole_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Curved mast arm reaching out over the road.
    prims.push(prim(
        solid(cylinder_tapered(0.08, 2.2, 6, 0.0, steel(STEEL_GREY))),
        [0.7, pole_h - 0.1, 0.0],
        quat_x(1.3),
    ));
    // Warm glowing luminaire at the arm end.
    prims.push(prim(
        cuboid_tapered([0.7, 0.25, 0.4], 0.2, glow(LAMP_WARM, 4.0)),
        [1.5, pole_h - 0.3, 0.0],
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
