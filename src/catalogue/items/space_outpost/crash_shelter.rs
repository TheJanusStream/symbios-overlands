//! Crash shelter — the Space-Outpost *poor* landmark. A scorched lander
//! capsule down on its side, repurposed as a shelter with a patched hatch and
//! a bent antenna. The hardscrabble counterpart to the
//! [`habitat_dome`](super::habitat_dome): same frontier, opposite end of the
//! prosperity axis (`Poor`), so a destitute space room grows the wreck site
//! instead of the colony.
//!
//! Rooted on a flat scorched berm (`id_quat`) so the capsule — a tapered
//! cylinder laid along X with a [`quat_z`] of π/2 — and all its trim are
//! children of an upright root (a rotated assemble root would spin every
//! child into its frame).

use std::f32::consts::{FRAC_PI_2, PI};

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_mul, quat_x,
    quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{HULL_PANEL, INTERIOR_WARM, SCORCH, STEEL_DARK, hull, painted, steel};

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
    let cap_y = 1.5_f32;
    let mut prims = vec![
        // Flat scorched berm where it ploughed in — the upright root.
        prim(
            solid(cuboid_tapered([4.8, 0.4, 3.0], 0.0, painted(SCORCH))),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
    ];

    // Scorched lander capsule laid along X (a child → rotation-safe).
    prims.push(prim(
        solid(cylinder_tapered(1.5, 4.0, 16, 0.3, hull(HULL_PANEL))),
        [0.0, cap_y, 0.0],
        quat_z(FRAC_PI_2),
    ));
    // Charred heat-shield nose cone on the +X end.
    prims.push(prim(
        solid(cone(1.45, 1.2, 14, hull(SCORCH))),
        [2.4, cap_y, 0.0],
        quat_z(-FRAC_PI_2),
    ));
    // Scorch drag-band wrapping the hull.
    prims.push(prim(
        solid(cylinder_tapered(1.53, 1.0, 16, 0.3, painted(SCORCH))),
        [-1.0, cap_y, 0.0],
        quat_z(FRAC_PI_2),
    ));

    // Patched hatch (mismatched plates) + a small dim warm port on the −Z
    // hero face.
    prims.push(prim(
        solid(cuboid_tapered([1.5, 1.4, 0.22], 0.0, hull(HULL_PANEL))),
        [-0.2, cap_y, -1.45],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.7, 0.9, 0.18], 0.0, hull(SCORCH))),
        [0.55, cap_y - 0.1, -1.5],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.45, 0.45, 0.12], 0.0, glow(INTERIOR_WARM, 1.4)),
        [-0.4, cap_y + 0.1, -1.58],
        id_quat(),
    ));

    // Two buckled landing legs jutting from under the hull.
    for (sx, lean) in [(-1.0_f32, 0.5_f32), (1.0, -0.4)] {
        prims.push(prim(
            solid(cylinder_tapered(0.1, 1.6, 5, 0.1, steel(STEEL_DARK))),
            [sx * 1.5, 0.7, 1.0],
            quat_mul(quat_z(lean), quat_x(0.55)),
        ));
    }

    // Bent salvaged antenna sticking out of the wreck.
    prims.push(prim(
        solid(cylinder_tapered(0.06, 2.2, 4, 0.0, steel(STEEL_DARK))),
        [-1.4, 2.4, 0.8],
        quat_mul(quat_z(0.4), quat_x(PI * 0.12)),
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
