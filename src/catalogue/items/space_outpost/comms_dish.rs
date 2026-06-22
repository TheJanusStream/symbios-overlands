//! Comms dish — a Space-Outpost secondary. A big concave parabolic dish on a
//! steel alt-az yoke, a feed horn at its focus and a warning light on the rim.
//! The deep-space link of the base.
//!
//! The dish is a shallow lower-hemisphere bowl (`profile_cut`) aimed skyward
//! toward the camera with a [`quat_x`], so the reflector reads as a real
//! concave dish rather than a convex pebble; the feed strut and horn run along
//! the same dish axis.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, prim_scaled, quat_x, quat_z,
    solid, sphere, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BEACON_RED, HULL_WHITE, PAD_GREY, STEEL_DARK, concrete, fx, hull, steel};

pub struct CommsDish;

impl CatalogueEntry for CommsDish {
    fn slug(&self) -> &'static str {
        "comms_dish"
    }
    fn name(&self) -> &'static str {
        "Comms Dish"
    }
    fn description(&self) -> &'static str {
        "Big parabolic dish on a steel pedestal with a feed horn and a warning light."
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
            clearance: 5.0,
            min_spawn_dist: 38.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Concrete base — the root.
        prim(
            solid(cuboid_tapered([2.6, 0.5, 2.6], 0.0, concrete(PAD_GREY))),
            [0.0, 0.25, 0.0],
            id_quat(),
        ),
    ];

    // Steel pedestal.
    prims.push(prim(
        solid(cylinder_tapered(0.5, 2.6, 12, 0.18, steel(STEEL_DARK))),
        [0.0, 1.8, 0.0],
        id_quat(),
    ));
    // Alt-az yoke fork arms holding the dish trunnion.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.22, 1.5, 0.55], 0.0, steel(STEEL_DARK))),
            [sx * 0.75, 3.6, 0.15],
            id_quat(),
        ));
    }
    // Trunnion axle across the fork (cylinder laid along X).
    prims.push(prim(
        solid(cylinder_tapered(0.18, 1.7, 10, 0.0, steel(STEEL_DARK))),
        [0.0, 4.25, 0.15],
        quat_z(FRAC_PI_2),
    ));

    // Concave parabolic dish — a shallow lower-hemisphere bowl, axis tilted
    // up-and-toward the camera (−Z) so the reflector face shows.
    let c = [0.0_f32, 4.4, 0.25];
    let axis = quat_x(-0.6);
    // The dish axis: quat_x(-0.6) applied to +Y.
    let d = [0.0_f32, (0.6_f32).cos(), -(0.6_f32).sin()];
    prims.push(prim_scaled(
        solid(with_cut(
            sphere(2.8, 6, hull(HULL_WHITE)),
            [0.0, 1.0],
            [0.0, 0.5],
            0.0,
        )),
        c,
        axis,
        [1.0, 0.5, 1.0],
    ));
    // Hub on the dish back, bridging to the trunnion.
    prims.push(prim(
        solid(cylinder_tapered(0.45, 0.7, 12, 0.2, steel(STEEL_DARK))),
        [c[0] - d[0] * 0.3, c[1] - d[1] * 0.3, c[2] - d[2] * 0.3],
        axis,
    ));
    // Feed strut + horn at the focus, along the dish axis.
    prims.push(prim(
        solid(cylinder_tapered(0.08, 1.7, 6, 0.0, steel(STEEL_DARK))),
        [c[0] + d[0] * 0.85, c[1] + d[1] * 0.85, c[2] + d[2] * 0.85],
        axis,
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.2, 0.5, 8, 0.45, hull(HULL_WHITE))),
        [c[0] + d[0] * 1.6, c[1] + d[1] * 1.6, c[2] + d[2] * 1.6],
        axis,
    ));
    // Red warning light on the rim.
    prims.push(prim(
        sphere(0.18, 4, glow(BEACON_RED, 2.5)),
        [2.4, 4.9, -0.1],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: radio static crackling from the receiver.
    root.audio = fx::comms_static();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CommsDish.build(""), "comms_dish");
    }
}
