//! Per-family default avatar builders.
//!
//! [`build_for_did`] is the single entry point the record layer calls:
//! it resolves the DID's [`ChassisFamily`] and dispatches to that
//! family's builder, returning both halves of the avatar record — the
//! visuals tree and a locomotion preset that *matches* the visuals
//! (boat → HoverBoat, airship → Helicopter, humanoid → Humanoid,
//! skiff → Car), so the default chassis drives the way it looks.
//!
//! One file per family, mirroring the
//! [`crate::seeded_defaults::avatar`] deriver layout; shared
//! primitive/material/banner vocabulary lives in [`common`].

mod airship;
mod boat;
mod common;
mod humanoid;
mod skiff;

use crate::pds::generator::Generator;
use crate::pds::types::Fp;
use crate::seeded_defaults::{AvatarBody, AvatarGait, ChassisFamily};

use super::locomotion::{
    CarParams, HelicopterParams, HoverBoatParams, HumanoidParams, LocomotionConfig,
    LocomotionPreset,
};

/// Build the full seeded default avatar (visuals + locomotion) for a
/// DID. Deterministic: every peer derives the identical record.
pub fn build_for_did(did: &str) -> (Generator, LocomotionConfig) {
    match ChassisFamily::for_did(did) {
        ChassisFamily::Boat => (boat::build(did), HoverBoatParams::default_config()),
        ChassisFamily::Airship => (airship::build(did), HelicopterParams::default_config()),
        ChassisFamily::Humanoid => (humanoid::build(did), humanoid_locomotion(did)),
        ChassisFamily::Skiff => (skiff::build(did), CarParams::default_config()),
    }
}

/// Humanoid locomotion tuned to the seeded body: the collider capsule
/// tracks the figure's height/build and the walk speed tracks the
/// seeded gait cadence (nominal 2.2 steps/s ↔ the preset's default
/// 4.0 m/s), so a long-legged strider actually covers ground faster
/// than a short-stepped walker.
fn humanoid_locomotion(did: &str) -> LocomotionConfig {
    let body = AvatarBody::for_did(did);
    let gait = AvatarGait::for_did(did);
    let mut p = HumanoidParams::default();
    let total_h = 1.70 * body.height_scale;
    p.capsule_radius = Fp(0.28 * body.shoulder_width_scale);
    p.capsule_length = Fp((total_h - 2.0 * p.capsule_radius.0).max(0.4));
    p.walk_speed = Fp(4.0 * (gait.step_cadence / 2.2));
    p.into_config()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::sanitize_avatar_visuals;

    fn family_dids() -> Vec<(ChassisFamily, String)> {
        // Hunt one DID per family so every builder is exercised.
        let mut found: Vec<(ChassisFamily, String)> = Vec::new();
        for s in 0u64..400 {
            let did = format!("did:test:{s}");
            let fam = ChassisFamily::for_did(&did);
            if !found.iter().any(|(f, _)| *f == fam) {
                found.push((fam, did));
            }
            if found.len() == 4 {
                break;
            }
        }
        assert_eq!(found.len(), 4, "couldn't find a DID for every family");
        found
    }

    #[test]
    fn deterministic_across_calls() {
        for (_, did) in family_dids() {
            let (a, la) = build_for_did(&did);
            let (b, lb) = build_for_did(&did);
            assert_eq!(a, b, "visuals must be bit-identical for {did}");
            assert_eq!(la, lb, "locomotion must be bit-identical for {did}");
        }
    }

    #[test]
    fn locomotion_matches_family() {
        for (fam, did) in family_dids() {
            let (_, loco) = build_for_did(&did);
            let tag = loco.kind_tag();
            let expected = match fam {
                ChassisFamily::Boat => "hover_boat",
                ChassisFamily::Airship => "helicopter",
                ChassisFamily::Humanoid => "humanoid",
                ChassisFamily::Skiff => "car",
            };
            assert_eq!(tag, expected, "family {fam:?} got locomotion {tag}");
        }
    }

    #[test]
    fn visuals_survive_sanitize_unchanged() {
        // The builders must emit records already inside every sanitiser
        // bound — if the sanitiser rewrites anything, a peer receiving
        // the record would see different geometry than the owner built.
        // Rotations are compared with an epsilon because the sanitiser
        // renormalises every quaternion, which can shift the last ulp
        // of an already-normalised rotation.
        fn assert_tree_eq(a: &Generator, b: &Generator, fam: ChassisFamily) {
            assert_eq!(a.kind, b.kind, "{fam:?}: kind rewritten by sanitiser");
            assert_eq!(
                a.transform.translation, b.transform.translation,
                "{fam:?}: translation rewritten"
            );
            assert_eq!(
                a.transform.scale, b.transform.scale,
                "{fam:?}: scale rewritten"
            );
            for i in 0..4 {
                assert!(
                    (a.transform.rotation.0[i] - b.transform.rotation.0[i]).abs() < 1e-5,
                    "{fam:?}: rotation rewritten beyond renormalisation: {:?} vs {:?}",
                    a.transform.rotation,
                    b.transform.rotation
                );
            }
            assert_eq!(a.children.len(), b.children.len(), "{fam:?}: child dropped");
            for (ca, cb) in a.children.iter().zip(b.children.iter()) {
                assert_tree_eq(ca, cb, fam);
            }
        }

        for (fam, did) in family_dids() {
            let (built, _) = build_for_did(&did);
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            assert_tree_eq(&built, &sanitized, fam);
        }
    }

    #[test]
    fn every_family_carries_a_pfp_banner() {
        use crate::pds::generator::GeneratorKind;
        fn has_sign(g: &Generator) -> bool {
            matches!(g.kind, GeneratorKind::Sign { .. }) || g.children.iter().any(has_sign)
        }
        for (fam, did) in family_dids() {
            let (built, _) = build_for_did(&did);
            assert!(has_sign(&built), "{fam:?} avatar lost its pfp banner");
        }
    }
}
