//! `agent walk-to` (#1418): the agent moves itself the way a person does,
//! toward a point, until it arrives, gets stuck, or is told to stop.
//!
//! There is no movement-command layer in the game to call. Every drive
//! system reads the keyboard (`ButtonInput<KeyCode>`) in `FixedUpdate`, and a
//! body on foot walks where the camera looks. So the controller presses the
//! keys a player would press and, for a walker, turns the orbit camera the
//! way a mouse would; the daemon has no keyboard or mouse of its own to fight
//! it. The physics, the gait and the transform everyone else sees are the
//! game's own.
//!
//! Straight lines only: there is no path-finding here, so a building in the
//! way ends the walk as `stuck`, with the agent's position, and the detour is
//! the agent's to choose. Bodies that fly are not steered yet (#1421).

use bevy::prelude::*;
use bevy_panorbit_camera::PanOrbitCamera;
use serde_json::{Value, json};

use crate::camera::WorldCamera;
use crate::config::agent::{
    ARRIVE_ON_FOOT_M, ARRIVE_WHEELED_M, PROGRESS_STEP_M, STEER_DEAD_ZONE, STUCK_AFTER_SECS,
};
use crate::pds::LocomotionConfig;
use crate::player::LocalMovement;
use crate::player::humanoid::WaterState;
use crate::state::{AppState, LiveAvatarRecord, LocalPlayer, TravelingTo};

use super::super::control::events::{EventKind, MoveOutcome};
use super::observe::EventSink;

/// The keys the controller drives with. It releases all of them whenever it
/// lets go, so a finished walk never leaves a key held down.
const DRIVE_KEYS: [KeyCode; 5] = [
    KeyCode::KeyW,
    KeyCode::KeyA,
    KeyCode::KeyD,
    KeyCode::ShiftLeft,
    KeyCode::Space,
];

/// How a body moves under the keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Drive {
    /// Walks where the camera looks (the humanoid).
    OnFoot,
    /// Drives forward and steers left and right (the car, the hover-boat).
    Wheeled,
}

impl Drive {
    fn of(locomotion: &LocomotionConfig) -> Result<Self, String> {
        match locomotion {
            LocomotionConfig::Humanoid(_) => Ok(Self::OnFoot),
            LocomotionConfig::Car(_) | LocomotionConfig::HoverBoat(_) => Ok(Self::Wheeled),
            LocomotionConfig::Airplane(_) | LocomotionConfig::Helicopter(_) => {
                Err("the agent cannot fly its body yet (#1421)".to_owned())
            }
            LocomotionConfig::Unknown => {
                Err("the agent's body moves in a way this build does not know".to_owned())
            }
        }
    }

    fn arrive_within(self) -> f32 {
        match self {
            Self::OnFoot => ARRIVE_ON_FOOT_M,
            Self::Wheeled => ARRIVE_WHEELED_M,
        }
    }
}

struct Goal {
    id: u64,
    /// The point on the ground, as (x, z).
    target: Vec2,
    run: bool,
    drive: Drive,
    /// The nearest the body has been, and when - what `stuck` is measured by.
    best_distance: f32,
    best_at: f64,
}

/// The walk under way, if any.
#[derive(Resource, Default)]
pub(super) struct Movement {
    goal: Option<Goal>,
    last_id: u64,
}

/// What to hold this frame: keys, and for a walker the camera's yaw.
#[derive(Debug, PartialEq)]
struct Controls {
    keys: Vec<KeyCode>,
    camera_yaw: Option<f32>,
}

/// A walker: face the camera at the point and walk.
fn on_foot(position: Vec3, target: Vec2, run: bool) -> Controls {
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
fn camera_yaw_facing(toward: Vec2) -> f32 {
    (-toward.x).atan2(-toward.y)
}

/// A driven body: throttle on, and steer toward the side the point is on -
/// hard over when it is behind.
fn wheeled(position: Vec3, forward: Vec3, target: Vec2, run: bool) -> Controls {
    let toward = (target - position.xz()).normalize_or_zero();
    let ahead = Vec2::new(forward.x, forward.z).normalize_or_zero();
    // The body's right, as `Transform::right` has it: forward x up.
    let right = Vec2::new(-ahead.y, ahead.x);
    let side = toward.dot(right);
    let behind = toward.dot(ahead) < 0.0;
    let mut keys = vec![KeyCode::KeyW];
    if run {
        keys.push(KeyCode::ShiftLeft);
    }
    if side > STEER_DEAD_ZONE || (behind && side >= 0.0) {
        keys.push(KeyCode::KeyD);
    } else if side < -STEER_DEAD_ZONE || behind {
        keys.push(KeyCode::KeyA);
    }
    Controls {
        keys,
        camera_yaw: None,
    }
}

/// `angle` moved by whole turns to within half a turn of `from`, so a camera
/// easing toward it goes the short way round.
fn nearest_turn(from: f32, angle: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    from + (angle - from + PI).rem_euclid(TAU) - PI
}

/// Start walking to `target` (x, z), replacing any walk under way.
pub(super) fn walk_to(world: &mut World, target: Vec2, run: bool) -> Result<Value, String> {
    if *world.resource::<State<AppState>>().get() != AppState::InGame {
        return Err("the agent is not in a world yet".to_owned());
    }
    if world.contains_resource::<TravelingTo>() {
        return Err("the agent is travelling".to_owned());
    }
    if !target.is_finite() {
        return Err("the point must be two finite numbers".to_owned());
    }
    let drive = Drive::of(&world.resource::<LiveAvatarRecord>().0.locomotion)?;
    let position = local_position(world).ok_or("the agent has no body yet")?;
    let now = world.resource::<Time<Real>>().elapsed_secs_f64();
    let distance = position.xz().distance(target);
    let mut movement = world.get_resource_or_init::<Movement>();
    let replaced = movement.goal.take();
    movement.last_id += 1;
    let id = movement.last_id;
    movement.goal = Some(Goal {
        id,
        target,
        run,
        drive,
        best_distance: distance,
        best_at: now,
    });
    if let Some(old) = replaced {
        end(world, &old, MoveOutcome::Replaced, position);
    }
    // The cursor a caller that wants to wait for this walk's end reads the
    // event log from: nothing before it can be this walk's.
    let events_seq = world.resource::<EventSink>().0.last_seq();
    Ok(json!({
        "goal_id": id,
        "to": [target.x, target.y],
        "distance_m": round(distance),
        "events_seq": events_seq,
    }))
}

/// Stop walking, if walking.
pub(super) fn halt(world: &mut World) -> Result<Value, String> {
    let goal = world.get_resource_or_init::<Movement>().goal.take();
    let Some(goal) = goal else {
        return Ok(json!({ "halted": null }));
    };
    let position = local_position(world).unwrap_or(Vec3::ZERO);
    release_keys(&mut world.resource_mut::<ButtonInput<KeyCode>>());
    end(world, &goal, MoveOutcome::Halted, position);
    Ok(json!({ "halted": goal.id }))
}

fn local_position(world: &mut World) -> Option<Vec3> {
    world
        .query_filtered::<&GlobalTransform, With<LocalPlayer>>()
        .iter(world)
        .next()
        .map(GlobalTransform::translation)
}

/// Record how `goal` ended.
fn end(world: &World, goal: &Goal, outcome: MoveOutcome, position: Vec3) {
    world
        .resource::<EventSink>()
        .0
        .push(EventKind::MovementEnded {
            goal_id: goal.id,
            outcome,
            position: [round(position.x), round(position.y), round(position.z)],
            distance_left_m: round(position.xz().distance(goal.target)),
        });
}

fn release_keys(keys: &mut ButtonInput<KeyCode>) {
    for key in DRIVE_KEYS {
        keys.release(key);
    }
}

/// `PreUpdate`, after input: hold the keys (and turn the camera) that carry
/// the body toward the goal, or end the walk.
#[allow(clippy::too_many_arguments)]
pub(super) fn steer(
    mut movement: ResMut<Movement>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    player: Query<&GlobalTransform, With<LocalPlayer>>,
    mut camera: Query<&mut PanOrbitCamera, With<WorldCamera>>,
    local: Option<Res<LocalMovement>>,
    state: Res<State<AppState>>,
    traveling: Option<Res<TravelingTo>>,
    time: Res<Time<Real>>,
    sink: Res<EventSink>,
) {
    let Some(goal) = movement.goal.as_mut() else {
        return;
    };
    let body = player.single().ok();
    let position = body.map_or(Vec3::ZERO, GlobalTransform::translation);
    let distance = position.xz().distance(goal.target);
    let now = time.elapsed_secs_f64();
    if distance + PROGRESS_STEP_M <= goal.best_distance {
        goal.best_distance = distance;
        goal.best_at = now;
    }
    let outcome = if *state.get() != AppState::InGame || traveling.is_some() || body.is_none() {
        Some(MoveOutcome::Interrupted)
    } else if distance <= goal.drive.arrive_within() {
        Some(MoveOutcome::Arrived)
    } else if now - goal.best_at >= STUCK_AFTER_SECS {
        Some(MoveOutcome::Stuck)
    } else {
        None
    };
    if let Some(outcome) = outcome {
        release_keys(&mut keys);
        sink.0.push(EventKind::MovementEnded {
            goal_id: goal.id,
            outcome,
            position: [round(position.x), round(position.y), round(position.z)],
            distance_left_m: round(distance),
        });
        movement.goal = None;
        return;
    }
    let Some(body) = body else {
        return;
    };
    let controls = match goal.drive {
        Drive::OnFoot => on_foot(position, goal.target, goal.run),
        Drive::Wheeled => wheeled(position, body.forward().as_vec3(), goal.target, goal.run),
    };
    for key in DRIVE_KEYS {
        if controls.keys.contains(&key) {
            keys.press(key);
        } else {
            keys.release(key);
        }
    }
    if let (Some(yaw), Ok(mut orbit)) = (controls.camera_yaw, camera.single_mut()) {
        // Turned at once, the way a mouse can turn it, rather than eased
        // toward: a body that walks where the camera looks overshoots while
        // an easing camera catches up, and a running one ended up circling
        // its goal just outside arm's reach. Nobody watches this camera.
        let yaw = nearest_turn(orbit.yaw.unwrap_or(orbit.target_yaw), yaw);
        orbit.yaw = Some(yaw);
        orbit.target_yaw = yaw;
        // A swimmer goes where the camera looks in three dimensions, so a
        // camera pitched down to watch the ground would steer it into the
        // lake bed. Level it while swimming.
        if local
            .as_deref()
            .is_some_and(|m| matches!(m.water, WaterState::Swimming { .. }))
        {
            orbit.pitch = Some(0.0);
            orbit.target_pitch = 0.0;
        }
        orbit.force_update = true;
    }
}

/// Centimetres are plenty.
fn round(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
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

    /// A car facing +Z has -X on its right (forward x up), so a point off to
    /// -X steers right with D, and one straight ahead does not steer.
    #[test]
    fn a_driven_body_steers_toward_the_points_side() {
        let facing_z = Vec3::Z;
        let right_of_it = wheeled(Vec3::ZERO, facing_z, Vec2::new(-10.0, 10.0), false);
        assert!(right_of_it.keys.contains(&KeyCode::KeyD), "{right_of_it:?}");
        let left_of_it = wheeled(Vec3::ZERO, facing_z, Vec2::new(10.0, 10.0), false);
        assert!(left_of_it.keys.contains(&KeyCode::KeyA), "{left_of_it:?}");
        let ahead = wheeled(Vec3::ZERO, facing_z, Vec2::new(0.0, 10.0), false);
        assert_eq!(ahead.keys, [KeyCode::KeyW]);
    }

    /// Straight behind is still a turn, not a stall: the body swings round.
    #[test]
    fn a_point_behind_turns_the_body_round() {
        let behind = wheeled(Vec3::ZERO, Vec3::Z, Vec2::new(0.0, -10.0), false);
        assert!(
            behind.keys.contains(&KeyCode::KeyA) || behind.keys.contains(&KeyCode::KeyD),
            "{behind:?}"
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
