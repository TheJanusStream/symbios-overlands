//! Flying an airplane to a point and landing on it (#1431), on the flight
//! model of #1615 - a published record's airplane, or a seeded one, a sixth
//! of seeds since #1617, which sits tail-down on her own gear and takes off
//! from it on power alone.
//!
//! The game's airplane flies as a light airplane does: its lift leans with
//! its wings, so it turns by banking; it holds its height hands-off, its
//! controller levelling its wings and its path the moment the keys are let
//! go; and its throttle HOLDS where it is left, so a flight sets its power
//! as a pilot does rather than holding a key for it. So a flight here is
//! three loops, each on its own keys:
//!
//! * its SPEED, on the throttle - Space when it is slower than the leg
//!   wants by more than [`WING_SPEED_BAND`], Shift when it is faster, and
//!   neither in between, the throttle holding;
//! * its HEIGHT, on the pitch - S when it climbs slower than the leg wants,
//!   W when faster, rounding out near the ground; with neither held its
//!   path is level;
//! * its HEADING, on the bank - D or A while its path is off the way it
//!   wants by more than the turn its bank would still give it as its wings
//!   level ([`AirplaneHandling::roll_out_turn`]), and wings level once it
//!   is not. On the ground it steers with the rudder (Q, E), which turns it
//!   on the spot as well.
//!
//! A flight to a point (owner, 2026-09-24): on the ground it swings round to
//! a clear run toward the point, opens the throttle and lifts its nose at
//! [`WING_ROTATE_OVER_STALL`] its stall speed; it cruises
//! [`CRUISE_HEIGHT_M`] up; from [`FINAL_M`] out it comes down a straight
//! final toward the point at its approach speed, closes its throttle
//! [`WING_POWER_OFF_M`] up, brakes once its wheels are down short of the
//! point, and stops at it. Not
//! lined up at the final's start, or low over something on it, it goes
//! round - out to [`GO_ROUND_TO_M`] and back - and after [`MAX_GO_ROUNDS`]
//! of those it lands where it can and says it is stuck. Landing where it can
//! is also how it comes down to end a movement in the air: straight ahead,
//! since it cannot stop up there.

use bevy::prelude::*;

use crate::config::agent::{
    ARRIVE_WINGED_M, CRUISE_CLEARANCE_M, CRUISE_HEIGHT_M, FACE_SETTLE_SECS, FACE_TOLERANCE_DEG,
    FINAL_M, FLARE_FROM_M, FLARE_SINK, FORCED_SINK, GLIDE_SLOPE, GO_ROUND_TO_M, LINED_UP_ASIDE_M,
    LINED_UP_DEG, MAX_GO_ROUNDS, OBSTACLE_SWEEP_M, TAKE_OFF_RUN_M, TAKE_OFF_SWEEP_LIFT_M,
    TOUCHDOWN_SECS, TOUCHDOWN_SHORT_M, TOUCHDOWN_STILL_M, UNDER_THE_GLIDE_M,
    WING_APPROACH_OVER_STALL, WING_BANK_DEAD_ZONE_DEG, WING_CLIMB_BAND, WING_CLIMB_GAIN,
    WING_CRUISE_OVER_STALL, WING_MAX_CLIMB, WING_MAX_SINK, WING_POWER_OFF_M,
    WING_ROTATE_OVER_STALL, WING_SPEED_BAND, WING_SWING_FIRST_DEG, WING_WHEELS_DOWN_M,
    WING_YAW_DEAD_ZONE_DEG,
};
use crate::pds::AirplaneParams;
use crate::player::AirplaneHandling;

use super::flight::{Craft, Flown, Over, Stillness, Terrain, Turned, flat, level};
use super::ground::turned;
use super::{TurnPhase, heading_off};

/// This far over its wheels' reach, an airplane is flying rather than
/// rolling (m).
const AIRBORNE_M: f32 = 1.0;

/// The least it flies over its stall speed and still lifts its nose (a
/// share): nearer the stall, a pull only stalls it.
const PULL_OVER_STALL: f32 = 1.05;

/// An airplane's handling, from its record: the speeds a flight flies it
/// at, how far its wheels reach, and how its turns die away.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Wing {
    /// Its stall speed (m/s).
    stall: f32,
    /// The speed it cruises at (m/s).
    cruise: f32,
    /// The speed it comes down a final at (m/s).
    approach: f32,
    /// The speed it lifts its nose at on its take-off run (m/s).
    rotate: f32,
    /// How far its wheels reach under its box (m): its box's underside is
    /// this high over the ground as they touch it.
    gear: f32,
    handling: AirplaneHandling,
}

impl Wing {
    pub(super) fn of(params: &AirplaneParams) -> Self {
        let handling = AirplaneHandling::of(params);
        // The stall the physics flies her by, floor and all.
        let stall = handling.stall();
        Self {
            stall,
            cruise: stall * WING_CRUISE_OVER_STALL,
            approach: stall * WING_APPROACH_OVER_STALL,
            rotate: stall * WING_ROTATE_OVER_STALL,
            gear: params.gear_height.0,
            handling,
        }
    }

    /// Whether it is down on its wheels - or within [`WING_WHEELS_DOWN_M`]
    /// of it, which a frame closes.
    fn down(&self, craft: &Craft) -> bool {
        craft.height() <= self.gear + WING_WHEELS_DOWN_M
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
    /// Down, throttle closed and braking, to a stop - at rest since a time,
    /// where it was then, once it is.
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
    pub(super) fn new(wing: &Wing, craft: &Craft, now: f64) -> Self {
        Self {
            leg: if airborne(wing, craft) {
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
    pub(super) fn landing_ahead(wing: &Wing, craft: &Craft, now: f64) -> Self {
        Self {
            leg: if airborne(wing, craft) {
                Leg::Forced {
                    along: track(craft),
                }
            } else {
                Leg::Rollout { resting: None }
            },
            ..Self::new(wing, craft, now)
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
    let airborne = airborne(wing, craft);
    let down = wing.down(craft);
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
        Leg::Final if down => sortie.leg = Leg::Rollout { resting: None },
        Leg::Final => {
            if let Some(wrong) = off_the_final(craft, terrain, target) {
                sortie.go_round(craft, wrong);
            }
        }
        Leg::Forced { .. } if down => sortie.leg = Leg::Rollout { resting: None },
        _ => {}
    }
    let speed = forward_speed(craft);
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
            keys.extend(rudder_key(wing, off, craft.yaw_rate));
            if off.abs() > WING_SWING_FIRST_DEG {
                // Not along its run yet: throttle closed and braked - to a
                // stop first, if it is still rolling - while it swings
                // round. Rolling, its steering turns it only as fast as its
                // grip allows, and on full power it reached its rotation
                // speed facing a way no sweep had found clear.
                keys.push(KeyCode::ShiftLeft);
            } else {
                // Full power down the run, and the nose up at its rotation
                // speed: it lifts off into its climb.
                keys.push(KeyCode::Space);
                if speed >= wing.rotate {
                    keys.push(KeyCode::KeyS);
                }
            }
        }
        Leg::Homing => {
            // Already coming down the glide it will land on, where that is
            // under its cruise: a final begun high crosses the point.
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
            let throttle = throttle_key(speed, wing.cruise);
            keys.extend(fly_level(
                wing,
                craft,
                level.max(floor),
                sinking,
                throttle,
                blocked,
            ));
            keys.extend(bank_for(wing, craft, bearing));
        }
        Leg::GoingRound { out } => {
            let out = *out;
            let level = level(terrain, craft, craft.position.xz() + out, CRUISE_HEIGHT_M);
            let (floor, blocked) = climb_over(&mut sortie.over, craft, terrain);
            let throttle = throttle_key(speed, wing.cruise);
            keys.extend(fly_level(
                wing,
                craft,
                level.max(floor),
                0.0,
                throttle,
                blocked,
            ));
            keys.extend(bank_for(wing, craft, out));
        }
        Leg::Final => {
            let glide = glide(terrain, target, distance);
            let sinking = glide_sink(craft, bearing);
            let throttle = landing_power(wing, craft, speed);
            keys.extend(fly_level(wing, craft, glide, sinking, throttle, false));
            keys.extend(bank_for(wing, craft, bearing));
        }
        Leg::Forced { along } => {
            let along = *along;
            keys.extend(landing_power(wing, craft, speed));
            keys.extend(pitch_key(wing, craft, flared(wing, craft, -FORCED_SINK)));
            keys.extend(bank_for(wing, craft, along));
        }
        Leg::Rollout { resting } => {
            // Throttle closed and the brakes on: Shift held does both.
            keys.push(KeyCode::ShiftLeft);
            match *resting {
                Some((since, at)) if down && craft.position.distance(at) <= TOUCHDOWN_STILL_M => {
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
                _ if down => *resting = Some((now, craft.position)),
                _ => *resting = None,
            }
        }
    }
    if sortie.still.nowhere_for_too_long(craft, now) {
        return Flown::Stuck;
    }
    Flown::Hold(keys)
}

/// The keys that hold it at `level` - its underside's height, itself rising
/// at `level_vy` - on the `throttle` key given: the pitch for its height,
/// the throttle for its speed. Full power and a pull while `climbing_over`
/// something in its way.
fn fly_level(
    wing: &Wing,
    craft: &Craft,
    level: f32,
    level_vy: f32,
    throttle: Option<KeyCode>,
    climbing_over: bool,
) -> Vec<KeyCode> {
    if climbing_over {
        return [KeyCode::Space]
            .into_iter()
            .chain(pitch_key(wing, craft, WING_MAX_CLIMB))
            .collect();
    }
    throttle
        .into_iter()
        .chain(pitch_key(
            wing,
            craft,
            climb_for(wing, craft, level, level_vy),
        ))
        .collect()
}

/// The throttle key coming in to land: its approach speed, until it is
/// [`WING_POWER_OFF_M`] over the ground - and from there the throttle
/// closed, so it is shut by the time its wheels are down and Shift is its
/// brakes at once. Closed only as they touched, it rolled on 30 m.
fn landing_power(wing: &Wing, craft: &Craft, speed: f32) -> Option<KeyCode> {
    if craft.height() - wing.gear < WING_POWER_OFF_M {
        Some(KeyCode::ShiftLeft)
    } else {
        throttle_key(speed, wing.approach)
    }
}

/// The climb (m/s) that brings its underside to `level`, which is itself
/// going up at `level_vy` (a glide's sink, negative), rounding out near the
/// ground. Without the level's own speed a glide was followed 2 m high:
/// rate over gain.
fn climb_for(wing: &Wing, craft: &Craft, level: f32, level_vy: f32) -> f32 {
    let want = (level_vy + WING_CLIMB_GAIN * (level - craft.underside))
        .clamp(-WING_MAX_SINK, WING_MAX_CLIMB);
    flared(wing, craft, want)
}

/// A wanted climb, its sink held to a round-out near the ground: no faster
/// than [`FLARE_SINK`] as its wheels touch, and easing up to
/// [`WING_MAX_SINK`] at [`FLARE_FROM_M`] over that.
fn flared(wing: &Wing, craft: &Craft, want: f32) -> f32 {
    let height = (craft.height() - wing.gear).max(0.0);
    if height < FLARE_FROM_M {
        let flare = FLARE_SINK + (WING_MAX_SINK - FLARE_SINK) * height / FLARE_FROM_M;
        want.max(-flare)
    } else {
        want
    }
}

/// The pitch key that brings its climb to `want` (m/s): S when it climbs
/// slower than that by more than [`WING_CLIMB_BAND`], W when faster,
/// neither in between - its path holds level with neither held. Never S at
/// the edge of its stall ([`PULL_OVER_STALL`]).
fn pitch_key(wing: &Wing, craft: &Craft, want: f32) -> Option<KeyCode> {
    let climbing = craft.velocity.y;
    if climbing < want - WING_CLIMB_BAND && forward_speed(craft) > wing.stall * PULL_OVER_STALL {
        Some(KeyCode::KeyS)
    } else if climbing > want + WING_CLIMB_BAND {
        Some(KeyCode::KeyW)
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
            if matches!(sortie.leg, Leg::Rollout { .. }) && wing.down(craft) =>
        {
            Flown::Landed
        }
        ended => ended,
    }
}

/// Swing a stopped airplane round on the spot to `dir` - its steering turns
/// it on the ground, on the spot too, Shift holding it braked - judged once
/// the swing has settled. On the ground only (owner, 2026-09-24): in
/// the air it cannot turn without flying on.
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
        TurnPhase::Moving { .. } => match rudder_key(wing, off, craft.yaw_rate) {
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
pub(super) fn airborne(wing: &Wing, craft: &Craft) -> bool {
    craft.height() > wing.gear + AIRBORNE_M
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

/// The throttle key for a forward speed `speed` that wants to be `want`:
/// Space when it is slower by more than [`WING_SPEED_BAND`], Shift when
/// faster, and neither inside that - the throttle holds where the last key
/// left it, so its power settles where it carries it at that speed.
fn throttle_key(speed: f32, want: f32) -> Option<KeyCode> {
    if speed < want - WING_SPEED_BAND {
        Some(KeyCode::Space)
    } else if speed > want + WING_SPEED_BAND {
        Some(KeyCode::ShiftLeft)
    } else {
        None
    }
}

/// The bank that brings its path round to `toward` and stops it there: D
/// or A while it is off by more than [`WING_BANK_DEAD_ZONE_DEG`], let go
/// early by the turn its bank still has in it as its wings level - about
/// fifteen degrees from its steepest bank at its cruise.
fn bank_for(wing: &Wing, craft: &Craft, toward: Vec2) -> Option<KeyCode> {
    let track = track(craft);
    let off = heading_off(Vec3::new(track.x, 0.0, track.y), toward);
    let right = craft.forward.cross(craft.up);
    let bank = (-right.y).atan2(craft.up.y);
    let rolling = craft.spin.dot(craft.forward);
    let coasting = wing
        .handling
        .roll_out_turn(bank, rolling, forward_speed(craft))
        .to_degrees();
    let rest = off - coasting;
    if rest > WING_BANK_DEAD_ZONE_DEG {
        Some(KeyCode::KeyD)
    } else if rest < -WING_BANK_DEAD_ZONE_DEG {
        Some(KeyCode::KeyA)
    } else {
        None
    }
}

/// The rudder key that steers its nose on the ground `off` degrees
/// clockwise and stops it there: let go early by the turn it still has in
/// it, which dies over its handling's settling time. E steers clockwise, Q
/// back.
fn rudder_key(wing: &Wing, off: f32, yaw_rate: f32) -> Option<KeyCode> {
    let coasting = (-yaw_rate * wing.handling.settle_secs()).to_degrees();
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

    /// About the throttle that holds the default airplane level at its
    /// cruise.
    const CRUISE_THROTTLE: f32 = 0.52;

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
                let sortie = sortie.get_or_insert_with(|| Sortie::new(wing, craft, now));
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
    /// its cruise, its throttle about where it holds it there.
    fn flying_at(record: &AvatarRecord, wing: &Wing, at: Vec3) -> FlightBench {
        let mut bench = FlightBench::new(record, at);
        bench.set_velocity(Vec3::new(0.0, 0.0, -wing.cruise));
        bench.set_throttle(CRUISE_THROTTLE);
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

    /// A seeded airplane flies the same flights (#1617): tail-down on her
    /// own gear, her centre of mass forward over her wing, she takes off
    /// from three points, comes down the glide and stops at a point ahead -
    /// and swings round on the spot first for one behind her.
    #[test]
    fn a_seeded_airplane_takes_off_and_lands_on_her_point() {
        use crate::seeded_defaults::ChassisFamily;
        let seed = (0u64..600)
            .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Airplane)
            .expect("some seed is an airplane");
        let record = AvatarRecord::default_for_seed(seed);
        let LocomotionConfig::Airplane(params) = &record.locomotion else {
            panic!("seed {seed}: an airplane flies the Airplane preset");
        };
        let wing = Wing::of(params);
        for target in [Vec2::new(0.0, -150.0), Vec2::new(0.0, 150.0)] {
            let mut bench = FlightBench::set_down(&record, 0.0, 0.0);
            let (end, trace) = fly(&mut bench, &wing, target, 120.0);
            assert_eq!(end, Some(Flown::Landed), "{target}: {:?}", trace.last());
            let missed = landed_within(&bench, target);
            assert!(
                missed <= ARRIVE_WINGED_M,
                "{target}: stopped {missed:.2} m from the point"
            );
        }
    }

    /// A point behind is swung round to on the spot - its steering turns
    /// it at rest, throttle closed and braked - before it rolls. It pivots
    /// about its mains, ahead of its middle, so its middle swings round
    /// them: within its own half-length of where it began.
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
            swung_in_place < 1.5,
            "it moved {swung_in_place:.2} m swinging"
        );
    }

    /// Still rolling away from its point when a flight to it begins - a
    /// walk-to given on a landing's rollout - it brakes and swings round
    /// before it opens the throttle, and lifts off along its run toward the
    /// point. On full power while it steered round, its grip turned it
    /// slower than it gathered speed, and it took off facing a way no sweep
    /// had found clear.
    #[test]
    fn rolling_away_from_its_point_it_stops_and_swings_round_first() {
        let (record, wing) = airplane();
        let mut bench = FlightBench::new(&record, Vec3::new(0.0, 0.35, 0.0));
        for _ in 0..32 {
            bench.step();
        }
        bench.set_velocity(Vec3::new(0.0, 0.0, -10.0));
        let target = Vec2::new(0.0, 150.0);

        let (end, trace) = fly(&mut bench, &wing, target, 120.0);

        assert_eq!(end, Some(Flown::Landed), "{:?}", trace.last());
        let up = trace
            .iter()
            .position(|s| s.height > wing.gear + 1.0)
            .expect("it took off");
        let path = (trace[up].position - trace[up - 3].position)
            .xz()
            .normalize();
        assert!(
            path.dot(Vec2::Y) > 30f32.to_radians().cos(),
            "it took off along {path}, not toward its point"
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
        assert!(wing.down(&craft), "down where it could");
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
                let sortie = sortie.get_or_insert_with(|| Sortie::landing_ahead(&wing, craft, now));
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
        assert!(wing.down(&craft) && craft.velocity.length() < 0.2);
    }

    /// E turns the nose clockwise and Q back, let go early by the turn its
    /// steering still has in it; the throttle moves only outside its band,
    /// and holds inside it.
    #[test]
    fn the_rudder_and_the_throttle_answer_the_way_they_should() {
        let (_, wing) = airplane();
        assert_eq!(rudder_key(&wing, 40.0, 0.0), Some(KeyCode::KeyE));
        assert_eq!(rudder_key(&wing, -40.0, 0.0), Some(KeyCode::KeyQ));
        // Turning clockwise at 1 rad/s coasts 5.7 degrees: its controller
        // and its damping take the turn off at 10 a second.
        assert_eq!(rudder_key(&wing, 8.0, -1.0), None);
        assert_eq!(rudder_key(&wing, 10.0, -1.0), Some(KeyCode::KeyE));
        assert_eq!(throttle_key(17.0, 16.0), Some(KeyCode::ShiftLeft));
        assert_eq!(throttle_key(16.3, 16.0), None);
        assert_eq!(throttle_key(15.0, 16.0), Some(KeyCode::Space));
    }

    /// D banks it toward a way to the right of its path and A to the left;
    /// banked its steepest, it lets go of D the turn its roll-out still has
    /// in it short of the way it wants - and banked toward a way it is
    /// already on, it rolls back.
    #[test]
    fn the_bank_is_let_go_by_the_turn_still_in_it() {
        let (_, wing) = airplane();
        let along = Vec2::NEG_Y;
        let craft = |bank_deg: f32| {
            let bank = bank_deg.to_radians();
            Craft {
                position: Vec3::new(0.0, 20.0, 0.0),
                forward: Vec3::NEG_Z,
                up: Vec3::new(bank.sin(), bank.cos(), 0.0),
                velocity: Vec3::NEG_Z * wing.cruise,
                yaw_rate: 0.0,
                spin: Vec3::ZERO,
                underside: 20.0,
                below: 0.0,
                on_water: false,
            }
        };
        let level = craft(0.0);
        assert_eq!(
            bank_for(&wing, &level, turned(along, 30.0)),
            Some(KeyCode::KeyD)
        );
        assert_eq!(
            bank_for(&wing, &level, turned(along, -30.0)),
            Some(KeyCode::KeyA)
        );
        assert_eq!(bank_for(&wing, &level, turned(along, 1.0)), None);
        // Its steepest bank, the one a held key takes it to.
        let steepest: f32 = 45.0;
        let coast = wing
            .handling
            .roll_out_turn(steepest.to_radians(), 0.0, wing.cruise)
            .to_degrees();
        assert!(
            (8.0..20.0).contains(&coast),
            "her steepest bank coasts {coast:.1} degrees"
        );
        let banked = craft(steepest);
        assert_eq!(bank_for(&wing, &banked, turned(along, coast)), None);
        assert_eq!(
            bank_for(&wing, &banked, turned(along, coast + 5.0)),
            Some(KeyCode::KeyD)
        );
        assert_eq!(bank_for(&wing, &banked, along), Some(KeyCode::KeyA));
    }
}
