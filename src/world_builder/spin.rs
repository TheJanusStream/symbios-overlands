//! The runtime of a node's spin (#1604): each client turning the parts a
//! record says turn, for itself, without a byte on the wire.
//!
//! The spawner gives every node whose [`Spin`] moves a [`Spinner`], holding
//! the node's authored local pose and its terms in radians and seconds. Two
//! systems then run every frame, around transform propagation:
//!
//! - [`animate_spinners`], before it, writes each spinner's `Transform` as
//!   its authored pose turned about the node's axis by the sum of its terms.
//!   Clock terms ([`SpinTerm::Constant`], [`SpinTerm::Swing`]) are a
//!   function of the time - UTC in the game, so every client shows a
//!   windmill at the same angle and a spinner skipped while out of sight
//!   comes back at the right one. Motion terms ([`SpinTerm::Roll`],
//!   [`SpinTerm::Steer`]) integrate what [`sample_spinner_motion`] measured.
//! - [`sample_spinner_motion`], after it, differences each motion
//!   spinner's world position, and its parent's world rotation, over the
//!   frame they spanned. After propagation is the only place that delta is
//!   honest for a remote peer, whose pose propagates only in `PostUpdate`
//!   (#1323).
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
//! recompile arrives. Settings > Effects intensity Off holds every spinner
//! the same way.
//!
//! A spinner that has lost its parent is the gizmo's - it detaches its
//! target into world space - and is never written.

use std::f32::consts::{PI, TAU};

use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crate::pds::sanitize::limits;
use crate::pds::spin::{Spin, SpinTerm};

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

    /// The time now (s).
    fn now(&mut self, time: &Time, real: &Time<Real>) -> f64 {
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

/// One term, in the units the runtime turns by.
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
    /// Radians of turn per radian per second of the parent's turning, and
    /// the radians it is held within.
    Steer { gain: f32, limit: f32 },
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
            SpinTerm::Steer { gain, limit } => Self::Steer {
                gain: gain.0,
                limit: limit.0.to_radians(),
            },
            SpinTerm::Unknown => return None,
        })
    }
}

/// What [`sample_spinner_motion`] last measured of a motion spinner.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Motion {
    /// World position and parent world rotation at the last sample.
    last: Option<(Vec3, Quat)>,
    /// World velocity over the last frame (m/s).
    velocity: Vec3,
    /// The parent's world angular velocity over the last frame (rad/s).
    turning: Vec3,
    /// The spin axis in world space, at the last sample.
    axis_world: Vec3,
    /// The velocity along the parent's +Z - forward, for every vehicle in
    /// Overlands is drawn facing +Z - at the last sample (m/s).
    forward_speed: f32,
}

/// A node that turns (#1604): its authored pose, its axis and terms, and
/// what its motion terms have accumulated.
#[derive(Component, Clone, Debug)]
pub struct Spinner {
    /// The node's authored local pose, which every turn composes onto.
    base: Transform,
    /// Unit axis in the node's frame.
    axis: Vec3,
    turns: Vec<Turn>,
    /// This copy's clock offset (s), see [`copy_epoch`].
    epoch: f64,
    /// Whether any term reads the part's motion.
    follows_motion: bool,
    /// The angle the roll terms have turned (rad, wrapped).
    rolled: f32,
    /// The steer terms' settled angle (rad).
    steered: f32,
    motion: Motion,
}

impl Spinner {
    /// The spinner for a node at `base` with `spin`, or `None` when the spin
    /// turns nothing. `epoch` is the copy's clock offset.
    pub(crate) fn new(spin: &Spin, base: Transform, epoch: f64) -> Option<Self> {
        let axis = Vec3::from_array(spin.axis.0).try_normalize()?;
        let turns: Vec<Turn> = spin.terms.iter().filter_map(Turn::from_term).collect();
        if turns.is_empty() {
            return None;
        }
        let follows_motion = spin.terms.iter().any(|t| t.moves() && t.follows_motion());
        Some(Self {
            base,
            axis,
            turns,
            epoch,
            follows_motion,
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
                } => {
                    let cycle = (t * per_second).rem_euclid(1.0);
                    angle +=
                        amplitude as f64 * (std::f64::consts::TAU * cycle + phase as f64).sin();
                }
                Turn::Roll { .. } | Turn::Steer { .. } => {}
            }
        }
        angle as f32
    }

    /// Advance the motion terms by `dt` on the last sampled motion.
    fn follow_motion(&mut self, dt: f32) {
        let Motion {
            velocity,
            turning,
            axis_world,
            forward_speed,
            ..
        } = self.motion;
        // Reversing through a bend turns the vehicle the other way for the
        // same wheels, so the wheels that follow its turning turn back.
        let heading = if forward_speed < -REVERSING_M_S {
            -1.0
        } else {
            1.0
        };
        let mut rate = 0.0;
        let mut steer = 0.0;
        for turn in &self.turns {
            match *turn {
                // A wheel rolling with velocity v on ground whose normal is
                // up turns at (up x v) / r: project that onto the axis, so
                // the sign follows the axis and a wheel turned across its
                // travel rolls only as far as it is carried along it.
                Turn::Roll { per_metre } => {
                    rate += Vec3::Y.cross(velocity).dot(axis_world) * per_metre;
                }
                Turn::Steer { gain, limit } => {
                    steer += (heading * gain * turning.dot(axis_world)).clamp(-limit, limit);
                }
                Turn::Constant(_) | Turn::Swing { .. } => {}
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

    /// The node's pose turned by `angle`.
    fn posed(&self, angle: f32) -> Transform {
        Transform {
            rotation: self.base.rotation * Quat::from_axis_angle(self.axis, angle),
            ..self.base
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
    let held = hold.editing || settings.is_some_and(|s| !s.effects_intensity.plays());
    if held {
        if !*frozen {
            for (mut spinner, mut tf, ..) in &mut spinners {
                *tf = spinner.base;
                spinner.rolled = 0.0;
                spinner.steered = 0.0;
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
        let angle = spinner.clock_angle(now) + spinner.rolled + spinner.steered;
        *tf = spinner.posed(angle);
    }
}

/// Measure each motion spinner's travel and its parent's turning over the
/// frame just propagated. See the module docs for why it runs after
/// propagation.
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
        if !spinner.follows_motion {
            continue;
        }
        let at = global.translation();
        let parent = frames
            .get(child_of.parent())
            .map_or(Quat::IDENTITY, GlobalTransform::rotation);
        let axis_world = (parent * (spinner.base.rotation * spinner.axis)).normalize_or_zero();
        let motion = &mut spinner.motion;
        if let Some((last_at, last_parent)) = motion.last {
            motion.velocity = (at - last_at) / dt;
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

    #[test]
    fn the_turn_composes_onto_the_authored_pose_rigidly() {
        let base = Transform {
            translation: Vec3::new(1.0, 2.0, 3.0),
            rotation: Quat::from_rotation_y(0.7),
            scale: Vec3::new(2.0, 1.0, 0.5),
        };
        let s =
            Spinner::new(&Spin::about([0.0, 0.0, 1.0], constant(10.0)), base, 0.0).expect("moves");
        let posed = s.posed(1.2);
        assert_eq!(posed.translation, base.translation);
        assert_eq!(posed.scale, base.scale);
        // The axis is in the node's frame after its own rotation: the node's
        // local Z, wherever the authored rotation pointed it, stays put.
        let axis_out = posed.rotation * Vec3::Z;
        let axis_in = base.rotation * Vec3::Z;
        assert!(axis_out.distance(axis_in) < 1e-5);
        assert!(s.posed(0.0).rotation.angle_between(base.rotation) < 1e-6);
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
}
