//! Avatar gait + idle animation timing.
//!
//! Computed by the deriver but **not yet consumed** anywhere: the
//! current default avatar is a hover-boat whose locomotion preset
//! ignores step / sway data, and there's no humanoid avatar default
//! to feed it into yet. Defining the surface now keeps it ready —
//! a future humanoid spawn path or animation system can read these
//! fields directly without needing to extend the deriver.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::seeded_defaults::hash::fnv1a_64;
use crate::seeded_defaults::scene::range_f32;

const AVATAR_GAIT_SALT: u64 = 0x6A1D_6A1D_6A1D_6A1D;

#[derive(Clone, Copy, Debug)]
pub struct AvatarGait {
    /// Walking step cadence (steps / sec).
    pub step_cadence: f32,
    /// Vertical bounce per step (m).
    pub step_bounce_amplitude: f32,
    /// Idle-sway frequency (Hz).
    pub idle_sway_frequency: f32,
    /// Idle-sway amplitude (m).
    pub idle_sway_amplitude: f32,
    /// Idle head-turn range (degrees, ± from forward).
    pub head_turn_variance_degrees: f32,
}

impl AvatarGait {
    pub fn for_did(did: &str) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(fnv1a_64(did) ^ AVATAR_GAIT_SALT);
        Self {
            step_cadence: range_f32(&mut rng, 1.8, 2.6),
            step_bounce_amplitude: range_f32(&mut rng, 0.01, 0.05),
            idle_sway_frequency: range_f32(&mut rng, 0.4, 1.2),
            idle_sway_amplitude: range_f32(&mut rng, 0.005, 0.025),
            head_turn_variance_degrees: range_f32(&mut rng, 5.0, 20.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = AvatarGait::for_did("did:plc:test");
        let b = AvatarGait::for_did("did:plc:test");
        assert_eq!(a.step_cadence, b.step_cadence);
        assert_eq!(a.idle_sway_amplitude, b.idle_sway_amplitude);
    }

    #[test]
    fn ranges() {
        for s in 0u64..16 {
            let g = AvatarGait::for_did(&format!("did:test:{s}"));
            assert!((1.8..=2.6).contains(&g.step_cadence));
            assert!((0.01..=0.05).contains(&g.step_bounce_amplitude));
            assert!((0.4..=1.2).contains(&g.idle_sway_frequency));
            assert!((0.005..=0.025).contains(&g.idle_sway_amplitude));
            assert!((5.0..=20.0).contains(&g.head_turn_variance_degrees));
        }
    }
}
