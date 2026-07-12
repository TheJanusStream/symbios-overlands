//! Vehicle proportion blueprint — the vehicle analogue of
//! [`HumanoidBlueprint`](super::blueprint::HumanoidBlueprint).
//!
//! Turns the seeded [`AvatarBody`] multiplier knobs (plus a vehicle
//! [`VehicleStance`] register sampled the way the humanoid samples its
//! [`StylizationTier`](super::body::StylizationTier)) into concrete
//! world-space proportions and mount landmarks for one vehicle chassis. The
//! part builders ([`crate::pds::avatar::parts`]) size their geometry from it
//! and the family assembler ([`crate::pds::avatar::default_visuals`]) reads
//! the *same* landmarks for its mount anchors — so the two can never drift
//! (the fixed-anchor / part-internal-constant coupling that floated stacks
//! and bows off mis-sized hulls, #782/#783).
//!
//! Per-family structs behind the [`VehicleBlueprint`] enum: a boat and an
//! airship have genuinely different landmarks (deck line vs belly line), so
//! each family exposes only its own, and a part reads its family's blueprint
//! or nothing. Families are added as their redesigns wire them; a chassis
//! with no blueprint yet (and the humanoid, which uses
//! [`HumanoidBlueprint`](super::blueprint::HumanoidBlueprint)) yields `None`.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use super::body::AvatarBody;
use super::chassis::ChassisFamily;
use crate::seeded_defaults::scene::range_f32;

/// Sub-stream salt so the blueprint's stance + jitter draws are decorrelated
/// from every sibling avatar deriver (body, palette, outfit, …).
const VEHICLE_BLUEPRINT_SALT: u64 = 0x0EE1_C0DE_0EE1_C0DE;

/// The overall build register a vehicle is drawn in — the vehicle counterpart
/// of the humanoid [`StylizationTier`](super::body::StylizationTier). Sampled
/// first, then the continuous proportion knobs are banded by it so they
/// covary: a `Heavy` hull always arrives wide and tall-sided, never on a
/// racer's low narrow freeboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleStance {
    /// Short and tall-for-its-length — a stubby tug / runabout.
    Compact,
    /// Long, low and narrow — a racer / cutter.
    Sleek,
    /// Wide and tall-sided — a hauler / barge.
    Heavy,
}

impl VehicleStance {
    /// Weighted seeded pick: Sleek 40 % / Heavy 30 % / Compact 30 %.
    fn sample(rng: &mut impl RngCore) -> Self {
        let roll = range_f32(rng, 0.0, 1.0);
        if roll < 0.40 {
            Self::Sleek
        } else if roll < 0.70 {
            Self::Heavy
        } else {
            Self::Compact
        }
    }

    /// `(length, freeboard, beam)` multipliers this stance applies to the
    /// nominal hull. Centred so the population average stays near the
    /// pre-blueprint hand-tuned dimensions.
    fn boat_factors(self) -> (f32, f32, f32) {
        match self {
            Self::Compact => (0.90, 1.10, 1.06),
            Self::Sleek => (1.12, 0.90, 0.94),
            Self::Heavy => (0.98, 1.08, 1.16),
        }
    }

    /// Gondola-size multiplier for this stance — a Heavy airship carries a
    /// roomier car, a Sleek one a trimmer pod.
    fn airship_gondola_factor(self) -> f32 {
        match self {
            Self::Compact => 0.95,
            Self::Sleek => 0.92,
            Self::Heavy => 1.10,
        }
    }

    /// `(length, radius)` multipliers the stance applies to the Lathe envelope
    /// profile (#791): a Sleek ship is long and slim, a Heavy one short and
    /// fat, a Compact one stubby. Centred so the population average stays near
    /// each form's hand-tuned nominal.
    fn airship_env_factors(self) -> (f32, f32) {
        match self {
            Self::Compact => (0.92, 1.02),
            Self::Sleek => (1.13, 0.9),
            Self::Heavy => (0.93, 1.14),
        }
    }

    /// `(length, width)` body multipliers — a Sleek skiff is a long low racer,
    /// a Heavy one a wide hauler, a Compact one a short runabout.
    fn skiff_factors(self) -> (f32, f32) {
        match self {
            Self::Compact => (0.90, 0.96),
            Self::Sleek => (1.12, 0.95),
            Self::Heavy => (0.96, 1.15),
        }
    }
}

/// Concrete boat proportions + mount landmarks (metres, hull centred at the
/// waterline origin). Nominal dimensions match the pre-blueprint hulls; the
/// stance and [`AvatarBody`] multipliers spread them per seed. The four hull
/// *forms* (mono / catamaran / trimaran / barge) stay discrete part picks that
/// scale their geometry from `beam` / `hull_len` / `freeboard`; the landmarks
/// (`deck_y`, `bow_z`, …) are what the assembler mounts to.
#[derive(Clone, Copy, Debug)]
pub struct BoatBlueprint {
    pub stance: VehicleStance,
    /// Monohull reference length (bow→stern); the other forms scale from it.
    pub hull_len: f32,
    /// Monohull reference beam (full width).
    pub beam: f32,
    /// Above-waterline height (freeboard).
    pub freeboard: f32,
    /// Deck / mast-foot height above the waterline origin.
    pub deck_y: f32,
    /// Mast column height (deck → masthead).
    pub mast_h: f32,
    /// Bow-slot mount station (+Z), derived from the hull length so a bow
    /// ornament always lands at the actual prow, not a fixed constant.
    pub bow_z: f32,
    /// Stern stack mount station (−Z), likewise length-derived.
    pub stack_z: f32,
    /// Deck-ornament mount station (just forward of amidships).
    pub ornament_z: f32,
}

impl BoatBlueprint {
    fn derive(body: &AvatarBody, rng: &mut ChaCha8Rng) -> Self {
        let stance = VehicleStance::sample(rng);
        let (len_f, fb_f, beam_f) = stance.boat_factors();
        // Overall size rides the body height knob; width rides shoulder
        // width; both stay inside the hand-tuned band.
        let size = body.height_scale;
        let hull_len = 1.32 * size * len_f * range_f32(rng, 0.94, 1.06);
        let beam = 0.5 * size * body.shoulder_width_scale * beam_f;
        let freeboard = 0.26 * size * fb_f * range_f32(rng, 0.95, 1.05);
        let deck_y = freeboard * 0.5;
        let mast_h = 0.42 * size * range_f32(rng, 0.9, 1.15);
        Self {
            stance,
            hull_len,
            beam,
            freeboard,
            deck_y,
            mast_h,
            // Stations as fractions of the hull length so the anchors track
            // the seeded hull instead of silently re-encoding its default
            // length as a literal (the coupling #783 removes everywhere). The
            // bow station sits *on* the stem (the hull's swept blob tips out at
            // ≈0.5·len once the iso-surface pulls in from the analytic cone),
            // not the old 0.59 that floated a figurehead clear ahead of the
            // prow — the survey's "seed-28 unanchored bow sphere" (#785). A
            // forward-projecting ram still overhangs via its own +Z offset.
            bow_z: hull_len * 0.50,
            stack_z: -hull_len * 0.42,
            ornament_z: hull_len * 0.076,
        }
    }
}

/// Airship proportions. Each envelope **form** is a seeded Lathe body of
/// revolution whose length + girth the `len_mult` / `radius_mult` here perturb
/// (#791); its mount *landmarks* (belly line, tail station, fin ring radius,
/// pod line) are read straight off that same profile by the assembler (see the
/// vehicle assembler's `airship_mounts` + `airship_profile`) — so a fat blimp
/// and a slim zeppelin each seat their slung gondola / cruciform fins / engine
/// pods on *their own* body, and they stay seated as the profile stretches (the
/// envelope-invariant-anchor bug that floated the twin's rigging clear of its
/// belly is gone by construction).
#[derive(Clone, Copy, Debug)]
pub struct AirshipBlueprint {
    /// Overall build register — read by the locomotion tuning (#794); today it
    /// biases the gondola size.
    pub stance: VehicleStance,
    /// Gondola size multiplier.
    pub gondola_scale: f32,
    /// Lathe-envelope length multiplier (#791) — scales each form's profile
    /// length so the population spans a continuum of silhouettes, not a few
    /// fixed sizes. The assembler scales the belly / tail / fin / pod mounts by
    /// the same factors so the slung parts stay seated.
    pub len_mult: f32,
    /// Lathe-envelope girth (max-radius) multiplier (#791).
    pub radius_mult: f32,
}

impl AirshipBlueprint {
    fn derive(body: &AvatarBody, rng: &mut ChaCha8Rng) -> Self {
        let stance = VehicleStance::sample(rng);
        let gondola_scale = (body.height_scale * body.head_scale * stance.airship_gondola_factor())
            .clamp(0.85, 1.2);
        // Envelope size rides the body height (length) + shoulder-width (girth)
        // knobs and the stance, with a small per-seed jitter — the #791
        // continuum. Clamped so a Lathe profile never degenerates.
        let (len_f, rad_f) = stance.airship_env_factors();
        let len_mult = (body.height_scale * len_f * range_f32(rng, 0.95, 1.06)).clamp(0.85, 1.28);
        let radius_mult =
            (body.shoulder_width_scale * rad_f * range_f32(rng, 0.95, 1.05)).clamp(0.85, 1.2);
        Self {
            stance,
            gondola_scale,
            len_mult,
            radius_mult,
        }
    }
}

/// Concrete skiff proportions + the wheel/fender/anchor landmarks that three
/// files used to encode as matching magic numbers (the fender tori baked into
/// the chassis part, the wheel part's radius, and the assembler's wheel
/// anchors). Deriving them once here is what lets the body vary per seed and
/// unblocks wheel variants (#788): the chassis sizes its tub + fenders from
/// this, the assembler places the four wheels from `track` / `wheelbase`, and
/// the wheel part sizes from `wheel_r` — all guaranteed to agree.
#[derive(Clone, Copy, Debug)]
pub struct SkiffBlueprint {
    pub stance: VehicleStance,
    /// Body tub length (fore-aft).
    pub body_len: f32,
    /// Body tub width.
    pub body_w: f32,
    /// Wheel/fender lateral offset from the centreline (±X).
    pub track: f32,
    /// Wheel/fender fore & aft offset (±Z).
    pub wheelbase: f32,
    /// Wheel hub height (the wheels' axle line, below the body origin).
    pub ride_y: f32,
    /// Wheel outer radius (tyre tread). The fender radius tracks this.
    pub wheel_r: f32,
}

impl SkiffBlueprint {
    fn derive(body: &AvatarBody, rng: &mut ChaCha8Rng) -> Self {
        let stance = VehicleStance::sample(rng);
        let (len_f, width_f) = stance.skiff_factors();
        let size = body.height_scale;
        let body_len = 1.5 * size * len_f * range_f32(rng, 0.95, 1.05);
        // Floor the width so the (still nominal-width) greenhouse canopy always
        // fits the cabin until the body redesign scales it too (#787).
        let body_w =
            (0.76 * size * body.shoulder_width_scale.clamp(0.85, 1.15) * width_f).clamp(0.64, 1.12);
        // Wheels "look good" as-is (user), so keep the radius near nominal — a
        // gentle limb-thickness nudge only. The fender radius derives from it.
        let wheel_r = (0.21 * body.limb_thickness_scale.clamp(0.9, 1.12)).clamp(0.17, 0.25);
        Self {
            stance,
            body_len,
            body_w,
            // Track/wheelbase as fractions of the body so wheels sit at its
            // corners regardless of the seeded size.
            track: body_w * 0.59,
            wheelbase: body_len * 0.367,
            ride_y: -0.12 * size,
            wheel_r,
        }
    }
}

/// Per-family vehicle proportion blueprint. One variant per chassis that has
/// been wired to the shared-landmark system; [`VehicleBlueprint::from_seed`]
/// yields `None` for a chassis without one yet (and for the humanoid).
#[derive(Clone, Copy, Debug)]
pub enum VehicleBlueprint {
    Boat(BoatBlueprint),
    Airship(AirshipBlueprint),
    Skiff(SkiffBlueprint),
}

impl VehicleBlueprint {
    /// Derive the blueprint for a seed's chassis, or `None` if that chassis
    /// has no vehicle blueprint (humanoid, or a family not yet wired).
    pub fn from_seed(seed: u64) -> Option<Self> {
        Self::from_body(
            &AvatarBody::for_seed(seed),
            ChassisFamily::for_seed(seed),
            seed,
        )
    }

    /// Derive from an already-sampled [`AvatarBody`] + chassis. `seed` opens
    /// the blueprint's own salted jitter stream (kept distinct from the body
    /// deriver's stream so the two never entangle).
    pub fn from_body(body: &AvatarBody, chassis: ChassisFamily, seed: u64) -> Option<Self> {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ VEHICLE_BLUEPRINT_SALT);
        match chassis {
            ChassisFamily::Boat => Some(Self::Boat(BoatBlueprint::derive(body, &mut rng))),
            ChassisFamily::Airship => Some(Self::Airship(AirshipBlueprint::derive(body, &mut rng))),
            ChassisFamily::Skiff => Some(Self::Skiff(SkiffBlueprint::derive(body, &mut rng))),
            // The humanoid uses HumanoidBlueprint.
            ChassisFamily::Humanoid => None,
        }
    }

    /// The boat blueprint, if this is a boat.
    pub fn boat(&self) -> Option<&BoatBlueprint> {
        match self {
            Self::Boat(b) => Some(b),
            _ => None,
        }
    }

    /// The airship blueprint, if this is an airship.
    pub fn airship(&self) -> Option<&AirshipBlueprint> {
        match self {
            Self::Airship(a) => Some(a),
            _ => None,
        }
    }

    /// The skiff blueprint, if this is a skiff.
    pub fn skiff(&self) -> Option<&SkiffBlueprint> {
        match self {
            Self::Skiff(s) => Some(s),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = VehicleBlueprint::from_seed(4242);
        let b = VehicleBlueprint::from_seed(4242);
        assert_eq!(
            a.and_then(|x| x.boat().map(|b| (b.hull_len, b.beam))),
            b.and_then(|x| x.boat().map(|b| (b.hull_len, b.beam)))
        );
    }

    #[test]
    fn every_vehicle_gets_its_family_blueprint_and_humanoids_do_not() {
        for s in 0u64..200 {
            let bp = VehicleBlueprint::from_seed(s);
            match ChassisFamily::for_seed(s) {
                ChassisFamily::Boat => {
                    assert!(
                        bp.and_then(|b| b.boat().copied()).is_some(),
                        "seed {s} boat"
                    );
                }
                ChassisFamily::Airship => {
                    assert!(
                        bp.and_then(|b| b.airship().copied()).is_some(),
                        "seed {s} airship"
                    );
                }
                ChassisFamily::Skiff => {
                    assert!(
                        bp.and_then(|b| b.skiff().copied()).is_some(),
                        "seed {s} skiff"
                    );
                }
                ChassisFamily::Humanoid => {
                    assert!(bp.is_none(), "seed {s}: humanoid got a blueprint")
                }
            }
        }
    }

    #[test]
    fn boat_dims_stay_in_sane_range() {
        // Every boat seed must land inside a band that keeps the hull a
        // believable, sanitiser-safe size (no zero/exploded dimensions).
        let mut seen = 0;
        for s in 0u64..600 {
            let Some(b) = VehicleBlueprint::from_seed(s).and_then(|bp| bp.boat().copied()) else {
                continue;
            };
            assert!(
                (0.9..=1.9).contains(&b.hull_len),
                "seed {s} len {}",
                b.hull_len
            );
            assert!((0.3..=0.8).contains(&b.beam), "seed {s} beam {}", b.beam);
            assert!(
                (0.15..=0.4).contains(&b.freeboard),
                "seed {s} fb {}",
                b.freeboard
            );
            // The bow station sits ahead of amidships and behind the hull tip.
            assert!(b.bow_z > 0.0 && b.bow_z < b.hull_len);
            assert!(b.stack_z < 0.0);
            seen += 1;
        }
        assert!(seen > 20, "too few boats sampled: {seen}");
    }

    #[test]
    fn every_stance_is_reachable() {
        let mut seen = [false; 3];
        for s in 0u64..600 {
            if let Some(bp) = VehicleBlueprint::from_seed(s)
                && let Some(b) = bp.boat()
            {
                let i = match b.stance {
                    VehicleStance::Compact => 0,
                    VehicleStance::Sleek => 1,
                    VehicleStance::Heavy => 2,
                };
                seen[i] = true;
            }
        }
        assert_eq!(seen, [true; 3], "some boat stance never sampled");
    }
}
