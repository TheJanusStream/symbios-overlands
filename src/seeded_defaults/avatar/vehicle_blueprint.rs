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
            // length as a literal (the coupling #783 removes everywhere).
            bow_z: hull_len * 0.59,
            stack_z: -hull_len * 0.42,
            ornament_z: hull_len * 0.076,
        }
    }
}

/// Per-family vehicle proportion blueprint. One variant per chassis that has
/// been wired to the shared-landmark system; [`VehicleBlueprint::from_seed`]
/// yields `None` for a chassis without one yet (and for the humanoid).
#[derive(Clone, Copy, Debug)]
pub enum VehicleBlueprint {
    Boat(BoatBlueprint),
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
            // Airship / skiff blueprints land with their redesigns; the
            // humanoid uses HumanoidBlueprint.
            _ => None,
        }
    }

    /// The boat blueprint, if this is a boat.
    pub fn boat(&self) -> Option<&BoatBlueprint> {
        match self {
            Self::Boat(b) => Some(b),
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
    fn only_boats_get_a_blueprint_so_far() {
        for s in 0u64..200 {
            let bp = VehicleBlueprint::from_seed(s);
            match ChassisFamily::for_seed(s) {
                ChassisFamily::Boat => {
                    assert!(bp.is_some(), "seed {s}: boat without a blueprint");
                    assert!(bp.unwrap().boat().is_some());
                }
                _ => assert!(bp.is_none(), "seed {s}: non-boat got a blueprint"),
            }
        }
    }

    #[test]
    fn boat_dims_stay_in_sane_range() {
        // Every boat seed must land inside a band that keeps the hull a
        // believable, sanitiser-safe size (no zero/exploded dimensions).
        let mut seen = 0;
        for s in 0u64..600 {
            let Some(bp) = VehicleBlueprint::from_seed(s) else {
                continue;
            };
            let b = bp.boat().unwrap();
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
