//! Landing pad — a Space-Outpost secondary. A wide ceramic pad ringed with
//! hazard paint and edge beacons, a touchdown cross at its centre. The
//! spaceport apron of the base; its beacons are emissive trim the ruin pass
//! can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the pad.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BEACON_RED, HAZARD_YELLOW, PAD_GREY, concrete, painted};

pub struct LandingPad;

impl CatalogueEntry for LandingPad {
    fn slug(&self) -> &'static str {
        "landing_pad"
    }
    fn name(&self) -> &'static str {
        "Landing Pad"
    }
    fn description(&self) -> &'static str {
        "Wide ceramic pad with hazard paint, edge beacons and a touchdown cross."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SpaceOutpost]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::OUTPOST_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 8.0,
            min_spawn_dist: 42.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let pad_h = 0.3_f32;
    let pad_top = pad_h;
    let radius = 6.0_f32;

    let mut prims = vec![
        // Ceramic pad — the root.
        prim(
            solid(cylinder_tapered(radius, pad_h, 28, 0.0, concrete(PAD_GREY))),
            [0.0, pad_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Hazard ring painted near the rim.
    prims.push(prim(
        torus(0.1, radius - 0.6, painted(HAZARD_YELLOW)),
        [0.0, pad_top + 0.02, 0.0],
        id_quat(),
    ));
    // Touchdown cross at the centre.
    for rot in [false, true] {
        let size = if rot {
            [0.5, 0.06, 3.0]
        } else {
            [3.0, 0.06, 0.5]
        };
        prims.push(prim(
            cuboid_tapered(size, 0.0, painted(HAZARD_YELLOW)),
            [0.0, pad_top + 0.04, 0.0],
            id_quat(),
        ));
    }

    // Edge beacons around the rim — emissive.
    for i in 0..6 {
        let a = i as f32 / 6.0 * TAU;
        prims.push(prim(
            cuboid_tapered([0.2, 0.6, 0.2], 0.0, glow(BEACON_RED, 2.5)),
            [
                a.cos() * (radius - 0.3),
                pad_top + 0.3,
                a.sin() * (radius - 0.3),
            ],
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
        assert_sanitize_stable(&LandingPad.build(""), "landing_pad");
    }
}
