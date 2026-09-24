//! Flying a rotorcraft (#1430). Every seeded airship drives as the helicopter
//! preset, a quarter of all seeded bodies.
//!
//! The preset holds no height of its own. Its hover thrust cancels its
//! weight and nothing more, and with no key held nothing brakes a climb or a
//! fall but the body's linear damping: let go at full climb and an airship
//! rises another six metres (measured, 6.2 m from 5 m/s). Space and Shift
//! each ask for a vertical speed, up or down at the record's
//! `vertical_speed`, so a flight holds its height the way a player does: it
//! asks for whichever closes on the height it wants, and lets go inside a
//! band. W and S push along the nose, Q and E beside it, and A and D turn it
//! by a torque - which turns it at rest, as a car's steering does, and
//! carries it on after the key is let go (7.4 degrees from full rate,
//! measured), so a turn lets go early by the turn its spin still has in it.
//!
//! A flight to a point ([`fly_to`]) takes off, climbing and swinging its nose
//! round to the point before it moves; cruises [`CRUISE_HEIGHT_M`] above the
//! highest ground on the next stretch of its course, and climbs over
//! whatever it finds standing in its way; stops over the point; and comes
//! down on it. It never ends in the air (owner, 2026-09-23): a flight that
//! arrives has landed, on whatever is under the point - the ground, a roof,
//! or water, which it stops on rather than sinks into.
//!
//! A follow ([`escort`]) flies [`ESCORT_HEIGHT_M`] up after the player,
//! keeping its distance, and lands beside them once they have stood still a
//! while; it takes off again when they move away. A turn ([`turn`]) swings
//! round on the spot at whatever height the body is (owner, 2026-09-24).
//!
//! A flight is `stuck` when the body gets nowhere - neither moves nor turns -
//! for as long as a walk may, and never by how far it still has to go: a
//! take-off, a swing and a landing close no distance at all.

use bevy::prelude::*;

use crate::config::agent::{
    ALTITUDE_GAIN, APPROACH_BRAKE_SHARE, APPROACH_STOP_SHORT_M, ARRIVE_FLYING_M, CLIMB_BAND,
    CRUISE_CLEARANCE_M, CRUISE_HEIGHT_M, CRUISE_LOOKAHEAD_M, CRUISE_SAMPLE_M, ESCORT_HEIGHT_M,
    ESCORT_ROOM_M, FACE_SETTLE_SECS, FACE_TOLERANCE_DEG, FOLLOW_SLACK_M, LAND_BESIDE_AFTER_SECS,
    LAND_OVER_SPEED, LAND_SINK_M, LAND_SLIDE_GAIN, LAND_SLIDE_SPEED, NOSE_HOLD_WITHIN_M,
    OBSTACLE_MARGIN_M, OBSTACLE_SWEEP_M, PLAYER_STILL_M, PROGRESS_STEP_M, PROGRESS_TURN_DEG,
    STUCK_AFTER_SECS, SURFACE_M, SWING_FIRST_DEG, TAKE_OFF_WITHIN_M, THRUST_BAND,
    TOUCHDOWN_HEIGHT_M, TOUCHDOWN_LEAN_DEG, TOUCHDOWN_SECS, TOUCHDOWN_STILL_M, WATER_COAST_FROM_M,
    WATER_REST_M, YAW_DEAD_ZONE_DEG,
};
use crate::pds::HelicopterParams;

use super::{TurnPhase, heading_off};

/// A rotorcraft's handling, from its record: what a flight has to know to
/// brake, turn and climb in time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Rotor {
    /// What W or S does to its speed along the nose (m/s^2): the cyclic
    /// force over the mass.
    cyclic_accel: f32,
    /// The climb and the descent Space and Shift ask for (m/s).
    vertical_speed: f32,
    /// How fast its speed dies with no key held (1/s): let go, it coasts
    /// speed / damping further.
    linear_damping: f32,
    /// How fast a spin dies once its key is let go (1/s).
    angular_damping: f32,
    /// How far the body reaches out from its middle over the ground (m):
    /// the corner of its chassis.
    reach: f32,
}

impl Rotor {
    pub(super) fn of(params: &HelicopterParams) -> Self {
        let [half_width, _, half_length] = params.chassis_half_extents.0;
        Self {
            cyclic_accel: params.cyclic_force.0 / params.mass.0.max(0.1),
            vertical_speed: params.vertical_speed.0,
            linear_damping: params.linear_damping.0,
            angular_damping: params.angular_damping.0,
            reach: half_width.hypot(half_length),
        }
    }
}

/// What a flight reads of the body, each frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Craft {
    pub(super) position: Vec3,
    /// Where the chassis points - tilted a little under thrust.
    pub(super) forward: Vec3,
    /// The chassis's up, which its stabiliser holds to the world's.
    pub(super) up: Vec3,
    pub(super) velocity: Vec3,
    /// How fast it turns, in radians a second anticlockwise seen from above
    /// (the body's angular velocity about +Y).
    pub(super) yaw_rate: f32,
    /// Its whole angular velocity (rad/s).
    pub(super) spin: Vec3,
    /// The height of the lowest point of its collider.
    pub(super) underside: f32,
    /// Where its underside would be once it had come straight down onto
    /// what is below it: the ground, anything that stands on it, or water.
    pub(super) below: f32,
    /// What is below it is water, which holds nothing up.
    pub(super) on_water: bool,
}

impl Craft {
    /// How high its underside is above what is below it (m); negative when
    /// it is in water.
    pub(super) fn height(&self) -> f32 {
        self.underside - self.below
    }

    /// How fast it turns about `axis` (rad/s, right-handed).
    pub(super) fn pitch_rate(&self, axis: Vec3) -> f32 {
        self.spin.dot(axis)
    }

    /// Whether it is down on what is below it - or near enough that it
    /// needs no landing.
    pub(super) fn down(&self) -> bool {
        self.height() <= TOUCHDOWN_HEIGHT_M
    }
}

/// The world a flight finds its way through.
pub(super) trait Terrain {
    /// The ground's height at a point on the map, water counted as ground:
    /// a body that goes into water has not landed.
    fn surface_at(&self, xz: Vec2) -> f32;
    /// How far the body can go along `toward` (on the ground, unit) before
    /// it touches something - up to `reach` - swept `lift` metres higher
    /// than it is: a body on the ground sweeps just clear of it, since the
    /// ground it rests on would stop a sweep at once.
    fn clear_along(&self, toward: Vec2, reach: f32, lift: f32) -> f32;
}

/// Where a flight is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Phase {
    /// Climbing to its cruising height, its nose swinging round to the
    /// point, before it moves.
    TakingOff,
    /// On its way: cruising to a point, or escorting a player.
    Cruising,
    /// Coming down over a spot - and at rest since a time, where it was
    /// then, once it is.
    Landing {
        spot: Vec2,
        resting: Option<(f64, Vec3)>,
    },
    /// Down beside the player it follows, waiting for them to move.
    Landed,
}

impl Phase {
    /// The phase as `status` names it.
    pub(super) fn word(self) -> &'static str {
        match self {
            Self::TakingOff => "taking_off",
            Self::Cruising => "cruising",
            Self::Landing { .. } => "landing",
            Self::Landed => "landed",
        }
    }
}

/// Where the body last got anywhere - moved or turned - and when: what
/// `stuck` is measured by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Stillness {
    at: Vec3,
    heading: Vec2,
    since: f64,
}

impl Stillness {
    pub(super) fn new(craft: &Craft, now: f64) -> Self {
        Self {
            at: craft.position,
            heading: flat(craft.forward),
            since: now,
        }
    }

    /// Note where the body is now: whether it has got nowhere for as long
    /// as a walk may.
    pub(super) fn nowhere_for_too_long(&mut self, craft: &Craft, now: f64) -> bool {
        let turned = self
            .heading
            .angle_to(flat(craft.forward))
            .abs()
            .to_degrees();
        if craft.position.distance(self.at) >= PROGRESS_STEP_M || turned >= PROGRESS_TURN_DEG {
            *self = Self::new(craft, now);
            return false;
        }
        now - self.since >= STUCK_AFTER_SECS
    }
}

/// Something found standing in the way: the height to hold the underside
/// at until the body is past it, and where it was last found in the way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Over {
    pub(super) level: f32,
    pub(super) at: Vec2,
}

/// A flight under way.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Flight {
    pub(super) phase: Phase,
    still: Stillness,
    /// Following: where the player has stood since when.
    player_still: Option<(Vec2, f64)>,
    /// Climbing, or gone up, over something in the way.
    over: Option<Over>,
}

impl Flight {
    pub(super) fn new(craft: &Craft, now: f64) -> Self {
        Self {
            phase: Phase::TakingOff,
            still: Stillness::new(craft, now),
            player_still: None,
            over: None,
        }
    }

    /// A flight coming straight down where the body is.
    pub(super) fn landing(craft: &Craft, now: f64) -> Self {
        Self {
            phase: Phase::Landing {
                spot: craft.position.xz(),
                resting: None,
            },
            ..Self::new(craft, now)
        }
    }
}

/// What a flight to a point does this frame.
#[derive(Debug, PartialEq)]
pub(super) enum Flown {
    /// Hold these keys.
    Hold(Vec<KeyCode>),
    /// Down on the point, at rest.
    Landed,
    /// Got nowhere for too long.
    Stuck,
}

/// Fly `craft` on toward `target` (x, z), and down onto it.
pub(super) fn fly_to(
    rotor: &Rotor,
    flight: &mut Flight,
    craft: &Craft,
    terrain: &dyn Terrain,
    target: Vec2,
    now: f64,
) -> Flown {
    let offset = target - craft.position.xz();
    let distance = offset.length();
    let over_the_point = distance <= ARRIVE_FLYING_M;
    let speed = craft.velocity.xz().length();
    let land = match flight.phase {
        Phase::TakingOff => over_the_point,
        Phase::Cruising => over_the_point && speed <= LAND_OVER_SPEED,
        Phase::Landing { .. } | Phase::Landed => false,
    };
    if land {
        flight.phase = Phase::Landing {
            spot: target,
            resting: None,
        };
    }
    let keys = match &mut flight.phase {
        Phase::Landing { spot, resting } => match come_down(rotor, craft, *spot, resting, now) {
            Some(keys) => keys,
            None => return Flown::Landed,
        },
        Phase::Landed => return Flown::Landed,
        Phase::TakingOff => {
            let level = level(terrain, craft, target, CRUISE_HEIGHT_M);
            if craft.underside >= level - TAKE_OFF_WITHIN_M {
                flight.phase = Phase::Cruising;
            }
            let mut keys: Vec<KeyCode> = climb_key(
                craft.velocity.y,
                climb_speed(level - craft.underside, rotor),
            )
            .into_iter()
            .collect();
            if distance > NOSE_HOLD_WITHIN_M {
                let off = heading_off(craft.forward, offset / distance);
                keys.extend(yaw_key(off, craft.yaw_rate, rotor));
            }
            // Holding still over where it rose from until it is up.
            thrust_keys(craft.forward, craft.velocity.xz(), Vec2::ZERO, &mut keys);
            keys
        }
        Phase::Cruising => {
            let course = offset / distance.max(f32::EPSILON);
            fly_on(
                rotor,
                craft,
                terrain,
                &mut flight.over,
                course,
                course * approach_speed(distance, rotor),
                level(terrain, craft, target, CRUISE_HEIGHT_M),
                distance > NOSE_HOLD_WITHIN_M,
            )
        }
    };
    if flight.still.nowhere_for_too_long(craft, now) {
        return Flown::Stuck;
    }
    Flown::Hold(keys)
}

/// What a follow does this frame.
#[derive(Debug, PartialEq)]
pub(super) struct Escort {
    pub(super) keys: Vec<KeyCode>,
    /// On its way after the player, rather than beside them.
    pub(super) closing: bool,
    /// On its way after the player, and got nowhere for as long as a walk
    /// may.
    pub(super) blocked: bool,
}

/// Escort the player at `player` (x, z), keeping `keep` metres off - more,
/// if the body is too big to keep that close: fly after them
/// [`ESCORT_HEIGHT_M`] up, land beside them once they have stood still for
/// [`LAND_BESIDE_AFTER_SECS`], and take off again once they are further
/// than the distance by [`FOLLOW_SLACK_M`] (owner, 2026-09-24).
pub(super) fn escort(
    rotor: &Rotor,
    flight: &mut Flight,
    craft: &Craft,
    terrain: &dyn Terrain,
    player: Vec2,
    keep: f32,
    now: f64,
) -> Escort {
    let keep = keep.max(rotor.reach + ESCORT_ROOM_M);
    let offset = player - craft.position.xz();
    let gap = offset.length();
    let stood = match flight.player_still {
        Some((at, since)) if at.distance(player) <= PLAYER_STILL_M => {
            now - since >= LAND_BESIDE_AFTER_SECS
        }
        _ => {
            flight.player_still = Some((player, now));
            false
        }
    };
    let near = gap <= keep + FOLLOW_SLACK_M;
    match flight.phase {
        Phase::Landing { .. } | Phase::Landed if !near => flight.phase = Phase::Cruising,
        Phase::TakingOff | Phase::Cruising if near && stood => {
            flight.phase = Phase::Landing {
                spot: craft.position.xz(),
                resting: None,
            };
        }
        _ => {}
    }
    let mut closing = false;
    let mut landed = false;
    let keys = match &mut flight.phase {
        Phase::Landed => Vec::new(),
        Phase::Landing { spot, resting } => come_down(rotor, craft, *spot, resting, now)
            .unwrap_or_else(|| {
                landed = true;
                Vec::new()
            }),
        Phase::TakingOff | Phase::Cruising => {
            closing = gap > keep;
            let course = offset / gap.max(f32::EPSILON);
            let want = if closing {
                course * approach_speed(gap - keep, rotor)
            } else {
                Vec2::ZERO
            };
            fly_on(
                rotor,
                craft,
                terrain,
                &mut flight.over,
                course,
                want,
                level(terrain, craft, player, ESCORT_HEIGHT_M),
                gap > ARRIVE_FLYING_M,
            )
        }
    };
    if landed {
        flight.phase = Phase::Landed;
    }
    // A blockage is measured only while it tries to close: waiting beside
    // them, it gets nowhere on purpose.
    if !closing {
        flight.still = Stillness::new(craft, now);
    }
    let blocked = closing && flight.still.nowhere_for_too_long(craft, now);
    Escort {
        keys,
        closing,
        blocked,
    }
}

/// What a turn does this frame.
#[derive(Debug, PartialEq)]
pub(super) enum Turned {
    Hold(Vec<KeyCode>),
    /// Facing the way asked, its spin settled.
    Faced,
    /// Got nowhere for too long.
    Stuck,
}

/// Swing the nose round to `dir` on the spot, keeping the body's height and
/// place - on the ground if it is down, in the air if not - and judge the
/// turn only once the spin has settled.
pub(super) fn turn(
    rotor: &Rotor,
    flight: &mut Flight,
    craft: &Craft,
    dir: Vec2,
    phase: &mut TurnPhase,
    now: f64,
) -> Turned {
    let off = heading_off(craft.forward, dir);
    let mut keys = hold(craft);
    match *phase {
        TurnPhase::Moving { .. } => match yaw_key(off, craft.yaw_rate, rotor) {
            Some(key) => keys.push(key),
            None => *phase = TurnPhase::Settling { since: now },
        },
        TurnPhase::Settling { since } if now - since < FACE_SETTLE_SECS => {}
        TurnPhase::Settling { .. } if off.abs() <= FACE_TOLERANCE_DEG => return Turned::Faced,
        TurnPhase::Settling { .. } => *phase = TurnPhase::Moving { since: now },
    }
    if flight.still.nowhere_for_too_long(craft, now) {
        return Turned::Stuck;
    }
    Turned::Hold(keys)
}

/// Staying where it is: its height, and its place over the ground. On the
/// ground that is no key at all.
pub(super) fn hold(craft: &Craft) -> Vec<KeyCode> {
    let mut keys: Vec<KeyCode> = climb_key(craft.velocity.y, 0.0).into_iter().collect();
    thrust_keys(craft.forward, craft.velocity.xz(), Vec2::ZERO, &mut keys);
    keys
}

/// One frame on the way along `course`: its underside held at `level`,
/// asking for `want` over the ground, its nose turned to the course while
/// `turn_nose`. It stops to swing round to a course well off its nose, as a
/// car does; to climb while it is well under its height with something
/// close below - ground rising faster than it climbs, a roof; and to climb
/// over whatever stands in its way at its height, which the ground's height
/// does not show - holding the height that cleared it until it is past it,
/// or the ordinary height would bring it down into it again, a little
/// closer each time, until it scraped up its face.
#[allow(clippy::too_many_arguments)]
fn fly_on(
    rotor: &Rotor,
    craft: &Craft,
    terrain: &dyn Terrain,
    over: &mut Option<Over>,
    course: Vec2,
    mut want: Vec2,
    level: f32,
    turn_nose: bool,
) -> Vec<KeyCode> {
    let here = craft.position.xz();
    if over.is_some_and(|over| here.distance(over.at) > OBSTACLE_SWEEP_M) {
        *over = None;
    }
    let level = over.map_or(level, |over| level.max(over.level));
    let mut keys = Vec::new();
    let mut want_vy = climb_speed(level - craft.underside, rotor);
    if turn_nose {
        let off = heading_off(craft.forward, course);
        keys.extend(yaw_key(off, craft.yaw_rate, rotor));
        if off.abs() > SWING_FIRST_DEG {
            want = Vec2::ZERO;
        }
    }
    if craft.underside < level - TAKE_OFF_WITHIN_M && craft.height() < CRUISE_CLEARANCE_M {
        want = Vec2::ZERO;
    }
    if want != Vec2::ZERO && in_the_way(rotor, craft, terrain, course) {
        want = Vec2::ZERO;
        want_vy = rotor.vertical_speed;
        *over = Some(Over {
            level: craft.underside + CRUISE_CLEARANCE_M,
            at: here,
        });
    }
    keys.extend(climb_key(craft.velocity.y, want_vy));
    thrust_keys(craft.forward, craft.velocity.xz(), want, &mut keys);
    keys
}

/// Whether something stands along `course` nearer than the body could stop
/// in, with [`OBSTACLE_MARGIN_M`] to spare.
fn in_the_way(rotor: &Rotor, craft: &Craft, terrain: &dyn Terrain, course: Vec2) -> bool {
    let speed = craft.velocity.xz().dot(course).max(0.0);
    let braking = (rotor.cyclic_accel * APPROACH_BRAKE_SHARE).max(0.1);
    let stopping = (speed * speed / (2.0 * braking) + OBSTACLE_MARGIN_M).min(OBSTACLE_SWEEP_M);
    terrain.clear_along(course, OBSTACLE_SWEEP_M, 0.0) < stopping
}

/// One frame of coming down over `spot`: the keys, or `None` once the body
/// is down on what is below it and at rest there.
fn come_down(
    rotor: &Rotor,
    craft: &Craft,
    spot: Vec2,
    resting: &mut Option<(f64, Vec3)>,
    now: f64,
) -> Option<Vec<KeyCode>> {
    let offset = spot - craft.position.xz();
    let over_the_spot = offset.length() <= ARRIVE_FLYING_M;
    let vy = craft.velocity.y;
    let height = craft.height();
    // Drifted off the spot: stop coming down until back over it.
    let key = if over_the_spot {
        landing_key(height, vy, craft.on_water, rotor)
    } else {
        climb_key(vy, 0.0)
    };
    // Down: at rest on the surface below, upright - judged by the body
    // staying put rather than by its speed, which a contact jolts, and not
    // while its stabiliser is still standing it up after a hillside tipped
    // it.
    let upright = craft.up.y >= TOUCHDOWN_LEAN_DEG.to_radians().cos();
    let could_be_down = over_the_spot && upright && craft.down();
    match *resting {
        Some((since, at)) if could_be_down && craft.position.distance(at) <= TOUCHDOWN_STILL_M => {
            if now - since >= TOUCHDOWN_SECS {
                return None;
            }
        }
        _ if could_be_down => *resting = Some((now, craft.position)),
        _ => *resting = None,
    }
    let mut keys: Vec<KeyCode> = key.into_iter().collect();
    if height > SURFACE_M {
        let slide = (offset * LAND_SLIDE_GAIN).clamp_length_max(LAND_SLIDE_SPEED);
        thrust_keys(craft.forward, craft.velocity.xz(), slide, &mut keys);
    }
    Some(keys)
}

/// The height a flight holds its underside at: `above` the highest ground
/// from here to [`CRUISE_LOOKAHEAD_M`] toward `toward` (or to it, if that is
/// nearer), and never less than [`CRUISE_CLEARANCE_M`] over whatever is
/// straight below.
pub(super) fn level(terrain: &dyn Terrain, craft: &Craft, toward: Vec2, above: f32) -> f32 {
    let from = craft.position.xz();
    let offset = toward - from;
    let reach = offset.length().min(CRUISE_LOOKAHEAD_M);
    let course = offset.normalize_or_zero();
    let samples = (reach / CRUISE_SAMPLE_M).ceil() as usize;
    let highest = (0..=samples)
        .map(|i| terrain.surface_at(from + course * (i as f32 * CRUISE_SAMPLE_M).min(reach)))
        .fold(f32::NEG_INFINITY, f32::max);
    (highest + above).max(craft.below + CRUISE_CLEARANCE_M)
}

/// The vertical speed that closes on a height `error` metres above the body
/// (below it, when negative).
fn climb_speed(error: f32, rotor: &Rotor) -> f32 {
    (error * ALTITUDE_GAIN).clamp(-rotor.vertical_speed, rotor.vertical_speed)
}

/// Coming down onto what is `height` metres below the underside, going up
/// at `vy`. Let go, a body coasts `vy / damping` further before it stops, and
/// the keys can only move where that is by steps: from rest a frame of Shift
/// moves it by a metre, but near its top speed down by a sixth of one. So it
/// comes down at full speed and lets go as that resting place reaches the
/// surface - for the ground, a little under it, so it gets there, slowly;
/// for water, which holds nothing up, at it, and too low over water to get
/// up to speed first, it climbs. Once on the ground it lets go: Shift asks
/// for a full-speed descent, so held there it only presses the body into
/// the ground - on a slope, down it - and the ground, let go at last, springs
/// it back up with nothing to stop it rising.
fn landing_key(height: f32, vy: f32, on_water: bool, rotor: &Rotor) -> Option<KeyCode> {
    let rest = height + vy / rotor.linear_damping.max(0.1);
    if on_water {
        if vy > 0.0 && height < WATER_COAST_FROM_M || rest < -WATER_REST_M {
            Some(KeyCode::Space)
        } else if rest > WATER_REST_M {
            Some(KeyCode::ShiftLeft)
        } else {
            None
        }
    } else if height > SURFACE_M && rest > -LAND_SINK_M {
        Some(KeyCode::ShiftLeft)
    } else {
        None
    }
}

/// The speed that stops a body `distance` from the point just short of it,
/// braking with part of what S can do.
fn approach_speed(distance: f32, rotor: &Rotor) -> f32 {
    let braking = rotor.cyclic_accel * APPROACH_BRAKE_SHARE;
    (2.0 * braking * (distance - APPROACH_STOP_SHORT_M).max(0.0)).sqrt()
}

/// Space to climb faster, Shift to come down faster, or neither, for a
/// body going up at `vy` that wants to go up at `want`.
fn climb_key(vy: f32, want: f32) -> Option<KeyCode> {
    if vy < want - CLIMB_BAND {
        Some(KeyCode::Space)
    } else if vy > want + CLIMB_BAND {
        Some(KeyCode::ShiftLeft)
    } else {
        None
    }
}

/// The key that turns the nose `off` degrees clockwise and stops it there:
/// let go early by the turn its spin still has in it, and turned back
/// against a spin that would carry it past.
fn yaw_key(off: f32, yaw_rate: f32, rotor: &Rotor) -> Option<KeyCode> {
    // Clockwise is a turn about -Y, and a spin coasts rate / damping.
    let coasting = (-yaw_rate / rotor.angular_damping.max(0.1)).to_degrees();
    let rest = off - coasting;
    if rest > YAW_DEAD_ZONE_DEG {
        Some(KeyCode::KeyD)
    } else if rest < -YAW_DEAD_ZONE_DEG {
        Some(KeyCode::KeyA)
    } else {
        None
    }
}

/// The keys that bring the body's velocity over the ground to `want`: W or
/// S for what is short along the nose, E or Q for what is short beside it.
fn thrust_keys(forward: Vec3, velocity: Vec2, want: Vec2, keys: &mut Vec<KeyCode>) {
    let ahead = flat(forward);
    // The body's right, as `Transform::right` has it: forward x up.
    let right = Vec2::new(-ahead.y, ahead.x);
    let short = want - velocity;
    let along = short.dot(ahead);
    let beside = short.dot(right);
    if along > THRUST_BAND {
        keys.push(KeyCode::KeyW);
    } else if along < -THRUST_BAND {
        keys.push(KeyCode::KeyS);
    }
    if beside > THRUST_BAND {
        keys.push(KeyCode::KeyE);
    } else if beside < -THRUST_BAND {
        keys.push(KeyCode::KeyQ);
    }
}

/// `forward` on the ground, unit.
pub(super) fn flat(forward: Vec3) -> Vec2 {
    Vec2::new(forward.x, forward.z).normalize_or(Vec2::NEG_Y)
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::SystemState;

    use super::super::sense::Sensing;
    use super::*;
    use crate::pds::LocomotionConfig;
    use crate::pds::avatar::AvatarRecord;
    use crate::player::sim::FlightBench;
    use crate::water::{WaterPlane, WaterSurfaces};

    /// The offline airship stand-in: a twin envelope, the heaviest and
    /// slowest-turning of the seeded airships.
    const AIRSHIP: &str = "did:plc:agentofflineair222222222";

    /// The daemon's frame, which is how often a flight is steered.
    const FRAME: f64 = 1.0 / 30.0;

    fn airship() -> (AvatarRecord, Rotor) {
        let record = AvatarRecord::default_for_did(AIRSHIP);
        let LocomotionConfig::Helicopter(params) = &record.locomotion else {
            panic!("{AIRSHIP} is an airship");
        };
        let rotor = Rotor::of(params);
        (record, rotor)
    }

    /// Ground at one height everywhere with nothing standing on it, for the
    /// sums that need no bench.
    struct Level(f32);

    impl Terrain for Level {
        fn surface_at(&self, _: Vec2) -> f32 {
            self.0
        }

        fn clear_along(&self, _: Vec2, reach: f32, _: f32) -> f32 {
            reach
        }
    }

    /// One of the daemon's frames: the body sensed the way the daemon
    /// senses it - its own [`Sensing`], over the bench's world - and `steer`
    /// asked what to hold, with that sensing as the world it flies through.
    fn frame<T>(bench: &mut FlightBench, steer: impl FnOnce(&Craft, &Sensing) -> T) -> (Craft, T) {
        let world = bench.world_mut();
        let mut state = SystemState::<Sensing>::new(world);
        let sensing = state
            .get(world)
            .expect("the bench runs the physics the sensing reads");
        let craft = sensing.craft().expect("the bench's body is over its floor");
        let held = steer(&craft, &sensing);
        (craft, held)
    }

    /// Step the bench's physics on until `start` + `until` seconds.
    fn catch_up(bench: &mut FlightBench, start: f64, until: f64) {
        while bench.elapsed() - start < until {
            bench.step();
        }
    }

    /// One frame of a flight, as the trace keeps it.
    #[derive(Debug)]
    struct Sample {
        t: f64,
        position: Vec3,
        underside: f32,
        height: f32,
        phase: Phase,
        keys: Vec<KeyCode>,
    }

    fn sample(t: f64, craft: &Craft, flight: &Flight, keys: &[KeyCode]) -> Sample {
        Sample {
            t,
            position: craft.position,
            underside: craft.underside,
            height: craft.height(),
            phase: flight.phase,
            keys: keys.to_vec(),
        }
    }

    /// Fly the bench's body to `target` the way the daemon does - steered
    /// once a frame, the physics stepped at its own rate in between - until
    /// the flight ends or `limit` seconds pass.
    fn fly(
        bench: &mut FlightBench,
        rotor: &Rotor,
        target: Vec2,
        limit: f64,
    ) -> (Option<Flown>, Vec<Sample>) {
        let start = bench.elapsed();
        let mut flight: Option<Flight> = None;
        let mut trace = Vec::new();
        let mut now = 0.0;
        while now < limit {
            let (craft, flown) = frame(bench, |craft, sensing| {
                let flight = flight.get_or_insert_with(|| Flight::new(craft, now));
                fly_to(rotor, flight, craft, sensing, target, now)
            });
            let Flown::Hold(keys) = flown else {
                bench.hold(&[]);
                return (Some(flown), trace);
            };
            bench.hold(&keys);
            trace.push(sample(now, &craft, flight.as_ref().expect("flying"), &keys));
            now += FRAME;
            catch_up(bench, start, now);
        }
        (None, trace)
    }

    fn horizontal(sample: &Sample) -> Vec2 {
        sample.position.xz()
    }

    /// THE FLIGHT: from rest on the ground, a point 50 m off the nose's
    /// side. It rises before it moves, cruises at its height, stops over
    /// the point and comes down on it - and stays down.
    #[test]
    fn an_airship_takes_off_cruises_and_lands_on_its_point() {
        let (record, rotor) = airship();
        let mut bench = FlightBench::new(&record, Vec3::new(0.0, 0.52, 0.0));
        let target = Vec2::new(40.0, 30.0);

        let (end, trace) = fly(&mut bench, &rotor, target, 60.0);

        assert_eq!(end, Some(Flown::Landed), "{:?}", trace.last());
        let missed = bench.position().xz().distance(target);
        assert!(missed <= 1.0, "landed {missed:.2} m from the point");
        assert!(
            bench.underside() <= TOUCHDOWN_HEIGHT_M,
            "down on the ground, not {:.2} m over it",
            bench.underside()
        );
        let rose_first = trace
            .iter()
            .filter(|s| s.height < CRUISE_HEIGHT_M - TAKE_OFF_WITHIN_M - 1.0)
            .take_while(|s| s.phase == Phase::TakingOff)
            .map(|s| horizontal(s).length())
            .fold(0.0, f32::max);
        assert!(
            rose_first < 1.0,
            "it moved {rose_first:.2} m before it was up"
        );
        let cruising: Vec<f32> = trace
            .iter()
            .filter(|s| s.phase == Phase::Cruising && s.t > 6.0)
            .map(|s| s.height)
            .collect();
        assert!(!cruising.is_empty(), "it cruised");
        for height in cruising {
            assert!(
                (height - CRUISE_HEIGHT_M).abs() < 2.0,
                "cruising at {height:.2} m, not about {CRUISE_HEIGHT_M}"
            );
        }
        assert!(
            trace
                .iter()
                .filter(|s| !matches!(s.phase, Phase::Landing { .. }))
                .all(|s| s.t < 1.0 || s.height > 1.0),
            "it touched nothing on the way"
        );

        // Let go of everything: a landed airship stays landed - its hover
        // thrust only cancels its weight.
        for _ in 0..(10 * 64) {
            bench.step();
        }
        assert!(
            bench.underside() < 0.05 && bench.position().xz().distance(target) <= 1.0,
            "ten seconds later it is where it landed: {:?}, underside {:.3}",
            bench.position(),
            bench.underside()
        );
    }

    /// A point behind is swung round to on the spot, as a car swings round
    /// to one: nothing pushes the body along until its nose is on the way.
    /// Already up at its cruising height, so it is the cruise that swings -
    /// a take-off swings while it climbs, and holds still anyway.
    #[test]
    fn a_point_behind_is_swung_round_to_before_the_body_moves() {
        let (record, rotor) = airship();
        let mut bench = FlightBench::new(&record, Vec3::new(0.0, CRUISE_HEIGHT_M + 0.5, 0.0));
        let target = Vec2::new(0.0, 40.0);

        let (end, trace) = fly(&mut bench, &rotor, target, 60.0);

        assert_eq!(end, Some(Flown::Landed), "{:?}", trace.last());
        assert_eq!(trace[1].phase, Phase::Cruising, "up already");
        let set_off = trace
            .iter()
            .find(|s| s.keys.contains(&KeyCode::KeyW))
            .map(|s| s.t)
            .expect("it set off");
        assert!(set_off > 2.0, "it swung round first, for {set_off:.2} s");
        let wandered = trace
            .iter()
            .filter(|s| s.t < set_off)
            .map(|s| horizontal(s).length())
            .fold(0.0, f32::max);
        assert!(wandered < 0.5, "it drifted {wandered:.2} m while swinging");
    }

    /// A point under the body - or within reach of it - is landed on
    /// without climbing to cruise first.
    #[test]
    fn a_point_under_the_body_is_come_down_on_straight() {
        let (record, rotor) = airship();
        let mut bench = FlightBench::new(&record, Vec3::new(0.0, 15.5, 0.0));

        let (end, trace) = fly(&mut bench, &rotor, Vec2::new(1.0, 0.5), 60.0);

        assert_eq!(end, Some(Flown::Landed), "{:?}", trace.last());
        assert!(
            trace.iter().all(|s| s.position.y <= 15.6),
            "it went up before it came down"
        );
        assert!(
            trace
                .iter()
                .all(|s| matches!(s.phase, Phase::Landing { .. })),
            "straight to landing"
        );
    }

    /// Water at `level` everywhere over the bench - which a body let down
    /// onto it would sink through to the floor.
    fn flood(bench: &mut FlightBench, level: f32) {
        bench.world_mut().insert_resource(WaterSurfaces {
            planes: vec![WaterPlane {
                world_from_local: Transform::from_xyz(0.0, level, 0.0),
                local_half_extents: Vec2::splat(1000.0),
                flow_strength: 0.0,
                owner: WaterPlane::NO_OWNER,
            }],
        });
    }

    /// Water holds nothing up, so a body let down onto it would sink to
    /// the bed. Here the bench's floor is 6 m under the water: the flight
    /// stops on the surface and calls that landed - and, let go, stays
    /// there. From cruising height, and from so low over the water it has
    /// to climb before it can come down right.
    #[test]
    fn water_is_landed_on_not_sunk_into() {
        let (record, rotor) = airship();
        for start in [6.0 + CRUISE_HEIGHT_M, 8.5] {
            let mut bench = FlightBench::new(&record, Vec3::new(0.0, start, 0.0));
            flood(&mut bench, 6.0);

            let (end, trace) = fly(&mut bench, &rotor, Vec2::new(0.5, 0.5), 60.0);

            assert_eq!(end, Some(Flown::Landed), "from {start}: {:?}", trace.last());
            for _ in 0..(10 * 64) {
                bench.step();
            }
            let underside = bench.underside();
            assert!(
                (underside - 6.0).abs() <= 2.0 * WATER_REST_M,
                "from {start}: on the surface at 6 m ten seconds later, not at {underside:.2}"
            );
        }
    }

    const SLOPE_DEG: f32 = 15.0;

    /// Ground rising at 15 degrees toward +Z, its surface 5 m up at the
    /// origin.
    fn slope(xz: Vec2) -> f32 {
        5.0 + SLOPE_DEG.to_radians().tan() * xz.y
    }

    /// THE CASE THAT ASKED FOR THIS: a landing on a hillside held Shift
    /// after it touched down - a full-speed descent asked of a body the
    /// ground has stopped - which pressed it into the slope and down it,
    /// kept the touchdown from ever being judged, and ended `stuck`; let go,
    /// the ground sprang it a metre and a half back up. Down on a slope, it
    /// lets go, is judged landed once its stabiliser has stood it up again,
    /// and stays where that leaves it.
    #[test]
    fn a_landing_on_a_slope_stays_where_it_touched_down() {
        let (record, rotor) = airship();
        let mut bench = FlightBench::new(&record, Vec3::new(0.0, 16.0, 0.0));
        let tilt = Quat::from_rotation_x(-SLOPE_DEG.to_radians());
        let normal = tilt * Vec3::Y;
        bench.block(
            Transform::from_translation(Vec3::new(0.0, 5.0, 0.0) - normal * 2.0)
                .with_rotation(tilt),
            Vec3::new(40.0, 2.0, 40.0),
        );
        bench.ground(slope);

        let (end, trace) = fly(&mut bench, &rotor, Vec2::new(0.5, 0.5), 60.0);

        assert_eq!(end, Some(Flown::Landed), "{:?}", trace.last());
        let landed_at = bench.position();
        for _ in 0..(10 * 64) {
            bench.step();
        }
        let moved = bench.position().distance(landed_at);
        let (craft, ()) = frame(&mut bench, |_, _| ());
        assert!(
            moved < 0.1 && craft.height() < 0.1,
            "ten seconds later it has moved {moved:.2} m and stands {:.2} m over the slope",
            craft.height()
        );
    }

    /// A body that cannot rise - a roof over it - gets nowhere, and a
    /// flight that gets nowhere is stuck, as a walk into a wall is.
    #[test]
    fn a_flight_under_a_roof_is_stuck() {
        let (record, rotor) = airship();
        let mut bench = FlightBench::new(&record, Vec3::new(0.0, 0.52, 0.0));
        bench.block(
            Transform::from_xyz(0.0, 3.0, 0.0),
            Vec3::new(10.0, 0.5, 10.0),
        );

        let (end, trace) = fly(&mut bench, &rotor, Vec2::new(0.0, -60.0), 60.0);

        assert_eq!(end, Some(Flown::Stuck), "{:?}", trace.last());
        let took = trace.last().map_or(0.0, |s| s.t);
        assert!(
            took < STUCK_AFTER_SECS + 2.0,
            "stuck when it stopped getting anywhere, not after {took:.1} s"
        );
    }

    /// A tower three times the cruising height stands on the course, where
    /// the ground's height shows nothing. The flight sees it by sweeping
    /// itself ahead, stops short, climbs over it, and lands beyond it -
    /// never touching it. (Pressed against its face, a flight would read the
    /// tower as under it and climb anyway, scraping up the wall: the test
    /// holds the flight to stopping short.)
    #[test]
    fn a_tower_in_the_way_is_climbed_over() {
        let (record, rotor) = airship();
        let mut bench = FlightBench::new(&record, Vec3::new(0.0, 0.52, 0.0));
        let tower_top = 3.0 * CRUISE_HEIGHT_M;
        bench.block(
            Transform::from_xyz(0.0, tower_top * 0.5, -40.0),
            Vec3::new(4.0, tower_top * 0.5, 4.0),
        );
        let target = Vec2::new(0.0, -80.0);

        let (end, trace) = fly(&mut bench, &rotor, target, 120.0);

        assert_eq!(end, Some(Flown::Landed), "{:?}", trace.last());
        assert!(bench.position().xz().distance(target) <= 1.0);
        let near_face = -40.0 + 4.0;
        let closest = trace
            .iter()
            .filter(|s| s.underside < tower_top && s.position.z > -40.0)
            .map(|s| s.position.z - near_face)
            .fold(f32::INFINITY, f32::min);
        assert!(
            closest > rotor.reach + 1.0,
            "under the tower's top its middle came {closest:.2} m from the tower's face"
        );
        let over = trace
            .iter()
            .filter(|s| (s.position.z + 40.0).abs() < 4.0)
            .map(|s| s.underside)
            .fold(f32::INFINITY, f32::min);
        assert!(
            over > tower_top,
            "it crossed the tower with its underside at {over:.2}, the top at {tower_top}"
        );
    }

    /// A turn in the air keeps the body's height and its place: it swings
    /// round where it is, and is judged once the swing has settled.
    #[test]
    fn a_turn_in_the_air_keeps_its_height_and_place() {
        let (record, rotor) = airship();
        let mut bench = FlightBench::new(&record, Vec3::new(0.0, 10.5, 0.0));
        let start = bench.elapsed();
        let from = bench.position();
        // 150 degrees to the right of the nose, which looks down -Z.
        let dir = Vec2::new(0.5, 0.866);
        let mut flight: Option<Flight> = None;
        let mut phase = TurnPhase::Moving { since: 0.0 };
        let mut now = 0.0;
        let turned = loop {
            let (_, turned) = frame(&mut bench, |craft, _| {
                let flight = flight.get_or_insert_with(|| Flight::new(craft, now));
                turn(&rotor, flight, craft, dir, &mut phase, now)
            });
            match turned {
                Turned::Hold(keys) => bench.hold(&keys),
                ended => break ended,
            }
            now += FRAME;
            assert!(now < 30.0, "the turn never ended");
            catch_up(&mut bench, start, now);
        };

        assert_eq!(turned, Turned::Faced);
        let (craft, ()) = frame(&mut bench, |_, _| ());
        let off = heading_off(craft.forward, dir);
        assert!(off.abs() <= FACE_TOLERANCE_DEG, "{off:.1} degrees off");
        assert!(
            (craft.position.y - from.y).abs() < 0.5
                && craft.position.xz().distance(from.xz()) < 0.5,
            "it turned where it was: {:?} from {from:?}",
            craft.position
        );
    }

    /// Where a scripted player is `t` seconds in: walking 40 m down -Z at
    /// 2 m/s, standing 12 s, then walking on.
    fn walker(t: f64) -> Vec2 {
        let t = t as f32;
        let along = if t < 20.0 {
            2.0 * t
        } else if t < 32.0 {
            40.0
        } else {
            40.0 + 2.0 * (t - 32.0)
        };
        Vec2::new(0.0, -along)
    }

    /// THE ESCORT (owner, 2026-09-24): after a walking player low, keeping
    /// up; down beside them once they have stood still a while; up and after
    /// them again when they walk on.
    #[test]
    fn an_escort_flies_low_after_a_player_and_lands_beside_them_once_they_stand() {
        let (record, rotor) = airship();
        let mut bench = FlightBench::new(&record, Vec3::new(0.0, 0.52, 10.0));
        let keep = 3.0;
        let start = bench.elapsed();
        let mut flight: Option<Flight> = None;
        let mut trace = Vec::new();
        let mut now = 0.0;
        while now < 45.0 {
            let player = walker(now);
            let (craft, escort) = frame(&mut bench, |craft, sensing| {
                let flight = flight.get_or_insert_with(|| Flight::new(craft, now));
                super::escort(&rotor, flight, craft, sensing, player, keep, now)
            });
            bench.hold(&escort.keys);
            assert!(!escort.blocked, "blocked on open ground at {now:.2}");
            trace.push((
                sample(now, &craft, flight.as_ref().expect("flying"), &escort.keys),
                player,
            ));
            now += FRAME;
            catch_up(&mut bench, start, now);
        }
        let gap = |(s, player): &(Sample, Vec2)| horizontal(s).distance(*player);
        // Walking: up at about its escort height, and keeping up.
        for entry in trace.iter().filter(|(s, _)| (8.0..20.0).contains(&s.t)) {
            let (s, _) = entry;
            assert!(
                (s.height - ESCORT_HEIGHT_M).abs() < 2.5,
                "escorting at {:.2} m at {:.2}",
                s.height,
                s.t
            );
            assert!(
                gap(entry) < 12.0,
                "fell {:.1} m behind at {:.2}",
                gap(entry),
                s.t
            );
        }
        // Standing: down beside them before they walk on.
        let down = trace
            .iter()
            .find(|(s, _)| s.phase == Phase::Landed)
            .expect("it landed beside them");
        assert!(
            (20.0 + LAND_BESIDE_AFTER_SECS..32.0).contains(&down.0.t),
            "landed at {:.2}",
            down.0.t
        );
        assert!(
            down.0.height < TOUCHDOWN_HEIGHT_M && gap(down) < keep + FOLLOW_SLACK_M + rotor.reach
        );
        // Walking on: up and after them again.
        let last = trace.last().expect("a trace");
        assert!(
            last.0.phase == Phase::Cruising && last.0.height > 5.0 && gap(last) < 15.0,
            "after them again: {last:?}"
        );
    }

    fn craft(position: Vec3, forward: Vec3) -> Craft {
        Craft {
            position,
            forward,
            up: Vec3::Y,
            velocity: Vec3::ZERO,
            yaw_rate: 0.0,
            spin: Vec3::ZERO,
            underside: position.y,
            below: 0.0,
            on_water: false,
        }
    }

    /// Getting somewhere is moving or turning: a slow swing on the spot is
    /// not stuck however long it takes, and standing is, after as long as a
    /// walk may stand.
    #[test]
    fn a_slow_swing_gets_somewhere_and_standing_does_not() {
        let mut still = Stillness::new(&craft(Vec3::ZERO, Vec3::NEG_Z), 0.0);
        for second in 1..=30 {
            let heading = Quat::from_rotation_y((second as f32 * 6.0).to_radians()) * Vec3::NEG_Z;
            assert!(
                !still.nowhere_for_too_long(&craft(Vec3::ZERO, heading), f64::from(second)),
                "swinging at 6 degrees a second, second {second}"
            );
        }
        let resting = craft(
            Vec3::new(0.1, 0.0, 0.0),
            Quat::from_rotation_y(3.2) * Vec3::NEG_Z,
        );
        assert!(!still.nowhere_for_too_long(&resting, 31.0));
        assert!(still.nowhere_for_too_long(&resting, 30.0 + STUCK_AFTER_SECS));
    }

    /// Space climbs faster, Shift comes down faster, and inside the band
    /// neither is pressed.
    #[test]
    fn the_climb_keys_close_on_the_speed_asked_for() {
        assert_eq!(climb_key(0.0, 3.0), Some(KeyCode::Space));
        assert_eq!(climb_key(0.0, -3.0), Some(KeyCode::ShiftLeft));
        assert_eq!(climb_key(2.8, 3.0), None);
        assert_eq!(climb_key(4.0, 3.0), Some(KeyCode::ShiftLeft));
    }

    /// A turn lets go early by the coast its spin has left - and turns back
    /// against a spin that would carry it well past.
    #[test]
    fn a_turn_lets_go_by_the_spin_it_still_has() {
        let (_, rotor) = airship();
        assert_eq!(yaw_key(40.0, 0.0, &rotor), Some(KeyCode::KeyD), "right");
        assert_eq!(yaw_key(-40.0, 0.0, &rotor), Some(KeyCode::KeyA), "left");
        // Turning clockwise (about -Y) at 0.77 rad/s coasts 7.4 degrees at
        // this body's damping.
        assert_eq!(yaw_key(7.0, -0.77, &rotor), None, "let go: it coasts there");
        assert_eq!(
            yaw_key(1.0, -0.77, &rotor),
            Some(KeyCode::KeyA),
            "it would coast 6 degrees past: turn back"
        );
    }

    /// Thrust is in the body's frame: a body facing +Z has -X on its right,
    /// so a velocity wanted toward -X is E, and one wanted behind it is S.
    #[test]
    fn thrust_is_asked_for_along_and_beside_the_nose() {
        let mut keys = Vec::new();
        thrust_keys(Vec3::Z, Vec2::ZERO, Vec2::new(0.0, 3.0), &mut keys);
        assert_eq!(keys, [KeyCode::KeyW]);
        keys.clear();
        thrust_keys(Vec3::Z, Vec2::ZERO, Vec2::new(-3.0, -3.0), &mut keys);
        assert_eq!(keys, [KeyCode::KeyS, KeyCode::KeyE]);
        keys.clear();
        thrust_keys(Vec3::Z, Vec2::new(2.0, 0.0), Vec2::ZERO, &mut keys);
        assert_eq!(keys, [KeyCode::KeyE], "sliding toward +X: push back right");
        keys.clear();
        thrust_keys(Vec3::Z, Vec2::new(0.1, 0.1), Vec2::ZERO, &mut keys);
        assert!(keys.is_empty(), "inside the band");
    }

    /// Onto the ground it comes down at full speed and lets go once coasting
    /// would carry it a little under the surface; on the ground it presses
    /// nothing. Onto water it lets go to coast to the surface itself, climbs
    /// back from under it, and climbs first from too low to get up to speed.
    #[test]
    fn a_landing_lets_go_where_coasting_ends_on_the_surface() {
        let (_, rotor) = airship();
        let shift = Some(KeyCode::ShiftLeft);
        let space = Some(KeyCode::Space);
        // An airship coasts 6.25 m from 5 m/s.
        assert_eq!(landing_key(20.0, 0.0, false, &rotor), shift, "high up");
        assert_eq!(landing_key(20.0, -5.0, false, &rotor), shift, "still high");
        assert_eq!(
            landing_key(6.0, -5.0, false, &rotor),
            None,
            "coasts to 0.25 m under"
        );
        assert_eq!(landing_key(0.0, 0.0, false, &rotor), None, "on the ground");
        assert_eq!(
            landing_key(6.5, -5.0, true, &rotor),
            shift,
            "water: 0.25 over"
        );
        assert_eq!(
            landing_key(6.3, -5.0, true, &rotor),
            None,
            "water: coasts onto it"
        );
        assert_eq!(
            landing_key(-0.5, 0.0, true, &rotor),
            space,
            "under the water"
        );
        assert_eq!(
            landing_key(3.0, 0.5, true, &rotor),
            space,
            "too low: up first"
        );
    }

    /// A hill on the course ahead raises the cruise before the body gets to
    /// it; one beyond the point does not.
    struct HillAt(f32);

    impl Terrain for HillAt {
        fn surface_at(&self, xz: Vec2) -> f32 {
            if (xz.x - self.0).abs() < 3.0 {
                15.0
            } else {
                0.0
            }
        }

        fn clear_along(&self, _: Vec2, reach: f32, _: f32) -> f32 {
            reach
        }
    }

    #[test]
    fn a_cruise_climbs_for_ground_ahead_of_it_not_beyond_the_point() {
        let body = craft(Vec3::new(0.0, 20.0, 0.0), Vec3::X);
        let cruise = |terrain: &dyn Terrain, body: &Craft, to: Vec2| {
            level(terrain, body, to, CRUISE_HEIGHT_M)
        };
        assert_eq!(
            cruise(&HillAt(20.0), &body, Vec2::new(100.0, 0.0)),
            15.0 + CRUISE_HEIGHT_M,
            "the hill 20 m ahead"
        );
        assert_eq!(
            cruise(&HillAt(20.0), &body, Vec2::new(10.0, 0.0)),
            CRUISE_HEIGHT_M,
            "the hill past a point 10 m off"
        );
        assert_eq!(
            cruise(
                &HillAt(CRUISE_LOOKAHEAD_M + 10.0),
                &body,
                Vec2::new(100.0, 0.0)
            ),
            CRUISE_HEIGHT_M,
            "the hill past the look-ahead"
        );
        let over_a_roof = Craft {
            below: 12.0,
            ..body
        };
        assert_eq!(
            cruise(&Level(0.0), &over_a_roof, Vec2::new(100.0, 0.0)),
            CRUISE_HEIGHT_M.max(12.0 + CRUISE_CLEARANCE_M)
        );
    }
}
