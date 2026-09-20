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
//! the #1359 redesign), and since #1364 both are there for real: [`boats`] and
//! [`skiffs`] each draw one builder per craft type off one profile, authored
//! in the true metres they are drawn at, with no parts and no scale bridge
//! behind them. **No seeded vehicle carries a root scale any more** - the
//! airship never did, the boat stopped in #1363 and the skiff was the last one
//! holding a bridge. The consequence outlives all of it: everything derived
//! here from a vehicle's size (mass, collider, ride height, travel-pose drop,
//! particle sprites) reads the **drawn** dimensions, and compares them against
//! a named true nominal.
//!
//! Shared primitive/material vocabulary lives in [`common`].

mod airship;
mod assemble;
mod boats;
pub(crate) mod common;
mod fx;
mod skiffs;

use crate::pds::avatar::parts::PartSlot;
// Aliased: `crate::seeded_defaults::AvatarBody` (imported below) is the
// seeded *proportions* anchor, a different thing with the same name.
use crate::pds::avatar::body::AvatarBody as RecordBody;
use crate::pds::types::{Fp, Fp3};
use crate::seeded_defaults::{
    AvatarFx, AvatarGait, AvatarOutfit, AvatarPalette, ChassisFamily, NOMINAL_BODY_LEN,
    NOMINAL_HULL_LEN, ParticleAura, VehicleBlueprint, fnv1a_64,
};

use super::locomotion::{
    CarParams, HelicopterParams, HoverBoatParams, HumanoidParams, LocomotionConfig,
    LocomotionPreset,
};

/// How a craft is driven, which is what she sounds like - asked of the craft
/// that is DRAWN, never of the type a seed picked, so the voice is right
/// before every type is built (#1383).
///
/// Under sail a boat carries the wash along her hull and the wind in her rig,
/// and only an engine hums. A horseless wagon has neither: it ROLLS, and what
/// is heard is its running gear - iron tyres on the track and a timber creak
/// (#1377). A third variant rather than a flag beside the enum, so no skiff
/// can be under sail and no wagon can putter; a scow's pole, a tug's boiler,
/// a dune buggy's air-cooled four, a cyclecar's electric motor and a junk's
/// battened sails are variants for the same reason. The airship's rotors are
/// an engine by construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Propulsion {
    /// Sails: no engine note at all.
    Sail,
    /// An engine, whose note is the family's.
    Engine,
    /// No engine and no sail: the rumble of iron tyres and a creak.
    Rolling,
    /// A pole and a sweep (#1373): water lapping a flat hull and the sweep
    /// creaking in its crutch. A working scow's, her stern-wheel variant
    /// included - the wheel is her stern gear, drawn, not an engine.
    Poled,
    /// A boiler (#1370): the chuff of the exhaust and the thump of a slow
    /// steam engine over the wash. The steam tug's - and a boiler is the one
    /// drive that makes smoke, so it is also what promotes her picked wake
    /// to steam from her funnel (`fx::drawn_aura`).
    Steam,
    /// An air-cooled flat four on an open stinger (#1374): a raspy low
    /// firing note under a valve-train clatter, where every other car on the
    /// road putters. The dune buggy's - and an air-cooled engine has no
    /// radiator to steam, so a Roadside buggy's picked steam is drawn as her
    /// exhaust (`fx::drawn_aura`).
    AirCooled,
    /// An electric motor (#1376): a clean tonal whine where every other car
    /// on the road putters or clatters. The cyclecar's - and a motor has no
    /// pipe and no boiler, so she trails neither exhaust nor steam
    /// (`fx::drawn_aura`), as a rolling wagon does not.
    Electric,
    /// Battened lug sails (#1371): the sail's wash and rig wind, and a dry
    /// creak of bamboo battens working over them. The junk's - she sails, so
    /// no engine note and no oscillator, as under [`Sail`](Self::Sail); the
    /// creak is what makes her a junk to the ear.
    Battened,
    /// A heavy diesel (#1375): a 33 Hz block note under a hard lowpass,
    /// loped twice a second - the lowest voice in the fleet, where every
    /// other car on the road putters, clatters or whines. The armoured car's.
    /// And a diesel has a radiator and a pipe, so unlike the buggy's
    /// air-cooled flat four she folds nothing away and trails whatever aura
    /// her theme picked, off her own drawn pipe (`fx::drawn_aura` needs no
    /// arm for her).
    Diesel,
}

impl Propulsion {
    /// What `render --outfit` says of it.
    fn label(self) -> &'static str {
        match self {
            Self::Sail => "under sail",
            Self::Engine => "under power",
            Self::Rolling => "rolling on iron tyres",
            Self::Poled => "poled and sculled",
            Self::Steam => "under steam",
            Self::AirCooled => "under power, air-cooled",
            Self::Electric => "under power, electric",
            Self::Battened => "under battened sail",
            Self::Diesel => "under power, diesel",
        }
    }
}

/// Build the full seeded default avatar (body + locomotion) for a
/// DID. Deterministic: every peer derives the identical record.
pub fn build_for_did(did: &str) -> (RecordBody, LocomotionConfig) {
    build_for_seed(fnv1a_64(did))
}

/// Build a seeded avatar in a NAMED heritage livery instead of the one its
/// seed picked - the render tool's `--livery <index>`, and the only caller.
///
/// It exists because a curated scheme list cannot be judged by hunting for
/// seeds that happen to have rolled each entry: the schemes have to stand
/// side by side on ONE hull with everything else held still (#1365). The
/// index is into that family's table and wraps, so a survey loop over more
/// indices than the list holds draws each scheme once rather than repeating
/// the last. `None` is exactly [`build_for_seed`].
pub fn build_in_livery(seed: u64, livery: Option<usize>) -> (RecordBody, LocomotionConfig) {
    build_seeded(seed, livery)
}

/// [`build_in_livery`] for a DID - exactly `build_in_livery(fnv1a_64(did), l)`,
/// stated here so the DID-to-seed rule stays in the one place that owns it.
pub fn build_for_did_in_livery(did: &str, livery: Option<usize>) -> (RecordBody, LocomotionConfig) {
    build_in_livery(fnv1a_64(did), livery)
}

/// Build from a pre-computed seed - the manual re-roll path. `seed`
/// chooses the chassis family and drives every derived value.
/// `build_for_did(did)` is exactly `build_for_seed(fnv1a_64(did))`.
/// (Avatars no longer wear a pfp identity sign - #733 removed the
/// chest-badge / hull-decal / bow-crest panels from every chassis.)
pub fn build_for_seed(seed: u64) -> (RecordBody, LocomotionConfig) {
    build_seeded(seed, None)
}

/// The one assembly path, with the livery override [`build_in_livery`] needs
/// threaded through it.
fn build_seeded(seed: u64, livery: Option<usize>) -> (RecordBody, LocomotionConfig) {
    let family = ChassisFamily::for_seed(seed);
    // The rigged family short-circuits the whole part-assembly pipeline:
    // there is no tree to compose, no FX mount to snap to a blueprint
    // landmark, and no sanitiser pass to owe - the engine record IS the
    // body, and the skinned build happens at spawn (#1057).
    if family == ChassisFamily::Humanoid {
        return (RecordBody::rigged_seeded(seed), humanoid_locomotion(seed));
    }
    // No visual-root scale: the airship-class bridge of #1361 carried a
    // uniform factor here (and a matching one into the FX, because a particle
    // sprite is sized in world metres rather than in its emitter's frame)
    // while the boat and the skiff were still authored a third of the size
    // they were drawn at. Both are authored at true size now (#1363, #1364),
    // so the bridge is gone from the root, from the particles and from
    // `apply_travel_pose`'s signature.
    let (mut visuals, loco) = match family {
        ChassisFamily::Boat => (boats::build(seed, livery), boat_locomotion(seed)),
        ChassisFamily::Airship => (airship::build(seed), airship_locomotion(seed)),
        ChassisFamily::Skiff => (skiffs::build(seed, livery), skiff_locomotion(seed)),
        // Handled above; a family added later lands here loudly rather
        // than silently assembling nothing.
        ChassisFamily::Humanoid => unreachable!("the rigged family returns above"),
    };
    // Seeded FX: hang the style's signature particle aura (floored to the
    // chassis wake / vent / exhaust) + body voice on the built root. The mount
    // is snapped to the seeded blueprint landmark for the aura - a boat's steam
    // leaves its funnel, its wake rides the stern - via [`fx_mount`].
    // The PICKED aura is resolved against the drawn craft's drive first: a
    // rolling wagon has no pipe to trail exhaust from (#1377), a boiler
    // turns the wake floor into steam from her funnel (#1370), and an
    // air-cooled engine has no radiator to steam, so a dune buggy's picked
    // steam is her exhaust (#1374).
    let fx = AvatarFx::for_seed(seed);
    let fx = AvatarFx {
        aura: fx::drawn_aura(fx.aura, fx::drive_of(family, seed)),
        ..fx
    };
    let accent = AvatarPalette::for_seed(seed).primary_accent;
    fx::attach(
        &mut visuals,
        &fx,
        fx_mount(fx.aura, family, seed),
        accent,
        family,
        seed,
    );
    (RecordBody::generator(visuals), loco)
}

/// What the seeded avatar for `seed` sounds like, in one line: its voice, the
/// drive under it and its detune bucket (#1383). The render tool's `--outfit`
/// readout, which is native-only, hence `pub` - a crate-private reader it
/// alone called would be dead code on wasm.
pub fn voice_label(seed: u64) -> String {
    fx::voice_label(seed)
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
/// assembler's yaw and drop). The station is snapped to the seeded blueprint
/// landmarks the assembler already mounts parts on - so the emitter tracks the
/// actual craft instead of a fixed constant, and a skiff's exhaust leaves its
/// tailpipe rather than empty air. A boat asks her own craft type, which reads
/// the station off her [`HullProfile`](boats::HullProfile) (#1363). Falls back
/// to a per-family constant if the blueprint is unavailable (never for a real
/// vehicle).
///
/// The two families still assembled from parts author in a frame their
/// assembler scales at the root, so their stations are authoring-frame metres
/// and grow with the craft; a boat is drawn at the size she is authored at and
/// has no such factor.
///
/// Vehicles author their stern at local `-Z`, so an aft mount rides behind the
/// craft once the 180° travel-facing yaw is applied.
fn fx_mount(aura: ParticleAura, family: ChassisFamily, seed: u64) -> [f32; 3] {
    match family {
        // A tight aura around the torso (chest height), not floating overhead.
        ChassisFamily::Humanoid => [0.0, 0.45, 0.0],
        // A boat's aura is read off her own hull by the craft type that
        // drew it (#1363) - a wake leaves the transom of the boat that is
        // actually there, not a fraction of a nominal one.
        ChassisFamily::Boat => boats::fx_mount(seed, aura).unwrap_or([0.0, 0.1, -0.8]),
        // Vents / thruster wash / motes all issue from beneath the slung
        // gondola - the assembler's belly line, tracking the chosen envelope.
        ChassisFamily::Airship => airship::fx_belly_anchor(seed),
        // A skiff's aura is read off its own body by the craft type that drew
        // it (#1364) - an exhaust wisp leaves the pipe mouth of the machine
        // that is actually there, not a fraction of a nominal one.
        ChassisFamily::Skiff => skiffs::fx_mount(seed, aura).unwrap_or([0.0, 0.3, 0.0]),
    }
}

/// The slug of the part filling `slot` in this seed's outfit (the discrete
/// envelope / chassis *class* - twin vs zeppelin, armored vs dune - which is a
/// part slug, not an enum), or `""` if unfilled. Boats have no parts and no
/// class since #1363: their discrete pick is a [`BoatType`](crate::seeded_defaults::BoatType), and the type
/// carries the feel.
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
// from the craft's discrete pick - a boat's [`BoatType`] since #1363, an
// envelope or chassis class for the two families still assembled from parts -
// and the seeded blueprint dimensions, keeping the drive **acceleration**
// inside a tuned feel band by construction (`force = mass · target_accel`), so
// a heavy craft is genuinely ponderous and a light one genuinely nimble but
// nothing is undriveable. The
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
/// this module assert against ([`boats::land_ride_height`] and the skiff's
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

/// Boat (hover-boat) locomotion from the seeded craft type + her true
/// proportions.
///
/// The type carries the feel now (`BoatCraft::feel`), where the four hull
/// *arrangements* used to: they went with the legacy pipeline in #1363, and a
/// sloop is what the monohull was, so the numbers are the monohull's and the
/// drive is the one validated with the scale bridge. The suspension spring,
/// buoyancy and lateral grip scale with the derived mass so the hull keeps her
/// hover ride height whatever she weighs.
fn boat_locomotion(seed: u64) -> LocomotionConfig {
    let bp = VehicleBlueprint::from_seed(seed);
    let b = bp.as_ref().and_then(VehicleBlueprint::boat);
    let (feel, draft, drawn_beam) = boats::feel_and_draft(seed);
    // TRUE metres throughout since #1363: the blueprint IS the drawn boat, so
    // mass, collider and ride height are all read straight off her. Dividing
    // by the nominal below is what stops the re-basing simply pinning every
    // craft against its mass clamp.
    let hull_len = b.map_or(NOMINAL_HULL_LEN, |b| b.hull_len);
    // The DRAWN beam where the craft says it differs from the blueprint's -
    // a catamaran is half as wide again (#1372) - so the collider is as wide
    // as the boat round it.
    let beam = drawn_beam.unwrap_or_else(|| b.map_or(NOMINAL_HULL_LEN / 3.5, |b| b.beam));
    let freeboard = b.map_or(NOMINAL_HULL_LEN * 0.107, |b| b.freeboard);

    // The 50 kg baseline is what the default suspension stiffness (4200) and
    // buoyancy (2500) hold at the stock ride height; scaling both by `mass/50`
    // keeps that height as mass grows. The clamp keeps the scaled stiffness
    // under its 50 000 sanitiser cap.
    const REF_MASS: f32 = BOAT_REF_MASS;
    let mut p = HoverBoatParams::default();
    let stock_stiffness = p.suspension_stiffness.0;
    let mass = (REF_MASS * feel.mass_factor * (hull_len / NOMINAL_HULL_LEN)).clamp(80.0, 480.0);
    let scale = mass / REF_MASS;
    // Scale a support field by mass and keep it under its sanitiser cap.
    let scaled = |v: f32, cap: f32| Fp((v * scale).min(cap));
    p.mass = Fp(mass);
    p.drive_force = Fp((mass * feel.drive_accel).min(50_000.0));
    p.turn_torque = Fp((mass * feel.turn_accel).min(50_000.0));
    p.linear_damping = Fp(feel.linear_damping);
    p.angular_damping = Fp(feel.angular_damping);
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
    // Hold the hull where [`boats::land_ride_height`] wants her: the assembler
    // hangs the design waterline `boats::TRAVEL_DROP` under the chassis origin,
    // and under that go her draft and her keel clearance. Derived from the
    // *clamped* half-extent, which is what the suspension casts from. The
    // un-seeded 0.8 m default this replaces was cut for a 1.32 m hull; left
    // alone it would leave an airship-class boat's keel 0.05 m off the ground -
    // beached, and ploughing every bump, since visuals carry no colliders
    // (#1361).
    let half_y = p.chassis_half_extents.0[1];
    p.suspension_rest_length = Fp(boats::land_ride_height(draft) - half_y
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

/// Skiff (car) locomotion from the seeded craft type + its true proportions.
///
/// The type carries the feel now (`SkiffCraft::feel`), where the four chassis
/// *classes* used to: they went with the legacy pipeline in #1364, and a
/// roadster is what the default chassis was, so the numbers are the default
/// chassis's and the drive is the one validated with the scale bridge. The
/// suspension and grip scale with the derived mass so the machine keeps its
/// ride height whatever it weighs.
fn skiff_locomotion(seed: u64) -> LocomotionConfig {
    let s = VehicleBlueprint::from_seed(seed).and_then(|b| b.skiff().copied());
    let (feel, half_extents) = skiffs::feel_and_box(seed);
    // TRUE metres throughout since #1364: the blueprint IS the drawn machine,
    // so mass, collider and ride height are all read straight off it.
    let length = s.map_or(NOMINAL_BODY_LEN, |s| s.length);

    const REF_MASS: f32 = SKIFF_REF_MASS;
    let mut p = CarParams::default();
    let mass = (REF_MASS * feel.mass_factor * (length / NOMINAL_BODY_LEN)).clamp(480.0, 1_500.0);
    let scale = mass / REF_MASS;
    // Scale a support field by mass and keep it under its sanitiser cap.
    let scaled = |v: f32, cap: f32| Fp((v * scale).min(cap));
    p.mass = Fp(mass);
    p.drive_force = Fp((mass * feel.drive_accel).min(200_000.0));
    p.turn_torque = Fp((mass * feel.turn_accel).min(50_000.0));
    p.suspension_stiffness = scaled(p.suspension_stiffness.0, 200_000.0);
    p.suspension_damping = scaled(p.suspension_damping.0, 20_000.0);
    p.lateral_grip = scaled(p.lateral_grip.0, 200_000.0);
    // The box comes from the bodywork the craft actually draws, not from the
    // body's LENGTH the way `0.4 · (body_len / 1.5)` did - see
    // [`skiffs::chassis_half_extents`] for why that formula could not survive
    // the rescale. The suspension rest length needs no re-basing to match: the
    // assembler derives its travel-pose drop from this same box, so the tyres
    // land on the ground whatever it is.
    p.chassis_half_extents = fit_extents(half_extents);
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
    // `seed_for_class` hunted a seed whose outfit rolled a named structural
    // part - how the boat feel tests used to find a barge or a catamaran.
    // Both callers retired with the hull arrangements in #1363; a craft type
    // is found with `BoatType::for_seed` (or `render --family-seeds --craft`)
    // rather than by the slug of a part that no longer exists.

    /// Drive acceleration (drive force / mass, m/s²) of a vehicle preset.
    fn drive_accel(loco: &LocomotionConfig) -> f32 {
        match loco {
            LocomotionConfig::HoverBoat(b) => b.drive_force.0 / b.mass.0,
            LocomotionConfig::Car(c) => c.drive_force.0 / c.mass.0,
            LocomotionConfig::Helicopter(h) => h.cyclic_force.0 / h.mass.0,
            _ => panic!("not a vehicle preset"),
        }
    }

    // `mass_story_is_no_longer_inverted` compared a barge against a catamaran
    // and against a skiff, pinning the #782 fix that a 50 kg barge no longer
    // ran the rover tuning at 36 m/s². Both boat arrangements it named went
    // with the legacy pipeline in #1363, and the claim that survives them -
    // that nothing is a rocket or a brick - is `every_vehicle_drive_accel_is_
    // in_the_feel_band` below, which checks every seed rather than three. A
    // per-type feel guard lands with the feel sweep, #1381.

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

    // `distinct_hull_classes_drive_differently` pinned that a barge and a
    // catamaran did not share one bit-identical locomotion config. Both hull
    // arrangements went with the legacy boat pipeline in #1363: a boat's feel
    // is a property of her craft TYPE now, and until a second type is built
    // there is nothing for it to compare. It comes back, as a per-type feel
    // guard, with #1381.

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

    /// No craft's deepest node scale reaches the sanitiser's cap on the
    /// product of scales down any root-to-leaf path.
    ///
    /// This used to measure the airship-class BRIDGE (#1361), a uniform factor
    /// on the visual root; that died with the last legacy pipeline in #1364
    /// and every seeded root is scale-free now. What it measures instead is
    /// the thing that actually risks the cap: a craft's own shaping scales,
    /// which the redesigned families lean on hard - a roadster's guard is a
    /// tube flattened 3.4x on one axis, and a hull's section is its node
    /// scale. The sanitiser CLAMPS an over-cap product rather than rejecting
    /// it, so a part that set one too deep under the root would be silently
    /// shrunk back for some seeds only.
    #[test]
    fn no_craft_leans_on_a_node_scale_the_sanitiser_would_clamp() {
        use crate::pds::sanitize::{accumulated_scale, limits::MAX_AVATAR_SCALE_PRODUCT};
        let mut worst: f32 = 0.0;
        for s in 0u64..600 {
            let Some(built) = visuals_for_seed(s) else {
                continue;
            };
            assert_eq!(
                built.transform.scale.0, [1.0; 3],
                "seed {s}: a seeded craft's visual ROOT carries a scale - the \
                 airship-class bridge died with #1364 and nothing should have \
                 put one back"
            );
            worst = worst.max(accumulated_scale(&built));
        }
        assert!(
            worst > 1.0,
            "no seeded craft shaped anything with a node scale at all, so this \
             measures nothing"
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
    ///
    /// In its #1364 form the tyre line is read off the machine's own
    /// [`BodyPlan`](skiffs::BodyPlan) rather than off a blueprint `ride_y`
    /// that a reader had to keep in step with the wheels: an axle is one wheel
    /// radius above the ground by construction now, so the only thing left to
    /// check is that the assembler dropped the body by the right amount.
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
            assert_eq!(
                visuals.transform.scale.0, [1.0; 3],
                "seed {s}: a skiff is authored at the size she is drawn at since \
                 #1364, so her root carries no scale bridge"
            );
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
            // The datum floats `ride - drop` over the ground, and the plan says
            // how far under the datum the ground is meant to be.
            let datum = ride - drop;
            let want = skiffs::datum_height_for_seed(s).expect("a skiff has a body plan");
            assert!(
                (datum - want).abs() < 1e-4,
                "seed {s}: the tyres sit {} m off the ground line",
                datum - want
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
            assert_eq!(
                visuals.transform.scale.0[1], 1.0,
                "seed {s}: a boat is authored at the size she is drawn at since #1363, \
                 so her root carries no scale bridge"
            );
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
            // The DRAWN hull's draft: the blueprint's fin draft on a sloop,
            // and on a runabout her own canoe body plus a skeg (#1372).
            let (_, draft, _) = boats::feel_and_draft(s);
            let want = boats::land_ride_height(draft);
            assert!(
                (ride - want).abs() < 1e-4,
                "seed {s}: hull rides at {ride} m, wanted {want} m"
            );
            // The keel clears the ground by a quarter of her draft.
            let keel = ride - drop - draft;
            assert!(
                keel > 0.0 && (keel - 0.25 * draft).abs() < 1e-4,
                "seed {s}: the keel sits {keel} m off the ground"
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

    /// A boat's aura leaves the hull she actually has (#1363).
    ///
    /// The funnel-presence test this replaces asked whether the seed had
    /// rolled a `Stack` part, and a boat has no parts any more: the mount is
    /// read off her own [`HullProfile`], so it cannot be anywhere the hull is
    /// not. A wake leaves at the after end of the wetted length rather than at
    /// the transom, which on a hull with this much rocker is clear of the
    /// water.
    #[test]
    fn a_boats_aura_leaves_her_own_hull() {
        let mut checked = 0;
        for s in (0u64..400).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat) {
            let bp = VehicleBlueprint::from_seed(s)
                .and_then(|b| b.boat().copied())
                .expect("a boat has a blueprint");
            let wake = fx_mount(ParticleAura::Wake, ChassisFamily::Boat, s);
            assert!(
                wake[2] < 0.0 && wake[2] >= -bp.hull_len * 0.5 - 1e-3,
                "seed {s}: a wake at z {} is not abaft amidships and within the \
                 transom",
                wake[2]
            );
            assert!(
                wake[1] < 0.0 && wake[1] > -bp.draft,
                "seed {s}: a wake at y {} is not just under the waterline",
                wake[1]
            );
            let motes = fx_mount(ParticleAura::ArcaneMotes, ChassisFamily::Boat, s);
            assert!(
                motes[1] > bp.freeboard,
                "seed {s}: decorative motes at y {} are not over the deck",
                motes[1]
            );
            checked += 1;
        }
        assert!(checked > 0, "no boat seed exercised the aura mount");
    }

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
