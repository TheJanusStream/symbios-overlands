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

/// Marks the player as using the Humanoid preset. Brings the controller's
/// water memory with it (#1324), so no humanoid chassis can be classified
/// without one.
#[derive(Component)]
#[require(super::humanoid::HumanoidWater)]
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
}

/// A chassis' collider, as its record sizes it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ChassisShape {
    /// A vehicle's box, by its half-extents (m).
    Box([f32; 3]),
    /// A person's capsule: its radius and cylindrical length (m).
    Capsule { radius: f32, length: f32 },
}

impl ChassisShape {
    fn collider(self) -> Collider {
        match self {
            Self::Box(half) => Collider::cuboid(half[0] * 2.0, half[1] * 2.0, half[2] * 2.0),
            Self::Capsule { radius, length } => Collider::capsule(radius, length),
        }
    }

    /// How far the shape reaches below its body's origin (m).
    fn half_height(self) -> f32 {
        match self {
            Self::Box(half) => half[1],
            Self::Capsule { radius, length } => radius + length * 0.5,
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
        ChassisSpec {
            shape: ChassisShape::Capsule {
                radius: self.capsule_radius.0.max(0.05),
                length: self.capsule_length.0.max(0.1),
            },
            mass: self.mass.0,
            linear_damping: self.linear_damping.0,
            angular_damping: cfg::ANGULAR_DAMPING,
            center_of_mass: None,
            steps_own_damping: false,
        }
    }

    fn insert_preset(&self, commands: &mut Commands, entity: Entity) {
        commands.entity(entity).insert((
            // Traditional character controller: lock all three rotation
            // axes so the physics capsule slides without spinning. The
            // walk controller rotates the chassis transform itself to
            // face the movement direction.
            LockedAxes::new()
                .lock_rotation_x()
                .lock_rotation_y()
                .lock_rotation_z(),
            HumanoidPreset,
        ));
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
    mut player: Query<
        (
            Entity,
            &mut ChassisBuilt,
            &mut Collider,
            &mut Mass,
            &mut LinearDamping,
            &mut AngularDamping,
            Option<&mut CenterOfMass>,
        ),
        (
            With<crate::state::LocalPlayer>,
            Without<super::VisualsEditFreeze>,
        ),
    >,
) {
    let locomotion = &live.0.locomotion;
    let Some(spec) = chassis_spec(locomotion) else {
        return;
    };
    let Ok((body, mut built, mut collider, mut mass, mut linear, mut angular, center)) =
        player.single_mut()
    else {
        return;
    };
    if built.kind != locomotion.kind_tag() || built.spec == spec {
        return;
    }
    // Silenced as avian queues it itself: a body outside the simulation
    // (no island) has nothing to wake.
    commands.queue_silenced(WakeBody(body));
    let was = built.spec;
    if spec.shape != was.shape {
        *collider = spec.shape.collider();
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
    built.spec = spec;
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
                LocomotionConfig::Humanoid(Box::new(person)),
                LocomotionConfig::Humanoid(Box::new(person2)),
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
            ChassisShape::Capsule {
                radius: capsule.radius,
                length: capsule.half_height() * 2.0,
            }
        } else {
            panic!("a chassis is a box or a capsule")
        }
    }

    /// What a body is, as far as its chassis goes: shape, mass, dampings,
    /// centre of mass and stamp.
    fn read(
        world: &World,
        body: Entity,
    ) -> (ChassisShape, f32, [f32; 2], Option<Vec3>, ChassisBuilt) {
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
            let (shape, mass, damping, center, built) = read(&kept, body);
            let want = read(&rebuilt, fresh);
            assert_ne!(before.0, want.0, "{kind}: the moved record moves the shape");
            assert_eq!(shape, want.0, "{kind}: the collider");
            assert_eq!(mass, want.1, "{kind}: the mass");
            assert_eq!(center, want.3, "{kind}: the centre of mass");
            assert_eq!(built, want.4, "{kind}: the stamp");
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
                ]
            };
            let settled = ticks(&kept);
            kept.run_system_once(sync_chassis).expect("the sync runs");
            assert_eq!(ticks(&kept), settled, "{kind}: an idle sync wrote");
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
                ChassisFamily::Humanoid | ChassisFamily::Airplane => {
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
            (kept.0, kept.1, kept.3, kept.4),
            (want.0, want.1, want.3, want.4),
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
    /// heavier climbed 54, still gathering speed. Now it settles and holds.
    ///
    /// Not to the millimetre: on the step of the swap the new thrust meets
    /// the mass avian computed the step before - `apply_force` divides by
    /// the mass the body holds when it is called, and avian takes up the
    /// one [`sync_chassis`] writes only in its own step - a one-step kick
    /// of g/64 times the mass ratio less one, which her damping takes out
    /// over a few seconds. The Zeppelin's seeds alone spanned 1.6 times, a
    /// kick under 0.2 m/s and 10 to 20 cm in all; since the airship fan-out
    /// (#1658-#1663) the seeded airships span 4.4 times, a 46 kg floater to
    /// a 200 kg steam dirigible, and the kick from the one to the other is
    /// 0.52 m/s: 0.67 m in all, 1.8 cm of it in the fifth second. The
    /// bounds leave a feel tweak room and sit far under the defect's metres
    /// a second and growing.
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
            bench
                .world_mut()
                .insert_resource(LiveAvatarRecord(now.clone()));
            for _ in 0..(64 * 4) {
                bench.step();
            }
            let fourth = bench.position().y;
            for _ in 0..64 {
                bench.step();
            }
            let (fifth, all) = (bench.position().y - fourth, bench.position().y - swapped);
            assert!(
                fifth.abs() < 0.04 && all.abs() < 1.0,
                "an airship re-rolled in place from {m_was:.1} kg to {m_now:.1} kg moved \
                 {all:+.2} m in five seconds, {fifth:+.3} m of it in the fifth: it is not \
                 holding its height"
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
