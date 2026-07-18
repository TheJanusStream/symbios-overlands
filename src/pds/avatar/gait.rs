//! Gait / idle-motion parameters — the record-side counterpart of the
//! seeded [`AvatarGait`] derivation.
//!
//! Historically the idle-motion amplitudes were derived from the owner's
//! DID on every peer and were not part of the avatar record at all
//! (#659/#797). This struct makes them authorable: an avatar record MAY
//! carry a `gait` section; when it is absent every peer falls back to the
//! DID-seeded derivation exactly as before, so pre-existing records keep
//! their published look without migration.

use super::locomotion::clamp_pos;
use crate::pds::types::Fp;
use crate::seeded_defaults::AvatarGait;
use serde::{Deserialize, Serialize};

/// Authorable idle-motion tuning, mirroring the five seeded
/// [`AvatarGait`] fields plus an overall intensity multiplier that lets
/// a chassis wallow theatrically or sit near-still without re-tuning
/// every amplitude. Consumed by `player::gait` for whichever idle
/// profile the locomotion preset selects (humanoid sway, boat heave,
/// airship drift, skiff shiver).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GaitParams {
    /// Walking step cadence (steps / sec). Humanoid profile only.
    pub step_cadence: Fp,
    /// Vertical bounce per step (m). Humanoid profile only.
    pub step_bounce_amplitude: Fp,
    /// Idle-sway frequency (Hz). Also paces the boat swell.
    pub idle_sway_frequency: Fp,
    /// Idle-sway amplitude (m). Vehicle profiles scale their heave /
    /// shiver from this.
    pub idle_sway_amplitude: Fp,
    /// Idle head-turn / nose-wander range (degrees, ± from forward).
    pub head_turn_variance_degrees: Fp,
    /// Overall idle-motion intensity multiplier applied to the composed
    /// offsets (1.0 = authored amplitudes, 0.0 = perfectly still).
    pub idle_intensity: Fp,
}

impl GaitParams {
    /// The gait a record without an explicit `gait` section exhibits —
    /// the DID/seed derivation shared with
    /// [`AvatarGait::for_seed`](crate::seeded_defaults::AvatarGait::for_seed).
    /// Also the re-roll path: same seed, same gait on every peer.
    pub fn for_seed(seed: u64) -> Self {
        let g = AvatarGait::for_seed(seed);
        Self {
            step_cadence: Fp(g.step_cadence),
            step_bounce_amplitude: Fp(g.step_bounce_amplitude),
            idle_sway_frequency: Fp(g.idle_sway_frequency),
            idle_sway_amplitude: Fp(g.idle_sway_amplitude),
            head_turn_variance_degrees: Fp(g.head_turn_variance_degrees),
            idle_intensity: Fp::ONE,
        }
    }

    /// The runtime amplitude struct `player::gait` animates with. The
    /// intensity multiplier stays separate — it scales the composed
    /// offsets, not the individual amplitudes, so cadence maths keep
    /// reading the authored values.
    pub fn to_runtime(&self) -> AvatarGait {
        AvatarGait {
            step_cadence: self.step_cadence.0,
            step_bounce_amplitude: self.step_bounce_amplitude.0,
            idle_sway_frequency: self.idle_sway_frequency.0,
            idle_sway_amplitude: self.idle_sway_amplitude.0,
            head_turn_variance_degrees: self.head_turn_variance_degrees.0,
        }
    }

    /// In-place numeric clamp, same contract as
    /// [`LocomotionPreset::sanitize`](super::locomotion::LocomotionPreset::sanitize).
    /// Ranges deliberately extend past the seeded derivation's so an
    /// author can push amplitudes the dice never roll, while still
    /// keeping a hostile record from driving the visual root to
    /// absurdity.
    pub fn sanitize(&mut self) {
        self.step_cadence = clamp_pos(self.step_cadence, 0.2, 6.0);
        self.step_bounce_amplitude = clamp_pos(self.step_bounce_amplitude, 0.0, 0.3);
        self.idle_sway_frequency = clamp_pos(self.idle_sway_frequency, 0.0, 3.0);
        self.idle_sway_amplitude = clamp_pos(self.idle_sway_amplitude, 0.0, 0.2);
        self.head_turn_variance_degrees = clamp_pos(self.head_turn_variance_degrees, 0.0, 60.0);
        self.idle_intensity = clamp_pos(self.idle_intensity, 0.0, 3.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_params_match_the_avatar_gait_derivation() {
        let seed = crate::seeded_defaults::fnv1a_64("did:plc:gait-record-test");
        let p = GaitParams::for_seed(seed);
        let g = AvatarGait::for_seed(seed);
        assert_eq!(p.step_cadence.0, g.step_cadence);
        assert_eq!(p.step_bounce_amplitude.0, g.step_bounce_amplitude);
        assert_eq!(p.idle_sway_frequency.0, g.idle_sway_frequency);
        assert_eq!(p.idle_sway_amplitude.0, g.idle_sway_amplitude);
        assert_eq!(p.head_turn_variance_degrees.0, g.head_turn_variance_degrees);
        assert_eq!(p.idle_intensity, Fp::ONE);
    }

    #[test]
    fn seeded_params_survive_sanitize_unchanged() {
        for s in 0u64..32 {
            let p = GaitParams::for_seed(s.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let mut q = p.clone();
            q.sanitize();
            assert_eq!(p, q, "seeded values must sit inside the clamp ranges");
        }
    }

    #[test]
    fn sanitize_collapses_hostile_values() {
        let mut p = GaitParams {
            step_cadence: Fp(f32::NAN),
            step_bounce_amplitude: Fp(1e9),
            idle_sway_frequency: Fp(-5.0),
            idle_sway_amplitude: Fp(f32::INFINITY),
            head_turn_variance_degrees: Fp(1e6),
            idle_intensity: Fp(-1.0),
        };
        p.sanitize();
        assert_eq!(p.step_cadence.0, 0.2);
        assert_eq!(p.step_bounce_amplitude.0, 0.3);
        assert_eq!(p.idle_sway_frequency.0, 0.0);
        assert_eq!(p.idle_sway_amplitude.0, 0.0);
        assert_eq!(p.head_turn_variance_degrees.0, 60.0);
        assert_eq!(p.idle_intensity.0, 0.0);
    }
}
