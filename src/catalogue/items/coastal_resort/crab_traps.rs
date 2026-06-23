//! Crab traps — a Coastal-Resort *poor* prop. A leaning stack of wire-frame
//! crab pots with bright net floats and a coil of rope: the working clutter
//! of the fishing hamlet's quay.

use crate::catalogue::items::util::{
    assemble, cone, cylinder_tapered, id_quat, prim, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{AWNING_WHITE, BUOY_RED, DECK_WOOD, enamel, plank, steel};

/// Galvanised wire of the pot hoops.
const WIRE_GALV: [f32; 3] = [0.62, 0.64, 0.66];
/// Dark tarred drum of the pot body.
const POT_DARK: [f32; 3] = [0.30, 0.31, 0.33];

/// A round wire crab pot: a dark drum ringed by two galvanised hoops with a
/// funnel mouth on top — reads as a cage rather than a solid block.
fn crab_pot(center: [f32; 3]) -> Vec<Generator> {
    let [cx, cy, cz] = center;
    let r = 0.36_f32;
    let mut out = vec![prim(
        solid(cylinder_tapered(r, 0.42, 12, 0.0, steel(POT_DARK))),
        [cx, cy, cz],
        id_quat(),
    )];
    for dy in [-0.14_f32, 0.14] {
        out.push(prim(
            torus(0.045, r, steel(WIRE_GALV)),
            [cx, cy + dy, cz],
            id_quat(),
        ));
    }
    // Funnel mouth on top — a small inverted cone narrowing into the pot.
    out.push(prim(
        cone(0.16, 0.2, 10, steel(WIRE_GALV)),
        [cx, cy + 0.31, cz],
        id_quat(),
    ));
    out
}

pub struct CrabTraps;

impl CatalogueEntry for CrabTraps {
    fn slug(&self) -> &'static str {
        "crab_traps"
    }
    fn name(&self) -> &'static str {
        "Crab Traps"
    }
    fn description(&self) -> &'static str {
        "A leaning stack of wire crab pots with net floats and a rope coil."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CoastalResort]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::RESORT_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.2,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // Three round wire pots in a leaning stack — the bottom pot's drum is the
    // flat root.
    let mut prims = crab_pot([0.0, 0.24, 0.0]);
    prims.extend(crab_pot([-0.58, 0.24, 0.34]));
    prims.extend(crab_pot([0.1, 0.78, 0.05]));

    // Bright net floats perched on the stack.
    for (pos, color) in [
        ([0.12_f32, 1.18, 0.06], BUOY_RED),
        ([-0.12, 1.18, -0.2], AWNING_WHITE),
        ([-0.58, 0.64, 0.34], BUOY_RED),
    ] {
        prims.push(prim(solid(sphere(0.16, 3, enamel(color))), pos, id_quat()));
    }

    // Coil of rope on the ground.
    prims.push(prim(
        torus(0.07, 0.4, plank(DECK_WOOD)),
        [0.8, 0.07, -0.5],
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
        assert_sanitize_stable(&CrabTraps.build(""), "crab_traps");
    }
}
