//! A bench for the flying presets (#1430), for tests anywhere in the crate:
//! one body wearing a record, over a flat floor whose top is at y = 0, with
//! the game's own flight systems in `FixedUpdate` as [`super::PlayerPlugin`]
//! schedules them and avian's step after them - stepped by hand, one fixed
//! step at a time, with whatever keys the test holds down.
//!
//! The same shape as the planar drive bench in `spawn`'s tests, which turns
//! gravity off because a car's feel on the flat does not need it. A flight
//! is nothing but gravity and the thrust that cancels it, so this bench
//! keeps both.
//!
//! The floor is physics; the ground's height a flight looks ahead at is the
//! game's [`FinishedHeightMap`], flat at 0 until a test lays another
//! ([`FlightBench::ground`]) - as it lays the blocks that stand on it.

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

pub(crate) struct FlightBench {
    app: App,
    body: Entity,
    steps: u64,
}

impl FlightBench {
    /// A body wearing `record`, its origin at `at`, facing -Z.
    pub(crate) fn new(record: &AvatarRecord, at: Vec3) -> Self {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(PhysicsPlugins::default())
            .insert_resource(Time::<Fixed>::from_hz(BENCH_HZ))
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(LiveAvatarRecord(record.clone()))
            // THE REAL SYSTEMS, in the order `PlayerPlugin` chains them. Each
            // stands down for a body that is not its preset.
            .add_systems(
                FixedUpdate,
                (
                    super::helicopter::apply_helicopter_stabilization,
                    super::airplane::apply_airplane_aerodynamics,
                    super::airplane::apply_airplane_uprighting,
                    super::airplane::apply_airplane_forces,
                    super::helicopter::apply_helicopter_forces,
                )
                    .chain(),
            );
        // The floor is a heightfield, as the game's terrain is, and of the
        // heightmap's size: against one box thousands of metres across a
        // shape cast stops centimetres short of it.
        let floor = vec![vec![0.0; FLOOR_CELLS + 1]; FLOOR_CELLS + 1];
        app.world_mut().spawn((
            RigidBody::Static,
            Collider::heightfield(floor, Vec3::new(FLOOR_M, 1.0, FLOOR_M)),
            Transform::IDENTITY,
        ));
        let body = app
            .world_mut()
            .spawn(super::spawn::chassis_root_bundle(
                Transform::from_translation(at),
            ))
            .id();
        let mut commands = app.world_mut().commands();
        super::preset::build_preset_components(&mut commands, body, &record.locomotion);
        app.world_mut().flush();
        // `App::update` would do these on its first run; the bench drives
        // the schedules by hand, and some of avian's resources land in them.
        app.finish();
        app.cleanup();
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
        self.app.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(half.x * 2.0, half.y * 2.0, half.z * 2.0),
            at,
        ));
        // The spawn lands in avian's collider trees on its next step.
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
        let mut input = self.app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        for key in FLIGHT_KEYS {
            if keys.contains(&key) {
                input.press(key);
            } else {
                input.release(key);
            }
        }
    }

    /// One fixed step of the game's loop: the flight systems, then avian's
    /// step, with the generic `Time` set to the fixed clock as Bevy's
    /// `FixedMain` sets it - which is where avian reads its timestep from.
    pub(crate) fn step(&mut self) {
        let dt = std::time::Duration::from_secs_f64(1.0 / BENCH_HZ);
        let world = self.app.world_mut();
        world.resource_mut::<Time<Fixed>>().advance_by(dt);
        let fixed = *world.resource::<Time<Fixed>>();
        *world.resource_mut::<Time>() = fixed.as_generic();
        world.run_schedule(FixedUpdate);
        world.run_schedule(FixedPostUpdate);
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
