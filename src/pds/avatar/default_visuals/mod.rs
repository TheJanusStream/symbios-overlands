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
//! primitive/material vocabulary lives in [`common`].

mod airship;
mod assemble;
mod boat;
pub(crate) mod common;
mod fx;
mod humanoid;
mod skiff;

use crate::pds::avatar::parts::PartSlot;
use crate::pds::generator::Generator;
use crate::pds::types::{Fp, Fp3};
use crate::seeded_defaults::{
    AvatarBody, AvatarFx, AvatarGait, AvatarOutfit, AvatarPalette, ChassisFamily, ParticleAura,
    VehicleBlueprint, fnv1a_64,
};

use super::locomotion::{
    CarParams, HelicopterParams, HoverBoatParams, HumanoidParams, LocomotionConfig,
    LocomotionPreset,
};

/// Build the full seeded default avatar (visuals + locomotion) for a
/// DID. Deterministic: every peer derives the identical record.
pub fn build_for_did(did: &str) -> (Generator, LocomotionConfig) {
    build_for_seed(fnv1a_64(did))
}

/// Build from a pre-computed seed — the manual re-roll path. `seed`
/// chooses the chassis family and drives every derived value.
/// `build_for_did(did)` is exactly `build_for_seed(fnv1a_64(did))`.
/// (Avatars no longer wear a pfp identity sign — #733 removed the
/// chest-badge / hull-decal / bow-crest panels from every chassis.)
pub fn build_for_seed(seed: u64) -> (Generator, LocomotionConfig) {
    let family = ChassisFamily::for_seed(seed);
    let (mut visuals, loco) = match family {
        ChassisFamily::Boat => (boat::build(seed), boat_locomotion(seed)),
        ChassisFamily::Airship => (airship::build(seed), airship_locomotion(seed)),
        ChassisFamily::Humanoid => (humanoid::build(seed), humanoid_locomotion(seed)),
        ChassisFamily::Skiff => (skiff::build(seed), skiff_locomotion(seed)),
    };
    // Seeded FX: hang the style's signature particle aura (floored to the
    // chassis wake / vent / exhaust) + body voice on the built root. The mount
    // is snapped to the seeded blueprint landmark for the aura — a boat's steam
    // leaves its funnel, its wake rides the stern — via [`fx_mount`].
    let fx = AvatarFx::for_seed(seed);
    let accent = AvatarPalette::for_seed(seed).primary_accent;
    fx::attach(
        &mut visuals,
        &fx,
        fx_mount(fx.aura, family, seed),
        accent,
        family,
        seed,
    );
    (visuals, loco)
}

/// Diegetic FX mount for `aura` on `family` (root-local frame, *before* the
/// assembler's yaw/drop). The station is snapped to the seeded blueprint
/// landmarks the assembler already mounts parts on — so the emitter tracks the
/// actual hull instead of a fixed constant, and a boat's steam leaves its
/// funnel rather than empty air amidships. Falls back to the legacy per-family
/// constant if the blueprint is unavailable (never for a real vehicle).
///
/// Vehicles author their stern at local `-Z`, so an aft mount rides behind the
/// craft once the 180° travel-facing yaw is applied.
fn fx_mount(aura: ParticleAura, family: ChassisFamily, seed: u64) -> [f32; 3] {
    let bp = VehicleBlueprint::from_seed(seed);
    match family {
        // A tight aura around the torso (chest height), not floating overhead.
        ChassisFamily::Humanoid => [0.0, 0.45, 0.0],
        ChassisFamily::Boat => match bp.as_ref().and_then(VehicleBlueprint::boat) {
            // Steam vents from the funnel (the Stack station, up off the deck)
            // — but only when a funnel was actually rolled: the Stack slot is
            // optional (ornateness-gated), so a stackless steam boat would
            // otherwise plume from empty air. Without a funnel it falls back to
            // the low stern, reading as engine spray like the wake does.
            Some(b) if aura == ParticleAura::Steam && boat_has_stack(seed) => {
                [0.0, b.deck_y * 0.62 + 0.5, b.stack_z]
            }
            Some(b) if matches!(aura, ParticleAura::Steam | ParticleAura::Wake) => {
                [0.0, 0.08, -b.hull_len * 0.5]
            }
            // Drifting motes ride the amidships deck line.
            Some(b) => [0.0, b.deck_y * 1.3, 0.0],
            None => [0.0, 0.1, -0.8],
        },
        // Vents / thruster wash / motes all issue from beneath the slung
        // gondola — the assembler's belly line, tracking the chosen envelope.
        ChassisFamily::Airship => airship::fx_belly_anchor(seed),
        ChassisFamily::Skiff => match bp.as_ref().and_then(VehicleBlueprint::skiff) {
            // Exhaust / steam leave the tailpipe (the Exhaust station aft-low,
            // matching the assembler); decorative motes hover over the body.
            Some(s) if matches!(aura, ParticleAura::Exhaust | ParticleAura::Steam) => {
                [0.0, 0.05, -0.70 * (s.body_len / 1.5)]
            }
            Some(_) => [0.0, 0.3, 0.0],
            None => [0.0, 0.1, -0.85],
        },
    }
}

/// Whether this seed's boat rolled a `Stack` (funnel / vent) part — the
/// diegetic source a steam plume can sit atop. The `Stack` slot is optional,
/// so a plain boat may have no funnel at all.
fn boat_has_stack(seed: u64) -> bool {
    AvatarOutfit::for_seed(seed)
        .parts
        .iter()
        .any(|p| p.slot == PartSlot::Stack)
}

/// The slug of the part filling `slot` in this seed's outfit (the discrete
/// hull / envelope / chassis *class* — barge vs catamaran, twin vs zeppelin,
/// armored vs dune — which is a part slug, not an enum), or `""` if unfilled.
fn structural_slug(outfit: &AvatarOutfit, slot: PartSlot) -> &'static str {
    outfit
        .parts
        .iter()
        .find(|p| p.slot == slot)
        .map_or("", |p| p.slug)
}

/// Humanoid locomotion tuned to the seeded body: the collider capsule
/// tracks the figure's height/build and the walk speed tracks the
/// seeded gait cadence (nominal 2.2 steps/s ↔ the preset's default
/// 4.0 m/s), so a long-legged strider actually covers ground faster
/// than a short-stepped walker.
fn humanoid_locomotion(seed: u64) -> LocomotionConfig {
    let body = AvatarBody::for_seed(seed);
    let bp = crate::seeded_defaults::HumanoidBlueprint::from_body(&body);
    let gait = AvatarGait::for_seed(seed);
    let mut p = HumanoidParams::default();
    // The capsule tracks the blueprint exactly: a Toy avatar is genuinely
    // small in-world, a Heroic one genuinely tall. Radius hugs the figure's
    // widest mass (shoulders + splayed arms).
    let total_h = bp.total_h;
    p.capsule_radius = Fp(((bp.shoulder_x + 2.0 * bp.arm_r) * 0.95).clamp(0.18, 0.34));
    p.capsule_length = Fp((total_h - 2.0 * p.capsule_radius.0).max(0.4));
    p.walk_speed = Fp(4.0 * (gait.step_cadence / 2.2));
    p.into_config()
}

// ---------------------------------------------------------------------------
// Seeded vehicle locomotion (#794)
//
// Every vehicle family used to share one un-seeded `default_config()`, and the
// baselines inverted the visual story: `HoverBoatParams::default` was the
// legacy 50 kg rover tuning (drive 1800 N → 36 m/s²), so the barge
// out-accelerated the 900 kg skiff four-to-one. These derive mass + forces
// from the picked hull / envelope / chassis *class* and the seeded blueprint
// dimensions, keeping the drive **acceleration** inside a tuned feel band by
// construction (`force = mass · target_accel`) — so a heavy barge is genuinely
// ponderous and a catamaran genuinely nimble, but nothing is undriveable. The
// support invariants are honoured: the hover-boat's suspension spring +
// buoyancy and the helicopter's `hover_thrust` all scale with the seeded mass
// so the craft sits at the same ride height it always did. Every value stays
// inside the locomotion sanitiser's clamps so the record round-trips unchanged.
//
// The **airplane** preset is a deliberate orphan: `ChassisFamily` has no
// `Airplane` variant, so no seed ever produces one — it is reachable only by a
// user manually picking it in the avatar editor (picker-only), and keeps its
// plain `default_config`. A fixed-wing visual family is out of scope here.
// ---------------------------------------------------------------------------

/// Boat (hover-boat) locomotion from the seeded hull class + proportions.
/// Barge = heavy + damped + sluggish; catamaran = light + agile; mono /
/// trimaran sit between. The suspension spring, buoyancy and lateral grip
/// scale with the derived mass so the hull keeps its hover ride height.
fn boat_locomotion(seed: u64) -> LocomotionConfig {
    let outfit = AvatarOutfit::for_seed(seed);
    let bp = VehicleBlueprint::from_seed(seed);
    let b = bp.as_ref().and_then(VehicleBlueprint::boat);
    // (mass factor over the 50 kg baseline, drive accel, turn accel, linear
    // damping, angular damping) per hull class.
    let (mass_f, drive_accel, turn_accel, lin_damp, ang_damp) =
        match structural_slug(&outfit, PartSlot::Hull) {
            "default_hull_barge" => (8.0, 6.5, 4.0, 2.2, 8.0),
            "default_hull_catamaran" => (2.4, 13.0, 10.0, 1.0, 4.0),
            "default_hull_trimaran" => (3.2, 10.5, 8.0, 1.3, 5.0),
            _ => (4.0, 9.0, 7.0, 1.5, 6.0), // monohull / fallback
        };
    let hull_len = b.map_or(1.32, |b| b.hull_len);
    let beam = b.map_or(0.5, |b| b.beam);
    let freeboard = b.map_or(0.26, |b| b.freeboard);

    // The 50 kg baseline is what the default suspension stiffness (4200) and
    // buoyancy (2500) hold at the stock ride height; scaling both by `mass/50`
    // keeps that height as mass grows. The clamp keeps the scaled stiffness
    // under its 50 000 sanitiser cap.
    const REF_MASS: f32 = 50.0;
    let mut p = HoverBoatParams::default();
    let mass = (REF_MASS * mass_f * (hull_len / 1.32)).clamp(80.0, 480.0);
    let scale = mass / REF_MASS;
    // Scale a support field by mass and keep it under its sanitiser cap.
    let scaled = |v: f32, cap: f32| Fp((v * scale).min(cap));
    p.mass = Fp(mass);
    p.drive_force = Fp((mass * drive_accel).min(50_000.0));
    p.turn_torque = Fp((mass * turn_accel).min(50_000.0));
    p.linear_damping = Fp(lin_damp);
    p.angular_damping = Fp(ang_damp);
    p.suspension_stiffness = scaled(p.suspension_stiffness.0, 48_000.0);
    p.suspension_damping = scaled(p.suspension_damping.0, 5_000.0);
    p.buoyancy_strength = scaled(p.buoyancy_strength.0, 90_000.0);
    p.buoyancy_damping = scaled(p.buoyancy_damping.0, 10_000.0);
    p.lateral_grip = scaled(p.lateral_grip.0, 48_000.0);
    p.chassis_half_extents = fit_extents([beam * 0.5, freeboard * 0.6, hull_len * 0.5]);
    p.into_config()
}

/// Airship (helicopter) locomotion from the seeded envelope class + girth.
/// A twin-hull envelope carries more mass and angular damping (harder to spin
/// up); a fat blimp is more ponderous than a slim zeppelin. `hover_thrust` is
/// re-derived as `mass · 9.81` so a fresh airship still floats at idle.
fn airship_locomotion(seed: u64) -> LocomotionConfig {
    let outfit = AvatarOutfit::for_seed(seed);
    let a = VehicleBlueprint::from_seed(seed).and_then(|b| b.airship().copied());
    // (mass factor over the 60 kg baseline, drive accel, yaw accel, angular
    // damping) per envelope class.
    let (mass_f, drive_accel, yaw_accel, ang_damp) =
        match structural_slug(&outfit, PartSlot::Envelope) {
            "default_envelope_twin" => (1.6, 6.0, 5.0, 6.0),
            "default_envelope_blimp" => (1.3, 6.5, 7.0, 4.5),
            "default_envelope_lobed" => (1.1, 8.0, 7.0, 4.0),
            _ => (1.0, 8.0, 7.0, 4.0), // zeppelin / teardrop / fallback
        };
    let (len_mult, radius_mult) = a.map_or((1.0, 1.0), |a| (a.len_mult, a.radius_mult));

    let mut p = HelicopterParams::default();
    // Envelope displacement ≈ length × girth²; that sets the lift-gas mass.
    let mass = (60.0 * mass_f * len_mult * radius_mult * radius_mult).clamp(60.0, 300.0);
    p.mass = Fp(mass);
    // The weight-support invariant (helicopter.rs): hover_thrust cancels
    // gravity at idle. It MUST track the seeded mass or the craft sinks/climbs.
    p.hover_thrust = Fp(mass * 9.81);
    p.cyclic_force = Fp((mass * drive_accel).min(50_000.0));
    p.strafe_force = Fp((mass * drive_accel * 0.9).min(50_000.0));
    p.yaw_torque = Fp((mass * yaw_accel).min(50_000.0));
    p.angular_damping = Fp(ang_damp);
    p.chassis_half_extents = fit_extents([0.7 * radius_mult, 0.6 * radius_mult, 1.4 * len_mult]);
    p.into_config()
}

/// Skiff (car) locomotion from the seeded chassis class + body size. The
/// armored hull is heavy + planted; the dune buggy / trike are light + nimble;
/// the default chassis keeps roughly the stock 900 kg / 8 000 N feel. The
/// suspension + grip scale with mass so the ride height holds.
fn skiff_locomotion(seed: u64) -> LocomotionConfig {
    let outfit = AvatarOutfit::for_seed(seed);
    let s = VehicleBlueprint::from_seed(seed).and_then(|b| b.skiff().copied());
    // (mass factor over the 900 kg baseline, drive accel, turn accel) per
    // chassis class.
    let (mass_f, drive_accel, turn_accel) = match structural_slug(&outfit, PartSlot::Chassis) {
        "skiff_chassis_armored" => (1.55, 6.5, 1.6),
        "skiff_chassis_dune" => (0.62, 11.0, 2.6),
        "skiff_chassis_trike" => (0.6, 11.5, 2.8),
        _ => (1.0, 8.9, 2.0), // default_chassis / fallback
    };
    let body_len = s.map_or(1.5, |s| s.body_len);
    let body_w = s.map_or(0.76, |s| s.body_w);

    const REF_MASS: f32 = 900.0;
    let mut p = CarParams::default();
    let mass = (REF_MASS * mass_f * (body_len / 1.5)).clamp(480.0, 1_500.0);
    let scale = mass / REF_MASS;
    // Scale a support field by mass and keep it under its sanitiser cap.
    let scaled = |v: f32, cap: f32| Fp((v * scale).min(cap));
    p.mass = Fp(mass);
    p.drive_force = Fp((mass * drive_accel).min(200_000.0));
    p.turn_torque = Fp((mass * turn_accel).min(50_000.0));
    p.suspension_stiffness = scaled(p.suspension_stiffness.0, 200_000.0);
    p.suspension_damping = scaled(p.suspension_damping.0, 20_000.0);
    p.lateral_grip = scaled(p.lateral_grip.0, 200_000.0);
    p.chassis_half_extents = fit_extents([body_w * 0.5, 0.4 * (body_len / 1.5), body_len * 0.5]);
    p.into_config()
}

/// Clamp a raw `[x, y, z]` half-extent to the collider sanitiser's per-axis
/// bounds (`0.05..50`) so the derived cuboid round-trips unchanged and never
/// degenerates to a zero-thickness box.
fn fit_extents(raw: [f32; 3]) -> Fp3 {
    Fp3([
        raw[0].clamp(0.05, 50.0),
        raw[1].clamp(0.05, 50.0),
        raw[2].clamp(0.05, 50.0),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::LocomotionConfig;

    /// The first seed of each chassis family whose outfit rolls the given
    /// structural class slug, searching a wide seed range.
    fn seed_for_class(fam: ChassisFamily, slot: PartSlot, slug: &str) -> Option<u64> {
        (0u64..2000).find(|&s| {
            ChassisFamily::for_seed(s) == fam
                && structural_slug(&AvatarOutfit::for_seed(s), slot) == slug
        })
    }

    /// Drive acceleration (drive force / mass, m/s²) of a vehicle preset.
    fn drive_accel(loco: &LocomotionConfig) -> f32 {
        match loco {
            LocomotionConfig::HoverBoat(b) => b.drive_force.0 / b.mass.0,
            LocomotionConfig::Car(c) => c.drive_force.0 / c.mass.0,
            LocomotionConfig::Helicopter(h) => h.cyclic_force.0 / h.mass.0,
            _ => panic!("not a vehicle preset"),
        }
    }

    /// The inverted mass story is fixed: the barge (which used to run the
    /// 50 kg rover tuning at 36 m/s²) is now the most ponderous vehicle, the
    /// catamaran the nimblest, and no boat out-accelerates the skiff the way
    /// the survey found (barge 4× the 900 kg skiff).
    #[test]
    fn mass_story_is_no_longer_inverted() {
        let barge = seed_for_class(ChassisFamily::Boat, PartSlot::Hull, "default_hull_barge")
            .expect("no barge seed");
        let cat = seed_for_class(
            ChassisFamily::Boat,
            PartSlot::Hull,
            "default_hull_catamaran",
        )
        .expect("no catamaran seed");
        let skiff = seed_for_class(ChassisFamily::Skiff, PartSlot::Chassis, "default_chassis")
            .expect("no default-skiff seed");

        let barge_a = drive_accel(&build_for_seed(barge).1);
        let cat_a = drive_accel(&build_for_seed(cat).1);
        let skiff_a = drive_accel(&build_for_seed(skiff).1);

        assert!(
            barge_a < cat_a,
            "barge ({barge_a}) should be more sluggish than the catamaran ({cat_a})"
        );
        assert!(
            barge_a <= skiff_a,
            "the barge ({barge_a}) must not out-accelerate the skiff ({skiff_a})"
        );
    }

    /// Every derived drive acceleration lands in a tuned, driveable band —
    /// nothing is a 36 m/s² rocket or an undriveable brick.
    #[test]
    fn every_vehicle_drive_accel_is_in_the_feel_band() {
        for s in 0u64..600 {
            let (_, loco) = build_for_seed(s);
            if matches!(loco, LocomotionConfig::Humanoid(_)) {
                continue;
            }
            let a = drive_accel(&loco);
            assert!(
                (5.0..=14.0).contains(&a),
                "seed {s} drive accel {a} out of the feel band"
            );
        }
    }

    /// The helicopter weight-support invariant: `hover_thrust ≈ mass · 9.81`,
    /// so a fresh airship floats at idle regardless of its seeded mass.
    #[test]
    fn airship_hover_thrust_cancels_gravity() {
        let mut checked = 0;
        for s in 0u64..400 {
            let (_, loco) = build_for_seed(s);
            if let LocomotionConfig::Helicopter(h) = loco {
                assert!(
                    (h.hover_thrust.0 - h.mass.0 * 9.81).abs() < 1.0,
                    "airship seed {s}: hover_thrust {} != mass·g {}",
                    h.hover_thrust.0,
                    h.mass.0 * 9.81
                );
                checked += 1;
            }
        }
        assert!(checked > 0, "no airship seed exercised the invariant");
    }

    /// Every seeded vehicle locomotion must already sit inside the sanitiser's
    /// clamps — else a peer receiving the record would drive different physics
    /// than the owner built (the locomotion analogue of the visuals round-trip).
    #[test]
    fn vehicle_locomotion_survives_sanitize_unchanged() {
        for s in 0u64..400 {
            let (_, loco) = build_for_seed(s);
            let mut sanitized = loco.clone();
            sanitized.sanitize();
            assert_eq!(
                loco, sanitized,
                "seed {s} locomotion was rewritten by the sanitiser"
            );
        }
    }

    /// The seeded engine voice (and any node audio) must survive the
    /// sanitiser unchanged — the `visuals_survive_sanitize_unchanged` tree
    /// comparison skips the `audio` field, so a voice whose freqs / gains fell
    /// outside the audio clamps would rewrite the record without that test
    /// noticing (#796).
    #[test]
    fn seeded_audio_survives_sanitize_unchanged() {
        fn collect_audio(g: &Generator, out: &mut Vec<crate::pds::SovereignAudioConfig>) {
            out.push(g.audio.clone());
            for c in &g.children {
                collect_audio(c, out);
            }
        }
        for s in 0u64..400 {
            let (built, _) = build_for_seed(s);
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            let (mut a, mut b) = (Vec::new(), Vec::new());
            collect_audio(&built, &mut a);
            collect_audio(&sanitized, &mut b);
            assert_eq!(a, b, "seed {s} audio was rewritten by the sanitiser");
        }
    }

    /// A re-roll changes the drive feel: two boats of different hull classes
    /// no longer share one bit-identical config.
    #[test]
    fn distinct_hull_classes_drive_differently() {
        let barge = seed_for_class(ChassisFamily::Boat, PartSlot::Hull, "default_hull_barge")
            .expect("no barge seed");
        let cat = seed_for_class(
            ChassisFamily::Boat,
            PartSlot::Hull,
            "default_hull_catamaran",
        )
        .expect("no catamaran seed");
        assert_ne!(build_for_seed(barge).1, build_for_seed(cat).1);
    }
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
    fn no_family_carries_a_pfp_sign() {
        // #733 removed the identity signs (chest badge / hull decal / bow
        // crest) from every chassis — pin the removal so a future part
        // can't quietly reintroduce one.
        use crate::pds::generator::GeneratorKind;
        fn has_sign(g: &Generator) -> bool {
            matches!(g.kind, GeneratorKind::Sign { .. }) || g.children.iter().any(has_sign)
        }
        for (fam, did) in family_dids() {
            let (built, _) = build_for_did(&did);
            assert!(!has_sign(&built), "{fam:?} avatar still carries a sign");
        }
    }

    /// A steam boat's plume only mounts at the funnel when a funnel was
    /// actually rolled; a stackless steam boat falls back to the low stern so
    /// the steam never issues from empty air above the deck (#795 review).
    #[test]
    fn steam_boat_mount_tracks_the_funnel_presence() {
        let (mut with_stack, mut without_stack) = (None, None);
        for s in 0u64..800 {
            if ChassisFamily::for_seed(s) != ChassisFamily::Boat {
                continue;
            }
            if AvatarFx::for_seed(s).aura != ParticleAura::Steam {
                continue;
            }
            if boat_has_stack(s) {
                with_stack.get_or_insert(s);
            } else {
                without_stack.get_or_insert(s);
            }
            if with_stack.is_some() && without_stack.is_some() {
                break;
            }
        }
        let with_stack = with_stack.expect("no steam boat with a funnel found");
        let without_stack = without_stack.expect("no stackless steam boat found");

        let funnel = fx_mount(ParticleAura::Steam, ChassisFamily::Boat, with_stack);
        let stern = fx_mount(ParticleAura::Steam, ChassisFamily::Boat, without_stack);
        assert!(
            funnel[1] > 0.4,
            "steam should vent high off the funnel (seed {with_stack}, y={})",
            funnel[1]
        );
        assert!(
            stern[1] < 0.2 && stern[2] < 0.0,
            "stackless steam should sit low and aft, not float above the deck \
             (seed {without_stack}, mount={stern:?})"
        );
    }

    /// The DID path must be exactly the seed path fed the hashed DID —
    /// this is the contract that lets `build_for_did` keep working
    /// untouched while the manual re-roll uses `build_for_seed`.
    #[test]
    fn build_for_did_equals_build_for_seed_of_hashed_did() {
        for (_, did) in family_dids() {
            let (va, la) = build_for_did(&did);
            let (vb, lb) = build_for_seed(fnv1a_64(&did));
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
        let (a, la) = build_for_seed(0xC0FF_EE12_3456_789A);
        let (b, lb) = build_for_seed(0xC0FF_EE12_3456_789A);
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

        let (built, _) = build_for_seed(aura_seed);
        assert!(
            has_particles(&built),
            "aura seed {aura_seed} grew no ParticleSystem"
        );

        let (built, _) = build_for_seed(voice_seed);
        assert!(
            !matches!(built.audio, crate::pds::SovereignAudioConfig::None),
            "voice seed {voice_seed} set no body audio"
        );
    }

    #[test]
    fn distinct_seeds_yield_distinct_avatars() {
        // A re-roll must actually change the look.
        let (a, _) = build_for_seed(1);
        let (b, _) = build_for_seed(2);
        assert_ne!(a, b, "re-roll produced an identical avatar for two seeds");
    }
}
