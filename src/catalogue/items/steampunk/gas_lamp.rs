//! Gas lamp — a Steampunk prop. A wrought-iron lamppost with brass bands,
//! scroll arms and a glowing gas mantle in a brass cage. Scatter clutter
//! lighting the works; its mantle is emissive trim the ruin pass can darken.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRASS, IRON_DARK, LAMP_GAS, brass, iron};

pub struct GasLamp;

impl CatalogueEntry for GasLamp {
    fn slug(&self) -> &'static str {
        "gas_lamp"
    }
    fn name(&self) -> &'static str {
        "Gas Lamp"
    }
    fn description(&self) -> &'static str {
        "Wrought-iron lamppost with brass bands and a glowing gas mantle in a cage."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Steampunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::STEAM_BAND
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
    let mut prims = vec![
        // Iron base — the root.
        prim(
            solid(cuboid_tapered([0.5, 0.4, 0.5], 0.2, iron(IRON_DARK))),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
    ];

    // Iron column with brass bands.
    prims.push(prim(
        solid(cylinder_tapered(0.1, 3.0, 8, 0.12, iron(IRON_DARK))),
        [0.0, 1.9, 0.0],
        id_quat(),
    ));
    for y in [1.0_f32, 3.0] {
        prims.push(prim(
            solid(torus(0.04, 0.13, brass(BRASS))),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }

    // Brass lantern cage.
    prims.push(prim(
        solid(cuboid_tapered([0.4, 0.7, 0.4], 0.25, brass(BRASS))),
        [0.0, 3.7, 0.0],
        id_quat(),
    ));
    // Glowing gas mantle — emissive trim.
    prims.push(prim(
        sphere(0.16, 3, glow(LAMP_GAS, 3.0)),
        [0.0, 3.6, 0.0],
        id_quat(),
    ));
    // Brass finial cap.
    prims.push(prim(
        solid(cylinder_tapered(0.12, 0.3, 6, 0.4, brass(BRASS))),
        [0.0, 4.15, 0.0],
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
        assert_sanitize_stable(&GasLamp.build(""), "gas_lamp");
    }
}
