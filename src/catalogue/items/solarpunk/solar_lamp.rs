//! Solar lamp — a Solarpunk prop. A path bollard with a small PV cap and a
//! warm glowing light. Scatter clutter lighting the garden paths; its lamp is
//! emissive trim the ruin pass can darken.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{LAMP_WARM, PV_BLUE, STEEL_GREY, pv, steel};

pub struct SolarLamp;

impl CatalogueEntry for SolarLamp {
    fn slug(&self) -> &'static str {
        "solar_lamp"
    }
    fn name(&self) -> &'static str {
        "Solar Lamp"
    }
    fn description(&self) -> &'static str {
        "Path bollard with a small PV cap and a warm glowing light."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Solarpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SOLAR_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.5,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let prims = vec![
        // Steel bollard post — the root.
        prim(
            solid(cylinder_tapered(0.1, 1.6, 8, 0.05, steel(STEEL_GREY))),
            [0.0, 0.8, 0.0],
            id_quat(),
        ),
        // Warm glowing light just under the cap — emissive trim.
        prim(
            cuboid_tapered([0.3, 0.3, 0.3], 0.0, glow(LAMP_WARM, 2.5)),
            [0.0, 1.7, 0.0],
            id_quat(),
        ),
        // Small tilted PV cap on top.
        prim(
            solid(cuboid_tapered([0.5, 0.06, 0.5], 0.0, pv(PV_BLUE))),
            [0.0, 1.95, 0.0],
            quat_x(0.25),
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
        assert_sanitize_stable(&SolarLamp.build(""), "solar_lamp");
    }
}
