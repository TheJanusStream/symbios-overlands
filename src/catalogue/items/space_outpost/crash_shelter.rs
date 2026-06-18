//! Crash shelter — the Space-Outpost *poor* landmark. A scorched lander
//! capsule down on its side, repurposed as a shelter with a patched hatch and
//! a bent antenna. The hardscrabble counterpart to the
//! [`habitat_dome`](super::habitat_dome): same frontier, opposite end of the
//! prosperity axis (`Poor`), so a destitute space room grows the wreck site
//! instead of the colony.
//!
//! The capsule is a tapered cylinder tipped on its side with a [`quat_x`] of
//! π/2.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{HULL_PANEL, SCORCH, STEEL_DARK, VIEWPORT_LIT, hull, steel};

pub struct CrashShelter;

impl CatalogueEntry for CrashShelter {
    fn slug(&self) -> &'static str {
        "crash_shelter"
    }
    fn name(&self) -> &'static str {
        "Crash Shelter"
    }
    fn description(&self) -> &'static str {
        "Scorched lander capsule on its side, repurposed with a patched hatch."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SpaceOutpost]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::OUTPOST_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 7.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Scorched capsule on its side — the root, laid along Z.
        prim(
            solid(cylinder_tapered(1.8, 4.5, 16, 0.3, hull(HULL_PANEL))),
            [0.0, 1.6, 0.0],
            quat_x(FRAC_PI_2),
        ),
    ];

    // Scorch band where it dragged in.
    prims.push(prim(
        solid(cylinder_tapered(1.82, 1.2, 16, 0.3, hull(SCORCH))),
        [0.0, 1.6, 1.4],
        quat_x(FRAC_PI_2),
    ));

    // Patched hatch + a small lit port on the +X side.
    prims.push(prim(
        solid(cuboid_tapered([0.2, 1.6, 1.2], 0.0, hull(HULL_PANEL))),
        [1.7, 1.4, -0.4],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.15, 0.5, 0.5], 0.0, glow(VIEWPORT_LIT, 0.8)),
        [1.82, 1.6, -0.4],
        id_quat(),
    ));

    // Bent antenna sticking out of the wreck.
    prims.push(prim(
        solid(cylinder_tapered(0.06, 2.2, 4, 0.0, steel(STEEL_DARK))),
        [-0.6, 2.6, -1.6],
        quat_x(0.5),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CrashShelter.build(""), "crash_shelter");
    }
}
