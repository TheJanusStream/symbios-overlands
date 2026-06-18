//! Comms dish — a Space-Outpost secondary. A big parabolic dish on a steel
//! pedestal yoke, a feed horn at its focus and a warning light on the rim.
//! The deep-space link of the base.
//!
//! The dish face is a shallow tapered disc tilted skyward with a [`quat_x`].

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid, sphere,
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

    // Steel pedestal + yoke.
    prims.push(prim(
        solid(cylinder_tapered(0.45, 3.0, 12, 0.1, steel(STEEL_DARK))),
        [0.0, 2.0, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([1.4, 0.6, 0.8], 0.0, steel(STEEL_DARK))),
        [0.0, 3.5, 0.0],
        id_quat(),
    ));

    // Dish face tilted skyward.
    prims.push(prim(
        solid(cylinder_tapered(3.0, 0.4, 20, 0.35, hull(HULL_WHITE))),
        [0.0, 4.4, 0.4],
        quat_x(-0.6),
    ));
    // Feed horn at the focus on a strut.
    prims.push(prim(
        solid(cylinder_tapered(0.16, 1.4, 8, 0.2, steel(STEEL_DARK))),
        [0.0, 5.6, 1.6],
        quat_x(-0.6),
    ));
    // Red warning light on the rim.
    prims.push(prim(
        sphere(0.2, 3, glow(BEACON_RED, 2.5)),
        [2.4, 5.6, 0.0],
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
