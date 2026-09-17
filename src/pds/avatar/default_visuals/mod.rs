//! Per-family default avatar builders.
//!
//! [`build_for_did`] is the single entry point the record layer calls:
//! it resolves the DID's [`ChassisFamily`] and dispatches to that
//! family's builder, returning both halves of the avatar record - the
//! body and a locomotion preset that *matches* it (boat → HoverBoat,
//! airship → Helicopter, humanoid → Humanoid, skiff → Car), so the
//! default chassis drives the way it looks.
//!
//! **The humanoid family is rigged now** (#1060, epic #1054). The three
//! vehicle families still assemble a [`crate::pds::Generator`] tree out of the tagged
//! part catalogue, one file per family; a humanoid instead resolves to a
//! parametric `symbios-avatar` body rolled from the same seed. That body
//! is filled in **locally** rather than fetched: the engine's roll is
//! deterministic, so every peer derives the same person for a DID with
//! nothing on the wire and no PDS round trip - the same promise the
//! generator families always kept. The wardrobe record key comes from
//! [`crate::pds::tid::tid_for_seed`] so two devices that both save a
//! never-edited seeded default agree on where it goes.
//!
//! **Boats and skiffs are drawn at airship class** (#1361, owner decision 1 of
//! the #1359 redesign): their parts are still authored around the old 1.32 m /
//! 1.5 m nominals, and each assembler puts one uniform scale on the visual root
//! to bring them up to 2.8 m / 2.65 m. That bridge is throwaway - it dies with
//! each legacy pipeline as the hero craft land - but the consequence is not:
//! everything derived here from a vehicle's size (mass, collider, ride height,
//! travel-pose drop, particle sprites) reads the **drawn** dimensions, never the
//! authored ones, and compares them against a named true nominal.
//!
//! Shared primitive/material vocabulary lives in [`common`].

mod airship;
mod assemble;
mod boat;
pub(crate) mod common;
mod fx;
mod skiff;

use crate::pds::avatar::parts::PartSlot;
// Aliased: `crate::seeded_defaults::AvatarBody` (imported below) is the
// seeded *proportions* anchor, a different thing with the same name.
use crate::pds::avatar::body::AvatarBody as RecordBody;
use crate::pds::types::{Fp, Fp3};
use crate::seeded_defaults::{
    AvatarFx, AvatarGait, AvatarOutfit, AvatarPalette, ChassisFamily, ParticleAura,
    VehicleBlueprint, fnv1a_64,
};

use super::locomotion::{
    CarParams, HelicopterParams, HoverBoatParams, HumanoidParams, LocomotionConfig,
    LocomotionPreset,
};

/// Build the full seeded default avatar (body + locomotion) for a
/// DID. Deterministic: every peer derives the identical record.
pub fn build_for_did(did: &str) -> (RecordBody, LocomotionConfig) {
    build_for_seed(fnv1a_64(did))
}

/// Build from a pre-computed seed - the manual re-roll path. `seed`
/// chooses the chassis family and drives every derived value.
/// `build_for_did(did)` is exactly `build_for_seed(fnv1a_64(did))`.
/// (Avatars no longer wear a pfp identity sign - #733 removed the
/// chest-badge / hull-decal / bow-crest panels from every chassis.)
pub fn build_for_seed(seed: u64) -> (RecordBody, LocomotionConfig) {
    let family = ChassisFamily::for_seed(seed);
    // The rigged family short-circuits the whole part-assembly pipeline:
    // there is no tree to compose, no FX mount to snap to a blueprint
    // landmark, and no sanitiser pass to owe - the engine record IS the
    // body, and the skinned build happens at spawn (#1057).
    if family == ChassisFamily::Humanoid {
        return (RecordBody::rigged_seeded(seed), humanoid_locomotion(seed));
    }
    // The third element is the family's uniform visual-root scale (#1361) -
    // the airship-class bridge. The FX attached below need it because a
    // particle sprite is sized in world metres, not in the emitter's frame.
    let (mut visuals, loco, visual_scale) = match family {
        ChassisFamily::Boat => (boat::build(seed), boat_locomotion(seed), boat::VISUAL_SCALE),
        ChassisFamily::Airship => (airship::build(seed), airship_locomotion(seed), 1.0),
        ChassisFamily::Skiff => (
            skiff::build(seed),
            skiff_locomotion(seed),
            skiff::VISUAL_SCALE,
        ),
        // Handled above; a family added later lands here loudly rather
        // than silently assembling nothing.
        ChassisFamily::Humanoid => unreachable!("the rigged family returns above"),
    };
    // Seeded FX: hang the style's signature particle aura (floored to the
    // chassis wake / vent / exhaust) + body voice on the built root. The mount
    // is snapped to the seeded blueprint landmark for the aura - a boat's steam
    // leaves its funnel, its wake rides the stern - via [`fx_mount`].
    let fx = AvatarFx::for_seed(seed);
    let accent = AvatarPalette::for_seed(seed).primary_accent;
    fx::attach(
        &mut visuals,
        &fx,
        fx_mount(fx.aura, family, seed),
        accent,
        family,
        seed,
        visual_scale,
    );
    (RecordBody::generator(visuals), loco)
}

/// How tall the engine body this seed rolls actually stands, in metres.
///
/// Read off the archetype's own stature axis rather than by building the
/// body: a record's height axis IS the nominal stature, and meshing a body
/// to measure one would cost a quarter-second per seeded default.
fn engine_stature(seed: u64) -> f32 {
    let record = super::wardrobe::engine_default_for_seed(seed);
    match &record.archetype {
        symbios_avatar::Archetype::Humanoid(params) => params.height,
        symbios_avatar::Archetype::Quadruped(params) => params.height,
        // A body plan this build cannot read gets the canon figure, which
        // is what the preset's own default collider was cut for.
        symbios_avatar::Archetype::Unknown { .. } => 1.7,
    }
}

/// Diegetic FX mount for `aura` on `family` (root-local frame, *before* the
/// assembler's yaw, drop and [scale](boat::VISUAL_SCALE) - so these are
/// authoring-frame metres, and they grow with the craft). The station is
/// snapped to the seeded blueprint
/// landmarks the assembler already mounts parts on - so the emitter tracks the
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
            // Steam vents from the funnel (the shared Stack station, raised to
            // the funnel mouth) - but only when a funnel was actually rolled:
            // the Stack slot is optional (ornateness-gated), so a stackless
            // steam boat would otherwise plume from empty air. Without a funnel
            // it falls back to the low stern, reading as engine spray like the
            // wake does.
            Some(b) if aura == ParticleAura::Steam && boat_has_stack(seed) => {
                let mut m = boat::stack_station(b.deck_y, b.stack_z);
                m[1] += boat::FUNNEL_MOUTH_RISE;
                m
            }
            Some(b) if matches!(aura, ParticleAura::Steam | ParticleAura::Wake) => {
                [0.0, 0.08, -b.hull_len * 0.5]
            }
            // Drifting motes ride the amidships deck line.
            Some(b) => [0.0, b.deck_y * 1.3, 0.0],
            None => [0.0, 0.1, -0.8],
        },
        // Vents / thruster wash / motes all issue from beneath the slung
        // gondola - the assembler's belly line, tracking the chosen envelope.
        ChassisFamily::Airship => airship::fx_belly_anchor(seed),
        ChassisFamily::Skiff => match bp.as_ref().and_then(VehicleBlueprint::skiff) {
            // Exhaust / steam leave the tailpipe (the shared Exhaust station,
            // matching the assembler); decorative motes hover over the body.
            Some(s) if matches!(aura, ParticleAura::Exhaust | ParticleAura::Steam) => {
                skiff::exhaust_station(s.body_len)
            }
            Some(_) => [0.0, 0.3, 0.0],
            None => [0.0, 0.1, -0.85],
        },
    }
}

/// Whether this seed's boat rolled a `Stack` (funnel / vent) part - the
/// diegetic source a steam plume can sit atop. The `Stack` slot is optional,
/// so a plain boat may have no funnel at all.
fn boat_has_stack(seed: u64) -> bool {
    AvatarOutfit::for_seed(seed)
        .parts
        .iter()
        .any(|p| p.slot == PartSlot::Stack)
}

/// The slug of the part filling `slot` in this seed's outfit (the discrete
/// hull / envelope / chassis *class* - barge vs catamaran, twin vs zeppelin,
/// armored vs dune - which is a part slug, not an enum), or `""` if unfilled.
fn structural_slug(outfit: &AvatarOutfit, slot: PartSlot) -> &'static str {
    outfit
        .parts
        .iter()
        .find(|p| p.slot == slot)
        .map_or("", |p| p.slug)
}

/// The gait cadence, in steps a second, at which a seeded humanoid travels
/// at exactly the preset's default travel speed.
const NOMINAL_STEP_CADENCE: f32 = 2.2;

/// The travel speed - the run since #1193 - a seeded humanoid gets for its
/// gait cadence, m/s: the preset's default scaled by the cadence against
/// [`NOMINAL_STEP_CADENCE`], so a long-legged strider actually covers ground
/// faster than a short-stepped walker.
///
/// Read off [`HumanoidParams::default`] rather than a literal of its own, so
/// the seeded records and the preset cannot disagree about what a default
/// run is (#1323 moved it 4.0 → 5.0 m/s), and shared with the procedural
/// gait's no-record fallback for the same reason.
pub(crate) fn seeded_travel_speed(step_cadence: f32) -> f32 {
    HumanoidParams::default().walk_speed.0 * (step_cadence / NOMINAL_STEP_CADENCE)
}

/// Humanoid locomotion tuned to the **engine** body the seed rolls
/// (#1060): the collider capsule tracks that body's own stature and the
/// travel speed tracks the seeded gait cadence ([`seeded_travel_speed`]).
///
/// Sized from the engine record rather than the retired humanoid
/// blueprint, and that is a correctness fix rather than a swap of
/// equivalent sources: the rigged spawn path seats the skinned body's
/// feet at the collider's *bottom*, so a capsule that disagreed with the
/// body's stature would sink it into the ground or float it above.
fn humanoid_locomotion(seed: u64) -> LocomotionConfig {
    let stature = engine_stature(seed);
    let gait = AvatarGait::for_seed(seed);
    let mut p = HumanoidParams::default();
    // A person is roughly a tenth of their height across the shoulders; the
    // clamp keeps a rolled extreme inside collider sanity.
    p.capsule_radius = Fp((stature * 0.11).clamp(0.18, 0.34));
    p.capsule_length = Fp((stature - 2.0 * p.capsule_radius.0).max(0.4));
    p.walk_speed = Fp(seeded_travel_speed(gait.step_cadence));
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
// construction (`force = mass · target_accel`) - so a heavy barge is genuinely
// ponderous and a catamaran genuinely nimble, but nothing is undriveable. The
// support invariants are honoured: the hover-boat's suspension spring +
// buoyancy and the helicopter's `hover_thrust` all scale with the seeded mass
// so the craft sits at the same ride height it always did. Every value stays
// inside the locomotion sanitiser's clamps so the record round-trips unchanged.
//
// The **airplane** preset is a deliberate orphan: `ChassisFamily` has no
// `Airplane` variant, so no seed ever produces one - it is reachable only by a
// user manually picking it in the avatar editor (picker-only), and keeps its
// plain `default_config`. A fixed-wing visual family is out of scope here.
// ---------------------------------------------------------------------------

/// Reference mass (kg) the hover-boat preset's default support fields
/// (suspension stiffness 4200, buoyancy 2500) are cut for. The seeded masses
/// scale every one of them by `mass / REF`, which is what keeps a 480 kg barge
/// at the same ride height as an 80 kg skimmer.
const BOAT_REF_MASS: f32 = 50.0;

/// The same for the car preset, whose defaults are cut for a 900 kg machine.
const SKIFF_REF_MASS: f32 = 900.0;

/// Standard gravity (m/s²), the value the presets' own weight-support
/// derivations use (see the airship's `hover_thrust`).
const GRAVITY: f32 = 9.81;

/// How far (m) a four-corner raycast suspension sits compressed under its own
/// craft's weight, at rest on flat ground.
///
/// Each corner spring carries a quarter of the weight, so `4 · k · c = m · g`.
/// The seeded mass cancels: both vehicle presets scale their stiffness with
/// mass off the same reference (`k = k_ref · m / m_ref`, the invariant that
/// holds a craft's ride height as its mass grows), so this is one number per
/// family, not per seed - which is what lets an assembler derive its
/// travel-pose drop without knowing which craft it is placing.
///
/// Pass the preset's *default* stiffness, not a seeded one. The relation holds
/// while the mass-scaled stiffness stays under its sanitiser cap, which it does
/// across both families' mass clamps by a wide margin (the boat would need
/// 571 kg against a 480 kg ceiling); [`boat_locomotion`] debug-asserts it.
fn static_suspension_compression(ref_mass: f32, ref_stiffness: f32) -> f32 {
    ref_mass * GRAVITY / (4.0 * ref_stiffness)
}

/// Where the game rests this chassis's **origin** above flat ground (m), for
/// a craft that settles on a suspension: `half_y + rest_length - compression`.
///
/// The render tool's `--play-view` question (#1360), and the one thing that
/// view cannot get wrong. A ground plane is the first time the tool can show
/// hover and wheels, so a subject stood on its own bounds - keel on the dirt,
/// tyres half buried - would be a picture that lies about exactly what it
/// exists to show. Physics answers this in the running game by simulating;
/// here it is arithmetic, and it is the same arithmetic the two pose tests in
/// this module assert against ([`boat::land_ride_height`] and the skiff's
/// tyre line).
///
/// The compression term reads the **seeded** mass and stiffness rather than
/// the family reference [`static_suspension_compression`] takes, and gets the
/// same number: both presets scale stiffness with mass off one reference, so
/// `m · g / (4 · k_ref · m / m_ref)` cancels the seed out. That keeps this
/// free of any family constant, which is what lets it be one short function
/// instead of a match over private tables.
///
/// `None` for a craft with no suspension - an airship holds itself up with
/// thrust and has no ground ride height at all - and for a rigged humanoid,
/// whose height above the ground is its own animator's business. The play
/// view stands those on their drawn bounds instead.
///
/// Native-only: the render tool is `cfg(not(wasm32))`, and a `pub(crate)`
/// helper with no caller on wasm is dead code under CI's `-D warnings`
/// (#1351).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn ground_ride_height(loco: &LocomotionConfig) -> Option<f32> {
    let (half_y, rest, mass, stiffness) = match loco {
        LocomotionConfig::HoverBoat(p) => (
            p.chassis_half_extents.0[1],
            p.suspension_rest_length.0,
            p.mass.0,
            p.suspension_stiffness.0,
        ),
        LocomotionConfig::Car(p) => (
            p.chassis_half_extents.0[1],
            p.suspension_rest_length.0,
            p.mass.0,
            p.suspension_stiffness.0,
        ),
        _ => return None,
    };
    Some(half_y + rest - mass * GRAVITY / (4.0 * stiffness))
}

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
    // TRUE (drawn) dimensions, not authored ones (#1361): the blueprint is in
    // the parts' authoring frame and the assembler scales the whole tree by
    // `boat::VISUAL_SCALE` at its root, so everything derived here - mass,
    // collider, ride height - has to be told the size the craft is actually
    // drawn at. Dividing by the matching TRUE nominal below is what stops that
    // re-basing from simply pinning every craft against its mass clamp.
    let hull_len = b.map_or(boat::AUTHORED_HULL_LEN, |b| b.hull_len) * boat::VISUAL_SCALE;
    let beam = b.map_or(boat::AUTHORED_BEAM, |b| b.beam) * boat::VISUAL_SCALE;
    let freeboard = b.map_or(boat::AUTHORED_FREEBOARD, |b| b.freeboard) * boat::VISUAL_SCALE;

    // The 50 kg baseline is what the default suspension stiffness (4200) and
    // buoyancy (2500) hold at the stock ride height; scaling both by `mass/50`
    // keeps that height as mass grows. The clamp keeps the scaled stiffness
    // under its 50 000 sanitiser cap.
    const REF_MASS: f32 = BOAT_REF_MASS;
    let mut p = HoverBoatParams::default();
    let stock_stiffness = p.suspension_stiffness.0;
    let mass = (REF_MASS * mass_f * (hull_len / boat::NOMINAL_HULL_LEN)).clamp(80.0, 480.0);
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
    debug_assert!(
        p.suspension_stiffness.0 == stock_stiffness * scale,
        "the mass-scaled suspension stiffness hit its cap, so the ride height \
         below is no longer the one `static_suspension_compression` derives"
    );
    p.chassis_half_extents = fit_extents([beam * 0.5, freeboard * 0.6, hull_len * 0.5]);
    // Hold the hull where [`boat::land_ride_height`] wants it: the assembler
    // hangs the design waterline `boat::TRAVEL_DROP` under the chassis origin,
    // and under that go the hull's draft and its keel clearance. Derived from
    // the *clamped* half-extent, which is what the suspension casts from. The
    // un-seeded 0.8 m default this replaces was cut for a 1.32 m hull; left
    // alone it would have left an airship-class boat's keel 0.05 m off the
    // ground - beached, and ploughing every bump, since visuals carry no
    // colliders (#1361).
    let half_y = p.chassis_half_extents.0[1];
    p.suspension_rest_length = Fp(boat::land_ride_height(freeboard) - half_y
        + static_suspension_compression(REF_MASS, stock_stiffness));
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
    // TRUE (drawn) dimensions - see the note in [`boat_locomotion`] (#1361).
    let body_len = s.map_or(skiff::AUTHORED_BODY_LEN, |s| s.body_len) * skiff::VISUAL_SCALE;
    let body_w = s.map_or(skiff::AUTHORED_BODY_W, |s| s.body_w) * skiff::VISUAL_SCALE;

    const REF_MASS: f32 = SKIFF_REF_MASS;
    let mut p = CarParams::default();
    let mass = (REF_MASS * mass_f * (body_len / skiff::NOMINAL_BODY_LEN)).clamp(480.0, 1_500.0);
    let scale = mass / REF_MASS;
    // Scale a support field by mass and keep it under its sanitiser cap.
    let scaled = |v: f32, cap: f32| Fp((v * scale).min(cap));
    p.mass = Fp(mass);
    p.drive_force = Fp((mass * drive_accel).min(200_000.0));
    p.turn_torque = Fp((mass * turn_accel).min(50_000.0));
    p.suspension_stiffness = scaled(p.suspension_stiffness.0, 200_000.0);
    p.suspension_damping = scaled(p.suspension_damping.0, 20_000.0);
    p.lateral_grip = scaled(p.lateral_grip.0, 200_000.0);
    // The half-height comes from the bodywork, not from the body's LENGTH the
    // way `0.4 · (body_len / 1.5)` did - see [`skiff::chassis_half_height`] for
    // why that formula could not survive the rescale. The suspension rest
    // length needs no re-basing to match: the assembler derives its travel-pose
    // drop from this same box, so the tyres land on the ground whatever it is.
    p.chassis_half_extents =
        fit_extents([body_w * 0.5, skiff::chassis_half_height(), body_len * 0.5]);
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
    use crate::pds::{Generator, LocomotionConfig};

    /// The generator tree a seed builds, or `None` for the rigged family
    /// (#1060). Every tree-walking test below is about *assembled* geometry,
    /// which a rigged body has none of - it skips rather than pretending.
    fn visuals_for_seed(seed: u64) -> Option<Generator> {
        build_for_seed(seed).0.visuals().cloned()
    }

    /// The same for a DID.
    fn visuals_for_did(did: &str) -> Option<Generator> {
        build_for_did(did).0.visuals().cloned()
    }

    /// A DID per VEHICLE family - the three that still assemble parts.
    fn vehicle_dids() -> Vec<(ChassisFamily, String)> {
        family_dids()
            .into_iter()
            .filter(|(fam, _)| *fam != ChassisFamily::Humanoid)
            .collect()
    }

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

    /// Every derived drive acceleration lands in a tuned, driveable band -
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
    /// clamps - else a peer receiving the record would drive different physics
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
    /// sanitiser unchanged - the `visuals_survive_sanitize_unchanged` tree
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
            let Some(built) = visuals_for_seed(s) else {
                continue;
            };
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
        // bound - if the sanitiser rewrites anything, a peer receiving
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

        for (fam, did) in vehicle_dids() {
            let built = visuals_for_did(&did).expect("a vehicle assembles a tree");
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            assert_tree_eq(&built, &sanitized, fam);
        }
        // Sweep rather than trust the three hand-picked DIDs: since #1361 an
        // airship-class bridge scale rides the root of every seeded boat and
        // skiff, and the sanitiser CLAMPS an over-cap scale product rather
        // than rejecting it - so a part that set its own scale too deep under
        // the root would be silently shrunk back for some seeds only.
        for s in 0u64..400 {
            let Some(built) = visuals_for_seed(s) else {
                continue;
            };
            let mut sanitized = built.clone();
            sanitize_avatar_visuals(&mut sanitized);
            assert_tree_eq(&built, &sanitized, ChassisFamily::for_seed(s));
        }
    }

    /// The bridge scale (#1361) has to leave headroom under the sanitiser's
    /// cap on the product of scales down any root-to-leaf path - this is the
    /// measurement behind the round-trip assertion above, and says how much
    /// room a part author still has for a child scale of their own.
    #[test]
    fn the_airship_class_bridge_leaves_headroom_under_the_scale_cap() {
        use crate::pds::sanitize::{accumulated_scale, limits::MAX_AVATAR_SCALE_PRODUCT};
        let mut worst: f32 = 0.0;
        for s in 0u64..600 {
            let Some(built) = visuals_for_seed(s) else {
                continue;
            };
            worst = worst.max(accumulated_scale(&built));
        }
        assert!(
            worst > 1.0,
            "no seeded vehicle carried a bridge scale at all"
        );
        assert!(
            worst < MAX_AVATAR_SCALE_PRODUCT,
            "deepest scale product {worst} is at the sanitiser cap \
             {MAX_AVATAR_SCALE_PRODUCT} - the craft would be clamped smaller \
             than it was built"
        );
    }

    /// A seeded skiff's tyres rest exactly on the ground its own suspension
    /// settles at - for the seeded wheel it actually rolls on, not a nominal
    /// one. The fixed 0.55 m drop this replaced floated or sank them by
    /// centimetres across the seeded radius band even before #1361 doubled
    /// everything (#1361).
    #[test]
    fn a_seeded_skiffs_tyres_rest_on_its_own_suspension_ground_line() {
        let compression = static_suspension_compression(
            SKIFF_REF_MASS,
            CarParams::default().suspension_stiffness.0,
        );
        let mut checked = 0;
        for s in (0u64..600).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Skiff) {
            let (body, loco) = build_for_seed(s);
            let LocomotionConfig::Car(p) = &loco else {
                panic!("seed {s} is a skiff without car locomotion");
            };
            let visuals = body.visuals().expect("a skiff assembles a tree");
            let bp = VehicleBlueprint::from_seed(s)
                .and_then(|b| b.skiff().copied())
                .expect("a skiff has a blueprint");
            let scale = visuals.transform.scale.0[1];
            let drop = -visuals.transform.translation.0[1];
            let ride = p.chassis_half_extents.0[1] + p.suspension_rest_length.0 - compression;
            // The play view (#1360) stands a craft at this height from the
            // seeded params alone. The two agree because the preset scales
            // stiffness with mass, so the seed cancels - assert it per seed
            // rather than trusting the algebra.
            assert!(
                (ground_ride_height(&loco).expect("a skiff settles on a suspension") - ride).abs()
                    < 1e-4,
                "seed {s}: ground_ride_height disagrees with the family derivation"
            );
            let tyre_bottom = ride - drop - (bp.wheel_r - bp.ride_y) * scale;
            assert!(
                tyre_bottom.abs() < 1e-4,
                "seed {s}: tyres sit {tyre_bottom} m off the ground line"
            );
            checked += 1;
        }
        assert!(checked > 0, "no skiff seed exercised the drop");
    }

    /// A seeded boat floats on its own painted waterline and hovers clear of
    /// the ground on land - the two equilibria the derived drop + suspension
    /// rest length exist to satisfy at once (#1361).
    #[test]
    fn a_seeded_boat_floats_on_its_waterline_and_hovers_over_land() {
        let compression = static_suspension_compression(
            BOAT_REF_MASS,
            HoverBoatParams::default().suspension_stiffness.0,
        );
        let mut checked = 0;
        for s in (0u64..600).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat) {
            let (body, loco) = build_for_seed(s);
            let LocomotionConfig::HoverBoat(p) = &loco else {
                panic!("seed {s} is a boat without hover-boat locomotion");
            };
            let visuals = body.visuals().expect("a boat assembles a tree");
            let bp = VehicleBlueprint::from_seed(s)
                .and_then(|b| b.boat().copied())
                .expect("a boat has a blueprint");
            let scale = visuals.transform.scale.0[1];
            let drop = -visuals.transform.translation.0[1];
            // On water buoyancy rests the chassis origin `water_rest_length`
            // above the surface, so this is the waterline meeting the water.
            assert!(
                (drop - p.water_rest_length.0).abs() < 1e-6,
                "seed {s}: waterline sits {} m off the surface",
                p.water_rest_length.0 - drop
            );
            // On land the suspension has to hold the hull where the assembler
            // wants it, keel clear of the ground.
            let ride = p.chassis_half_extents.0[1] + p.suspension_rest_length.0 - compression;
            assert!(
                (ground_ride_height(&loco).expect("a boat settles on a suspension") - ride).abs()
                    < 1e-4,
                "seed {s}: ground_ride_height disagrees with the family derivation"
            );
            let want = boat::land_ride_height(bp.freeboard * scale);
            assert!(
                (ride - want).abs() < 1e-4,
                "seed {s}: hull rides at {ride} m, wanted {want} m"
            );
            assert!(
                ride - drop - bp.freeboard * scale > 0.0,
                "seed {s}: the keel is in the ground"
            );
            checked += 1;
        }
        assert!(checked > 0, "no boat seed exercised the drop");
    }

    #[test]
    fn no_family_carries_a_pfp_sign() {
        // #733 removed the identity signs (chest badge / hull decal / bow
        // crest) from every chassis - pin the removal so a future part
        // can't quietly reintroduce one.
        use crate::pds::generator::GeneratorKind;
        fn has_sign(g: &Generator) -> bool {
            matches!(g.kind, GeneratorKind::Sign { .. }) || g.children.iter().any(has_sign)
        }
        for (fam, did) in vehicle_dids() {
            let built = visuals_for_did(&did).expect("a vehicle assembles a tree");
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

    /// The DID path must be exactly the seed path fed the hashed DID -
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

        let built = visuals_for_seed(aura_seed).expect("a vehicle assembles a tree");
        assert!(
            has_particles(&built),
            "aura seed {aura_seed} grew no ParticleSystem"
        );

        let built = visuals_for_seed(voice_seed).expect("a vehicle assembles a tree");
        assert!(
            !matches!(built.audio, crate::pds::SovereignAudioConfig::None),
            "voice seed {voice_seed} set no body audio"
        );
    }

    #[test]
    fn the_humanoid_family_is_rigged_and_resolves_without_a_fetch() {
        // The #1060 contract, and the reason a seeded default still works
        // for a peer nobody has ever heard of: the engine body is rolled
        // locally from the same seed, so two clients agree on the person
        // AND on the wardrobe key it would be saved under, with nothing
        // exchanged.
        let seed = (0u64..400)
            .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Humanoid)
            .expect("some seed rolls a humanoid");
        let (body, loco) = build_for_seed(seed);
        let rig = body.rigged_ref().expect("the humanoid family is rigged");
        let resolved = rig
            .resolved
            .as_ref()
            .expect("seeded bodies resolve locally - no PDS round trip");
        assert_eq!(rig.avatar.len(), 13, "a deterministic TID rkey");
        assert!(body.visuals().is_none(), "no generator tree to walk");
        assert_eq!(loco.kind_tag(), "humanoid", "a rigged body walks");

        let (again, _) = build_for_seed(seed);
        let again = again.rigged_ref().expect("still rigged");
        assert_eq!(rig.avatar, again.avatar, "the rkey must be deterministic");
        assert_eq!(
            resolved.body,
            again.resolved.as_ref().expect("resolved").body,
            "the person must be deterministic"
        );
    }

    #[test]
    fn a_rigged_collider_matches_the_body_it_carries() {
        // The capsule is what `player::rigged` seats the skinned feet
        // against, so a capsule that disagreed with the body's stature
        // would sink it into the ground or float it above.
        for seed in (0u64..600).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Humanoid) {
            let stature = engine_stature(seed);
            let LocomotionConfig::Humanoid(p) = build_for_seed(seed).1 else {
                panic!("seed {seed} rolled a rigged body without humanoid locomotion");
            };
            let capsule = p.capsule_length.0 + 2.0 * p.capsule_radius.0;
            assert!(
                (capsule - stature).abs() < 0.05,
                "seed {seed}: a {stature:.2} m body in a {capsule:.2} m capsule"
            );
        }
    }

    #[test]
    fn distinct_seeds_yield_distinct_avatars() {
        // A re-roll must actually change the look.
        let (a, _) = build_for_seed(1);
        let (b, _) = build_for_seed(2);
        assert_ne!(a, b, "re-roll produced an identical avatar for two seeds");
    }
}
