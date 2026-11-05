//! Winged flight on the on-foot preset (#1861): one chassis with a runtime
//! mode, flown by an arcade model.
//!
//! A body whose record carries a flight section
//! ([`crate::pds::FlightParams`]) walks as any on-foot body does, and takes
//! to the air on Space. What it is doing is a [`FlightMode`] on the chassis
//! ([`Flight`]), never the record: a preset swap at every take-off would edit
//! the published record - an unsaved avatar, an undo step, an avatar update
//! to everyone and a chassis rebuilt.
//!
//! # Keys
//!
//! * **Space** on the ground takes off: a leap up and forward, and two beats
//!   for a body that can flap. In the air a press beats once and holding it
//!   keeps beating at the wingbeat rate, each beat costing stamina.
//! * **No key** in the air glides, at the polar's trim speed - the nose drops
//!   when the body is slow and rises when it is fast - recovering stamina.
//! * **W / S** pitch the nose down and up, trading height for speed.
//! * **A / D** bank left and right, which turns the body, to the record's
//!   steepest bank at its roll rate; let go, the wings level.
//! * **Shift** folds the wings and dives.
//!
//! A body walking off a ledge opens its wings once it has fallen a moment,
//! and Space in the air does at once.
//!
//! # The model
//!
//! Arcade, not aerodynamic (research-11, section 8.5). The air turns the
//! body's path toward its nose and holds it up against its weight, with no
//! more than `(speed / stall_speed)^2` of its weight: at its stall speed it
//! can just hold level. Drag is the glide polar's, its least at the trim
//! speed - half there from the speed, half from the lift - so a hands-off
//! glide at trim gives up a metre of height for every `glide_ratio` it
//! travels and sinks at `trim_speed / glide_ratio`. Under the stall speed
//! the nose drops whatever is held and the body dives until it is flying
//! again: it never just falls. A turn is a bank: the heading turns at the
//! coordinated rate `g tan(bank) / speed` and the path follows the nose.
//!
//! # Landing on tops
//!
//! A flight looks along its path for the TOP of something - ground, a roof,
//! a deck, a branch's top, anything whose surface leans less than 30 degrees
//! from level, as a rotorcraft's landing reads it - within a capture radius
//! of it. The flare begins by time to contact (the distance along the path
//! over the speed, as pigeons judge it), not by distance: the nose comes up,
//! the speed bleeds away, and the path is drawn onto the point. Resting on a
//! top, as `super::resting_on` reads one, or within a few centimetres over
//! one, the body lands, settles, and is on the ground.
//!
//! # The chassis turns kinematically
//!
//! In the air its rotation is WRITTEN, every step, from the flight's own
//! heading, pitch and bank, and the physics holds all three of its rotations
//! (`LockedAxes`), on a beast's lying capsule as on a person's. A physical
//! attitude - torques toward a target, as the airplane's - would let a bump
//! against a branch tumble the body and would make the attitude an answer of
//! the solver rather than of the keys; written, it is exactly what the model
//! says, readable by a later phase that poses a rigged body from it
//! ([`Flight::attitude`]), and sent to peers in the chassis' transform. On
//! the ground the walk turns it, as it always did.
//!
//! # What is passive
//!
//! The keys are read by [`read_flight_keys`], which stands down while the
//! owner types (#821); the flight itself is [`fly_on_foot`]'s, which does
//! not - typing in the air glides.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::pds::{FlightParams, HumanoidParams, LocomotionConfig};
use crate::state::{LiveAvatarRecord, LocalPlayer, TravelingTo};
use crate::water::WaterSurfaces;

use super::humanoid::{JumpQueued, WaterColumn, WaterState, water_state};
use super::{HumanoidPreset, VisualsEditFreeze};

#[cfg(test)]
pub(crate) mod tests;

/// What a flying body is doing (#1861). Runtime state on the chassis, never
/// on the record; sent to peers by its [`Self::wire`] code.
///
/// **Append only**: a peer reads the code by its position in [`Self::ALL`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FlightMode {
    /// On its feet: the on-foot walk moves it.
    #[default]
    Ground,
    /// The leap off the ground, and its first beats.
    Takeoff,
    /// Beating its wings, the flap key held.
    Flap,
    /// Wings spread, no key: the glide polar.
    Glide,
    /// Wings folded, Shift held: falling nose-first.
    Dive,
    /// Nose up over a top it is about to land on.
    Flare,
    /// Down on a top, settling onto its feet.
    Land,
}

impl FlightMode {
    /// Every mode, in wire order.
    pub const ALL: [Self; 7] = [
        Self::Ground,
        Self::Takeoff,
        Self::Flap,
        Self::Glide,
        Self::Dive,
        Self::Flare,
        Self::Land,
    ];

    /// Its code on the wire: its place in [`Self::ALL`].
    pub fn wire(self) -> u8 {
        Self::ALL
            .iter()
            .position(|mode| *mode == self)
            .map_or(0, |at| at as u8)
    }

    /// The mode a wire code names - `None` for one this build does not know,
    /// from a newer peer.
    pub fn from_wire(code: u8) -> Option<Self> {
        Self::ALL.get(usize::from(code)).copied()
    }

    /// Off its feet, or not yet settled on them: the physics holds every
    /// rotation and the flight writes it.
    pub fn airborne(self) -> bool {
        self != Self::Ground
    }

    /// Its name, as the agent's `status` says it.
    // Read by the agent's `status`, which is Unix-only, and the tests.
    #[cfg_attr(not(unix), allow(dead_code))]
    pub fn word(self) -> &'static str {
        match self {
            Self::Ground => "ground",
            Self::Takeoff => "takeoff",
            Self::Flap => "flap",
            Self::Glide => "glide",
            Self::Dive => "dive",
            Self::Flare => "flare",
            Self::Land => "land",
        }
    }
}

/// How long (s) the take-off lasts before the body flies as its keys say: a
/// leap and two beats at the default wingbeat.
pub(crate) const TAKEOFF_S: f32 = 0.6;
/// How many beats the take-off gives a body that can flap.
const TAKEOFF_BEATS: u32 = 2;
/// The take-off's nose, up (radians, about 20 degrees).
const TAKEOFF_PITCH: f32 = 0.35;
/// How the leap splits between up and forward.
const LEAP_UP: f32 = 0.8;
const LEAP_FORWARD: f32 = 0.6;
/// How much stronger a body that cannot flap leaps (research-11, section
/// 3.3: a glider's leap is all it has).
const GLIDER_LEAP: f32 = 1.6;
/// How a beat's push splits between the body's up and its nose.
const BEAT_UP: f32 = 0.8;
const BEAT_FORWARD: f32 = 0.6;
/// A press beats at once when at least this share of a beat's period has
/// passed since the last; a held key beats once a period.
const PRESS_BEAT_SHARE: f32 = 0.5;
/// Stamina an exhausted body must get back (a share of full) before it can
/// flap again: without the margin a held key would beat each beat's worth
/// the moment it came back.
const EXHAUSTED_UNTIL: f32 = 0.25;
/// How much faster stamina comes back standing on something than gliding.
const GROUND_RECOVERY: f32 = 3.0;

/// How fast (1/s) the air turns the path toward the nose, at the speed's
/// own scale.
const PATH_GAIN: f32 = 3.0;
/// How far the hands-off nose rises (radians) for each trim speed it flies
/// over the trim - and drops for each under: a glider's speed stability.
const SPEED_TRIM_GAIN: f32 = 0.5;
/// The hands-off nose's range (radians): 30 degrees down to 15 up.
const TRIM_PITCH_DOWN: f32 = -0.52;
const TRIM_PITCH_UP: f32 = 0.26;
/// W's nose (radians below the hands-off one, at most 45 degrees down) and
/// S's (about 25 degrees up).
const PUSH_PITCH: f32 = 0.52;
const PUSH_PITCH_MOST: f32 = -0.79;
const PULL_PITCH: f32 = 0.44;
/// How far over the glide's nose a beating body holds its own (radians,
/// about 20 degrees): a climb at its trim.
const FLAP_CLIMB_PITCH: f32 = 0.35;
/// A stalled nose (about 40 degrees down), pitched there half again as fast.
const STALL_PITCH: f32 = -0.7;
/// A stalled body flies again over this share of its stall speed.
const STALL_RECOVER: f32 = 1.25;
/// The dive's nose (about 60 degrees down), its lift and its drag, as shares
/// of the spread wings'.
const DIVE_PITCH: f32 = -1.05;
const FOLDED_LIFT: f32 = 0.15;
const FOLDED_DRAG: f32 = 0.5;

/// The flare begins when contact is this near (s): research-11's 0.3-0.5,
/// a 2-3 beat flare.
pub(crate) const FLARE_TAU_S: f32 = 0.45;
/// And gives up when contact moves this far off (s), or the top is lost for
/// [`FLARE_LOST_S`].
const FLARE_ABORT_TAU_S: f32 = 1.2;
const FLARE_LOST_S: f32 = 0.3;
/// The flare's nose (about 55 degrees up), pitched there twice as fast; its
/// lift and its drag as shares of a glide's.
const FLARE_PITCH: f32 = 0.96;
const FLARE_LIFT: f32 = 1.3;
const FLARE_DRAG: f32 = 4.0;
/// How far ahead (s at its speed) a flight looks for a top to land on.
const LOOK_AHEAD_S: f32 = 1.5;
/// The capture radius, as a share of the body's height (at least its own
/// radius): a top this near the path is one it can land on.
const CAPTURE_OF_HEIGHT: f32 = 0.6;

/// The steepest surface a body lands on: a rotorcraft's (the cosine of 30
/// degrees), so a roof's or a deck's top counts and a wall never does.
pub(crate) const LAND_NORMAL_Y: f32 = super::helicopter::LAND_NORMAL_Y;
/// How far under its feet a top may be and the body still be down on it (m),
/// as a rotorcraft's `NEAR_GROUND_M`.
const NEAR_TOP_M: f32 = 0.1;
/// How long (s) the landing settles before the body is on its feet, and how
/// fast (1/s) it takes away its speed over the ground meanwhile.
pub(crate) const LAND_S: f32 = 0.25;
const LAND_BRAKE: f32 = 8.0;
/// A body on its feet falling faster than this (m/s) for [`OPEN_AFTER_S`]
/// opens its wings: it walked off a ledge.
const OPEN_FALLING_M_S: f32 = 2.5;
const OPEN_AFTER_S: f32 = 0.2;

/// What a flying body is doing, and its attitude and stamina (#1861): on
/// every on-foot chassis, at [`FlightMode::Ground`] for a body that does not
/// fly.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Flight {
    mode: FlightMode,
    /// Heading (radians): 0 faces -Z, positive turns left, as a rotation
    /// about +Y does.
    heading: f32,
    /// Nose over level (radians).
    pitch: f32,
    /// Right wing down (radians).
    bank: f32,
    /// What is left of a full stamina, 0 to 1.
    stamina: f32,
    /// Out of stamina until it is back to [`EXHAUSTED_UNTIL`].
    exhausted: bool,
    /// Seconds since the last beat.
    since_beat: f32,
    /// Seconds in this mode.
    in_mode: f32,
    /// The wings have stalled.
    stalled: bool,
    /// Seconds a body on its feet has been falling, or a flare without its
    /// top, or a landing body without one.
    lost: f32,
    /// Beats since the mode began: the take-off's count.
    mode_beats: u32,
    /// Every beat, counted: what a pose will phase its wings by.
    beats: u32,
}

impl Default for Flight {
    fn default() -> Self {
        Self {
            mode: FlightMode::Ground,
            heading: 0.0,
            pitch: 0.0,
            bank: 0.0,
            stamina: 1.0,
            exhausted: false,
            since_beat: 1.0e3,
            in_mode: 0.0,
            stalled: false,
            lost: 0.0,
            mode_beats: 0,
            beats: 0,
        }
    }
}

/// What a peer's winged body is doing (#1861): the mode its last `Flight`
/// message named (`network::inbound`), kept on the peer's body for what draws
/// it. Absent on a body that has never said, which is on its feet.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PeerFlight(pub(crate) FlightMode);

/// The keys a flight reads on a step (#1861), written by the input-gated
/// [`read_flight_keys`] and spent by [`fly_on_foot`], so a step the keys
/// stood down on is a step with none.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct FlightKeys {
    /// Space pressed since the last step: take off, or beat.
    pub(crate) press: bool,
    /// Space held: keep beating.
    pub(crate) held: bool,
    /// Nose up (+1, S) or down (-1, W).
    pub(crate) pitch: i8,
    /// Bank right (+1, D) or left (-1, A).
    pub(crate) bank: i8,
    /// Shift: fold the wings and dive.
    pub(crate) fold: bool,
}

/// A flight's numbers, as one step flies them: the record's, in radians and
/// per-second shares.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Wings {
    power: f32,
    can_flap: bool,
    glide_ratio: f32,
    trim: f32,
    stall: f32,
    beat_speed: f32,
    beat_period: f32,
    /// Stamina one beat costs: a weaker flyer's beats tire it sooner.
    beat_cost: f32,
    /// Stamina gliding gives back, a second.
    recovery: f32,
    takeoff: f32,
    max_bank: f32,
    roll_rate: f32,
    pitch_rate: f32,
    /// The capture radius (m).
    capture: f32,
    /// How far the feet are under the chassis' origin (m).
    feet: f32,
    /// The capsule's radius (m).
    radius: f32,
}

impl Wings {
    pub(crate) fn of(flight: &FlightParams, body: &HumanoidParams) -> Self {
        let power = flight.flight_power.0.clamp(0.0, 1.0);
        let can_flap = flight.can_flap();
        let height = body.total_height();
        Self {
            power,
            can_flap,
            glide_ratio: flight.glide_ratio.0.max(1.0),
            trim: flight.trim_speed.0.max(1.0),
            stall: flight.stall_speed.0.max(0.5),
            beat_speed: flight.beat_speed.0.max(0.0),
            beat_period: 1.0 / flight.wingbeat_rate.0.max(0.5),
            beat_cost: if can_flap {
                1.0 / (flight.stamina_beats.0.max(1.0) * power)
            } else {
                f32::INFINITY
            },
            recovery: 1.0 / flight.stamina_recovery.0.max(0.5),
            takeoff: flight.takeoff_speed.0.max(0.0),
            max_bank: flight.max_bank.0.to_radians(),
            roll_rate: flight.roll_rate.0.to_radians(),
            pitch_rate: flight.pitch_rate.0.to_radians(),
            capture: (CAPTURE_OF_HEIGHT * height).max(body.capsule_radius.0),
            feet: height * 0.5,
            radius: body.capsule_radius.0,
        }
    }

    /// The nose a hands-off glide holds at `speed` (radians): the polar's
    /// glide angle at the trim speed, lower when it is slower and higher
    /// when it is faster, so it settles at its trim.
    pub(crate) fn trim_pitch(&self, speed: f32) -> f32 {
        let glide = -(1.0 / self.glide_ratio).asin();
        (glide + SPEED_TRIM_GAIN * (speed - self.trim) / self.trim)
            .clamp(TRIM_PITCH_DOWN, TRIM_PITCH_UP)
    }
}

/// A top a flight is closing on (#1861): how soon it reaches it, and where.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Approach {
    /// Time to contact (s): the distance along the path over the speed.
    pub(crate) tau: f32,
    /// Where it touches the top.
    pub(crate) point: Vec3,
}

/// What a flight reads of its body and the world on a step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Reading {
    pub(crate) position: Vec3,
    pub(crate) velocity: Vec3,
    /// The chassis' heading, as the walk left it (radians).
    pub(crate) facing: f32,
    /// Ground under its feet to leap off (`humanoid::feet_on_the_ground`).
    pub(crate) on_ground: bool,
    /// Resting on a top, or within [`NEAR_TOP_M`] over one.
    pub(crate) resting: bool,
    /// A top along its path.
    pub(crate) approach: Option<Approach>,
    /// In water deep enough to wade or swim: the walk's.
    pub(crate) in_water: bool,
    /// Gravity's pull (m/s^2).
    pub(crate) gravity: f32,
}

/// `value` moved toward `target` by at most `step`.
fn toward(value: f32, target: f32, step: f32) -> f32 {
    value + (target - value).clamp(-step, step)
}

/// The heading of a body turned `rotation` (radians, as [`Flight`] counts
/// it): the way its nose points over the ground.
pub(crate) fn heading_of(rotation: Quat) -> f32 {
    let forward = rotation * Vec3::NEG_Z;
    (-forward.x).atan2(-forward.z)
}

impl Flight {
    /// What it is doing.
    pub fn mode(&self) -> FlightMode {
        self.mode
    }

    /// What is left of its stamina, 0 to 1.
    // Read by the agent's `status`, which is Unix-only, and the tests.
    #[cfg_attr(not(unix), allow(dead_code))]
    pub fn stamina(&self) -> f32 {
        self.stamina
    }

    /// Every beat it has made.
    // Read by the agent's `status`, which is Unix-only, and the tests.
    #[cfg_attr(not(unix), allow(dead_code))]
    pub fn beats(&self) -> u32 {
        self.beats
    }

    /// Whether its wings have stalled.
    // Read by the agent's `status`, which is Unix-only, and the tests.
    #[cfg_attr(not(unix), allow(dead_code))]
    pub fn stalled(&self) -> bool {
        self.stalled
    }

    /// Its heading, nose and bank (radians): what a pose reads.
    // Read by the agent's `status`, which is Unix-only, and the tests.
    #[cfg_attr(not(unix), allow(dead_code))]
    pub fn attitude(&self) -> (f32, f32, f32) {
        (self.heading, self.pitch, self.bank)
    }

    /// The chassis rotation its attitude makes: turned to its heading, its
    /// nose up by its pitch, its right wing down by its bank.
    pub fn rotation(&self) -> Quat {
        Quat::from_euler(EulerRot::YXZ, self.heading, self.pitch, -self.bank)
    }

    fn nose(&self) -> Vec3 {
        self.rotation() * Vec3::NEG_Z
    }

    fn level_nose(&self) -> Vec3 {
        Quat::from_rotation_y(self.heading) * Vec3::NEG_Z
    }

    /// Put it in `mode`, as a flight there would be, for a test elsewhere in
    /// the crate.
    #[cfg(test)]
    pub(crate) fn set_mode_for_tests(&mut self, mode: FlightMode) {
        self.enter(mode);
    }

    fn enter(&mut self, mode: FlightMode) {
        if self.mode != mode {
            self.mode = mode;
            self.in_mode = 0.0;
            self.lost = 0.0;
            self.mode_beats = 0;
        }
    }

    /// Set down on its feet (#1861): a spawn, a recovery, a requested move or
    /// an arrival puts a flying body on the ground, as it does a rotorcraft.
    fn set_down(&mut self) {
        self.enter(FlightMode::Ground);
        self.pitch = 0.0;
        self.bank = 0.0;
        self.stalled = false;
    }

    fn recover(&mut self, rate: f32, dt: f32) {
        self.stamina = (self.stamina + rate * dt).min(1.0);
        if self.exhausted && self.stamina >= EXHAUSTED_UNTIL {
            self.exhausted = false;
        }
    }

    /// One beat: the push along its wings, for its stamina - or nothing, and
    /// exhausted, when it has too little left.
    fn beat(&mut self, wings: &Wings, velocity: Vec3) -> Vec3 {
        if !wings.can_flap {
            return velocity;
        }
        if self.stamina < wings.beat_cost {
            self.exhausted = true;
            return velocity;
        }
        self.stamina -= wings.beat_cost;
        self.since_beat = 0.0;
        self.mode_beats += 1;
        self.beats = self.beats.wrapping_add(1);
        let up = self.rotation() * Vec3::Y;
        let push = (up * BEAT_UP + self.nose() * BEAT_FORWARD).normalize_or(Vec3::Y);
        velocity + push * wings.beat_speed * wings.power
    }

    /// The mode the keys fly it in, off the ground and not landing.
    fn cruising(&self, wings: &Wings, keys: FlightKeys) -> FlightMode {
        if keys.fold {
            FlightMode::Dive
        } else if keys.held && wings.can_flap && !self.exhausted {
            FlightMode::Flap
        } else {
            FlightMode::Glide
        }
    }

    /// One fixed step of `dt` seconds: the state it moves to, and the
    /// velocity it gives the body - `None` on its feet, where the walk moves
    /// it. Pure, so the model is unit-testable without the physics.
    pub(crate) fn stepped(
        mut self,
        wings: &Wings,
        keys: FlightKeys,
        now: Reading,
        dt: f32,
    ) -> (Self, Option<Vec3>) {
        self.since_beat = (self.since_beat + dt).min(1.0e3);
        self.in_mode += dt;
        let mut velocity = now.velocity;
        if self.mode != FlightMode::Ground && now.in_water {
            // Into the water it swims, as the walk swims it.
            self.set_down();
            return (self, None);
        }
        match self.mode {
            FlightMode::Ground => return self.on_its_feet(wings, keys, now, dt),
            FlightMode::Land => return self.landing(wings, keys, now, dt),
            _ => {}
        }

        // Down on a top: it lands - unless it is climbing away from it, the
        // flap key held, as a rotorcraft does not land with its climb key
        // held (`helicopter::RotorLift`).
        let climbing = keys.held && wings.can_flap && !self.exhausted;
        let leaving = self.mode == FlightMode::Takeoff && self.in_mode < TAKEOFF_S;
        if now.resting && !leaving && !climbing {
            self.enter(FlightMode::Land);
            self.stalled = false;
            return self.landing(wings, keys, now, dt);
        }

        let speed = velocity.length();
        // The mode the keys and the ground ask for.
        match self.mode {
            FlightMode::Takeoff if self.in_mode >= TAKEOFF_S => {
                let next = self.cruising(wings, keys);
                self.enter(next);
            }
            FlightMode::Flare => {
                if keys.press && wings.can_flap && !self.exhausted {
                    // Round again.
                    self.enter(FlightMode::Flap);
                } else {
                    let close = now.approach.is_some_and(|a| a.tau <= FLARE_ABORT_TAU_S);
                    self.lost = if close { 0.0 } else { self.lost + dt };
                    if self.lost >= FLARE_LOST_S {
                        let next = self.cruising(wings, keys);
                        self.enter(next);
                    }
                }
            }
            FlightMode::Flap | FlightMode::Glide | FlightMode::Dive => {
                let flare = !keys.held && now.approach.is_some_and(|a| a.tau <= FLARE_TAU_S);
                let next = if flare {
                    FlightMode::Flare
                } else {
                    self.cruising(wings, keys)
                };
                self.enter(next);
            }
            _ => {}
        }
        let mode = self.mode;

        // The stall: never on the leap, nor in the flare, which is one.
        self.stalled = match mode {
            FlightMode::Takeoff | FlightMode::Flare => false,
            _ if self.stalled => speed < wings.stall * STALL_RECOVER,
            _ => speed < wings.stall,
        };

        // The attitude.
        let bank_to = match mode {
            FlightMode::Takeoff | FlightMode::Flap | FlightMode::Glide if !self.stalled => {
                f32::from(keys.bank) * wings.max_bank
            }
            _ => 0.0,
        };
        self.bank = toward(self.bank, bank_to, wings.roll_rate * dt);
        let (pitch_to, pitch_rate) = match mode {
            FlightMode::Flare => (FLARE_PITCH, 2.0 * wings.pitch_rate),
            FlightMode::Dive => (DIVE_PITCH, wings.pitch_rate),
            _ if self.stalled => (STALL_PITCH, 1.5 * wings.pitch_rate),
            _ => {
                let hands_off = match mode {
                    FlightMode::Takeoff => TAKEOFF_PITCH,
                    // Beating, the nose rides over the glide's: the beats
                    // carry it up its path rather than down.
                    FlightMode::Flap => wings.trim_pitch(speed) + FLAP_CLIMB_PITCH,
                    _ => wings.trim_pitch(speed),
                };
                let to = match keys.pitch.signum() {
                    1 => PULL_PITCH,
                    -1 => (hands_off - PUSH_PITCH).max(PUSH_PITCH_MOST),
                    _ => hands_off,
                };
                (to, wings.pitch_rate)
            }
        };
        self.pitch = toward(self.pitch, pitch_to, pitch_rate * dt);
        if !matches!(mode, FlightMode::Dive | FlightMode::Flare) {
            // A coordinated turn: the bank's own rate, right wing down
            // turning right.
            let rate = now.gravity * self.bank.tan() / speed.max(wings.stall);
            self.heading = (self.heading - rate * dt).rem_euclid(std::f32::consts::TAU);
        }

        // The beats, and the stamina.
        match mode {
            FlightMode::Takeoff => {
                if wings.can_flap
                    && self.mode_beats < TAKEOFF_BEATS
                    && self.since_beat >= wings.beat_period
                {
                    velocity = self.beat(wings, velocity);
                }
            }
            FlightMode::Flap => {
                let pressed = keys.press && self.since_beat >= PRESS_BEAT_SHARE * wings.beat_period;
                if pressed || self.since_beat >= wings.beat_period {
                    velocity = self.beat(wings, velocity);
                }
            }
            _ => self.recover(wings.recovery, dt),
        }

        velocity = self.air(wings, mode, velocity, now, dt);
        (self, Some(velocity))
    }

    /// On its feet: stamina comes back, Space takes off, and a fall opens
    /// the wings. The walk moves it.
    fn on_its_feet(
        mut self,
        wings: &Wings,
        keys: FlightKeys,
        now: Reading,
        dt: f32,
    ) -> (Self, Option<Vec3>) {
        self.recover(wings.recovery * GROUND_RECOVERY, dt);
        let mut velocity = now.velocity;
        if keys.press && now.on_ground {
            self.heading = now.facing;
            self.pitch = TAKEOFF_PITCH;
            self.bank = 0.0;
            self.enter(FlightMode::Takeoff);
            let leap = wings.takeoff * if wings.can_flap { 1.0 } else { GLIDER_LEAP };
            velocity += (self.level_nose() * LEAP_FORWARD + Vec3::Y * LEAP_UP) * leap;
            velocity = self.beat(wings, velocity);
            return (self, Some(velocity));
        }
        let footing = now.on_ground || now.resting;
        self.lost = if footing || velocity.y > -OPEN_FALLING_M_S {
            0.0
        } else {
            self.lost + dt
        };
        if !footing && (keys.press || self.lost >= OPEN_AFTER_S) {
            // Off a ledge, or Space in the air: the wings open.
            self.heading = now.facing;
            self.pitch = wings.trim_pitch(velocity.length());
            self.bank = 0.0;
            let next = if keys.press && wings.can_flap && !self.exhausted {
                FlightMode::Flap
            } else {
                FlightMode::Glide
            };
            self.enter(next);
            if next == FlightMode::Flap {
                velocity = self.beat(wings, velocity);
            }
            return (self, Some(velocity));
        }
        (self, None)
    }

    /// Down on a top, settling: its speed over the ground and its attitude
    /// taken away, then on its feet. A body that drops off again flies.
    fn landing(
        mut self,
        wings: &Wings,
        keys: FlightKeys,
        now: Reading,
        dt: f32,
    ) -> (Self, Option<Vec3>) {
        self.recover(wings.recovery * GROUND_RECOVERY, dt);
        let level = 3.0 * wings.pitch_rate.max(wings.roll_rate) * dt;
        self.pitch = toward(self.pitch, 0.0, level);
        self.bank = toward(self.bank, 0.0, level);
        let mut velocity = now.velocity;
        let keep = (-LAND_BRAKE * dt).exp();
        velocity.x *= keep;
        velocity.z *= keep;
        let footing = now.on_ground || now.resting;
        self.lost = if footing { 0.0 } else { self.lost + dt };
        if self.lost >= OPEN_AFTER_S && velocity.y < -OPEN_FALLING_M_S {
            let next = self.cruising(wings, keys);
            self.enter(next);
        } else if self.in_mode >= LAND_S {
            self.set_down();
        }
        (self, Some(velocity))
    }

    /// The air's part of a step: lift that turns the path toward where the
    /// body is flying and holds it up, no more than its speed allows, and the
    /// polar's drag. Gravity is the physics'.
    fn air(&self, wings: &Wings, mode: FlightMode, velocity: Vec3, now: Reading, dt: f32) -> Vec3 {
        let g = now.gravity;
        let speed = velocity.length();
        let along = if speed > 0.1 {
            velocity / speed
        } else {
            self.nose()
        };
        // Where the path is drawn: the nose - or, in the flare, the point it
        // lands on, while the nose rises over it.
        let aim = match (mode, now.approach) {
            (FlightMode::Flare, Some(approach)) => {
                let feet = now.position - Vec3::Y * wings.feet;
                (approach.point - feet).normalize_or(along)
            }
            (FlightMode::Flare, None) => along.with_y(0.0).normalize_or(along),
            _ => self.nose(),
        };
        let share = match mode {
            FlightMode::Dive => FOLDED_LIFT,
            FlightMode::Flare => FLARE_LIFT,
            _ => 1.0,
        };
        let most = share * (speed / wings.stall).powi(2) * g;
        let weight = Vec3::NEG_Y * g;
        let held_up = -(weight - along * weight.dot(along));
        let turned = (aim - along * aim.dot(along)) * PATH_GAIN * speed;
        let lift = (held_up + turned).clamp_length_max(most);
        // The polar: least at the trim speed, half from the speed and half
        // from what the lift costs.
        let load = lift.length() / g.max(1e-3);
        let fast = (speed / wings.trim).powi(2);
        let slow = (wings.trim / speed.max(0.5 * wings.stall)).powi(2);
        let polar = g / (2.0 * wings.glide_ratio);
        let drag = match mode {
            FlightMode::Dive => polar * FOLDED_DRAG * fast,
            FlightMode::Flare => polar * FLARE_DRAG * (fast + load * load * slow),
            _ => polar * (fast + load * load * slow),
        };
        // Drag slows the body; it never turns it round.
        let drag = drag.min(speed / dt.max(1e-4));
        velocity + (lift - along * drag) * dt
    }
}

/// Set a flying body down (#1861): whatever it was doing, a body put on the
/// ground - by a spawn, a recovery, a requested move or an arrival - is on
/// its feet. A body of any other preset carries no [`Flight`] and is left as
/// it is.
pub(crate) fn set_down(flight: Option<Mut<Flight>>) {
    if let Some(mut flight) = flight
        && flight.mode != FlightMode::Ground
    {
        flight.set_down();
    }
}

/// Read the keys into the body's [`FlightKeys`] - the one flight system the
/// owner's typing stands down (#821). Space's press is the jump latch's
/// (`humanoid::JumpQueued`, #852), so a tap is seen by exactly one step.
pub(super) fn read_flight_keys(
    live: Res<LiveAvatarRecord>,
    keyboard: Res<ButtonInput<KeyCode>>,
    jump_queued: Res<JumpQueued>,
    mut query: Query<&mut FlightKeys, (With<LocalPlayer>, With<HumanoidPreset>)>,
) {
    let LocomotionConfig::Humanoid(p) = &live.0.locomotion else {
        return;
    };
    if !p.flies() {
        return;
    }
    let Ok(mut keys) = query.single_mut() else {
        return;
    };
    let held = |keys: &[KeyCode]| keys.iter().any(|key| keyboard.pressed(*key));
    let axis = |plus: bool, minus: bool| i8::from(plus) - i8::from(minus);
    let next = FlightKeys {
        press: jump_queued.0,
        held: keyboard.pressed(KeyCode::Space),
        pitch: axis(
            held(&[KeyCode::KeyS, KeyCode::ArrowDown]),
            held(&[KeyCode::KeyW, KeyCode::ArrowUp]),
        ),
        bank: axis(
            held(&[KeyCode::KeyD, KeyCode::ArrowRight]),
            held(&[KeyCode::KeyA, KeyCode::ArrowLeft]),
        ),
        fold: held(&[KeyCode::ShiftLeft, KeyCode::ShiftRight]),
    };
    // Guarded: a `&mut` through a query stamps a change tick even when the
    // value is the same.
    if *keys != next {
        *keys = next;
    }
}

/// The local body's flight, every fixed step (#1861): its mode, its attitude
/// written onto the chassis, and the velocity the air gives it - see the
/// module docs. On its feet it only watches for a take-off or a fall; the
/// walk moves it.
///
/// Ungated on the owner's typing (#821), its keys spent all the same;
/// parked during the editor's freeze (#814). During travel no key counts.
/// The chassis a flight steps: what it reads and what it writes.
type FlyingBody<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Position,
        &'static mut Rotation,
        &'static mut LinearVelocity,
        &'static mut Flight,
        &'static mut FlightKeys,
    ),
    (
        With<LocalPlayer>,
        With<HumanoidPreset>,
        Without<VisualsEditFreeze>,
    ),
>;

#[allow(clippy::too_many_arguments)]
pub(super) fn fly_on_foot(
    live: Res<LiveAvatarRecord>,
    time: Res<Time>,
    water_surfaces: Res<WaterSurfaces>,
    contacts: Option<Res<ContactGraph>>,
    sensors: Query<Entity, With<Sensor>>,
    gravity: Option<Res<Gravity>>,
    traveling: Option<Res<TravelingTo>>,
    // The world's colliders, then the body: the casts read every collider's
    // rotation, which the flight writes on its own.
    mut world: ParamSet<(SpatialQuery, FlyingBody)>,
) {
    let LocomotionConfig::Humanoid(p) = &live.0.locomotion else {
        return;
    };
    let (body, at, rotated, moving, flight, held) = {
        let mut bodies = world.p1();
        let Ok((body, position, rotation, velocity, flight, mut keys)) = bodies.single_mut() else {
            return;
        };
        // Spent: a step the reader stood down on is a step with none.
        let held = std::mem::take(&mut *keys.bypass_change_detection());
        (body, position.0, rotation.0, velocity.0, *flight, held)
    };
    let keys_now = if traveling.is_some() {
        FlightKeys::default()
    } else {
        held
    };
    let Some(params) = p.flight.as_ref() else {
        // The record stopped flying: on its feet, standing up.
        if flight.mode != FlightMode::Ground {
            let mut grounded = flight;
            grounded.set_down();
            let mut bodies = world.p1();
            if let Ok((_, _, mut rotation, _, mut flying, _)) = bodies.single_mut() {
                *flying = grounded;
                rotation.0 = Quat::from_rotation_y(grounded.heading);
            }
        }
        return;
    };
    let wings = Wings::of(params, p);
    let pull = gravity
        .as_deref()
        .map_or(Gravity::default().0, |gravity| gravity.0);
    let filter = super::ground_ray_filter(body, sensors.iter());
    let (on_ground, near_top, approach) = {
        let spatial = world.p0();
        let on_ground =
            super::humanoid::feet_on_the_ground(&spatial, at, p.total_height(), &filter);
        let near_top = top_under(&spatial, at, &wings, &filter);
        let approach = (flight.mode.airborne() && flight.mode != FlightMode::Land)
            .then(|| approach(&spatial, at, moving, &wings, &filter))
            .flatten();
        (on_ground, near_top, approach)
    };
    let touching = contacts
        .as_deref()
        .is_some_and(|contacts| super::resting_on(contacts, &sensors, body, LAND_NORMAL_Y));
    let in_water = water_state(
        false,
        at.y,
        Vec2::new(at.x, at.z),
        WaterColumn::of(p),
        &water_surfaces,
    ) != WaterState::Dry;
    let now = Reading {
        position: at,
        velocity: moving,
        facing: heading_of(rotated),
        on_ground,
        resting: touching || near_top,
        approach,
        in_water,
        gravity: -pull.y,
    };
    let was = flight.mode;
    let (next, new_velocity) = flight.stepped(&wings, keys_now, now, time.delta_secs());
    let mut bodies = world.p1();
    let Ok((_, _, mut rotation, mut velocity, mut flying, _)) = bodies.single_mut() else {
        return;
    };
    // Each written only when it changes: a `&mut` through a query stamps a
    // change tick even when the value is the same.
    if *flying != next {
        *flying = next;
    }
    if let Some(v) = new_velocity
        && v != velocity.0
        && v.is_finite()
    {
        velocity.0 = v;
    }
    // The attitude is the flight's while it is off its feet, and stood
    // upright the step it is back on them; the walk turns it from there.
    if next.mode.airborne() || was.airborne() {
        let turned = if next.mode.airborne() {
            next.rotation()
        } else {
            Quat::from_rotation_y(next.heading)
        };
        // The physics' own rotation, which it writes back to the transform
        // after its step, as a respawn's set-down writes it.
        if !turned.abs_diff_eq(rotation.0, 1e-6) {
            rotation.0 = turned;
        }
    }
}

/// Whether a top stands within [`NEAR_TOP_M`] under the feet of a body at
/// `at`: a rotorcraft's near-ground reading, with the normal a landing takes.
fn top_under(spatial: &SpatialQuery, at: Vec3, wings: &Wings, filter: &SpatialQueryFilter) -> bool {
    let radius = 0.05;
    let origin = at - Vec3::Y * (wings.feet - radius - 0.02);
    let config = ShapeCastConfig {
        ignore_origin_penetration: true,
        ..ShapeCastConfig::from_max_distance(NEAR_TOP_M + 0.02)
    };
    spatial
        .cast_shape(
            &Collider::sphere(radius),
            origin,
            Quat::IDENTITY,
            Dir3::NEG_Y,
            &config,
            filter,
        )
        .is_some_and(|hit| hit.normal1.y >= LAND_NORMAL_Y)
}

/// The top a body at `at` moving at `velocity` closes on, within its capture
/// radius of its path and [`LOOK_AHEAD_S`] of flight: a sphere of the capture
/// radius cast from its feet along its path, which a top's surface stops.
fn approach(
    spatial: &SpatialQuery,
    at: Vec3,
    velocity: Vec3,
    wings: &Wings,
    filter: &SpatialQueryFilter,
) -> Option<Approach> {
    let speed = velocity.length();
    let along = Dir3::new(velocity).ok()?;
    if velocity.y >= 0.0 || speed < 0.5 {
        return None;
    }
    // The sphere's bottom at the feet.
    let origin = at - Vec3::Y * (wings.feet - wings.capture);
    let config = ShapeCastConfig {
        ignore_origin_penetration: true,
        ..ShapeCastConfig::from_max_distance(speed * LOOK_AHEAD_S)
    };
    let hit = spatial.cast_shape(
        &Collider::sphere(wings.capture),
        origin,
        Quat::IDENTITY,
        along,
        &config,
        filter,
    )?;
    // Where it touches, moved in over the top by the body's own radius: a
    // sphere passing an edge touches the edge, and a body set down on the
    // edge would stand half off it. From the sphere's middle the touch lies
    // toward the top's inside; under it, it moves nothing.
    let middle = origin + along * hit.distance;
    let inward = (hit.point1 - middle).with_y(0.0).normalize_or_zero();
    (hit.normal1.y >= LAND_NORMAL_Y).then(|| Approach {
        tau: hit.distance / speed,
        point: hit.point1 + inward * wings.radius,
    })
}
