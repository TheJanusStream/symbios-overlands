//! Benches for the presets that leave the ground, for tests anywhere in the
//! crate: one body wearing a record, over a flat floor whose top is at
//! y = 0, with the game's own systems in `FixedUpdate` as
//! [`super::PlayerPlugin`] schedules them and avian's step after them -
//! stepped by hand, one fixed step at a time, with whatever keys the test
//! holds down.
//!
//! * [`FlightBench`] (#1430) runs the flight systems.
//! * [`DriveBench`] (#1524) runs the car's: suspension, drive, uprighting.
//! * [`run_at_frame_rate`] (#1548) runs every vehicle's systems the way the
//!   game's frames do, at a display's frame rate, rather than step by step.
//!
//! Both keep gravity, unlike the planar drive probe in `spawn`'s tests,
//! which turns it off because a car's feel on the flat does not need it. A
//! flight is nothing but gravity and the thrust that cancels it, and a jump
//! is nothing but gravity.
//!
//! The floor is physics; the ground's height a flight looks ahead at is the
//! game's [`FinishedHeightMap`], flat at 0 until a test lays another
//! ([`FlightBench::ground`]) - as it lays the blocks that stand on it. The
//! car reads only the physics, through its suspension rays.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::pds::avatar::AvatarRecord;
use crate::state::LiveAvatarRecord;
use crate::terrain::FinishedHeightMap;

/// The heightmap a bench lays: this many samples a side, a metre apart, so
/// it covers the world from -256 to 256 m.
const GROUND_SAMPLES: usize = 513;

/// The physics floor, as wide as the heightmap, in cells 8 m a side.
const FLOOR_M: f32 = 512.0;
const FLOOR_CELLS: usize = 64;

/// The game's fixed step: Bevy's default, which the game keeps.
pub(crate) const BENCH_HZ: f64 = 64.0;

/// Every key a flight can hold.
const FLIGHT_KEYS: [KeyCode; 8] = [
    KeyCode::KeyW,
    KeyCode::KeyS,
    KeyCode::KeyA,
    KeyCode::KeyD,
    KeyCode::KeyQ,
    KeyCode::KeyE,
    KeyCode::Space,
    KeyCode::ShiftLeft,
];

/// The car's key table (#1533): every key the car reads, and what it does
/// on the ground and in the air. A drive holds these; and the controls
/// sheet's guard, in `ui::toolbar`, checks the sheet's skiff rows against
/// them.
pub(crate) use super::car::{CAR_KEYS, InTheAir, OnTheGround};

/// The app both benches stand on: the floor, and one body wearing `record`
/// posed at `at`, built through the game's own spawn path, with `systems`
/// in `FixedUpdate` - the real ones, in the order `PlayerPlugin` chains
/// them.
fn bench_app<M>(
    record: &AvatarRecord,
    at: Transform,
    systems: impl IntoScheduleConfigs<bevy::ecs::system::ScheduleSystem, M>,
) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(PhysicsPlugins::default())
        .insert_resource(Time::<Fixed>::from_hz(BENCH_HZ))
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(LiveAvatarRecord(record.clone()))
        .add_systems(FixedUpdate, systems);
    let body = furnish(&mut app, record, at);
    // `App::update` would do these on its first run; the bench drives
    // the schedules by hand, and some of avian's resources land in them.
    app.finish();
    app.cleanup();
    (app, body)
}

/// The floor, and one body wearing `record` posed at `at`, built through
/// the game's own spawn path.
fn furnish(app: &mut App, record: &AvatarRecord, at: Transform) -> Entity {
    // The floor is a heightfield, built by the builder the game's terrain
    // uses (with parry's internal-edge fix, #1538), and of the heightmap's
    // size: against one box thousands of metres across a shape cast stops
    // centimetres short of it. The builder spans what the mesh does, a
    // map's samples less one times its scale, so FLOOR_CELLS + 1 samples
    // FLOOR_M / FLOOR_CELLS apart span FLOOR_M.
    let floor =
        bevy_symbios_ground::build_heightfield_collider(&bevy_symbios_ground::HeightMap::new(
            FLOOR_CELLS + 1,
            FLOOR_CELLS + 1,
            FLOOR_M / FLOOR_CELLS as f32,
        ));
    // The flight benches fly out past the floor's edge, so its size steers
    // them: 8 m wider, one came round again and did not land in time.
    let spans = floor.shape().as_heightfield().map(|field| field.scale());
    assert_eq!(
        spans,
        Some(Vec3::new(FLOOR_M, 1.0, FLOOR_M)),
        "the bench floor is not {FLOOR_M} m across: has the builder's extent changed?"
    );
    app.world_mut()
        .spawn((RigidBody::Static, floor, Transform::IDENTITY));
    let body = app
        .world_mut()
        .spawn(super::spawn::chassis_root_bundle(at))
        .id();
    let mut commands = app.world_mut().commands();
    super::preset::build_preset_components(&mut commands, body, &record.locomotion);
    app.world_mut().flush();
    body
}

/// The game's heightmap with the ground at `height` at each point (x, z).
fn heightmap(height: impl Fn(Vec2) -> f32) -> FinishedHeightMap {
    let half = (GROUND_SAMPLES - 1) as f32 * 0.5;
    let mut heightmap = bevy_symbios_ground::HeightMap::new(GROUND_SAMPLES, GROUND_SAMPLES, 1.0);
    for z in 0..GROUND_SAMPLES {
        for x in 0..GROUND_SAMPLES {
            heightmap.set(x, z, height(Vec2::new(x as f32 - half, z as f32 - half)));
        }
    }
    FinishedHeightMap(heightmap, None)
}

/// A fixed block, `half` its half-extents, placed by `at`.
fn spawn_block(app: &mut App, at: Transform, half: Vec3) {
    app.world_mut().spawn((
        RigidBody::Static,
        Collider::cuboid(half.x * 2.0, half.y * 2.0, half.z * 2.0),
        at,
    ));
    // The spawn lands in avian's collider trees on its next step.
}

/// Hold `keys` down, and let go of every other key in `every`.
fn hold_keys(app: &mut App, keys: &[KeyCode], every: &[KeyCode]) {
    let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    for key in every {
        if keys.contains(key) {
            input.press(*key);
        } else {
            input.release(*key);
        }
    }
}

/// One fixed step of the game's loop: the systems, then avian's step,
/// with the generic `Time` set to the fixed clock as Bevy's `FixedMain`
/// sets it - which is where avian reads its timestep from.
fn fixed_step(app: &mut App) {
    let dt = std::time::Duration::from_secs_f64(1.0 / BENCH_HZ);
    let world = app.world_mut();
    world.resource_mut::<Time<Fixed>>().advance_by(dt);
    let fixed = *world.resource::<Time<Fixed>>();
    *world.resource_mut::<Time>() = fixed.as_generic();
    world.run_schedule(FixedUpdate);
    world.run_schedule(FixedPostUpdate);
}

pub(crate) struct FlightBench {
    app: App,
    body: Entity,
    steps: u64,
}

impl FlightBench {
    /// A body wearing `record`, its origin at `at`, facing -Z.
    pub(crate) fn new(record: &AvatarRecord, at: Vec3) -> Self {
        // THE REAL SYSTEMS, in the order `PlayerPlugin` chains them. Each
        // stands down for a body that is not its preset.
        let (app, body) = bench_app(
            record,
            Transform::from_translation(at),
            (
                super::helicopter::apply_helicopter_stabilization,
                super::airplane::apply_airplane_aerodynamics,
                super::airplane::apply_airplane_uprighting,
                super::airplane::apply_airplane_forces,
                super::helicopter::apply_helicopter_forces,
            )
                .chain(),
        );
        let mut bench = Self {
            app,
            body,
            steps: 0,
        };
        bench.ground(|_| 0.0);
        bench
    }

    /// Lay the ground's height, as the game's heightmap has it, from
    /// `height` at each point (x, z). The physics floor stays where it is:
    /// blocks are what a body can touch.
    pub(crate) fn ground(&mut self, height: impl Fn(Vec2) -> f32) {
        self.app.world_mut().insert_resource(heightmap(height));
    }

    /// A fixed block, `half` its half-extents, placed by `at`: a wall, a
    /// roof over the body, or - turned - a slope.
    pub(crate) fn block(&mut self, at: Transform, half: Vec3) {
        spawn_block(&mut self.app, at, half);
    }

    /// Set the body moving at `velocity`, as if it had been flying.
    pub(crate) fn set_velocity(&mut self, velocity: Vec3) {
        let body = self.body;
        if let Some(mut moving) = self.app.world_mut().get_mut::<LinearVelocity>(body) {
            moving.0 = velocity;
        }
    }

    /// Hold `keys` down, and let go of every other flight key.
    pub(crate) fn hold(&mut self, keys: &[KeyCode]) {
        hold_keys(&mut self.app, keys, &FLIGHT_KEYS);
    }

    /// One fixed step of the game's loop: the flight systems, then avian's
    /// step.
    pub(crate) fn step(&mut self) {
        fixed_step(&mut self.app);
        self.steps += 1;
    }

    /// The bench's world, for what a test reads of it the way the game
    /// does - a system param over it, say - or runs in it.
    pub(crate) fn world_mut(&mut self) -> &mut World {
        self.app.world_mut()
    }

    /// Seconds of physics stepped so far.
    pub(crate) fn elapsed(&self) -> f64 {
        self.steps as f64 / BENCH_HZ
    }

    pub(crate) fn position(&self) -> Vec3 {
        self.component::<Position>().0
    }

    pub(crate) fn rotation(&self) -> Quat {
        self.component::<Rotation>().0
    }

    /// The lowest point of the body's collider, as it stands.
    pub(crate) fn underside(&self) -> f32 {
        self.component::<Collider>()
            .aabb(self.position(), self.rotation())
            .min
            .y
    }

    fn component<C: Component>(&self) -> &C {
        self.app
            .world()
            .get::<C>(self.body)
            .expect("the bench body has every physics component")
    }
}

/// The car's bench (#1524): the flight bench's floor and body, driven by the
/// car's own three systems - suspension, drive, uprighting - in the order
/// `PlayerPlugin` chains them, with gravity on.
///
/// Unlike [`FlightBench`] it lays no heightmap: the car reads the ground
/// only through its suspension rays, so the physics floor and the blocks a
/// test places on it are all the ground it has - a ramp is a tilted
/// [`DriveBench::block`].
pub(crate) struct DriveBench {
    app: App,
    body: Entity,
    steps: u64,
}

/// Whether the bench's player is typing into a text field, which is what
/// stands the drive down in the game: `PlayerPlugin` gates
/// `apply_car_drive` on egui keyboard focus (and on two modal holds that
/// work the same way), and leaves the suspension and the uprighting to run.
#[derive(Resource, Default)]
struct Typing(bool);

fn not_typing(typing: Res<Typing>) -> bool {
    !typing.0
}

impl DriveBench {
    /// A body wearing `record`, posed by `at`.
    pub(crate) fn new(record: &AvatarRecord, at: Transform) -> Self {
        let (mut app, body) = bench_app(
            record,
            at,
            (
                super::car::apply_car_suspension,
                super::car::apply_car_drive.run_if(not_typing),
                super::car::apply_car_uprighting,
            )
                .chain(),
        );
        app.init_resource::<Typing>();
        Self {
            app,
            body,
            steps: 0,
        }
    }

    /// Run the car's suspension with `stop` for its bump stop - see
    /// `CarBumpStop::OFF` - in place of the game's.
    pub(crate) fn bump_stop(&mut self, stop: super::car::CarBumpStop) {
        self.app.world_mut().insert_resource(stop);
    }

    /// Put the player's focus in a text field, or take it out: the drive
    /// stands down as it does in the game, and nothing else does.
    pub(crate) fn typing(&mut self, typing: bool) {
        self.app.world_mut().resource_mut::<Typing>().0 = typing;
    }

    /// A car wearing `record` standing still on the floor at `(x, z)`,
    /// facing -Z: dropped from where its springs will hold it and given
    /// half a second to settle.
    pub(crate) fn parked(record: &AvatarRecord, x: f32, z: f32) -> Self {
        let crate::pds::LocomotionConfig::Car(p) = &record.locomotion else {
            panic!("the drive bench is for the car preset");
        };
        // The spring carries a quarter of the weight at each corner.
        let sag = p.mass.0 * 9.81 / (4.0 * p.suspension_stiffness.0.max(1.0));
        let ride = p.chassis_half_extents.0[1] + p.suspension_rest_length.0 - sag;
        let mut bench = Self::new(record, Transform::from_xyz(x, ride, z));
        bench.run(0.5);
        bench
    }

    /// A fixed block, `half` its half-extents, placed by `at`: a wall, or -
    /// turned - a ramp.
    pub(crate) fn block(&mut self, at: Transform, half: Vec3) {
        spawn_block(&mut self.app, at, half);
    }

    /// Set the body moving at `velocity`.
    pub(crate) fn set_velocity(&mut self, velocity: Vec3) {
        let body = self.body;
        if let Some(mut moving) = self.app.world_mut().get_mut::<LinearVelocity>(body) {
            moving.0 = velocity;
        }
    }

    /// Set the body turning at `spin` (rad/s, world axes).
    pub(crate) fn set_angular_velocity(&mut self, spin: Vec3) {
        let body = self.body;
        if let Some(mut turning) = self.app.world_mut().get_mut::<AngularVelocity>(body) {
            turning.0 = spin;
        }
    }

    /// Hold `keys` down, and let go of every other key the car reads.
    pub(crate) fn hold(&mut self, keys: &[KeyCode]) {
        hold_keys(&mut self.app, keys, &CAR_KEYS.map(|car_key| car_key.key));
    }

    /// One fixed step of the game's loop: the car's systems, then avian's
    /// step.
    pub(crate) fn step(&mut self) {
        fixed_step(&mut self.app);
        self.steps += 1;
    }

    /// Step for `secs` seconds with the keys as they are held.
    pub(crate) fn run(&mut self, secs: f64) {
        for _ in 0..(secs * BENCH_HZ).round() as usize {
            self.step();
        }
    }

    /// The bench's world, as [`FlightBench::world_mut`] gives its own: for
    /// what a test reads of it the way the game does, or runs in it - the
    /// agent's steering, say (#1531).
    pub(crate) fn world_mut(&mut self) -> &mut World {
        self.app.world_mut()
    }

    /// Seconds of physics stepped so far.
    pub(crate) fn elapsed(&self) -> f64 {
        self.steps as f64 / BENCH_HZ
    }

    pub(crate) fn position(&self) -> Vec3 {
        self.component::<Position>().0
    }

    pub(crate) fn rotation(&self) -> Quat {
        self.component::<Rotation>().0
    }

    pub(crate) fn velocity(&self) -> Vec3 {
        self.component::<LinearVelocity>().0
    }

    pub(crate) fn angular_velocity(&self) -> Vec3 {
        self.component::<AngularVelocity>().0
    }

    /// The lowest point of the chassis box, as it stands.
    pub(crate) fn underside(&self) -> f32 {
        self.component::<Collider>()
            .aabb(self.position(), self.rotation())
            .min
            .y
    }

    /// What the suspension rays found on the last step.
    pub(crate) fn contact(&self) -> super::car::CarContact {
        *self.component::<super::car::CarContact>()
    }

    /// The chassis' linear and angular damping as avian will apply them.
    pub(crate) fn damping(&self) -> (f32, f32) {
        (
            self.component::<LinearDamping>().0,
            self.component::<AngularDamping>().0,
        )
    }

    fn component<C: Component>(&self) -> &C {
        self.app
            .world()
            .get::<C>(self.body)
            .expect("the bench car has every physics component")
    }
}

/// The poses [`run_at_frame_rate`] records, one after each fixed step.
#[derive(Resource, Default)]
struct StepPoses(Vec<(Vec3, Quat)>);

fn record_step_pose(
    bodies: Query<(&Position, &Rotation), With<crate::state::LocalPlayer>>,
    mut poses: ResMut<StepPoses>,
) {
    if let Ok((position, rotation)) = bodies.single() {
        poses.0.push((position.0, rotation.0));
    }
}

/// One body wearing `record`, posed by `at` over the benches' floor - under
/// water up to `water` when given - with `keys` held, run as the game runs
/// it (#1548): in frames of `1 / fps`
/// seconds, each taking as many fixed steps as the frame's time holds - none
/// at all, or several - with the chassis eased between steps and the
/// scene's transforms propagated after them, as `PostUpdate` does. Every
/// preset's fixed-step systems run, in the order `PlayerPlugin` chains them
/// (each stands down for a body that is not its preset). The pose after
/// each of the first `steps` fixed steps.
///
/// The benches above step the physics by hand and propagate nothing, so a
/// system that read the pose a frame leaves - `GlobalTransform` - instead of
/// the physics' own read it one step late, always, as at a frame rate of 64
/// or a whole fraction of it; only frames of another length show the rest.
pub(crate) fn run_at_frame_rate(
    record: &AvatarRecord,
    at: Transform,
    water: Option<f32>,
    keys: &[KeyCode],
    fps: f64,
    steps: usize,
) -> Vec<(Vec3, Quat)> {
    let mut app = App::new();
    // Whole frames run avian's housekeeping too, which reads mesh events.
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        bevy::asset::AssetPlugin::default(),
    ))
    .init_asset::<Mesh>()
    .add_plugins(PhysicsPlugins::default())
    .insert_resource(Time::<Fixed>::from_hz(BENCH_HZ))
    .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
        std::time::Duration::from_secs_f64(1.0 / fps),
    ))
    .init_resource::<ButtonInput<KeyCode>>()
    .insert_resource(crate::water::WaterSurfaces {
        planes: water
            .map(|y| crate::water::WaterPlane {
                world_from_local: Transform::from_xyz(0.0, y, 0.0),
                local_half_extents: Vec2::splat(FLOOR_M * 0.5),
                flow_strength: 0.0,
                owner: crate::water::WaterPlane::NO_OWNER,
            })
            .into_iter()
            .collect(),
    })
    .init_resource::<super::humanoid::JumpQueued>()
    .init_resource::<super::RigHold>()
    .init_resource::<StepPoses>()
    .insert_resource(LiveAvatarRecord(record.clone()))
    .insert_resource(heightmap(|_| 0.0))
    .add_systems(
        FixedUpdate,
        (
            super::hover_boat::sync_hover_boat_physics,
            super::hover_boat::apply_hover_boat_suspension,
            super::hover_boat::apply_hover_boat_buoyancy,
            super::helicopter::apply_helicopter_stabilization,
            super::airplane::apply_airplane_aerodynamics,
            super::airplane::apply_airplane_uprighting,
            super::hover_boat::apply_hover_boat_drive,
            super::hover_boat::apply_hover_boat_uprighting,
            super::humanoid::apply_humanoid_walk,
            super::humanoid::clear_jump_queue,
            super::airplane::apply_airplane_forces,
            super::helicopter::apply_helicopter_forces,
            super::car::apply_car_suspension,
            super::car::apply_car_drive,
            super::car::apply_car_uprighting,
        )
            .chain(),
    )
    .add_systems(FixedLast, record_step_pose);
    furnish(&mut app, record, at);
    app.finish();
    app.cleanup();
    {
        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        for key in keys {
            input.press(*key);
        }
    }
    while app.world().resource::<StepPoses>().0.len() < steps {
        app.update();
    }
    let mut poses = std::mem::take(&mut app.world_mut().resource_mut::<StepPoses>().0);
    poses.truncate(steps);
    poses
}

#[cfg(test)]
mod frame_rate_tests {
    use super::*;
    use crate::pds::LocomotionConfig;

    /// Three seconds of the game's fixed steps.
    const STEPS: usize = 192;

    /// A record wearing `locomotion`'s preset at its defaults.
    fn wearing(locomotion: LocomotionConfig) -> AvatarRecord {
        let mut record = AvatarRecord::default_for_did("did:plc:frame-rate-bench");
        record.locomotion = locomotion;
        record
    }

    /// #1548: a body does the same at any frame rate. Its fixed-step
    /// systems read the pose the last frame left - `GlobalTransform`, set by
    /// avian only before its solve and eased by `PostUpdate` - instead of
    /// the physics' own, so a step saw the body a whole step late or part
    /// of one, depending on how many steps the frame held and where it fell
    /// on the step grid: two players on different displays flew the same
    /// jump differently. Each body, with its keys held, must go through the
    /// same poses step for step and to the bit at 12.5, 30, 50 and 144
    /// frames a second as at 64 - each vehicle driven, and the systems no
    /// drive reaches: a car on its side righting itself, a hover-boat
    /// floating, a tilted helicopter levelling, a swimmer at the surface.
    #[test]
    fn a_body_does_the_same_at_any_frame_rate() {
        use std::f32::consts::FRAC_PI_2;
        let car = wearing(LocomotionConfig::Car(Box::default()));
        let car_half = match &car.locomotion {
            LocomotionConfig::Car(p) => p.chassis_half_extents.0,
            _ => unreachable!("a car"),
        };
        let bodies = [
            (
                "car",
                car.clone(),
                Transform::from_xyz(0.0, 1.2, 0.0),
                None,
                vec![KeyCode::KeyW, KeyCode::KeyD],
            ),
            (
                "car on its side",
                car,
                Transform::from_xyz(0.0, car_half[0] + 0.01, 0.0)
                    .with_rotation(Quat::from_rotation_z(FRAC_PI_2)),
                None,
                vec![],
            ),
            (
                "hover-boat",
                wearing(LocomotionConfig::HoverBoat(Box::default())),
                Transform::from_xyz(0.0, 1.5, 0.0),
                None,
                vec![KeyCode::KeyW, KeyCode::KeyD],
            ),
            (
                "hover-boat afloat",
                wearing(LocomotionConfig::HoverBoat(Box::default())),
                Transform::from_xyz(0.0, 2.6, 0.0),
                Some(2.0),
                vec![KeyCode::KeyW, KeyCode::KeyD],
            ),
            (
                "helicopter",
                wearing(LocomotionConfig::Helicopter(Box::default())),
                Transform::from_xyz(0.0, 12.0, 0.0),
                None,
                vec![KeyCode::KeyW, KeyCode::KeyD],
            ),
            (
                "tilted helicopter",
                wearing(LocomotionConfig::Helicopter(Box::default())),
                Transform::from_xyz(0.0, 12.0, 0.0)
                    .with_rotation(Quat::from_rotation_x(0.35) * Quat::from_rotation_z(0.25)),
                None,
                vec![],
            ),
            (
                "airplane",
                wearing(LocomotionConfig::Airplane(Box::default())),
                Transform::from_xyz(0.0, 0.5, 0.0),
                None,
                vec![KeyCode::KeyW],
            ),
            (
                "swimmer",
                wearing(LocomotionConfig::Humanoid(Box::default())),
                Transform::from_xyz(0.0, 1.6, 0.0),
                Some(2.0),
                vec![KeyCode::KeyW],
            ),
        ];
        for (name, record, at, water, keys) in &bodies {
            let locked = run_at_frame_rate(record, *at, *water, keys, 64.0, STEPS);
            let (first, last) = (locked[0], locked[STEPS - 1]);
            let (moved, turned) = (
                first.0.distance(last.0),
                first.1.angle_between(last.1).to_degrees(),
            );
            assert!(
                moved > 0.5 || turned > 20.0,
                "fixture: the {name} went {moved:.2} m and turned {turned:.1} deg - a body that \
                 does nothing shows nothing"
            );
            for fps in [12.5, 30.0, 50.0, 144.0] {
                let other = run_at_frame_rate(record, *at, *water, keys, fps, STEPS);
                if let Some(step) = (0..STEPS).find(|&i| other[i] != locked[i]) {
                    panic!(
                        "the {name} at {fps} frames a second leaves the run at 64 at step {step}: \
                         {:?} against {:?}, and after {STEPS} steps is {:.4} m and {:.3} deg away",
                        other[step],
                        locked[step],
                        other[STEPS - 1].0.distance(locked[STEPS - 1].0),
                        other[STEPS - 1]
                            .1
                            .angle_between(locked[STEPS - 1].1)
                            .to_degrees(),
                    );
                }
            }
        }
    }
}
