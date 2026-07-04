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
mod assemble;
mod boat;
pub(crate) mod common;
mod fx;
mod humanoid;
mod skiff;

use crate::pds::generator::Generator;
use crate::pds::types::Fp;
use crate::seeded_defaults::{
    AvatarBody, AvatarFx, AvatarGait, AvatarPalette, ChassisFamily, fnv1a_64,
};

use super::locomotion::{
    CarParams, HelicopterParams, HoverBoatParams, HumanoidParams, LocomotionConfig,
    LocomotionPreset,
};

/// Build the full seeded default avatar (visuals + locomotion) for a
/// DID. Deterministic: every peer derives the identical record.
pub fn build_for_did(did: &str) -> (Generator, LocomotionConfig) {
    build_for_seed(fnv1a_64(did), did)
}

/// Build from a pre-computed seed — the manual re-roll path. `seed`
/// chooses the chassis family and drives every derived value; `did` is
/// threaded through only for identity references (the pfp banner) that
/// must survive a re-roll. `build_for_did(did)` is exactly
/// `build_for_seed(fnv1a_64(did), did)`.
pub fn build_for_seed(seed: u64, did: &str) -> (Generator, LocomotionConfig) {
    let family = ChassisFamily::for_seed(seed);
    let (mut visuals, loco) = match family {
        ChassisFamily::Boat => (boat::build(seed, did), HoverBoatParams::default_config()),
        ChassisFamily::Airship => (
            airship::build(seed, did),
            HelicopterParams::default_config(),
        ),
        ChassisFamily::Humanoid => (humanoid::build(seed, did), humanoid_locomotion(seed)),
        ChassisFamily::Skiff => (skiff::build(seed, did), CarParams::default_config()),
    };
    // Seeded FX: hang the style's signature particle aura + body voice on the
    // built root. The mount is a coarse per-family body centre; the part
    // catalogue could later let each part mount its own FX precisely.
    let fx = AvatarFx::for_seed(seed);
    let accent = AvatarPalette::for_seed(seed).primary_accent;
    fx::attach(&mut visuals, &fx, fx_mount(family), accent, seed);
    (visuals, loco)
}

/// Diegetic per-family FX mount (root-local frame, *before* the assembler's
/// yaw/drop). Vehicles author their stern at local -Z, so a rear mount rides
/// behind the craft once the 180° travel-facing yaw is applied.
fn fx_mount(family: ChassisFamily) -> [f32; 3] {
    match family {
        // A tight aura around the torso (chest height), not floating overhead.
        ChassisFamily::Humanoid => [0.0, 0.45, 0.0],
        // Stern wake, low and aft.
        ChassisFamily::Boat => [0.0, 0.1, -0.8],
        // Vents beneath the slung gondola.
        ChassisFamily::Airship => [0.0, -1.25, 0.0],
        // Exhaust at the rear, low to the ground.
        ChassisFamily::Skiff => [0.0, 0.1, -0.85],
    }
}

/// Humanoid locomotion tuned to the seeded body: the collider capsule
/// tracks the figure's height/build and the walk speed tracks the
/// seeded gait cadence (nominal 2.2 steps/s ↔ the preset's default
/// 4.0 m/s), so a long-legged strider actually covers ground faster
/// than a short-stepped walker.
fn humanoid_locomotion(seed: u64) -> LocomotionConfig {
    let body = AvatarBody::for_seed(seed);
    let gait = AvatarGait::for_seed(seed);
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

    /// The DID path must be exactly the seed path fed the hashed DID —
    /// this is the contract that lets `build_for_did` keep working
    /// untouched while the manual re-roll uses `build_for_seed`.
    #[test]
    fn build_for_did_equals_build_for_seed_of_hashed_did() {
        for (_, did) in family_dids() {
            let (va, la) = build_for_did(&did);
            let (vb, lb) = build_for_seed(fnv1a_64(&did), &did);
            assert_eq!(
                va, vb,
                "visuals diverged from the hashed-DID seed for {did}"
            );
            assert_eq!(
                la, lb,
                "locomotion diverged from the hashed-DID seed for {did}"
            );
        }
    }

    #[test]
    fn build_for_seed_is_deterministic() {
        let (a, la) = build_for_seed(0xC0FF_EE12_3456_789A, "did:plc:reroll");
        let (b, lb) = build_for_seed(0xC0FF_EE12_3456_789A, "did:plc:reroll");
        assert_eq!(a, b);
        assert_eq!(la, lb);
    }

    /// Seeded FX must actually land on the tree: a seed whose anchor rolls a
    /// signature aura grows a `ParticleSystem` node, and a seed with a voice
    /// sets the root audio. Proves the [`fx::attach`] wiring, not just the
    /// spec deriver.
    #[test]
    fn seeded_fx_attaches_emitter_and_voice() {
        use crate::pds::generator::GeneratorKind;
        use crate::seeded_defaults::{AvatarFx, AvatarVoice, ParticleAura};
        fn has_particles(g: &Generator) -> bool {
            matches!(g.kind, GeneratorKind::ParticleSystem(..))
                || g.children.iter().any(has_particles)
        }

        // Hunt a seed with a non-None aura and one with a non-None voice.
        let mut aura_seed = None;
        let mut voice_seed = None;
        for s in 0u64..400 {
            let fx = AvatarFx::for_seed(s);
            if aura_seed.is_none() && fx.aura != ParticleAura::None {
                aura_seed = Some(s);
            }
            if voice_seed.is_none() && fx.voice != AvatarVoice::None {
                voice_seed = Some(s);
            }
            if aura_seed.is_some() && voice_seed.is_some() {
                break;
            }
        }
        let aura_seed = aura_seed.expect("no seed rolled a particle aura");
        let voice_seed = voice_seed.expect("no seed rolled a voice");

        let (built, _) = build_for_seed(aura_seed, "did:plc:fx");
        assert!(
            has_particles(&built),
            "aura seed {aura_seed} grew no ParticleSystem"
        );

        let (built, _) = build_for_seed(voice_seed, "did:plc:fx");
        assert!(
            !matches!(built.audio, crate::pds::SovereignAudioConfig::None),
            "voice seed {voice_seed} set no body audio"
        );
    }

    #[test]
    fn distinct_seeds_yield_distinct_avatars() {
        // A re-roll must actually change the look (same DID, new seed).
        let (a, _) = build_for_seed(1, "did:plc:reroll");
        let (b, _) = build_for_seed(2, "did:plc:reroll");
        assert_ne!(a, b, "re-roll produced an identical avatar for two seeds");
    }

    /// Re-rolling changes the look but not *whose* avatar it is: the pfp
    /// banner's DID is threaded straight through and must be independent
    /// of the seed.
    #[test]
    fn pfp_banner_did_is_seed_independent() {
        use crate::pds::generator::{GeneratorKind, SignSource};
        fn find_pfp_did(g: &Generator) -> Option<&str> {
            if let GeneratorKind::Sign {
                source: SignSource::DidPfp { did },
                ..
            } = &g.kind
            {
                return Some(did);
            }
            g.children.iter().find_map(find_pfp_did)
        }
        let did = "did:plc:identity";
        for seed in [1u64, 7, 999_999, u64::MAX] {
            let (built, _) = build_for_seed(seed, did);
            assert_eq!(
                find_pfp_did(&built),
                Some(did),
                "pfp banner lost the owner DID at seed {seed}"
            );
        }
    }
}
