//! Hay bales — a Rural/Farmland prop. A pair of big round bales lying on the
//! cut stubble beside a stack of square bales, drying golden in the field
//! after the harvest.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{HAY_GOLD, weathered};

/// Pale cut stubble underfoot.
const STUBBLE: [f32; 3] = [0.60, 0.52, 0.30];
/// Dark baling twine wrapped round the rolls.
const TWINE: [f32; 3] = [0.20, 0.18, 0.14];

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
    // Flat stubble patch — the *root* (a plain `id_quat` base). The round bales
    // each need `quat_x` to lie on their sides, so they must stay non-first
    // children; a rotated `prims[0]` would spin the whole prop.
    let mut prims = vec![prim(
        solid(cuboid_tapered([6.2, 0.12, 3.0], 0.0, weathered(STUBBLE))),
        [0.0, 0.06, 0.0],
        id_quat(),
    )];

    // Two big round bales lying on their sides on the stubble, set side by side
    // (spaced along X; each roll runs front-to-back along Z).
    let bale_r = 0.85_f32;
    for bx in [-2.1_f32, -0.35] {
        prims.push(prim(
            solid(cylinder_tapered(bale_r, 1.5, 16, 0.0, weathered(HAY_GOLD))),
            [bx, 0.12 + bale_r, -0.2],
            quat_x(FRAC_PI_2),
        ));
        // A couple of twine wraps round each roll (axis follows the bale).
        for tz in [-0.45_f32, 0.45] {
            prims.push(prim(
                torus(0.04, bale_r + 0.02, weathered(TWINE)),
                [bx, 0.12 + bale_r, -0.2 + tz],
                quat_x(FRAC_PI_2),
            ));
        }
    }

    // A pyramid stack of square bales alongside, bound with twine.
    let square = || solid(cuboid_tapered([0.95, 0.55, 0.45], 0.0, weathered(HAY_GOLD)));
    for (x, y, z) in [
        (1.9_f32, 0.12 + 0.28_f32, -0.6_f32),
        (1.9, 0.12 + 0.28, -0.05),
        (1.9, 0.12 + 0.28, 0.5),
        (1.9, 0.12 + 0.83, -0.32),
        (1.9, 0.12 + 0.83, 0.23),
    ] {
        prims.push(prim(square(), [x, y, z], id_quat()));
        // Two twine bands across each square bale.
        for tz in [-0.12_f32, 0.12] {
            prims.push(prim(
                cuboid_tapered([0.98, 0.04, 0.05], 0.0, weathered(TWINE)),
                [x, y, z + tz],
                id_quat(),
            ));
        }
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
