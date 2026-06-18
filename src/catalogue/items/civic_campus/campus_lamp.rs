//! Campus lamp — a Civic/Campus prop. A traditional cast lamppost with a
//! glowing globe on a steel column. Scatter clutter lighting the quad paths;
//! its globe is emissive trim the ruin pass can darken.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, LAMP_WARM, STEEL_GREY, concrete, steel};

pub struct CampusLamp;

impl CatalogueEntry for CampusLamp {
    fn slug(&self) -> &'static str {
        "campus_lamp"
    }
    fn name(&self) -> &'static str {
        "Campus Lamp"
    }
    fn description(&self) -> &'static str {
        "Cast lamppost with a glowing globe on a steel column."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CivicCampus]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CAMPUS_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.6,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let prims = vec![
        // Concrete base — the root.
        prim(
            solid(cuboid_tapered(
                [0.4, 0.3, 0.4],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
        // Steel column.
        prim(
            solid(cylinder_tapered(0.08, 3.2, 8, 0.1, steel(STEEL_GREY))),
            [0.0, 1.9, 0.0],
            id_quat(),
        ),
        // Lantern housing.
        prim(
            solid(cuboid_tapered([0.34, 0.4, 0.34], 0.2, steel(STEEL_GREY))),
            [0.0, 3.6, 0.0],
            id_quat(),
        ),
        // Glowing globe — emissive trim.
        prim(
            sphere(0.2, 3, glow(LAMP_WARM, 3.0)),
            [0.0, 3.45, 0.0],
            id_quat(),
        ),
    ];

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CampusLamp.build(""), "campus_lamp");
    }
}
