//! How a body on the ground is steered (#1418, #1421): a walker, which goes
//! where the orbit camera looks, and a driven body - a car or a hover-boat -
//! which steers by a torque, and so turns on the spot.

use bevy::prelude::*;

use crate::config::agent::{
    ARRIVE_ON_FOOT_M, ARRIVE_WHEELED_M, FACE_STEP_SECS, FOLLOW_SLACK_M, PROGRESS_STEP_M,
    STEER_DEAD_ZONE, STUCK_AFTER_SECS, SWING_FIRST_DEG,
};

use super::{Controls, heading_off};

/// How a body on the ground moves under the keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Ground {
    /// Walks where the camera looks (the humanoid).
    OnFoot,
    /// Drives forward and steers left and right (the car, the hover-boat).
    Wheeled,
}

impl Ground {
    /// How close to its point a walk has arrived.
    pub(super) fn arrive_within(self) -> f32 {
        match self {
            Self::OnFoot => ARRIVE_ON_FOOT_M,
            Self::Wheeled => ARRIVE_WHEELED_M,
        }
    }

    /// How long a turn may take: a step on foot, a swing round on wheels.
    pub(super) fn turn_within(self) -> f64 {
        match self {
            Self::OnFoot => FACE_STEP_SECS,
            Self::Wheeled => STUCK_AFTER_SECS,
        }
    }

    /// The controls that carry the body from `position` toward `target`.
    pub(super) fn toward(self, position: Vec3, forward: Vec3, target: Vec2, run: bool) -> Controls {
        match self {
            Self::OnFoot => on_foot(position, target, run),
            Self::Wheeled => wheeled(position, forward, target, run),
        }
    }

    /// The controls that turn the body toward `dir`: a step aimed
    /// `bias_deg` past it for a walker, a swing on the spot for a driven
    /// body.
    pub(super) fn turn(self, position: Vec3, forward: Vec3, dir: Vec2, bias_deg: f32) -> Controls {
        match self {
            Self::OnFoot => on_foot(position, position.xz() + turned(dir, bias_deg), false),
            Self::Wheeled => swing(forward, dir),
        }
    }
}

/// A walker: face the camera at the point and walk.
pub(super) fn on_foot(position: Vec3, target: Vec2, run: bool) -> Controls {
    let toward = target - position.xz();
    let mut keys = vec![KeyCode::KeyW];
    if run {
        keys.push(KeyCode::ShiftLeft);
    }
    Controls {
        keys,
        camera_yaw: Some(camera_yaw_facing(toward)),
    }
}

/// The orbit camera's yaw that looks along `toward`. The camera sits at
/// `focus + R(yaw) * (0, 0, radius)` looking back at the focus, so at level
/// pitch its forward is `(-sin yaw, 0, -cos yaw)`.
pub(in super::super) fn camera_yaw_facing(toward: Vec2) -> f32 {
    (-toward.x).atan2(-toward.y)
}

/// A driven body: swing round on the spot to a point well off the nose,
/// then throttle on and steer toward the side the point is on.
pub(super) fn wheeled(position: Vec3, forward: Vec3, target: Vec2, run: bool) -> Controls {
    let toward = (target - position.xz()).normalize_or_zero();
    if heading_off(forward, toward).abs() > SWING_FIRST_DEG {
        return swing(forward, toward);
    }
    let ahead = Vec2::new(forward.x, forward.z).normalize_or_zero();
    // The body's right, as `Transform::right` has it: forward x up.
    let right = Vec2::new(-ahead.y, ahead.x);
    let side = toward.dot(right);
    let mut keys = vec![KeyCode::KeyW];
    if run {
        keys.push(KeyCode::ShiftLeft);
    }
    if side > STEER_DEAD_ZONE {
        keys.push(KeyCode::KeyD);
    } else if side < -STEER_DEAD_ZONE {
        keys.push(KeyCode::KeyA);
    }
    Controls {
        keys,
        camera_yaw: None,
    }
}

/// A driven body turning on the spot: steering alone, toward the side
/// `toward` is on. The steering is a torque, so it turns a standing car or
/// boat as well as a moving one.
pub(super) fn swing(forward: Vec3, toward: Vec2) -> Controls {
    let ahead = Vec2::new(forward.x, forward.z).normalize_or_zero();
    let right = Vec2::new(-ahead.y, ahead.x);
    let key = if toward.dot(right) >= 0.0 {
        KeyCode::KeyD
    } else {
        KeyCode::KeyA
    };
    Controls {
        keys: vec![key],
        camera_yaw: None,
    }
}

/// Whether a follower moves this frame, given the gap to its player: it
/// sets off when they are further than `keep` by [`FOLLOW_SLACK_M`], and
/// stands once it is back within `keep`.
pub(super) fn follow_moves(gap: f32, keep: f32, closing: &mut bool) -> bool {
    if *closing && gap <= keep {
        *closing = false;
    } else if !*closing && gap > keep + FOLLOW_SLACK_M {
        *closing = true;
    }
    *closing
}

/// Whether a follower closing on its player is blocked: it has not got
/// [`PROGRESS_STEP_M`] from where it stood for [`STUCK_AFTER_SECS`]. Getting
/// somewhere moves `still_at` and `still_since` up to here and now.
pub(super) fn follow_blocked(
    here: Vec2,
    still_at: &mut Vec2,
    still_since: &mut f64,
    now: f64,
) -> bool {
    if here.distance(*still_at) >= PROGRESS_STEP_M {
        *still_at = here;
        *still_since = now;
        return false;
    }
    now - *still_since >= STUCK_AFTER_SECS
}

/// `angle` moved by whole turns to within half a turn of `from`, so a camera
/// easing toward it goes the short way round.
pub(super) fn nearest_turn(from: f32, angle: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    from + (angle - from + PI).rem_euclid(TAU) - PI
}

/// `dir` turned `degrees` clockwise - toward its own right.
pub(super) fn turned(dir: Vec2, degrees: f32) -> Vec2 {
    let right = Vec2::new(-dir.y, dir.x);
    let (sin, cos) = degrees.to_radians().sin_cos();
    dir * cos + right * sin
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forward_of_yaw(yaw: f32) -> Vec2 {
        // What the orbit camera's transform makes of a yaw at level pitch.
        let forward = Quat::from_axis_angle(Vec3::Y, yaw) * Vec3::NEG_Z;
        Vec2::new(forward.x, forward.z)
    }

    /// The camera the controller aims ends up looking where the agent is
    /// going, in every quadrant - checked against the camera's own rotation,
    /// not against the formula that produced the yaw.
    #[test]
    fn the_camera_yaw_looks_toward_the_point() {
        for toward in [
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(-3.0, -4.0),
            Vec2::new(5.0, -1.0),
        ] {
            let looks = forward_of_yaw(camera_yaw_facing(toward));
            assert!(
                looks.distance(toward.normalize()) < 1e-5,
                "{toward:?} -> {looks:?}"
            );
        }
    }

    #[test]
    fn a_walker_walks_and_runs_on_request() {
        let walking = on_foot(Vec3::ZERO, Vec2::new(0.0, 10.0), false);
        assert_eq!(walking.keys, [KeyCode::KeyW]);
        let running = on_foot(Vec3::ZERO, Vec2::new(0.0, 10.0), true);
        assert_eq!(running.keys, [KeyCode::KeyW, KeyCode::ShiftLeft]);
    }

    /// A car facing +Z has -X on its right (forward x up), so a point a
    /// little off to -X steers right with D on the move, and one straight
    /// ahead does not steer.
    #[test]
    fn a_driven_body_steers_toward_the_points_side() {
        let facing_z = Vec3::Z;
        let right_of_it = wheeled(Vec3::ZERO, facing_z, Vec2::new(-3.0, 10.0), false);
        assert_eq!(right_of_it.keys, [KeyCode::KeyW, KeyCode::KeyD]);
        let left_of_it = wheeled(Vec3::ZERO, facing_z, Vec2::new(3.0, 10.0), false);
        assert_eq!(left_of_it.keys, [KeyCode::KeyW, KeyCode::KeyA]);
        let ahead = wheeled(Vec3::ZERO, facing_z, Vec2::new(0.0, 10.0), false);
        assert_eq!(ahead.keys, [KeyCode::KeyW]);
    }

    /// A point well off the nose - to the side, or straight behind - is
    /// swung round to on the spot, throttle off, before the body drives:
    /// driving and turning at once ran a car into what stood in front of it.
    #[test]
    fn a_point_well_off_the_nose_is_swung_round_to_first() {
        let facing_z = Vec3::Z;
        let beside = wheeled(Vec3::ZERO, facing_z, Vec2::new(-10.0, 0.0), false);
        assert_eq!(beside.keys, [KeyCode::KeyD]);
        let behind = wheeled(Vec3::ZERO, facing_z, Vec2::new(0.0, -10.0), false);
        assert!(!behind.keys.contains(&KeyCode::KeyW), "{behind:?}");
        assert_eq!(behind.keys.len(), 1, "{behind:?}");
    }

    /// A turn on the spot steers alone - no throttle - toward the side the
    /// direction is on, and the heading it is measured by is clockwise.
    #[test]
    fn a_driven_body_turns_on_the_spot_toward_the_side() {
        let facing_z = Vec3::Z;
        assert_eq!(swing(facing_z, Vec2::new(-1.0, 0.0)).keys, [KeyCode::KeyD]);
        assert_eq!(swing(facing_z, Vec2::new(1.0, 0.0)).keys, [KeyCode::KeyA]);
        assert!((heading_off(facing_z, Vec2::new(-1.0, 0.0)) - 90.0).abs() < 1e-3);
        assert!((heading_off(facing_z, Vec2::new(1.0, 0.0)) + 90.0).abs() < 1e-3);
        assert!(heading_off(facing_z, Vec2::new(0.0, 1.0)).abs() < 1e-3);
    }

    /// A follower sets off when its player is further than the distance by
    /// the slack, stands once back within the distance, and does not start
    /// and stop at the edge of it.
    #[test]
    fn a_follower_waits_for_the_slack_before_setting_off_again() {
        let keep = 3.0;
        let mut closing = true;
        assert!(follow_moves(10.0, keep, &mut closing), "far away: go");
        assert!(
            !follow_moves(2.9, keep, &mut closing),
            "within the distance: stand"
        );
        assert!(
            !follow_moves(keep + FOLLOW_SLACK_M - 0.1, keep, &mut closing),
            "inside the slack: keep standing"
        );
        assert!(
            follow_moves(keep + FOLLOW_SLACK_M + 0.1, keep, &mut closing),
            "past the slack: go again"
        );
        assert!(
            follow_moves(keep + 0.5, keep, &mut closing),
            "and keep going in"
        );
    }

    /// Aiming past a direction turns it the way `heading_off` measures:
    /// clockwise is positive, so a step that came to rest short on the left
    /// is re-aimed further right by exactly that much.
    #[test]
    fn a_biased_aim_turns_the_way_the_error_is_measured() {
        let dir = Vec2::new(0.0, 1.0);
        let right_of_it = turned(dir, 90.0);
        assert!(
            right_of_it.distance(Vec2::new(-1.0, 0.0)) < 1e-5,
            "{right_of_it}"
        );
        let facing = Vec3::new(0.3, 0.0, 1.0).normalize();
        let off = heading_off(facing, dir);
        let corrected = turned(dir, off);
        assert!(
            (heading_off(facing, corrected) - 2.0 * off).abs() < 1e-3,
            "the aim moves by the error again, past the direction"
        );
    }

    /// THE CASE THAT ASKED FOR THIS: a follower running after a player who
    /// walked away just as fast was called blocked - the gap never closed.
    /// A follower is blocked when IT gets nowhere, whatever the gap does.
    #[test]
    fn a_follower_is_blocked_by_standing_still_not_by_a_steady_gap() {
        let (mut still_at, mut still_since) = (Vec2::ZERO, 0.0);
        for second in 1..=10 {
            let here = Vec2::new(0.0, second as f32 * 3.0);
            assert!(
                !follow_blocked(here, &mut still_at, &mut still_since, f64::from(second)),
                "running along at second {second}"
            );
        }
        let stopped = Vec2::new(0.1, 30.1);
        assert!(!follow_blocked(
            stopped,
            &mut still_at,
            &mut still_since,
            12.0
        ));
        assert!(
            follow_blocked(
                stopped,
                &mut still_at,
                &mut still_since,
                10.0 + STUCK_AFTER_SECS
            ),
            "nowhere for as long as a walk takes to be stuck"
        );
    }

    #[test]
    fn the_camera_turns_the_short_way_round() {
        use std::f32::consts::PI;
        let turned = nearest_turn(0.1, 2.0 * PI - 0.1);
        assert!((turned - (-0.1)).abs() < 1e-5, "{turned}");
        let far = nearest_turn(10.0 * PI, 0.5);
        assert!((far - 10.0 * PI - 0.5).abs() < 1e-4, "{far}");
    }
}
