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
//! movement, or travel, cuts it off at once. And with no movement under
//! way an airplane holds Shift - its throttle closed and its brakes on
//! ([`park`]) - as a person leaves one parked.
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
    CAMERA_CAUGHT_UP_DEG, FACE_SETTLE_SECS, FACE_TOLERANCE_DEG, FOLLOW_RUN_BEYOND_M,
    PROGRESS_STEP_M, PROGRESS_TURN_DEG, STUCK_AFTER_SECS, WHEELED_STOPPED_MS,
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
use ground::{Ground, Stopping, brake, follow_blocked, follow_moves, nearest_turn};
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
    /// by on the ground. A walker's turn only uses the time: when it
    /// started.
    best_distance: f32,
    best_at: f64,
    /// How far a walk-to set out from its point - what a driven body's
    /// arrival circle shrinks to half of (#1866); infinite for every other
    /// movement.
    set_out: f32,
    /// The least a driven body has been turned from its way, degrees either
    /// side: turning nearer it is progress too (#1541), and moves `best_at` -
    /// on a follow, the blockage clock `still_since`.
    best_off: f32,
    /// How the body stops, for a driven one's approach (#1570).
    stopping: Stopping,
    /// A rotorcraft's flight, from the frame it first flies.
    flight: Option<Flight>,
    /// An airplane's, from the frame it first flies.
    sortie: Option<Sortie>,
    /// The fastest a beast lying along its capsule turns (rad/s, #1820),
    /// from its record: it is turned by the physics, its ends no faster
    /// than the walk lets them sweep, so a turn takes it the time the angle
    /// needs, where a person's facing comes round in a step. `None` for
    /// every other body.
    sweep: Option<f32>,
}

/// The movement under way, if any - and a standing follow (#1852), which
/// outlives it.
#[derive(Resource, Default)]
pub(super) struct Movement {
    goal: Option<Goal>,
    last_id: u64,
    standing: Option<Standing>,
}

/// A standing follow (#1852): `follow --standing`. The follow it starts ends
/// as any follow does, but the aim stays: with no movement under way the
/// daemon starts it again ([`resume_standing`]) whenever the player is in
/// the agent's world - when they come back, and after each errand (a walk, a
/// turn, a drive, a trip away and back). Only `halt`, or another `follow`
/// (even a refused one), ends it. Kept in the daemon's memory alone: a
/// restart forgets it.
#[derive(Clone, Debug, PartialEq)]
struct Standing {
    did: String,
    keep: f32,
    run: bool,
    /// Why it could not start again the last time it tried, while it waits:
    /// the player is not in this world, or the body cannot follow.
    waiting: Option<String>,
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
/// movement under way. `standing` (#1852) keeps the aim after the follow
/// ends, to start it again whenever the player is back ([`Standing`]) - and
/// is taken while they are away, to start once they come. Any other follow
/// past the argument checks - plain, or a `standing` one refused - ends an
/// older standing follow; a refusal says so.
pub(super) fn follow(
    world: &mut World,
    did: String,
    keep: f32,
    run: bool,
    standing: bool,
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
    // Past its argument checks above (which change nothing), any follow ends
    // an older standing follow, refused or not - a refused `--standing` one
    // too: the newest word is the aim, and an old one carrying on unasked is
    // the surprise. The follow under way, if a refusal leaves one, runs on
    // until it ends, and does not start again.
    let old = world.get_resource_or_init::<Movement>().standing.take();
    let started = start_follow(world, &did, keep, run);
    let aim = Standing {
        did: did.clone(),
        keep,
        run,
        waiting: started.as_ref().err().cloned(),
    };
    match started {
        Ok(mut answer) => {
            world.get_resource_or_init::<Movement>().standing = standing.then_some(aim);
            if standing {
                answer["standing"] = json!(true);
            }
            Ok(answer)
        }
        // Away: a standing follow waits for them.
        Err(why) if standing && !peer_here(world, &did) => {
            let mut movement = world.get_resource_or_init::<Movement>();
            movement.standing = Some(aim);
            Ok(json!({
                "goal_id": null,
                "following": did,
                "keep_m": hundredths(keep),
                "standing": true,
                "waiting": why,
            }))
        }
        Err(why) => Err(match old {
            Some(old) => format!("{why}; the standing follow of {} has ended", old.did),
            None => why,
        }),
    }
}

/// Whether the player `did` is in the agent's world, placed or not.
fn peer_here(world: &mut World, did: &str) -> bool {
    world
        .query::<&RemotePeer>()
        .iter(world)
        .any(|peer| peer.did.as_deref() == Some(did))
}

/// Begin a follow of `did`, replacing any movement under way: what `follow`
/// and a standing follow's resumption share.
fn start_follow(world: &mut World, did: &str, keep: f32, run: bool) -> Result<Value, String> {
    let did = did.to_owned();
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
    let locomotion = &world
        .get_resource::<LiveAvatarRecord>()
        .ok_or("the agent has no body yet")?
        .0
        .locomotion;
    let drive = Drive::of(locomotion)?;
    let stopping = Stopping::of(locomotion);
    let sweep = ground::lying_sweep(locomotion);
    if let Drive::Wing(wing) = drive {
        match &aim {
            Aim::Peer { .. } => {
                return Err(
                    "an airplane cannot follow: it cannot hover, nor keep to a walker's \
                            pace (#1431)"
                        .to_owned(),
                );
            }
            Aim::Heading { .. }
                if sense::craft(world).is_some_and(|c| wing::airborne(&wing, &c)) =>
            {
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
        set_out: best_distance,
        best_off: f32::INFINITY,
        stopping,
        flight: None,
        sortie: None,
        sweep,
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
    let mut answer = halt_goal(world)?;
    // A halt ends a standing follow too (#1852): it is how one is ended.
    if let Some(standing) = world.get_resource_or_init::<Movement>().standing.take() {
        answer["standing_ended"] = json!(standing.did);
    }
    Ok(answer)
}

/// Stop the movement under way, if any - see [`halt`].
fn halt_goal(world: &mut World) -> Result<Value, String> {
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
    goal.sortie = match goal.drive {
        Drive::Wing(wing) => Some(Sortie::landing_ahead(&wing, craft, now)),
        Drive::Rotor(_) | Drive::Ground(_) => None,
    };
}

/// Whether a body is down, as far as ending a movement goes: a rotorcraft
/// down on what is below it, an airplane on the ground.
fn grounded(drive: Drive, craft: &Craft) -> bool {
    match drive {
        Drive::Wing(wing) => !wing::airborne(&wing, craft),
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
    // A standing follow (#1852), whatever is under way: the follow it keeps
    // starting, or what it waits for.
    let standing = movement.standing.as_ref().map(|standing| {
        json!({
            "peer": standing.did,
            "keep_m": hundredths(standing.keep),
            "run": standing.run,
        })
    });
    let Some(goal) = movement.goal.as_ref() else {
        let Some(waiting) = movement.standing.as_ref() else {
            return Value::Null;
        };
        return json!({
            "goal_id": null,
            "doing": "waiting_to_follow",
            "peer": waiting.did,
            "waiting": waiting.waiting,
            "standing": standing,
        });
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
    if let Some(standing) = standing {
        described["standing"] = standing;
    }
    described
}

/// `PreUpdate`, before the steering: start a standing follow again (#1852)
/// when nothing is under way and its player is in the agent's world - back
/// from away, or after the errand that replaced it - saying so with a
/// `follow_resumed`. One it cannot start (an airplane cannot follow) waits,
/// the reason in `status.movement.waiting`, and is tried again each frame.
pub(super) fn resume_standing(world: &mut World) {
    let Some(movement) = world.get_resource::<Movement>() else {
        return;
    };
    let Some(standing) = movement.standing.clone() else {
        return;
    };
    if movement.goal.is_some()
        || *world.resource::<State<AppState>>().get() != AppState::InGame
        || world.contains_resource::<TravelingTo>()
    {
        return;
    }
    // The one body the steering reads (`steer` takes `single()`, as here):
    // two would let a follow begin on the first and end `interrupted` the same
    // frame, to begin again the next - a loop of `follow_resumed` and
    // `movement_ended`. It waits for one body instead.
    let one_body = world
        .query_filtered::<&GlobalTransform, With<LocalPlayer>>()
        .single(world)
        .is_ok();
    let started = if !one_body {
        Err("the agent has no single body to follow with".to_owned())
    } else if peer_here(world, &standing.did) {
        start_follow(world, &standing.did, standing.keep, standing.run)
    } else {
        Err(format!("{} is not in this world", standing.did))
    };
    let waiting = match started {
        Ok(answer) => {
            if let Some(goal_id) = answer["goal_id"].as_u64() {
                world
                    .resource::<EventSink>()
                    .0
                    .push(EventKind::FollowResumed {
                        goal_id,
                        did: standing.did.clone(),
                    });
            }
            None
        }
        Err(why) => Some(why),
    };
    if let Some(aim) = world.resource_mut::<Movement>().standing.as_mut()
        && aim.waiting != waiting
    {
        aim.waiting = waiting;
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
    // How the body moves now: a wear changes it under a movement (#1851).
    live: Option<Res<LiveAvatarRecord>>,
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
    let changed = live
        .as_deref()
        .and_then(|live| take_up_body(goal, &live.0.locomotion, position, craft.as_ref(), now));
    let step = if *state.get() != AppState::InGame || traveling.is_some() || body.is_none() {
        Step::End(MoveOutcome::Interrupted)
    } else if let Some(outcome) = changed {
        Step::End(outcome)
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
                steer_ground(
                    goal,
                    ground,
                    body,
                    swimming(local.as_deref()),
                    &peers,
                    now,
                    &sink,
                )
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
        if swimming(local.as_deref()) {
            orbit.pitch = Some(0.0);
            orbit.target_pitch = 0.0;
        }
        orbit.force_update = true;
    }
}

/// Carry `goal` on under the body the agent wears now, moving by
/// `locomotion` (#1851): the outcome to end it with when that body cannot.
///
/// A movement used to work out how to drive the body once, when it began,
/// and steer by that to its end: Kairos followed the owner on foot, wore an
/// airship, and the follow pressed a walker's W on the landed craft - which
/// only Space lifts off - until it said `follow_blocked` 1.5 minutes later
/// (live 19). Now each frame reads how the body moves. The same way with new
/// numbers (an edit of the preset's tuning) carries straight on with them; a
/// new way begins the movement again from where the body is - a flight or a
/// sortie from scratch, the clocks `stuck` and `follow_blocked` are measured
/// by restarted, a turn's re-aiming forgotten - so a follow escorts, a walk
/// flies, a turn swings round, under the body it has. A body that comes
/// down from a halt comes down as its new self, or is down already. What
/// the new body cannot do ends: an airplane cannot follow, and a `drive`'s
/// keys were meant for the body on wheels it began with.
fn take_up_body(
    goal: &mut Goal,
    locomotion: &LocomotionConfig,
    position: Vec3,
    craft: Option<&Craft>,
    now: f64,
) -> Option<MoveOutcome> {
    let Ok(drive) = Drive::of(locomotion) else {
        return Some(MoveOutcome::BodyChanged);
    };
    // A person become a beast, or a beast re-fitted, walks on: its turn
    // takes the time the new body needs.
    goal.sweep = ground::lying_sweep(locomotion);
    goal.stopping = Stopping::of(locomotion);
    let same_way = match (drive, goal.drive) {
        (Drive::Ground(now), Drive::Ground(was)) => now == was,
        (Drive::Rotor(_), Drive::Rotor(_)) | (Drive::Wing(_), Drive::Wing(_)) => true,
        _ => false,
    };
    goal.drive = drive;
    if same_way {
        return None;
    }
    match (&mut goal.aim, drive) {
        (Aim::Keys(_), _) | (Aim::Peer { .. }, Drive::Wing(_)) => {
            return Some(MoveOutcome::BodyChanged);
        }
        (Aim::Point(target), _) => {
            goal.best_distance = position.xz().distance(*target);
            goal.set_out = goal.best_distance;
        }
        (
            Aim::Peer {
                closing,
                still_at,
                still_since,
                blocked,
                ..
            },
            _,
        ) => {
            *closing = true;
            *still_at = position.xz();
            *still_since = now;
            *blocked = false;
        }
        (
            Aim::Heading {
                bias_deg, phase, ..
            },
            _,
        ) => {
            *bias_deg = 0.0;
            *phase = TurnPhase::Moving { since: now };
        }
        (Aim::Down { .. }, _) => {}
    }
    goal.best_at = now;
    goal.best_off = f32::INFINITY;
    goal.flight = None;
    goal.sortie = None;
    if let (Aim::Down { .. }, Some(craft)) = (&goal.aim, craft) {
        goal.flight = Some(Flight::landing(craft, now));
        if let Drive::Wing(wing) = drive {
            goal.sortie = Some(Sortie::landing_ahead(&wing, craft, now));
        }
    }
    None
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
    swims: bool,
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
        set_out,
        best_off,
        stopping,
        sweep,
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
            if wheeled {
                let way = *target - position.xz();
                turning(best_off, best_at, heading_off(forward, way), now);
            }
            // Planned to stop half the arrival radius short of the point; it
            // has arrived once in the circle and at rest.
            let arrive_within = ground.arrive_within(*set_out);
            if distance <= arrive_within {
                if wheeled && rolling > WHEELED_STOPPED_MS {
                    Step::Drive(brake(forward, velocity))
                } else {
                    Step::End(MoveOutcome::Arrived)
                }
            } else if now - *best_at >= STUCK_AFTER_SECS {
                Step::End(MoveOutcome::Stuck)
            } else if wheeled && rolling > stopping.approach_speed(distance - arrive_within * 0.5) {
                Step::Drive(brake(forward, velocity))
            } else {
                Step::Drive(ground.toward(position, forward, *target, *run && !swims))
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
                        *best_off = f32::INFINITY;
                    }
                    // A driven body swinging round to its player is getting
                    // somewhere (#1541).
                    if wheeled {
                        let way = at.xz() - position.xz();
                        turning(best_off, still_since, heading_off(forward, way), now);
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
                    if wheeled && rolling > stopping.approach_speed(*gap - *keep) {
                        Step::Drive(brake(forward, velocity))
                    } else {
                        Step::Drive(ground.toward(position, forward, at.xz(), run && !swims))
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
            // A swing round is given its time from its last real turn, not
            // from its start: a car turns at its own pace (#1541).
            if wheeled {
                turning(best_off, best_at, off, now);
            }
            let out_of_time = now - *best_at >= ground.turn_within(*sweep);
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
                    let stepped =
                        ground == Ground::OnFoot && now - since >= ground::face_pulse(*sweep);
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
    let sortie = sortie.get_or_insert_with(|| Sortie::new(wing, craft, now));
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
/// holds Shift, its throttle closed and its brakes on. Its throttle holds
/// where it is left (#1615), so a movement that ended with it open - stuck
/// on its take-off run, say - would leave it rolling on, and up into the
/// air once it was fast enough; and on a slope only its brakes hold it. A
/// person would hold Shift; so does the agent.
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

/// Whether the body swims now. A walker asked to run lets go of the run key
/// while it does (#1821): the same key swims down, and a run into a lake ended
/// on its bed.
fn swimming(local: Option<&LocalMovement>) -> bool {
    local.is_some_and(|m| matches!(m.water, WaterState::Swimming { .. }))
}

/// Note `off`, how far the body is turned from its way in degrees either
/// side, if it is the least yet by a real turn ([`PROGRESS_TURN_DEG`]), and
/// when: whether it was.
fn turning(best_off: &mut f32, best_at: &mut f64, off: f32, now: f64) -> bool {
    let off = off.abs();
    let nearer = off + PROGRESS_TURN_DEG <= *best_off;
    if nearer {
        *best_off = off;
        *best_at = now;
    }
    nearer
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

    /// A beast lying along its capsule goes on foot (#1802): the preset is a
    /// person's, so the agent walks it where the camera looks, and it is no
    /// body on wheels for `drive`.
    /// **A run into a lake swims, it does not dive** (#1821): the run key is
    /// the swim-down key, and a run held on into the water took a body to the
    /// bed and along it. The steering lets go of it from here.
    #[test]
    fn a_run_lets_go_of_its_key_while_swimming() {
        let on = |water| LocalMovement {
            water,
            ..Default::default()
        };
        assert!(!swimming(None));
        assert!(!swimming(Some(&on(WaterState::Dry))));
        assert!(!swimming(Some(&on(WaterState::Wading { depth: 0.2 }))));
        assert!(swimming(Some(&on(WaterState::Swimming { depth: 0.3 }))));
        let held = |swims: bool| ground::on_foot(Vec3::ZERO, Vec2::new(0.0, 10.0), !swims).keys;
        assert!(held(false).contains(&KeyCode::ShiftLeft));
        assert!(!held(true).contains(&KeyCode::ShiftLeft));
    }

    #[test]
    fn a_beast_walks_on_foot() {
        let beast = crate::pds::avatar::locomotion::beast_record(symbios_avatar::plan::Family::Dog);
        assert!(matches!(
            &beast.locomotion,
            LocomotionConfig::Humanoid(p) if p.is_lying()
        ));
        assert_eq!(
            Drive::of(&beast.locomotion),
            Ok(Drive::Ground(Ground::OnFoot))
        );
        assert_eq!(keys::airborne_above(&beast.locomotion), None);
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
                drive: Drive::Ground(Ground::OnFoot),
                best_distance: 50.0,
                best_at: 0.0,
                set_out: 50.0,
                best_off: f32::INFINITY,
                stopping: Stopping::default(),
                flight: None,
                sortie: None,
                sweep: None,
            }),
            last_id: 4,
            standing: None,
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
                    set_out: f32::INFINITY,
                    best_off: f32::INFINITY,
                    stopping: Stopping::default(),
                    flight: None,
                    sortie: None,
                    sweep: None,
                }),
                last_id: 7,
                standing: None,
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

    /// A person on foot.
    fn person() -> crate::pds::avatar::AvatarRecord {
        let mut person = crate::pds::avatar::AvatarRecord::default_for_did("did:plc:agentperson");
        person.locomotion = LocomotionConfig::Humanoid(Box::default());
        person
    }

    /// The daemon's movement systems as it schedules them - a standing
    /// follow's resumption, then the steering - over a person at the origin
    /// and nobody else (#1852).
    fn standing_app() -> (App, Arc<EventLog>, Entity) {
        let log = Arc::new(EventLog::new(64, "test".into()));
        let mut app = App::new();
        app.insert_resource(EventSink(Arc::clone(&log)))
            .insert_resource(State::new(AppState::InGame))
            .insert_resource(LiveAvatarRecord(person()))
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Time<Real>>()
            .init_resource::<Movement>()
            .add_systems(Update, (resume_standing, steer).chain());
        let me = app
            .world_mut()
            .spawn((LocalPlayer, GlobalTransform::default()))
            .id();
        (app, log, me)
    }

    /// The friend, placed `z` metres down +Z.
    fn friend_at(app: &mut App, z: f32) -> Entity {
        app.world_mut()
            .spawn((
                peer("did:plc:friend"),
                placed(true),
                GlobalTransform::from(Transform::from_xyz(0.0, 0.0, z)),
            ))
            .id()
    }

    /// Every event of the log, in order.
    fn events(log: &EventLog) -> Vec<EventKind> {
        log.after(0, Duration::ZERO)
            .events
            .into_iter()
            .map(|e| e.what)
            .collect()
    }

    fn following(app: &App) -> bool {
        app.world()
            .resource::<Movement>()
            .goal
            .as_ref()
            .is_some_and(|goal| matches!(goal.aim, Aim::Peer { .. }))
    }

    /// **A standing follow starts again when its player comes back**
    /// (#1852; the owner, live 17: "Please follow me, while you do not need
    /// to be somewhere else in the region"). The follow ends `peer_left` as
    /// any does - a tab asleep two minutes is gone - and the aim stays:
    /// `status.movement` says it is waiting, and on the player's return the
    /// follow starts again by itself, saying so.
    #[test]
    fn a_standing_follow_starts_again_when_its_player_comes_back() {
        let (mut app, log, _) = standing_app();
        let friend = friend_at(&mut app, 10.0);
        let answer = follow(app.world_mut(), "did:plc:friend".into(), 3.0, false, true)
            .expect("a standing follow");
        assert_eq!(answer["standing"], true, "{answer}");
        app.update();
        assert!(walking(&app), "following");

        app.world_mut().entity_mut(friend).despawn();
        app.update();
        app.update();
        assert!(
            !following(&app) && !walking(&app),
            "they left: nobody to follow"
        );
        let status = describe(app.world_mut());
        assert_eq!(status["doing"], "waiting_to_follow", "{status}");
        assert_eq!(status["standing"]["peer"], "did:plc:friend", "{status}");

        friend_at(&mut app, 10.0);
        app.update();
        assert!(
            following(&app) && walking(&app),
            "they came back: following again"
        );
        let status = describe(app.world_mut());
        assert_eq!(status["doing"], "following", "{status}");
        assert_eq!(status["standing"]["peer"], "did:plc:friend", "{status}");
        let kinds = events(&log);
        assert!(
            matches!(
                kinds.as_slice(),
                [
                    EventKind::MovementEnded {
                        outcome: MoveOutcome::PeerLeft,
                        ..
                    },
                    EventKind::FollowResumed { did, .. },
                ] if did == "did:plc:friend"
            ),
            "{kinds:?}"
        );
    }

    /// **A standing follow resumes after the agent's errands, and a halt
    /// ends it** (#1852): a walk-to replaces the follow, and once it has
    /// arrived the follow starts again; a trip away (out of the world and
    /// back) the same. `halt` ends the follow and the aim with it, and
    /// nothing starts again after; a plain `follow` ends the aim too.
    #[test]
    fn a_standing_follow_resumes_after_an_errand_and_ends_on_halt() {
        let (mut app, log, me) = standing_app();
        friend_at(&mut app, 10.0);
        follow(app.world_mut(), "did:plc:friend".into(), 3.0, false, true)
            .expect("a standing follow");
        app.update();

        walk_to(app.world_mut(), Vec2::new(5.0, 0.0), false).expect("an errand");
        app.update();
        assert!(!following(&app) && walking(&app), "on the errand");
        assert_eq!(
            describe(app.world_mut())["standing"]["peer"],
            "did:plc:friend",
            "the aim outlives the errand"
        );
        *app.world_mut().get_mut::<GlobalTransform>(me).expect("me") =
            GlobalTransform::from(Transform::from_xyz(5.0, 0.0, 0.0));
        app.update();
        app.update();
        assert!(following(&app), "back to following once it arrived");

        // Away to another world and back.
        app.insert_resource(State::new(AppState::Loading));
        app.update();
        app.update();
        assert!(!following(&app), "travel cuts a follow off");
        app.insert_resource(State::new(AppState::InGame));
        app.update();
        assert!(
            following(&app),
            "back in a world with them: following again"
        );

        let halted = halt(app.world_mut()).expect("a halt");
        assert_eq!(halted["standing_ended"], "did:plc:friend", "{halted}");
        for _ in 0..3 {
            app.update();
        }
        assert!(!following(&app), "a halt ends a standing follow");
        assert!(describe(app.world_mut()).is_null());

        let outcomes: Vec<MoveOutcome> = events(&log)
            .into_iter()
            .filter_map(|kind| match kind {
                EventKind::MovementEnded { outcome, .. } => Some(outcome),
                _ => None,
            })
            .collect();
        assert_eq!(
            outcomes,
            [
                MoveOutcome::Replaced,
                MoveOutcome::Arrived,
                MoveOutcome::Interrupted,
                MoveOutcome::Halted
            ]
        );
        let resumed = events(&log)
            .iter()
            .filter(|kind| matches!(kind, EventKind::FollowResumed { .. }))
            .count();
        assert_eq!(resumed, 2, "after the errand, and after the trip");

        // A plain follow ends a standing one; a standing follow is taken
        // while its player is away, a plain one is not.
        follow(app.world_mut(), "did:plc:friend".into(), 3.0, false, true).expect("standing");
        follow(app.world_mut(), "did:plc:friend".into(), 3.0, false, false).expect("plain");
        assert!(app.world().resource::<Movement>().standing.is_none());
        let away = follow(app.world_mut(), "did:plc:away".into(), 3.0, false, true)
            .expect("taken while they are away");
        assert!(
            away["goal_id"].is_null() && away["waiting"].is_string(),
            "{away}"
        );
        assert!(follow(app.world_mut(), "did:plc:gone".into(), 3.0, false, false).is_err());
    }

    /// **A refused follow still ends an older standing follow** (#1852): a
    /// plain `follow` of a player not in the world, and a `follow --standing`
    /// refused while its player is here (the agent between worlds), each
    /// leave no standing aim behind - and the refusal says it ended.
    #[test]
    fn a_refused_follow_ends_a_standing_follow() {
        let (mut app, log, _) = standing_app();
        friend_at(&mut app, 10.0);

        follow(app.world_mut(), "did:plc:friend".into(), 3.0, false, true).expect("standing");
        let refused = follow(app.world_mut(), "did:plc:gone".into(), 3.0, false, false)
            .expect_err("nobody to follow");
        assert!(
            refused.contains("the standing follow of did:plc:friend has ended"),
            "{refused}"
        );
        assert!(app.world().resource::<Movement>().standing.is_none());

        follow(app.world_mut(), "did:plc:friend".into(), 3.0, false, true).expect("standing");
        app.insert_resource(State::new(AppState::Loading));
        let refused = follow(app.world_mut(), "did:plc:friend".into(), 3.0, false, true)
            .expect_err("not in a world");
        assert!(refused.contains("has ended"), "{refused}");
        assert!(app.world().resource::<Movement>().standing.is_none());
        // The follow under way runs on (a refusal changes no movement), and
        // once it ends nothing starts it again.
        app.insert_resource(State::new(AppState::InGame));
        app.update();
        assert!(following(&app), "the follow under way runs on");
        halt_goal(app.world_mut()).expect("its end");
        for _ in 0..3 {
            app.update();
        }
        assert!(!following(&app), "nothing starts the old aim again");
        let kinds = events(&log);
        assert!(
            !kinds
                .iter()
                .any(|kind| matches!(kind, EventKind::FollowResumed { .. })),
            "{kinds:?}"
        );
    }

    /// **A standing follow waits for the one body the steering needs**: with
    /// two local players `steer` cannot take `single()` and ends a follow
    /// `interrupted` the frame it begins, so a resumption that took the first
    /// body went round `follow_resumed` / `movement_ended` every frame (9 of
    /// each in 10 frames). It waits, saying why, and resumes once there is
    /// one body again.
    #[test]
    fn a_standing_follow_waits_while_there_are_two_bodies() {
        let (mut app, log, _) = standing_app();
        friend_at(&mut app, 10.0);
        follow(app.world_mut(), "did:plc:friend".into(), 3.0, false, true).expect("standing");
        app.update();
        assert!(following(&app), "following");

        let second = app
            .world_mut()
            .spawn((LocalPlayer, GlobalTransform::default()))
            .id();
        for _ in 0..10 {
            app.update();
        }
        let count = |log: &EventLog| {
            let kinds = events(log);
            let resumed = kinds
                .iter()
                .filter(|kind| matches!(kind, EventKind::FollowResumed { .. }))
                .count();
            let ended = kinds
                .iter()
                .filter(|kind| matches!(kind, EventKind::MovementEnded { .. }))
                .count();
            (resumed, ended)
        };
        assert_eq!(count(&log), (0, 1), "{:?}", events(&log));
        let status = describe(app.world_mut());
        assert_eq!(status["doing"], "waiting_to_follow", "{status}");
        assert!(status["waiting"].is_string(), "{status}");

        app.world_mut().entity_mut(second).despawn();
        app.update();
        assert!(following(&app), "one body again: following");
        assert_eq!(count(&log), (1, 1), "{:?}", events(&log));
    }

    /// A goal under way, by the body that began it (#1851).
    fn goal_on(aim: Aim, locomotion: &LocomotionConfig) -> Goal {
        Goal {
            id: 1,
            aim,
            run: false,
            drive: Drive::of(locomotion).expect("a known way of moving"),
            best_distance: 40.0,
            best_at: 0.0,
            set_out: 40.0,
            best_off: 5.0,
            stopping: Stopping::of(locomotion),
            flight: None,
            sortie: None,
            sweep: ground::lying_sweep(locomotion),
        }
    }

    fn peer_aim() -> Aim {
        Aim::Peer {
            did: "did:plc:friend".into(),
            keep: 3.0,
            closing: false,
            still_at: Vec2::ZERO,
            still_since: 0.0,
            blocked: true,
            gap: 10.0,
        }
    }

    /// **A movement takes up the body the agent wears now** (#1851): a new
    /// way of moving begins it again from where the body is, under the new
    /// drive, its clocks restarted; new numbers for the same way carry on
    /// with them; and what the new body cannot do ends `body_changed` - a
    /// follow in an airplane, a drive's keys on anything new.
    #[test]
    fn a_movement_takes_up_the_body_the_agent_wears_now() {
        let on_foot = person().locomotion;
        let rotor = stand_in(false).locomotion;
        let airplane = stand_in(true).locomotion;
        assert!(
            matches!(rotor, LocomotionConfig::Helicopter(_)),
            "the premise"
        );
        let at = Vec3::new(1.0, 0.0, 2.0);

        let mut goal = goal_on(peer_aim(), &on_foot);
        assert_eq!(take_up_body(&mut goal, &on_foot, at, None, 9.0), None);
        assert_eq!(goal.best_at, 0.0, "the same body: nothing begun again");

        assert_eq!(take_up_body(&mut goal, &rotor, at, None, 9.0), None);
        assert!(matches!(goal.drive, Drive::Rotor(_)));
        assert_eq!(goal.best_at, 9.0);
        assert!(matches!(
            goal.aim,
            Aim::Peer { closing: true, blocked: false, still_since, still_at, .. }
                if still_since == 9.0 && still_at == at.xz()
        ));

        let mut goal = goal_on(peer_aim(), &rotor);
        assert_eq!(
            take_up_body(&mut goal, &airplane, at, None, 9.0),
            Some(MoveOutcome::BodyChanged),
            "an airplane cannot follow"
        );

        let mut goal = goal_on(Aim::Point(Vec2::new(0.0, -40.0)), &on_foot);
        assert_eq!(take_up_body(&mut goal, &airplane, at, None, 9.0), None);
        assert!(
            matches!(goal.drive, Drive::Wing(_)),
            "an airplane flies a walk-to"
        );
        assert_eq!(goal.best_distance, at.xz().distance(Vec2::new(0.0, -40.0)));

        let mut goal = goal_on(
            Aim::Heading {
                dir: Vec2::X,
                bias_deg: 40.0,
                phase: TurnPhase::Settling { since: 1.0 },
            },
            &on_foot,
        );
        let car = LocomotionConfig::Car(Box::default());
        assert_eq!(take_up_body(&mut goal, &car, at, None, 9.0), None);
        assert!(matches!(
            goal.aim,
            Aim::Heading {
                bias_deg: 0.0,
                phase: TurnPhase::Moving { since: 9.0 },
                ..
            }
        ));

        // A car tuned in place: the same way of moving, its new braking.
        let mut slow = crate::pds::CarParams::default();
        slow.drive_force.0 *= 0.1;
        let slow = LocomotionConfig::Car(Box::new(slow));
        let mut goal = goal_on(Aim::Point(Vec2::new(0.0, -40.0)), &car);
        assert_eq!(take_up_body(&mut goal, &slow, at, None, 9.0), None);
        assert_eq!(goal.stopping, Stopping::of(&slow));
        assert_eq!(goal.best_at, 0.0, "carried straight on");

        let mut goal = goal_on(
            Aim::Keys(Box::new(keys::KeyRun::new(
                keys::segments(vec![DriveSegment {
                    keys: vec!["W".to_owned()],
                    secs: 2.0,
                }])
                .expect("a segment"),
                0.0,
                keys::airborne_above(&car).expect("a car drives"),
            ))),
            &car,
        );
        assert_eq!(
            take_up_body(&mut goal, &on_foot, at, None, 9.0),
            Some(MoveOutcome::BodyChanged),
            "a drive's keys were for the car"
        );
    }

    /// **A follow begun on foot escorts once the body flies** (#1851). Live
    /// 19: Kairos followed the owner on foot and wore an airship; the follow
    /// went on pressing a walker's W on the landed craft, which only Space
    /// lifts off, and said `follow_blocked` 1.5 minutes later. Here a person
    /// follows a player 40 m off, puts the airship on a second in, and is
    /// flying their escort, never blocked, within 20 s.
    #[test]
    fn a_follow_begun_on_foot_escorts_once_the_body_flies() {
        let walker = person();
        let mut daemon = Daemon::over(FlightBench::set_down(&walker, 0.0, 0.0));
        daemon.world().spawn((
            peer("did:plc:friend"),
            placed(true),
            GlobalTransform::from(Transform::from_xyz(0.0, 0.0, -40.0)),
        ));
        follow(daemon.world(), "did:plc:friend".into(), 3.0, false, false).expect("a follow");
        let from = daemon.bench.position();
        for _ in 0..30 {
            daemon.frame();
        }
        assert!(
            daemon.bench.position().distance(from) > 0.5,
            "the premise: it walks after them"
        );

        daemon.bench.wear(&stand_in(false));
        daemon.until(20.0, |daemon| daemon.height() > 5.0);

        let blocked = daemon
            .log
            .after(0, Duration::ZERO)
            .events
            .into_iter()
            .any(|e| matches!(e.what, EventKind::FollowBlocked { .. }));
        assert!(!blocked, "said to be blocked after the wear");
        assert!(daemon.endings().is_empty(), "{:?}", daemon.endings());
        assert!(
            describe(daemon.world())["phase"].is_string(),
            "escorting in the air"
        );
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

    impl Bench for crate::player::sim::WalkBench {
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

    /// The orbit camera's turn landing, as the camera plugin lands it at
    /// the end of a frame: the world camera's transform looks along the yaw
    /// the steering set, level - what a walker walks along. A bench with no
    /// camera has nothing to turn.
    fn land_the_camera_turn(world: &mut World) {
        let mut cameras =
            world.query_filtered::<(&PanOrbitCamera, &mut GlobalTransform), With<WorldCamera>>();
        for (orbit, mut at) in cameras.iter_mut(world) {
            let yaw = orbit.yaw.unwrap_or(orbit.target_yaw);
            *at = GlobalTransform::from(Transform::from_rotation(Quat::from_rotation_y(yaw)));
        }
    }

    /// A daemon frame, as the agent's loop runs them.
    const FRAME: f64 = 1.0 / 30.0;

    /// The stand-in airplane's box is no higher than this over the ground
    /// standing on its wheels (m): their reach, which its weight sags.
    fn on_its_wheels() -> f32 {
        crate::pds::AirplaneParams::default().gear_height.0
    }

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
        /// The airship stand-in set down on the floor at the origin.
        fn set_down() -> Self {
            Self::over(FlightBench::set_down(&stand_in(false), 0.0, 0.0))
        }

        fn wearing(record: &crate::pds::avatar::AvatarRecord, at: Vec3) -> Self {
            Self::over(FlightBench::new(record, at))
        }
    }

    impl Daemon<crate::player::sim::WalkBench> {
        /// A body on foot wearing `record`, standing on the floor at the
        /// origin facing -Z, with the orbit camera a walker is steered by.
        fn on_foot(record: &crate::pds::avatar::AvatarRecord) -> Self {
            let mut bench = crate::player::sim::WalkBench::standing(record, 0.0, 0.0);
            bench.world_mut().spawn((
                Camera3d::default(),
                WorldCamera,
                PanOrbitCamera::default(),
                GlobalTransform::default(),
            ));
            Self::over(bench)
        }
    }

    /// The bodies on foot a turn is held to: the default person, the
    /// default dog, cat and horse lying along their capsules (#1802), and
    /// Catoid's tabby as its record carries its locomotion
    /// (`exports/catoid/src/avatar.json`, pulled 2026-10-31, the body live
    /// session 17's `face` ended `stuck` on, #1820) - each with how far a
    /// `face` may carry it from where it stood (m), a fifth to a third over
    /// the most measured.
    fn on_foot_bodies() -> Vec<(&'static str, crate::pds::avatar::AvatarRecord, f32)> {
        use crate::pds::avatar::locomotion::beast_record;
        use symbios_avatar::plan::Family;
        let mut person = crate::pds::avatar::AvatarRecord::default_for_did("did:plc:agentface");
        person.locomotion = LocomotionConfig::Humanoid(Box::default());
        let mut tabby = beast_record(Family::Cat);
        let mut locomotion: LocomotionConfig = serde_json::from_value(serde_json::json!({
            "$type": "network.symbios.locomotion.humanoid",
            "acceleration": 120000,
            "capsule_length": 4727,
            "capsule_radius": 1731,
            "jump_impulse": 749706,
            "linear_damping": 3000,
            "mass": 133281,
            "shape": {
                "$type": "network.symbios.locomotion.humanoid#lying",
                "ahead": 26,
                "height": 5910
            },
            "stop_damping": 200000,
            "swim_speed": 25000,
            "swim_vertical_speed": 18000,
            "turn_rate": 120000,
            "wading_speed_factor": 5000,
            "walk_speed": 40009
        }))
        .expect("Catoid's locomotion reads");
        locomotion.sanitize();
        assert!(
            matches!(&locomotion, LocomotionConfig::Humanoid(p) if p.is_lying()),
            "the tabby lies along its capsule"
        );
        tabby.locomotion = locomotion;
        vec![
            ("the default person", person, 0.15),
            ("the default dog", beast_record(Family::Dog), 0.7),
            ("the default cat", beast_record(Family::Cat), 0.6),
            ("the default horse", beast_record(Family::Horse), 2.0),
            ("Catoid's tabby", tabby, 0.65),
        ]
    }

    /// The body's facing on the bench, as `steer` reads it.
    fn bench_forward(daemon: &Daemon<crate::player::sim::WalkBench>) -> Vec3 {
        daemon.bench.rotation() * Vec3::NEG_Z
    }

    /// **A beast faces the way asked, as a person does** (#1820). Live 17:
    /// `face` on Catoid's tabby ended `stuck` with `facing_off` 60 degrees.
    /// Each body on foot here turns to a point at each of these angles off
    /// its nose, either side, and ends `faced` within the tolerance a
    /// person is promised - and no further from where it stood than its
    /// bound in [`on_foot_bodies`], which `moving.md` ("Turning") quotes.
    /// A face step holds the walk ([`ground::face_pulse`]), up to about
    /// 1.4 s for the horse, so a turn walks a little: measured 2026-11-09,
    /// the person 0.05-0.11 m, the dog 0.18-0.55, the cat 0.15-0.45, the
    /// tabby 0.18-0.49 and the horse 0.38 m (45 degrees), 0.91 (90), 1.15
    /// (-120), 1.43 (150) and 1.71 (180). A bound that breaks after a
    /// physics or gait change: measure again, and move the doc with it.
    #[test]
    fn a_beast_faces_the_way_asked_as_a_person_does() {
        let mut failures = Vec::new();
        for (name, record, travel) in on_foot_bodies() {
            for degrees in [45.0_f32, 90.0, 150.0, 180.0, -60.0, -120.0] {
                let mut daemon = Daemon::on_foot(&record);
                let start = daemon.bench.position().xz();
                let way = ground::turned(Vec2::NEG_Y, degrees);
                face(daemon.world(), FaceTarget::Point(start + way * 30.0)).expect("a face");
                daemon.until(15.0, |daemon| !daemon.endings().is_empty());
                let (outcome, _) = daemon.endings()[0];
                let off = heading_off(bench_forward(&daemon), way);
                let moved = daemon.bench.position().xz().distance(start);
                if outcome != MoveOutcome::Faced || off.abs() > FACE_TOLERANCE_DEG || moved > travel
                {
                    failures.push(format!(
                        "{name}, {degrees} degrees: {outcome:?} {off:.1} off after {:.2} s, \
                         {moved:.2} m from where it stood (at most {travel:.2})",
                        daemon.now
                    ));
                }
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
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
            land_the_camera_turn(world);
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
        let mut daemon = Daemon::set_down();
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
        let mut daemon = Daemon::set_down();
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
        let mut daemon = Daemon::set_down();
        let friend = daemon
            .world()
            .spawn((
                peer("did:plc:friend"),
                placed(true),
                GlobalTransform::from(Transform::from_xyz(0.0, 0.0, -40.0)),
            ))
            .id();
        follow(daemon.world(), "did:plc:friend".into(), 3.0, false, false).expect("a follow");
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

    /// An airplane with nothing flying it stays put on its wheels: the
    /// daemon holds its throttle closed and its brakes on.
    #[test]
    fn an_airplane_with_nothing_to_do_stays_on_the_ground() {
        let mut daemon = Daemon::wearing(&stand_in(true), Vec3::new(0.0, 0.35, 0.0));
        let from = daemon.bench.position();

        for _ in 0..(5 * 30) {
            daemon.frame();
        }

        assert!(
            daemon.height() < on_its_wheels() && daemon.bench.position().distance(from) < 0.5,
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

        let followed = follow(daemon.world(), "did:plc:friend".into(), 3.0, false, false);
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
        // And down, it stays down: the parking holds its throttle closed.
        for _ in 0..(3 * 30) {
            daemon.frame();
        }
        assert!(
            daemon.height() < on_its_wheels(),
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
        for (name, record) in swinging_cars() {
            let mut daemon = Daemon::over(DriveBench::parked(&record, 0.0, 0.0));
            let target = Vec2::new(0.0, -40.0);
            walk_to(daemon.world(), target, false).expect("a walk-to");

            daemon.until(40.0, |daemon| !daemon.endings().is_empty());
            assert_eq!(daemon.endings()[0].0, MoveOutcome::Arrived, "{name}");
            for _ in 0..60 {
                daemon.frame();
            }

            let off = daemon.bench.position().xz().distance(target);
            assert!(
                off <= crate::config::agent::ARRIVE_WHEELED_M,
                "two seconds after it arrived {name} is {off:.2} m from its point"
            );
        }
    }

    /// The cars #1541 was found on: the default car, and the seeded wagon
    /// and armoured car, which swing round at about its pace.
    fn swinging_cars() -> Vec<(&'static str, crate::pds::avatar::AvatarRecord)> {
        use crate::seeded_defaults::{AvatarPins, CraftType, SkiffType};
        let mut default = crate::pds::avatar::AvatarRecord::default_for_did("did:plc:agentwalkcar");
        default.locomotion = LocomotionConfig::Car(Box::default());
        let seeded = |skiff| {
            let mut pins = AvatarPins::default();
            pins.lock_craft(Some(CraftType::Skiff(skiff)));
            crate::pds::avatar::AvatarRecord::default_for_seed(
                pins.find_seed(0).expect("the skiff is reachable"),
            )
        };
        vec![
            ("the default car", default),
            ("the wagon", seeded(SkiffType::Wagon)),
            ("the armoured car", seeded(SkiffType::ArmouredCar)),
        ]
    }

    /// Kairos's Hexapod and Roadfactory (#1866), as their items carry their
    /// locomotion (`exports/kairos/gen/fleet_hex_loco.json` and
    /// `fleet_road_loco.json`, built by `b/fleet.py` from the Catalogue
    /// rover on 2026-11-05 and worn in live sessions 20 and 21): the car
    /// preset on a 2.9 m by 3.1 m box, the Hexapod at half the
    /// Roadfactory's drive. Their visuals do not move them, so the default
    /// body stands in for the factory's.
    fn kairos_cars() -> Vec<(&'static str, crate::pds::avatar::AvatarRecord)> {
        let car = |drive_force: u64| {
            let mut locomotion: LocomotionConfig = serde_json::from_value(serde_json::json!({
                "$type": "network.symbios.locomotion.car",
                "air_angular_damping": 10000,
                "air_control_accel": 40000,
                "air_level_accel": 60000,
                "air_linear_damping": 500,
                "angular_damping": 30000,
                "center_of_mass_drop": 6000,
                "chassis_half_extents": [14500, 1316, 15500],
                "drive_force": drive_force,
                "handbrake_grip_factor": 2500,
                "lateral_grip": 131448720,
                "linear_damping": 9000,
                "mass": 5915192,
                "suspension_damping": 8938513,
                "suspension_rest_length": 6000,
                "suspension_stiffness": 110416920,
                "turn_torque": 21259723,
                "upright_assist_accel": 25000,
                "upright_assist_damping": 8000,
                "upright_engage_tilt_degrees": 600000
            }))
            .expect("Kairos's locomotion reads");
            locomotion.sanitize();
            assert!(
                matches!(locomotion, LocomotionConfig::Car(_)),
                "a car after the sanitiser, as when it was built"
            );
            let mut record =
                crate::pds::avatar::AvatarRecord::default_for_did("did:plc:agentkairoscar");
            record.locomotion = locomotion;
            record
        };
        vec![
            ("Kairos Hexapod", car(26618364)),
            ("Kairos Roadfactory", car(53236728)),
        ]
    }

    /// How far each movement that has ended still had to go.
    fn distances_left<B: Bench>(daemon: &Daemon<B>) -> Vec<f64> {
        daemon
            .log
            .after(0, Duration::ZERO)
            .events
            .into_iter()
            .filter_map(|e| match e.what {
                EventKind::MovementEnded {
                    distance_left_m, ..
                } => Some(distance_left_m),
                _ => None,
            })
            .collect()
    }

    /// A car's short walk-to drives (#1866). Live 21: Kairos's Hexapod and
    /// Roadfactory, at rest 2.97-2.99 m from the teleporter's deck, each
    /// ended a walk-to there `arrived` at once, with that much left and not
    /// a key pressed - a car's arrival circle is 3 m, and they began inside
    /// it. A car at rest can close in; a walk-to now asks it to come within
    /// half the way it set out from (never nearer than a walker's metre),
    /// ahead of it or beside it.
    #[test]
    fn a_cars_short_walk_to_drives_it_closer() {
        let from = 2.98;
        for (name, record) in kairos_cars().into_iter().chain(swinging_cars()) {
            for (side, target) in [
                ("ahead", Vec2::new(0.0, -from)),
                ("beside", Vec2::new(from, 0.0)),
            ] {
                let mut daemon = Daemon::over(DriveBench::parked(&record, 0.0, 0.0));
                let start = daemon.bench.position().xz();
                let target = start + target;
                walk_to(daemon.world(), target, false).expect("a walk-to");

                daemon.until(30.0, |daemon| !daemon.endings().is_empty());
                let off = daemon.bench.position().xz().distance(target);
                assert_eq!(
                    daemon.endings()[0].0,
                    MoveOutcome::Arrived,
                    "{name}'s walk-to of a point {from} m {side}: {off:.2} m left"
                );
                assert!(
                    off <= from * 0.5,
                    "{name} arrived {off:.2} m from a point {from} m {side} of where it stood"
                );
                for _ in 0..60 {
                    daemon.frame();
                }
                let off = daemon.bench.position().xz().distance(target);
                assert!(
                    off <= from * 0.5,
                    "two seconds after it arrived {name} is {off:.2} m from its point {side}"
                );
            }
        }
    }

    /// And a car that cannot close in says so (#1866): nosed against a wall
    /// with its point 2.98 m off beyond it, a car used to end its walk-to
    /// `arrived` at once, as live 21's did on open ground, without trying;
    /// it ends `stuck`, with the metres it has left, once it has gone
    /// [`STUCK_AFTER_SECS`] no nearer.
    #[test]
    fn a_car_walled_in_short_of_its_point_ends_stuck() {
        let from = 2.98;
        for (name, record) in kairos_cars() {
            let LocomotionConfig::Car(params) = &record.locomotion else {
                unreachable!("a car");
            };
            let nose = params.chassis_half_extents.0[2];
            let mut bench = DriveBench::parked(&record, 0.0, 0.0);
            let start = bench.position().xz();
            // A wall 0.15 m off the nose, 0.5 m thick: the point is beyond it.
            bench.block(
                Transform::from_xyz(start.x, 1.0, start.y - nose - 0.4),
                Vec3::new(6.0, 1.0, 0.25),
            );
            let mut daemon = Daemon::over(bench);
            let target = start + Vec2::new(0.0, -from);
            walk_to(daemon.world(), target, false).expect("a walk-to");

            daemon.until(30.0, |daemon| !daemon.endings().is_empty());
            assert_eq!(daemon.endings()[0].0, MoveOutcome::Stuck, "{name}");
            // It rears up against the wall, its front corners' rays on the
            // wall's top, and leans there: its middle comes a metre nearer
            // and no further, short of the circle it was asked to reach.
            let left = distances_left(&daemon)[0];
            assert!(
                left > f64::from(from) * 0.5,
                "{name} ended stuck with {left} m left, the wall between"
            );
        }
    }

    /// A car's walk-to of a point behind it arrives (#1541). It swings round
    /// on the spot first - the default car at about 25 degrees a second -
    /// and with progress counted by distance alone the walk ended stuck
    /// after 6 s, still some 24 degrees short of facing it. Turning nearer
    /// its way counts too. Each
    /// car here goes longer than that without getting half a metre closer,
    /// or the test would not show it.
    #[test]
    fn a_cars_walk_to_a_point_behind_it_arrives() {
        for (name, record) in swinging_cars() {
            let mut daemon = Daemon::over(DriveBench::parked(&record, 0.0, 0.0));
            let target = Vec2::new(0.0, 40.0);
            let from = daemon.bench.position().xz().distance(target);
            walk_to(daemon.world(), target, false).expect("a walk-to");
            let mut closer_at = None;
            daemon.until(60.0, |daemon| {
                let distance = daemon.bench.position().xz().distance(target);
                if closer_at.is_none() && distance <= from - crate::config::agent::PROGRESS_STEP_M {
                    closer_at = Some(daemon.now);
                }
                !daemon.endings().is_empty()
            });
            assert_eq!(
                daemon.endings()[0].0,
                MoveOutcome::Arrived,
                "{name}'s walk-to of a point behind it"
            );
            assert!(
                closer_at.is_some_and(|t| t > STUCK_AFTER_SECS),
                "the premise: {name} swings round longer than a walk may go without getting \
                 closer, but got closer at {closer_at:?} s"
            );
        }
    }

    /// And a car faces a point behind it (#1541): a turn on wheels had
    /// [`STUCK_AFTER_SECS`] from its start, and a car swinging round at its
    /// own pace ended stuck before it faced the point; it has that long from
    /// its last real turn now. Each car here first faces it after that long,
    /// or the test would not show it.
    #[test]
    fn a_car_faces_a_point_behind_it() {
        for (name, record) in swinging_cars() {
            let mut daemon = Daemon::over(DriveBench::parked(&record, 0.0, 0.0));
            let point = Vec2::new(0.0, 40.0);
            face(daemon.world(), FaceTarget::Point(point)).expect("a face");
            let mut faced_at = None;
            daemon.until(60.0, |daemon| {
                let forward = daemon.bench.rotation() * Vec3::NEG_Z;
                let way = point - daemon.bench.position().xz();
                if faced_at.is_none() && heading_off(forward, way).abs() <= FACE_TOLERANCE_DEG {
                    faced_at = Some(daemon.now);
                }
                !daemon.endings().is_empty()
            });
            assert_eq!(
                daemon.endings()[0].0,
                MoveOutcome::Faced,
                "{name}'s face of a point behind it"
            );
            assert!(
                faced_at.is_some_and(|t| t > STUCK_AFTER_SECS),
                "the premise: {name} first faced the point at {faced_at:?} s, within the old budget"
            );
        }
    }

    /// A car following a player behind it is not blocked while it swings
    /// round to them (#1541's follow): counted by its moving alone, each car
    /// here was said to be blocked at 6.07 s, still turning, before it had
    /// gone half a metre - so the first half metre must come after that
    /// long, or the test would not show it.
    #[test]
    fn a_car_following_a_player_behind_it_is_not_blocked_turning_round() {
        for (name, record) in swinging_cars() {
            let mut daemon = Daemon::over(DriveBench::parked(&record, 0.0, 0.0));
            let start = daemon.bench.position().xz();
            let player = Vec3::new(0.0, 0.0, 40.0);
            daemon.world().spawn((
                peer("did:plc:friend"),
                placed(true),
                GlobalTransform::from(Transform::from_translation(player)),
            ));
            follow(daemon.world(), "did:plc:friend".into(), 3.0, false, false).expect("a follow");
            let mut moved_at = None;
            for _ in 0..(15.0 / FRAME) as usize {
                daemon.frame();
                if moved_at.is_none()
                    && daemon.bench.position().xz().distance(start)
                        >= crate::config::agent::PROGRESS_STEP_M
                {
                    moved_at = Some(daemon.now);
                }
            }
            let blocked = daemon
                .log
                .after(0, Duration::ZERO)
                .events
                .into_iter()
                .any(|e| matches!(e.what, EventKind::FollowBlocked { .. }));
            assert!(
                !blocked,
                "{name} was said to be blocked following a player behind it"
            );
            assert!(
                moved_at.is_some_and(|t| t > STUCK_AFTER_SECS),
                "the premise: {name} had gone half a metre at {moved_at:?} s"
            );
        }
    }

    /// A car following a player brakes to a stand short of them (#1536): it
    /// used to let go at its following distance at full speed and drive on
    /// through the player - from 40 m, 10.85 m/s, 0.01 m from where they
    /// stood, at rest 6.78 m beyond (the session's end review). And a car
    /// that brakes and slows less does too (#1570): planned on 4 m/s^2 and
    /// no coast, the wagon - reversing at 1.9 and coasting 2.2 m once the
    /// brake let go - came to rest 0.35 m from a player ahead of it and 0.98
    /// m from one behind, and the armoured car 1.4 m from either.
    #[test]
    fn a_car_following_a_player_stops_short_of_them() {
        let keep = 3.0;
        for (name, record) in swinging_cars() {
            for (side, z) in [("ahead", -40.0), ("behind", 40.0)] {
                let mut daemon = Daemon::over(DriveBench::parked(&record, 0.0, 0.0));
                let player = Vec3::new(0.0, 0.0, z);
                daemon.world().spawn((
                    peer("did:plc:friend"),
                    placed(true),
                    GlobalTransform::from(Transform::from_translation(player)),
                ));
                follow(daemon.world(), "did:plc:friend".into(), keep, false, false)
                    .expect("a follow");

                let mut nearest = f32::MAX;
                for _ in 0..(20.0 / FRAME) as usize {
                    daemon.frame();
                    nearest = nearest.min(daemon.bench.position().xz().distance(player.xz()));
                }

                assert!(
                    nearest >= keep * 0.5,
                    "{name} came within {nearest:.2} m of a player {side} it follows at {keep} m"
                );
                let resting = daemon.bench.position().xz().distance(player.xz());
                assert!(
                    resting <= keep + crate::config::agent::FOLLOW_SLACK_M + 1.0,
                    "{name} stood {resting:.2} m off a player {side}, following at {keep} m"
                );
            }
        }
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
