//! Flying an airplane to a point and landing on it (#1431). No seeded body
//! is an airplane: the preset is worn by published records.
//!
//! The game's airplane is not a real one. Its lift is straight up, in
//! proportion to its forward speed and nothing else - not its pitch, not its
//! bank - so it flies level at the one speed whose lift carries its weight
//! (8.7 m/s for the default record), climbs when faster, and drops like a
//! stone under its stall speed, where lift stops altogether. With no key
//! held its engine runs at cruise, several times that speed: hands off it
//! takes off and climbs for ever, so on the ground it holds Shift, the
//! engine cut, whenever nothing is flying it. And its controls are far too
//! quick for the daemon's frame - a frame of A or D rolls it about 140
//! degrees, one of W or S pitches it about 27 - so a flight holds the nose
//! level and never touches them. Its height is its throttle's (Space, none,
//! Shift), asking for the speed that climbs or sinks as it needs; its heading
//! is its rudder's (Q, E), a frame of which turns it about 12 degrees. The
//! path follows the nose only over seconds - nothing pushes it sideways but
//! its engine and its drag - so the nose leads the path round.
//!
//! A flight to a point (owner, 2026-09-24): on the ground it swings round to
//! a clear run toward the point and takes off; it cruises [`CRUISE_HEIGHT_M`]
//! up; from [`FINAL_M`] out it comes down a straight final toward the point
//! by slowing, cuts its engine at touchdown just short, and stops at the
//! point. Not lined up at the final's start, or low over something on it, it
//! goes round - out to [`GO_ROUND_TO_M`] and back - and after
//! [`MAX_GO_ROUNDS`] of those it lands where it can and says it is stuck.
//! Landing where it can is also how it comes down to end a movement in the
//! air: straight ahead, since it cannot stop up there.

use bevy::prelude::*;

use crate::config::agent::{
    ARRIVE_WINGED_M, CRUISE_CLEARANCE_M, CRUISE_HEIGHT_M, FACE_SETTLE_SECS, FACE_TOLERANCE_DEG,
    FINAL_M, FLARE_FROM_M, FLARE_SINK, FORCED_SINK, GLIDE_SLOPE, GO_ROUND_TO_M, LINED_UP_ASIDE_M,
    LINED_UP_DEG, MAX_GO_ROUNDS, OBSTACLE_SWEEP_M, SURFACE_M, TAKE_OFF_RUN_M,
    TAKE_OFF_SWEEP_LIFT_M, TOUCHDOWN_HEIGHT_M, TOUCHDOWN_SECS, TOUCHDOWN_SHORT_M,
    TOUCHDOWN_STILL_M, UNDER_THE_GLIDE_M, WING_CLIMB_GAIN, WING_LEAD_MAX_DEG, WING_MAX_CLIMB,
    WING_MAX_SINK, WING_NOSE_OFF_PATH_MAX_DEG, WING_PITCH_TOLERANCE_DEG, WING_SPEED_BAND,
    WING_SPEED_PER_CLIMB, WING_STALL_MARGIN, WING_SWING_FIRST_DEG, WING_YAW_DEAD_ZONE_DEG,
};
use crate::pds::AirplaneParams;

use super::flight::{Craft, Flown, Over, Stillness, Terrain, Turned, flat, level};
use super::ground::turned;
use super::{TurnPhase, heading_off};

/// The gravity the game's physics runs under: avian's default, which the
/// game keeps.
const GRAVITY: f32 = 9.81;

/// Below this height over what is under it, an airplane is flying rather
/// than rolling (m).
const AIRBORNE_M: f32 = 1.0;

/// An airplane's handling, from its record: what a flight has to know to
/// hold its height and turn in time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Wing {
    /// The forward speed whose lift carries the weight (m/s).
    level_speed: f32,
    /// The slowest it flies: a margin over the stall (m/s).
    slowest: f32,
    /// How fast a spin dies once its key is let go (1/s).
    angular_damping: f32,
}

impl Wing {
    pub(super) fn of(params: &AirplaneParams) -> Self {
        let level_speed = params.mass.0 * GRAVITY / params.lift_per_speed.0.max(0.1);
        let slowest = params.min_airspeed.0 * WING_STALL_MARGIN;
        Self {
            // A record whose lift carries it below the stall still has to
            // fly above the stall to lift at all.
            level_speed: level_speed.max(slowest),
            slowest,
            angular_damping: params.angular_damping.0,
        }
    }
}

/// Why an approach failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wrong {
    /// Not lined up at the final's start.
    NotLinedUp,
    /// Its path strayed off the point on the final.
    Astray,
    /// The point passed while it was still flying.
    Passed,
    /// Something standing under the glide the ground's height does not show.
    OverSomething,
    /// Something standing in its way ahead.
    InTheWay,
    /// Stopped too far from the point.
    StoppedShort,
}

/// Where an airplane's flight to a point is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Leg {
    /// On the ground: swinging round to its run, then rolling down it to
    /// take off.
    Ground { run: Option<Vec2> },
    /// Up, making for the final.
    Homing,
    /// Going round: straight out along `out` until far enough to come
    /// round again.
    GoingRound { out: Vec2 },
    /// On the final, gliding down onto the point.
    Final,
    /// Landing where it can, straight on along `along`.
    Forced { along: Vec2 },
    /// Down, engine cut, sliding to a stop - at rest since a time, where it
    /// was then, once it is.
    Rollout { resting: Option<(f64, Vec3)> },
}

impl Leg {
    /// The leg as `status` names it.
    pub(super) fn word(self) -> &'static str {
        match self {
            Self::Ground { .. } => "taking_off",
            Self::Homing => "cruising",
            Self::GoingRound { .. } => "going_round",
            Self::Final => "final",
            Self::Forced { .. } => "landing_where_it_can",
            Self::Rollout { .. } => "rolling_out",
        }
    }
}

/// An airplane's flight under way.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Sortie {
    pub(super) leg: Leg,
    /// Approaches that have failed, and so gone round.
    tries: u32,
    /// Been out past the final's start since it last took off - so the
    /// next time it comes in is an approach, which may fail. Going round
    /// straight after taking off near its point is not a failed one.
    been_out: bool,
    /// Out of tries: landing where it can, to end stuck.
    giving_up: bool,
    still: Stillness,
    over: Option<Over>,
}

impl Sortie {
    pub(super) fn new(craft: &Craft, now: f64) -> Self {
        Self {
            leg: if craft.height() > AIRBORNE_M {
                Leg::Homing
            } else {
                Leg::Ground { run: None }
            },
            tries: 0,
            been_out: false,
            giving_up: false,
            still: Stillness::new(craft, now),
            over: None,
        }
    }

    /// A flight landing where it can, straight ahead - how an airplane
    /// comes down to end a movement in the air.
    pub(super) fn landing_ahead(craft: &Craft, now: f64) -> Self {
        Self {
            leg: if craft.height() > AIRBORNE_M {
                Leg::Forced {
                    along: track(craft),
                }
            } else {
                Leg::Rollout { resting: None }
            },
            ..Self::new(craft, now)
        }
    }

    /// Another approach has failed: go round - straight on, or aside from
    /// something in its way - or, out of tries, land where it can.
    fn go_round(&mut self, craft: &Craft, wrong: Wrong) {
        if wrong == Wrong::NotLinedUp && !self.been_out {
            self.leg = Leg::GoingRound { out: track(craft) };
            return;
        }
        self.tries += 1;
        info!(
            "The airplane's approach failed ({wrong:?}): try {} of {}",
            self.tries,
            MAX_GO_ROUNDS + 1
        );
        let on = track(craft);
        let out = match wrong {
            Wrong::InTheWay => turned(on, 90.0),
            Wrong::NotLinedUp
            | Wrong::Astray
            | Wrong::Passed
            | Wrong::OverSomething
            | Wrong::StoppedShort => on,
        };
        if self.tries > MAX_GO_ROUNDS {
            self.giving_up = true;
            self.leg = Leg::Forced { along: out };
        } else {
            self.leg = Leg::GoingRound { out };
        }
    }
}

/// Fly an airplane on toward `target` (x, z), and land on it.
pub(super) fn fly_to(
    wing: &Wing,
    sortie: &mut Sortie,
    craft: &Craft,
    terrain: &dyn Terrain,
    target: Vec2,
    now: f64,
) -> Flown {
    let offset = target - craft.position.xz();
    let distance = offset.length();
    let bearing = offset.normalize_or(flat(craft.forward));
    let airborne = craft.height() > AIRBORNE_M;
    if distance > FINAL_M && airborne {
        sortie.been_out = true;
    }
    // The legs that end one another, before this frame's keys.
    match sortie.leg {
        Leg::Ground { .. } if airborne => sortie.leg = Leg::Homing,
        Leg::Homing if distance <= FINAL_M => {
            if lined_up(craft, target) {
                sortie.leg = Leg::Final;
            } else {
                sortie.go_round(craft, Wrong::NotLinedUp);
            }
        }
        Leg::GoingRound { .. } if distance >= GO_ROUND_TO_M => sortie.leg = Leg::Homing,
        Leg::Final if craft.height() <= SURFACE_M => {
            sortie.leg = Leg::Rollout { resting: None };
        }
        Leg::Final => {
            if let Some(wrong) = off_the_final(craft, terrain, target) {
                sortie.go_round(craft, wrong);
            }
        }
        Leg::Forced { .. } if craft.height() <= SURFACE_M => {
            sortie.leg = Leg::Rollout { resting: None };
        }
        _ => {}
    }
    let mut keys = Vec::new();
    match &mut sortie.leg {
        Leg::Ground { run } => {
            if distance <= ARRIVE_WINGED_M && craft.velocity.length() < 1.0 {
                return Flown::Landed;
            }
            let run = match *run {
                Some(chosen) => chosen,
                None => {
                    // Walled in: nowhere to take off.
                    let Some(found) = clear_run(terrain, bearing) else {
                        return Flown::Stuck;
                    };
                    *run = Some(found);
                    found
                }
            };
            let off = heading_off(craft.forward, run);
            keys.extend(rudder_key(off, craft.yaw_rate, wing));
            if craft.velocity.xz().length() < 1.0 && off.abs() > WING_SWING_FIRST_DEG {
                // Swinging round on the spot, the engine cut to stay put.
                keys.push(KeyCode::ShiftLeft);
            } else {
                // Rolling: asking for the climb it wants lifts it off just
                // over the speed that carries it - at full throttle it left
                // the ground at 25 m/s and shot up to 73 m.
                let level = level(terrain, craft, target, CRUISE_HEIGHT_M);
                keys.extend(throttle_for(wing, craft, level, 0.0, false));
            }
        }
        Leg::Homing => {
            // Already coming down the glide it will land on, where that is
            // under its cruise: its height answers its speed only over
            // seconds, and a final begun 8 m too high crossed the point.
            let cruise = level(terrain, craft, target, CRUISE_HEIGHT_M);
            let lowest = level(terrain, craft, target, CRUISE_CLEARANCE_M);
            let glide = glide(terrain, target, distance);
            let level = cruise.min(glide.max(lowest));
            let on_the_glide = level == glide;
            let (floor, blocked) = climb_over(&mut sortie.over, craft, terrain);
            let sinking = if on_the_glide {
                glide_sink(craft, bearing)
            } else {
                0.0
            };
            keys.extend(throttle_for(
                wing,
                craft,
                level.max(floor),
                sinking,
                blocked,
            ));
            keys.extend(nose_for(wing, craft, bearing));
        }
        Leg::GoingRound { out } => {
            let out = *out;
            let level = level(terrain, craft, craft.position.xz() + out, CRUISE_HEIGHT_M);
            let (floor, blocked) = climb_over(&mut sortie.over, craft, terrain);
            keys.extend(throttle_for(wing, craft, level.max(floor), 0.0, blocked));
            keys.extend(nose_for(wing, craft, out));
        }
        Leg::Final => {
            let glide = glide(terrain, target, distance);
            let sinking = glide_sink(craft, bearing);
            keys.extend(throttle_for(wing, craft, glide, sinking, false));
            keys.extend(nose_for(wing, craft, bearing));
        }
        Leg::Forced { along } => {
            let along = *along;
            keys.extend(throttle_key(
                forward_speed(craft),
                speed_for_climb(wing, craft, -FORCED_SINK),
            ));
            keys.extend(nose_for(wing, craft, along));
        }
        Leg::Rollout { resting } => {
            // Engine cut: it slides to a stop within a few metres.
            keys.push(KeyCode::ShiftLeft);
            let could_be_down = craft.height() <= TOUCHDOWN_HEIGHT_M;
            match *resting {
                Some((since, at))
                    if could_be_down && craft.position.distance(at) <= TOUCHDOWN_STILL_M =>
                {
                    if now - since >= TOUCHDOWN_SECS {
                        if sortie.giving_up {
                            return Flown::Stuck;
                        }
                        if distance <= ARRIVE_WINGED_M {
                            return Flown::Landed;
                        }
                        // Stopped, but short of the point or past it: an
                        // approach that failed. Take off and come round.
                        sortie.go_round(craft, Wrong::StoppedShort);
                        if !sortie.giving_up {
                            sortie.leg = Leg::Ground { run: None };
                            sortie.been_out = false;
                        }
                    }
                }
                _ if could_be_down => *resting = Some((now, craft.position)),
                _ => *resting = None,
            }
        }
    }
    keys.extend(level_the_nose(wing, craft));
    if sortie.still.nowhere_for_too_long(craft, now) {
        return Flown::Stuck;
    }
    Flown::Hold(keys)
}

/// W or S to bring a nose knocked more than [`WING_PITCH_TOLERANCE_DEG`]
/// off level back to it, let go early by the pitch its spin still has in
/// it: W tips the nose down.
fn level_the_nose(wing: &Wing, craft: &Craft) -> Option<KeyCode> {
    let pitch = craft.forward.y.clamp(-1.0, 1.0).asin().to_degrees();
    // A turn about the right axis lifts the nose.
    let right = craft.forward.cross(craft.up).normalize_or_zero();
    let coasting = (craft.pitch_rate(right) / wing.angular_damping.max(0.1)).to_degrees();
    let rest = pitch + coasting;
    if rest > WING_PITCH_TOLERANCE_DEG {
        Some(KeyCode::KeyW)
    } else if rest < -WING_PITCH_TOLERANCE_DEG {
        Some(KeyCode::KeyS)
    } else {
        None
    }
}

/// Land an airplane where it can - straight ahead, since it cannot stop in
/// the air - and say when it is down and at rest (`Landed`), or that it
/// cannot get down (`Stuck`).
pub(super) fn land_ahead(
    wing: &Wing,
    sortie: &mut Sortie,
    craft: &Craft,
    terrain: &dyn Terrain,
    now: f64,
) -> Flown {
    if !matches!(sortie.leg, Leg::Forced { .. } | Leg::Rollout { .. }) {
        sortie.leg = Leg::Forced {
            along: track(craft),
        };
    }
    // Wherever it stops is where it was going.
    let here = craft.position.xz();
    sortie.giving_up = false;
    match fly_to(wing, sortie, craft, terrain, here, now) {
        Flown::Hold(keys) => Flown::Hold(keys),
        Flown::Landed | Flown::Stuck
            if matches!(sortie.leg, Leg::Rollout { .. })
                && craft.height() <= TOUCHDOWN_HEIGHT_M =>
        {
            Flown::Landed
        }
        ended => ended,
    }
}

/// Swing a stopped airplane round on the spot to `dir` - the rudder turns
/// it freely at rest, the engine cut to keep it there - judged once the
/// swing has settled. On the ground only (owner, 2026-09-24): in the air it
/// cannot turn without flying on.
pub(super) fn turn(
    wing: &Wing,
    sortie: &mut Sortie,
    craft: &Craft,
    dir: Vec2,
    phase: &mut TurnPhase,
    now: f64,
) -> Turned {
    let off = heading_off(craft.forward, dir);
    let mut keys = vec![KeyCode::ShiftLeft];
    match *phase {
        TurnPhase::Moving { .. } => match rudder_key(off, craft.yaw_rate, wing) {
            Some(key) => keys.push(key),
            None => *phase = TurnPhase::Settling { since: now },
        },
        TurnPhase::Settling { since } if now - since < FACE_SETTLE_SECS => {}
        TurnPhase::Settling { .. } if off.abs() <= FACE_TOLERANCE_DEG => return Turned::Faced,
        TurnPhase::Settling { .. } => *phase = TurnPhase::Moving { since: now },
    }
    if sortie.still.nowhere_for_too_long(craft, now) {
        return Turned::Stuck;
    }
    Turned::Hold(keys)
}

/// Whether an airplane is up in the air rather than on the ground.
pub(super) fn airborne(craft: &Craft) -> bool {
    craft.height() > AIRBORNE_M
}

/// How fast the glide it follows sinks under it, flying at its speed toward
/// the point along `bearing` (m/s, negative).
fn glide_sink(craft: &Craft, bearing: Vec2) -> f32 {
    -GLIDE_SLOPE * craft.velocity.xz().dot(bearing).max(0.0)
}

/// The glide it lands on: the height its underside wants `distance` out
/// from `target`, down to the ground just short of it.
fn glide(terrain: &dyn Terrain, target: Vec2, distance: f32) -> f32 {
    terrain.surface_at(target) + GLIDE_SLOPE * (distance - TOUCHDOWN_SHORT_M).max(0.0)
}

/// Whether it is lined up on `target`: its path points within
/// [`LINED_UP_DEG`] of it, and passes within [`LINED_UP_ASIDE_M`] of it.
fn lined_up(craft: &Craft, target: Vec2) -> bool {
    let offset = target - craft.position.xz();
    let track = track(craft);
    let off = heading_off(Vec3::new(track.x, 0.0, track.y), offset.normalize_or_zero());
    let aside = offset.length() * off.to_radians().sin().abs();
    off.abs() <= LINED_UP_DEG && aside <= LINED_UP_ASIDE_M
}

/// What has gone wrong with a final, if anything: its path no longer on
/// the point, the point passed by more than half the room it has to stop in
/// while it is still flying, something standing under the glide the
/// ground's height does not show, or something in the way ahead.
fn off_the_final(craft: &Craft, terrain: &dyn Terrain, target: Vec2) -> Option<Wrong> {
    let offset = target - craft.position.xz();
    let track = track(craft);
    let off = heading_off(Vec3::new(track.x, 0.0, track.y), offset.normalize_or_zero());
    let aside = offset.length() * off.to_radians().sin().abs();
    let ahead = off.abs() <= 90.0;
    let astray = ahead && (off.abs() > 2.0 * LINED_UP_DEG || aside > 2.0 * LINED_UP_ASIDE_M);
    let passed = !ahead && offset.length() > 0.5 * ARRIVE_WINGED_M;
    let over_something = !craft.on_water
        && craft.height() < UNDER_THE_GLIDE_M
        && craft.below > terrain.surface_at(craft.position.xz()) + UNDER_THE_GLIDE_M;
    let in_the_way = terrain.clear_along(track, OBSTACLE_SWEEP_M, 0.0) < OBSTACLE_SWEEP_M;
    if in_the_way {
        Some(Wrong::InTheWay)
    } else if passed {
        Some(Wrong::Passed)
    } else if over_something {
        Some(Wrong::OverSomething)
    } else if astray {
        Some(Wrong::Astray)
    } else {
        None
    }
}

/// The run an airplane takes off along: the way nearest `toward` that is
/// clear for [`TAKE_OFF_RUN_M`], or `None` when none is.
fn clear_run(terrain: &dyn Terrain, toward: Vec2) -> Option<Vec2> {
    [
        0.0, 30.0, -30.0, 60.0, -60.0, 90.0, -90.0, 120.0, -120.0, 150.0, -150.0, 180.0,
    ]
    .into_iter()
    .map(|degrees| turned(toward, degrees))
    .find(|run| terrain.clear_along(*run, TAKE_OFF_RUN_M, TAKE_OFF_SWEEP_LIFT_M) >= TAKE_OFF_RUN_M)
}

/// The height to hold over something standing in the way along its path -
/// found by sweeping itself ahead - until it is past it (`NEG_INFINITY`
/// when nothing is), and whether it is in the way this frame. An airplane
/// cannot stop short of it, so it climbs.
fn climb_over(over: &mut Option<Over>, craft: &Craft, terrain: &dyn Terrain) -> (f32, bool) {
    let here = craft.position.xz();
    if over.is_some_and(|over| here.distance(over.at) > OBSTACLE_SWEEP_M) {
        *over = None;
    }
    let blocked = terrain.clear_along(track(craft), OBSTACLE_SWEEP_M, 0.0) < OBSTACLE_SWEEP_M;
    if blocked {
        *over = Some(Over {
            level: craft.underside + CRUISE_CLEARANCE_M,
            at: here,
        });
    }
    (over.map_or(f32::NEG_INFINITY, |over| over.level), blocked)
}

/// The throttle key that holds its underside at `level` - by its speed,
/// since its speed is its lift - where that level is itself going up at
/// `level_vy` (a glide's sink, negative), rounding out near the ground; at
/// full throttle while `climbing_over` something in its way. Without the
/// level's own speed a glide was followed 2 m high: rate over gain.
fn throttle_for(
    wing: &Wing,
    craft: &Craft,
    level: f32,
    level_vy: f32,
    climbing_over: bool,
) -> Option<KeyCode> {
    if climbing_over {
        return Some(KeyCode::Space);
    }
    let mut want_vy = (level_vy + WING_CLIMB_GAIN * (level - craft.underside))
        .clamp(-WING_MAX_SINK, WING_MAX_CLIMB);
    let height = craft.height().max(0.0);
    if height < FLARE_FROM_M {
        let flare = FLARE_SINK + (WING_MAX_SINK - FLARE_SINK) * height / FLARE_FROM_M;
        want_vy = want_vy.max(-flare);
    }
    throttle_key(forward_speed(craft), speed_for_climb(wing, craft, want_vy))
}

/// The forward speed that brings its vertical speed to `want_vy`: the
/// speed that holds it level, and more or less by how far it is short.
fn speed_for_climb(wing: &Wing, craft: &Craft, want_vy: f32) -> f32 {
    let short = want_vy - craft.velocity.y;
    (wing.level_speed + WING_SPEED_PER_CLIMB * short).clamp(wing.slowest, 2.5 * wing.level_speed)
}

/// The throttle for a forward speed `speed` that wants to be `want`: the
/// engine cut (Shift) unless it is short, at cruise (no key) when it is,
/// and full (Space) when it is well short. The engine's steps are coarse
/// against the drag, so the speed is held by the odd frame of power.
fn throttle_key(speed: f32, want: f32) -> Option<KeyCode> {
    if speed < want - 3.0 * WING_SPEED_BAND {
        Some(KeyCode::Space)
    } else if speed < want - WING_SPEED_BAND {
        None
    } else {
        Some(KeyCode::ShiftLeft)
    }
}

/// The rudder that brings its path round to `toward`: the nose aimed past
/// it by however far the path is off it, so the path, which follows the
/// nose only slowly, comes round sooner - but never more than
/// [`WING_NOSE_OFF_PATH_MAX_DEG`] off the path, or its lift goes.
fn nose_for(wing: &Wing, craft: &Craft, toward: Vec2) -> Option<KeyCode> {
    let track = track(craft);
    let lag = heading_off(Vec3::new(track.x, 0.0, track.y), toward);
    let lead = lag.clamp(-WING_LEAD_MAX_DEG, WING_LEAD_MAX_DEG);
    let off_path = (lag + lead).clamp(-WING_NOSE_OFF_PATH_MAX_DEG, WING_NOSE_OFF_PATH_MAX_DEG);
    let nose = turned(track, off_path);
    rudder_key(heading_off(craft.forward, nose), craft.yaw_rate, wing)
}

/// The rudder key that turns the nose `off` degrees clockwise and stops it
/// there: let go early by the turn its spin still has in it. E turns it
/// clockwise, Q back.
fn rudder_key(off: f32, yaw_rate: f32, wing: &Wing) -> Option<KeyCode> {
    let coasting = (-yaw_rate / wing.angular_damping.max(0.1)).to_degrees();
    let rest = off - coasting;
    if rest > WING_YAW_DEAD_ZONE_DEG {
        Some(KeyCode::KeyE)
    } else if rest < -WING_YAW_DEAD_ZONE_DEG {
        Some(KeyCode::KeyQ)
    } else {
        None
    }
}

/// Its speed along its nose - what its lift is measured by.
fn forward_speed(craft: &Craft) -> f32 {
    craft.forward.dot(craft.velocity)
}

/// The way it is going over the ground: its path, or its nose when it is
/// barely moving.
fn track(craft: &Craft) -> Vec2 {
    let over_ground = craft.velocity.xz();
    if over_ground.length() > 1.0 {
        over_ground.normalize()
    } else {
        flat(craft.forward)
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::SystemState;

    use super::super::sense::Sensing;
    use super::*;
    use crate::pds::LocomotionConfig;
    use crate::pds::avatar::AvatarRecord;
    use crate::player::sim::FlightBench;

    /// The daemon's frame, which is how often a flight is steered.
    const FRAME: f64 = 1.0 / 30.0;

    /// The default airplane, worn by the airship stand-in.
    fn airplane() -> (AvatarRecord, Wing) {
        let mut record = AvatarRecord::default_for_did("did:plc:agentofflineair222222222");
        let params = AirplaneParams::default();
        let wing = Wing::of(&params);
        record.locomotion = LocomotionConfig::Airplane(Box::new(params));
        (record, wing)
    }

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

    #[derive(Debug)]
    struct Sample {
        t: f64,
        position: Vec3,
        height: f32,
        leg: Leg,
        keys: Vec<KeyCode>,
    }

    /// Fly the bench's airplane to `target` as the daemon does, until the
    /// flight ends or `limit` seconds pass.
    fn fly(
        bench: &mut FlightBench,
        wing: &Wing,
        target: Vec2,
        limit: f64,
    ) -> (Option<Flown>, Vec<Sample>) {
        let start = bench.elapsed();
        let mut sortie: Option<Sortie> = None;
        let mut trace = Vec::new();
        let mut now = 0.0;
        while now < limit {
            let (craft, flown) = frame(bench, |craft, sensing| {
                let sortie = sortie.get_or_insert_with(|| Sortie::new(craft, now));
                fly_to(wing, sortie, craft, sensing, target, now)
            });
            let Flown::Hold(keys) = flown else {
                return (Some(flown), trace);
            };
            bench.hold(&keys);
            trace.push(Sample {
                t: now,
                position: craft.position,
                height: craft.height(),
                leg: sortie.as_ref().expect("flying").leg,
                keys,
            });
            now += FRAME;
            while bench.elapsed() - start < now {
                bench.step();
            }
        }
        (None, trace)
    }

    /// The bench's airplane up in the air at `at`, flying level along -Z at
    /// the speed that holds it there.
    fn flying_at(record: &AvatarRecord, wing: &Wing, at: Vec3) -> FlightBench {
        let mut bench = FlightBench::new(record, at);
        bench.set_velocity(Vec3::new(0.0, 0.0, -wing.level_speed));
        bench
    }

    fn landed_within(bench: &FlightBench, target: Vec2) -> f32 {
        bench.position().xz().distance(target)
    }

    /// THE FLIGHT: from rest on the ground, a point 150 m ahead. It rolls,
    /// lifts off just over the speed that carries it, comes down the glide
    /// and stops at the point - going round never.
    #[test]
    fn an_airplane_takes_off_and_lands_on_a_point_ahead() {
        let (record, wing) = airplane();
        let mut bench = FlightBench::new(&record, Vec3::new(0.0, 0.35, 0.0));
        let target = Vec2::new(0.0, -150.0);

        let (end, trace) = fly(&mut bench, &wing, target, 90.0);

        assert_eq!(end, Some(Flown::Landed), "{:?}", trace.last());
        let missed = landed_within(&bench, target);
        assert!(
            missed <= ARRIVE_WINGED_M,
            "stopped {missed:.2} m from the point"
        );
        assert!(
            trace
                .iter()
                .all(|s| !matches!(s.leg, Leg::GoingRound { .. })),
            "it went round"
        );
        let highest = trace.iter().map(|s| s.height).fold(0.0, f32::max);
        assert!(
            highest < CRUISE_HEIGHT_M + 3.0,
            "it climbed to {highest:.1} m: its take-off ran away with it"
        );
    }

    /// A point behind is swung round to on the spot - the rudder turns it
    /// freely at rest, the engine cut - before it rolls.
    #[test]
    fn a_point_behind_is_swung_round_to_on_the_ground_first() {
        let (record, wing) = airplane();
        let mut bench = FlightBench::new(&record, Vec3::new(0.0, 0.35, 0.0));
        let target = Vec2::new(0.0, 150.0);

        let (end, trace) = fly(&mut bench, &wing, target, 120.0);

        assert_eq!(end, Some(Flown::Landed), "{:?}", trace.last());
        assert!(landed_within(&bench, target) <= ARRIVE_WINGED_M);
        let rolled = trace
            .iter()
            .find(|s| !s.keys.contains(&KeyCode::ShiftLeft))
            .map(|s| s.t)
            .expect("it rolled");
        let swung_in_place = trace
            .iter()
            .filter(|s| s.t < rolled)
            .map(|s| s.position.xz().length())
            .fold(0.0, f32::max);
        assert!(rolled > 0.5, "it swung round first, for {rolled:.2} s");
        assert!(
            swung_in_place < 1.0,
            "it moved {swung_in_place:.2} m swinging"
        );
    }

    /// Up in the air, a point well off its path: it cannot turn tight, so
    /// it comes round - once, or twice - and lands on it.
    #[test]
    fn a_point_well_off_its_path_is_come_round_to() {
        let (record, wing) = airplane();
        for target in [Vec2::new(150.0, 0.0), Vec2::new(0.0, 40.0)] {
            let mut bench = flying_at(&record, &wing, Vec3::new(0.0, 20.5, 0.0));

            let (end, trace) = fly(&mut bench, &wing, target, 180.0);

            assert_eq!(end, Some(Flown::Landed), "{target}: {:?}", trace.last());
            let missed = landed_within(&bench, target);
            assert!(
                missed <= ARRIVE_WINGED_M,
                "{target}: stopped {missed:.2} m off"
            );
        }
    }

    /// Hemmed in on the ground with no clear run anywhere, it is stuck.
    #[test]
    fn an_airplane_walled_in_is_stuck() {
        let (record, wing) = airplane();
        let mut bench = FlightBench::new(&record, Vec3::new(0.0, 0.35, 0.0));
        for (x, z, half) in [
            (0.0, -8.0, Vec3::new(10.0, 3.0, 0.5)),
            (0.0, 8.0, Vec3::new(10.0, 3.0, 0.5)),
            (-8.0, 0.0, Vec3::new(0.5, 3.0, 10.0)),
            (8.0, 0.0, Vec3::new(0.5, 3.0, 10.0)),
        ] {
            bench.block(Transform::from_xyz(x, 3.0, z), half);
        }
        for _ in 0..4 {
            bench.step();
        }

        let (end, _) = fly(&mut bench, &wing, Vec2::new(0.0, -150.0), 30.0);

        assert_eq!(end, Some(Flown::Stuck));
    }

    /// A tower stands on the point, so every final meets it: after going
    /// round as often as it may, it lands where it can and says it is
    /// stuck (owner, 2026-09-24).
    #[test]
    fn out_of_tries_it_lands_where_it_can_and_is_stuck() {
        let (record, wing) = airplane();
        let mut bench = flying_at(&record, &wing, Vec3::new(0.0, 20.5, 0.0));
        let target = Vec2::new(0.0, -150.0);
        bench.block(
            Transform::from_xyz(target.x, 15.0, target.y),
            Vec3::new(6.0, 15.0, 6.0),
        );

        let (end, trace) = fly(&mut bench, &wing, target, 400.0);

        assert_eq!(end, Some(Flown::Stuck), "{:?}", trace.last());
        let went_round = trace
            .windows(2)
            .filter(|w| {
                !matches!(w[0].leg, Leg::GoingRound { .. })
                    && matches!(w[1].leg, Leg::GoingRound { .. })
            })
            .count();
        assert_eq!(
            went_round as u32, MAX_GO_ROUNDS,
            "it went round {went_round} times"
        );
        let (craft, ()) = frame(&mut bench, |_, _| ());
        assert!(craft.height() <= TOUCHDOWN_HEIGHT_M, "down where it could");
    }

    /// Landing where it can - straight ahead, since it cannot stop up
    /// there - ends down on the ground, at rest.
    #[test]
    fn landing_ahead_comes_down_where_it_can() {
        let (record, wing) = airplane();
        let mut bench = flying_at(&record, &wing, Vec3::new(0.0, 20.5, 0.0));
        let start = bench.elapsed();
        let mut sortie: Option<Sortie> = None;
        let mut now = 0.0;
        let landed = loop {
            let (_, flown) = frame(&mut bench, |craft, sensing| {
                let sortie = sortie.get_or_insert_with(|| Sortie::landing_ahead(craft, now));
                land_ahead(&wing, sortie, craft, sensing, now)
            });
            match flown {
                Flown::Hold(keys) => bench.hold(&keys),
                ended => break ended,
            }
            now += FRAME;
            assert!(now < 90.0, "it never came down");
            while bench.elapsed() - start < now {
                bench.step();
            }
        };

        assert_eq!(landed, Flown::Landed);
        let (craft, ()) = frame(&mut bench, |_, _| ());
        assert!(craft.height() <= TOUCHDOWN_HEIGHT_M && craft.velocity.length() < 0.2);
    }

    /// E turns the nose clockwise and Q back, let go early by the coast;
    /// the throttle is cut unless the speed is short, cruise when it is,
    /// full when it is well short.
    #[test]
    fn the_rudder_and_the_throttle_answer_the_way_they_should() {
        let (_, wing) = airplane();
        assert_eq!(rudder_key(40.0, 0.0, &wing), Some(KeyCode::KeyE));
        assert_eq!(rudder_key(-40.0, 0.0, &wing), Some(KeyCode::KeyQ));
        // Turning clockwise at 1 rad/s coasts 29 degrees at damping 2.
        assert_eq!(rudder_key(28.0, -1.0, &wing), None);
        assert_eq!(throttle_key(9.0, 9.0), Some(KeyCode::ShiftLeft));
        assert_eq!(throttle_key(8.7, 9.0), None);
        assert_eq!(throttle_key(8.0, 9.0), Some(KeyCode::Space));
    }
}
