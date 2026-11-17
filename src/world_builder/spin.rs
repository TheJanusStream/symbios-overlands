//! The runtime of a node's spin (#1604), slide and swell (#1935): each
//! client turning, moving and breathing the parts a record says do, for
//! itself, without a byte on the wire.
//!
//! The spawner gives every node whose [`Spin`], [`Slide`] or [`Swell`]
//! moves a [`Spinner`], holding the node's authored local pose and its
//! terms in radians, metres and seconds. One component writes all three
//! channels - the turn into the rotation, the slide into the translation,
//! the swell into the scale - from one clock and one copy offset, so two
//! channels of one node keep their phase (a spin's swing and a slide's of
//! one period peak together) and nothing else fights over the `Transform`.
//! Two systems then run every frame, around transform propagation:
//!
//! - [`animate_spinners`], before it, writes each spinner's `Transform` as
//!   its authored pose turned about the node's axis by the sum of its turn
//!   terms, moved along its slide axis - in the node's frame after its
//!   authored rotation, in metres whatever its own scale (an ancestor's
//!   scale stretches it, as it does every offset below it) - by the sum of its
//!   slide terms, and scaled per axis by its swell (see [`SwellTerm`]).
//!   Clock terms ([`SpinTerm::Constant`], [`SpinTerm::Swing`],
//!   [`SpinTerm::Wobble`]) are a function of the time - UTC in the game, so
//!   every client shows a windmill at the same angle and a spinner skipped
//!   while out of sight comes back at the right one. Wind terms
//!   ([`SpinTerm::Wind`], [`SpinTerm::Vane`]) add the room's wind - the
//!   foliage's own [`VegetationWind`](crate::wind::VegetationWind) - to the
//!   clock. Motion terms ([`SpinTerm::Roll`], [`SpinTerm::Steer`],
//!   [`SpinTerm::Speed`], [`SpinTerm::Lean`]) integrate what
//!   [`sample_spinner_motion`] measured.
//! - [`sample_spinner_motion`], after it, differences each motion or wind
//!   spinner's world position, and its parent's world rotation, over the
//!   frame they spanned, and records how the part stands in the world.
//!   After propagation is the only place that delta is honest for a remote
//!   peer, whose pose propagates only in `PostUpdate` (#1323). Every part is
//!   measured as drawn, its own slide included, so a slide and a lean on one
//!   node move as the same slide on a carrier with the lean below it. There
//!   is no telling a jump from travel: a belt's jump back reads as one fast
//!   frame of travel, which a roll, speed or lean term on or under the belt
//!   part feels - the roll rate cap and the lean's bound only limit how much.
//!   The slide's and the swell's wind terms read the room's wind as a turn's
//!   do, and are sampled for it.
//!
//! **Selection freezes the world.** While an editor has anything selected
//! ([`SpinHold`], mirrored from the editors), every spinner stands at its
//! authored pose: a gizmo bakes its target's world pose when it attaches
//! and converts world to local against the parent's on commit, so a part
//! caught mid-turn - or hanging below one - would carry the turn into the
//! record. The freeze snaps once, on the frame it starts, and then writes
//! nothing, so the gizmo owns the pose for as long as the selection lasts.
//! [`SpinSettled`] tells the gizmo when the snap has propagated, and the
//! gizmo attaches no sooner. The hold outlasts the selection until the gizmo
//! has let go of what it held (see `ui::room::mirror_spin_hold`), and then
//! each spinner takes the pose it was left at as its new authored one, so a
//! drag that was committed turns from where it was put, even before the
//! recompile arrives - its place and size as well as its turn. Settings >
//! Effects intensity Off holds every spinner the same way, and Reduced moves
//! every slide and swell half as far ([`REDUCED_MOTION_SHARE`]), as it calms
//! a light's flicker - except a belt, which runs at half speed over its
//! whole length, so its jump back stays where the run hides it; a turn keeps
//! its authored motion at Reduced.
//!
//! A spinner that has lost its parent is the gizmo's - it detaches its
//! target into world space - and is never written.

#[cfg(test)]
use std::f32::consts::FRAC_PI_2;
use std::f32::consts::{PI, TAU};

use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crate::pds::Generator;
use crate::pds::sanitize::limits;
use crate::pds::slide::{Slide, SlideTerm};
use crate::pds::spin::{Spin, SpinTerm};
use crate::pds::swell::{Swell, SwellTerm, axis_factors};

/// The span a copy's time offset is drawn from (s): see [`copy_epoch`].
const COPY_EPOCH_SPAN_S: f64 = 3600.0;

/// Farther than this from every active camera, a spinner holds its pose
/// (m) - nobody is close enough to see a part turn, and an unturned part
/// costs no transform propagation. Past the orbit camera's 200 m zoom limit
/// with room for what it looks at.
pub(crate) const SPIN_RANGE_M: f32 = 500.0;

/// How quickly a steer term follows its target (s): the time constant of
/// its settling, short enough to read as the wheels following the turn and
/// long enough to smooth the frame-to-frame jitter of a measured turn rate.
const STEER_SETTLE_S: f32 = 0.12;

/// Backwards faster than this (m/s, along the parent's +Z), a steer term
/// steers the other way - see [`SpinTerm::Steer`]. Slower, a vehicle
/// creeping or standing still keeps the forward sense, so the wheels do not
/// flick across as it stops.
const REVERSING_M_S: f32 = 0.3;

/// A lean term's spring damping ratio: under-damped, so a hanging lantern
/// swings past its lean and back a few times before it settles.
const LEAN_DAMPING: f32 = 0.3;

/// The furthest a lean term tips its part (degrees) - its target, and its
/// swing past the target too. The chassis takes its velocity straight from
/// the drive, so its measured acceleration can hold far past anything a
/// pendulum would answer for frames together, and an under-damped spring
/// overshoots its target by a third.
const MAX_LEAN_DEG: f32 = 60.0;

/// The longest step a lean term's spring takes (s): a frame longer than this
/// is integrated in several, or a stiff spring would blow up across a hitch.
const LEAN_STEP_S: f32 = 1.0 / 120.0;

/// The wind speed a wind term's amplitude is authored at (m/s) - the
/// default room's, which drifts the clouds at 4 m/s.
const REFERENCE_WIND_M_S: f32 = 4.0;

/// How far a vane hunts either side of downwind in the reference wind
/// (degrees).
const VANE_HUNT_DEG: f32 = 5.0;

/// How far a slide moves and a swell breathes at Settings > Effects
/// intensity Reduced, as a share of what was authored: half, as a light's
/// flicker is calmed (`world_builder::light`). A belt runs at this share of
/// its speed instead, its length kept. A turn is left as authored.
const REDUCED_MOTION_SHARE: f32 = 0.5;

/// Register the spin systems and their resources on `app`, with the clock
/// its spinners read. The game passes [`SpinClock::utc`]; the headless
/// render tool passes [`SpinClock::Scene`], whose frames are the same on
/// every run.
pub(crate) fn register(app: &mut App, clock: SpinClock) {
    app.insert_resource(clock)
        .init_resource::<SpinHold>()
        .init_resource::<SpinSettled>()
        .add_systems(
            PostUpdate,
            (
                animate_spinners.before(TransformSystems::Propagate),
                sample_spinner_motion.after(TransformSystems::Propagate),
            ),
        );
}

/// Whether an editor has something selected that a gizmo may move, or a
/// gizmo has not yet let go of what it held (#1604): every spinner then
/// stands at its authored pose.
///
/// Mirrored once a frame in `PreUpdate` by `ui::room::mirror_spin_hold`,
/// the only writer, from the same rule the gizmo attaches by - were the two
/// to disagree, the gizmo could wait on a freeze that never came. Absent
/// editor state (before login, the render tool) reads as nothing selected.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpinHold {
    pub editing: bool,
}

/// Whether every spinner stands at its authored pose as this frame's
/// transforms propagate: what the editor gizmo waits for before it bakes a
/// new selection into world space. True with nothing spinning, so a world
/// without spinners attaches a gizmo the frame it is asked to.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpinSettled(pub bool);

impl Default for SpinSettled {
    fn default() -> Self {
        Self(true)
    }
}

/// The time the clock terms read.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub enum SpinClock {
    /// Wall-clock UTC (s), so every client turns a part to the same angle
    /// with nothing exchanged: read as the offset of UTC from the real
    /// (never paused, never clamped) app clock, taken on the first frame.
    Utc { offset: Option<f64> },
    /// The app's own clock from zero - the headless render tool's, which it
    /// steps by hand so a clip's frame `k` is the scene at `k / fps`.
    Scene,
}

impl SpinClock {
    /// The game's clock, its offset taken on the first frame.
    pub(crate) fn utc() -> Self {
        Self::Utc { offset: None }
    }

    /// The time now (s) - also the clock a light's flicker reads
    /// (`world_builder::light`).
    pub(crate) fn now(&mut self, time: &Time, real: &Time<Real>) -> f64 {
        match self {
            Self::Scene => time.elapsed_secs_f64(),
            Self::Utc { offset } => {
                let offset = *offset.get_or_insert_with(|| {
                    let micros = chrono::Utc::now().timestamp_micros();
                    micros as f64 / 1e6 - real.elapsed_secs_f64()
                });
                offset + real.elapsed_secs_f64()
            }
        }
    }
}

/// The time offset of one copy of a placed generator (s): every spinner of
/// one copy shares it, so a pair of meshing cogs stays meshed, and the
/// copies of a scatter differ, so a field of windmills does not turn in
/// lockstep. Derived from the record alone - the placement's index and the
/// copy's offset across the ground, to the centimetre - so every client
/// derives the same one.
pub(crate) fn copy_epoch(placement: usize, offset: Vec3) -> f64 {
    let centimetres = |v: f32| (v * 100.0).round() as i64 as u64;
    let mut h = splitmix64(placement as u64);
    h = splitmix64(h ^ centimetres(offset.x));
    h = splitmix64(h ^ centimetres(offset.z));
    (h >> 11) as f64 / (1u64 << 53) as f64 * COPY_EPOCH_SPAN_S
}

fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// One term, in the units the runtime turns by - and, for a lean, the state
/// of its spring.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Turn {
    /// Radians per second, in f64: at UTC's ~1.8e9 s, an f32 rate's own
    /// rounding (90 deg/s is 4e-8 rad/s off a quarter turn) grows into
    /// tens of radians.
    Constant(f64),
    /// Radians either side, cycles per second, starting phase in radians.
    Swing {
        amplitude: f32,
        per_second: f64,
        phase: f32,
    },
    /// One over the radius (1/m).
    Roll { per_metre: f32 },
    /// Radians of turn per radian per second of the parent's turning, the
    /// radians it is held within, and whether that turning is read about the
    /// parent's own up rather than the node's axis (#1653).
    Steer {
        gain: f32,
        limit: f32,
        follow_yaw: bool,
    },
    /// Radians per second standing still, and radians per metre travelled
    /// forward.
    Speed { idle: f32, per_metre: f32 },
    /// Radians per m/s^2 across the axis, the spring's angular frequency
    /// (rad/s), and the spring's angle (rad) and rate (rad/s).
    Lean {
        gain: f32,
        omega: f32,
        angle: f32,
        rate: f32,
    },
    /// Radians of sway in the reference wind crossing the axis squarely.
    Wind { amplitude: f32 },
    /// Radians either side, cycles per second of its slowest swing.
    Wobble { amplitude: f32, per_second: f64 },
    /// The unit direction, in the part's own frame, that points downwind.
    Vane { facing: Vec3 },
}

impl Turn {
    fn from_term(term: &SpinTerm) -> Option<Self> {
        if !term.moves() {
            return None;
        }
        Some(match term {
            SpinTerm::Constant { rate } => Self::Constant((rate.0 as f64).to_radians()),
            SpinTerm::Swing {
                amplitude,
                period,
                phase,
            } => Self::Swing {
                amplitude: amplitude.0.to_radians(),
                per_second: 1.0 / period.0 as f64,
                phase: phase.0.to_radians(),
            },
            SpinTerm::Roll { radius } => Self::Roll {
                per_metre: radius.0.recip(),
            },
            SpinTerm::Steer {
                gain,
                limit,
                follow_yaw,
            } => Self::Steer {
                gain: gain.0,
                limit: limit.0.to_radians(),
                follow_yaw: *follow_yaw,
            },
            SpinTerm::Speed { idle, gain } => Self::Speed {
                idle: idle.0.to_radians(),
                per_metre: gain.0.to_radians(),
            },
            SpinTerm::Lean { gain, period } => Self::Lean {
                gain: gain.0.to_radians(),
                omega: TAU / period.0,
                angle: 0.0,
                rate: 0.0,
            },
            SpinTerm::Wind { amplitude } => Self::Wind {
                amplitude: amplitude.0.to_radians(),
            },
            SpinTerm::Wobble { amplitude, period } => Self::Wobble {
                amplitude: amplitude.0.to_radians(),
                per_second: 1.0 / period.0 as f64,
            },
            SpinTerm::Vane { facing } => Self::Vane {
                facing: Vec3::from_array(facing.0).try_normalize()?,
            },
            SpinTerm::Unknown => return None,
        })
    }

    /// Back to rest: a lean's spring hangs straight again.
    fn settle(&mut self) {
        if let Self::Lean { angle, rate, .. } = self {
            *angle = 0.0;
            *rate = 0.0;
        }
    }
}

/// One slide term, in the units the runtime moves by.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Shift {
    /// Metres either side, cycles per second, starting phase in radians.
    Swing {
        amplitude: f32,
        per_second: f64,
        phase: f32,
    },
    /// Metres either side, cycles per second of its slowest swing.
    Wobble { amplitude: f32, per_second: f64 },
    /// Metres along the axis in the reference wind blowing along it.
    Wind { amplitude: f32 },
    /// Metres at the top of a hop, hops per second, starting phase in
    /// radians.
    Bounce {
        height: f32,
        per_second: f64,
        phase: f32,
    },
    /// Metres per second, metres before the jump back and metres along the
    /// run it starts, in f64: at UTC's ~1.8e9 s an f32 product has lost
    /// every digit that places the part.
    Belt { speed: f64, length: f64, start: f64 },
}

impl Shift {
    fn from_term(term: &SlideTerm) -> Option<Self> {
        if !term.moves() {
            return None;
        }
        Some(match term {
            SlideTerm::Swing {
                amplitude,
                period,
                phase,
            } => Self::Swing {
                amplitude: amplitude.0,
                per_second: 1.0 / period.0 as f64,
                phase: phase.0.to_radians(),
            },
            SlideTerm::Wobble { amplitude, period } => Self::Wobble {
                amplitude: amplitude.0,
                per_second: 1.0 / period.0 as f64,
            },
            SlideTerm::Wind { amplitude } => Self::Wind {
                amplitude: amplitude.0,
            },
            SlideTerm::Bounce {
                height,
                period,
                phase,
            } => Self::Bounce {
                height: height.0,
                per_second: 1.0 / period.0 as f64,
                phase: phase.0.to_radians(),
            },
            SlideTerm::Belt {
                speed,
                length,
                start,
            } => Self::Belt {
                speed: speed.0 as f64,
                length: length.0 as f64,
                start: start.0 as f64,
            },
            SlideTerm::Unknown => return None,
        })
    }
}

/// One swell term, in the units the runtime breathes by: amounts are shares
/// of the authored size.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Breath {
    /// The amount either side, cycles per second, starting phase in radians.
    Swing {
        amount: f32,
        per_second: f64,
        phase: f32,
    },
    /// The amount either side, cycles per second of its slowest swing.
    Wobble { amount: f32, per_second: f64 },
    /// The amount at the top of a gust in the reference wind.
    Wind { amount: f32 },
}

impl Breath {
    fn from_term(term: &SwellTerm) -> Option<Self> {
        if !term.moves() {
            return None;
        }
        Some(match term {
            SwellTerm::Swing {
                amount,
                period,
                phase,
            } => Self::Swing {
                amount: amount.0,
                per_second: 1.0 / period.0 as f64,
                phase: phase.0.to_radians(),
            },
            SwellTerm::Wobble { amount, period } => Self::Wobble {
                amount: amount.0,
                per_second: 1.0 / period.0 as f64,
            },
            SwellTerm::Wind { amount } => Self::Wind { amount: amount.0 },
            SwellTerm::Unknown => return None,
        })
    }
}

/// A swing's value at `t` (s): `amplitude` either side, `per_second` cycles
/// a second, starting `phase` radians in - the one formula a turn's, a
/// slide's and a swell's swing share, so channels of one node keep phase.
fn swing(amplitude: f32, per_second: f64, phase: f32, t: f64) -> f64 {
    let cycle = (t * per_second).rem_euclid(1.0);
    amplitude as f64 * (std::f64::consts::TAU * cycle + phase as f64).sin()
}

/// A hop's height at `t` (s): `height` at the top, back to nothing once a
/// hop, `per_second` hops a second, starting `phase` radians in.
fn bounce(height: f32, per_second: f64, phase: f32, t: f64) -> f64 {
    let cycle = (t * per_second).rem_euclid(1.0);
    height as f64 * (std::f64::consts::PI * cycle + phase as f64).sin().abs()
}

/// A belt's offset at `t` (s): `speed x t + start` wrapped into
/// `[0, length)` - held under `length` after the f32 cast too, whose
/// rounding could carry a value a hair short of it onto it.
fn belt(speed: f64, length: f64, start: f64, t: f64) -> f32 {
    let wrapped = (speed * t + start).rem_euclid(length) as f32;
    if wrapped < length as f32 {
        wrapped
    } else {
        0.0
    }
}

/// Three incommensurate swings summed, scaled to stay within one either way
/// (their amplitudes sum to 1.95): a wander that never settles into a beat.
/// The two faster swings run at the golden and the silver ratio of the
/// slowest, numbers no fraction approximates well, so the three never line
/// up again. `cycles` counts the slowest swing's cycles - some 6e8 by the
/// wall clock, which f64 still holds to a ten-millionth of a cycle, so it is
/// never reduced and the wander never jumps.
fn wobble(cycles: f64) -> f32 {
    use std::f64::consts::{GOLDEN_RATIO, SQRT_2, TAU as TAU64};
    let w = (TAU64 * cycles).sin()
        + 0.6 * (TAU64 * GOLDEN_RATIO * cycles + 1.3).sin()
        + 0.35 * (TAU64 * (1.0 + SQRT_2) * cycles + 4.1).sin();
    (w / 1.95) as f32
}

/// The room's wind as a spinner reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct RoomWind {
    /// Unit downwind direction across the ground, or zero in a still room.
    downwind: Vec3,
    /// Wind speed (m/s).
    speed: f32,
}

impl RoomWind {
    fn of(wind: &crate::wind::VegetationWind) -> Self {
        let dir = Vec3::new(wind.dir.x, 0.0, wind.dir.y).normalize_or_zero();
        let speed = if wind.speed.is_finite() {
            wind.speed.max(0.0)
        } else {
            0.0
        };
        Self {
            downwind: if speed > 0.0 { dir } else { Vec3::ZERO },
            speed,
        }
    }

    /// How hard it blows, against the speed a wind term is authored at.
    fn scale(&self) -> f32 {
        (self.speed / REFERENCE_WIND_M_S).min(limits::MAX_WIND_SCALE)
    }

    /// A bend at `at` and `now` with the foliage's pace, `speed x 0.35`
    /// radians a second, and its gust front, which reaches a point later the
    /// further downwind it stands - so a sign sways with the trees' rhythm,
    /// though never in step with them: the shader runs on its own clock and
    /// phases each plant by a hash. `phase` keeps two terms of one part from
    /// bending in step.
    fn bend(&self, now: f64, at: Vec3, phase: f32) -> f32 {
        let pace = (now * self.speed as f64 * 0.35).rem_euclid(std::f64::consts::TAU) as f32;
        let gust = at.dot(self.downwind) * 0.08;
        (pace - gust + phase).sin()
    }
}

/// What [`sample_spinner_motion`] last measured of a motion or wind
/// spinner.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Motion {
    /// World position and parent world rotation at the last sample.
    last: Option<(Vec3, Quat)>,
    /// World velocity over the last frame (m/s).
    velocity: Vec3,
    /// Whether [`Self::velocity`] was measured over two samples, so the next
    /// can difference it: the first velocity after a spawn is no reading.
    velocity_known: bool,
    /// World acceleration over the last frame (m/s^2).
    acceleration: Vec3,
    /// How the part stands in the world unturned: its parent's world
    /// rotation composed with its own authored one.
    frame: Quat,
    /// The parent's world angular velocity over the last frame (rad/s).
    turning: Vec3,
    /// The spin axis in world space, at the last sample.
    axis_world: Vec3,
    /// The parent's up (+Y) in world space, at the last sample - what a
    /// steer term that follows the yaw reads the turning about (#1653).
    parent_up: Vec3,
    /// The velocity along the parent's +Z - forward, for every vehicle in
    /// Overlands is drawn facing +Z - at the last sample (m/s).
    forward_speed: f32,
}

/// A node that turns (#1604), slides or swells (#1935): its authored pose,
/// each channel's axis and terms, and what its motion terms have
/// accumulated. A channel without terms leaves its part of the pose as
/// authored, to the bit.
#[derive(Component, Clone, Debug)]
pub struct Spinner {
    /// The node's authored local pose, which every turn, slide and swell
    /// composes onto.
    base: Transform,
    /// Unit turn axis in the node's frame.
    axis: Vec3,
    turns: Vec<Turn>,
    /// Unit slide axis in the node's frame, after its authored rotation.
    slide_axis: Vec3,
    shifts: Vec<Shift>,
    /// Each local axis's share of the swell's amount, in `[0, 1]`.
    weights: Vec3,
    /// Whether the swell keeps the part's volume (see [`SwellTerm`]).
    keep_volume: bool,
    breaths: Vec<Breath>,
    /// This copy's clock offset (s), see [`copy_epoch`].
    epoch: f64,
    /// Whether any term reads the part's motion.
    follows_motion: bool,
    /// Whether any term reads the room's wind.
    reads_wind: bool,
    /// The angle the rate terms - roll and speed - have turned (rad,
    /// wrapped).
    rolled: f32,
    /// The steer terms' settled angle (rad).
    steered: f32,
    motion: Motion,
}

impl Spinner {
    /// The spinner for `node` spawned at `base`, or `None` when nothing
    /// about it moves: its spin, slide and swell as the node may carry them
    /// ([`Generator::moving_spin`] and its siblings). `epoch` is the copy's
    /// clock offset.
    pub(crate) fn of_node(node: &Generator, base: Transform, epoch: f64) -> Option<Self> {
        Self::build(
            node.moving_spin(),
            node.moving_slide(),
            node.moving_swell(),
            base,
            epoch,
        )
    }

    /// The spinner for a node at `base` with `spin` alone, or `None` when
    /// the spin turns nothing.
    #[cfg(test)]
    pub(crate) fn new(spin: &Spin, base: Transform, epoch: f64) -> Option<Self> {
        Self::build(Some(spin), None, None, base, epoch)
    }

    fn build(
        spin: Option<&Spin>,
        slide: Option<&Slide>,
        swell: Option<&Swell>,
        base: Transform,
        epoch: f64,
    ) -> Option<Self> {
        // Each channel stands alone: a zero axis stills its own channel and
        // no other.
        let spin = spin.and_then(|spin| {
            let axis = Vec3::from_array(spin.axis.0).try_normalize()?;
            Some((spin, axis))
        });
        let slide = slide.and_then(|slide| {
            let axis = Vec3::from_array(slide.axis.0).try_normalize()?;
            Some((slide, axis))
        });
        let swell = swell.filter(|swell| swell.moves());
        let turns: Vec<Turn> = spin.map_or_else(Vec::new, |(spin, _)| {
            spin.terms.iter().filter_map(Turn::from_term).collect()
        });
        let shifts: Vec<Shift> = slide.map_or_else(Vec::new, |(slide, _)| {
            slide.terms.iter().filter_map(Shift::from_term).collect()
        });
        let breaths: Vec<Breath> = swell.map_or_else(Vec::new, |swell| {
            swell.terms.iter().filter_map(Breath::from_term).collect()
        });
        if turns.is_empty() && shifts.is_empty() && breaths.is_empty() {
            return None;
        }
        let spin_terms = || spin.into_iter().flat_map(|(spin, _)| &spin.terms);
        let follows_motion = spin_terms().any(|t| t.moves() && t.follows_motion());
        let reads_wind = spin_terms().any(|t| t.moves() && t.reads_wind())
            || slide
                .into_iter()
                .flat_map(|(slide, _)| &slide.terms)
                .any(|t| t.moves() && t.reads_wind())
            || swell
                .into_iter()
                .flat_map(|swell| &swell.terms)
                .any(|t| t.moves() && t.reads_wind());
        let weights = swell.map_or(Vec3::ONE, |swell| Vec3::from_array(swell.held_weights()));
        Some(Self {
            base,
            axis: spin.map_or(Vec3::Y, |(_, axis)| axis),
            turns,
            slide_axis: slide.map_or(Vec3::Y, |(_, axis)| axis),
            shifts,
            weights,
            keep_volume: swell.is_some_and(|swell| swell.keep_volume),
            breaths,
            epoch,
            follows_motion,
            reads_wind,
            rolled: 0.0,
            steered: 0.0,
            motion: Motion::default(),
        })
    }

    /// This copy's clock offset (s).
    #[cfg(test)]
    pub(crate) fn epoch(&self) -> f64 {
        self.epoch
    }

    /// The angle the clock terms give at `now` (rad).
    fn clock_angle(&self, now: f64) -> f32 {
        let t = now + self.epoch;
        let mut angle = 0.0_f64;
        for turn in &self.turns {
            match *turn {
                // In f64: UTC is ~1.8e9 s, where an f32 product has lost
                // every digit that says where the sails are.
                Turn::Constant(rate) => {
                    angle += (rate * t).rem_euclid(std::f64::consts::TAU);
                }
                Turn::Swing {
                    amplitude,
                    per_second,
                    phase,
                } => angle += swing(amplitude, per_second, phase, t),
                Turn::Wobble {
                    amplitude,
                    per_second,
                } => {
                    angle += (amplitude * wobble(t * per_second)) as f64;
                }
                Turn::Roll { .. }
                | Turn::Steer { .. }
                | Turn::Speed { .. }
                | Turn::Lean { .. }
                | Turn::Wind { .. }
                | Turn::Vane { .. } => {}
            }
        }
        angle as f32
    }

    /// The slide offset (m) and the swell amount the clock terms give at
    /// `now`, on the clock the turn reads, calmed by `calm` (one as
    /// authored, [`REDUCED_MOTION_SHARE`] at Effects Reduced).
    ///
    /// Calmed term by term, not as a sum: a swing, a wobble and a bounce
    /// move `calm` as far at the same phase, but a belt runs `calm` as fast
    /// over its whole length. Calming a belt's offset would shrink its run
    /// and its crates' spacing, and bring its jump back out of the machine
    /// that hides it into the middle of the run.
    fn clock_shift(&self, now: f64, calm: f32) -> (f32, f32) {
        let t = now + self.epoch;
        let mut offset = 0.0_f64;
        for shift in &self.shifts {
            match *shift {
                Shift::Swing {
                    amplitude,
                    per_second,
                    phase,
                } => offset += swing(amplitude * calm, per_second, phase, t),
                Shift::Wobble {
                    amplitude,
                    per_second,
                } => offset += (amplitude * calm * wobble(t * per_second)) as f64,
                Shift::Bounce {
                    height,
                    per_second,
                    phase,
                } => offset += bounce(height * calm, per_second, phase, t),
                Shift::Belt {
                    speed,
                    length,
                    start,
                } => offset += belt(speed * calm as f64, length, start, t) as f64,
                Shift::Wind { .. } => {}
            }
        }
        let mut amount = 0.0_f64;
        for breath in &self.breaths {
            match *breath {
                Breath::Swing {
                    amount: reach,
                    per_second,
                    phase,
                } => amount += swing(reach * calm, per_second, phase, t),
                Breath::Wobble {
                    amount: reach,
                    per_second,
                } => amount += (reach * calm * wobble(t * per_second)) as f64,
                Breath::Wind { .. } => {}
            }
        }
        (offset as f32, amount as f32)
    }

    /// The slide offset (m) and the swell amount the wind terms give at
    /// `now`, for a part standing at `at` in `wind`: the gust a turn's wind
    /// term sways by, so a part that sways and slides in the wind is pushed
    /// by one gust. Each term is calmed by `calm`, as in
    /// [`Self::clock_shift`].
    fn wind_shift(&self, now: f64, wind: RoomWind, at: Vec3, calm: f32) -> (f32, f32) {
        if wind.downwind == Vec3::ZERO {
            return (0.0, 0.0);
        }
        let t = now + self.epoch;
        let gust = wind.scale() * (0.6 + 0.4 * wind.bend(t, at, 0.0));
        // How squarely the wind blows along the slide axis, downwind
        // positive.
        let along = wind.downwind.dot(self.motion.frame * self.slide_axis);
        let offset = self
            .shifts
            .iter()
            .map(|shift| match *shift {
                Shift::Wind { amplitude } => amplitude * calm * gust * along,
                _ => 0.0,
            })
            .sum();
        let amount = self
            .breaths
            .iter()
            .map(|breath| match *breath {
                Breath::Wind { amount } => amount * calm * gust,
                _ => 0.0,
            })
            .sum();
        (offset, amount)
    }

    /// Each local axis's factor on the authored scale at swell `amount`, by
    /// [`axis_factors`] - the formula the avatar scale cap bounds a swell
    /// by ([`Swell::peak_factor`]), so what is drawn is what was bounded.
    fn swell_factors(&self, amount: f32) -> Vec3 {
        Vec3::from_array(axis_factors(
            self.weights.to_array(),
            self.keep_volume,
            amount,
        ))
    }

    /// The angle the wind terms give at `now`, for a part standing at `at`
    /// in `wind` (rad).
    fn wind_angle(&self, now: f64, wind: RoomWind, at: Vec3) -> f32 {
        if wind.downwind == Vec3::ZERO {
            return 0.0;
        }
        let t = now + self.epoch;
        let axis = self.motion.frame * self.axis;
        let up = self.motion.frame * Vec3::Y;
        let mut angle = 0.0;
        for turn in &self.turns {
            match *turn {
                // A turn about the axis carries the part's top toward
                // `axis x up`; how squarely the wind blows that way is how
                // hard it pushes. A hanging part's BOTTOM swings downwind, so
                // a positive amplitude turns it against that push.
                Turn::Wind { amplitude } => {
                    let push = wind.downwind.dot(axis.cross(up));
                    let sway = 0.6 + 0.4 * wind.bend(t, at, 0.0);
                    angle -= amplitude * wind.scale() * push * sway;
                }
                Turn::Vane { facing } => {
                    let flat = |v: Vec3| (v - axis * v.dot(axis)).normalize_or_zero();
                    let (from, to) = (flat(self.motion.frame * facing), flat(wind.downwind));
                    if from == Vec3::ZERO || to == Vec3::ZERO {
                        continue;
                    }
                    let hunt = VANE_HUNT_DEG.to_radians()
                        * wind.scale().min(1.0)
                        * wind.bend(t * 0.7, at, 2.1);
                    angle += from.cross(to).dot(axis).atan2(from.dot(to)) + hunt;
                }
                _ => {}
            }
        }
        angle
    }

    /// The lean terms' springs' angles (rad).
    fn lean_angle(&self) -> f32 {
        self.turns
            .iter()
            .map(|turn| match *turn {
                Turn::Lean { angle, .. } => angle,
                _ => 0.0,
            })
            .sum()
    }

    /// Back to rest, as a freeze leaves every part: nothing rolled, nothing
    /// steered, every lean hanging straight.
    fn settle(&mut self) {
        self.rolled = 0.0;
        self.steered = 0.0;
        for turn in &mut self.turns {
            turn.settle();
        }
    }

    /// Advance the motion terms by `dt` on the last sampled motion.
    fn follow_motion(&mut self, dt: f32) {
        let Motion {
            velocity,
            acceleration,
            turning,
            axis_world,
            parent_up,
            forward_speed,
            frame,
            ..
        } = self.motion;
        // A turn about the axis carries the part's top toward `axis x up`.
        let tipping = axis_world.cross(frame * Vec3::Y);
        // Reversing through a bend turns the vehicle the other way for the
        // same wheels, so the wheels that follow its turning turn back.
        let heading = if forward_speed < -REVERSING_M_S {
            -1.0
        } else {
            1.0
        };
        let mut rate = 0.0;
        let mut steer = 0.0;
        for turn in &mut self.turns {
            match turn {
                // A wheel rolling with velocity v on ground whose normal is
                // up turns at (up x v) / r: project that onto the axis, so
                // the sign follows the axis and a wheel turned across its
                // travel rolls only as far as it is carried along it.
                Turn::Roll { per_metre } => {
                    rate += Vec3::Y.cross(velocity).dot(axis_world) * *per_metre;
                }
                // Read about the node's own axis, or - following the yaw -
                // about the parent's up, which the parent's bank and roll
                // never turn about.
                Turn::Steer {
                    gain,
                    limit,
                    follow_yaw,
                } => {
                    let about = if *follow_yaw { parent_up } else { axis_world };
                    steer += (heading * *gain * turning.dot(about)).clamp(-*limit, *limit);
                }
                Turn::Speed { idle, per_metre } => {
                    rate += *idle + *per_metre * forward_speed;
                }
                // A hanging part's bottom lags its carrier's acceleration:
                // it swings toward minus the acceleration, which a positive
                // turn does when the acceleration runs along `axis x up`.
                Turn::Lean {
                    gain,
                    omega,
                    angle,
                    rate: swing,
                } => {
                    let cap = MAX_LEAN_DEG.to_radians();
                    let target = *gain * acceleration.dot(tipping);
                    let target = if target.is_finite() {
                        target.clamp(-cap, cap)
                    } else {
                        0.0
                    };
                    let steps = (dt / LEAN_STEP_S).ceil().clamp(1.0, 64.0);
                    let h = dt / steps;
                    for _ in 0..steps as usize {
                        let pull = *omega * *omega * (target - *angle);
                        let drag = 2.0 * LEAN_DAMPING * *omega * *swing;
                        *swing += (pull - drag) * h;
                        *angle += *swing * h;
                        // A stop, not a bounce: the part comes to rest
                        // against the bound and swings back from it.
                        if angle.abs() > cap {
                            *angle = angle.signum() * cap;
                            *swing = 0.0;
                        }
                    }
                    if !angle.is_finite() || !swing.is_finite() {
                        *angle = 0.0;
                        *swing = 0.0;
                    }
                }
                Turn::Constant(_)
                | Turn::Swing { .. }
                | Turn::Wind { .. }
                | Turn::Wobble { .. }
                | Turn::Vane { .. } => {}
            }
        }
        // A teleport reads as one enormous frame of travel: the cap keeps it
        // to a flick of the wheel.
        let cap = limits::MAX_SPIN_RATE_DEG.to_radians();
        let rate = if rate.is_finite() {
            rate.clamp(-cap, cap)
        } else {
            0.0
        };
        self.rolled = (self.rolled + rate * dt).rem_euclid(TAU);
        if steer.is_finite() {
            let follow = 1.0 - (-dt / STEER_SETTLE_S).exp();
            self.steered += (steer - self.steered) * follow;
        }
    }

    /// The node's pose turned by `angle`, slid `offset` metres along its
    /// slide axis and swollen by `amount`. A channel without terms keeps the
    /// authored value untouched, so a part that only turns keeps its place
    /// and size to the bit.
    fn posed(&self, angle: f32, offset: f32, amount: f32) -> Transform {
        let base = self.base;
        Transform {
            rotation: if self.turns.is_empty() {
                base.rotation
            } else {
                base.rotation * Quat::from_axis_angle(self.axis, angle)
            },
            translation: if self.shifts.is_empty() {
                base.translation
            } else {
                base.translation + base.rotation * (self.slide_axis * offset)
            },
            scale: if self.breaths.is_empty() {
                base.scale
            } else {
                base.scale * self.swell_factors(amount)
            },
        }
    }
}

/// Turn every spinner for this frame, or hold them all at their authored
/// poses while an editor has a selection or effects are off. See the module
/// docs for the freeze's rules.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn animate_spinners(
    time: Res<Time>,
    real: Res<Time<Real>>,
    mut clock: ResMut<SpinClock>,
    hold: Res<SpinHold>,
    settings: Option<Res<crate::state::LocalSettings>>,
    wind: Option<Res<crate::wind::VegetationWind>>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    mut spinners: Query<
        (
            &mut Spinner,
            &mut Transform,
            &GlobalTransform,
            Option<&InheritedVisibility>,
        ),
        With<ChildOf>,
    >,
    mut settled: ResMut<SpinSettled>,
    mut frozen: Local<bool>,
) {
    let effects = settings.map(|s| s.effects_intensity);
    let held = hold.editing || effects.is_some_and(|e| !e.plays());
    // A slide and a swell are calmed at Reduced; a turn is not.
    let calm = if effects == Some(crate::state::EffectsIntensity::Reduced) {
        REDUCED_MOTION_SHARE
    } else {
        1.0
    };
    if held {
        if !*frozen {
            for (mut spinner, mut tf, ..) in &mut spinners {
                *tf = spinner.base;
                spinner.settle();
            }
            *frozen = true;
        }
        settled.set_if_neq(SpinSettled(true));
        return;
    }
    if *frozen {
        // Whatever the editor left is the authored pose now: a committed
        // drag turns from where it was put, not from where the record said
        // before it, while its recompile is still on the way.
        for (mut spinner, tf, ..) in &mut spinners {
            spinner.base = *tf;
        }
        *frozen = false;
    }
    settled.set_if_neq(SpinSettled(spinners.is_empty()));

    let now = clock.now(&time, &real);
    let dt = time.delta_secs();
    let wind = wind.map_or(
        RoomWind {
            downwind: Vec3::ZERO,
            speed: 0.0,
        },
        |w| RoomWind::of(&w),
    );
    let eyes: Vec<Vec3> = cameras
        .iter()
        .filter(|(camera, _)| camera.is_active)
        .map(|(_, global)| global.translation())
        .collect();
    let range_sq = SPIN_RANGE_M * SPIN_RANGE_M;
    for (mut spinner, mut tf, global, visibility) in &mut spinners {
        if spinner.follows_motion && dt > 0.0 {
            spinner.follow_motion(dt);
        }
        let at = global.translation();
        let seen = visibility.is_none_or(|v| v.get())
            && (eyes.is_empty() || eyes.iter().any(|eye| eye.distance_squared(at) <= range_sq));
        if !seen {
            continue;
        }
        let mut angle =
            spinner.clock_angle(now) + spinner.rolled + spinner.steered + spinner.lean_angle();
        let (mut offset, mut amount) = spinner.clock_shift(now, calm);
        // Not before the part's first sample: until then its world frame is
        // unknown, and a vane drawn against an unturned frame would face the
        // wrong way for a frame and snap. The authored pose stands in.
        if spinner.reads_wind && spinner.motion.last.is_some() {
            angle += spinner.wind_angle(now, wind, at);
            let (blown, filled) = spinner.wind_shift(now, wind, at, calm);
            offset += blown;
            amount += filled;
        }
        // Written only when it moved: a change tick is a promise that
        // something changed (a still belt, a held clock).
        tf.set_if_neq(spinner.posed(angle, offset, amount));
    }
}

/// Measure each motion or wind spinner's travel, its parent's turning and
/// how it stands in the world, over the frame just propagated. See the
/// module docs for why it runs after propagation.
pub(crate) fn sample_spinner_motion(
    time: Res<Time>,
    mut spinners: Query<(&mut Spinner, &GlobalTransform, &ChildOf)>,
    frames: Query<&GlobalTransform>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for (mut spinner, global, child_of) in &mut spinners {
        if !spinner.follows_motion && !spinner.reads_wind {
            continue;
        }
        let parent = frames
            .get(child_of.parent())
            .map_or(Quat::IDENTITY, GlobalTransform::rotation);
        // Where the part is drawn, its own slide included: a slide on the
        // node itself and one on a carrier above it are the same motion.
        let at = global.translation();
        let frame = parent * spinner.base.rotation;
        let axis_world = (frame * spinner.axis).normalize_or_zero();
        let motion = &mut spinner.motion;
        motion.frame = frame;
        if let Some((last_at, last_parent)) = motion.last {
            let velocity = (at - last_at) / dt;
            motion.acceleration = if motion.velocity_known {
                (velocity - motion.velocity) / dt
            } else {
                Vec3::ZERO
            };
            motion.velocity = velocity;
            motion.velocity_known = true;
            // The shorter way round: a quaternion and its negation are one
            // rotation, and only one of them has an angle under a half turn.
            let mut delta = parent * last_parent.inverse();
            if delta.w < 0.0 {
                delta = -delta;
            }
            let (axis, angle) = delta.to_axis_angle();
            motion.turning = if angle > 1e-6 && angle < PI {
                axis * (angle / dt)
            } else {
                Vec3::ZERO
            };
        }
        motion.last = Some((at, parent));
        motion.axis_world = axis_world;
        motion.parent_up = parent * Vec3::Y;
        motion.forward_speed = motion.velocity.dot(parent * Vec3::Z);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::Fp;
    use crate::pds::Fp3;

    fn spinner(axis: [f32; 3], terms: Vec<SpinTerm>) -> Spinner {
        Spinner::new(
            &Spin {
                axis: Fp3(axis),
                terms,
            },
            Transform::IDENTITY,
            0.0,
        )
        .expect("a spin that moves")
    }

    fn constant(deg_per_s: f32) -> SpinTerm {
        SpinTerm::Constant {
            rate: Fp(deg_per_s),
        }
    }

    #[test]
    fn a_spin_that_turns_nothing_gets_no_spinner() {
        let none = |spin: Spin| Spinner::new(&spin, Transform::IDENTITY, 0.0).is_none();
        assert!(none(Spin::about([0.0; 3], constant(30.0))));
        assert!(none(Spin::about([0.0, 1.0, 0.0], constant(0.0))));
        assert!(none(Spin::about([0.0, 1.0, 0.0], SpinTerm::Unknown)));
    }

    #[test]
    fn a_steady_turn_is_a_function_of_the_clock_alone() {
        let s = spinner([0.0, 0.0, 2.0], vec![constant(90.0)]);
        assert!((s.clock_angle(1.0) - 90f32.to_radians()).abs() < 1e-5);
        // A wall clock's worth of seconds keeps its precision: one second
        // past an exact number of turns reads one second's turn, and one
        // frame there turns one frame's worth.
        let late = 1.8e9_f64;
        let whole_turns = (late * 0.25).floor() / 0.25;
        assert!((s.clock_angle(whole_turns + 1.0) - 90f32.to_radians()).abs() < 1e-4);
        let frame = (s.clock_angle(late + 1.0 / 64.0) - s.clock_angle(late)).rem_euclid(TAU);
        assert!(
            (frame - (90f32 / 64.0).to_radians()).abs() < 1e-4,
            "{frame}"
        );
        // The copy's offset shifts the clock, not the rate.
        let mut later = s.clone();
        later.epoch = 2.0;
        assert!((later.clock_angle(0.0) - PI).abs() < 1e-5);
    }

    #[test]
    fn a_swing_returns_through_its_pose_twice_a_period() {
        let s = spinner(
            [1.0, 0.0, 0.0],
            vec![SpinTerm::Swing {
                amplitude: Fp(30.0),
                period: Fp(2.0),
                phase: Fp::ZERO,
            }],
        );
        assert!(s.clock_angle(0.0).abs() < 1e-6);
        assert!((s.clock_angle(0.5) - 30f32.to_radians()).abs() < 1e-5);
        assert!(s.clock_angle(1.0).abs() < 1e-5);
        assert!((s.clock_angle(1.5) + 30f32.to_radians()).abs() < 1e-5);
    }

    /// A left wheel whose axle points out to the left and a right wheel
    /// whose axle points out to the right both roll FORWARD, each about its
    /// own axis - the sign comes from the axis, with nothing to mirror.
    #[test]
    fn mirrored_wheels_both_roll_forwards() {
        let radius = 0.5;
        let roll = || vec![SpinTerm::Roll { radius: Fp(radius) }];
        // Travelling along -Z (Bevy's forward) at 2 m/s for one second.
        let travel = |axis: Vec3| {
            let mut wheel = spinner(axis.to_array(), roll());
            wheel.motion.velocity = Vec3::new(0.0, 0.0, -2.0);
            wheel.motion.axis_world = axis;
            for _ in 0..10 {
                wheel.follow_motion(0.1);
            }
            wheel
        };
        let left = travel(Vec3::NEG_X);
        let right = travel(Vec3::X);
        // 2 m at r = 0.5 is 4 rad of roll: +4 about the left axle, -4 about
        // the right one (wrapped into a turn).
        assert!((left.rolled - 4.0).abs() < 1e-3, "{}", left.rolled);
        assert!(
            (right.rolled - (TAU - 4.0)).abs() < 1e-3,
            "{}",
            right.rolled
        );
        // Opposite angles about opposite axes are the same turn in the
        // world: each carries the wheel's top to the same place.
        let top = |w: &Spinner, axis: Vec3| Quat::from_axis_angle(axis, w.rolled) * Vec3::Y;
        let (l, r) = (top(&left, Vec3::NEG_X), top(&right, Vec3::X));
        assert!(l.distance(r) < 1e-3, "{l} vs {r}");
        // And the top moves FORWARD first: a quarter of a radian in, it
        // leans toward -Z.
        let mut early = spinner([1.0, 0.0, 0.0], roll());
        early.motion.velocity = Vec3::new(0.0, 0.0, -2.0);
        early.motion.axis_world = Vec3::X;
        early.follow_motion(0.0625);
        assert!(top(&early, Vec3::X).z < 0.0);
    }

    #[test]
    fn a_wheel_carried_along_its_axle_does_not_roll() {
        let mut wheel = spinner([1.0, 0.0, 0.0], vec![SpinTerm::Roll { radius: Fp(0.4) }]);
        wheel.motion.velocity = Vec3::new(3.0, 0.0, 0.0);
        wheel.motion.axis_world = Vec3::X;
        wheel.follow_motion(0.5);
        assert_eq!(wheel.rolled, 0.0);
    }

    #[test]
    fn a_teleport_flicks_a_wheel_no_further_than_the_rate_cap() {
        let mut wheel = spinner([1.0, 0.0, 0.0], vec![SpinTerm::Roll { radius: Fp(0.4) }]);
        wheel.motion.velocity = Vec3::new(0.0, 0.0, -1e6);
        wheel.motion.axis_world = Vec3::X;
        wheel.follow_motion(1.0 / 60.0);
        let cap = limits::MAX_SPIN_RATE_DEG.to_radians() / 60.0;
        let turned = wheel.rolled.min(TAU - wheel.rolled);
        assert!(turned <= cap + 1e-5, "{turned} > {cap}");
    }

    #[test]
    fn a_steer_follows_the_turn_within_its_limit() {
        let mut front = spinner(
            [0.0, 1.0, 0.0],
            vec![SpinTerm::Steer {
                gain: Fp(0.5),
                limit: Fp(20.0),
                follow_yaw: false,
            }],
        );
        front.motion.axis_world = Vec3::Y;
        // Turning left at 30 deg/s: half of it, 15 deg, inside the limit.
        front.motion.turning = Vec3::new(0.0, 30f32.to_radians(), 0.0);
        for _ in 0..120 {
            front.follow_motion(1.0 / 60.0);
        }
        assert!((front.steered - 15f32.to_radians()).abs() < 1e-3);
        // Turning right hard: held at the limit.
        front.motion.turning = Vec3::new(0.0, -200f32.to_radians(), 0.0);
        for _ in 0..120 {
            front.follow_motion(1.0 / 60.0);
        }
        assert!((front.steered + 20f32.to_radians()).abs() < 1e-3);
        // Reversing, a vehicle whose wheels are turned left yaws right: the
        // same measured turn now means the wheels point the other way.
        front.motion.turning = Vec3::new(0.0, -30f32.to_radians(), 0.0);
        front.motion.forward_speed = -2.0;
        for _ in 0..120 {
            front.follow_motion(1.0 / 60.0);
        }
        assert!((front.steered - 15f32.to_radians()).abs() < 1e-3);
        // Creeping back slower than a walk keeps the forward sense.
        front.motion.forward_speed = -0.1;
        for _ in 0..120 {
            front.follow_motion(1.0 / 60.0);
        }
        assert!((front.steered + 15f32.to_radians()).abs() < 1e-3);
    }

    /// A steer that follows the yaw reads its parent's turning about the
    /// parent's own up, whatever axis it turns about (#1653): a steering
    /// wheel on a raked column follows its vehicle's yaw and not its bank.
    /// Read about the column instead, the same bank turns it most of the way
    /// to its limit.
    #[test]
    fn a_steer_that_follows_the_yaw_ignores_its_parents_bank() {
        let column = Vec3::new(0.0, 0.69, -0.72).normalize();
        let wheel = |follow_yaw| {
            let mut wheel = spinner(
                [0.0, 1.0, 0.0],
                vec![SpinTerm::Steer {
                    gain: Fp(1.8),
                    limit: Fp(90.0),
                    follow_yaw,
                }],
            );
            wheel.motion.axis_world = column;
            wheel.motion.parent_up = Vec3::Y;
            wheel
        };
        let settled = |mut wheel: Spinner, turning: Vec3| {
            wheel.motion.turning = turning;
            for _ in 0..120 {
                wheel.follow_motion(1.0 / 60.0);
            }
            wheel.steered
        };
        // Banking into a bend at 60 deg/s, about the fore-and-aft axis.
        let bank = Vec3::new(0.0, 0.0, -60f32.to_radians());
        assert!(settled(wheel(true), bank).abs() < 1e-4);
        assert!(
            settled(wheel(false), bank).abs() > 30f32.to_radians(),
            "read about the column, the bank turns the wheel"
        );
        // Turning left at 20 deg/s: 1.8 times it, 36 degrees.
        let yaw = Vec3::new(0.0, 20f32.to_radians(), 0.0);
        assert!((settled(wheel(true), yaw) - 36f32.to_radians()).abs() < 1e-3);
    }

    #[test]
    fn the_turn_composes_onto_the_authored_pose_rigidly() {
        let base = Transform {
            translation: Vec3::new(1.0, 2.0, 3.0),
            rotation: Quat::from_rotation_y(0.7),
            scale: Vec3::new(2.0, 1.0, 0.5),
        };
        let s =
            Spinner::new(&Spin::about([0.0, 0.0, 1.0], constant(10.0)), base, 0.0).expect("moves");
        let posed = s.posed(1.2, 0.0, 0.0);
        assert_eq!(posed.translation, base.translation);
        assert_eq!(posed.scale, base.scale);
        // The axis is in the node's frame after its own rotation: the node's
        // local Z, wherever the authored rotation pointed it, stays put.
        let axis_out = posed.rotation * Vec3::Z;
        let axis_in = base.rotation * Vec3::Z;
        assert!(axis_out.distance(axis_in) < 1e-5);
        assert!(s.posed(0.0, 0.0, 0.0).rotation.angle_between(base.rotation) < 1e-6);
    }

    /// An app turning spinners through the real schedule: transform
    /// propagation, a hand-stepped clock, and the spin systems around it.
    fn spin_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TransformPlugin));
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_secs_f64(1.0 / 64.0),
        ));
        register(&mut app, SpinClock::Scene);
        app
    }

    /// A spinner at `base` under a parent at the origin, as the spawner
    /// hangs one.
    fn spawn_spinner(app: &mut App, spin: &Spin, base: Transform) -> (Entity, Entity) {
        let parent = app.world_mut().spawn(Transform::IDENTITY).id();
        let spinner = Spinner::new(spin, base, 0.0).expect("moves");
        let child = app
            .world_mut()
            .spawn((spinner, base, InheritedVisibility::VISIBLE, ChildOf(parent)))
            .id();
        (parent, child)
    }

    fn pose(app: &App, entity: Entity) -> Transform {
        *app.world().get::<Transform>(entity).expect("a transform")
    }

    fn hold(app: &mut App, editing: bool) {
        app.world_mut().resource_mut::<SpinHold>().editing = editing;
    }

    fn settled(app: &App) -> bool {
        app.world().resource::<SpinSettled>().0
    }

    /// The editor's contract (#1604): a selection snaps every spinner to its
    /// authored pose the frame the hold arrives, reports it settled, and then
    /// leaves the pose to the gizmo; the end of the selection takes the pose
    /// the gizmo left as the authored one.
    #[test]
    fn a_selection_snaps_spinners_home_then_leaves_them_to_the_gizmo() {
        let mut app = spin_app();
        let base = Transform::from_xyz(1.0, 2.0, 3.0);
        let (_, wheel) = spawn_spinner(
            &mut app,
            &Spin::about([0.0, 1.0, 0.0], constant(90.0)),
            base,
        );
        for _ in 0..16 {
            app.update();
        }
        assert!(pose(&app, wheel).rotation.angle_between(base.rotation) > 0.1);
        assert!(!settled(&app), "a world with a turning part is not settled");

        hold(&mut app, true);
        app.update();
        assert_eq!(pose(&app, wheel), base, "snapped home on the hold's frame");
        assert!(settled(&app));

        // The gizmo moves the part; the frozen spinner never writes again.
        let dragged = Transform::from_xyz(5.0, 2.0, 3.0);
        *app.world_mut().get_mut::<Transform>(wheel).unwrap() = dragged;
        for _ in 0..8 {
            app.update();
        }
        assert_eq!(pose(&app, wheel), dragged, "the gizmo owns the pose");

        // Released: it turns again, from where the editor left it.
        hold(&mut app, false);
        for _ in 0..8 {
            app.update();
        }
        let turned = pose(&app, wheel);
        assert_eq!(turned.translation, dragged.translation);
        assert!(turned.rotation.angle_between(dragged.rotation) > 0.01);
        assert!(!settled(&app));
    }

    /// The same through the real schedule (#1653): a wheel drawn along a
    /// raked column, under a vehicle that banks and then yaws. The sampler
    /// reads the vehicle's up off its propagated pose, so the bank leaves the
    /// wheel where it was drawn and the yaw turns it.
    #[test]
    fn a_wheel_that_follows_the_yaw_holds_still_through_its_vehicles_bank() {
        let mut app = spin_app();
        let base = Transform::from_rotation(Quat::from_rotation_x(-0.82));
        let (vehicle, wheel) = spawn_spinner(
            &mut app,
            &Spin::about(
                [0.0, 1.0, 0.0],
                SpinTerm::Steer {
                    gain: Fp(1.8),
                    limit: Fp(90.0),
                    follow_yaw: true,
                },
            ),
            base,
        );
        let bank = 7f32.to_radians();
        let set = |app: &mut App, rotation: Quat| {
            app.world_mut()
                .get_mut::<Transform>(vehicle)
                .expect("the vehicle")
                .rotation = rotation;
            app.update();
        };
        // Bank seven degrees into a bend over five frames, and hold it.
        for k in 1..=24u8 {
            set(
                &mut app,
                Quat::from_rotation_z(-bank * f32::from(k.min(5)) / 5.0),
            );
            let turned = pose(&app, wheel).rotation.angle_between(base.rotation);
            assert!(
                turned < 1e-3,
                "frame {k}: the bank turned the wheel {turned} rad"
            );
        }
        // Yaw left at 20 deg/s for a second, still banked.
        for k in 1..=64u8 {
            let heading = 20f32.to_radians() * f32::from(k) / 64.0;
            set(
                &mut app,
                Quat::from_rotation_y(heading) * Quat::from_rotation_z(-bank),
            );
        }
        let turned = pose(&app, wheel).rotation.angle_between(base.rotation);
        assert!(
            (turned - 36f32.to_radians()).abs() < 2f32.to_radians(),
            "the yaw turned the wheel {} deg, not 1.8 x 20",
            turned.to_degrees()
        );
    }

    #[test]
    fn a_spinner_the_gizmo_has_detached_is_never_written() {
        let mut app = spin_app();
        let base = Transform::from_xyz(0.0, 1.0, 0.0);
        let (_, wheel) = spawn_spinner(
            &mut app,
            &Spin::about([0.0, 1.0, 0.0], constant(90.0)),
            base,
        );
        // Detached into world space at some world pose.
        app.world_mut().entity_mut(wheel).remove::<ChildOf>();
        let world_pose = Transform::from_xyz(40.0, 9.0, -3.0);
        *app.world_mut().get_mut::<Transform>(wheel).unwrap() = world_pose;
        for _ in 0..8 {
            app.update();
        }
        assert_eq!(pose(&app, wheel), world_pose);
    }

    #[test]
    fn effects_off_holds_every_spinner_at_its_pose() {
        let mut app = spin_app();
        app.insert_resource(crate::state::LocalSettings {
            effects_intensity: crate::state::EffectsIntensity::Off,
            ..Default::default()
        });
        let base = Transform::from_xyz(0.0, 1.0, 0.0);
        let (_, wheel) = spawn_spinner(
            &mut app,
            &Spin::about([0.0, 1.0, 0.0], constant(90.0)),
            base,
        );
        for _ in 0..8 {
            app.update();
        }
        assert_eq!(pose(&app, wheel), base);
        assert!(settled(&app));
    }

    #[test]
    fn a_world_with_nothing_turning_is_settled_at_once() {
        let mut app = spin_app();
        app.update();
        assert!(
            settled(&app),
            "a gizmo there attaches the frame it is asked to"
        );
    }

    /// A wheel carried along by its parent rolls by the distance travelled
    /// over its radius - measured through propagation, as a remote peer's
    /// wheels are (#1323's rule: difference after propagation).
    #[test]
    fn a_carried_wheel_rolls_its_travel_over_its_radius() {
        let mut app = spin_app();
        let radius = 0.5;
        let (carrier, wheel) = spawn_spinner(
            &mut app,
            &Spin::about([1.0, 0.0, 0.0], SpinTerm::Roll { radius: Fp(radius) }),
            Transform::IDENTITY,
        );
        // 2 m/s along -Z, Bevy's forward.
        app.add_systems(
            Update,
            move |time: Res<Time>, mut frames: Query<&mut Transform, Without<Spinner>>| {
                if let Ok(mut tf) = frames.get_mut(carrier) {
                    tf.translation.z -= 2.0 * time.delta_secs();
                }
            },
        );
        for _ in 0..64 {
            app.update();
        }
        // One second, 2 m, 4 rad about -X (forward roll about +X is
        // negative) - less the frame or two before the first measurement.
        let (axis, angle) = pose(&app, wheel).rotation.to_axis_angle();
        let signed = if axis.x < 0.0 { angle } else { -angle };
        let about_neg_x = if signed < 0.0 { signed + TAU } else { signed };
        assert!(
            (about_neg_x - 4.0).abs() < 0.2,
            "rolled {about_neg_x} rad about -X, wanted ~4"
        );
    }

    #[test]
    fn a_speed_term_idles_spins_up_and_backs_in_reverse() {
        let mut prop = spinner(
            [0.0, 0.0, 1.0],
            vec![SpinTerm::Speed {
                idle: Fp(120.0),
                gain: Fp(60.0),
            }],
        );
        let turned = |prop: &mut Spinner, forward: f32| {
            prop.rolled = 0.0;
            prop.motion.forward_speed = forward;
            prop.follow_motion(0.25);
            let a = prop.rolled;
            (if a > PI { a - TAU } else { a }).to_degrees() * 4.0
        };
        assert!((turned(&mut prop, 0.0) - 120.0).abs() < 1e-2, "idles");
        assert!((turned(&mut prop, 2.0) - 240.0).abs() < 1e-2, "spins up");
        assert!((turned(&mut prop, -3.0) + 60.0).abs() < 1e-2, "backs");
    }

    /// The pendulum's own rule: a part hanging from a lateral axis settles
    /// at `gain` per m/s^2 of acceleration, its bottom swung back AGAINST
    /// the acceleration - and swings past that on the way, as a lantern does.
    #[test]
    fn a_hanging_lean_settles_lagging_its_carriers_acceleration() {
        let mut lantern = spinner(
            [1.0, 0.0, 0.0],
            vec![SpinTerm::Lean {
                gain: Fp(5.8),
                period: Fp(1.2),
            }],
        );
        lantern.motion.frame = Quat::IDENTITY;
        lantern.motion.axis_world = Vec3::X;
        // Speeding up toward -Z, Bevy's forward, at 2 m/s^2.
        lantern.motion.acceleration = Vec3::new(0.0, 0.0, -2.0);
        let mut deepest = 0.0_f32;
        for _ in 0..600 {
            lantern.follow_motion(1.0 / 60.0);
            deepest = deepest.min(lantern.lean_angle());
        }
        let settled = lantern.lean_angle();
        assert!(
            (settled.to_degrees() + 11.6).abs() < 0.1,
            "{}",
            settled.to_degrees()
        );
        assert!(
            deepest < settled - 0.02,
            "it swings past its lean on the way"
        );
        // Its bottom, -Y, now stands toward +Z: behind, against the push.
        let bottom = Quat::from_axis_angle(Vec3::X, settled) * Vec3::NEG_Y;
        assert!(bottom.z > 0.1, "{bottom}");
        // A standing part (negative gain) tips its TOP back instead.
        let mut aerial = spinner(
            [1.0, 0.0, 0.0],
            vec![SpinTerm::Lean {
                gain: Fp(-5.8),
                period: Fp(1.2),
            }],
        );
        aerial.motion = lantern.motion;
        for _ in 0..600 {
            aerial.follow_motion(1.0 / 60.0);
        }
        let top = Quat::from_axis_angle(Vec3::X, aerial.lean_angle()) * Vec3::Y;
        assert!(top.z > 0.1, "{top}");
    }

    #[test]
    fn a_lean_survives_a_hitch_and_a_spike() {
        let mut lantern = spinner(
            [1.0, 0.0, 0.0],
            vec![SpinTerm::Lean {
                gain: Fp(30.0),
                period: Fp(0.5),
            }],
        );
        lantern.motion.frame = Quat::IDENTITY;
        lantern.motion.axis_world = Vec3::X;
        lantern.motion.acceleration = Vec3::new(0.0, 0.0, 1e6);
        // A quarter-second hitch, a stiff spring and an absurd spike.
        lantern.follow_motion(0.25);
        let a = lantern.lean_angle();
        assert!(
            a.is_finite() && a.abs() <= MAX_LEAN_DEG.to_radians() + 1e-6,
            "{a}"
        );
        // Held for frames together, as a driven chassis's can be, the swing
        // still never passes the bound its target is clamped to.
        lantern.motion.acceleration = Vec3::new(0.0, 0.0, -45.0);
        for _ in 0..240 {
            lantern.follow_motion(1.0 / 60.0);
            assert!(lantern.lean_angle().abs() <= MAX_LEAN_DEG.to_radians() + 1e-6);
        }
    }

    fn wind(downwind: Vec3, speed: f32) -> RoomWind {
        RoomWind { downwind, speed }
    }

    /// A sign hanging from a pole across the wind swings its bottom
    /// downwind; one standing on its axis tips its top downwind; the wind
    /// along the axis, or no wind, moves neither; a stronger wind moves them
    /// further.
    #[test]
    fn the_wind_swings_a_hanging_part_downwind_and_bends_a_standing_one() {
        let sign = |amplitude: f32| {
            let mut s = spinner(
                [1.0, 0.0, 0.0],
                vec![SpinTerm::Wind {
                    amplitude: Fp(amplitude),
                }],
            );
            s.motion.frame = Quat::IDENTITY;
            s
        };
        let blow = wind(Vec3::Z, 4.0);
        for t in [0.0, 1.3, 2.9, 7.7, 1.8e9] {
            let hanging = sign(20.0).wind_angle(t, blow, Vec3::ZERO);
            let bottom = Quat::from_axis_angle(Vec3::X, hanging) * Vec3::NEG_Y;
            assert!(bottom.z > 0.0, "t {t}: the bottom swings downwind");
            assert!(hanging.abs() <= 20f32.to_radians() + 1e-5);
            let standing = sign(-20.0).wind_angle(t, blow, Vec3::ZERO);
            let top = Quat::from_axis_angle(Vec3::X, standing) * Vec3::Y;
            assert!(top.z > 0.0, "t {t}: the top bends downwind");
        }
        assert_eq!(
            sign(20.0).wind_angle(3.0, wind(Vec3::X, 4.0), Vec3::ZERO),
            0.0
        );
        assert_eq!(
            sign(20.0).wind_angle(3.0, wind(Vec3::ZERO, 0.0), Vec3::ZERO),
            0.0
        );
        let calm = sign(20.0).wind_angle(3.0, blow, Vec3::ZERO);
        let gale = sign(20.0).wind_angle(3.0, wind(Vec3::Z, 8.0), Vec3::ZERO);
        assert!(gale.abs() > calm.abs(), "{gale} vs {calm}");
    }

    #[test]
    fn a_vane_points_its_tail_downwind_whatever_its_parent_does() {
        let mut vane = spinner(
            [0.0, 1.0, 0.0],
            vec![SpinTerm::Vane {
                facing: Fp3([0.0, 0.0, 1.0]),
            }],
        );
        vane.motion.frame = Quat::IDENTITY;
        let east = wind(Vec3::X, 4.0);
        for t in [0.0, 2.0, 5.5, 1.8e9] {
            let a = vane.wind_angle(t, east, Vec3::ZERO);
            let tail = Quat::from_axis_angle(Vec3::Y, a) * Vec3::Z;
            assert!(tail.angle_between(Vec3::X) <= (VANE_HUNT_DEG + 0.01).to_radians());
        }
        // Its parent turned a quarter round already points it downwind.
        vane.motion.frame = Quat::from_rotation_y(FRAC_PI_2);
        assert!(vane.wind_angle(0.0, east, Vec3::ZERO).abs() <= VANE_HUNT_DEG.to_radians());
    }

    #[test]
    fn a_wobble_wanders_within_its_amplitude_and_agrees_with_itself() {
        let buoy = spinner(
            [1.0, 0.0, 0.0],
            vec![SpinTerm::Wobble {
                amplitude: Fp(10.0),
                period: Fp(3.0),
            }],
        );
        let reach = 10f32.to_radians();
        let mut widest = 0.0_f32;
        for k in 0..2000 {
            let t = 1.8e9 + k as f64 * 0.05;
            let a = buoy.clock_angle(t);
            assert!(a.abs() <= reach + 1e-5, "{a}");
            assert_eq!(a, buoy.clock_angle(t), "one time, one angle");
            widest = widest.max(a.abs());
        }
        assert!(widest > 0.5 * reach, "it does wander: {widest}");
    }

    /// Through the real schedule: a lantern hung from a carrier that speeds
    /// up swings back, on an acceleration measured after propagation.
    #[test]
    fn a_lantern_on_a_carrier_that_sets_off_swings_back() {
        let mut app = spin_app();
        let (carrier, lantern) = spawn_spinner(
            &mut app,
            &Spin::about(
                [1.0, 0.0, 0.0],
                SpinTerm::Lean {
                    gain: Fp(5.8),
                    period: Fp(1.2),
                },
            ),
            Transform::IDENTITY,
        );
        // Setting off toward -Z at 3 m/s^2.
        app.add_systems(
            Update,
            move |time: Res<Time>,
                  mut frames: Query<&mut Transform, Without<Spinner>>,
                  mut speed: Local<f32>| {
                let dt = time.delta_secs();
                *speed += 3.0 * dt;
                if let Ok(mut tf) = frames.get_mut(carrier) {
                    tf.translation.z -= *speed * dt;
                }
            },
        );
        for _ in 0..32 {
            app.update();
        }
        let bottom = pose(&app, lantern).rotation * Vec3::NEG_Y;
        assert!(
            bottom.z > 0.05,
            "the lantern's bottom lags behind: {bottom}"
        );
    }

    #[test]
    fn copies_draw_different_offsets_and_one_copy_one_offset() {
        let a = copy_epoch(3, Vec3::new(10.0, 0.0, 20.0));
        assert_eq!(
            a,
            copy_epoch(3, Vec3::new(10.001, 7.0, 20.001)),
            "cm, and no height"
        );
        assert_ne!(a, copy_epoch(3, Vec3::new(10.5, 0.0, 20.0)));
        assert_ne!(a, copy_epoch(4, Vec3::new(10.0, 0.0, 20.0)));
        assert!((0.0..COPY_EPOCH_SPAN_S).contains(&a));
    }

    // --- Slide and swell (#1935) -------------------------------------------

    fn slider(axis: [f32; 3], terms: Vec<SlideTerm>) -> Spinner {
        let slide = Slide {
            axis: Fp3(axis),
            terms,
        };
        Spinner::build(None, Some(&slide), None, Transform::IDENTITY, 0.0)
            .expect("a slide that moves")
    }

    fn sweller(weights: [f32; 3], keep_volume: bool, terms: Vec<SwellTerm>) -> Spinner {
        let swell = Swell {
            weights: Fp3(weights),
            keep_volume,
            terms,
        };
        Spinner::build(None, None, Some(&swell), Transform::IDENTITY, 0.0)
            .expect("a swell that moves")
    }

    fn slide_swing(amplitude: f32, period: f32, phase: f32) -> SlideTerm {
        SlideTerm::Swing {
            amplitude: Fp(amplitude),
            period: Fp(period),
            phase: Fp(phase),
        }
    }

    fn swell_swing(amount: f32, period: f32) -> SwellTerm {
        SwellTerm::Swing {
            amount: Fp(amount),
            period: Fp(period),
            phase: Fp::ZERO,
        }
    }

    #[test]
    fn a_slide_or_swell_alone_gets_a_spinner_and_a_still_one_none() {
        let build = |slide: Option<&Slide>, swell: Option<&Swell>| {
            Spinner::build(None, slide, swell, Transform::IDENTITY, 0.0)
        };
        let bob = Slide::along([0.0, 1.0, 0.0], slide_swing(0.3, 4.0, 0.0));
        assert!(build(Some(&bob), None).is_some());
        assert!(
            build(
                Some(&Slide::along([0.0; 3], slide_swing(0.3, 4.0, 0.0))),
                None
            )
            .is_none()
        );
        assert!(
            build(
                Some(&Slide::along([0.0, 1.0, 0.0], SlideTerm::Unknown)),
                None
            )
            .is_none()
        );
        assert!(build(None, Some(&Swell::even(swell_swing(0.1, 2.0)))).is_some());
        let even_kept = Swell {
            keep_volume: true,
            ..Swell::even(swell_swing(0.1, 2.0))
        };
        assert!(
            build(None, Some(&even_kept)).is_none(),
            "an even swell that keeps its volume changes nothing"
        );
        // Each channel stands alone: a zero spin axis stills the turn, not
        // the slide.
        let still_spin = Spin::about([0.0; 3], constant(30.0));
        let both = Spinner::build(
            Some(&still_spin),
            Some(&bob),
            None,
            Transform::IDENTITY,
            0.0,
        )
        .expect("the slide still moves");
        assert!(both.turns.is_empty() && !both.shifts.is_empty());
    }

    #[test]
    fn a_slide_swing_is_at_its_amplitude_a_quarter_period_in() {
        let buoy = slider([0.0, 1.0, 0.0], vec![slide_swing(0.3, 4.0, 0.0)]);
        assert!(buoy.clock_shift(0.0, 1.0).0.abs() < 1e-6);
        assert!((buoy.clock_shift(1.0, 1.0).0 - 0.3).abs() < 1e-5);
        assert!(buoy.clock_shift(2.0, 1.0).0.abs() < 1e-5);
        assert!((buoy.clock_shift(3.0, 1.0).0 + 0.3).abs() < 1e-5);
        // A phase of 90 degrees starts at the top.
        let late = slider([0.0, 1.0, 0.0], vec![slide_swing(0.3, 4.0, 90.0)]);
        assert!((late.clock_shift(0.0, 1.0).0 - 0.3).abs() < 1e-5);
        // The wall clock's seconds keep their precision.
        let whole = (1.8e9_f64 / 4.0).floor() * 4.0;
        assert!((buoy.clock_shift(whole + 1.0, 1.0).0 - 0.3).abs() < 1e-4);
        let posed = buoy.posed(0.0, buoy.clock_shift(1.0, 1.0).0, 0.0);
        assert!((posed.translation - Vec3::new(0.0, 0.3, 0.0)).length() < 1e-5);
        assert_eq!(posed.rotation, Quat::IDENTITY);
        assert_eq!(posed.scale, Vec3::ONE);
    }

    #[test]
    fn a_slide_wobble_wanders_within_its_amplitude() {
        let drone = slider(
            [1.0, 0.0, 0.0],
            vec![SlideTerm::Wobble {
                amplitude: Fp(0.2),
                period: Fp(3.0),
            }],
        );
        let mut widest = 0.0_f32;
        for k in 0..2000 {
            let t = 1.8e9 + k as f64 * 0.05;
            let offset = drone.clock_shift(t, 1.0).0;
            assert!(offset.abs() <= 0.2 + 1e-6, "{offset}");
            widest = widest.max(offset.abs());
        }
        assert!(widest > 0.1, "it does wander: {widest}");
    }

    #[test]
    fn a_bounce_never_goes_under_and_lands_once_a_period() {
        let ball = slider(
            [0.0, 1.0, 0.0],
            vec![SlideTerm::Bounce {
                height: Fp(0.5),
                period: Fp(1.0),
                phase: Fp::ZERO,
            }],
        );
        for k in 0..400 {
            let t = k as f64 / 100.0;
            let h = ball.clock_shift(t, 1.0).0;
            assert!((0.0..=0.5 + 1e-6).contains(&h), "t {t}: {h}");
        }
        for landing in [0.0, 1.0, 2.0, 7.0, 1.8e9] {
            assert!(
                ball.clock_shift(landing, 1.0).0.abs() < 1e-4,
                "lands at {landing}"
            );
            assert!(
                (ball.clock_shift(landing + 0.5, 1.0).0 - 0.5).abs() < 1e-4,
                "tops out half a period later"
            );
        }
    }

    #[test]
    fn a_belt_runs_within_its_length_and_wraps() {
        let belt = |speed: f32| {
            slider(
                [0.0, 0.0, 1.0],
                vec![SlideTerm::Belt {
                    speed: Fp(speed),
                    length: Fp(2.0),
                    start: Fp::ZERO,
                }],
            )
        };
        let forward = belt(0.5);
        let back = belt(-0.5);
        for k in 0..2000 {
            let t = 1.8e9 + k as f64 * 0.037;
            for b in [&forward, &back] {
                let offset = b.clock_shift(t, 1.0).0;
                assert!((0.0..2.0).contains(&offset), "t {t}: {offset}");
            }
        }
        assert!((forward.clock_shift(1.0, 1.0).0 - 0.5).abs() < 1e-5);
        assert!((forward.clock_shift(3.9, 1.0).0 - 1.95).abs() < 1e-5);
        assert!(
            (forward.clock_shift(4.1, 1.0).0 - 0.05).abs() < 1e-5,
            "past its length it starts again"
        );
        assert!(
            (back.clock_shift(1.0, 1.0).0 - 1.5).abs() < 1e-5,
            "running back, it wraps from the far end"
        );
    }

    /// Four crates at one place, their starts a quarter of the run apart:
    /// at every moment they stand a quarter of the run apart, one stream
    /// filling the run however late the clock.
    #[test]
    fn a_belts_starts_keep_its_crates_evenly_spaced() {
        let crates: Vec<Spinner> = (0..4)
            .map(|k| {
                slider(
                    [1.0, 0.0, 0.0],
                    vec![SlideTerm::Belt {
                        speed: Fp(0.5),
                        length: Fp(4.0),
                        start: Fp(k as f32),
                    }],
                )
            })
            .collect();
        for k in 0..500 {
            let t = 1.8e9 + k as f64 * 0.173;
            let mut at: Vec<f32> = crates.iter().map(|c| c.clock_shift(t, 1.0).0).collect();
            at.sort_by(f32::total_cmp);
            for pair in at.windows(2) {
                assert!((pair[1] - pair[0] - 1.0).abs() < 1e-3, "t {t}: {at:?}");
            }
            assert!(at[0] < 1.0 + 1e-3 && at[3] < 4.0, "t {t}: {at:?}");
        }
    }

    /// The axis is in the node's frame after its authored rotation, as a
    /// spin's is, and the offset is in metres whatever the node's own scale.
    #[test]
    fn a_slide_runs_along_its_turned_axis_in_metres() {
        let base = Transform {
            translation: Vec3::new(1.0, 2.0, 3.0),
            rotation: Quat::from_rotation_z(FRAC_PI_2),
            scale: Vec3::splat(4.0),
        };
        let slide = Slide::along([0.0, 1.0, 0.0], slide_swing(0.3, 4.0, 0.0));
        let s = Spinner::build(None, Some(&slide), None, base, 0.0).expect("moves");
        let posed = s.posed(0.0, 0.5, 0.0);
        // +Y turned a quarter round about Z is -X.
        let moved = posed.translation - base.translation;
        assert!(
            (moved - Vec3::new(-0.5, 0.0, 0.0)).length() < 1e-5,
            "{moved}"
        );
        assert_eq!(posed.rotation, base.rotation);
        assert_eq!(posed.scale, base.scale);
    }

    #[test]
    fn a_swell_grows_each_axis_by_its_weight() {
        let cap = sweller([1.0, 0.5, 0.0], false, vec![swell_swing(0.2, 2.0)]);
        let base = Transform::from_scale(Vec3::new(2.0, 2.0, 2.0));
        let cap = Spinner { base, ..cap };
        let posed = cap.posed(0.0, 0.0, 0.2);
        assert!((posed.scale - Vec3::new(2.4, 2.2, 2.0)).length() < 1e-5);
        assert_eq!(posed.translation, base.translation);
        assert_eq!(posed.rotation, base.rotation);
        // The swing's amount a quarter period in.
        assert!((cap.clock_shift(0.5, 1.0).1 - 0.2).abs() < 1e-5);
    }

    #[test]
    fn a_swell_that_keeps_its_volume_squashes_as_it_stretches() {
        let blob = sweller([0.0, 1.0, 0.0], true, vec![swell_swing(0.5, 2.0)]);
        for amount in [-2.0, -0.9, -0.3, 0.0, 0.4, 0.9, 3.6] {
            let f = blob.swell_factors(amount);
            assert!(
                (f.x * f.y * f.z - 1.0).abs() < 1e-5,
                "{amount}: {f} keeps no volume"
            );
            assert!(
                f.min_element() >= limits::MIN_SWELL_FACTOR - 1e-6,
                "{amount}: {f}"
            );
        }
        let tall = blob.swell_factors(0.5);
        assert!(tall.y > 1.0 && tall.x < 1.0 && (tall.x - tall.z).abs() < 1e-6);
    }

    #[test]
    fn a_summed_swell_never_shrinks_an_axis_past_a_tenth() {
        // Four of the deepest terms summed, at the bottom of each swing.
        let sink = sweller(
            [1.0, 0.25, 0.5],
            false,
            vec![swell_swing(limits::MAX_SWELL_AMOUNT, 2.0); 4],
        );
        let amount = sink.clock_shift(1.5, 1.0).1;
        assert!(
            (amount + 4.0 * limits::MAX_SWELL_AMOUNT).abs() < 1e-4,
            "{amount}"
        );
        let f = sink.swell_factors(amount);
        assert!(f.min_element() >= limits::MIN_SWELL_FACTOR - 1e-6, "{f}");
        assert!(
            (f.x - limits::MIN_SWELL_FACTOR).abs() < 1e-5,
            "the widest weight sets the floor: {f}"
        );
        assert!(f.y > f.x && f.z > f.x);
        assert_eq!(sink.swell_factors(f32::NAN), Vec3::ONE);
    }

    /// One clock and one copy offset for every channel: a turn's swing, a
    /// slide's and a swell's of one period and phase peak together.
    #[test]
    fn a_turn_slide_and_swell_of_one_period_keep_phase() {
        let spin = Spin::about(
            [1.0, 0.0, 0.0],
            SpinTerm::Swing {
                amplitude: Fp(30.0),
                period: Fp(3.0),
                phase: Fp(40.0),
            },
        );
        let slide = Slide::along([0.0, 1.0, 0.0], slide_swing(0.3, 3.0, 40.0));
        let swell = Swell::even(SwellTerm::Swing {
            amount: Fp(0.2),
            period: Fp(3.0),
            phase: Fp(40.0),
        });
        let s = Spinner::build(
            Some(&spin),
            Some(&slide),
            Some(&swell),
            Transform::IDENTITY,
            123.4,
        )
        .expect("moves");
        for k in 0..300 {
            let now = 1.8e9 + k as f64 * 0.07;
            let share = s.clock_angle(now) / 30f32.to_radians();
            let (offset, amount) = s.clock_shift(now, 1.0);
            assert!((offset / 0.3 - share).abs() < 1e-3, "{now}");
            assert!((amount / 0.2 - share).abs() < 1e-3, "{now}");
        }
    }

    /// Downwind along the slide axis moves the part forward, against it
    /// back, across it not at all; a swell fills whichever way it blows.
    #[test]
    fn the_wind_slides_a_part_downwind_along_its_axis_and_fills_a_swell() {
        let mut flag = slider(
            [1.0, 0.0, 0.0],
            vec![SlideTerm::Wind { amplitude: Fp(0.2) }],
        );
        flag.motion.frame = Quat::IDENTITY;
        for t in [0.0, 1.3, 7.7, 1.8e9] {
            let along = flag.wind_shift(t, wind(Vec3::X, 4.0), Vec3::ZERO, 1.0).0;
            assert!(along > 0.0 && along <= 0.2 + 1e-6, "t {t}: {along}");
            let against = flag
                .wind_shift(t, wind(Vec3::NEG_X, 4.0), Vec3::ZERO, 1.0)
                .0;
            assert!(against < 0.0, "t {t}: {against}");
        }
        assert_eq!(
            flag.wind_shift(3.0, wind(Vec3::Z, 4.0), Vec3::ZERO, 1.0).0,
            0.0
        );
        assert_eq!(
            flag.wind_shift(3.0, wind(Vec3::ZERO, 0.0), Vec3::ZERO, 1.0)
                .0,
            0.0
        );
        // Its parent turned a quarter round carries the axis with it.
        flag.motion.frame = Quat::from_rotation_y(FRAC_PI_2);
        assert!(
            flag.wind_shift(3.0, wind(Vec3::X, 4.0), Vec3::ZERO, 1.0)
                .0
                .abs()
                < 1e-6
        );

        let mut sail = sweller(
            [1.0, 1.0, 1.0],
            false,
            vec![SwellTerm::Wind { amount: Fp(0.1) }],
        );
        sail.motion.frame = Quat::IDENTITY;
        for dir in [Vec3::X, Vec3::NEG_Z] {
            let filled = sail.wind_shift(2.0, wind(dir, 4.0), Vec3::ZERO, 1.0).1;
            assert!(filled > 0.0 && filled <= 0.1 + 1e-6, "{filled}");
        }
    }

    /// One motion built two ways moves alike: a node that slides and leans
    /// leans exactly as a hub with the slide does the lean on its child, and
    /// the same for a slide and a roll - every part is measured as drawn,
    /// its own slide included.
    #[test]
    fn a_slide_on_the_node_moves_its_motion_terms_as_one_on_its_carrier() {
        let sliding = || Slide::along([0.0, 0.0, 1.0], slide_swing(2.0, 0.5, 0.0));
        let lean = || {
            Spin::about(
                [1.0, 0.0, 0.0],
                SpinTerm::Lean {
                    gain: Fp(30.0),
                    period: Fp(0.5),
                },
            )
        };
        // A wheel about X rolls with travel along Z.
        let roll = || Spin::about([1.0, 0.0, 0.0], SpinTerm::Roll { radius: Fp(0.4) });
        let spawn = |app: &mut App, node: Generator, parent: Entity| {
            let spinner = Spinner::of_node(&node, Transform::IDENTITY, 0.0).expect("moves");
            app.world_mut()
                .spawn((
                    spinner,
                    Transform::IDENTITY,
                    InheritedVisibility::VISIBLE,
                    ChildOf(parent),
                ))
                .id()
        };
        for (name, spin) in [("lean", lean()), ("roll", roll())] {
            let mut app = spin_app();
            let ground = app.world_mut().spawn(Transform::IDENTITY).id();
            // One node: the slide and the turn together.
            let mut one = Generator::default_cuboid();
            one.slide = Some(sliding());
            one.spin = Some(spin.clone());
            let together = spawn(&mut app, one, ground);
            // Two: the slide on a hub, the turn on its child.
            let mut hub = Generator::default_cuboid();
            hub.slide = Some(sliding());
            let hub = spawn(&mut app, hub, ground);
            let mut child = Generator::default_cuboid();
            child.spin = Some(spin);
            let nested = spawn(&mut app, child, hub);

            let mut turned = 0.0_f32;
            for frame in 0..96 {
                app.update();
                let (a, b) = (pose(&app, together).rotation, pose(&app, nested).rotation);
                // Component by component: `angle_between` reads an acos
                // near one, where a rounding of the dot is a whole 1e-3.
                assert!(
                    a.abs_diff_eq(b, 1e-5),
                    "{name}, frame {frame}: {a} against {b}"
                );
                turned = turned.max(a.angle_between(Quat::IDENTITY));
            }
            assert!(turned > 0.05, "{name}: it does turn at its slide: {turned}");
        }
    }

    /// A spawned slider, swell and spin together, under a parent at the
    /// origin, at `base`.
    fn spawn_mover(app: &mut App, base: Transform) -> Entity {
        let mut node = Generator::default_cuboid();
        node.spin = Some(Spin::about([0.0, 1.0, 0.0], constant(90.0)));
        node.slide = Some(Slide::along([0.0, 1.0, 0.0], slide_swing(0.3, 4.0, 0.0)));
        node.swell = Some(Swell::even(swell_swing(0.2, 4.0)));
        let parent = app.world_mut().spawn(Transform::IDENTITY).id();
        let spinner = Spinner::of_node(&node, base, 0.0).expect("moves");
        app.world_mut()
            .spawn((spinner, base, InheritedVisibility::VISIBLE, ChildOf(parent)))
            .id()
    }

    #[test]
    fn a_selection_snaps_place_and_size_home_and_rebases_all_three() {
        let mut app = spin_app();
        let base = Transform::from_xyz(1.0, 2.0, 3.0);
        let part = spawn_mover(&mut app, base);
        for _ in 0..40 {
            app.update();
        }
        let moving = pose(&app, part);
        assert!(moving.translation.distance(base.translation) > 0.05);
        assert!(moving.scale.distance(base.scale) > 0.02);

        hold(&mut app, true);
        app.update();
        assert_eq!(pose(&app, part), base, "snapped home in every channel");

        // The gizmo moves and resizes the part; released, it moves from
        // there - its new place and size are the authored ones.
        let dragged = Transform::from_xyz(5.0, 2.0, 3.0).with_scale(Vec3::splat(2.0));
        *app.world_mut().get_mut::<Transform>(part).unwrap() = dragged;
        app.update();
        assert_eq!(pose(&app, part), dragged, "the gizmo owns the pose");
        hold(&mut app, false);
        for _ in 0..40 {
            app.update();
        }
        let after = pose(&app, part);
        let swing = after.translation - dragged.translation;
        assert!(swing.x.abs() < 1e-5 && swing.z.abs() < 1e-5 && swing.y.abs() <= 0.3 + 1e-5);
        let ratio = after.scale / dragged.scale;
        assert!(
            (ratio.x - ratio.y).abs() < 1e-5 && (ratio.x - 1.0).abs() <= 0.2 + 1e-5,
            "{ratio}"
        );
    }

    #[test]
    fn effects_off_holds_every_channel_at_its_authored_pose() {
        let mut app = spin_app();
        app.insert_resource(crate::state::LocalSettings {
            effects_intensity: crate::state::EffectsIntensity::Off,
            ..Default::default()
        });
        let base = Transform::from_xyz(0.0, 1.0, 0.0).with_scale(Vec3::splat(1.5));
        let part = spawn_mover(&mut app, base);
        for _ in 0..40 {
            app.update();
        }
        assert_eq!(pose(&app, part), base);
        assert!(settled(&app));
    }

    /// Reduced moves a slide and a swell half as far and leaves the turn as
    /// authored: the same frames, the same clock, side by side.
    #[test]
    fn effects_reduced_halves_a_slide_and_a_swell_but_not_a_turn() {
        let run = |effects| {
            let mut app = spin_app();
            app.insert_resource(crate::state::LocalSettings {
                effects_intensity: effects,
                ..Default::default()
            });
            let part = spawn_mover(&mut app, Transform::IDENTITY);
            for _ in 0..40 {
                app.update();
            }
            pose(&app, part)
        };
        let full = run(crate::state::EffectsIntensity::Full);
        let calm = run(crate::state::EffectsIntensity::Reduced);
        assert!(full.translation.y.abs() > 0.05, "{full:?}");
        assert!((calm.translation.y - 0.5 * full.translation.y).abs() < 1e-5);
        let grew = |tf: Transform| tf.scale.x - 1.0;
        assert!(grew(full).abs() > 0.02);
        assert!((grew(calm) - 0.5 * grew(full)).abs() < 1e-5);
        assert_eq!(calm.rotation, full.rotation, "a turn is left as authored");
    }

    /// The critic's probe (#1935): a 4 m belt at half a metre a second. At
    /// Reduced it runs at half speed over the same run - its offset reaches
    /// the far end before it jumps back, as at Full - rather than jumping
    /// back halfway along, in plain sight.
    #[test]
    fn effects_reduced_runs_a_belt_slower_over_its_whole_length() {
        let reach = |effects| {
            let mut app = spin_app();
            app.insert_resource(crate::state::LocalSettings {
                effects_intensity: effects,
                ..Default::default()
            });
            let parent = app.world_mut().spawn(Transform::IDENTITY).id();
            let mut node = Generator::default_cuboid();
            node.slide = Some(Slide::along(
                [1.0, 0.0, 0.0],
                SlideTerm::Belt {
                    speed: Fp(0.5),
                    length: Fp(4.0),
                    start: Fp::ZERO,
                },
            ));
            let spinner = Spinner::of_node(&node, Transform::IDENTITY, 0.0).expect("moves");
            let part = app
                .world_mut()
                .spawn((
                    spinner,
                    Transform::IDENTITY,
                    InheritedVisibility::VISIBLE,
                    ChildOf(parent),
                ))
                .id();
            // Long enough for a half-speed run to reach its end: 4 m at a
            // quarter metre a second is 16 s, and a frame is 1/64 s.
            let mut furthest = 0.0_f32;
            for _ in 0..(22 * 64) {
                app.update();
                let x = pose(&app, part).translation.x;
                if x < furthest - 1.0 {
                    return furthest;
                }
                furthest = furthest.max(x);
            }
            panic!("{effects:?}: the belt never jumped back (reached {furthest})");
        };
        let full = reach(crate::state::EffectsIntensity::Full);
        let calm = reach(crate::state::EffectsIntensity::Reduced);
        assert!(full > 0.99 * 4.0, "Full reaches {full}");
        assert!(calm > 0.99 * 4.0, "Reduced reaches {calm}");
    }

    /// Every other slide term moves exactly half as far at Reduced at the
    /// same moment, its phase unchanged, and a swell breathes half as much.
    #[test]
    fn effects_reduced_halves_each_slide_term_at_the_same_phase() {
        let mut s = Spinner::build(
            None,
            Some(&Slide {
                axis: Fp3([1.0, 0.0, 0.0]),
                terms: vec![
                    slide_swing(0.3, 4.0, 30.0),
                    SlideTerm::Wobble {
                        amplitude: Fp(0.2),
                        period: Fp(3.0),
                    },
                    SlideTerm::Bounce {
                        height: Fp(0.5),
                        period: Fp(1.3),
                        phase: Fp(10.0),
                    },
                    SlideTerm::Wind { amplitude: Fp(0.4) },
                ],
            }),
            Some(&Swell {
                weights: Fp3([1.0; 3]),
                keep_volume: false,
                terms: vec![swell_swing(0.2, 2.0), SwellTerm::Wind { amount: Fp(0.1) }],
            }),
            Transform::IDENTITY,
            17.0,
        )
        .expect("moves");
        s.motion.frame = Quat::IDENTITY;
        let blow = wind(Vec3::X, 6.0);
        let mut moved = 0.0_f32;
        for k in 0..400 {
            let now = 1.8e9 + k as f64 * 0.053;
            let (full, calm) = (s.clock_shift(now, 1.0), s.clock_shift(now, 0.5));
            assert_eq!(calm.0, 0.5 * full.0, "{now}");
            assert_eq!(calm.1, 0.5 * full.1, "{now}");
            let (full, calm) = (
                s.wind_shift(now, blow, Vec3::ZERO, 1.0),
                s.wind_shift(now, blow, Vec3::ZERO, 0.5),
            );
            assert_eq!(calm.0, 0.5 * full.0, "{now}");
            assert_eq!(calm.1, 0.5 * full.1, "{now}");
            moved = moved.max(full.0.abs());
        }
        assert!(moved > 0.1, "{moved}");
    }

    /// What the runtime draws never passes the bound the avatar scale cap
    /// reads ([`Swell::peak_factor`]), wind at its strongest included, and
    /// reaches it when the terms line up.
    #[test]
    fn a_drawn_swell_stays_within_its_peak_factor() {
        let swells = [
            Swell {
                terms: vec![swell_swing(0.9, 2.0); 4],
                ..Swell::default()
            },
            Swell {
                weights: Fp3([0.0, 1.0, 0.0]),
                keep_volume: true,
                terms: vec![swell_swing(0.9, 2.0), SwellTerm::Wind { amount: Fp(-0.5) }],
            },
            Swell {
                weights: Fp3([0.2, 0.6, 1.0]),
                keep_volume: true,
                terms: vec![SwellTerm::Wind { amount: Fp(0.9) }, swell_swing(0.3, 3.0)],
            },
        ];
        for swell in swells {
            let peak = swell.peak_factor();
            let mut s =
                Spinner::build(None, None, Some(&swell), Transform::IDENTITY, 0.0).expect("moves");
            s.motion.frame = Quat::IDENTITY;
            let mut widest = 0.0_f32;
            for k in 0..4000 {
                let now = k as f64 * 0.011;
                // A gale far past the strongest wind the scale reads.
                for speed in [0.0, 4.0, 1e3] {
                    let amount = s.clock_shift(now, 1.0).1
                        + s.wind_shift(now, wind(Vec3::X, speed), Vec3::ZERO, 1.0).1;
                    widest = widest.max(s.swell_factors(amount).max_element());
                }
            }
            assert!(
                widest <= peak + 1e-4,
                "{swell:?}: drew {widest} past {peak}"
            );
            assert!(
                widest > 0.97 * peak,
                "{swell:?}: the bound is tight: {widest} {peak}"
            );
        }
    }
}
