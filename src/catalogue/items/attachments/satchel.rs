//! Traveler's satchel — the first wearable (#1087), and the wiring's test
//! article: a small leather bag for the left hip. Deliberately modest —
//! fixed size, no fit declaration, five prims — so the Wear path can be
//! judged end to end before the hero items (#1089–#1091) raise the bar.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;

use super::{brass, leather};

/// Pouch leather.
const TAN: [f32; 3] = [0.36, 0.24, 0.13];
/// Flap and loop — a shade darker, so the silhouette reads at a glance.
const DARK: [f32; 3] = [0.27, 0.17, 0.09];

pub struct Satchel;

impl CatalogueEntry for Satchel {
    fn slug(&self) -> &'static str {
        "satchel"
    }
    fn name(&self) -> &'static str {
        "Traveler's Satchel"
    }
    fn description(&self) -> &'static str {
        "Small leather hip bag with a buckled flap — wearable, or set down anywhere."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Attachment
    }
    fn wear_socket(&self) -> Option<symbios_avatar::Socket> {
        Some(symbios_avatar::Socket::LeftHip)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.6,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The bag hangs below the attach origin (the engine seats the origin
/// against the hip; identity offset is the sentinel for that): pouch,
/// lid, a flap sunk into the +Z face with its clasp, and a small belt
/// loop at the top back edge — the part that meets the belt line worn.
fn build_tree() -> Generator {
    // Main pouch — slight taper pinches the top, so the bag bellies
    // toward its bottom the way a loaded satchel sags. Hangs below the
    // attach origin; the engine seats the origin against the hip.
    let mut prims = vec![prim(
        solid(cuboid_tapered([0.26, 0.20, 0.09], 0.10, leather(TAN))),
        [0.0, -0.12, 0.03],
        id_quat(),
    )];
    // Lid across the pouch mouth, a shade darker.
    prims.push(prim(
        solid(cuboid_tapered([0.25, 0.02, 0.10], 0.0, leather(DARK))),
        [0.0, -0.025, 0.03],
        id_quat(),
    ));
    // Flap draping over the top third of the front face. Its back sinks a
    // few millimetres INTO the pouch — intersecting solids, never a shared
    // plane (the z-fight rule) and never a daylight gap.
    prims.push(prim(
        solid(cuboid_tapered([0.25, 0.11, 0.018], 0.0, leather(DARK))),
        [0.0, -0.075, 0.078],
        id_quat(),
    ));
    // Brass clasp riding the flap's lower lip.
    prims.push(prim(
        solid(cuboid_tapered([0.045, 0.045, 0.014], 0.0, brass())),
        [0.0, -0.115, 0.09],
        id_quat(),
    ));
    // Belt loop: small, standing in the X–Y plane at the top back edge —
    // the part that meets the belt line when worn, sunk into the lid so it
    // reads as stitched on rather than perched.
    prims.push(prim(
        torus(0.01, 0.032, leather(DARK)),
        [0.0, -0.01, -0.01],
        quat_x(FRAC_PI_2),
    ));
    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Satchel.build(""), "satchel");
    }

    #[test]
    fn satchel_is_wearable_at_the_left_hip() {
        assert_eq!(Satchel.wear_socket(), Some(symbios_avatar::Socket::LeftHip));
        assert_eq!(Satchel.role(), StructureRole::Attachment);
    }
}
