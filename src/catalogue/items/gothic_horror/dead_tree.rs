//! Dead tree — a Gothic-Horror prop. A bare, gnarled, leafless tree, its
//! branches clawing at the fog. Scatter clutter haunting the necropolis.
//!
//! Branches tilt with a [`quat_x`].

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, id_quat, prim, quat_mul, quat_x, quat_y, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DEADWOOD, wood};

pub struct DeadTree;

impl CatalogueEntry for DeadTree {
    fn slug(&self) -> &'static str {
        "dead_tree"
    }
    fn name(&self) -> &'static str {
        "Dead Tree"
    }
    fn description(&self) -> &'static str {
        "Bare, gnarled, leafless tree, its branches clawing at the fog."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::GOTHIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.6,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let dw = || wood(DEADWOOD);
    let mut prims = vec![
        // Gnarled trunk — the root, tapering up.
        prim(
            solid(cylinder_tapered(0.36, 3.6, 8, 0.5, dw())),
            [0.0, 1.8, 0.0],
            id_quat(),
        ),
    ];

    // Buttress root flare splaying from the base.
    for k in 0..5 {
        let az = k as f32 * std::f32::consts::TAU / 5.0;
        prims.push(prim(
            solid(cylinder_tapered(0.13, 0.8, 6, 0.7, dw())),
            [az.sin() * 0.28, 0.18, az.cos() * 0.28],
            quat_mul(quat_y(az), quat_x(1.15)),
        ));
    }

    // Gnarled limbs reaching out and up at varied heights and azimuths.
    for (y, len, tilt, az, r) in [
        (2.1_f32, 1.7_f32, 0.8_f32, 0.4_f32, 0.12_f32),
        (2.5, 1.5, 0.7, 2.5, 0.11),
        (2.9, 1.4, 0.6, 4.2, 0.1),
        (3.2, 1.2, 0.5, 1.4, 0.09),
        (3.5, 1.0, 0.45, 5.4, 0.08),
    ] {
        prims.push(prim(
            solid(cylinder_tapered(r, len, 6, 0.6, dw())),
            [az.sin() * 0.2, y + len * 0.22, az.cos() * 0.2],
            quat_mul(quat_y(az), quat_x(tilt)),
        ));
        // A clawing sub-twig forking off each limb's tip.
        let tx = az.sin() * (0.2 + len * 0.45);
        let tz = az.cos() * (0.2 + len * 0.45);
        prims.push(prim(
            solid(cylinder_tapered(0.045, 0.7, 5, 0.7, dw())),
            [tx, y + len * 0.7, tz],
            quat_mul(quat_y(az + 0.8), quat_x(0.5)),
        ));
    }

    // A few crooked twigs clawing at the crown.
    for (tilt, az) in [(1.0_f32, 0.6_f32), (1.1, 3.0), (0.9, 4.6)] {
        prims.push(prim(
            solid(cylinder_tapered(0.04, 0.8, 5, 0.7, dw())),
            [az.sin() * 0.1, 3.7, az.cos() * 0.1],
            quat_mul(quat_y(az), quat_x(tilt)),
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
        assert_sanitize_stable(&DeadTree.build(""), "dead_tree");
    }
}
