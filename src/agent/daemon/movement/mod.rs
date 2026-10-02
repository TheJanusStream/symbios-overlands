//! `agent walk-to`, `follow` and `face` (#1418, #1421, #1430): the agent
//! moves itself the way a person does - toward a point, after another
//! player, or round to face something - until it gets there, gets stuck, or
//! is told to stop. And `agent drive` (#1527): a body on wheels holds keys
//! segment by segment, as a driver does at a ramp, and the ending says what
//! the run did ([`keys`]).
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
//! the agent's to choose. A body that flies - a rotorcraft, which is what an
//! airship is - climbs over what is in the way instead, and lands on its
//! point ([`flight`]); an airplane comes down a straight final onto it
//! ([`wing`]).
//!
//! A follow is a walk whose point moves: every frame it aims at where the
//! player it follows is drawn, stands once it is within its distance, and
//! sets off again when they get further than that by a margin - running to
//! catch up when they are far ahead. It ends when it is halted or replaced,
//! when the player leaves, or on travel; blocked, it says so once and keeps
//! trying. A player whose body has never been placed - a sleeping browser
//! tab, still at its spawn stand-in - is waited for, not walked to. A
//! rotorcraft escorts them low, and lands beside them once they stand still;
//! an airplane, which cannot hover, does not follow at all.
//!
//! A turn (`face`) is how a person turns. There is no turning on the spot on
//! foot: a walker steps toward the direction until it faces it. A car or a
//! hover-boat swings round under its steering alone, throttle off; a
//! rotorcraft swings round wherever it is, in the air or on the ground, and
//! an airplane on the ground only.
//!
//! A body that flies never ends a movement in the air (owner, 2026-09-23 and
//! 2026-09-24): halted, stuck, or left by the player it followed, it first
//! comes down - a rotorcraft straight down where it is, an airplane where it
//! can, straight ahead - and the movement ends once it is down. Only another
//! movement, or travel, cuts it off at once. And an airplane's engine runs
//! with no key held, so with no movement under way it holds its engine cut
//! ([`park`]) or it would take off on its own.
//!
//! This file holds the movement under way and the systems that steer it;
//! [`ground`] how a walker and a driven body are steered, [`flight`] how a
//! rotorcraft is flown, [`wing`] how an airplane is, and [`sense`] what they
//! read of the body.

mod flight;
mod ground;
mod keys;
mod sense;
mod wing;

use bevy::prelude::*;
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde_json::{Value, json};

use crate::camera::WorldCamera;
use crate::config::agent::{
    CAMERA_CAUGHT_UP_DEG, FACE_PULSE_SECS, FACE_SETTLE_SECS, FACE_TOLERANCE_DEG,
    FOLLOW_RUN_BEYOND_M, PROGRESS_STEP_M, STUCK_AFTER_SECS, WHEELED_STOPPED_MS,
};
use crate::network::PeerResolve;
use crate::pds::LocomotionConfig;
use crate::player::LocalMovement;
use crate::player::humanoid::WaterState;
use crate::state::{AppState, LiveAvatarRecord, LocalPlayer, RemotePeer, TravelingTo};

use super::super::control::events::{EventKind, MoveOutcome};
use super::super::control::protocol::DriveSegment;
use super::observe::EventSink;
use super::{hundredths, hundredths3};

pub(super) use ground::camera_yaw_facing;
pub(super) use sense::height;

use flight::{Craft, Flight, Flown, Rotor, Turned};
use ground::{Ground, approach_speed, brake, follow_blocked, follow_moves, nearest_turn};
use wing::{Sortie, Wing};

/// The keys the controller drives with. It releases all of them whenever it
/// lets go, so a finished movement never leaves a key held down.
const DRIVE_KEYS: [KeyCode; 8] = [
    KeyCode::KeyW,
    KeyCode::KeyS,
    KeyCode::KeyA,
    KeyCode::KeyD,
    KeyCode::KeyQ,
    KeyCode::KeyE,
    KeyCode::ShiftLeft,
    KeyCode::Space,
];

/// How a body moves under the keys.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drive {
    /// On the ground, walked or driven.
    Ground(Ground),
    /// In the air: climbs and comes down, pushes along its nose and beside
    /// it, and turns on the spot (the helicopter preset, every airship).
    Rotor(Rotor),
    /// In the air on wings: lifts by its speed, turns by its rudder (the
    /// airplane preset).
    Wing(Wing),
}

impl Drive {
    fn of(locomotion: &LocomotionConfig) -> Result<Self, String> {
        match locomotion {
            LocomotionConfig::Humanoid(_) => Ok(Self::Ground(Ground::OnFoot)),
            LocomotionConfig::Car(_) | LocomotionConfig::HoverBoat(_) => {
                Ok(Self::Ground(Ground::Wheeled))
            }
            LocomotionConfig::Helicopter(params) => Ok(Self::Rotor(Rotor::of(params))),
            LocomotionConfig::Airplane(params) => Ok(Self::Wing(Wing::of(params))),
            LocomotionConfig::Unknown => {
                Err("the agent's body moves in a way this build does not know".to_owned())
            }
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
        /// a blockage is measured by on the ground: the follower not getting
        /// anywhere, not the gap, which a player walking away holds steady.
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
    /// A body that flies coming straight down, to end as `then` once it is
    /// down, having had `left` metres still to go when it stopped.
    Down { then: MoveOutcome, left: f32 },
    /// Keys held segment by segment, the run measured as it goes (#1527).
    Keys(Box<keys::KeyRun>),
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
    /// by on the ground. A turn only uses the time: when it started.
    best_distance: f32,
    best_at: f64,
    /// A rotorcraft's flight, from the frame it first flies.
    flight: Option<Flight>,
    /// An airplane's, from the frame it first flies.
    sortie: Option<Sortie>,
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

impl Controls {
    /// Keys alone, the camera left where it is.
    fn keys(keys: Vec<KeyCode>) -> Self {
        Self {
            keys,
            camera_yaw: None,
        }
    }
}

/// How far `toward` is from where `forward` points, in degrees, clockwise -
/// the frame `status` and `look --heading` use.
fn heading_off(forward: Vec3, toward: Vec2) -> f32 {
    let ahead = Vec2::new(forward.x, forward.z).normalize_or(Vec2::NEG_Y);
    let right = Vec2::new(-ahead.y, ahead.x);
    toward.dot(right).atan2(toward.dot(ahead)).to_degrees()
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

/// Hold keys segment by segment (#1527), replacing any movement under way:
/// a body on wheels driven at a ramp, a bend, a gap. Its ending carries what
/// the run did.
pub(super) fn drive(world: &mut World, asked: Vec<DriveSegment>) -> Result<Value, String> {
    let airborne_above = keys::airborne_above(
        &world
            .get_resource::<LiveAvatarRecord>()
            .ok_or("the agent has no body yet")?
            .0
            .locomotion,
    )
    .ok_or(
        "`drive` is for a body on wheels - a car or a hover-boat; a walker walks, and a \
         body that flies never ends a movement in the air",
    )?;
    let segments = keys::segments(asked)?;
    let count = segments.len();
    let now = world.resource::<Time<Real>>().elapsed_secs_f64();
    let run = keys::KeyRun::new(segments, now, airborne_above);
    let total = run.total();
    let (started, _) = begin(world, Aim::Keys(Box::new(run)), false)?;
    Ok(json!({
        "goal_id": started.id,
        "segments": count,
        "total_s": hundredths(total as f32),
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
    if let Drive::Wing(_) = drive {
        match &aim {
            Aim::Peer { .. } => {
                return Err(
                    "an airplane cannot follow: it cannot hover, nor keep to a walker's \
                            pace (#1431)"
                        .to_owned(),
                );
            }
            Aim::Heading { .. } if sense::craft(world).is_some_and(|c| wing::airborne(&c)) => {
                return Err(
                    "an airplane turns on the spot on the ground only; walk-to a point \
                            to land it first (#1431)"
                        .to_owned(),
                );
            }
            _ => {}
        }
    }
    let position = local_body(world)
        .ok_or("the agent has no body yet")?
        .translation();
    let now = world.resource::<Time<Real>>().elapsed_secs_f64();
    let best_distance = match &aim {
        Aim::Point(target) => position.xz().distance(*target),
        Aim::Peer { .. } | Aim::Heading { .. } | Aim::Down { .. } | Aim::Keys(_) => f32::INFINITY,
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
        flight: None,
        sortie: None,
    });
    if let Some(old) = replaced {
        end(world, &old, MoveOutcome::Replaced, position);
    }
    let events_seq = world.resource::<EventSink>().0.last_seq();
    Ok((Started { id, events_seq }, position))
}

/// Stop moving, if moving. A body that flies and is up in the air comes
/// straight down first, and the movement ends once it is down.
pub(super) fn halt(world: &mut World) -> Result<Value, String> {
    let goal = world.get_resource_or_init::<Movement>().goal.take();
    let Some(mut goal) = goal else {
        return Ok(json!({ "halted": null }));
    };
    let position = local_body(world).map_or(Vec3::ZERO, |body| body.translation());
    if let Drive::Rotor(_) | Drive::Wing(_) = goal.drive {
        let now = world.resource::<Time<Real>>().elapsed_secs_f64();
        let craft = sense::craft(world);
        let coming_down = matches!(goal.aim, Aim::Down { .. });
        if coming_down || craft.is_some_and(|craft| !grounded(goal.drive, &craft)) {
            if let (Some(craft), false) = (craft, coming_down) {
                come_down_then(&mut goal, MoveOutcome::Halted, &craft, now);
            }
            let id = goal.id;
            world.resource_mut::<Movement>().goal = Some(goal);
            return Ok(json!({ "halted": id, "landing": true }));
        }
    }
    release_keys(&mut world.resource_mut::<ButtonInput<KeyCode>>());
    end(world, &goal, MoveOutcome::Halted, position);
    Ok(json!({ "halted": goal.id }))
}

/// Turn `goal` - a flight that has to end `outcome` up in the air - into
/// one coming down, to end so once it is down: a rotorcraft straight down
/// where it is, an airplane where it can.
fn come_down_then(goal: &mut Goal, outcome: MoveOutcome, craft: &Craft, now: f64) {
    let left = distance_left(goal, craft.position);
    goal.aim = Aim::Down {
        then: outcome,
        left,
    };
    goal.flight = Some(Flight::landing(craft, now));
    goal.sortie = Some(Sortie::landing_ahead(craft, now));
}

/// Whether a body is down, as far as ending a movement goes: a rotorcraft
/// down on what is below it, an airplane on the ground.
fn grounded(drive: Drive, craft: &Craft) -> bool {
    match drive {
        Drive::Wing(_) => !wing::airborne(craft),
        Drive::Rotor(_) | Drive::Ground(_) => craft.down(),
    }
}

/// What the agent is doing on its feet (or wheels, or rotors), for
/// `status`: `None` when standing still of its own accord.
pub(super) fn describe(world: &mut World) -> Value {
    // Only a drive reads the clock, and a world with no movement - a test's,
    // or the daemon's before its first frame - may have none yet.
    let now = world
        .get_resource::<Time<Real>>()
        .map_or(0.0, Time::<Real>::elapsed_secs_f64);
    let Some(movement) = world.get_resource::<Movement>() else {
        return Value::Null;
    };
    let Some(goal) = movement.goal.as_ref() else {
        return Value::Null;
    };
    let phase = match goal.drive {
        Drive::Wing(_) => goal
            .sortie
            .as_ref()
            .map_or("taking_off", |sortie| sortie.leg.word()),
        Drive::Rotor(_) | Drive::Ground(_) => goal
            .flight
            .as_ref()
            .map_or(flight::Phase::TakingOff, |flight| flight.phase)
            .word(),
    };
    let flying = matches!(goal.drive, Drive::Rotor(_) | Drive::Wing(_));
    let mut described = match &goal.aim {
        Aim::Point(target) => json!({
            "goal_id": goal.id,
            "doing": if flying { "flying" } else { "walking" },
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
        Aim::Down { then, .. } => json!({
            "goal_id": goal.id,
            "doing": "landing",
            "then": then,
        }),
        Aim::Keys(run) => {
            let (segment, left) = run.describe(now);
            json!({
                "goal_id": goal.id,
                "doing": "driving",
                "segment": segment.map(|index| index + 1),
                "left_s": hundredths(left as f32),
            })
        }
    };
    if flying && matches!(goal.aim, Aim::Point(_) | Aim::Peer { .. }) {
        described["phase"] = json!(phase);
    }
    described
}

fn local_body(world: &mut World) -> Option<GlobalTransform> {
    world
        .query_filtered::<&GlobalTransform, With<LocalPlayer>>()
        .iter(world)
        .next()
        .copied()
}

/// How far `goal` still had to go from `position`: to its point, to its
/// player as last seen, nothing for a turn - or, coming down after it
/// stopped, what it had left when it did.
fn distance_left(goal: &Goal, position: Vec3) -> f32 {
    match &goal.aim {
        Aim::Point(target) => position.xz().distance(*target),
        Aim::Peer { gap, .. } => *gap,
        Aim::Heading { .. } | Aim::Keys(_) => 0.0,
        Aim::Down { left, .. } => *left,
    }
}

/// Record how `goal` ended.
fn end(world: &mut World, goal: &Goal, outcome: MoveOutcome, position: Vec3) {
    let forward = local_body(world).map_or(Vec3::NEG_Z, |body| body.forward().as_vec3());
    let height = match goal.drive {
        Drive::Rotor(_) | Drive::Wing(_) => sense::height(world),
        Drive::Ground(_) => None,
    };
    let now = world
        .get_resource::<Time<Real>>()
        .map_or(0.0, Time::<Real>::elapsed_secs_f64);
    world
        .resource::<EventSink>()
        .0
        .push(ended(goal, outcome, position, forward, height, now));
}

/// The event that says how `goal` ended at `now`, with the body at
/// `position` facing `forward` - and, for a body that flies, `height` above
/// what is below it; for a drive, what the run did.
fn ended(
    goal: &Goal,
    outcome: MoveOutcome,
    position: Vec3,
    forward: Vec3,
    height: Option<f32>,
    now: f64,
) -> EventKind {
    EventKind::MovementEnded {
        goal_id: goal.id,
        outcome,
        position: hundredths3(position),
        distance_left_m: hundredths(distance_left(goal, position)),
        facing_off_deg: match &goal.aim {
            Aim::Heading { dir, .. } => Some(hundredths(heading_off(forward, *dir))),
            Aim::Point(_) | Aim::Peer { .. } | Aim::Down { .. } | Aim::Keys(_) => None,
        },
        height_m: height.map(hundredths),
        report: match &goal.aim {
            Aim::Keys(run) => Some(run.report(now)),
            _ => None,
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
    /// The movement is over, but a body that flies comes down first.
    EndOnceDown(MoveOutcome),
}

type Peers<'w, 's> = Query<
    'w,
    's,
    (
        &'static RemotePeer,
        &'static GlobalTransform,
        Option<&'static PeerResolve>,
    ),
>;

/// Where the player `did` is drawn, and whether their body has been placed
/// at all - or `None` once they have left.
fn peer_at(peers: &Peers, did: &str) -> Option<(Vec3, bool)> {
    peers
        .iter()
        .find(|(peer, ..)| peer.did.as_deref() == Some(did))
        .map(|(_, at, resolve)| (at.translation(), resolve.is_some_and(|r| r.placed)))
}

/// `PreUpdate`, after input: hold the keys (and turn the camera) that carry
/// the body toward the goal, or end it.
#[allow(clippy::too_many_arguments)]
pub(super) fn steer(
    mut movement: ResMut<Movement>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    player: Query<&GlobalTransform, With<LocalPlayer>>,
    peers: Peers,
    mut camera: Query<(&mut PanOrbitCamera, &GlobalTransform), With<WorldCamera>>,
    local: Option<Res<LocalMovement>>,
    state: Res<State<AppState>>,
    traveling: Option<Res<TravelingTo>>,
    time: Res<Time<Real>>,
    sink: Res<EventSink>,
    // Only a flight reads it, and it wants the physics a walk does not.
    sensing: Option<sense::Sensing>,
    // A car's own count of wheels on the ground, which a drive's report
    // reads to know when it is in the air (#1527, #1524).
    contact: Query<&crate::player::CarContact, With<LocalPlayer>>,
    // How fast the body rolls: a driven walk brakes onto its point (#1536).
    velocity: Query<&avian3d::prelude::LinearVelocity, With<LocalPlayer>>,
) {
    let Some(goal) = movement.goal.as_mut() else {
        return;
    };
    let body = player.single().ok();
    let position = body.map_or(Vec3::ZERO, GlobalTransform::translation);
    let forward = body.map_or(Vec3::NEG_Z, |body| body.forward().as_vec3());
    let velocity = velocity.single().map_or(Vec3::ZERO, |moving| moving.0);
    let now = time.elapsed_secs_f64();
    let craft = sensing.as_ref().and_then(sense::Sensing::craft);
    let step = if *state.get() != AppState::InGame || traveling.is_some() || body.is_none() {
        Step::End(MoveOutcome::Interrupted)
    } else if let Aim::Keys(run) = &mut goal.aim {
        // A drive holds what it was told whatever the body is doing, and
        // notes each frame what that was (#1527).
        run.step(now, craft.as_ref(), contact.single().ok().map(|c| c.wheels))
    } else {
        match (goal.drive, sensing.as_ref(), craft.as_ref()) {
            (Drive::Ground(ground), ..) => {
                let body = Body {
                    position,
                    forward,
                    velocity,
                };
                steer_ground(goal, ground, body, &peers, now, &sink)
            }
            (Drive::Rotor(rotor), Some(sensing), Some(craft)) => {
                steer_flight(goal, &rotor, craft, sensing, &peers, now, &sink)
            }
            (Drive::Wing(wing), Some(sensing), Some(craft)) => {
                steer_wing(goal, &wing, craft, sensing, now)
            }
            // Nothing under the body to measure from yet - the ground is
            // still being built: wait for it.
            (Drive::Rotor(_) | Drive::Wing(_), ..) => Step::Stand,
        }
    };
    let controls = match step {
        Step::EndOnceDown(outcome) if craft.is_some_and(|craft| !grounded(goal.drive, &craft)) => {
            if let Some(craft) = craft {
                come_down_then(goal, outcome, &craft, now);
            }
            release_keys(&mut keys);
            return;
        }
        Step::End(outcome) | Step::EndOnceDown(outcome) => {
            release_keys(&mut keys);
            let height = match goal.drive {
                Drive::Rotor(_) | Drive::Wing(_) => craft.map(|craft| craft.height()),
                Drive::Ground(_) => None,
            };
            sink.0
                .push(ended(goal, outcome, position, forward, height, now));
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

/// Where a body on the ground is, which way it faces, and how it moves.
#[derive(Clone, Copy)]
struct Body {
    position: Vec3,
    forward: Vec3,
    velocity: Vec3,
}

/// One frame of a movement on the ground.
fn steer_ground(
    goal: &mut Goal,
    ground: Ground,
    body: Body,
    peers: &Peers,
    now: f64,
    sink: &EventSink,
) -> Step {
    let Body {
        position,
        forward,
        velocity,
    } = body;
    let Goal {
        id,
        aim,
        run,
        best_distance,
        best_at,
        ..
    } = goal;
    // A driven body that lets go rolls on (#1536): a walk-to used to say it
    // had arrived at its circle's edge at full throttle and roll 7-11 m past,
    // and a follow drove through its player. It comes in no faster than it
    // can stop by where it should, and brakes to a stand there.
    let rolling = velocity.xz().length();
    let wheeled = ground == Ground::Wheeled;
    match aim {
        Aim::Point(target) => {
            let distance = position.xz().distance(*target);
            progress(best_distance, best_at, distance, now);
            // Planned to stop half the arrival radius short of the point; it
            // has arrived once in the circle and at rest.
            if distance <= ground.arrive_within() {
                if wheeled && rolling > WHEELED_STOPPED_MS {
                    Step::Drive(brake(forward, velocity))
                } else {
                    Step::End(MoveOutcome::Arrived)
                }
            } else if now - *best_at >= STUCK_AFTER_SECS {
                Step::End(MoveOutcome::Stuck)
            } else if wheeled && rolling > approach_speed(distance - ground.arrive_within() * 0.5) {
                Step::Drive(brake(forward, velocity))
            } else {
                Step::Drive(ground.toward(position, forward, *target, *run))
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
        } => match peer_at(peers, did) {
            None => Step::End(MoveOutcome::PeerLeft),
            // Still at its spawn stand-in: nowhere to walk to yet.
            Some((_, false)) => Step::Stand,
            Some((at, true)) => {
                *gap = position.xz().distance(at.xz());
                let was_closing = *closing;
                if !follow_moves(*gap, *keep, closing) {
                    if wheeled && rolling > WHEELED_STOPPED_MS {
                        Step::Drive(brake(forward, velocity))
                    } else {
                        Step::Stand
                    }
                } else {
                    if !was_closing {
                        // Setting off again: a blockage is measured from
                        // here.
                        *still_at = position.xz();
                        *still_since = now;
                        *blocked = false;
                    }
                    if follow_blocked(position.xz(), still_at, still_since, now) && !*blocked {
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
                    if wheeled && rolling > approach_speed(*gap - *keep) {
                        Step::Drive(brake(forward, velocity))
                    } else {
                        Step::Drive(ground.toward(position, forward, at.xz(), run))
                    }
                }
            }
        },
        Aim::Heading {
            dir,
            bias_deg,
            phase,
        } => {
            let off = heading_off(forward, *dir);
            let aligned = off.abs() <= FACE_TOLERANCE_DEG;
            let out_of_time = now - *best_at >= ground.turn_within();
            match *phase {
                TurnPhase::Settling { since } if now - since < FACE_SETTLE_SECS => Step::Stand,
                TurnPhase::Settling { .. } if aligned => Step::End(MoveOutcome::Faced),
                _ if out_of_time => Step::End(MoveOutcome::Stuck),
                TurnPhase::Settling { .. } => {
                    // At rest short of the way asked, or past it: go again,
                    // aiming past it by what the ground took off.
                    if ground == Ground::OnFoot {
                        *bias_deg += off;
                    }
                    *phase = TurnPhase::Moving { since: now };
                    Step::Drive(ground.turn(position, forward, *dir, *bias_deg))
                }
                TurnPhase::Moving { since } => {
                    let stepped = ground == Ground::OnFoot && now - since >= FACE_PULSE_SECS;
                    if aligned || stepped {
                        *phase = TurnPhase::Settling { since: now };
                        Step::Stand
                    } else {
                        Step::Drive(ground.turn(position, forward, *dir, *bias_deg))
                    }
                }
            }
        }
        // A body on the ground is down already.
        Aim::Down { then, .. } => Step::End(*then),
        // `steer` holds a drive's keys itself, before it gets here.
        Aim::Keys(_) => Step::End(MoveOutcome::Interrupted),
    }
}

/// One frame of a movement in the air.
fn steer_flight(
    goal: &mut Goal,
    rotor: &Rotor,
    craft: &Craft,
    sensing: &sense::Sensing,
    peers: &Peers,
    now: f64,
    sink: &EventSink,
) -> Step {
    let Goal {
        id, aim, flight, ..
    } = goal;
    let flight = flight.get_or_insert_with(|| Flight::new(craft, now));
    match aim {
        Aim::Point(target) => match flight::fly_to(rotor, flight, craft, sensing, *target, now) {
            Flown::Hold(keys) => Step::Drive(Controls::keys(keys)),
            Flown::Landed => Step::End(MoveOutcome::Arrived),
            Flown::Stuck => Step::EndOnceDown(MoveOutcome::Stuck),
        },
        Aim::Peer {
            did,
            keep,
            closing,
            blocked,
            gap,
            ..
        } => match peer_at(peers, did) {
            None => Step::EndOnceDown(MoveOutcome::PeerLeft),
            // Still at its spawn stand-in: nowhere to fly to yet.
            Some((_, false)) => Step::Drive(Controls::keys(flight::hold(craft))),
            Some((at, true)) => {
                *gap = craft.position.xz().distance(at.xz());
                let escort = flight::escort(rotor, flight, craft, sensing, at.xz(), *keep, now);
                *closing = escort.closing;
                if escort.blocked && !*blocked {
                    *blocked = true;
                    sink.0.push(EventKind::FollowBlocked {
                        goal_id: *id,
                        position: hundredths3(craft.position),
                        distance_m: hundredths(*gap),
                    });
                } else if !escort.blocked {
                    // Got somewhere: a later blockage is news again.
                    *blocked = false;
                }
                Step::Drive(Controls::keys(escort.keys))
            }
        },
        Aim::Heading { dir, phase, .. } => {
            match flight::turn(rotor, flight, craft, *dir, phase, now) {
                Turned::Hold(keys) => Step::Drive(Controls::keys(keys)),
                Turned::Faced => Step::End(MoveOutcome::Faced),
                Turned::Stuck => Step::EndOnceDown(MoveOutcome::Stuck),
            }
        }
        Aim::Down { then, .. } => {
            let spot = match flight.phase {
                flight::Phase::Landing { spot, .. } => spot,
                _ => craft.position.xz(),
            };
            match flight::fly_to(rotor, flight, craft, sensing, spot, now) {
                Flown::Hold(keys) => Step::Drive(Controls::keys(keys)),
                // Down - or unable to get down: it ends as it would have.
                Flown::Landed | Flown::Stuck => Step::End(*then),
            }
        }
        // `drive` refuses a body that flies.
        Aim::Keys(_) => Step::End(MoveOutcome::Interrupted),
    }
}

/// One frame of a movement in an airplane.
fn steer_wing(
    goal: &mut Goal,
    wing: &Wing,
    craft: &Craft,
    sensing: &sense::Sensing,
    now: f64,
) -> Step {
    let Goal { aim, sortie, .. } = goal;
    let sortie = sortie.get_or_insert_with(|| Sortie::new(craft, now));
    match aim {
        Aim::Point(target) => match wing::fly_to(wing, sortie, craft, sensing, *target, now) {
            Flown::Hold(keys) => Step::Drive(Controls::keys(keys)),
            Flown::Landed => Step::End(MoveOutcome::Arrived),
            Flown::Stuck => Step::EndOnceDown(MoveOutcome::Stuck),
        },
        Aim::Heading { dir, phase, .. } => {
            match wing::turn(wing, sortie, craft, *dir, phase, now) {
                Turned::Hold(keys) => Step::Drive(Controls::keys(keys)),
                Turned::Faced => Step::End(MoveOutcome::Faced),
                Turned::Stuck => Step::End(MoveOutcome::Stuck),
            }
        }
        Aim::Down { then, .. } => match wing::land_ahead(wing, sortie, craft, sensing, now) {
            Flown::Hold(keys) => Step::Drive(Controls::keys(keys)),
            // Down - or unable to get down: it ends as it would have.
            Flown::Landed | Flown::Stuck => Step::End(*then),
        },
        // `begin` refuses a follow in an airplane, and `drive` any body that
        // flies.
        Aim::Peer { .. } | Aim::Keys(_) => Step::End(MoveOutcome::Interrupted),
    }
}

/// `PreUpdate`, after the steering: an airplane with no movement under way
/// holds its engine cut. Its engine runs at cruise with no key held, so
/// left alone on the ground it takes off within a second and climbs for
/// ever - from the moment it is spawned, too. A person would hold Shift;
/// so does the agent.
pub(super) fn park(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    movement: Option<Res<Movement>>,
    live: Option<Res<LiveAvatarRecord>>,
    mut parked: Local<bool>,
) {
    let idle = movement.is_none_or(|movement| movement.goal.is_none());
    let airplane =
        live.is_some_and(|live| matches!(live.0.locomotion, LocomotionConfig::Airplane(_)));
    if idle && airplane {
        keys.press(KeyCode::ShiftLeft);
        *parked = true;
    } else if *parked {
        // A movement now holds the keys it wants; a body that is no longer
        // an airplane wants none.
        if idle {
            keys.release(KeyCode::ShiftLeft);
        }
        *parked = false;
    }
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;
    use crate::agent::control::events::{DriveReport, EventLog};
    use crate::player::sim::{DriveBench, FlightBench};

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
                drive: Drive::Ground(Ground::OnFoot),
                best_distance: 50.0,
                best_at: 0.0,
                flight: None,
                sortie: None,
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
                    drive: Drive::Ground(Ground::OnFoot),
                    best_distance: f32::INFINITY,
                    best_at: 0.0,
                    flight: None,
                    sortie: None,
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

    /// The daemon's own steering, flying the airship stand-in on the flight
    /// bench - or driving a car on the drive bench (#1531): the `steer`
    /// system, the event log it writes to, and the clock it reads, turned on
    /// a frame at a time with the physics caught up between.
    struct Daemon<B = FlightBench> {
        bench: B,
        steer: bevy::ecs::system::SystemId,
        park: bevy::ecs::system::SystemId,
        log: Arc<EventLog>,
        start: f64,
        now: f64,
    }

    /// What the daemon needs of a bench: its world, its clock, and one fixed
    /// step of its physics.
    trait Bench {
        fn world_of(&mut self) -> &mut World;
        fn seconds(&self) -> f64;
        fn step_physics(&mut self);
    }

    impl Bench for FlightBench {
        fn world_of(&mut self) -> &mut World {
            self.world_mut()
        }
        fn seconds(&self) -> f64 {
            self.elapsed()
        }
        fn step_physics(&mut self) {
            self.step();
        }
    }

    impl Bench for DriveBench {
        fn world_of(&mut self) -> &mut World {
            self.world_mut()
        }
        fn seconds(&self) -> f64 {
            self.elapsed()
        }
        fn step_physics(&mut self) {
            self.step();
        }
    }

    /// A daemon frame, as the agent's loop runs them.
    const FRAME: f64 = 1.0 / 30.0;

    /// The airship stand-in's record - wearing the default airplane
    /// instead, when `airplane`.
    fn stand_in(airplane: bool) -> crate::pds::avatar::AvatarRecord {
        let mut record =
            crate::pds::avatar::AvatarRecord::default_for_did("did:plc:agentofflineair222222222");
        if airplane {
            record.locomotion = LocomotionConfig::Airplane(Box::default());
        }
        record
    }

    impl Daemon {
        fn at(at: Vec3) -> Self {
            Self::wearing(&stand_in(false), at)
        }

        fn wearing(record: &crate::pds::avatar::AvatarRecord, at: Vec3) -> Self {
            Self::over(FlightBench::new(record, at))
        }
    }

    impl<B: Bench> Daemon<B> {
        fn over(mut bench: B) -> Self {
            let log = Arc::new(EventLog::new(64, "test".into()));
            let world = bench.world_of();
            world.insert_resource(EventSink(Arc::clone(&log)));
            world.insert_resource(State::new(AppState::InGame));
            world.init_resource::<Movement>();
            let steer = world.register_system(steer);
            let park = world.register_system(park);
            let start = bench.seconds();
            Self {
                bench,
                steer,
                park,
                log,
                start,
                now: 0.0,
            }
        }

        fn world(&mut self) -> &mut World {
            self.bench.world_of()
        }

        /// One frame: the clock on, the steering, the physics caught up.
        fn frame(&mut self) {
            self.now += FRAME;
            let world = self.bench.world_of();
            world
                .resource_mut::<Time<Real>>()
                .update_with_duration(Duration::from_secs_f64(FRAME));
            world.run_system(self.steer).expect("the steering runs");
            world.run_system(self.park).expect("the parking runs");
            while self.bench.seconds() - self.start < self.now {
                self.bench.step_physics();
            }
        }

        /// The body's height above what is below it.
        fn height(&mut self) -> f32 {
            sense::height(self.world()).expect("a body over the floor")
        }

        /// Frames until `done` says so - or panic after `limit` seconds.
        fn until(&mut self, limit: f64, mut done: impl FnMut(&mut Self) -> bool) {
            let deadline = self.now + limit;
            while !done(self) {
                assert!(self.now < deadline, "nothing after {limit} s");
                self.frame();
            }
        }

        /// Every movement that has ended: its outcome and the height it
        /// ended at.
        fn endings(&self) -> Vec<(MoveOutcome, Option<f64>)> {
            self.log
                .after(0, Duration::ZERO)
                .events
                .into_iter()
                .filter_map(|e| match e.what {
                    EventKind::MovementEnded {
                        outcome, height_m, ..
                    } => Some((outcome, height_m)),
                    _ => None,
                })
                .collect()
        }

        /// The report of every drive that has ended, in order.
        fn reports(&self) -> Vec<DriveReport> {
            self.log
                .after(0, Duration::ZERO)
                .events
                .into_iter()
                .filter_map(|e| match e.what {
                    EventKind::MovementEnded { report, .. } => report,
                    _ => None,
                })
                .collect()
        }
    }

    /// THE OWNER'S RULE (2026-09-24): a body that flies never ends a
    /// movement in the air. Halted up there, it comes straight down first,
    /// and `halted` is said once it is down - on the ground, a halt ends at
    /// once.
    #[test]
    fn a_halt_in_the_air_comes_down_before_it_ends() {
        let mut daemon = Daemon::at(Vec3::new(0.0, 0.52, 0.0));
        walk_to(daemon.world(), Vec2::new(0.0, -80.0), false).expect("a walk-to");
        daemon.until(20.0, |daemon| daemon.height() > 10.0);

        let halted = halt(daemon.world()).expect("a halt");

        assert_eq!(halted["landing"], true, "{halted}");
        assert!(daemon.endings().is_empty(), "not over while it is up there");
        daemon.until(30.0, |daemon| !daemon.endings().is_empty());
        let ended = daemon.endings();
        assert_eq!(ended.len(), 1, "{ended:?}");
        let (outcome, height) = ended[0];
        assert_eq!(outcome, MoveOutcome::Halted);
        assert!(
            height.is_some_and(|h| h <= f64::from(crate::config::agent::TOUCHDOWN_HEIGHT_M)),
            "ended {height:?} m up"
        );

        walk_to(daemon.world(), Vec2::new(0.0, -80.0), false).expect("another");
        let halted = halt(daemon.world()).expect("a halt on the ground");
        assert!(halted["landing"].is_null(), "down already: {halted}");
        assert_eq!(daemon.endings().len(), 2, "ended at once");
    }

    /// Stuck in the air - here under a ceiling it cannot climb through -
    /// it comes down where it is, and says `stuck` once it is down.
    #[test]
    fn a_flight_stuck_in_the_air_comes_down_before_it_ends() {
        let mut daemon = Daemon::at(Vec3::new(0.0, 0.52, 0.0));
        daemon.bench.block(
            Transform::from_xyz(0.0, 12.0, 0.0),
            Vec3::new(60.0, 0.5, 60.0),
        );
        walk_to(daemon.world(), Vec2::new(0.0, -80.0), false).expect("a walk-to");

        daemon.until(60.0, |daemon| !daemon.endings().is_empty());

        let ended = daemon.endings();
        assert_eq!(ended[0].0, MoveOutcome::Stuck, "{ended:?}");
        assert!(
            ended[0]
                .1
                .is_some_and(|h| h <= f64::from(crate::config::agent::TOUCHDOWN_HEIGHT_M)),
            "ended {:?} m up",
            ended[0].1
        );
    }

    /// Escorting a player who leaves, it comes down where it is and says
    /// `peer_left` once it is down.
    #[test]
    fn an_escort_left_in_the_air_comes_down_before_it_ends() {
        let mut daemon = Daemon::at(Vec3::new(0.0, 0.52, 0.0));
        let friend = daemon
            .world()
            .spawn((
                peer("did:plc:friend"),
                placed(true),
                GlobalTransform::from(Transform::from_xyz(0.0, 0.0, -40.0)),
            ))
            .id();
        follow(daemon.world(), "did:plc:friend".into(), 3.0, false).expect("a follow");
        daemon.until(20.0, |daemon| daemon.height() > 5.0);

        daemon.world().entity_mut(friend).despawn();

        daemon.until(30.0, |daemon| !daemon.endings().is_empty());
        let ended = daemon.endings();
        assert_eq!(ended[0].0, MoveOutcome::PeerLeft, "{ended:?}");
        assert!(
            ended[0]
                .1
                .is_some_and(|h| h <= f64::from(crate::config::agent::TOUCHDOWN_HEIGHT_M)),
            "ended {:?} m up",
            ended[0].1
        );
    }

    /// An airplane's engine runs with no key held: left alone on the ground
    /// it takes off within a second and climbs for ever. With nothing flying
    /// it, the daemon holds its engine cut, and it stays put.
    #[test]
    fn an_airplane_with_nothing_to_do_stays_on_the_ground() {
        let mut daemon = Daemon::wearing(&stand_in(true), Vec3::new(0.0, 0.35, 0.0));
        let from = daemon.bench.position();

        for _ in 0..(5 * 30) {
            daemon.frame();
        }

        assert!(
            daemon.height() < 0.1 && daemon.bench.position().distance(from) < 0.5,
            "it is {:.2} m up, {:.2} m from where it stood",
            daemon.height(),
            daemon.bench.position().distance(from)
        );
    }

    /// An airplane does not follow, and does not turn on the spot in the
    /// air (owner, 2026-09-24) - each is refused with the reason.
    #[test]
    fn an_airplane_refuses_a_follow_and_a_turn_in_the_air() {
        let mut daemon = Daemon::wearing(&stand_in(true), Vec3::new(0.0, 0.35, 0.0));
        daemon.world().spawn((
            peer("did:plc:friend"),
            placed(true),
            GlobalTransform::from(Transform::from_xyz(0.0, 0.0, -40.0)),
        ));

        let followed = follow(daemon.world(), "did:plc:friend".into(), 3.0, false);
        assert!(followed.is_err_and(|e| e.contains("cannot follow")));

        walk_to(daemon.world(), Vec2::new(0.0, -300.0), false).expect("a walk-to");
        daemon.until(20.0, |daemon| daemon.height() > 5.0);
        let faced = face(daemon.world(), FaceTarget::Point(Vec2::new(100.0, 0.0)));
        assert!(
            faced.is_err_and(|e| e.contains("on the ground only")),
            "a face in the air"
        );
    }

    /// Halted up in the air, an airplane lands where it can - it cannot
    /// come straight down - and says `halted` once it is down.
    #[test]
    fn a_halt_in_an_airplane_lands_ahead_before_it_ends() {
        let mut daemon = Daemon::wearing(&stand_in(true), Vec3::new(0.0, 0.35, 0.0));
        walk_to(daemon.world(), Vec2::new(0.0, -300.0), false).expect("a walk-to");
        daemon.until(20.0, |daemon| daemon.height() > 10.0);

        let halted = halt(daemon.world()).expect("a halt");

        assert_eq!(halted["landing"], true, "{halted}");
        daemon.until(90.0, |daemon| !daemon.endings().is_empty());
        let (outcome, height) = daemon.endings()[0];
        assert_eq!(outcome, MoveOutcome::Halted);
        assert!(
            height.is_some_and(|h| h <= f64::from(crate::config::agent::TOUCHDOWN_HEIGHT_M)),
            "ended {height:?} m up"
        );
        // And down, it stays down: the parking holds its engine cut.
        for _ in 0..(3 * 30) {
            daemon.frame();
        }
        assert!(
            daemon.height() < 0.1,
            "{:.2} m up after landing",
            daemon.height()
        );
    }

    /// A ramp of two planks rising toward -Z at `degrees`, its lip `height`
    /// m up at `lip_z`: one plank under each wheel track of a car
    /// `half_width` m wide and nothing between them, the way a car is driven
    /// up onto a trailer. Each plank starts halfway between the car's box
    /// and the 90% of it the agent's height sweeps down, so the car's corner
    /// rays meet the planks and the sweep passes between them to the floor.
    fn plank_ramp(bench: &mut DriveBench, half_width: f32, lip_z: f32, height: f32, degrees: f32) {
        let theta = degrees.to_radians();
        let length = height / theta.sin();
        let thick = 1.0;
        let turned = Quat::from_rotation_x(theta);
        let (inner, outer) = (0.95 * half_width, half_width + 0.6);
        for side in [-1.0, 1.0] {
            let middle = Vec3::new(
                side * (inner + outer) * 0.5,
                height * 0.5,
                lip_z + length * 0.5 * theta.cos(),
            );
            bench.block(
                Transform::from_translation(middle - turned * Vec3::Y * thick)
                    .with_rotation(turned),
                Vec3::new((outer - inner) * 0.5, thick, length * 0.5),
            );
        }
    }

    /// A car's walk-to stops on its point (#1536): it comes in no faster than
    /// it can stop, says it has arrived once it is in its circle and at
    /// rest, and is still there two seconds on. It used to hold the throttle
    /// to the circle's edge and let go there: Jink's lineups rolled 7-11 m
    /// past their points (session 895), once to within 9 m of a gateway.
    #[test]
    fn a_cars_walk_to_stops_on_its_point() {
        let mut record = crate::pds::avatar::AvatarRecord::default_for_did("did:plc:agentwalkcar");
        record.locomotion = LocomotionConfig::Car(Box::default());
        let mut daemon = Daemon::over(DriveBench::parked(&record, 0.0, 0.0));
        let target = Vec2::new(0.0, -40.0);
        walk_to(daemon.world(), target, false).expect("a walk-to");

        daemon.until(30.0, |daemon| !daemon.endings().is_empty());
        assert_eq!(daemon.endings()[0].0, MoveOutcome::Arrived);
        for _ in 0..60 {
            daemon.frame();
        }

        let off = daemon.bench.position().xz().distance(target);
        assert!(
            off <= crate::config::agent::ARRIVE_WHEELED_M,
            "two seconds after it arrived the car is {off:.2} m from its point"
        );
    }

    /// A car following a player brakes to a stand short of them (#1536): it
    /// used to let go at its following distance at full speed and drive on
    /// through the player - from 40 m, 10.85 m/s, 0.01 m from where they
    /// stood, at rest 6.78 m beyond (the session's end review).
    #[test]
    fn a_car_following_a_player_stops_short_of_them() {
        let mut record =
            crate::pds::avatar::AvatarRecord::default_for_did("did:plc:agentfollowcar");
        record.locomotion = LocomotionConfig::Car(Box::default());
        let mut daemon = Daemon::over(DriveBench::parked(&record, 0.0, 0.0));
        let player = Vec3::new(0.0, 0.0, -40.0);
        daemon.world().spawn((
            peer("did:plc:friend"),
            placed(true),
            GlobalTransform::from(Transform::from_translation(player)),
        ));
        let keep = 3.0;
        follow(daemon.world(), "did:plc:friend".into(), keep, false).expect("a follow");

        let mut nearest = f32::MAX;
        for _ in 0..(15.0 / FRAME) as usize {
            daemon.frame();
            nearest = nearest.min(daemon.bench.position().xz().distance(player.xz()));
        }

        assert!(
            nearest >= keep * 0.5,
            "it came within {nearest:.2} m of the player it follows at {keep} m"
        );
        let resting = daemon.bench.position().xz().distance(player.xz());
        assert!(
            resting <= keep + crate::config::agent::FOLLOW_SLACK_M + 1.0,
            "it stood {resting:.2} m off, following at {keep} m"
        );
    }

    /// A drive reads a car's jump by its wheels (#1531): `steer` hands the
    /// run the car's own count of wheels on the ground. Up a ramp of two
    /// planks the car's body has nothing under it but the floor, so by its
    /// height alone - the rule for a body with no count - it left the ground
    /// halfway up the ramp, with every wheel on a plank; by its wheels it
    /// leaves at the lip.
    #[test]
    fn a_drive_reads_a_cars_jump_by_its_wheels() {
        const LIP_Z: f32 = -30.0;
        let mut record = crate::pds::avatar::AvatarRecord::default_for_did("did:plc:agentdrive");
        record.locomotion = LocomotionConfig::Car(Box::default());
        let half_width = match &record.locomotion {
            LocomotionConfig::Car(p) => p.chassis_half_extents.0[0],
            _ => unreachable!("a car"),
        };
        let airborne_above = keys::airborne_above(&record.locomotion).expect("a car drives");
        let mut bench = DriveBench::parked(&record, 0.0, 0.0);
        plank_ramp(&mut bench, half_width, LIP_Z, 1.5, 18.0);
        let mut daemon = Daemon::over(bench);
        drive(
            daemon.world(),
            vec![DriveSegment {
                keys: vec!["W".to_owned()],
                secs: 6.0,
            }],
        )
        .expect("a drive");

        // The most frames running, every wheel down, that the height rule
        // reads as off the ground - two make a jump.
        let (mut running, mut most) = (0, 0);
        daemon.until(8.0, |daemon| {
            let height = daemon.height();
            let world = daemon.world();
            let wheels = world
                .query_filtered::<&crate::player::CarContact, With<LocalPlayer>>()
                .single(world)
                .expect("the bench car")
                .wheels;
            running = if wheels == 4 && height > airborne_above {
                running + 1
            } else {
                0
            };
            most = most.max(running);
            !daemon.endings().is_empty()
        });

        assert!(
            most >= 2,
            "the premise: up the planks the height rule reads the car as off the ground"
        );
        let reports = daemon.reports();
        let [report] = reports.as_slice() else {
            panic!("one drive, one report: {reports:?}");
        };
        // The first jump is the one off the lip: read it alone, so a landing
        // that lifted the wheels again - the default car did on its damping
        // before #1534 - cannot change what this reads.
        let jump = report.jumps.first().expect("a jump off the lip");
        assert!(jump.landed_s.is_some(), "it came down: {jump:?}");
        // Off at the lip: the car's middle past it, so its rear wheels too
        // within a frame.
        assert!(
            jump.from[1] < f64::from(LIP_Z),
            "it left the ground at z {}, short of the lip at {LIP_Z}: {jump:?}",
            jump.from[1]
        );
    }
}
