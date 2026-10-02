//! Benches for the presets that leave the ground, for tests anywhere in the
//! crate: one body wearing a record, over a flat floor whose top is at
//! y = 0, with the game's own systems in `FixedUpdate` as
//! [`super::PlayerPlugin`] schedules them and avian's step after them -
//! stepped by hand, one fixed step at a time, with whatever keys the test
//! holds down.
//!
//! * [`FlightBench`] (#1430) runs the flight systems.
//! * [`DriveBench`] (#1524) runs the car's: suspension, drive, uprighting.
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
    // The floor is a heightfield, built as the game's terrain is (with
    // parry's internal-edge fix, #1538), and of the heightmap's size:
    // against one box thousands of metres across a shape cast stops
    // centimetres short of it.
    let floor = vec![vec![0.0; FLOOR_CELLS + 1]; FLOOR_CELLS + 1];
    app.world_mut().spawn((
        RigidBody::Static,
        crate::terrain::heightfield_collider(floor, Vec3::new(FLOOR_M, 1.0, FLOOR_M)),
        Transform::IDENTITY,
    ));
    let body = app
        .world_mut()
        .spawn(super::spawn::chassis_root_bundle(at))
        .id();
    let mut commands = app.world_mut().commands();
    super::preset::build_preset_components(&mut commands, body, &record.locomotion);
    app.world_mut().flush();
    // `App::update` would do these on its first run; the bench drives
    // the schedules by hand, and some of avian's resources land in them.
    app.finish();
    app.cleanup();
    (app, body)
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
        let half = (GROUND_SAMPLES - 1) as f32 * 0.5;
        let mut heightmap =
            bevy_symbios_ground::HeightMap::new(GROUND_SAMPLES, GROUND_SAMPLES, 1.0);
        for z in 0..GROUND_SAMPLES {
            for x in 0..GROUND_SAMPLES {
                heightmap.set(x, z, height(Vec2::new(x as f32 - half, z as f32 - half)));
            }
        }
        self.app
            .world_mut()
            .insert_resource(FinishedHeightMap(heightmap));
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
