//! Seeded gateway spot (#747) — where a room's social gateway stands and
//! where its forecourt landing pose lies.
//!
//! Unlike the settlement (which road-growing themes skip), every seeded
//! room gets exactly one gateway: it is the room's connective tissue, not
//! settlement dressing. The gate stands on a seeded bearing a short walk
//! from the spawn origin, outside the ±5 m legacy spawn-scatter square,
//! facing the origin; the default landing sits on the origin side of the
//! gate, facing it — so a visitor arriving through the gateway steps out
//! of the gate they conceptually came through.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use std::f32::consts::TAU;

use crate::seeded_defaults::scene::{range_f32, unit_f32};

/// Sub-stream salt distinct from every sibling room deriver.
const GATEWAY_STREAM_SALT: u64 = 0x6A7E_11A7_6A7E_11A7;

/// Distance band from the spawn origin. Closer than the settlement
/// landmark's ≥30 m band, so the gate reads as "yours" rather than part
/// of the settlement cluster, but clear of the ±5 m spawn scatter.
const MIN_DIST: f32 = 12.0;
const MAX_DIST: f32 = 20.0;

/// How far in front of the gate (toward the origin) the landing sits.
const LANDING_STANDOFF: f32 = 4.5;

/// Derived gateway placement for one room seed.
#[derive(Clone, Copy, Debug)]
pub struct GatewaySpot {
    /// World XZ of the gate structure's origin.
    pub offset: [f32; 2],
    /// Structure yaw (radians around Y), facing the spawn origin — the
    /// same convention as the settlement landmark.
    pub yaw_rad: f32,
    /// World XZ of the default landing on the gate's forecourt.
    pub landing: [f32; 2],
    /// Landing facing in degrees (0 faces −Z, 90 faces +X — the
    /// landmark-link convention), aimed at the gate.
    pub landing_yaw_deg: f32,
}

impl GatewaySpot {
    pub fn from_seed(room_seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(room_seed ^ GATEWAY_STREAM_SALT);
        let angle = unit_f32(&mut rng) * TAU;
        let dist = range_f32(&mut rng, MIN_DIST, MAX_DIST);
        let offset = [angle.sin() * dist, angle.cos() * dist];

        // Face the origin (hero convention: models face −Z at yaw 0).
        let yaw_rad = offset[0].atan2(offset[1]);

        // Landing on the origin side of the gate, looking at it.
        let t = (dist - LANDING_STANDOFF) / dist;
        let landing = [offset[0] * t, offset[1] * t];
        let to_gate = [offset[0] - landing[0], offset[1] - landing[1]];
        let landing_yaw_deg = to_gate[0].atan2(-to_gate[1]).to_degrees();

        Self {
            offset,
            yaw_rad,
            landing,
            landing_yaw_deg,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_clears_spawn_square_and_landing_sits_in_front() {
        for seed in 0..64u64 {
            let spot = GatewaySpot::from_seed(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let gate_d = (spot.offset[0].powi(2) + spot.offset[1].powi(2)).sqrt();
            assert!(
                (MIN_DIST..=MAX_DIST).contains(&gate_d),
                "seed {seed}: gate at {gate_d}m outside the band"
            );
            let landing_d = (spot.landing[0].powi(2) + spot.landing[1].powi(2)).sqrt();
            assert!(
                (gate_d - landing_d - LANDING_STANDOFF).abs() < 1e-3,
                "seed {seed}: landing not {LANDING_STANDOFF}m in front of the gate"
            );
            assert!(
                landing_d > 5.0,
                "seed {seed}: landing inside the ±5m spawn-scatter square"
            );
        }
    }

    /// The landing yaw (deg, 0 = −Z, 90 = +X) must aim at the gate: the
    /// facing vector reconstructed from it matches the landing→gate
    /// direction.
    #[test]
    fn landing_faces_the_gate() {
        for seed in [1u64, 42, 4096, u64::MAX / 3] {
            let spot = GatewaySpot::from_seed(seed);
            let rad = spot.landing_yaw_deg.to_radians();
            let forward = [rad.sin(), -rad.cos()];
            let to_gate = [
                spot.offset[0] - spot.landing[0],
                spot.offset[1] - spot.landing[1],
            ];
            let len = (to_gate[0].powi(2) + to_gate[1].powi(2)).sqrt();
            let dot = (forward[0] * to_gate[0] + forward[1] * to_gate[1]) / len;
            assert!(
                dot > 0.999,
                "seed {seed}: landing yaw {} deg does not face the gate (dot {dot})",
                spot.landing_yaw_deg
            );
        }
    }

    #[test]
    fn spot_is_deterministic() {
        let a = GatewaySpot::from_seed(0xDEAD_BEEF);
        let b = GatewaySpot::from_seed(0xDEAD_BEEF);
        assert_eq!(a.offset, b.offset);
        assert_eq!(a.landing_yaw_deg, b.landing_yaw_deg);
    }
}
