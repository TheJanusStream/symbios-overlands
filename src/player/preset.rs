//! Per-preset physics components: the player-side [`PresetComponents`]
//! trait (one impl per locomotion `*Params`), the preset marker
//! components, and the build/strip pair the spawn + hot-swap paths share.
//!
//! The dispatch lives here rather than on [`LocomotionConfig`] because
//! the record
//! layer must stay Bevy/Avian-free - PDS describes *what the avatar is*;
//! the player module owns *what that means in the physics world* (the
//! same layering as [`crate::interaction`]'s `LocomotionFootprint` trait).

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::config::rover as cfg;
use crate::pds::{
    AirplaneParams, CarParams, HelicopterParams, HoverBoatParams, HumanoidParams, LocomotionConfig,
};

/// Marks the local or remote player as currently using the HoverBoat
/// preset. Inserted by [`build_preset_components`]; stripped by the
/// hot-swap system when the owner picks a different preset.
#[derive(Component)]
pub struct HoverBoatPreset;

/// Marks the player as using the Humanoid preset - the "On foot" body, a
/// person upright on its capsule or a beast lying along it (#1802). Brings
/// the controller's water memory with it (#1324), so no humanoid chassis can
/// be classified without one, and a lying body's turn memory (#1802).
#[derive(Component)]
#[require(super::humanoid::HumanoidWater, super::humanoid::LyingTurn)]
pub struct HumanoidPreset;

/// Marks the player as using the Airplane preset.
#[derive(Component)]
pub struct AirplanePreset;

/// Marks the player as using the Helicopter preset.
#[derive(Component)]
pub struct HelicopterPreset;

/// Marks the player as using the Car preset.
#[derive(Component)]
pub struct CarPreset;

/// Aggregate marker query target for camera follow / vehicle-yaw
/// inheritance. Covers every preset whose physics body rotates around Y
/// - i.e. anything except the upright-locked Humanoid.
#[derive(Component)]
pub struct VehicleChassis;

/// Per-preset physics component builder: each locomotion `*Params`
/// knows the Avian components its chassis needs. The caller is
/// responsible for having stripped any prior preset's components first
/// ([`strip_preset_components`]) or for the entity being fresh.
pub(super) trait PresetComponents {
    /// The body this preset builds from its record: shape, mass, dampings
    /// and centre of mass - what [`sync_chassis`] keeps a living body in
    /// step with (#1610).
    fn chassis(&self) -> ChassisSpec;
    /// Everything else the preset's body carries: its markers, and what
    /// only it needs.
    fn insert_preset(&self, commands: &mut Commands, entity: Entity);
}

/// The body a preset builds, read off its record (#1610).
///
/// The one description both a build ([`build_preset_components`]) and the
/// per-step [`sync_chassis`] apply, so a body rebuilt for a record and a
/// body kept in step with it cannot differ in anything this describes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ChassisSpec {
    shape: ChassisShape,
    mass: f32,
    linear_damping: f32,
    angular_damping: f32,
    /// Where the body's mass is centred in its own frame - the car's,
    /// lowered (#804), and the airplane's, where her record puts it along
    /// her (#1617); `None` leaves avian's own, the shape's middle.
    center_of_mass: Option<Vec3>,
    /// The preset writes its dampings itself every fixed step - the car's
    /// suspension blends its air and ground ones by the wheels it has down
    /// (#1524) - so the sync leaves them to it.
    steps_own_damping: bool,
    /// Which ways the physics may not turn the body, or `None` for a body
    /// it turns freely - every vehicle's.
    locks: Option<Locks>,
}

/// A chassis' collider, as its record sizes it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ChassisShape {
    /// A vehicle's box, by its half-extents (m).
    Box([f32; 3]),
    /// An on-foot body's capsule: its radius and cylindrical length (m), up
    /// the body or along it.
    Capsule {
        radius: f32,
        length: f32,
        axis: CapsuleAxis,
    },
}

/// Which way an on-foot body's capsule runs (#1802).
#[derive(Clone, Copy, Debug, PartialEq)]
enum CapsuleAxis {
    /// Up the body, a person's: avian's own capsule, along Y.
    Upright,
    /// Along the body, a beast's lying on it: along the body's Z, its
    /// forward axis, its middle `ahead` metres in front of the body's
    /// origin - the girdles' midpoint
    /// ([`crate::pds::HumanoidParams::capsule_ahead`]).
    Lying { ahead: f32 },
}

/// Which rotations the physics holds an on-foot body out of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Locks {
    /// All three: the body is turned by its controller writing its facing,
    /// which an upright capsule - round about Y - is the same shape at.
    Upright,
    /// Pitch and roll, so the physics turns it about Y (#1802): a lying
    /// capsule is long, and turned by a written facing its ends would be put
    /// into whatever stands beside it. The controller turns it by its
    /// angular velocity instead, and the solver stops a turn at a wall.
    Level,
}

impl Locks {
    fn axes(self) -> LockedAxes {
        match self {
            Self::Upright => LockedAxes::new()
                .lock_rotation_x()
                .lock_rotation_y()
                .lock_rotation_z(),
            Self::Level => LockedAxes::new().lock_rotation_x().lock_rotation_z(),
        }
    }
}

impl ChassisShape {
    fn collider(self) -> Collider {
        match self {
            Self::Box(half) => Collider::cuboid(half[0] * 2.0, half[1] * 2.0, half[2] * 2.0),
            Self::Capsule {
                radius,
                length,
                axis: CapsuleAxis::Upright,
            } => Collider::capsule(radius, length),
            Self::Capsule {
                radius,
                length,
                axis: CapsuleAxis::Lying { ahead },
            } => {
                // Bevy's forward is -Z: ahead of the origin is toward it.
                let (middle, end) = (Vec3::NEG_Z * ahead, Vec3::Z * (length * 0.5));
                Collider::capsule_endpoints(radius, middle - end, middle + end)
            }
        }
    }

    /// How far the shape reaches below its body's origin (m).
    fn half_height(self) -> f32 {
        match self {
            Self::Box(half) => half[1],
            Self::Capsule {
                radius,
                length,
                axis: CapsuleAxis::Upright,
            } => radius + length * 0.5,
            Self::Capsule {
                axis: CapsuleAxis::Lying { .. },
                radius,
                ..
            } => radius,
        }
    }
}

/// How far a body's underside clears the ground when it is set down (m):
/// a hand's breadth, so it drops onto the ground rather than starting in it.
const SET_DOWN_CLEARANCE_M: f32 = 0.1;

/// How high above the ground a body wearing `locomotion` is set down (m):
/// where a spawn, a recovery, a requested move or an arrival puts its
/// origin.
///
/// The spawn's metre ([`cfg::SPAWN_HEIGHT_OFFSET`]) - unless the body is
/// taller than that, when it is its own half-height and a hand's breadth:
/// an airship published between #1612 and #1657 has a box reaching its
/// slung gondola, up to 1.34 m below its origin, and set down a metre up its
/// keel started in the ground and was shoved out of it. The seeded Zeppelin
/// that replaced it stands about a metre tall in all, as the blimp, the sky
/// galleon and the hybrid do; a hot-air balloon, a floater and a long steam
/// dirigible stand taller, and are set down by their own boxes
/// (#1658-#1663). An airplane reaches below her box by her gear (#1615):
/// set down on gear taller than the metre leaves, she started with her
/// wheels pressed into the ground and sprang up off them.
pub(crate) fn set_down_height(locomotion: &LocomotionConfig) -> f32 {
    let reach = chassis_spec(locomotion).map_or(0.0, |spec| spec.shape.half_height());
    let gear = match locomotion {
        LocomotionConfig::Airplane(p) => p.gear_height.0,
        _ => 0.0,
    };
    cfg::SPAWN_HEIGHT_OFFSET.max(reach + gear + SET_DOWN_CLEARANCE_M)
}

/// Whether `locomotion`'s body lies along its capsule - a beast on foot
/// (#1802) - and so is always level: the physics holds its pitch and roll
/// ([`Locks::Level`]) and only ever turns it about Y, so a tilt it is set
/// down at is one nothing would take away.
pub(crate) fn lies_level(locomotion: &LocomotionConfig) -> bool {
    matches!(locomotion, LocomotionConfig::Humanoid(p) if p.is_lying())
}

/// `rotation` with its pitch and roll taken away (#1802): its turn about Y
/// alone - the twist of its swing-twist split about Y - so the way it faces
/// is kept. A rotation that is already level comes back bit for bit; one
/// turned upside down about a level axis, which faces no way, comes back
/// unturned.
pub(crate) fn level(rotation: Quat) -> Quat {
    if rotation.x == 0.0 && rotation.z == 0.0 {
        return rotation;
    }
    let twist = Quat::from_xyzw(0.0, rotation.y, 0.0, rotation.w);
    if twist.length_squared() < 1e-12 {
        return Quat::IDENTITY;
    }
    twist.normalize()
}

/// The rotation `locomotion`'s body is set down at where a spawn or a
/// recovery picks `rotation` - the ground's tilt under the way it faces: a
/// lying body level ([`lies_level`], [`level`]), every other body as picked.
pub(crate) fn set_down_rotation(locomotion: &LocomotionConfig, rotation: Quat) -> Quat {
    if lies_level(locomotion) {
        level(rotation)
    } else {
        rotation
    }
}

impl ChassisSpec {
    /// A vehicle's box chassis: the shape all four vehicle presets share.
    fn vehicle(half: [f32; 3], mass: f32, linear_damping: f32, angular_damping: f32) -> Self {
        Self {
            shape: ChassisShape::Box(half),
            mass,
            linear_damping,
            angular_damping,
            center_of_mass: None,
            steps_own_damping: false,
            locks: None,
        }
    }

    fn insert(self, commands: &mut Commands, entity: Entity) {
        let mut body = commands.entity(entity);
        body.insert((
            self.shape.collider(),
            Mass(self.mass),
            LinearDamping(self.linear_damping),
            AngularDamping(self.angular_damping),
        ));
        if let Some(center) = self.center_of_mass {
            body.insert(CenterOfMass(center));
        }
        if let Some(locks) = self.locks {
            body.insert(locks.axes());
        }
    }
}

/// The body a chassis was last built or synced to (#1610): what tells
/// [`sync_chassis`] whether a record change owes it anything.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(super) struct ChassisBuilt {
    /// The locomotion variant it was built for
    /// ([`LocomotionConfig::kind_tag`]). A variant change rebuilds the
    /// chassis (`hotswap::detect_local_locomotion_change`), so the sync
    /// stands down until the rebuild has landed.
    kind: &'static str,
    spec: ChassisSpec,
}

impl PresetComponents for HoverBoatParams {
    fn chassis(&self) -> ChassisSpec {
        ChassisSpec::vehicle(
            self.chassis_half_extents.0,
            self.mass.0,
            self.linear_damping.0,
            self.angular_damping.0,
        )
    }

    fn insert_preset(&self, commands: &mut Commands, entity: Entity) {
        commands
            .entity(entity)
            .insert((HoverBoatPreset, VehicleChassis));
    }
}

impl PresetComponents for AirplaneParams {
    fn chassis(&self) -> ChassisSpec {
        let half = self.chassis_half_extents.0;
        ChassisSpec {
            // Forward of the box's middle by the record's share (#1617): over
            // the wing, so the tail wheel carries what a taildragger's does.
            // Always set, the middle included, so a record that moves it
            // later has a component to move.
            center_of_mass: Some(Vec3::new(0.0, 0.0, -self.center_of_mass_ahead.0 * half[2])),
            ..ChassisSpec::vehicle(
                half,
                self.mass.0,
                self.linear_damping.0,
                self.angular_damping.0,
            )
        }
    }

    fn insert_preset(&self, commands: &mut Commands, entity: Entity) {
        // Throttle closed (#1615): a fresh airplane stays where it is put.
        commands.entity(entity).insert((
            AirplanePreset,
            VehicleChassis,
            super::airplane::Throttle::default(),
            super::airplane::Stick::default(),
        ));
    }
}

impl PresetComponents for HelicopterParams {
    fn chassis(&self) -> ChassisSpec {
        ChassisSpec::vehicle(
            self.chassis_half_extents.0,
            self.mass.0,
            self.linear_damping.0,
            self.angular_damping.0,
        )
    }

    fn insert_preset(&self, commands: &mut Commands, entity: Entity) {
        // Landed (#1612): a fresh chassis comes down onto what is under it.
        commands.entity(entity).insert((
            HelicopterPreset,
            VehicleChassis,
            super::helicopter::RotorLift::default(),
        ));
    }
}

impl PresetComponents for CarParams {
    fn chassis(&self) -> ChassisSpec {
        let half = self.chassis_half_extents.0;
        ChassisSpec {
            // A low centre of mass is the car's primary anti-rollover lever
            // (#804): placed below the body origin at a fraction of the
            // chassis half-height, it leaves the level ride height untouched
            // (the four symmetric suspension corners still balance the weight
            // with zero net torque when level) but makes gravity resist roll
            // and an inverted rest unstable. The fraction is record tuning
            // since #876.
            center_of_mass: Some(Vec3::new(0.0, -self.center_of_mass_drop.0 * half[1], 0.0)),
            steps_own_damping: true,
            ..ChassisSpec::vehicle(
                half,
                self.mass.0,
                self.linear_damping.0,
                self.angular_damping.0,
            )
        }
    }

    fn insert_preset(&self, commands: &mut Commands, entity: Entity) {
        // Contact starts FULLY GROUNDED (#1524): the suspension overwrites it
        // on the first step, and a harness that runs the drive without the
        // suspension - the planar drive probe - measures full traction.
        commands.entity(entity).insert((
            CarPreset,
            VehicleChassis,
            super::car::CarContact::default(),
            super::car::CarAirKeys::default(),
        ));
    }
}

impl PresetComponents for HumanoidParams {
    fn chassis(&self) -> ChassisSpec {
        // Traditional character controller: a person's capsule has all three
        // rotation axes locked so it slides without spinning, and the walk
        // controller rotates the chassis transform itself to face the
        // movement direction. A beast's lying capsule keeps only its pitch
        // and roll locked, and is turned by its angular velocity (#1802,
        // [`Locks::Level`]).
        let (axis, locks) = if self.is_lying() {
            (
                CapsuleAxis::Lying {
                    ahead: self.capsule_ahead(),
                },
                Locks::Level,
            )
        } else {
            (CapsuleAxis::Upright, Locks::Upright)
        };
        ChassisSpec {
            shape: ChassisShape::Capsule {
                radius: self.capsule_radius.0.max(0.05),
                length: self.capsule_length.0.max(0.1),
                axis,
            },
            mass: self.mass.0,
            linear_damping: self.linear_damping.0,
            angular_damping: cfg::ANGULAR_DAMPING,
            center_of_mass: None,
            steps_own_damping: false,
            locks: Some(locks),
        }
    }

    fn insert_preset(&self, commands: &mut Commands, entity: Entity) {
        commands.entity(entity).insert(HumanoidPreset);
    }
}

/// The body `locomotion`'s preset builds, or `None` for a record whose
/// preset this build does not model.
fn chassis_spec(locomotion: &LocomotionConfig) -> Option<ChassisSpec> {
    match locomotion {
        LocomotionConfig::HoverBoat(p) => Some(p.chassis()),
        LocomotionConfig::Humanoid(p) => Some(p.chassis()),
        LocomotionConfig::Airplane(p) => Some(p.chassis()),
        LocomotionConfig::Helicopter(p) => Some(p.chassis()),
        LocomotionConfig::Car(p) => Some(p.chassis()),
        LocomotionConfig::Unknown => None,
    }
}

/// Build `preset`'s body on `entity`: its chassis, stamped as built, and
/// the rest of what it carries.
fn build_preset(
    preset: &impl PresetComponents,
    kind: &'static str,
    commands: &mut Commands,
    entity: Entity,
) {
    let spec = preset.chassis();
    spec.insert(commands, entity);
    commands.entity(entity).insert(ChassisBuilt { kind, spec });
    preset.insert_preset(commands, entity);
}

/// Insert the physics components appropriate to the avatar's locomotion
/// preset - the per-variant dispatch into [`PresetComponents`].
pub(super) fn build_preset_components(
    commands: &mut Commands,
    entity: Entity,
    locomotion: &LocomotionConfig,
) {
    let kind = locomotion.kind_tag();
    match locomotion {
        LocomotionConfig::HoverBoat(p) => build_preset(p.as_ref(), kind, commands, entity),
        LocomotionConfig::Humanoid(p) => build_preset(p.as_ref(), kind, commands, entity),
        LocomotionConfig::Airplane(p) => build_preset(p.as_ref(), kind, commands, entity),
        LocomotionConfig::Helicopter(p) => build_preset(p.as_ref(), kind, commands, entity),
        LocomotionConfig::Car(p) => build_preset(p.as_ref(), kind, commands, entity),
        LocomotionConfig::Unknown => {
            // Forward-compat shipping a record whose preset we don't model:
            // give the entity a minimal collider so the simulation does not
            // explode. The owner's editor flags the unrecognised variant.
            commands
                .entity(entity)
                .insert((Collider::cuboid(0.5, 0.5, 0.5), Mass(40.0)));
        }
    }
}

/// Keep the local player's body in step with its live record (#1610): what
/// a rebuild would insert for the record - the collider, the mass, the
/// dampings, the car's centre of mass - written onto the living body
/// whenever the record changes it.
///
/// A body is only REBUILT when its locomotion variant changes
/// (`hotswap::detect_local_locomotion_change`), and everything else that
/// rewrites a record in place used to leave it as it was built: a seed
/// re-roll from one skiff to another, a Locomotion-tab slider, an undo.
/// Every system that pushes a body reads the RECORD - the car's springs,
/// drive and righting, the airship's hover thrust, the height a rigged
/// person is hung below the capsule - so the two parted: a skiff re-rolled
/// from an armoured car to a cyclecar settled 19 cm into the ground on the
/// old car's weight, an airship re-rolled to a lighter one fell 25 m in
/// five seconds, and a person re-rolled taller stood in the ground on the
/// old capsule. The hover boat alone used to keep its mass and dampings in
/// step; every preset keeps all of it now.
///
/// First in the fixed step, so every system after it reads the body the
/// record describes. Writes only what changed: a changed `Collider` makes
/// avian re-fit its bounds and recompute its mass properties, and a `&mut`
/// through a query stamps a change tick even when the value is the same.
///
/// And wakes the body when it writes. Avian wakes a sleeping body for a
/// changed pose, velocity or force, never for a changed collider or mass,
/// so a person standing still - asleep - kept standing on the capsule it
/// fell asleep on, half a metre into the ground under one re-rolled taller.
///
/// And brings the mass properties avian holds up to date at once (#1666).
/// `Forces::apply_force` divides by the mass the body holds when it is
/// called, and avian took up a new mass, collider or centre of mass only
/// in its own step, after this one's forces: sized for the new chassis,
/// they met the old body once - an airship re-rolled from the 200 kg
/// steam dirigible to the 46 kg floater lost 0.12 m/s of climb in the
/// step, the other way round gained 0.52. Avian's own update, run here;
/// its step repeats it to the same numbers.
///
/// DEFERRED while the visuals-edit freeze parks the chassis, as the
/// rebuild is (`hotswap::apply_local_locomotion_rebuild`, #867): a grown
/// collider on a parked, fully locked body cannot be pushed out of the
/// ground it now overlaps, and a collider changed under a parked body
/// touching the terrain is the class of edit behind the #867 meltdown. The
/// stamp keeps what is owed, so the first step after the release applies
/// it.
#[allow(clippy::type_complexity)]
pub(super) fn sync_chassis(
    mut commands: Commands,
    live: Res<crate::state::LiveAvatarRecord>,
    // The body, then avian's mass-property update over it: both read its
    // `Mass`, so they take turns.
    mut bodies: ParamSet<(
        Query<
            (
                Entity,
                &mut ChassisBuilt,
                (&mut Collider, &ColliderDensity, &mut ColliderMassProperties),
                &mut Mass,
                &mut LinearDamping,
                &mut AngularDamping,
                Option<&mut CenterOfMass>,
                Option<&mut LockedAxes>,
            ),
            (
                With<crate::state::LocalPlayer>,
                Without<super::VisualsEditFreeze>,
            ),
        >,
        MassPropertyHelper,
    )>,
) {
    let locomotion = &live.0.locomotion;
    let Some(spec) = chassis_spec(locomotion) else {
        return;
    };
    let mut player = bodies.p0();
    let Ok((body, mut built, shape, mut mass, mut linear, mut angular, center, locked)) =
        player.single_mut()
    else {
        return;
    };
    let (mut collider, density, mut collider_mass) = shape;
    if built.kind != locomotion.kind_tag() || built.spec == spec {
        return;
    }
    // Silenced as avian queues it itself: a body outside the simulation
    // (no island) has nothing to wake.
    commands.queue_silenced(WakeBody(body));
    let was = built.spec;
    if spec.shape != was.shape {
        *collider = spec.shape.collider();
        // As avian's own collider update computes them in its step.
        *collider_mass = ColliderMassProperties::from_shape(&*collider, density.0);
    }
    if spec.mass != was.mass {
        mass.0 = spec.mass;
    }
    if !spec.steps_own_damping {
        if spec.linear_damping != was.linear_damping {
            linear.0 = spec.linear_damping;
        }
        if spec.angular_damping != was.angular_damping {
            angular.0 = spec.angular_damping;
        }
    }
    if let (Some(want), Some(mut center)) = (spec.center_of_mass, center)
        && center.0 != want
    {
        center.0 = want;
    }
    // A person become a beast in place, or a beast a person (#1802): the
    // shape's axis and the rotations the physics holds it out of go
    // together, or a lying capsule is left unable to turn.
    if let (Some(want), Some(mut locked)) = (spec.locks, locked)
        && Some(want) != was.locks
    {
        *locked = want.axes();
    }
    built.spec = spec;
    bodies.p1().update_mass_properties(body);
}

/// Remove every preset-specific component + marker from `entity`.
/// Safe to call even if the entity currently carries only a subset - Bevy's
/// `remove` no-ops when the component is absent.
///
/// KEEP IN SYNC with the [`PresetComponents`] impls above: every component
/// any preset inserts must appear in this remove list, or a hot-swap
/// leaves the old preset's physics riding along under the new one.
pub(super) fn strip_preset_components(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).remove::<(
        // The chassis `ChassisSpec::insert` builds, and its stamp.
        (
            ChassisBuilt,
            Collider,
            Mass,
            CenterOfMass,
            LinearDamping,
            AngularDamping,
        ),
        LockedAxes,
        HoverBoatPreset,
        HumanoidPreset,
        AirplanePreset,
        HelicopterPreset,
        CarPreset,
        super::car::CarContact,
        super::car::CarAirKeys,
        super::helicopter::RotorLift,
        super::airplane::Throttle,
        super::airplane::Stick,
        VehicleChassis,
        super::gait::GaitAnimation,
    )>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::Fp;
    use crate::pds::avatar::AvatarRecord;
    use crate::player::sim::{DriveBench, FlightBench};
    use crate::seeded_defaults::{ChassisFamily, SkiffType};
    use crate::state::{LiveAvatarRecord, LocalPlayer};
    use bevy::ecs::system::RunSystemOnce;

    /// A record wearing `locomotion`.
    fn wearing(locomotion: LocomotionConfig) -> AvatarRecord {
        let mut record = AvatarRecord::default_for_did("did:plc:chassis-sync");
        record.locomotion = locomotion;
        record
    }

    /// Each preset's defaults, and the same preset with every field its body
    /// is built from moved.
    fn every_preset_moved() -> Vec<(AvatarRecord, AvatarRecord)> {
        fn moved_box(half: &mut crate::pds::Fp3, mass: &mut Fp, linear: &mut Fp, angular: &mut Fp) {
            half.0 = half.0.map(|h| h * 1.25);
            mass.0 *= 1.7;
            linear.0 += 0.25;
            angular.0 += 0.5;
        }
        let boat = HoverBoatParams::default();
        let mut boat2 = boat.clone();
        moved_box(
            &mut boat2.chassis_half_extents,
            &mut boat2.mass,
            &mut boat2.linear_damping,
            &mut boat2.angular_damping,
        );
        let plane = AirplaneParams::default();
        let mut plane2 = plane.clone();
        moved_box(
            &mut plane2.chassis_half_extents,
            &mut plane2.mass,
            &mut plane2.linear_damping,
            &mut plane2.angular_damping,
        );
        let ship = HelicopterParams::default();
        let mut ship2 = ship.clone();
        moved_box(
            &mut ship2.chassis_half_extents,
            &mut ship2.mass,
            &mut ship2.linear_damping,
            &mut ship2.angular_damping,
        );
        let car = CarParams::default();
        let mut car2 = car.clone();
        moved_box(
            &mut car2.chassis_half_extents,
            &mut car2.mass,
            &mut car2.linear_damping,
            &mut car2.angular_damping,
        );
        car2.center_of_mass_drop.0 *= 0.5;
        let person = HumanoidParams::default();
        let mut person2 = person.clone();
        person2.capsule_radius.0 *= 1.2;
        person2.capsule_length.0 *= 1.3;
        person2.mass.0 *= 1.4;
        person2.linear_damping.0 += 0.2;
        // A person become a beast in place (#1802): the same preset, its
        // capsule turned to lie along the body and its locks with it.
        let beast = crate::pds::avatar::fit_on_foot(
            &crate::pds::avatar::locomotion::default_beast(symbios_avatar::plan::Family::Dog),
            &person,
        );
        [
            (
                LocomotionConfig::HoverBoat(Box::new(boat)),
                LocomotionConfig::HoverBoat(Box::new(boat2)),
            ),
            (
                LocomotionConfig::Airplane(Box::new(plane)),
                LocomotionConfig::Airplane(Box::new(plane2)),
            ),
            (
                LocomotionConfig::Helicopter(Box::new(ship)),
                LocomotionConfig::Helicopter(Box::new(ship2)),
            ),
            (
                LocomotionConfig::Car(Box::new(car)),
                LocomotionConfig::Car(Box::new(car2)),
            ),
            (
                LocomotionConfig::Humanoid(Box::new(person.clone())),
                LocomotionConfig::Humanoid(Box::new(person2)),
            ),
            (
                LocomotionConfig::Humanoid(Box::new(person.clone())),
                LocomotionConfig::Humanoid(Box::new(beast.clone())),
            ),
            (
                LocomotionConfig::Humanoid(Box::new(beast)),
                LocomotionConfig::Humanoid(Box::new(person)),
            ),
        ]
        .into_iter()
        .map(|(a, b)| (wearing(a), wearing(b)))
        .collect()
    }

    /// The local player's body built for `record` on a bare world - no
    /// physics, so nothing but the system under test touches it.
    fn bare_body(record: &AvatarRecord) -> (World, Entity) {
        let mut world = World::new();
        world.insert_resource(LiveAvatarRecord(record.clone()));
        let body = world.spawn(LocalPlayer).id();
        let mut commands = world.commands();
        build_preset_components(&mut commands, body, &record.locomotion);
        world.flush();
        (world, body)
    }

    /// The collider's shape, read back as the spec writes it.
    fn shape_of(collider: &Collider) -> ChassisShape {
        let shape = collider.shape();
        if let Some(cuboid) = shape.as_cuboid() {
            let h = cuboid.half_extents;
            ChassisShape::Box([h.x, h.y, h.z])
        } else if let Some(capsule) = shape.as_capsule() {
            let (a, b) = (capsule.segment.a, capsule.segment.b);
            let axis = if a.x == b.x && a.z == b.z {
                CapsuleAxis::Upright
            } else {
                assert!(
                    a.x == 0.0 && b.x == 0.0 && a.y == 0.0 && b.y == 0.0,
                    "a lying capsule lies along the body's Z: {a:?} to {b:?}"
                );
                CapsuleAxis::Lying {
                    ahead: -(a.z + b.z) * 0.5,
                }
            };
            ChassisShape::Capsule {
                radius: capsule.radius,
                length: capsule.half_height() * 2.0,
                axis,
            }
        } else {
            panic!("a chassis is a box or a capsule")
        }
    }

    /// What a body is, as far as its chassis goes: shape, mass, dampings,
    /// centre of mass, stamp, and the rotations it is locked out of.
    #[allow(clippy::type_complexity)]
    fn read(
        world: &World,
        body: Entity,
    ) -> (
        ChassisShape,
        f32,
        [f32; 2],
        Option<Vec3>,
        ChassisBuilt,
        Option<u8>,
    ) {
        let at = world.entity(body);
        (
            shape_of(at.get::<Collider>().expect("a collider")),
            at.get::<Mass>().expect("a mass").0,
            [
                at.get::<LinearDamping>().expect("damping").0,
                at.get::<AngularDamping>().expect("damping").0,
            ],
            at.get::<CenterOfMass>().map(|c| c.0),
            *at.get::<ChassisBuilt>().expect("a stamp"),
            at.get::<LockedAxes>().map(|locked| locked.to_bits()),
        )
    }

    /// The rule (#1610): a body kept in step with a record changed in
    /// place IS the body a rebuild would build for it - every preset, every
    /// field its body is built from - except the car's dampings, which are
    /// its suspension's to write every step. And a sync with nothing to do
    /// writes nothing: a changed `Collider` makes avian rebuild the shape.
    #[test]
    fn a_body_kept_in_step_is_the_body_a_rebuild_builds() {
        for (was, now) in every_preset_moved() {
            let kind = now.locomotion.kind_tag();
            let (mut kept, body) = bare_body(&was);
            let before = read(&kept, body);
            kept.insert_resource(LiveAvatarRecord(now.clone()));
            kept.run_system_once(sync_chassis).expect("the sync runs");
            let (rebuilt, fresh) = bare_body(&now);
            let (shape, mass, damping, center, built, locked) = read(&kept, body);
            let want = read(&rebuilt, fresh);
            assert_ne!(before.0, want.0, "{kind}: the moved record moves the shape");
            assert_eq!(shape, want.0, "{kind}: the collider");
            assert_eq!(mass, want.1, "{kind}: the mass");
            assert_eq!(center, want.3, "{kind}: the centre of mass");
            assert_eq!(built, want.4, "{kind}: the stamp");
            assert_eq!(locked, want.5, "{kind}: the locked rotations");
            if want.4.spec.steps_own_damping {
                assert_eq!(damping, before.2, "{kind}: the suspension's dampings");
            } else {
                assert_eq!(damping, want.2, "{kind}: the dampings");
            }

            // Nothing more to do: nothing written.
            let ticks = |world: &World| {
                let at = world.entity(body);
                [
                    at.get_change_ticks::<Collider>().map(|t| t.changed),
                    at.get_change_ticks::<Mass>().map(|t| t.changed),
                    at.get_change_ticks::<LinearDamping>().map(|t| t.changed),
                    at.get_change_ticks::<AngularDamping>().map(|t| t.changed),
                    at.get_change_ticks::<CenterOfMass>().map(|t| t.changed),
                    at.get_change_ticks::<ChassisBuilt>().map(|t| t.changed),
                    at.get_change_ticks::<LockedAxes>().map(|t| t.changed),
                ]
            };
            let settled = ticks(&kept);
            kept.run_system_once(sync_chassis).expect("the sync runs");
            assert_eq!(ticks(&kept), settled, "{kind}: an idle sync wrote");
        }
    }

    /// **A beast's chassis lies along it** (#1802): its collider a capsule
    /// along its own Z, its middle the girdles' midpoint ahead of its origin,
    /// its pitch and roll locked and its yaw free for the physics to turn -
    /// held against the axes a lying body needs, written out here rather than
    /// read back off [`Locks`] - and it is set down by its radius, the
    /// spawn's metre, every default beast's capsule reaching less than that
    /// below its middle.
    #[test]
    fn a_beasts_chassis_lies_along_it() {
        use symbios_avatar::plan::Family;
        for family in [Family::Dog, Family::Cat, Family::Horse] {
            let record = crate::pds::avatar::locomotion::beast_record(family);
            let LocomotionConfig::Humanoid(p) = &record.locomotion else {
                panic!("a beast on foot");
            };
            let (world, body) = bare_body(&record);
            let (shape, mass, ..) = read(&world, body);
            let ChassisShape::Capsule {
                radius,
                length,
                axis: CapsuleAxis::Lying { ahead },
            } = shape
            else {
                panic!("a {family:?} is not lying on its capsule: {shape:?}");
            };
            assert_eq!((radius, length), (p.capsule_radius.0, p.capsule_length.0));
            assert!(
                (ahead - p.capsule_ahead()).abs() < 1e-6 && p.capsule_ahead() != 0.0,
                "a {family:?}'s capsule is centred {ahead} m ahead, its record {}",
                p.capsule_ahead()
            );
            assert_eq!(mass, p.mass.0);
            let locked = world.get::<LockedAxes>(body).expect("locked axes");
            assert_eq!(
                locked.to_bits(),
                LockedAxes::new()
                    .lock_rotation_x()
                    .lock_rotation_z()
                    .to_bits(),
                "a {family:?} is held out of its pitch and roll alone"
            );
            assert!(
                !locked.is_rotation_y_locked(),
                "a {family:?}'s yaw is the physics' to turn"
            );
            assert_eq!(shape.half_height(), p.capsule_radius.0);
            assert_eq!(
                set_down_height(&record.locomotion),
                cfg::SPAWN_HEIGHT_OFFSET
            );
        }
    }

    /// **A lying body is set down level, and nothing else is** (#1802): the
    /// ground's tilt under a spawn or a recovery is taken off a beast's
    /// rotation, keeping the way it faces, and a person's and a vehicle's
    /// keep it as they always have; a level rotation comes back bit for bit.
    /// Through the spawn and the recovery: `spawn::tests` and
    /// `respawn::tests`.
    #[test]
    fn only_a_lying_body_is_set_down_level() {
        let tilt = Quat::from_rotation_arc(Vec3::Y, Vec3::new(0.15, 1.0, 0.2).normalize());
        let picked = tilt * Quat::from_rotation_y(1.1);
        let beast = crate::pds::avatar::locomotion::beast_record(symbios_avatar::plan::Family::Dog);
        let set = set_down_rotation(&beast.locomotion, picked);
        assert!(
            (set * Vec3::Y).abs_diff_eq(Vec3::Y, 1e-6),
            "a beast is set down tilted: {set:?}"
        );
        // The ground's tilt is a turn about a level axis, so what is left is
        // the facing exactly.
        assert!(
            set.abs_diff_eq(Quat::from_rotation_y(1.1), 1e-6),
            "a beast set down level faces the way it was picked to: {set:?}"
        );
        let level_turn = Quat::from_rotation_y(-2.3);
        assert_eq!(set_down_rotation(&beast.locomotion, level_turn), level_turn);
        for other in [
            LocomotionConfig::Humanoid(Box::default()),
            AvatarRecord::default_for_seed(0).locomotion,
        ] {
            assert_eq!(set_down_rotation(&other, picked), picked, "{other:?}");
        }
        assert_eq!(
            level(Quat::from_rotation_x(std::f32::consts::PI)),
            Quat::IDENTITY
        );
    }

    /// **Every seeded person's chassis is the one it always was** (#1802):
    /// over 2000 people, the upright capsule the stature rule sizes, locked
    /// every way, at the preset's mass and dampings - the chassis built
    /// before a beast could lie along one.
    #[test]
    fn every_seeded_persons_chassis_is_unchanged() {
        let mut people = 0;
        for seed in 0u64.. {
            if people == 2000 {
                break;
            }
            if ChassisFamily::for_seed(seed) != ChassisFamily::Humanoid {
                continue;
            }
            people += 1;
            let body = crate::pds::avatar::wardrobe::engine_default_for_seed(seed);
            let symbios_avatar::Archetype::Humanoid(engine) = &body.archetype else {
                panic!("seed {seed} rolled no person");
            };
            let radius = (engine.height * 0.11).clamp(0.18, 0.34);
            let length = (engine.height - 2.0 * radius).max(0.4);
            let record = AvatarRecord::default_for_seed(seed);
            let LocomotionConfig::Humanoid(p) = &record.locomotion else {
                panic!("seed {seed}: a person on foot");
            };
            assert_eq!(
                p.chassis(),
                ChassisSpec {
                    shape: ChassisShape::Capsule {
                        radius: radius.max(0.05),
                        length: length.max(0.1),
                        axis: CapsuleAxis::Upright,
                    },
                    mass: 80.0,
                    linear_damping: 0.3,
                    angular_damping: cfg::ANGULAR_DAMPING,
                    center_of_mass: None,
                    steps_own_damping: false,
                    locks: Some(Locks::Upright),
                },
                "seed {seed}"
            );
        }
    }

    /// An airplane is set down with her wheels clear of the ground (#1615):
    /// on the default gear that is still the spawn's metre, and on tall
    /// gear it is higher by what the gear reaches.
    #[test]
    fn an_airplane_is_set_down_on_her_gear() {
        let mut p = AirplaneParams::default();
        let fresh = LocomotionConfig::Airplane(Box::new(p.clone()));
        assert_eq!(set_down_height(&fresh), cfg::SPAWN_HEIGHT_OFFSET);
        p.gear_height = Fp(2.0);
        let tall = LocomotionConfig::Airplane(Box::new(p.clone()));
        let wheels = set_down_height(&tall) - p.chassis_half_extents.0[1] - 2.0;
        assert!(
            (wheels - SET_DOWN_CLEARANCE_M).abs() < 1e-5,
            "on 2 m gear her wheels start {wheels} m up"
        );
    }

    /// Every seeded body is set down clear of the ground by its own box or
    /// capsule (#1612), and never lower than the spawn's metre. The bodies
    /// that ride a suspension - boats and skiffs - keep exactly that metre,
    /// which their ride heights are measured from; what rises is a long
    /// airplane on her tall legs and a person tall enough that the metre left
    /// their capsule a few millimetres off the ground - and an airship that
    /// stands taller than the metre leaves: every hot-air balloon and
    /// floater and a long steam dirigible (#1658-#1663), as one published
    /// before #1657 did, whose box reaches its slung gondola. The Zeppelin,
    /// the blimp, the sky galleon and the hybrid keep the metre.
    #[test]
    fn every_seeded_body_is_set_down_clear_of_the_ground() {
        let mut raised = 0;
        for seed in 0u64..400 {
            let record = AvatarRecord::default_for_seed(seed);
            let height = set_down_height(&record.locomotion);
            let reach = chassis_spec(&record.locomotion)
                .expect("a seeded body has a chassis")
                .shape
                .half_height();
            assert!(
                height >= reach + SET_DOWN_CLEARANCE_M - 1e-6 && height >= cfg::SPAWN_HEIGHT_OFFSET,
                "seed {seed}: set down {height} m up, reaching {reach} m below its origin"
            );
            match ChassisFamily::for_seed(seed) {
                // A helicopter's box is her fuselage, at most 1.5 m tall at
                // the blueprint's largest corner: half of it and the set-down
                // clearance is under the stock metre, so she sets down from
                // the stock height too.
                ChassisFamily::Boat | ChassisFamily::Skiff | ChassisFamily::Helicopter => {
                    assert_eq!(height, cfg::SPAWN_HEIGHT_OFFSET, "seed {seed}");
                }
                // The Zeppelin, the blimp, the sky galleon and the hybrid
                // reach less than the metre leaves and set down from it. A
                // hot-air balloon's box reaches past it to her basket's floor
                // (#1661), a floater's to her tentacles' tips (#1663) and a
                // long steam dirigible's to her pusher's disc (#1659): set
                // down 1.24 to 1.38 m, 1.09 to 1.15 and up to 1.08 under 400.
                ChassisFamily::Airship => {
                    let t = crate::seeded_defaults::AirshipType::for_seed(seed);
                    use crate::seeded_defaults::AirshipType::*;
                    match t {
                        HotAirBalloon | Floater | SteamDirigible => {
                            raised += usize::from(height > cfg::SPAWN_HEIGHT_OFFSET);
                        }
                        Zeppelin | Blimp | SkyGalleon | Hybrid => {
                            assert_eq!(height, cfg::SPAWN_HEIGHT_OFFSET, "seed {seed}: a {t:?}");
                        }
                    }
                }
                // An airplane stands on her gear (#1617): a long one on her
                // tall legs reaches past the metre, and is set down higher.
                ChassisFamily::Humanoid | ChassisFamily::Beast | ChassisFamily::Airplane => {
                    raised += usize::from(height > cfg::SPAWN_HEIGHT_OFFSET);
                }
            }
        }
        // 47 under 400 when #1657 took the airships out of the raised set -
        // the tall humanoids and the long airplanes - before the fan-out's
        // balloons, floaters and long steam dirigibles came back into it
        // (#1658-#1663).
        assert!(
            raised > 40,
            "only {raised} bodies under 400 are set down higher"
        );
    }

    /// A variant change is the rebuild's: until it lands, the body built
    /// for the old variant is left alone rather than given the new one's
    /// box, mass and dampings.
    #[test]
    fn a_variant_change_is_left_to_the_rebuild() {
        let boat = wearing(LocomotionConfig::HoverBoat(Box::default()));
        let car = wearing(LocomotionConfig::Car(Box::default()));
        let (mut world, body) = bare_body(&boat);
        let before = read(&world, body);
        world.insert_resource(LiveAvatarRecord(car));
        world.run_system_once(sync_chassis).expect("the sync runs");
        assert_eq!(read(&world, body), before);
    }

    /// A body parked by the visuals-edit freeze is left as it is, as the
    /// rebuild leaves it (#867), and is owed the change the moment the
    /// freeze lets go: the first sync after the release applies it.
    #[test]
    fn a_parked_body_is_brought_into_step_on_its_release() {
        let (was, now) = every_preset_moved()
            .into_iter()
            .find(|(was, _)| matches!(was.locomotion, LocomotionConfig::Car(_)))
            .expect("the car is among the presets");
        let (mut world, body) = bare_body(&was);
        let before = read(&world, body);
        world
            .entity_mut(body)
            .insert(super::super::VisualsEditFreeze {
                prior_locked_axes: None,
            });
        world.insert_resource(LiveAvatarRecord(now.clone()));
        world.run_system_once(sync_chassis).expect("the sync runs");
        assert_eq!(read(&world, body), before, "a parked body was rewritten");

        world
            .entity_mut(body)
            .remove::<super::super::VisualsEditFreeze>();
        world.run_system_once(sync_chassis).expect("the sync runs");
        let (rebuilt, fresh) = bare_body(&now);
        let (kept, want) = (read(&world, body), read(&rebuilt, fresh));
        assert_eq!(
            (kept.0, kept.1, kept.3, kept.4, kept.5),
            (want.0, want.1, want.3, want.4, want.5),
            "the released body was not brought into step"
        );
    }

    /// The first seed under 600 that draws a skiff of type `t`.
    fn skiff(t: SkiffType) -> AvatarRecord {
        let seed = (0u64..600)
            .find(|&s| {
                ChassisFamily::for_seed(s) == ChassisFamily::Skiff && SkiffType::for_seed(s) == t
            })
            .expect("every type is reachable under 600");
        AvatarRecord::default_for_seed(seed)
    }

    /// The bug #1610 was filed for: a skiff re-rolled into another in place
    /// (the seed row's re-roll, which keeps the Car preset and so never
    /// rebuilds the chassis) settled on the OLD skiff's weight, against
    /// the new one's springs. Measured on these four before the fix: an
    /// armoured car re-rolled to a cyclecar sank its tyres 19.3 cm into the
    /// ground, a wagon to a rover 22.7 cm and a roadster to a dune buggy
    /// 7.8 cm, while a cyclecar to an armoured car floated 7.8 cm. Each now
    /// stands where its own visual hangs its tyres from: the ground line the
    /// assembler derived its travel drop against.
    #[test]
    fn a_skiff_re_rolled_in_place_stands_on_its_own_wheels() {
        use SkiffType::{ArmouredCar, Cyclecar, DuneBuggy, Roadster, Rover, Wagon};
        for (from, to) in [
            (ArmouredCar, Cyclecar),
            (Wagon, Rover),
            (Roadster, DuneBuggy),
            (Cyclecar, ArmouredCar),
        ] {
            let (was, now) = (skiff(from), skiff(to));
            let mut bench = DriveBench::parked(&was, 0.0, 0.0);
            bench.run(1.0);
            bench
                .world_mut()
                .insert_resource(LiveAvatarRecord(now.clone()));
            bench.run(4.0);
            let want = crate::pds::avatar::default_visuals::ground_ride_height(&now.locomotion)
                .expect("a skiff settles on a suspension");
            let off = bench.position().y - want;
            assert!(
                off.abs() < 1e-3,
                "{} re-rolled to a {}: its tyres stand {:+.1} cm off the ground",
                from.label(),
                to.label(),
                off * 100.0
            );
        }
    }

    /// The same defect in the air: an airship's hover thrust is its
    /// record's, sized for the record's mass, so on the old body's mass a
    /// re-roll to a lighter airship fell 25 m in five seconds and one to a
    /// heavier climbed 54, still gathering speed. Now it holds.
    ///
    /// And from the step of the swap (#1666): `apply_force` divides by the
    /// mass the body holds when it is called, and avian took up the one
    /// [`sync_chassis`] writes only in its own step, so the new thrust met
    /// the old mass once - a kick of g/64 times the mass ratio less one. The
    /// seeded airships span 4.4 times since the fan-out (#1658-#1663), a
    /// 46 kg floater to a 200 kg steam dirigible, and the kick from the one
    /// to the other was 0.52 m/s: 0.67 m in all. The swap step's change of
    /// speed is read on its own here, so a kick cannot hide in the drift;
    /// with the mass avian holds brought up to date in the swap step, the
    /// swap moves her climb by under 5 mm/s either way, and five seconds on
    /// she is within 2 cm of where she was.
    #[test]
    fn an_airship_re_rolled_in_place_holds_its_height() {
        let mut ships: Vec<(f32, AvatarRecord)> = (0u64..600)
            .filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Airship)
            .map(AvatarRecord::default_for_seed)
            .filter_map(|r| match &r.locomotion {
                LocomotionConfig::Helicopter(p) => Some((p.mass.0, r.clone())),
                _ => None,
            })
            .collect();
        ships.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (light, heavy) = (&ships[0].1, &ships[ships.len() - 1].1);
        assert!(
            ships[ships.len() - 1].0 > ships[0].0 * 1.5,
            "the seeded airships no longer span a mass range worth testing"
        );
        let (m_light, m_heavy) = (ships[0].0, ships[ships.len() - 1].0);
        for ((was, now), (m_was, m_now)) in [
            ((heavy, light), (m_heavy, m_light)),
            ((light, heavy), (m_light, m_heavy)),
        ] {
            let mut bench = FlightBench::new(was, Vec3::new(0.0, 30.0, 0.0)).aloft();
            for _ in 0..64 {
                bench.step();
            }
            let swapped = bench.position().y;
            let held = bench.velocity().y;
            bench
                .world_mut()
                .insert_resource(LiveAvatarRecord(now.clone()));
            bench.step();
            let kick = bench.velocity().y - held;
            assert!(
                kick.abs() < 0.005,
                "re-rolled in place from {m_was:.1} kg to {m_now:.1} kg, the swap step \
                 changed her climb by {kick:+.3} m/s: the new chassis's forces met the old mass"
            );
            for _ in 0..(64 * 4 - 1) {
                bench.step();
            }
            let fourth = bench.position().y;
            for _ in 0..64 {
                bench.step();
            }
            let (fifth, all) = (bench.position().y - fourth, bench.position().y - swapped);
            assert!(
                fifth.abs() < 0.002 && all.abs() < 0.02,
                "an airship re-rolled in place from {m_was:.1} kg to {m_now:.1} kg moved \
                 {all:+.2} m in five seconds, {fifth:+.3} m of it in the fifth: it is not \
                 holding its height"
            );
        }
    }

    /// What [`sync_chassis`] leaves avian in the swap step is what avian's
    /// own update makes of the new body (#1666): its mass, its inertia and
    /// its centre of mass, read between the flight systems and avian's step
    /// and again after it, both ways between the lightest and the heaviest
    /// seeded airship. A difference would be a second kick, in the step
    /// after the swap's.
    #[test]
    fn a_re_rolled_chassis_holds_the_mass_avian_gives_it() {
        let mut ships: Vec<(f32, AvatarRecord)> = (0u64..600)
            .filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Airship)
            .map(AvatarRecord::default_for_seed)
            .filter_map(|r| match &r.locomotion {
                LocomotionConfig::Helicopter(p) => Some((p.mass.0, r.clone())),
                _ => None,
            })
            .collect();
        ships.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (light, heavy) = (&ships[0].1, &ships[ships.len() - 1].1);
        let held = |world: &mut World| {
            let mut bodies = world.query_filtered::<(
                &ComputedMass,
                &ComputedAngularInertia,
                &ComputedCenterOfMass,
            ), With<LocalPlayer>>();
            let (mass, inertia, centre) = bodies.single(world).expect("one body on the bench");
            (*mass, *inertia, *centre)
        };
        for (was, now) in [(heavy, light), (light, heavy)] {
            let mut bench = FlightBench::new(was, Vec3::new(0.0, 30.0, 0.0)).aloft();
            for _ in 0..64 {
                bench.step();
            }
            let before = held(bench.world_mut());
            bench
                .world_mut()
                .insert_resource(LiveAvatarRecord(now.clone()));
            let mut left = None;
            bench.step_reading(|world| left = Some(held(world)));
            let left = left.expect("read between the systems and avian's step");
            let avians = held(bench.world_mut());
            assert_ne!(left.0, before.0, "the swap left avian the old body's mass");
            assert_eq!(
                left, avians,
                "the swap left avian one body and avian made another of it"
            );
        }
    }

    /// And on foot: a rigged person is hung half the RECORD's capsule below
    /// the chassis, but stood on the capsule it was built with, so a person
    /// re-rolled taller stood in the ground and one re-rolled shorter
    /// floated. The kept body now stands exactly where a body built for the
    /// new record does.
    #[test]
    fn a_person_re_rolled_in_place_stands_where_a_rebuilt_one_does() {
        let mut people: Vec<(f32, AvatarRecord)> = (0u64..600)
            .filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Humanoid)
            .map(AvatarRecord::default_for_seed)
            .filter_map(|r| match &r.locomotion {
                LocomotionConfig::Humanoid(p) => Some((p.total_height(), r.clone())),
                _ => None,
            })
            .collect();
        people.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (short, tall) = (&people[0], &people[people.len() - 1]);
        assert!(
            tall.0 - short.0 > 0.1,
            "the seeded people no longer span statures worth testing"
        );
        let stands = |record: &AvatarRecord| {
            let mut bench = FlightBench::new(record, Vec3::new(0.0, 3.0, 0.0));
            for _ in 0..(64 * 3) {
                bench.step();
            }
            bench
        };
        for (was, now) in [(short, tall), (tall, short)] {
            let mut kept = stands(&was.1);
            kept.world_mut()
                .insert_resource(LiveAvatarRecord(now.1.clone()));
            for _ in 0..(64 * 3) {
                kept.step();
            }
            let want = stands(&now.1).position().y;
            let off = kept.position().y - want;
            assert!(
                off.abs() < 1e-3,
                "a person re-rolled from {:.2} m to {:.2} m stands {:+.1} cm off the ground",
                was.0,
                now.0,
                off * 100.0
            );
        }
    }
}
