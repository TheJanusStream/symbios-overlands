//! Seeded gateway spot (#747, relocated #774) — where a room's social
//! gateway stands and where its forecourt landing pose lies.
//!
//! Unlike the settlement (which road-growing themes skip), every seeded
//! room gets exactly one gateway: it is the room's connective tissue, not
//! settlement dressing.
//!
//! Two placements:
//!
//! - [`GatewaySpot::for_landmark`] — the common case. The gate is a
//!   gatehouse on the origin→landmark approach: it stands on that axis a
//!   short way *in front of* the settlement landmark (clearing its
//!   footprint), facing the origin, and the default landing sits just in
//!   front of the gate facing back toward the gate and the settlement
//!   beyond it. Visitors — and the owner logging in — arrive at the
//!   settlement frontage rather than an empty field at the region centre.
//! - [`GatewaySpot::central`] — the fallback for road-growing themes,
//!   which have no concentric settlement to anchor to. A seeded bearing a
//!   short walk from the origin, facing it, landing just in front.
//!
//! Facing convention: yaw follows the spawn path's
//! `Quat::from_rotation_y(deg)`, whose forward vector is
//! `(-sin, 0, -cos)` — so a pose facing world-direction `(dx, dz)` has
//! `yaw = atan2(-dx, -dz)`. The World Editor's `PlayerPose::from_transform`
//! (Environment tab) is the empirically-verified inverse.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use std::f32::consts::TAU;

use crate::seeded_defaults::scene::{range_f32, unit_f32};

/// Sub-stream salt distinct from every sibling room deriver.
const GATEWAY_STREAM_SALT: u64 = 0x6A7E_11A7_6A7E_11A7;

/// Central-fallback distance band from the spawn origin (road themes).
/// Clear of the ±5 m spawn scatter, close enough to read as "the room's
/// gate" rather than a distant object.
const CENTRAL_MIN_DIST: f32 = 12.0;
const CENTRAL_MAX_DIST: f32 = 20.0;

/// Extra gap between the landmark's footprint edge and the gate, on top of
/// both clearances, so the gatehouse frames the approach without crowding
/// the landmark.
const GATE_LANDMARK_MARGIN: f32 = 3.0;

/// Floor on the gate's distance from origin, so a large landmark sitting
/// close to spawn can't push its gatehouse back onto the spawn point.
const MIN_GATE_DIST: f32 = 10.0;

/// How far in front of the gate (toward the origin) the landing sits.
const LANDING_STANDOFF: f32 = 4.5;

/// Floor on the landing's distance from origin, so it clears the ±5 m
/// spawn-scatter square even when the gate is at [`MIN_GATE_DIST`].
const MIN_LANDING_DIST: f32 = 6.0;

/// Derived gateway placement for one room.
#[derive(Clone, Copy, Debug)]
pub struct GatewaySpot {
    /// World XZ of the gate structure's origin.
    pub offset: [f32; 2],
    /// Structure yaw (radians around Y), facing the spawn origin — the
    /// same convention as the settlement landmark.
    pub yaw_rad: f32,
    /// World XZ of the default landing on the gate's forecourt.
    pub landing: [f32; 2],
    /// Landing facing in degrees (spawn `from_rotation_y` convention),
    /// aimed at the gate.
    pub landing_yaw_deg: f32,
}

impl GatewaySpot {
    /// A gatehouse on the origin→landmark approach. `landmark_offset` is
    /// the landmark's world XZ, `landmark_clearance` / `gate_clearance`
    /// its and the gate's footprint radii. The gate stands on the same
    /// ray from the origin, a footprint-clearing standoff in front of the
    /// landmark; the landing sits [`LANDING_STANDOFF`] further toward the
    /// origin.
    pub fn for_landmark(
        landmark_offset: [f32; 2],
        landmark_clearance: f32,
        gate_clearance: f32,
    ) -> Self {
        let d_l = (landmark_offset[0].powi(2) + landmark_offset[1].powi(2))
            .sqrt()
            .max(1e-3);
        let bearing = [landmark_offset[0] / d_l, landmark_offset[1] / d_l];

        let front_standoff = landmark_clearance + gate_clearance + GATE_LANDMARK_MARGIN;
        let gate_dist = (d_l - front_standoff).max(MIN_GATE_DIST);
        let offset = [bearing[0] * gate_dist, bearing[1] * gate_dist];

        let landing_dist = (gate_dist - LANDING_STANDOFF).max(MIN_LANDING_DIST);
        let landing = [bearing[0] * landing_dist, bearing[1] * landing_dist];

        Self::finish(offset, landing, bearing)
    }

    /// Road-theme fallback: a seeded bearing a short walk from the origin.
    pub fn central(room_seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(room_seed ^ GATEWAY_STREAM_SALT);
        let angle = unit_f32(&mut rng) * TAU;
        let dist = range_f32(&mut rng, CENTRAL_MIN_DIST, CENTRAL_MAX_DIST);
        let bearing = [angle.sin(), angle.cos()];
        let offset = [bearing[0] * dist, bearing[1] * dist];

        let landing_dist = (dist - LANDING_STANDOFF).max(MIN_LANDING_DIST);
        let landing = [bearing[0] * landing_dist, bearing[1] * landing_dist];

        Self::finish(offset, landing, bearing)
    }

    /// Shared tail: the gate faces the origin; the landing faces the gate
    /// (i.e. along `+bearing`, away from the origin). `bearing` is the
    /// unit origin→gate direction.
    fn finish(offset: [f32; 2], landing: [f32; 2], bearing: [f32; 2]) -> Self {
        // Gate faces origin: to face direction `d`, yaw = atan2(-d.x, -d.z);
        // facing −bearing gives atan2(bearing.x, bearing.z).
        let yaw_rad = offset[0].atan2(offset[1]);
        // Landing faces the gate, i.e. +bearing.
        let landing_yaw_deg = (-bearing[0]).atan2(-bearing[1]).to_degrees();
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

    /// The spawn path applies `Quat::from_rotation_y(yaw_deg)`, whose
    /// forward is `(-sin, -cos)`. This must equal the landing→gate
    /// direction for every spot, or arrivals face the wrong way (the #747
    /// mirror bug this fixes).
    fn assert_landing_faces_gate(spot: &GatewaySpot, tag: &str) {
        let rad = spot.landing_yaw_deg.to_radians();
        let forward = [-rad.sin(), -rad.cos()];
        let to_gate = [
            spot.offset[0] - spot.landing[0],
            spot.offset[1] - spot.landing[1],
        ];
        let len = (to_gate[0].powi(2) + to_gate[1].powi(2)).sqrt();
        assert!(len > 1e-3, "{tag}: landing coincides with gate");
        let dot = (forward[0] * to_gate[0] + forward[1] * to_gate[1]) / len;
        assert!(
            dot > 0.999,
            "{tag}: landing yaw {} deg does not face the gate (dot {dot})",
            spot.landing_yaw_deg
        );
    }

    #[test]
    fn for_landmark_puts_gate_on_the_approach() {
        // Landmark 40 m out on a slanted bearing, clearance 6, gate 3.5.
        let landmark = [24.0, 32.0]; // |.| = 40
        let spot = GatewaySpot::for_landmark(landmark, 6.0, 3.5);

        let d_l = 40.0_f32;
        let d_g = (spot.offset[0].powi(2) + spot.offset[1].powi(2)).sqrt();
        let d_a = (spot.landing[0].powi(2) + spot.landing[1].powi(2)).sqrt();
        // Gate is in front of the landmark (closer to origin) but not at
        // the centre; landing is in front of the gate.
        assert!(d_g < d_l && d_g >= MIN_GATE_DIST, "gate dist {d_g}");
        assert!(d_a < d_g && d_a >= MIN_LANDING_DIST, "landing dist {d_a}");
        // Gate, landing and landmark are colinear from the origin (same
        // bearing) — the gatehouse sits squarely on the approach.
        let cross = spot.offset[0] * landmark[1] - spot.offset[1] * landmark[0];
        assert!(cross.abs() < 1e-2, "gate off the landmark bearing: {cross}");
        assert_landing_faces_gate(&spot, "for_landmark");
    }

    #[test]
    fn for_landmark_floor_keeps_a_big_close_landmark_off_centre() {
        // Landmark only 30 m out but huge (clearance 25): the naive
        // standoff would push the gate to ~0 m; the floor holds it out.
        let spot = GatewaySpot::for_landmark([0.0, 30.0], 25.0, 3.5);
        let d_g = (spot.offset[0].powi(2) + spot.offset[1].powi(2)).sqrt();
        assert!((d_g - MIN_GATE_DIST).abs() < 1e-3, "gate dist {d_g}");
        assert_landing_faces_gate(&spot, "for_landmark floor");
    }

    #[test]
    fn central_clears_spawn_and_faces_gate() {
        for seed in 0..64u64 {
            let spot = GatewaySpot::central(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let d_g = (spot.offset[0].powi(2) + spot.offset[1].powi(2)).sqrt();
            assert!(
                (CENTRAL_MIN_DIST..=CENTRAL_MAX_DIST).contains(&d_g),
                "seed {seed}: gate at {d_g} m outside the band"
            );
            let d_a = (spot.landing[0].powi(2) + spot.landing[1].powi(2)).sqrt();
            assert!(d_a > 5.0, "seed {seed}: landing inside spawn scatter");
            assert_landing_faces_gate(&spot, "central");
        }
    }

    #[test]
    fn gate_faces_origin() {
        // The gate's own yaw must point its front (−Z rotated) at the
        // origin, matching the settlement landmark convention.
        let spot = GatewaySpot::for_landmark([15.0, 20.0], 5.0, 3.5);
        let rad = spot.yaw_rad;
        let forward = [-rad.sin(), -rad.cos()];
        let to_origin = [-spot.offset[0], -spot.offset[1]];
        let len = (to_origin[0].powi(2) + to_origin[1].powi(2)).sqrt();
        let dot = (forward[0] * to_origin[0] + forward[1] * to_origin[1]) / len;
        assert!(dot > 0.999, "gate does not face origin (dot {dot})");
    }

    #[test]
    fn central_is_deterministic() {
        let a = GatewaySpot::central(0xDEAD_BEEF);
        let b = GatewaySpot::central(0xDEAD_BEEF);
        assert_eq!(a.offset, b.offset);
        assert_eq!(a.landing_yaw_deg, b.landing_yaw_deg);
    }
}
