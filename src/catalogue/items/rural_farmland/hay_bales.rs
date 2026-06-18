//! Hay bales — a Rural/Farmland prop. A pair of big round bales beside a
//! stack of square bales, drying golden in the field after the cut.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{HAY_GOLD, weathered};

pub struct HayBales;

impl CatalogueEntry for HayBales {
    fn slug(&self) -> &'static str {
        "hay_bales"
    }
    fn name(&self) -> &'static str {
        "Hay Bales"
    }
    fn description(&self) -> &'static str {
        "Big round bales beside a stack of square bales, golden in the field."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::RuralFarmland]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FARM_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.4,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // Big round bale on its side — the root.
    let round = || solid(cylinder_tapered(0.85, 1.4, 16, 0.0, weathered(HAY_GOLD)));
    let mut prims = vec![prim(round(), [0.0, 0.85, 0.0], quat_x(FRAC_PI_2))];
    prims.push(prim(round(), [0.0, 0.85, 1.6], quat_x(FRAC_PI_2)));

    // A stack of square bales alongside.
    let square = || solid(cuboid_tapered([0.95, 0.55, 0.45], 0.0, weathered(HAY_GOLD)));
    for (x, y, z) in [
        (2.0_f32, 0.3_f32, -0.3_f32),
        (2.0, 0.3, 0.25),
        (2.0, 0.3, 0.8),
        (2.0, 0.85, 0.0),
        (2.0, 0.85, 0.55),
    ] {
        prims.push(prim(square(), [x, y, z], id_quat()));
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&HayBales.build(""), "hay_bales");
    }
}
