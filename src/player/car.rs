//! Car preset - ground vehicle with 4-corner raycast suspension, no
//! buoyancy.
//!
//! Controls, on the ground - each acts through the wheels, so each is scaled
//! by the share of the four corners in contact ([`CarContact`], #1524):
//!   * **W / S** - forward / reverse drive force.
//!   * **A / D** - yaw torque (steer), inverted while reversing (#723).
//!   * **Space** - handbrake: cuts the forward force and multiplies lateral
//!     grip by `handbrake_grip_factor` (typically <1, letting the rear
//!     slip out).
//!
//! In the air nothing pushes, steers or grips, so a jump flies
//! ballistically. While any wheel is off the ground the air model takes over,
//! scaled by the share of wheels OFF it:
//!   * **W / S** - pitch the nose down / up.
//!   * **A / D** - yaw left / right.
//!   * **Q / E** - roll left / right.
//!   * No key on pitch or roll - the chassis levels itself on that axis.
//!
//! W/S/A/D count in the air only when pressed with no wheel down: one held
//! while a wheel touched is driving or steering, and acts in the air once let
//! go and pressed again, so a driver holding W or a turn off a ramp lands
//! level instead of nosing in. Q/E do nothing on the ground and count whenever
//! they are held. No air key counts while the car lies on the ground - on
//! its roof, a side or its nose ([`CarContact::lying`]): that is not the air,
//! and the uprighting assist rights it. The keys are the player's and stand
//! down while the player types; the levelling, and the swap of the chassis'
//! dampings for the record's lighter air ones, are passive and do not
//! ([`apply_car_suspension`], #821).
//!
//! The car has no buoyancy (it sinks in water). It resists rollover through
//! a low centre of mass (set in [`super::preset`]) and recovers from a flip
//! through a gated uprighting assist ([`apply_car_uprighting`]) that engages
//! only near the ground and only once the chassis is tipped well past any
//! cornering lean, so a car can never end up stuck on its roof (#804). Neither
//! it nor the levelling acts on an axis a counted air key holds, so a flip in
//! the air is the player's to finish for as long as they hold the key; once
//! the car lies on the ground no key counts, and the assist has every axis.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::config::rover as cfg;
use crate::pds::{CarParams, LocomotionConfig};
use crate::state::{LiveAvatarRecord, LocalPlayer, TravelingTo};

use super::{CarPreset, chassis_corners};

/// What the car's four suspension rays found on the last fixed step (#1524):
/// how many wheels are on the ground, whether any ground is near at all, and
/// whether the car lies on it.
///
/// Written by [`apply_car_suspension`], which runs first in the car's chain
/// and is not input-gated, so the reading is fresh whatever the player is
/// doing; read by [`apply_car_drive`] for traction and the air control, and
/// by [`apply_car_uprighting`], which acts only near the ground.
///
/// The chassis is built with [`CarContact::default`], which is FULLY
/// GROUNDED. The planar drive probe in `spawn`'s tests runs the drive with no
/// suspension and no ground, and it measures the planar feel only because the
/// drive there always reads four wheels down.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CarContact {
    /// Corners whose ray met the ground within the suspension's rest length
    /// plus [`cfg::CAR_CONTACT_SLACK_M`], counted only while the chassis is
    /// within [`cfg::CAR_WHEEL_TILT_LIMIT_DEGREES`] of upright: 0 to 4.
    pub(crate) wheels: u8,
    /// Any corner ray met the ground within its full reach (rest length plus
    /// 1.5 m), or the body itself is touching something solid. The corner
    /// rays of a car on its roof or side start above the chassis and pass
    /// through it, so they still reach the ground unless the box is very
    /// tall or very wide; the touch covers those, which a record may author.
    /// Touching means a point of the box within
    /// [`cfg::CAR_BODY_TOUCH_M`] of the thing, not merely one of avian's
    /// speculative contacts.
    pub(crate) near_ground: bool,
    /// The car lies on the ground rather than flying or driving: its box on
    /// something solid, or tipped past its wheels with a corner within a
    /// spring's length of the ground. A car lying there is not in the air,
    /// so the air keys do not count and neither does the levelling: it is
    /// the uprighting assist's to right, on every axis.
    pub(crate) lying: bool,
}

impl Default for CarContact {
    fn default() -> Self {
        Self {
            wheels: 4,
            near_ground: true,
            lying: false,
        }
    }
}

impl CarContact {
    /// The share of the car's traction that reaches the ground: 0 in the
    /// air, 1 with all four wheels down.
    pub(crate) fn traction(self) -> f32 {
        f32::from(self.wheels.min(4)) / 4.0
    }
}

/// The car suspension's bump stop (#1524): past `start` of the rest length a
/// corner adds a force rising with the square of how far past it is, and
/// damps the corner both ways, so a landing keeps the chassis box off the
/// ground where the linear spring alone let it hit.
///
/// Every term is per kilogram of the corner's quarter of the mass, so the
/// stop holds any car the same way. It is shaped by what a 64 Hz fixed step
/// can integrate, each part MEASURED on the drive bench:
///
/// * The spring is weighed where the corner will be at the END of the step
///   (`compression + closing speed x dt`). The force is held for the whole
///   step, and a stiff one weighed at the step's start pushes too little
///   going in and too much coming out: this stop weighed that way let a 3 m
///   drop down to 5 mm over the floor and threw it back up at 4.3 m/s,
///   harder than the box hitting the ground does (3.7). Weighed ahead it
///   brakes earlier and lets go sooner: 38 mm, and 2.8 m/s.
/// * The look-ahead is a share of the stop's travel like the depth is, so
///   it stops at full travel, where the box is on the ground. Unbounded,
///   the first, stiffer stop tried threw a car that landed at 9 m/s back up
///   at 10.
/// * The damping works both ways. Damping only the compression lets the
///   spring hand its energy back: from 2 m / 3 m a car rebounded at 4.7 /
///   6.2 m/s, against 2.3 / 2.8 both ways and 3.7 with no stop at all.
///
/// A resource, which the game never inserts, so the default - the
/// `config::rover` constants - is what every car rides on; the drive bench
/// inserts `CarBumpStop::OFF` to measure a car without it.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub(crate) struct CarBumpStop {
    /// Compression, as a fraction of the rest length, where the stop begins.
    pub(crate) start: f32,
    /// Rate (1/s^2): per kilogram of the corner, the stop pushes with this
    /// times the stop's travel times the square of how far into it (0 to 1)
    /// the corner will be at the end of the step.
    pub(crate) rate: f32,
    /// Damping (1/s) at full travel, fading to none at `start`: per
    /// kilogram of the corner, this times the depth times the speed the
    /// corner closes on the ground at, or opens from it.
    pub(crate) damping: f32,
}

impl Default for CarBumpStop {
    fn default() -> Self {
        Self {
            start: cfg::CAR_BUMP_STOP_START,
            rate: cfg::CAR_BUMP_STOP_RATE,
            damping: cfg::CAR_BUMP_STOP_DAMPING,
        }
    }
}

impl CarBumpStop {
    /// No stop at all: the suspension as it was before #1524.
    #[cfg(test)]
    pub(crate) const OFF: Self = Self {
        start: 1.0,
        rate: 0.0,
        damping: 0.0,
    };

    /// The stop's extra force (N) at a corner compressed `compression` m of
    /// its `rest` m of travel, closing on the ground at `closing_speed` m/s
    /// (negative while it opens), carrying `corner_mass` kg, for a step of
    /// `dt` s.
    fn force(
        self,
        compression: f32,
        closing_speed: f32,
        rest: f32,
        corner_mass: f32,
        dt: f32,
    ) -> f32 {
        let start = self.start * rest;
        let travel = rest - start;
        if travel <= 0.0 {
            return 0.0;
        }
        let depth = ((compression - start) / travel).clamp(0.0, 1.0);
        let ahead = ((compression + closing_speed * dt - start) / travel).clamp(0.0, 1.0);
        let spring = self.rate * travel * ahead * ahead;
        let damping = self.damping * depth * closing_speed;
        corner_mass * (spring + damping)
    }
}

/// `air` blended toward `ground` by the traction `t`, written so both ends
/// are EXACT: `t = 1` gives `ground` to the bit, which is what keeps a car
/// with four wheels down on the record's damping exactly as before #1524
/// (`a + (b - a) * t` can miss `b` by an ulp).
fn blend(air: f32, ground: f32, t: f32) -> f32 {
    air * (1.0 - t) + ground * t
}

/// The car's four corner springs, each with its bump stop, pushing along
/// world +Y (slopes cost a car nothing, and the fleet's tuning rests on it);
/// then what the rays found, as [`CarContact`], the dampings that follow from
/// it - the record's with every wheel down, its air ones with none - and the
/// air self-levelling ([`air_level_torque`]). First in the car's chain and
/// never input-gated: all of it is passive.
#[allow(clippy::type_complexity)]
pub(super) fn apply_car_suspension(
    live: Res<LiveAvatarRecord>,
    mut query: Query<
        (
            Entity,
            Forces,
            &GlobalTransform,
            &mut CarContact,
            &mut CarAirKeys,
            &mut LinearDamping,
            &mut AngularDamping,
        ),
        (With<LocalPlayer>, With<CarPreset>),
    >,
    sensors: Query<Entity, With<Sensor>>,
    spatial_query: SpatialQuery,
    contacts: Option<Res<ContactGraph>>,
    bump_stop: Option<Res<CarBumpStop>>,
    time: Res<Time>,
) {
    let LocomotionConfig::Car(p) = &live.0.locomotion else {
        return;
    };
    let bump_stop = bump_stop.as_deref().copied().unwrap_or_default();
    let Ok((
        chassis_entity,
        mut forces,
        global_tf,
        mut contact,
        mut air_keys,
        mut linear_damping,
        mut angular_damping,
    )) = query.single_mut()
    else {
        return;
    };

    let half_extents = Vec3::from_array(p.chassis_half_extents.0);
    let corners = chassis_corners(half_extents);
    let ray_max = p.suspension_rest_length.0 + 1.5;
    let wheel_reach = p.suspension_rest_length.0 + cfg::CAR_CONTACT_SLACK_M;
    // A corner is a wheel only while the chassis is the right way up - see
    // `CAR_WHEEL_TILT_LIMIT_DEGREES`. Past it the corner rays are cast from
    // the side or the roof, through the body, and they only measure a car
    // lying on the ground: no wheel counts and no spring pushes. A spring
    // there pushed the lower side of a car lying on its side up, off-centre,
    // and rolled it onto its roof - #804's failure at HEAD, measured on the
    // drive bench.
    let on_its_wheels = global_tf.up().y >= cfg::CAR_WHEEL_TILT_LIMIT_DEGREES.to_radians().cos();
    let chassis_tf = global_tf.compute_transform();
    // Exclude self + every sensor so the suspension never rests on a gateway
    // veil / portal (#813) - see [`super::ground_ray_filter`].
    let filter = super::ground_ray_filter(chassis_entity, sensors.iter());
    let lin_vel = forces.linear_velocity();
    let ang_vel = forces.angular_velocity();
    let center_of_mass = global_tf.translation();

    let mut wheels = 0u8;
    let mut near_ground = false;
    // A corner of a car past its wheels within a spring's length of the
    // ground: the car is lying there.
    let mut corner_down = false;
    for local_offset in corners {
        let world_origin = chassis_tf.transform_point(local_offset);
        let Some(hit) = spatial_query.cast_ray(world_origin, Dir3::NEG_Y, ray_max, true, &filter)
        else {
            continue;
        };
        near_ground = true;
        let compression = p.suspension_rest_length.0 - hit.distance;
        if !on_its_wheels {
            corner_down |= compression > 0.0;
            continue;
        }
        if hit.distance <= wheel_reach {
            wheels += 1;
        }

        if compression > 0.0 {
            let r = world_origin - center_of_mass;
            let point_vel = lin_vel + ang_vel.cross(r);
            let closing_speed = -point_vel.dot(hit.normal);
            let spring_force = p.suspension_stiffness.0 * compression;
            let damping_force = p.suspension_damping.0 * closing_speed;
            let bump_force = bump_stop.force(
                compression,
                closing_speed,
                p.suspension_rest_length.0,
                p.mass.0 * 0.25,
                time.delta_secs(),
            );
            let total_force = (spring_force + damping_force + bump_force).max(0.0);
            forces.apply_force_at_point(Vec3::Y * total_force, world_origin);
        }
    }
    // A body resting on something solid is near the ground whatever its
    // rays say - the tall or wide box whose roof-side rays fall short.
    // Sensors are not ground here any more than they are to the rays. Read
    // off the contact points, not avian's `CollidingEntities`: that counts a
    // moving body as touching whatever it could close on in a step - 23 cm
    // at 15 m/s - and paused the levelling below at every lip and every
    // landing, measured on the drive bench.
    let body_touching = contacts.as_deref().is_some_and(|graph| {
        graph
            .contact_pairs_with(chassis_entity)
            .filter(|pair| pair.is_touching())
            .any(|pair| {
                let other = if pair.collider1 == chassis_entity {
                    pair.collider2
                } else {
                    pair.collider1
                };
                !sensors.contains(other)
                    && pair.manifolds.iter().any(|manifold| {
                        manifold
                            .points
                            .iter()
                            .any(|point| point.penetration > -cfg::CAR_BODY_TOUCH_M)
                    })
            })
    });
    near_ground |= body_touching;
    let lying = body_touching || corner_down;

    // Every write below is guarded: a `&mut` taken through a query stamps a
    // change tick even when the value is the same.
    let now = CarContact {
        wheels,
        near_ground,
        lying,
    };
    contact.set_if_neq(now);

    // The dampings follow the wheels, from here rather than the drive so the
    // swap still happens while the player types in chat.
    let t = now.traction();
    let linear = blend(p.air_linear_damping.0, p.linear_damping.0, t);
    let angular = blend(p.air_angular_damping.0, p.angular_damping.0, t);
    if linear_damping.0 != linear {
        linear_damping.0 = linear;
    }
    if angular_damping.0 != angular {
        angular_damping.0 = angular;
    }

    // The self-levelling, here for the same reason (#821): it is passive, and
    // a car in flight must still come in level while the player types. It
    // leaves alone the axes the drive's keys held last step, and a car LYING
    // on the ground - its box on something, or tipped past its wheels with a
    // corner within a spring's length of the ground: that is the uprighting
    // assist's to right, which the record controls.
    let held = air_keys.0;
    air_keys.set_if_neq(CarAirKeys::default());
    let air = 1.0 - t;
    if air > 0.0 && !lying {
        let level = air_level_torque(
            global_tf.right().as_vec3(),
            global_tf.up().as_vec3(),
            global_tf.forward().as_vec3(),
            ang_vel,
            held,
            p,
        );
        forces.apply_torque(level * air);
    }
}

/// The six air-control keys (#1524). `pub(super)` only because the drive
/// system remembers a set of them in a `Local` and [`CarAirKeys`] carries a
/// set on the chassis, and both are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct AirKeys {
    nose_down: bool,
    nose_up: bool,
    yaw_left: bool,
    yaw_right: bool,
    roll_left: bool,
    roll_right: bool,
}

impl AirKeys {
    /// The keys as they stand this step. The arrows double W/S/A/D, as they
    /// do on the ground.
    fn read(keyboard: &ButtonInput<KeyCode>) -> Self {
        let any = |keys: &[KeyCode]| keys.iter().any(|k| keyboard.pressed(*k));
        Self {
            nose_down: any(&[KeyCode::KeyW, KeyCode::ArrowUp]),
            nose_up: any(&[KeyCode::KeyS, KeyCode::ArrowDown]),
            yaw_left: any(&[KeyCode::KeyA, KeyCode::ArrowLeft]),
            yaw_right: any(&[KeyCode::KeyD, KeyCode::ArrowRight]),
            roll_left: any(&[KeyCode::KeyQ]),
            roll_right: any(&[KeyCode::KeyE]),
        }
    }

    /// The keys held here but not in `latched`.
    fn except(self, latched: Self) -> Self {
        Self {
            nose_down: self.nose_down && !latched.nose_down,
            nose_up: self.nose_up && !latched.nose_up,
            yaw_left: self.yaw_left && !latched.yaw_left,
            yaw_right: self.yaw_right && !latched.yaw_right,
            roll_left: self.roll_left && !latched.roll_left,
            roll_right: self.roll_right && !latched.roll_right,
        }
    }

    /// The keys held in both.
    fn and(self, other: Self) -> Self {
        Self {
            nose_down: self.nose_down && other.nose_down,
            nose_up: self.nose_up && other.nose_up,
            yaw_left: self.yaw_left && other.yaw_left,
            yaw_right: self.yaw_right && other.yaw_right,
            roll_left: self.roll_left && other.roll_left,
            roll_right: self.roll_right && other.roll_right,
        }
    }

    /// The keys that also act on the ground - W/S/A/D, throttle and steer -
    /// which the latch holds back from the air. Q/E are not among them.
    fn on_the_ground(self) -> Self {
        Self {
            roll_left: false,
            roll_right: false,
            ..self
        }
    }

    fn holds_pitch(self) -> bool {
        self.nose_down || self.nose_up
    }

    fn holds_yaw(self) -> bool {
        self.yaw_left || self.yaw_right
    }

    fn holds_roll(self) -> bool {
        self.roll_left || self.roll_right
    }

    /// `torque` with nothing left about an axis these keys hold: pitch
    /// (`right`), yaw (`up`) or roll (`forward`), the chassis' own axes.
    fn leave_held_axes(self, torque: Vec3, right: Vec3, up: Vec3, forward: Vec3) -> Vec3 {
        let mut free = torque;
        for (held, about) in [
            (self.holds_pitch(), right),
            (self.holds_yaw(), up),
            (self.holds_roll(), forward),
        ] {
            if held {
                free -= about * free.dot(about);
            }
        }
        free
    }
}

/// The air keys the player's hands hold on the car (#1524): what
/// [`apply_car_drive`] counted this step, after the latch. The passive halves
/// of the air model leave those axes to the player - the levelling in
/// [`apply_car_suspension`] and the uprighting assist in
/// [`apply_car_uprighting`].
///
/// The suspension, first in the chain, reads it and CLEARS it; the drive
/// writes it again when it runs, and the uprighting reads it after the drive.
/// So a drive that stands down - the player typing, a modal up, a visuals row
/// selected - leaves no key held, and the passive halves act on every axis,
/// as passive stabilisation must (#821). The levelling reads the drive's
/// keys of the step before: a step's lag, 1/64 s.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CarAirKeys(AirKeys);

/// +1, -1 or 0 for a pair of opposed keys.
fn axis(positive: bool, negative: bool) -> f32 {
    f32::from(u8::from(positive)) - f32::from(u8::from(negative))
}

/// The air control's torque for one step (#1524), before it is scaled by
/// the share of wheels off the ground: mass x `air_control_accel` about the
/// chassis' own axes - W (`nose_down`) about -right, S about +right, A about
/// +up, D about -up, Q about -forward (left side down), E about +forward
/// (right side down). `keys` are the ones that count this step (after the
/// latch). Pure, so the signs can be pinned without a physics world.
fn air_control_torque(right: Vec3, up: Vec3, forward: Vec3, keys: AirKeys, p: &CarParams) -> Vec3 {
    let control = p.mass.0 * p.air_control_accel.0;
    right * (control * axis(keys.nose_up, keys.nose_down))
        + up * (control * axis(keys.yaw_left, keys.yaw_right))
        + forward * (control * axis(keys.roll_right, keys.roll_left))
}

/// The air self-levelling's torque for one step (#1524), before it is scaled
/// by the share of wheels off the ground: on each of pitch and roll that no
/// counted air key holds, mass x `air_level_accel` x the sine of the tilt,
/// turning the chassis' up toward world up, less a damping of that axis'
/// rate. Never yaw: the levelling axis `up x Y` is perpendicular to `up`. The
/// damping is critical for the chassis' own box about the axis, so the car
/// comes back to level as fast as it can without swinging past it.
///
/// `right` / `up` / `forward` are the chassis' own world-space axes and
/// `ang_vel` its angular velocity. Pure, so the levelling can be pinned
/// without a physics world.
fn air_level_torque(
    right: Vec3,
    up: Vec3,
    forward: Vec3,
    ang_vel: Vec3,
    keys: AirKeys,
    p: &CarParams,
) -> Vec3 {
    let stiffness = p.air_level_accel.0;
    if stiffness <= 0.0 {
        return Vec3::ZERO;
    }
    let mass = p.mass.0;
    let [hx, hy, hz] = p.chassis_half_extents.0;
    let tilt = up.cross(Vec3::Y);
    // `inertia` is the box's own about the axis, per kilogram: avian takes a
    // cuboid's as mass x (the other two half-extents squared) / 3 (measured
    // on the drive bench, the lowered centre of mass leaves it be). The
    // damping makes the swing back critical for it, net of the air damping
    // avian already applies.
    let level = |about: Vec3, inertia: f32| {
        let damping =
            (2.0 * (stiffness * inertia).sqrt() - inertia * p.air_angular_damping.0).max(0.0);
        about * (mass * (stiffness * tilt.dot(about) - damping * ang_vel.dot(about)))
    };
    let mut torque = Vec3::ZERO;
    if !keys.holds_pitch() {
        torque += level(right, (hy * hy + hz * hz) / 3.0);
    }
    if !keys.holds_roll() {
        torque += level(forward, (hx * hx + hy * hy) / 3.0);
    }
    torque
}

/// The player's hands on the car: throttle, steering and grip through the
/// wheels, scaled by the traction [`CarContact`] reports, and the air control
/// ([`air_control_torque`]) scaled by the share of wheels off the ground. It
/// also says which air keys count ([`CarAirKeys`]) for the passive halves to
/// leave alone. Stands down while the player types, like every drive system.
#[allow(clippy::type_complexity)]
pub(super) fn apply_car_drive(
    live: Res<LiveAvatarRecord>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<
        (Forces, &GlobalTransform, &CarContact, &mut CarAirKeys),
        (With<LocalPlayer>, With<CarPreset>),
    >,
    traveling: Option<Res<TravelingTo>>,
    // The drive and steer keys that were already down while a wheel was on
    // the ground: they drive and steer, and must not also tip the car the
    // moment it leaves it. Cleared key by key as each is let go.
    mut latched: Local<AirKeys>,
) {
    if traveling.is_some() {
        return;
    }
    let LocomotionConfig::Car(p) = &live.0.locomotion else {
        return;
    };
    let Ok((mut forces, global_tf, contact, mut air_keys)) = query.single_mut() else {
        return;
    };

    let t = contact.traction();
    let held = AirKeys::read(&keyboard);
    // Refreshed while ANY wheel is down, not only all four: a W or an A
    // pressed with three wheels on uneven ground is throttle and steer, and
    // counted as an air key it would pitch the car at a quarter share, and
    // yaw it without the reverse-steer rule. Q/E are never latched.
    *latched = if t > 0.0 {
        held.on_the_ground()
    } else {
        latched.and(held)
    };
    // A car lying on the ground is not in the air: no air key counts, so the
    // assist has every axis. Counted there, a key held the car down - the
    // assist left its axis out, and the key's torque is less than gravity's
    // to tip the box - on its roof with E or Q, on its nose with W.
    let keys = if contact.lying {
        AirKeys::default()
    } else {
        held.except(*latched)
    };
    air_keys.set_if_neq(CarAirKeys(keys));

    let lin_vel = forces.linear_velocity();
    let forward = global_tf.forward().as_vec3();
    let flat_forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let local_up = global_tf.up().as_vec3();
    let right = global_tf.right().as_vec3();

    // ---- On the ground: through the wheels, so scaled by traction. ----
    if t > 0.0 {
        let handbrake = keyboard.pressed(KeyCode::Space);
        let drive = p.drive_force.0 * t;
        if !handbrake {
            if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
                forces.apply_force(flat_forward * drive);
            }
            if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
                forces.apply_force(-flat_forward * drive);
            }
        }
        // Invert the steer response when actually reversing (#723): with the
        // wheels held one way, a real car's heading rotates the opposite way
        // in reverse vs. forward, so a raw key→yaw mapping steers backwards
        // while backing up. Keyed on longitudinal (not raw) velocity so a
        // sideways slide doesn't flip it. Ground steering only: in the air
        // A always yaws left.
        let forward_speed = flat_forward.dot(lin_vel);
        let steer = local_up * (p.turn_torque.0 * t) * super::reverse_steer_sign(forward_speed);
        if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
            forces.apply_torque(steer);
        }
        if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
            forces.apply_torque(-steer);
        }

        // Lateral grip - strong by default to keep the car planted; scaled
        // by `handbrake_grip_factor` when Space is held (below 1 by default,
        // so the rear breaks loose for arcade-style drifts).
        let grip = if handbrake {
            p.lateral_grip.0 * p.handbrake_grip_factor.0
        } else {
            p.lateral_grip.0
        };
        let lateral_vel = right.dot(lin_vel);
        // In HEAD's order, so four wheels down (t = 1) is the old force to
        // the bit.
        forces.apply_force(-right * lateral_vel * grip * t);
    }

    // ---- Off the ground: the air control, scaled by the share in the air.
    // The levelling is the suspension's, ungated. ----
    let air = 1.0 - t;
    if air > 0.0 {
        forces.apply_torque(air_control_torque(right, local_up, forward, keys, p) * air);
    }
}

/// World-up restoring torque for the car's uprighting assist, or `None` when
/// the chassis is upright enough to leave alone. Pure so the engage threshold,
/// the mass-scaling, and the roof roll can be unit-tested without a physics
/// world.
///
/// `up` / `forward` are the chassis's world-space up and forward axes,
/// `ang_vel` its angular velocity, `gravity` the strength of the world's
/// gravity (m/s^2) and `p` the record's car tuning (#876 promoted the engage
/// tilt / accel / damping from constants). Returns `None` while the tilt is
/// inside `upright_engage_tilt_degrees` - i.e. within normal cornering-lean /
/// slope-driving range - so the assist never fights ordinary driving. Past
/// that tilt it returns a torque that turns `up` toward world-up, minus a
/// spin-damping term so it settles level rather than oscillating:
///
/// * **Up to 90 degrees** (on a side or the nose): about `up x Y`.
/// * **Past 90 degrees** (on its back, #1524): ROLLED about the chassis' own
///   forward axis, toward the nearer side - the narrow way over. The
///   dead-inverted fallback used to turn about the RIGHT axis, which pitches
///   the car end over end - the long way. A car past 90 degrees because it
///   stands steeply on its nose or tail is pitched back as below 90 instead:
///   its forward points at the ground, and a roll about it would only spin
///   it.
///
/// Either way of mass x `upright_assist_accel`, or of
/// [`cfg::CAR_UPRIGHT_TIP_MARGIN`] times the torque gravity needs to tip the
/// lying box back over the edge it pivots on, whichever is more (#1524). On
/// its back that edge is a roof edge, half-width from the centre of mass; on
/// a side or its nose it is an edge of the underside, which the lowered
/// centre of mass sits `(1 - center_of_mass_drop)` x half-height above. Each
/// lever is scaled by how far into its pose the car lies, so the two meet at
/// 90 degrees without a step. The assist alone used to be about a third of
/// the roof's: measured on the drive bench at #1524, no car of the seeded
/// fleet came off its roof, and a record's tall box stayed on its side.
fn upright_assist_torque(
    up: Vec3,
    forward: Vec3,
    ang_vel: Vec3,
    gravity: f32,
    p: &CarParams,
) -> Option<Vec3> {
    if up.dot(Vec3::Y) >= p.upright_engage_tilt_degrees.0.to_radians().cos() {
        return None;
    }
    let mass = p.mass.0;
    let damping = -ang_vel * (mass * p.upright_assist_damping.0);
    let tilt_axis = up.cross(Vec3::Y);
    // A strength of 0 is an owner turning the assist off, and the tipping
    // floor goes with it.
    let accel = if p.upright_assist_accel.0 > 0.0 {
        let [hx, hy, _] = p.chassis_half_extents.0;
        let on_its_back = hx * (-up.y).max(0.0);
        let on_a_side = (1.0 - p.center_of_mass_drop.0) * hy * (1.0 - up.y * up.y).max(0.0).sqrt();
        let tip = cfg::CAR_UPRIGHT_TIP_MARGIN * gravity * on_its_back.max(on_a_side);
        p.upright_assist_accel.0.max(tip)
    } else {
        0.0
    };
    // Steep: within 45 degrees of pointing straight up or down.
    let on_end = forward.y.abs() > std::f32::consts::FRAC_1_SQRT_2;
    if up.y >= 0.0 || on_end {
        let restoring = tilt_axis.normalize_or_zero() * (mass * accel);
        return Some(restoring + damping);
    }
    // On its back. `tilt_axis` along `forward` says which way over is back
    // toward upright; dead inverted it says nothing, and the deadband keeps a
    // flat roof from choosing a side afresh every step.
    let side = tilt_axis.dot(forward);
    let over = if side * side < cfg::CAR_UPRIGHT_DEGENERATE_SQ {
        1.0
    } else {
        side.signum()
    };
    Some(forward * (over * mass * accel) + damping)
}

/// Right a car that has tipped onto its side or roof. Runs every fixed step
/// (like the hover-boat's uprighting) but stays dormant until the chassis is
/// tilted past the record's engage tilt, so it leaves normal driving -
/// cornering lean, driving across slopes - untouched and only rescues a
/// genuine flip. Not input-gated: a flipped car keeps righting even while the
/// owner types in a chat field.
///
/// Only NEAR THE GROUND (#1524), and never about an axis a counted air key
/// holds ([`CarAirKeys`], this step's): in the air a flip is the player's to
/// make and finish with the air controls, and within the rays' reach of the
/// ground - most of a kicker's flight - the assist would otherwise fight a
/// held roll past its engage tilt and leave the car hanging on its side. A
/// car on its roof or side is near the ground by
/// [`CarContact::near_ground`]'s own definition, and lying there
/// ([`CarContact::lying`]) it counts no air key, so the assist has every
/// axis: that is what keeps #804.
#[allow(clippy::type_complexity)]
pub(super) fn apply_car_uprighting(
    live: Res<LiveAvatarRecord>,
    gravity: Option<Res<Gravity>>,
    mut query: Query<
        (Forces, &GlobalTransform, &CarContact, &CarAirKeys),
        (With<LocalPlayer>, With<CarPreset>),
    >,
) {
    let LocomotionConfig::Car(p) = &live.0.locomotion else {
        return;
    };
    let Ok((mut forces, global_tf, contact, air_keys)) = query.single_mut() else {
        return;
    };
    if !contact.near_ground {
        return;
    }
    let g = gravity.map_or(Gravity::default().0, |g| g.0).length();
    let up = global_tf.up().as_vec3();
    let forward = global_tf.forward().as_vec3();
    let ang_vel = forces.angular_velocity();
    if let Some(torque) = upright_assist_torque(up, forward, ang_vel, g, p) {
        let right = global_tf.right().as_vec3();
        forces.apply_torque(air_keys.0.leave_held_axes(torque, right, up, forward));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The world's gravity, as avian's default has it and the game keeps it.
    const G: f32 = 9.81;

    fn params() -> CarParams {
        CarParams::default()
    }

    #[test]
    fn upright_car_gets_no_assist() {
        // Dead level, and a modest cornering lean (30°) - both within the
        // assist's silent band, so it must return None (never fight driving).
        let p = params();
        assert!(upright_assist_torque(Vec3::Y, Vec3::NEG_Z, Vec3::ZERO, G, &p).is_none());
        let leaned = Quat::from_rotation_z(30f32.to_radians()) * Vec3::Y;
        assert!(upright_assist_torque(leaned, Vec3::NEG_Z, Vec3::ZERO, G, &p).is_none());
    }

    #[test]
    fn tipped_car_is_pushed_back_toward_upright() {
        // Rolled 80° and 100° about +Z (past the default 60° engage tilt,
        // either side of lying on a side). With no spin the torque must point
        // along `up × Y` - the axis whose rotation lifts the up vector back
        // toward world-up - and be mass-scaled.
        let p = params();
        for degrees in [80f32, 100.0] {
            let up = Quat::from_rotation_z(degrees.to_radians()) * Vec3::Y;
            let torque = upright_assist_torque(up, Vec3::NEG_Z, Vec3::ZERO, G, &p)
                .expect("a car tipped past 60° must be assisted");
            let righting_axis = up.cross(Vec3::Y).normalize();
            assert!(
                torque.normalize().dot(righting_axis) > 0.99,
                "at {degrees} deg the torque should roll the chassis back toward upright, \
                 got {torque:?}"
            );
            assert!(
                torque.length() >= p.mass.0 * p.upright_assist_accel.0 * 0.5,
                "righting torque should be mass-scaled and substantial"
            );
        }
    }

    #[test]
    fn dead_inverted_car_falls_back_to_the_roll_axis() {
        // Exactly upside down: `up × Y` degenerates to ~zero, so without the
        // fallback the car would perch on its roof. The assist must instead
        // torque about the ROLL axis - the chassis' forward - to tip it off
        // over a side, the narrow way. Until #1524 it turned about the right
        // axis, which pitches the car end over end.
        let p = params();
        let forward = Vec3::NEG_Z;
        let torque = upright_assist_torque(Vec3::NEG_Y, forward, Vec3::ZERO, G, &p)
            .expect("an inverted car must be assisted");
        assert!(
            torque.normalize().dot(forward).abs() > 0.999,
            "inverted assist should act about the roll axis, got {torque:?}"
        );
    }

    /// #1524: on its back the car must be rolled harder than gravity holds
    /// it there. A box resting on its flat roof only tips over an edge when
    /// the torque beats mass x g x half-width; the #876 assist of mass x 2.5
    /// was about a third of that for the default car, and measured on the
    /// drive bench no car of the seeded fleet came off its roof.
    #[test]
    fn a_car_on_its_back_is_rolled_harder_than_its_roof_holds_it() {
        let p = params();
        let tip = p.mass.0 * G * p.chassis_half_extents.0[0];
        for degrees in [180f32, 170.0, 150.0, 120.0] {
            let tilt = Quat::from_rotation_z(degrees.to_radians());
            let up = tilt * Vec3::Y;
            let forward = tilt * Vec3::NEG_Z;
            let torque = upright_assist_torque(up, forward, Vec3::ZERO, G, &p)
                .expect("a car on its back must be assisted");
            let needed = tip * -up.y;
            assert!(
                torque.dot(forward).abs() > needed,
                "at {degrees} deg the roll {:.0} N m must beat gravity's {needed:.0} N m",
                torque.dot(forward).abs()
            );
            if degrees < 180.0 {
                assert!(
                    torque.dot(up.cross(Vec3::Y)) > 0.0,
                    "at {degrees} deg the roll must go the way back to upright"
                );
            }
        }
    }

    /// #1524: on a side a car pivots back onto its wheels over an edge of its
    /// underside, and gravity holds it down with mass x g x the height of
    /// its centre of mass over that face. The record's 2.5 x mass is more
    /// than that for every seeded skiff, but a record may author a tall box:
    /// at a half-height of 1.2 m it needs 4.7 x mass, and the assist gives it
    /// the margin over that. The default car keeps its 2.5 to the bit.
    #[test]
    fn a_tall_car_on_its_side_is_rolled_harder_than_gravity_holds_it() {
        let side = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let (up, forward) = (side * Vec3::Y, side * Vec3::NEG_Z);
        let roll = |p: &CarParams| {
            upright_assist_torque(up, forward, Vec3::ZERO, G, p)
                .expect("a car on its side must be assisted")
                .dot(up.cross(Vec3::Y).normalize())
        };
        let mut tall = params();
        tall.chassis_half_extents = crate::pds::Fp3([0.8, 1.2, 1.6]);
        let held = tall.mass.0 * G * (1.0 - tall.center_of_mass_drop.0) * 1.2;
        assert!(
            roll(&tall) > held,
            "the roll {:.0} N m must beat gravity's {held:.0} N m",
            roll(&tall)
        );
        let p = params();
        assert_eq!(roll(&p), p.mass.0 * p.upright_assist_accel.0);
    }

    /// #1524: a counted air key holds its axis against the assist, as it does
    /// against the levelling - the roll it leaves out, and nothing else.
    #[test]
    fn a_held_air_key_leaves_its_axis_out_of_the_assist() {
        let p = params();
        let tilt = Quat::from_rotation_z(100f32.to_radians());
        let (right, up, forward) = (tilt * Vec3::X, tilt * Vec3::Y, tilt * Vec3::NEG_Z);
        let spin = Vec3::new(0.5, -0.25, 2.0);
        let torque = upright_assist_torque(up, forward, spin, G, &p).expect("assisted");
        let rolling = AirKeys {
            roll_right: true,
            ..Default::default()
        };
        let left = rolling.leave_held_axes(torque, right, up, forward);
        assert!(left.dot(forward).abs() < 1e-3, "no roll left, got {left:?}");
        for about in [right, up] {
            assert!((left.dot(about) - torque.dot(about)).abs() < 1e-3);
        }
        let none = AirKeys::default().leave_held_axes(torque, right, up, forward);
        assert_eq!(none, torque, "no key held, nothing is left out");
    }

    /// Past 90 degrees because it stands on its nose, not because it lies on
    /// its back: the roll axis points at the ground, so the assist pitches
    /// the car back instead of spinning it where it stands.
    #[test]
    fn a_car_on_its_nose_past_vertical_is_pitched_back_not_spun() {
        let p = params();
        let tilt = Quat::from_rotation_x(-110f32.to_radians());
        let (up, forward) = (tilt * Vec3::Y, tilt * Vec3::NEG_Z);
        assert!(up.y < 0.0, "the premise: past 90 degrees");
        let torque = upright_assist_torque(up, forward, Vec3::ZERO, G, &p)
            .expect("a car on its nose must be assisted");
        let righting_axis = up.cross(Vec3::Y).normalize();
        assert!(
            torque.normalize().dot(righting_axis) > 0.99,
            "the car must be pitched back toward upright, got {torque:?}"
        );
    }

    #[test]
    fn spin_is_damped_while_righting() {
        // A chassis tipped 100° about +Z rights by rotating in -Z. Give it a
        // spin already in that righting sense: the damping term must shrink the
        // net righting torque (magnitude along -Z) versus the static case, so
        // the car settles upright instead of overshooting past level.
        let p = params();
        let up = Quat::from_rotation_z(100f32.to_radians()) * Vec3::Y;
        let righting_axis = up.cross(Vec3::Y).normalize(); // ≈ -Z
        let still = upright_assist_torque(up, Vec3::NEG_Z, Vec3::ZERO, G, &p).unwrap();
        let spinning = upright_assist_torque(up, Vec3::NEG_Z, righting_axis * 5.0, G, &p).unwrap();
        assert!(
            spinning.dot(righting_axis) < still.dot(righting_axis),
            "a chassis already rotating upright should get less righting torque"
        );
    }

    #[test]
    fn widened_engage_tilt_keeps_the_assist_silent_longer() {
        // The promoted engage-tilt knob (#876): at 80° the default assist
        // engages but a record tuned to 85° stays silent.
        let tilted = Quat::from_rotation_z(80f32.to_radians()) * Vec3::Y;
        let p = params();
        assert!(upright_assist_torque(tilted, Vec3::NEG_Z, Vec3::ZERO, G, &p).is_some());
        let mut wide = params();
        wide.upright_engage_tilt_degrees = crate::pds::Fp(85.0);
        assert!(upright_assist_torque(tilted, Vec3::NEG_Z, Vec3::ZERO, G, &wide).is_none());
    }
}

/// The air model on the drive bench (#1524): each test runs the game's own
/// three car systems over avian with gravity on, and each pins one rule -
/// undo the rule and the test fails.
#[cfg(test)]
mod air_model {
    use super::*;
    use crate::pds::Fp;
    use crate::pds::avatar::AvatarRecord;
    use crate::player::sim::DriveBench;

    /// Metres per second squared, as avian's default gravity has it.
    const G: f32 = 9.81;

    /// A record wearing the default car.
    fn default_car() -> AvatarRecord {
        let mut record = AvatarRecord::default_for_did("did:plc:air-model-bench");
        record.locomotion = LocomotionConfig::Car(Box::default());
        record
    }

    /// The seeded Cyclecar of did:plc:lr2ocunor73lfqpwtgvt274o, through the
    /// game's own seeding - the skiff the brief of #1524 was written against.
    fn seeded_cyclecar() -> AvatarRecord {
        AvatarRecord::default_for_did("did:plc:lr2ocunor73lfqpwtgvt274o")
    }

    fn car_params(record: &AvatarRecord) -> &CarParams {
        let LocomotionConfig::Car(p) = &record.locomotion else {
            panic!("the drive bench is for the car preset");
        };
        p
    }

    /// `record` with its car tuning changed by `change`.
    fn tuned(record: &AvatarRecord, change: impl FnOnce(&mut CarParams)) -> AvatarRecord {
        let mut record = record.clone();
        let LocomotionConfig::Car(p) = &mut record.locomotion else {
            panic!("the drive bench is for the car preset");
        };
        change(p);
        record
    }

    /// Nose above the horizon, degrees.
    fn pitch(q: Quat) -> f32 {
        (q * Vec3::NEG_Z).y.clamp(-1.0, 1.0).asin().to_degrees()
    }

    /// Right side above the horizon, degrees: positive is rolled LEFT.
    fn roll(q: Quat) -> f32 {
        (q * Vec3::X).y.clamp(-1.0, 1.0).asin().to_degrees()
    }

    /// Tilt of the chassis' up off world up, degrees.
    fn tilt(q: Quat) -> f32 {
        (q * Vec3::Y).y.clamp(-1.0, 1.0).acos().to_degrees()
    }

    /// Compass heading of a flat direction, degrees, positive to the LEFT
    /// of -Z.
    fn heading(dir: Vec3) -> f32 {
        (-dir.x).atan2(-dir.z).to_degrees()
    }

    /// A car in the open air, 40 m up - well past its rays' reach - level,
    /// facing -Z, and moving at `velocity`.
    fn aloft(record: &AvatarRecord, velocity: Vec3, turned: Quat) -> DriveBench {
        let mut bench = DriveBench::new(
            record,
            Transform::from_xyz(0.0, 40.0, 0.0).with_rotation(turned),
        );
        bench.set_velocity(velocity);
        bench
    }

    /// A ramp rising toward -Z at `degrees`, its lip `height` m up at
    /// `lip_z`, as a tilted block. Returns the z where it meets the floor.
    fn ramp(bench: &mut DriveBench, lip_z: f32, height: f32, degrees: f32) -> f32 {
        let theta = degrees.to_radians();
        let length = height / theta.sin();
        let thick = 1.0;
        let turned = Quat::from_rotation_x(theta);
        let middle = Vec3::new(0.0, height * 0.5, lip_z + length * 0.5 * theta.cos());
        bench.block(
            Transform::from_translation(middle - turned * Vec3::Y * thick).with_rotation(turned),
            Vec3::new(4.0, thick, length * 0.5),
        );
        lip_z + length * theta.cos()
    }

    /// One step of a ramp run, as the test reads it.
    struct Sample {
        t: f64,
        velocity: Vec3,
        rotation: Quat,
        spin: Vec3,
        position: Vec3,
        underside: f32,
    }

    impl Sample {
        fn of(bench: &DriveBench) -> Self {
            Self {
                t: bench.elapsed(),
                velocity: bench.velocity(),
                rotation: bench.rotation(),
                spin: bench.angular_velocity(),
                position: bench.position(),
                underside: bench.underside(),
            }
        }

        fn flat_speed(&self) -> f32 {
            Vec2::new(self.velocity.x, self.velocity.z).length()
        }
    }

    /// A run at a ramp and the flight off its lip.
    struct Flight {
        /// Every step from the last one on the ramp to the first one back
        /// on the ground, both included.
        airborne: Vec<Sample>,
        /// The second after that first step back on the ground.
        landing: Vec<Sample>,
    }

    impl Flight {
        fn takeoff(&self) -> &Sample {
            self.airborne.first().expect("a flight has a takeoff")
        }
        fn touchdown(&self) -> &Sample {
            self.airborne.last().expect("a flight has a touchdown")
        }
        /// The steps with no wheel and no body on anything.
        fn aloft(&self) -> &[Sample] {
            &self.airborne[1..self.airborne.len() - 1]
        }

        /// Degrees the car rolled about its own forward axis from the lip to
        /// the ground, whichever way.
        fn rolled(&self) -> f32 {
            let hz = crate::player::sim::BENCH_HZ as f32;
            self.airborne
                .iter()
                .map(|s| s.spin.dot(s.rotation * Vec3::NEG_Z) / hz)
                .sum::<f32>()
                .abs()
                .to_degrees()
        }
    }

    /// What a ramp run does the moment the car is off the lip.
    type AtTakeoff = fn(&mut DriveBench);

    /// Nothing: the keys held from the run-up stay held.
    fn keep_going(_: &mut DriveBench) {}

    /// Let go of W for a step and press it again, so it is a press in the
    /// air.
    fn press_w_again(bench: &mut DriveBench) {
        bench.hold(&[]);
        bench.step();
        bench.hold(&[KeyCode::KeyW]);
    }

    /// Put the player's focus in a text field: the drive stands down.
    fn start_typing(bench: &mut DriveBench) {
        bench.typing(true);
    }

    /// Press E, keeping W: a roll pressed in the air.
    fn press_e(bench: &mut DriveBench) {
        bench.hold(&[KeyCode::KeyW, KeyCode::KeyE]);
    }

    /// The lip of every ramp run, and its height.
    const LIP_Z: f32 = -30.0;
    const LIP_M: f32 = 1.5;

    /// Drive `record` at a `degrees` ramp with `keys` held from the run-up
    /// (W at least), riding on `stop`: pinned to `approach` m/s until its
    /// nose meets the ramp, then on its own up the ramp, off the lip, back to
    /// the ground and a second on. `at_takeoff` runs once, the moment the car
    /// is off the lip.
    ///
    /// Down is a wheel down or the box on the ground - measured, not read
    /// off avian's contacts, which count a body moving at 15 m/s as touching
    /// anything within the 23 cm it could cover in a step.
    fn ramp_run(
        record: &AvatarRecord,
        approach: f32,
        degrees: f32,
        keys: &[KeyCode],
        at_takeoff: AtTakeoff,
        stop: CarBumpStop,
    ) -> Flight {
        let half_length = car_params(record).chassis_half_extents.0[2];
        let base = LIP_Z + LIP_M / degrees.to_radians().tan();
        let mut bench = DriveBench::parked(record, 0.0, base + 10.0 + half_length);
        bench.bump_stop(stop);
        ramp(&mut bench, LIP_Z, LIP_M, degrees);
        bench.hold(keys);
        let mut last_on_ramp = Sample::of(&bench);
        let mut airborne: Vec<Sample> = Vec::new();
        let mut landing: Vec<Sample> = Vec::new();
        for _ in 0..(8.0 * crate::player::sim::BENCH_HZ) as usize {
            if bench.position().z - half_length > base {
                let v = bench.velocity();
                bench.set_velocity(Vec3::new(v.x, v.y, -approach));
            }
            bench.step();
            if !landing.is_empty() {
                landing.push(Sample::of(&bench));
                if bench.elapsed() - landing[0].t >= 1.0 {
                    return Flight { airborne, landing };
                }
                continue;
            }
            let down = bench.contact().wheels > 0 || bench.underside() < 0.005;
            if airborne.is_empty() {
                // Off the lip: the whole car past it, no wheel down.
                if down || bench.position().z + half_length > LIP_Z {
                    last_on_ramp = Sample::of(&bench);
                    continue;
                }
                airborne.push(std::mem::replace(&mut last_on_ramp, Sample::of(&bench)));
                at_takeoff(&mut bench);
            }
            airborne.push(Sample::of(&bench));
            if down {
                landing.push(Sample::of(&bench));
            }
        }
        panic!("the car never came back down");
    }

    // -----------------------------------------------------------------------
    // Traction: throttle, steering and grip only through the wheels
    // -----------------------------------------------------------------------

    /// Off a ramp at speed with W held: nothing pushes the car along once it
    /// is off the ground, and it falls at g. Before #1524 the throttle drove
    /// it on through the air at `drive_force / mass` (8.9 m/s^2 on the
    /// default car) and the ground drag held its fall to about g / 0.8.
    #[test]
    fn a_car_off_a_ramp_flies_ballistically() {
        let flight = ramp_run(
            &default_car(),
            14.0,
            18.0,
            &[KeyCode::KeyW],
            keep_going,
            CarBumpStop::default(),
        );
        let aloft = flight.aloft();
        assert!(
            flight.touchdown().t - flight.takeoff().t > 0.8,
            "the ramp must throw the car into a real flight, got {:.2} s",
            flight.touchdown().t - flight.takeoff().t
        );
        let flat = |s: &Sample| Vec2::new(s.velocity.x, s.velocity.z).length();
        let launched = flat(&aloft[0]);
        for s in aloft {
            assert!(
                flat(s) <= launched + 1e-3,
                "W held in the air sped the car up: {launched:.3} -> {:.3} m/s at {:.2} s",
                flat(s),
                s.t
            );
        }
        // The fall: from the top of the flight to the last step aloft, where
        // the drag of the old ground damping was largest.
        let apex = aloft
            .iter()
            .position(|s| s.velocity.y <= 0.0)
            .expect("a flight has a top");
        let (top, end) = (&aloft[apex], aloft.last().expect("a flight"));
        let fall = (end.velocity.y - top.velocity.y) / (end.t - top.t) as f32;
        assert!(
            (fall + G).abs() < 0.03 * G,
            "the fall must be ballistic: {fall:.2} m/s^2 against -{G}"
        );
    }

    /// With A held in the air the car turns, but its flight does not: no grip
    /// swings the velocity round to the nose. Before #1524 the lateral grip
    /// did exactly that, and a jump could be steered like an aircraft.
    #[test]
    fn a_turn_in_the_air_turns_the_car_not_its_flight() {
        let mut bench = aloft(&default_car(), Vec3::new(0.0, 0.0, -15.0), Quat::IDENTITY);
        bench.hold(&[KeyCode::KeyA]);
        bench.run(1.0);
        let turned = heading(bench.rotation() * Vec3::NEG_Z);
        let flying = heading(bench.velocity() * Vec3::new(1.0, 0.0, 1.0));
        assert!(
            turned > 20.0,
            "A must still yaw the car left in the air, got {turned:.1} deg"
        );
        assert!(
            flying.abs() < 2.0,
            "the flight must keep its line with no air grip, turned {flying:.2} deg"
        );
    }

    // -----------------------------------------------------------------------
    // The air controls and the self-levelling
    // -----------------------------------------------------------------------

    /// Each air key turns the car the way the controls say, about its own
    /// axes: W the nose down, S up, A left, D right, Q the left side down, E
    /// the right side down.
    #[test]
    fn the_air_keys_pitch_yaw_and_roll_the_stated_ways() {
        let record = default_car();
        let turn = |key: KeyCode| {
            let mut bench = aloft(&record, Vec3::ZERO, Quat::IDENTITY);
            bench.hold(&[key]);
            bench.run(0.4);
            bench.rotation()
        };
        let q = turn(KeyCode::KeyW);
        assert!(
            pitch(q) < -5.0,
            "W must pitch the nose down, got {:.1}",
            pitch(q)
        );
        let q = turn(KeyCode::KeyS);
        assert!(
            pitch(q) > 5.0,
            "S must pitch the nose up, got {:.1}",
            pitch(q)
        );
        let q = turn(KeyCode::KeyA);
        assert!(heading(q * Vec3::NEG_Z) > 5.0, "A must yaw left");
        let q = turn(KeyCode::KeyD);
        assert!(heading(q * Vec3::NEG_Z) < -5.0, "D must yaw right");
        let q = turn(KeyCode::KeyQ);
        assert!(
            roll(q) > 5.0,
            "Q must roll left (left side down), got {:.1}",
            roll(q)
        );
        let q = turn(KeyCode::KeyE);
        assert!(
            roll(q) < -5.0,
            "E must roll right (right side down), got {:.1}",
            roll(q)
        );
    }

    /// A car in the air with its nose 25 degrees up, or rolled 25 degrees,
    /// and no key held, comes back to level - and a held key on the axis
    /// overrides it: S held carries the nose over the top, where a levelling
    /// that fought the key would hold it near 42 degrees
    /// (`sin = air_control_accel / air_level_accel`).
    #[test]
    fn an_airborne_car_levels_itself_unless_a_key_holds_the_axis() {
        let record = default_car();
        for (name, turned, angle) in [
            (
                "pitched",
                Quat::from_rotation_x(25f32.to_radians()),
                pitch as fn(Quat) -> f32,
            ),
            ("rolled", Quat::from_rotation_z(25f32.to_radians()), roll),
        ] {
            let mut bench = aloft(&record, Vec3::ZERO, turned);
            bench.run(1.5);
            let left = angle(bench.rotation()).abs();
            assert!(
                left < 5.0,
                "{name} 25 deg with no key held, still {left:.1} deg after 1.5 s"
            );
        }
        let mut bench = aloft(&record, Vec3::ZERO, Quat::IDENTITY);
        bench.hold(&[KeyCode::KeyS]);
        let mut over_the_top = false;
        for _ in 0..(1.5 * crate::player::sim::BENCH_HZ) as usize {
            bench.step();
            over_the_top |= (bench.rotation() * Vec3::Y).y < 0.0;
        }
        assert!(
            over_the_top,
            "S held for 1.5 s must carry the nose over the top"
        );
    }

    /// A driver holding W off a ramp lands on the wheels: the press was made
    /// on the ground, so it drives and does not tip the car in the air, and
    /// the levelling brings the car in flat. Let go and press it again in the
    /// air and it counts - the same run then noses in.
    #[test]
    fn a_key_held_off_the_ground_does_not_tip_the_car() {
        let record = default_car();
        let held = ramp_run(
            &record,
            14.0,
            18.0,
            &[KeyCode::KeyW],
            keep_going,
            CarBumpStop::default(),
        );
        let landed = pitch(held.touchdown().rotation);
        assert!(
            landed.abs() < 10.0,
            "W held from the ground must not pitch the car: landed at {landed:.1} deg"
        );
        let pressed = ramp_run(
            &record,
            14.0,
            18.0,
            &[KeyCode::KeyW],
            press_w_again,
            CarBumpStop::default(),
        );
        let landed = pitch(pressed.touchdown().rotation);
        assert!(
            landed < -20.0,
            "W pressed in the air must pitch the nose down: landed at {landed:.1} deg"
        );
    }

    /// The self-levelling is passive, so it goes on while the player types
    /// (#821): W held off a ramp, then the focus in a text field from the
    /// lip - the drive stands down, the car still comes in level. When it
    /// lived in the drive, the critic of #1524 measured this landing at
    /// -29 degrees against -4 with the keys simply let go.
    #[test]
    fn the_levelling_keeps_working_while_the_player_types() {
        let flight = ramp_run(
            &default_car(),
            14.0,
            18.0,
            &[KeyCode::KeyW],
            start_typing,
            CarBumpStop::default(),
        );
        let landed = pitch(flight.touchdown().rotation);
        assert!(
            landed.abs() < 10.0,
            "typing from the lip, the car must still land level: {landed:.1} deg"
        );
    }

    /// Near the ground a held roll is the player's to finish: the uprighting
    /// assist leaves alone the axis a counted air key holds, as the levelling
    /// does. E pressed at the lip, the car rolls on until it lands; with the
    /// assist fighting it past its engage tilt (the critic of #1524: 130
    /// degrees, touching down at 129 degrees of tilt, on its side) it hung
    /// on its side.
    #[test]
    fn a_held_roll_near_the_ground_goes_on_until_the_car_lands() {
        let flight = ramp_run(
            &default_car(),
            14.0,
            18.0,
            &[KeyCode::KeyW],
            press_e,
            CarBumpStop::default(),
        );
        let rolled = flight.rolled();
        assert!(
            rolled > 200.0,
            "E held from the lip must roll the car on over its roof, rolled {rolled:.0} deg"
        );
    }

    /// Q/E do nothing on the ground, so the latch does not hold them back:
    /// E held from the run-up rolls the car as soon as it leaves the lip.
    /// Latched like W/S/A/D it rolled it not at all.
    #[test]
    fn a_roll_key_held_from_the_run_up_rolls_the_car_off_the_lip() {
        let flight = ramp_run(
            &default_car(),
            14.0,
            18.0,
            &[KeyCode::KeyW, KeyCode::KeyE],
            keep_going,
            CarBumpStop::default(),
        );
        let rolled = flight.rolled();
        assert!(
            rolled > 200.0,
            "E held from the run-up must roll the car in the air, rolled {rolled:.0} deg"
        );
    }

    /// The self-levelling is for the air. A car LYING on the ground - on its
    /// side or its roof, the rover on its roof riding nothing but its box - is
    /// the uprighting assist's to right, and with the assist turned off (its
    /// strength and its damping at 0) it stays down. Before, the levelling
    /// (mass x 6 x the sine of the tilt, more than the assist's 2.5) righted
    /// it in under a second whatever the record said.
    #[test]
    fn a_car_lying_on_the_ground_is_left_to_the_uprighting_assist() {
        use crate::seeded_defaults::{AvatarPins, CraftType, SkiffType};
        let mut pins = AvatarPins::default();
        pins.lock_craft(Some(CraftType::Skiff(SkiffType::Rover)));
        let rover =
            AvatarRecord::default_for_seed(pins.find_seed(0).expect("a rover is reachable"));
        for (name, record) in [("the default car", default_car()), ("the rover", rover)] {
            let no_assist = tuned(&record, |p| {
                p.upright_assist_accel = Fp(0.0);
                p.upright_assist_damping = Fp(0.0);
            });
            assert!(
                car_params(&no_assist).air_level_accel.0 > 0.0,
                "the levelling is on"
            );
            for (pose, turned) in [
                ("side", Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
                ("roof", Quat::from_rotation_z(std::f32::consts::PI)),
            ] {
                assert!(
                    rights_itself(&no_assist, turned, &[], 3.0).is_none(),
                    "{name} on its {pose} righted itself with the assist off"
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // The bump stop
    // -----------------------------------------------------------------------

    /// A car dropped flat with its box 3 m up lands on its springs, not its
    /// box, and comes back up no harder than it would have off the box. The
    /// linear spring alone lets the box hit - it sits 22% into its travel at
    /// rest and the drop lands at 6.8 m/s - and a stop that pumps energy in
    /// fails the second half: weighed at the step's start rather than its
    /// end, this one kept 5 mm under the box and threw it up at 4.3 m/s.
    #[test]
    fn a_hard_landing_keeps_the_box_off_the_floor() {
        let record = default_car();
        let half_height = car_params(&record).chassis_half_extents.0[1];
        let drop = |stop: CarBumpStop| {
            let mut bench =
                DriveBench::new(&record, Transform::from_xyz(0.0, 3.0 + half_height, 0.0));
            bench.bump_stop(stop);
            let (mut lowest, mut rebound) = (f32::MAX, 0.0f32);
            for _ in 0..(1.5 * crate::player::sim::BENCH_HZ) as usize {
                bench.step();
                lowest = lowest.min(bench.underside());
                rebound = rebound.max(bench.velocity().y);
            }
            (lowest, rebound)
        };
        let (bare, bare_rebound) = drop(CarBumpStop::OFF);
        assert!(
            bare < 0.01,
            "the premise: without the stop the box hits, {bare:.3} m"
        );
        let (stopped, rebound) = drop(CarBumpStop::default());
        assert!(
            stopped > 0.02,
            "the box must stay off the floor through the landing, came to {stopped:.3} m"
        );
        assert!(
            rebound <= bare_rebound,
            "the stop must not throw the car back up: {rebound:.2} m/s against {bare_rebound:.2}"
        );
    }

    // -----------------------------------------------------------------------
    // Contact and the dampings
    // -----------------------------------------------------------------------

    /// With all four wheels down the chassis carries the record's ground
    /// dampings to the bit, in the air the air ones, and with two wheels
    /// down the blend - and the swap is the suspension's, so it happens while
    /// the player types and the drive is stood down.
    #[test]
    fn the_dampings_follow_the_wheels_even_while_typing() {
        let record = default_car();
        let p = car_params(&record).clone();
        let parked = DriveBench::parked(&record, 0.0, 0.0);
        assert_eq!(parked.contact().wheels, 4);
        assert_eq!(parked.damping(), (p.linear_damping.0, p.angular_damping.0));

        let mut flying = aloft(&record, Vec3::ZERO, Quat::IDENTITY);
        flying.typing(true);
        flying.step();
        assert_eq!(flying.contact().wheels, 0);
        assert_eq!(
            flying.damping(),
            (p.air_linear_damping.0, p.air_angular_damping.0),
            "the air dampings must be on while the player types"
        );

        // The rear wheels on a platform's edge, the front ones over the drop.
        let half = Vec3::from_array(p.chassis_half_extents.0);
        let mut ledge = DriveBench::new(
            &record,
            Transform::from_xyz(0.0, 2.0 + half.y + 0.45, -half.z * 0.5),
        );
        ledge.block(Transform::from_xyz(0.0, 1.0, 5.0), Vec3::new(4.0, 1.0, 5.0));
        ledge.typing(true);
        ledge.step();
        ledge.step();
        assert_eq!(ledge.contact().wheels, 2, "two wheels on the ledge");
        let blend = |air: f32, ground: f32| air * 0.5 + ground * 0.5;
        assert_eq!(
            ledge.damping(),
            (
                blend(p.air_linear_damping.0, p.linear_damping.0),
                blend(p.air_angular_damping.0, p.angular_damping.0)
            )
        );
    }

    /// A car the wrong way up has no wheel on the ground, however near its
    /// corner rays find it. Five of the six seeded skiff types are thin
    /// enough that a car lying on its roof would read four wheels down - and
    /// the rover, the thinnest, is the test. Its roof and its side are still
    /// near the ground, which is what the uprighting reads.
    #[test]
    fn a_car_on_its_roof_or_side_has_no_wheels_down_but_is_near_the_ground() {
        use crate::seeded_defaults::{AvatarPins, CraftType, SkiffType};
        let mut pins = AvatarPins::default();
        pins.lock_craft(Some(CraftType::Skiff(SkiffType::Rover)));
        let rover =
            AvatarRecord::default_for_seed(pins.find_seed(0).expect("a rover is reachable"));
        let half = Vec3::from_array(car_params(&rover).chassis_half_extents.0);
        for (pose, turned, rests_at) in [
            ("roof", Quat::from_rotation_z(std::f32::consts::PI), half.y),
            (
                "side",
                Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                half.x,
            ),
        ] {
            let mut bench = DriveBench::new(
                &tuned(&rover, |p| p.upright_assist_accel = Fp(0.0)),
                Transform::from_xyz(0.0, rests_at + 0.01, 0.0).with_rotation(turned),
            );
            bench.step();
            let contact = bench.contact();
            assert_eq!(contact.wheels, 0, "on its {pose} no wheel is down");
            assert!(
                contact.near_ground,
                "on its {pose} the car is near the ground"
            );
        }
    }

    // -----------------------------------------------------------------------
    // The uprighting: near the ground only, and it rights a car on its roof
    // -----------------------------------------------------------------------

    /// In the air the uprighting is silent: a car tumbling well past its
    /// engage tilt keeps tumbling exactly as it would with no assist at all.
    /// The levelling is off here, so nothing else acts.
    #[test]
    fn the_uprighting_is_silent_in_the_air() {
        let spin = |record: &AvatarRecord| {
            let mut bench = aloft(record, Vec3::ZERO, Quat::from_rotation_z(2.0));
            bench.set_angular_velocity(Vec3::new(0.0, 0.0, 3.0));
            bench.run(1.0);
            (bench.rotation(), bench.angular_velocity())
        };
        let still = tuned(&default_car(), |p| p.air_level_accel = Fp(0.0));
        let no_assist = tuned(&still, |p| {
            p.upright_assist_accel = Fp(0.0);
            p.upright_assist_damping = Fp(0.0);
        });
        let (with_q, with_w) = spin(&still);
        let (without_q, without_w) = spin(&no_assist);
        assert!(
            with_w.abs_diff_eq(without_w, 1e-5) && with_q.abs_diff_eq(without_q, 1e-5),
            "the assist acted in the air: spin {with_w:?} against {without_w:?}"
        );
        // Only the air damping acts on it: 3 rad/s x e^-1 after a second at
        // the default 1/s. The assist's own damping would have taken it to a
        // twentieth of that.
        assert!(with_w.z > 1.0, "the tumble must carry on, got {with_w:?}");
    }

    /// Seconds for a car resting on the floor turned by `turned`, with
    /// `keys` held throughout, to stand on all four wheels, upright, for half
    /// a second - or `None` in `limit`.
    fn rights_itself(
        record: &AvatarRecord,
        turned: Quat,
        keys: &[KeyCode],
        limit: f64,
    ) -> Option<f64> {
        let half = Vec3::from_array(car_params(record).chassis_half_extents.0);
        // Where the turned box's lowest point is, so it starts at rest.
        let low: f32 = [Vec3::X, Vec3::Y, Vec3::Z]
            .iter()
            .map(|axis| (turned * (*axis * half.dot(*axis))).y.abs())
            .sum();
        let mut bench = DriveBench::new(
            record,
            Transform::from_xyz(0.0, low + 0.01, 0.0).with_rotation(turned),
        );
        bench.hold(keys);
        let mut since: Option<f64> = None;
        while bench.elapsed() < limit {
            bench.step();
            if tilt(bench.rotation()) < 15.0 && bench.contact().wheels == 4 {
                let at = *since.get_or_insert(bench.elapsed());
                if bench.elapsed() - at >= 0.5 {
                    return Some(at);
                }
            } else {
                since = None;
            }
        }
        None
    }

    /// #804's promise, for the whole seeded fleet: a car on its roof or its
    /// side on the ground rights itself. At #1524's start it did not - the
    /// drive bench found no car of the fleet coming off its roof, and the
    /// default car and the fleet's Cyclecar rolled from their left side onto it.
    #[test]
    fn every_seeded_skiff_rights_itself_from_its_roof_and_its_side() {
        use crate::seeded_defaults::{AvatarPins, CraftType};
        let mut fleet = vec![("the default car".to_string(), default_car())];
        for craft in CraftType::SKIFFS {
            let mut pins = AvatarPins::default();
            pins.lock_craft(Some(craft));
            let seed = pins.find_seed(0).expect("every skiff type is reachable");
            fleet.push((
                craft.label().to_string(),
                AvatarRecord::default_for_seed(seed),
            ));
        }
        for (name, record) in &fleet {
            for (pose, turned) in [
                ("roof", Quat::from_rotation_z(std::f32::consts::PI)),
                (
                    "left side",
                    Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                ),
            ] {
                assert!(
                    rights_itself(record, turned, &[], 5.0).is_some(),
                    "{name} on its {pose} did not right itself in 5 s"
                );
            }
        }
    }

    /// A car lying on the ground is not in the air, so a key held there
    /// does not keep it down: the air keys count only off the ground, and
    /// the assist has every axis. Before, the assist left out the axis a
    /// counted key held, and a key's own torque (mass x `air_control_accel`)
    /// is less than gravity's to tip the box - E or Q held kept every car of
    /// the fleet on its roof, the wrong roll key kept a car on its side, and
    /// W kept a Cyclecar on its nose (session 893's end review of #1524).
    #[test]
    fn a_key_held_while_lying_does_not_keep_the_car_down() {
        use crate::seeded_defaults::{AvatarPins, CraftType};
        use std::f32::consts::{FRAC_PI_2, PI};
        let mut fleet = vec![
            ("the default car".to_string(), default_car()),
            ("Jink's Cyclecar".to_string(), seeded_cyclecar()),
        ];
        for craft in CraftType::SKIFFS {
            let mut pins = AvatarPins::default();
            pins.lock_craft(Some(craft));
            let seed = pins.find_seed(0).expect("every skiff type is reachable");
            fleet.push((
                craft.label().to_string(),
                AvatarRecord::default_for_seed(seed),
            ));
        }
        let roof = Quat::from_rotation_z(PI);
        let side = Quat::from_rotation_z(FRAC_PI_2);
        let nose = Quat::from_rotation_x(-FRAC_PI_2);
        for (name, record) in &fleet {
            for (pose, turned, key) in [
                ("roof", roof, KeyCode::KeyE),
                ("roof", roof, KeyCode::KeyQ),
                ("left side", side, KeyCode::KeyE),
                ("left side", side, KeyCode::KeyQ),
                ("nose", nose, KeyCode::KeyW),
            ] {
                assert!(
                    rights_itself(record, turned, &[key], 5.0).is_some(),
                    "{name} on its {pose} with {key:?} held did not right itself in 5 s"
                );
            }
        }
    }

    /// A box too tall for its corner rays to reach the floor from its roof
    /// is near the ground by touching it, and rights itself too; and lying on
    /// its side, where gravity holds a tall box down harder than the record's
    /// assist strength, the assist's tipping floor rolls it back onto its
    /// wheels. The rays' reach is the rest length plus 1.5 m; a record may
    /// author a chassis 2.4 m tall.
    #[test]
    fn a_car_too_tall_for_its_rays_still_rights_itself() {
        let tall = tuned(&default_car(), |p| {
            p.chassis_half_extents = crate::pds::Fp3([0.8, 1.2, 1.6]);
        });
        for (pose, turned) in [
            ("roof", Quat::from_rotation_z(std::f32::consts::PI)),
            ("side", Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
        ] {
            assert!(
                rights_itself(&tall, turned, &[], 6.0).is_some(),
                "a 2.4 m tall car on its {pose} did not right itself in 6 s"
            );
        }
    }

    // -----------------------------------------------------------------------
    // The jump table (#1524)
    // -----------------------------------------------------------------------

    /// Seconds for a held key to turn a car one full turn about its own axis,
    /// high in the air.
    fn full_turn(record: &AvatarRecord, key: KeyCode) -> f32 {
        let mut bench = aloft(record, Vec3::ZERO, Quat::IDENTITY);
        bench.hold(&[key]);
        let mut turned = 0.0;
        while bench.elapsed() < 6.0 {
            bench.step();
            let axis = bench.rotation()
                * match key {
                    KeyCode::KeyW | KeyCode::KeyS => Vec3::X,
                    KeyCode::KeyA | KeyCode::KeyD => Vec3::Y,
                    _ => Vec3::Z,
                };
            turned +=
                bench.angular_velocity().dot(axis).abs() / crate::player::sim::BENCH_HZ as f32;
            if turned >= std::f32::consts::TAU {
                return bench.elapsed() as f32;
            }
        }
        f32::NAN
    }

    /// PRINT-ONLY. What a jump does, for the region's ramps to be designed
    /// from (#1524): the default car and the seeded Cyclecar of
    /// did:plc:lr2ocunor73lfqpwtgvt274o, W held from the run-up to the
    /// landing as a driver holds it, at 10 / 14 / 18 m/s into ramps of 12 /
    /// 18 / 25 degrees whose lip stands 1.5 m up.
    ///
    /// * `lip` - the car's flat speed as it leaves the ramp (m/s): the ramp
    ///   costs speed, and the default car's top speed is 11.1 m/s.
    /// * `air` - seconds from the last wheel on the ramp to the first back
    ///   down.
    /// * `peak` - how far the wheels' line rose above the lip (m).
    /// * `dist` - lip to where the car's centre was at touchdown (m).
    /// * `pitch` - nose above the horizon at touchdown (deg), with the
    ///   self-levelling, and `bare` without it.
    /// * `under` / `kept` / `up` - the landing: the least clearance under the
    ///   box over the second after touchdown (m), the least flat speed in it
    ///   as a share of the touchdown speed, and the fastest the car rose in
    ///   it (m/s) - with the bump stop, and after `off` without it.
    ///
    /// Then how long a held key takes to turn each car a full turn about
    /// each axis in the air.
    #[test]
    #[ignore = "probe for #1524: the jump table the region's ramps are designed from"]
    fn probe_the_jump_table() {
        for (name, record) in [
            ("default car", default_car()),
            ("Cyclecar", seeded_cyclecar()),
        ] {
            let bare = tuned(&record, |p| p.air_level_accel = Fp(0.0));
            println!("\n{name} - lip {LIP_M} m, W held throughout");
            println!(
                "{:>4} {:>4} | {:>5} {:>5} {:>5} {:>6} | {:>6} {:>6} | {:>6} {:>5} {:>4} | {:>6} {:>5} {:>4}",
                "m/s",
                "deg",
                "lip",
                "air",
                "peak",
                "dist",
                "pitch",
                "bare",
                "under",
                "kept",
                "up",
                "off",
                "kept",
                "up"
            );
            for degrees in [12.0, 18.0, 25.0] {
                for approach in [10.0, 14.0, 18.0] {
                    let w = [KeyCode::KeyW];
                    let run = ramp_run(
                        &record,
                        approach,
                        degrees,
                        &w,
                        keep_going,
                        CarBumpStop::default(),
                    );
                    let unlevelled = ramp_run(
                        &bare,
                        approach,
                        degrees,
                        &w,
                        keep_going,
                        CarBumpStop::default(),
                    );
                    let unstopped =
                        ramp_run(&record, approach, degrees, &w, keep_going, CarBumpStop::OFF);
                    let landing = |flight: &Flight| {
                        let touchdown = flight.touchdown().flat_speed();
                        let under = flight
                            .landing
                            .iter()
                            .map(|s| s.underside)
                            .fold(f32::MAX, f32::min);
                        let kept = flight
                            .landing
                            .iter()
                            .map(Sample::flat_speed)
                            .fold(f32::MAX, f32::min);
                        let up = flight
                            .landing
                            .iter()
                            .map(|s| s.velocity.y)
                            .fold(0.0, f32::max);
                        (under, 100.0 * kept / touchdown, up)
                    };
                    let (under, kept, up) = landing(&run);
                    let (off_under, off_kept, off_up) = landing(&unstopped);
                    let peak = run
                        .airborne
                        .iter()
                        .map(|s| s.position.y)
                        .fold(f32::MIN, f32::max);
                    let parked = car_params(&record);
                    let sag = parked.mass.0 * G / (4.0 * parked.suspension_stiffness.0);
                    let ride =
                        parked.chassis_half_extents.0[1] + parked.suspension_rest_length.0 - sag;
                    println!(
                        "{approach:>4.0} {degrees:>4.0} | {:>5.2} {:>5.2} {:>5.2} {:>6.2} | {:>6.1} {:>6.1} | {:>6.3} {:>4.0}% {up:>4.1} | {:>6.3} {:>4.0}% {off_up:>4.1}",
                        run.takeoff().flat_speed(),
                        run.touchdown().t - run.takeoff().t,
                        peak - ride - LIP_M,
                        LIP_Z - run.touchdown().position.z,
                        pitch(run.touchdown().rotation),
                        pitch(unlevelled.touchdown().rotation),
                        under,
                        kept,
                        off_under,
                        off_kept,
                    );
                }
            }
            println!(
                "a full turn with the key held: pitch (S) {:.2} s, roll (E) {:.2} s, yaw (A) {:.2} s",
                full_turn(&record, KeyCode::KeyS),
                full_turn(&record, KeyCode::KeyE),
                full_turn(&record, KeyCode::KeyA)
            );
        }
    }
}
