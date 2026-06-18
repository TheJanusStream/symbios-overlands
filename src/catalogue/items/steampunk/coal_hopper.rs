//! Coal hopper — a Steampunk prop. A riveted iron coal bunker on legs with a
//! funnel chute and a heap of coal beneath. Scatter clutter feeding the
//! furnaces.
//!
//! The chute is a cone flipped apex-down with a [`quat_x`] of π.

use std::f32::consts::PI;

use crate::catalogue::items::util::{assemble, cone, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRASS, IRON_DARK, brass, iron};

/// Matte black of the heaped coal.
const COAL: [f32; 3] = [0.08, 0.08, 0.09];

pub struct CoalHopper;

impl CatalogueEntry for CoalHopper {
    fn slug(&self) -> &'static str {
        "coal_hopper"
    }
    fn name(&self) -> &'static str {
        "Coal Hopper"
    }
    fn description(&self) -> &'static str {
        "Riveted iron coal bunker on legs with a funnel chute and a heap of coal."
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
            clearance: 1.5,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Iron bunker body — the root.
        prim(
            solid(cuboid_tapered([1.6, 1.4, 1.6], 0.0, iron(IRON_DARK))),
            [0.0, 1.8, 0.0],
            id_quat(),
        ),
    ];
    // Brass band around the body.
    prims.push(prim(
        solid(cuboid_tapered([1.7, 0.25, 1.7], 0.0, brass(BRASS))),
        [0.0, 2.1, 0.0],
        id_quat(),
    ));

    // Four iron legs.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.15, 1.2, 0.15], 0.0, iron(IRON_DARK))),
                [sx * 0.65, 0.6, sz * 0.65],
                id_quat(),
            ));
        }
    }

    // Funnel chute under the body, apex down.
    prims.push(prim(
        solid(cone(0.8, 1.0, 12, iron(IRON_DARK))),
        [0.0, 0.95, 0.0],
        quat_x(PI),
    ));

    // Heap of coal spilled beneath the chute.
    for (cx, cz, s) in [
        (0.0_f32, 0.0_f32, 0.5_f32),
        (0.4, 0.2, 0.3),
        (-0.3, -0.25, 0.35),
    ] {
        prims.push(prim(
            solid(cuboid_tapered([s, s * 0.6, s], 0.3, iron(COAL))),
            [cx, s * 0.3, cz],
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
        assert_sanitize_stable(&CoalHopper.build(""), "coal_hopper");
    }
}
