//! `agent walk-to`, `follow` and `face` (#1418, #1421): the agent moves
//! itself the way a person does - toward a point, after another player, or
//! round to face something - until it gets there, gets stuck, or is told to
//! stop.
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
//! way ends a walk as `stuck`, with the agent's position, and the detour is
//! the agent's to choose. Bodies that fly are not steered yet (#1421).
//!
//! A follow is a walk whose point moves: every frame it aims at where the
//! player it follows is drawn, stands once it is within its distance, and
//! sets off again when they get further than that by a margin - running to
//! catch up when they are far ahead. It ends when it is halted or replaced,
//! when the player leaves, or on travel; blocked, it says so once and keeps
//! trying. A player whose body has never been placed - a sleeping browser
//! tab, still at its spawn stand-in - is waited for, not walked to.
//!
//! A turn (`face`) is how a person turns. There is no turning on the spot on
//! foot: a walker steps toward the direction until it faces it. A car or a
//! hover-boat swings round under its steering alone, throttle off.

use bevy::prelude::*;
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde_json::{Value, json};

use crate::camera::WorldCamera;
use crate::config::agent::{
    ARRIVE_ON_FOOT_M, ARRIVE_WHEELED_M, CAMERA_CAUGHT_UP_DEG, FACE_PULSE_SECS, FACE_SETTLE_SECS,
    FACE_STEP_SECS, FACE_TOLERANCE_DEG, FOLLOW_RUN_BEYOND_M, FOLLOW_SLACK_M, PROGRESS_STEP_M,
    STEER_DEAD_ZONE, STUCK_AFTER_SECS, SWING_FIRST_DEG,
};
use crate::network::PeerResolve;
use crate::pds::LocomotionConfig;
use crate::player::LocalMovement;
use crate::player::humanoid::WaterState;
use crate::state::{AppState, LiveAvatarRecord, LocalPlayer, RemotePeer, TravelingTo};

use super::super::control::events::{EventKind, MoveOutcome};
use super::observe::EventSink;
use super::{hundredths, hundredths3};

/// The keys the controller drives with. It releases all of them whenever it
/// lets go, so a finished movement never leaves a key held down.
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

    /// How long a turn may take: a step on foot, a swing round on wheels.
    fn turn_within(self) -> f64 {
        match self {
            Self::OnFoot => FACE_STEP_SECS,
            Self::Wheeled => STUCK_AFTER_SECS,
        }
    }
}

/// What a movement aims at.
#[derive(Debug, Clone, PartialEq)]
enum Aim {
    /// A point on the ground, (x, z).
    Point(Vec2),
    /// A player, by DID, to stay within `keep` metres of.
    Peer {
        did: String,
        keep: f32,
        /// Setting off after them, rather than standing near them.
        closing: bool,
        /// Where the body has been since `still_since` while closing - what
        /// a blockage is measured by: the follower not getting anywhere, not
        /// the gap, which a player walking away holds steady.
        still_at: Vec2,
        still_since: f64,
        /// Already said to be blocked since it last got anywhere.
        blocked: bool,
        /// How far away they were last seen.
        gap: f32,
    },
    /// A direction on the ground, unit, to turn the body toward.
    Heading {
        dir: Vec2,
        /// How far past `dir` to aim, clockwise in degrees: what the ground
        /// took off the last step.
        bias_deg: f32,
        phase: TurnPhase,
    },
}

/// Where a turn is: stepping (or swinging) since a time, or let go and
/// waiting since a time for the body to come to rest before it is judged.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TurnPhase {
    Moving { since: f64 },
    Settling { since: f64 },
}

struct Goal {
    id: u64,
    aim: Aim,
    run: bool,
    drive: Drive,
    /// The nearest the body has been, and when - what `stuck` is measured
    /// by. A turn only uses the time: when it started.
    best_distance: f32,
    best_at: f64,
}

/// The movement under way, if any.
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
pub(super) fn camera_yaw_facing(toward: Vec2) -> f32 {
    (-toward.x).atan2(-toward.y)
}

/// A driven body: swing round on the spot to a point well off the nose,
/// then throttle on and steer toward the side the point is on.
fn wheeled(position: Vec3, forward: Vec3, target: Vec2, run: bool) -> Controls {
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
fn swing(forward: Vec3, toward: Vec2) -> Controls {
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

/// How far `toward` is from where `forward` points, in degrees, clockwise -
/// the frame `status` and `look --heading` use.
fn heading_off(forward: Vec3, toward: Vec2) -> f32 {
    let ahead = Vec2::new(forward.x, forward.z).normalize_or(Vec2::NEG_Y);
    let right = Vec2::new(-ahead.y, ahead.x);
    toward.dot(right).atan2(toward.dot(ahead)).to_degrees()
}

/// Whether a follower moves this frame, given the gap to its player: it
/// sets off when they are further than `keep` by [`FOLLOW_SLACK_M`], and
/// stands once it is back within `keep`.
fn follow_moves(gap: f32, keep: f32, closing: &mut bool) -> bool {
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
fn follow_blocked(here: Vec2, still_at: &mut Vec2, still_since: &mut f64, now: f64) -> bool {
    if here.distance(*still_at) >= PROGRESS_STEP_M {
        *still_at = here;
        *still_since = now;
        return false;
    }
    now - *still_since >= STUCK_AFTER_SECS
}

/// `angle` moved by whole turns to within half a turn of `from`, so a camera
/// easing toward it goes the short way round.
fn nearest_turn(from: f32, angle: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    from + (angle - from + PI).rem_euclid(TAU) - PI
}

/// Start walking to `target` (x, z), replacing any movement under way.
pub(super) fn walk_to(world: &mut World, target: Vec2, run: bool) -> Result<Value, String> {
    if !target.is_finite() {
        return Err("the point must be two finite numbers".to_owned());
    }
    let (started, position) = begin(world, Aim::Point(target), run)?;
    let distance = position.xz().distance(target);
    Ok(json!({
        "goal_id": started.id,
        "to": [hundredths(target.x), hundredths(target.y)],
        "distance_m": hundredths(distance),
        "events_seq": started.events_seq,
    }))
}

/// Start following the player `did`, `keep` metres behind, replacing any
/// movement under way.
pub(super) fn follow(
    world: &mut World,
    did: String,
    keep: f32,
    run: bool,
) -> Result<Value, String> {
    if !keep.is_finite() || keep < 0.0 {
        return Err("the distance must be a number of metres, 0 or more".to_owned());
    }
    if world
        .get_resource::<AtprotoSession>()
        .is_some_and(|session| session.did == did)
    {
        return Err("the agent cannot follow itself".to_owned());
    }
    let seen = world
        .query::<(&RemotePeer, &GlobalTransform, Option<&PeerResolve>)>()
        .iter(world)
        .find(|(peer, ..)| peer.did.as_deref() == Some(did.as_str()))
        .map(|(_, at, resolve)| (at.translation(), resolve.is_some_and(|r| r.placed)));
    let Some((at, placed)) = seen else {
        return Err(format!("{did} is not in this world"));
    };
    let now = world.resource::<Time<Real>>().elapsed_secs_f64();
    let here = local_body(world).map_or(Vec2::ZERO, |body| body.translation().xz());
    let aim = Aim::Peer {
        did: did.clone(),
        keep,
        closing: true,
        still_at: here,
        still_since: now,
        blocked: false,
        gap: 0.0,
    };
    let (started, position) = begin(world, aim, run)?;
    Ok(json!({
        "goal_id": started.id,
        "following": did,
        "keep_m": hundredths(keep),
        "distance_m": placed.then(|| hundredths(position.xz().distance(at.xz()))),
        "placed": placed,
        "events_seq": started.events_seq,
    }))
}

/// What `face` turns toward.
pub(super) enum FaceTarget {
    Point(Vec2),
    Peer(String),
}

/// Turn to face a point or a player, replacing any movement under way - or
/// answer at once if the body already faces it.
pub(super) fn face(world: &mut World, target: FaceTarget) -> Result<Value, String> {
    let body = local_body(world).ok_or("the agent has no body yet")?;
    let point = match target {
        FaceTarget::Point(point) if point.is_finite() => point,
        FaceTarget::Point(_) => return Err("the point must be two finite numbers".to_owned()),
        FaceTarget::Peer(did) => world
            .query::<(&RemotePeer, &GlobalTransform, Option<&PeerResolve>)>()
            .iter(world)
            .find(|(peer, ..)| peer.did.as_deref() == Some(did.as_str()))
            .filter(|(.., resolve)| resolve.is_some_and(|r| r.placed))
            .map(|(_, at, _)| at.translation().xz())
            .ok_or(format!("{did} is not in this world, or not placed yet"))?,
    };
    let toward = point - body.translation().xz();
    if toward.length() < 0.1 {
        return Err("that is where the agent stands".to_owned());
    }
    let toward = toward.normalize();
    let off = heading_off(body.forward().as_vec3(), toward);
    if off.abs() <= FACE_TOLERANCE_DEG {
        // Nothing to turn - but a face ends whatever was under way all the
        // same, as a turn would, so asking for one never leaves a follow
        // running in one case and stopped in the other.
        let replaced = world.get_resource_or_init::<Movement>().goal.take();
        if let Some(old) = replaced {
            release_keys(&mut world.resource_mut::<ButtonInput<KeyCode>>());
            end(world, &old, MoveOutcome::Replaced, body.translation());
        }
        return Ok(json!({
            "goal_id": null,
            "already_facing": true,
            "turn_deg": hundredths(off),
        }));
    }
    let now = world.resource::<Time<Real>>().elapsed_secs_f64();
    let aim = Aim::Heading {
        dir: toward,
        bias_deg: 0.0,
        phase: TurnPhase::Moving { since: now },
    };
    let (started, _) = begin(world, aim, false)?;
    Ok(json!({
        "goal_id": started.id,
        "turn_deg": hundredths(off),
        "toward": [hundredths(toward.x), hundredths(toward.y)],
        "events_seq": started.events_seq,
    }))
}

/// A movement just begun: its id, and the event cursor a caller that wants
/// to wait for its end reads from - nothing before it can be this one's.
struct Started {
    id: u64,
    events_seq: u64,
}

/// Replace whatever movement is under way with one aiming at `aim`.
fn begin(world: &mut World, aim: Aim, run: bool) -> Result<(Started, Vec3), String> {
    if *world.resource::<State<AppState>>().get() != AppState::InGame {
        return Err("the agent is not in a world yet".to_owned());
    }
    if world.contains_resource::<TravelingTo>() {
        return Err("the agent is travelling".to_owned());
    }
    let drive = Drive::of(
        &world
            .get_resource::<LiveAvatarRecord>()
            .ok_or("the agent has no body yet")?
            .0
            .locomotion,
    )?;
    let position = local_body(world)
        .ok_or("the agent has no body yet")?
        .translation();
    let now = world.resource::<Time<Real>>().elapsed_secs_f64();
    let best_distance = match &aim {
        Aim::Point(target) => position.xz().distance(*target),
        Aim::Peer { .. } | Aim::Heading { .. } => f32::INFINITY,
    };
    let mut movement = world.get_resource_or_init::<Movement>();
    let replaced = movement.goal.take();
    movement.last_id += 1;
    let id = movement.last_id;
    movement.goal = Some(Goal {
        id,
        aim,
        run,
        drive,
        best_distance,
        best_at: now,
    });
    if let Some(old) = replaced {
        end(world, &old, MoveOutcome::Replaced, position);
    }
    let events_seq = world.resource::<EventSink>().0.last_seq();
    Ok((Started { id, events_seq }, position))
}

/// Stop moving, if moving.
pub(super) fn halt(world: &mut World) -> Result<Value, String> {
    let goal = world.get_resource_or_init::<Movement>().goal.take();
    let Some(goal) = goal else {
        return Ok(json!({ "halted": null }));
    };
    let position = local_body(world).map_or(Vec3::ZERO, |body| body.translation());
    release_keys(&mut world.resource_mut::<ButtonInput<KeyCode>>());
    end(world, &goal, MoveOutcome::Halted, position);
    Ok(json!({ "halted": goal.id }))
}

/// What the agent is doing on its feet (or wheels), for `status`: `None`
/// when standing still of its own accord.
pub(super) fn describe(world: &mut World) -> Value {
    let Some(movement) = world.get_resource::<Movement>() else {
        return Value::Null;
    };
    let Some(goal) = movement.goal.as_ref() else {
        return Value::Null;
    };
    match &goal.aim {
        Aim::Point(target) => json!({
            "goal_id": goal.id,
            "doing": "walking",
            "to": [hundredths(target.x), hundredths(target.y)],
        }),
        Aim::Peer {
            did,
            keep,
            closing,
            gap,
            ..
        } => json!({
            "goal_id": goal.id,
            "doing": "following",
            "peer": did,
            "keep_m": hundredths(*keep),
            "distance_m": hundredths(*gap),
            "closing": closing,
        }),
        Aim::Heading { dir: toward, .. } => json!({
            "goal_id": goal.id,
            "doing": "turning",
            "toward": [hundredths(toward.x), hundredths(toward.y)],
        }),
    }
}

fn local_body(world: &mut World) -> Option<GlobalTransform> {
    world
        .query_filtered::<&GlobalTransform, With<LocalPlayer>>()
        .iter(world)
        .next()
        .copied()
}

/// How far `goal` still had to go from `position`: to its point, to its
/// player as last seen, or nothing for a turn.
fn distance_left(goal: &Goal, position: Vec3) -> f32 {
    match &goal.aim {
        Aim::Point(target) => position.xz().distance(*target),
        Aim::Peer { gap, .. } => *gap,
        Aim::Heading { .. } => 0.0,
    }
}

/// Record how `goal` ended.
fn end(world: &mut World, goal: &Goal, outcome: MoveOutcome, position: Vec3) {
    let forward = local_body(world).map_or(Vec3::NEG_Z, |body| body.forward().as_vec3());
    world
        .resource::<EventSink>()
        .0
        .push(ended(goal, outcome, position, forward));
}

/// The event that says how `goal` ended, with the body at `position`
/// facing `forward`.
fn ended(goal: &Goal, outcome: MoveOutcome, position: Vec3, forward: Vec3) -> EventKind {
    EventKind::MovementEnded {
        goal_id: goal.id,
        outcome,
        position: hundredths3(position),
        distance_left_m: hundredths(distance_left(goal, position)),
        facing_off_deg: match &goal.aim {
            Aim::Heading { dir, .. } => Some(hundredths(heading_off(forward, *dir))),
            Aim::Point(_) | Aim::Peer { .. } => None,
        },
    }
}

fn release_keys(keys: &mut ButtonInput<KeyCode>) {
    for key in DRIVE_KEYS {
        keys.release(key);
    }
}

/// What a movement does this frame.
enum Step {
    /// Hold these controls.
    Drive(Controls),
    /// Stand still: every key released.
    Stand,
    /// The movement is over.
    End(MoveOutcome),
}

/// `PreUpdate`, after input: hold the keys (and turn the camera) that carry
/// the body toward the goal, or end it.
#[allow(clippy::too_many_arguments)]
pub(super) fn steer(
    mut movement: ResMut<Movement>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    player: Query<&GlobalTransform, With<LocalPlayer>>,
    peers: Query<(&RemotePeer, &GlobalTransform, Option<&PeerResolve>)>,
    mut camera: Query<(&mut PanOrbitCamera, &GlobalTransform), With<WorldCamera>>,
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
    let forward = body.map_or(Vec3::NEG_Z, |body| body.forward().as_vec3());
    let now = time.elapsed_secs_f64();
    let step = if *state.get() != AppState::InGame || traveling.is_some() || body.is_none() {
        Step::End(MoveOutcome::Interrupted)
    } else {
        let Goal {
            id,
            aim,
            run,
            drive,
            best_distance,
            best_at,
        } = &mut *goal;
        match aim {
            Aim::Point(target) => {
                let distance = position.xz().distance(*target);
                progress(best_distance, best_at, distance, now);
                if distance <= drive.arrive_within() {
                    Step::End(MoveOutcome::Arrived)
                } else if now - *best_at >= STUCK_AFTER_SECS {
                    Step::End(MoveOutcome::Stuck)
                } else {
                    Step::Drive(toward(*drive, position, forward, *target, *run))
                }
            }
            Aim::Peer {
                did,
                keep,
                closing,
                still_at,
                still_since,
                blocked,
                gap,
            } => {
                let seen = peers
                    .iter()
                    .find(|(peer, ..)| peer.did.as_deref() == Some(did.as_str()))
                    .map(|(_, at, resolve)| (at.translation(), resolve.is_some_and(|r| r.placed)));
                match seen {
                    None => Step::End(MoveOutcome::PeerLeft),
                    // Still at its spawn stand-in: nowhere to walk to yet.
                    Some((_, false)) => Step::Stand,
                    Some((at, true)) => {
                        *gap = position.xz().distance(at.xz());
                        let was_closing = *closing;
                        if !follow_moves(*gap, *keep, closing) {
                            Step::Stand
                        } else {
                            if !was_closing {
                                // Setting off again: a blockage is measured
                                // from here.
                                *still_at = position.xz();
                                *still_since = now;
                                *blocked = false;
                            }
                            if follow_blocked(position.xz(), still_at, still_since, now)
                                && !*blocked
                            {
                                *blocked = true;
                                sink.0.push(EventKind::FollowBlocked {
                                    goal_id: *id,
                                    position: hundredths3(position),
                                    distance_m: hundredths(*gap),
                                });
                            } else if *still_since == now {
                                // Got somewhere: a later blockage is news again.
                                *blocked = false;
                            }
                            let run = *run || *gap > *keep + FOLLOW_RUN_BEYOND_M;
                            Step::Drive(toward(*drive, position, forward, at.xz(), run))
                        }
                    }
                }
            }
            Aim::Heading {
                dir,
                bias_deg,
                phase,
            } => {
                let off = heading_off(forward, *dir);
                let aligned = off.abs() <= FACE_TOLERANCE_DEG;
                let out_of_time = now - *best_at >= drive.turn_within();
                match *phase {
                    TurnPhase::Settling { since } if now - since < FACE_SETTLE_SECS => Step::Stand,
                    TurnPhase::Settling { .. } if aligned => Step::End(MoveOutcome::Faced),
                    _ if out_of_time => Step::End(MoveOutcome::Stuck),
                    TurnPhase::Settling { .. } => {
                        // At rest short of the way asked, or past it: go
                        // again, aiming past it by what the ground took off.
                        if *drive == Drive::OnFoot {
                            *bias_deg += off;
                        }
                        *phase = TurnPhase::Moving { since: now };
                        Step::Drive(turn_controls(*drive, position, forward, *dir, *bias_deg))
                    }
                    TurnPhase::Moving { since } => {
                        let stepped = *drive == Drive::OnFoot && now - since >= FACE_PULSE_SECS;
                        if aligned || stepped {
                            *phase = TurnPhase::Settling { since: now };
                            Step::Stand
                        } else {
                            Step::Drive(turn_controls(*drive, position, forward, *dir, *bias_deg))
                        }
                    }
                }
            }
        }
    };
    let controls = match step {
        Step::End(outcome) => {
            release_keys(&mut keys);
            sink.0.push(ended(goal, outcome, position, forward));
            movement.goal = None;
            return;
        }
        Step::Stand => {
            release_keys(&mut keys);
            return;
        }
        Step::Drive(controls) => controls,
    };
    let mut controls = controls;
    // A walker walks where the camera looked when this frame began: the
    // walk reads the camera's propagated transform, and a turn of it lands
    // only at the end of the frame. So the step waits a frame for the
    // camera to come round, rather than going the old way.
    if let (Some(yaw), Ok((_, camera_at))) = (controls.camera_yaw, camera.single()) {
        let looking = camera_at.forward().as_vec3().xz().normalize_or_zero();
        let wanted = Vec2::new(-yaw.sin(), -yaw.cos());
        if looking.dot(wanted) < CAMERA_CAUGHT_UP_DEG.to_radians().cos() {
            controls.keys.retain(|key| *key != KeyCode::KeyW);
        }
    }
    for key in DRIVE_KEYS {
        if controls.keys.contains(&key) {
            keys.press(key);
        } else {
            keys.release(key);
        }
    }
    if let (Some(yaw), Ok((mut orbit, _))) = (controls.camera_yaw, camera.single_mut()) {
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

/// The controls that turn `drive` toward `dir`: a step aimed `bias_deg`
/// past it for a walker, a swing on the spot for a driven body.
fn turn_controls(
    drive: Drive,
    position: Vec3,
    forward: Vec3,
    dir: Vec2,
    bias_deg: f32,
) -> Controls {
    match drive {
        Drive::OnFoot => on_foot(position, position.xz() + turned(dir, bias_deg), false),
        Drive::Wheeled => swing(forward, dir),
    }
}

/// `dir` turned `degrees` clockwise - toward its own right.
fn turned(dir: Vec2, degrees: f32) -> Vec2 {
    let right = Vec2::new(-dir.y, dir.x);
    let (sin, cos) = degrees.to_radians().sin_cos();
    dir * cos + right * sin
}

/// Note `distance` if it is the nearest yet by a real step, and when:
/// whether it was.
fn progress(best_distance: &mut f32, best_at: &mut f64, distance: f32, now: f64) -> bool {
    let closer = distance + PROGRESS_STEP_M <= *best_distance;
    if closer {
        *best_distance = distance;
        *best_at = now;
    }
    closer
}

/// The controls that carry `drive` from `position` toward `target`.
fn toward(drive: Drive, position: Vec3, forward: Vec3, target: Vec2, run: bool) -> Controls {
    match drive {
        Drive::OnFoot => on_foot(position, target, run),
        Drive::Wheeled => wheeled(position, forward, target, run),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;
    use crate::agent::control::events::EventLog;

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

    /// A face with nothing to turn still ends the movement under way - the
    /// follow it interrupts stops either way - and lets go of the keys.
    #[test]
    fn a_face_already_facing_still_ends_the_movement_under_way() {
        let log = Arc::new(EventLog::new(16, "test".into()));
        let mut world = World::new();
        world.insert_resource(EventSink(Arc::clone(&log)));
        world.insert_resource(State::new(AppState::InGame));
        world.init_resource::<ButtonInput<KeyCode>>();
        world.init_resource::<Time<Real>>();
        world.spawn((
            LocalPlayer,
            GlobalTransform::from(Transform::IDENTITY.looking_to(Vec3::Z, Vec3::Y)),
        ));
        world.insert_resource(Movement {
            goal: Some(Goal {
                id: 4,
                aim: Aim::Point(Vec2::new(0.0, 50.0)),
                run: false,
                drive: Drive::OnFoot,
                best_distance: 50.0,
                best_at: 0.0,
            }),
            last_id: 4,
        });
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);

        let answer = face(&mut world, FaceTarget::Point(Vec2::new(0.0, 10.0))).expect("faced");

        assert_eq!(answer["already_facing"], true);
        assert!(world.resource::<Movement>().goal.is_none());
        assert!(
            !world
                .resource::<ButtonInput<KeyCode>>()
                .pressed(KeyCode::KeyW)
        );
        let ended: Vec<MoveOutcome> = log
            .after(0, Duration::ZERO)
            .events
            .into_iter()
            .filter_map(|e| match e.what {
                EventKind::MovementEnded { outcome, .. } => Some(outcome),
                _ => None,
            })
            .collect();
        assert_eq!(ended, [MoveOutcome::Replaced]);
    }

    #[test]
    fn the_camera_turns_the_short_way_round() {
        use std::f32::consts::PI;
        let turned = nearest_turn(0.1, 2.0 * PI - 0.1);
        assert!((turned - (-0.1)).abs() < 1e-5, "{turned}");
        let far = nearest_turn(10.0 * PI, 0.5);
        assert!((far - 10.0 * PI - 0.5).abs() < 1e-4, "{far}");
    }

    fn peer(did: &str) -> RemotePeer {
        RemotePeer {
            peer_id: serde_json::from_str("\"00000000-0000-0000-0000-000000000003\"")
                .expect("a uuid"),
            did: Some(did.to_owned()),
            handle: None,
            muted: false,
            avatar: None,
            build: None,
            connected_at: 0.0,
        }
    }

    fn placed(placed: bool) -> PeerResolve {
        PeerResolve {
            placed,
            ..default()
        }
    }

    fn follow_app() -> (App, Arc<EventLog>, Entity) {
        let log = Arc::new(EventLog::new(16, "test".into()));
        let mut app = App::new();
        app.insert_resource(EventSink(Arc::clone(&log)))
            .insert_resource(State::new(AppState::InGame))
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Time<Real>>()
            .insert_resource(Movement {
                goal: Some(Goal {
                    id: 7,
                    aim: Aim::Peer {
                        did: "did:plc:friend".into(),
                        keep: 3.0,
                        closing: true,
                        still_at: Vec2::ZERO,
                        still_since: 0.0,
                        blocked: false,
                        gap: 0.0,
                    },
                    run: false,
                    drive: Drive::OnFoot,
                    best_distance: f32::INFINITY,
                    best_at: 0.0,
                }),
                last_id: 7,
            })
            .add_systems(Update, steer);
        app.world_mut().spawn((
            LocalPlayer,
            GlobalTransform::from(Transform::from_xyz(0.0, 0.0, 0.0)),
        ));
        let friend = app
            .world_mut()
            .spawn((
                peer("did:plc:friend"),
                placed(false),
                GlobalTransform::from(Transform::from_xyz(0.0, 10.0, 0.0)),
            ))
            .id();
        (app, log, friend)
    }

    fn walking(app: &App) -> bool {
        app.world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::KeyW)
    }

    /// THE SEQUENCE: the player is still at their spawn stand-in, then is
    /// placed 10 m away, then comes within the distance, then leaves. The
    /// follower waits, walks, stands, and ends `peer_left` - never walking
    /// to the stand-in.
    #[test]
    fn a_follow_waits_for_a_body_walks_to_it_stands_and_ends_when_it_leaves() {
        let (mut app, log, friend) = follow_app();

        app.update();
        assert!(!walking(&app), "a stand-in is not somewhere to walk to");

        app.world_mut().entity_mut(friend).insert((
            placed(true),
            GlobalTransform::from(Transform::from_xyz(0.0, 0.0, 10.0)),
        ));
        app.update();
        assert!(walking(&app), "placed and far: walk");

        app.world_mut()
            .entity_mut(friend)
            .insert(GlobalTransform::from(Transform::from_xyz(0.0, 0.0, 2.0)));
        app.update();
        assert!(!walking(&app), "within the distance: stand");

        app.world_mut().entity_mut(friend).despawn();
        app.update();
        let ended: Vec<MoveOutcome> = log
            .after(0, Duration::ZERO)
            .events
            .into_iter()
            .filter_map(|e| match e.what {
                EventKind::MovementEnded {
                    goal_id: 7,
                    outcome,
                    ..
                } => Some(outcome),
                _ => None,
            })
            .collect();
        assert_eq!(ended, [MoveOutcome::PeerLeft]);
        assert!(app.world().resource::<Movement>().goal.is_none());
    }
}
