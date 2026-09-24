//! Local-avatar spawn: the `OnEnter(InGame)` chassis + preset + visuals
//! assembly, and the shared chassis-root bundle the #670 easing guard
//! tests against.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::boot_params::TargetPos;
use crate::config::rover as cfg;
use crate::state::{LiveAvatarRecord, LiveRoomRecord, LocalPlayer, PendingSpawnPlacement};

use super::preset::build_preset_components;
use super::{random_spawn_xz, visuals};

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_local_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    hm_res: Res<crate::terrain::FinishedHeightMap>,
    live: Res<LiveAvatarRecord>,
    placement: Option<Res<PendingSpawnPlacement>>,
    room: Option<Res<LiveRoomRecord>>,
    mut avatar_deps: visuals::AvatarSpawnDeps,
) {
    let hm = &hm_res.0;
    let extent = (hm.width() - 1) as f32 * hm.scale();
    let half = extent * 0.5;
    let centre = half;

    // Spawn-pose precedence (#745): an explicit URL/CLI placement wins
    // wholesale; otherwise the room record's owner-configured default
    // landing; otherwise the legacy random scatter. The landing converts to
    // the same `TargetPos` shape the placement path uses (optional y =
    // drop-pin, height from the heightmap) so the two sources can't drift.
    let (pose_pos, pose_yaw_deg) = match placement.as_deref() {
        Some(p) => (p.pos, p.yaw_deg),
        None => match room.as_deref().and_then(|r| r.0.default_landing) {
            Some(landing) => (
                Some(TargetPos {
                    x: landing.pos.0[0],
                    y: landing.y.map(|y| y.0),
                    z: landing.pos.0[1],
                }),
                Some(landing.yaw_deg.0),
            ),
            None => (None, None),
        },
    };

    // Pick (rx, rz) from the resolved pose when supplied, falling back to
    // the random spawn-scatter. World coordinates are centred on (0, 0); the
    // heightmap sample uses (centre + x, centre + z).
    let (rx, rz) = match pose_pos {
        Some(TargetPos { x, z, .. }) => (x.clamp(-half, half), z.clamp(-half, half)),
        None => random_spawn_xz(),
    };
    let hm_x = (centre + rx).clamp(0.0, extent);
    let hm_z = (centre + rz).clamp(0.0, extent);
    let ground_y = hm.get_height_at(hm_x, hm_z);
    let surface_normal = hm.get_normal_at(hm_x, hm_z);
    let tilt = Quat::from_rotation_arc(Vec3::Y, Vec3::from_array(surface_normal));
    // Apply yaw on top of the surface tilt so a landmark "facing N" lands the
    // chassis aimed at -Z while still resting flush on the slope.
    let yaw = pose_yaw_deg
        .map(|deg| Quat::from_rotation_y(deg.to_radians()))
        .unwrap_or(Quat::IDENTITY);
    let rotation = tilt * yaw;
    // y override (`pos=x,y,z`) bypasses the heightmap sample; the drop-pin
    // form (`pos=x,z`) keeps the heightmap-resolved height.
    let oy = match pose_pos.and_then(|p| p.y) {
        Some(y) => y,
        None => ground_y + cfg::SPAWN_HEIGHT_OFFSET,
    };
    let (ox, oz) = (rx, rz);

    let entity = commands
        .spawn(chassis_root_bundle(
            Transform::from_xyz(ox, oy, oz).with_rotation(rotation),
        ))
        .id();

    // One-shot: remove the resource so a portal travel or fall-respawn
    // later in the session does not retroactively reapply this placement.
    if placement.is_some() {
        commands.remove_resource::<PendingSpawnPlacement>();
    }

    build_preset_components(&mut commands, entity, &live.0.locomotion);
    visuals::spawn_avatar_visuals(
        &mut commands,
        entity,
        &live.0.body,
        None,
        &mut meshes,
        &mut materials,
        &mut images,
        &mut avatar_deps,
        true,
    );
    // What was just painted (#1104), so the first record edit compares
    // against it instead of respawning unconditionally.
    commands
        .entity(entity)
        .insert(super::hotswap::AppliedLocalBody::painted(&live.0.body));
}

/// Preset-independent components of the local chassis root, shared by the
/// live spawn path, the #670 regression test and the test-only flight
/// bench (`player::sim`) so the three can't drift.
///
/// `TransformInterpolation` is load-bearing: Avian steps physics (and the
/// `Position` → `Transform` writeback) entirely inside `FixedPostUpdate`
/// at the 64 Hz fixed timestep, so without easing the chassis `Transform`
/// holds still on tick-less render frames and the own avatar judders at
/// the fixed-vs-refresh beat (~4 Hz on a 60 Hz display) - remote avatars
/// don't, because the network smoother repositions them every render
/// frame (#670). The easing writes the smoothed pose in
/// `RunFixedMainLoop`, before `Update`, so per-frame readers such as the
/// camera follow see it, while `FixedUpdate` systems (drive controllers,
/// transform broadcast) still read true tick poses. Transform writes
/// outside the fixed schedules - portal teleports, the terrain-hot-load
/// lift - are detected as teleports and snap for that timestep, which is
/// the wanted shape.
pub(super) fn chassis_root_bundle(transform: Transform) -> impl Bundle {
    (
        transform,
        Visibility::default(),
        RigidBody::Dynamic,
        TransformInterpolation,
        CollidingEntities::default(),
        LocalPlayer,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::LocomotionConfig;
    use crate::pds::avatar::AvatarRecord;

    /// The #670 guard: the chassis root must opt into transform easing.
    /// Avian writes `Transform` only on 64 Hz fixed ticks, so a chassis
    /// without `TransformInterpolation` visibly steps against the render
    /// rate. `#[require]` on the component chains in the per-axis easing
    /// components, so spawning the bundle in a bare `World` (no plugins)
    /// proves the whole easing state machinery lands on the entity - a
    /// regression that drops the component from `chassis_root_bundle`
    /// (the exact bundle `spawn_local_player` uses) fails here.
    #[test]
    fn chassis_root_opts_into_transform_easing() {
        let mut world = World::new();
        let entity = world.spawn(chassis_root_bundle(Transform::IDENTITY)).id();
        let e = world.entity(entity);
        assert!(
            e.contains::<TransformInterpolation>(),
            "chassis root must carry TransformInterpolation (#670)"
        );
        assert!(
            e.contains::<TranslationInterpolation>() && e.contains::<RotationInterpolation>(),
            "easing per-axis components must be required in by TransformInterpolation"
        );
        assert!(
            e.contains::<RigidBody>() && e.contains::<LocalPlayer>(),
            "bundle must still assemble the physics chassis root"
        );
    }

    // -----------------------------------------------------------------------
    // The drive probe (#1381 phase 1)
    // -----------------------------------------------------------------------

    /// The fixed rate the drive systems run at, as `PlayerPlugin` schedules
    /// them and `spawn_local_player`'s own doc states: 64 Hz.
    const DRIVE_HZ: f64 = 64.0;

    /// What one craft type does under the wheel, as the real drive systems
    /// produce it. Every field is measured, never derived.
    struct DriveCard {
        turn_speed: f32,
        /// How much the yaw rate still moved over the last second of the
        /// turn, as a fraction of the reading. The reading is only a
        /// STEADY turn if this is tiny, and it is what lets the guard use
        /// a shorter window than the card and prove the window was enough.
        yaw_drift: f32,
        /// The same question of the straight line: how much the speed
        /// still moved over its last second.
        speed_drift: f32,
        top_speed: f32,
        to_half: f32,
        to_ninety: f32,
        yaw_rate: f32,
        circle: f32,
        lengths: f32,
        coast_time: f32,
        coast_distance: f32,
        bank_degrees: Option<f32>,
        hull_len: f32,
        mass: f32,
    }

    /// How long the probe holds each phase of the drive, and whether it
    /// measures the coast at all (#1381).
    ///
    /// The card the owner drives by wants everything and can afford it.
    /// The per-type guard runs in the gate, so it wants the least that is
    /// still honest - and "honest" is not a judgement here: `DriveCard`
    /// reports how far the speed and the yaw rate still moved over their
    /// last second, and the guard asserts both are under a tenth of a
    /// percent, so a window that were too short would fail loudly rather
    /// than quietly measure a transient.
    #[derive(Clone, Copy)]
    struct Window {
        straight_secs: f64,
        turn_secs: f64,
        coast: bool,
    }

    impl Window {
        /// The card: far past converged for every type, coast included.
        const FULL: Self = Self {
            straight_secs: 20.0,
            turn_secs: 20.0,
            coast: true,
        };

        /// What the guard needs, and no more.
        ///
        /// Both phases are first-order lags, so what the width has to buy
        /// is a reading that has stopped moving. The guard asserts the
        /// speed and the yaw rate each drift under 0.1% over their last
        /// second, and for a lag at rate `k` that drift is
        /// `(1 - e^-k) e^-k(T-1)`: the slowest linear rate in the fleet is
        /// the wagon's `linear_damping` 0.45, which needs 14.1 s, and the
        /// slowest angular is the rover's `angular_damping` 3.0, which
        /// needs 2.9 s. Rounded up to 16 and 4. The coast is dropped
        /// because no relation the guard asserts reads it.
        ///
        /// MEASURED, one binary, twelve types, after the port: the full
        /// probe is 4.51 s at test-release and this is 2.41 s; under plain
        /// `cargo test --lib`, which CI runs, the guard is 21.4 s
        /// (re-measured at #1382; 21.9 s at #1381). The drift assertions hold
        /// on all twelve at both widths, which is what says the width is
        /// enough rather than merely cheap.
        ///
        /// #1382 LOOKED FOR A TRIM HERE AND FOUND THE COST IS SOMEWHERE ELSE.
        /// Timed type by type in one unoptimised binary, the whole vehicle
        /// guard set is about 155 s on CI and about 130 s of that is
        /// `common::touch::assert_one_machine` - the boats' one-machine sweep
        /// alone is 100.4 s. The BUILDS are nearly free: the sanitiser sweep
        /// covers the same 900 sloops plus five other types in 0.2 s. So
        /// thinning a sweep buys almost nothing and costs coverage, while
        /// making `touch::report` cheaper than its pairwise O(n^2) scan would
        /// buy back most of the gate for nothing - which is #1393's, since it
        /// is already the issue that owns that helper. This 21.4 s is the
        /// second-largest single item and is left as it stands.
        const GUARD: Self = Self {
            straight_secs: 16.0,
            turn_secs: 4.0,
            coast: false,
        };
    }

    /// A headless avian app holding one seeded craft, built through the
    /// game's own spawn path: [`chassis_root_bundle`] plus
    /// [`build_preset_components`], so the collider, the mass, both dampings
    /// and whatever inertia avian derives from them are the game's and not
    /// this test's.
    ///
    /// GRAVITY OFF AND NO GROUND is an honest harness for the planar feel,
    /// which is the whole of what the card reports. Read off both systems in
    /// session 833: neither [`super::hover_boat::apply_hover_boat_drive`] nor
    /// [`super::car::apply_car_drive`] gates on ground contact - each wants a
    /// `LiveAvatarRecord`, a `ButtonInput<KeyCode>`, a `LocalPlayer` with its
    /// preset marker and no `TravelingTo` - so the suspension stays out of
    /// it. And the buoyancy's drag acts along the water's NORMAL only (its
    /// own comment: lateral resistance is the body's linear damping), so a
    /// boat's numbers here are her numbers over still water.
    fn drive_app(record: &AvatarRecord) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(avian3d::prelude::PhysicsPlugins::default())
            .insert_resource(avian3d::prelude::Gravity(Vec3::ZERO))
            .insert_resource(Time::<Fixed>::from_hz(DRIVE_HZ))
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(LiveAvatarRecord(record.clone()))
            // THE REAL SYSTEMS, in `FixedUpdate` as `PlayerPlugin` schedules
            // them, chained ahead of avian's own step in `FixedPostUpdate`.
            // Only the two drive systems: the suspension and the buoyancy
            // want a heightmap and a water registry this harness has none of,
            // and neither is part of the planar feel. Avian propagates
            // `Transform` to `GlobalTransform` at the head of its own
            // schedule, so the drive systems read a pose one step old -
            // exactly as they do in the game.
            .add_systems(
                FixedUpdate,
                (
                    crate::player::hover_boat::sync_hover_boat_physics,
                    crate::player::hover_boat::apply_hover_boat_drive,
                    crate::player::car::apply_car_drive,
                )
                    .chain(),
            );
        let entity = app
            .world_mut()
            .spawn(chassis_root_bundle(Transform::IDENTITY))
            .id();
        let mut commands = app.world_mut().commands();
        build_preset_components(&mut commands, entity, &record.locomotion);
        app.world_mut().flush();
        // `App::update` would do these on its first run; this harness drives
        // the schedules by hand, and some of avian's resources land in them.
        app.finish();
        app.cleanup();
        (app, entity)
    }

    /// One fixed step of the game's own loop: the drive systems in
    /// `FixedUpdate`, then avian's step in `FixedPostUpdate`, with the
    /// generic `Time` set to the fixed clock exactly as Bevy's `FixedMain`
    /// sets it - which is where avian reads its timestep from.
    fn drive_step(app: &mut App, keys: &[KeyCode]) {
        {
            let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            for key in [KeyCode::KeyW, KeyCode::KeyS, KeyCode::KeyA, KeyCode::KeyD] {
                if keys.contains(&key) {
                    input.press(key);
                } else {
                    input.release(key);
                }
            }
        }
        let dt = std::time::Duration::from_secs_f64(1.0 / DRIVE_HZ);
        app.world_mut().resource_mut::<Time<Fixed>>().advance_by(dt);
        let fixed = *app.world().resource::<Time<Fixed>>();
        *app.world_mut().resource_mut::<Time>() = fixed.as_generic();
        app.world_mut().run_schedule(FixedUpdate);
        app.world_mut().run_schedule(FixedPostUpdate);
    }

    /// Flat speed (m/s) and yaw rate (rad/s, +ve turning left about +Y).
    fn motion(app: &App, entity: Entity) -> (f32, f32) {
        let v = app
            .world()
            .get::<LinearVelocity>(entity)
            .expect("the chassis has a linear velocity")
            .0;
        let w = app
            .world()
            .get::<AngularVelocity>(entity)
            .expect("the chassis has an angular velocity")
            .0;
        (Vec3::new(v.x, 0.0, v.z).length(), w.y)
    }

    /// Drive one craft: hold forward to her top speed, then hold a turn at
    /// speed, then let go.
    fn drive_card(record: &AvatarRecord, window: Window) -> DriveCard {
        let (mut app, entity) = drive_app(record);
        let (hull_len, mass) = match &record.locomotion {
            LocomotionConfig::HoverBoat(p) => (p.chassis_half_extents.0[2] * 2.0, p.mass.0),
            LocomotionConfig::Car(p) => (p.chassis_half_extents.0[2] * 2.0, p.mass.0),
            _ => panic!("the card is for the two driven families"),
        };

        // 1. From rest, W held.
        let steps = (DRIVE_HZ * window.straight_secs) as usize;
        let mut speeds = Vec::with_capacity(steps);
        for _ in 0..steps {
            drive_step(&mut app, &[KeyCode::KeyW]);
            speeds.push(motion(&app, entity).0);
        }
        let top_speed = *speeds.last().expect("a trajectory");
        let after = |frac: f32| {
            speeds
                .iter()
                .position(|s| *s >= frac * top_speed)
                .map_or(f32::NAN, |i| (i + 1) as f32 / DRIVE_HZ as f32)
        };
        let (to_half, to_ninety) = (after(0.5), after(0.9));
        // How far the speed still moved over its last second - the straight
        // line's own answer to "was this window long enough".
        let a_second_back = speeds[speeds.len().saturating_sub(DRIVE_HZ as usize + 1)];
        let speed_drift = (top_speed - a_second_back).abs() / top_speed.abs().max(1e-6);

        // 2. W and A held together, from that speed: the steady turn. The
        // yaw rate a second before the reading says whether it IS steady -
        // an un-converged turn would make the circle below fiction.
        for _ in 0..(DRIVE_HZ * (window.turn_secs - 1.0)) as usize {
            drive_step(&mut app, &[KeyCode::KeyW, KeyCode::KeyA]);
        }
        let early_yaw = motion(&app, entity).1;
        for _ in 0..DRIVE_HZ as usize {
            drive_step(&mut app, &[KeyCode::KeyW, KeyCode::KeyA]);
        }
        let (turn_speed, yaw_rate) = motion(&app, entity);
        let yaw_drift = (yaw_rate - early_yaw).abs() / yaw_rate.abs().max(1e-6);
        // The circle she actually carves: diameter, and in her own lengths.
        let circle = if yaw_rate.abs() > 1e-4 {
            2.0 * turn_speed / yaw_rate.abs()
        } else {
            f32::INFINITY
        };
        // The idle profile's bank, from the two numbers it reads - both
        // measured here rather than guessed (src/player/gait.rs `advance_skiff`).
        let bank_degrees = matches!(record.locomotion, LocomotionConfig::Car(_)).then(|| {
            use crate::player::gait::{skiff_bank, skiff_bank_clamp, skiff_bank_sign};
            let (clamp, sign) = match &record.locomotion {
                LocomotionConfig::Car(p) => (
                    skiff_bank_clamp(
                        record
                            .gait
                            .as_ref()
                            .map_or(0.0, |g| g.head_turn_variance_degrees.0),
                    ),
                    skiff_bank_sign(p.mass.0),
                ),
                _ => unreachable!("the bank column is a skiff's"),
            };
            skiff_bank(yaw_rate, turn_speed, clamp, sign).to_degrees()
        });

        // 3. Everything released: the coast down to a tenth of her top speed.
        // Skipped for the guard, which asserts no relation that reads it.
        let mut coast_time = f32::NAN;
        let mut coast_distance = 0.0;
        if window.coast {
            for step in 0..(DRIVE_HZ * 60.0) as usize {
                drive_step(&mut app, &[]);
                let (speed, _) = motion(&app, entity);
                coast_distance += speed / DRIVE_HZ as f32;
                if coast_time.is_nan() && speed <= 0.1 * top_speed {
                    coast_time = (step + 1) as f32 / DRIVE_HZ as f32;
                    break;
                }
            }
        }

        DriveCard {
            turn_speed,
            yaw_drift,
            speed_drift,
            top_speed,
            to_half,
            to_ninety,
            yaw_rate: yaw_rate.abs().to_degrees(),
            circle,
            lengths: circle / hull_len,
            coast_time,
            coast_distance,
            bank_degrees,
            hull_len,
            mass,
        }
    }

    /// One seed per craft type, found by the craft pin's own hunt - the tool
    /// #1380 built, so a number on the card is a number the owner can reach
    /// with one lock and one re-roll.
    fn seed_per_craft() -> Vec<(crate::seeded_defaults::CraftType, u64)> {
        use crate::seeded_defaults::{AvatarPins, CraftType};
        let mut out = Vec::new();
        for craft in CraftType::BOATS.into_iter().chain(CraftType::SKIFFS) {
            let mut pins = AvatarPins::default();
            pins.lock_craft(Some(craft));
            let seed = pins
                .find_seed(0)
                .unwrap_or_else(|| panic!("{} is reachable", craft.label()));
            out.push((craft, seed));
        }
        out
    }

    /// A candidate feel, as the two locomotion builders express one:
    /// accelerations, not forces, since `drive_force = mass x drive_accel`
    /// and `turn_torque = mass x turn_accel` hold by construction
    /// (default_visuals/mod.rs `boat_locomotion` / `skiff_locomotion`).
    struct Candidate {
        mass_factor_scale: f32,
        drive_accel: f32,
        turn_accel: f32,
        linear_damping: f32,
        angular_damping: f32,
    }

    /// Re-derive a built locomotion config under a candidate feel, the way
    /// the builders derive one: mass scaled by the mass-factor ratio, the
    /// mass-scaled support fields with it under their own caps, the two
    /// forces as mass times the candidate's accelerations, and the dampings
    /// verbatim.
    ///
    /// THE COLLIDER IS NOT TOUCHED, and that is the point: the drawn craft
    /// is not changing, so her box - and the inertia avian takes from it -
    /// is the same box, which is exactly the term no arithmetic on this page
    /// would have got right.
    fn under_candidate(loco: &LocomotionConfig, c: &Candidate) -> LocomotionConfig {
        use crate::pds::types::Fp;
        let mut loco = loco.clone();
        match &mut loco {
            LocomotionConfig::HoverBoat(p) => {
                p.mass = Fp(p.mass.0 * c.mass_factor_scale);
                p.lateral_grip = Fp((p.lateral_grip.0 * c.mass_factor_scale).min(48_000.0));
                p.drive_force = Fp((p.mass.0 * c.drive_accel).min(50_000.0));
                p.turn_torque = Fp((p.mass.0 * c.turn_accel).min(50_000.0));
                p.linear_damping = Fp(c.linear_damping);
                p.angular_damping = Fp(c.angular_damping);
            }
            LocomotionConfig::Car(p) => {
                p.mass = Fp(p.mass.0 * c.mass_factor_scale);
                p.lateral_grip = Fp((p.lateral_grip.0 * c.mass_factor_scale).min(200_000.0));
                p.drive_force = Fp((p.mass.0 * c.drive_accel).min(50_000.0));
                p.turn_torque = Fp((p.mass.0 * c.turn_accel).min(50_000.0));
                p.linear_damping = Fp(c.linear_damping);
                p.angular_damping = Fp(c.angular_damping);
            }
            _ => panic!("the card is for the two driven families"),
        }
        loco
    }

    /// PRINT-ONLY. The candidate tuples of
    /// `target/dump/vehicles2026-09/feel/drive_card.txt`, run through the
    /// same probe that measured the craft as she is, so the card's "what it
    /// would do" column was measured and not predicted (#1381 phase 1).
    ///
    /// SINCE THE PORT IT IS A CROSS-CHECK, and that is worth keeping. The
    /// `now` row builds the config the way the game does - the seed's own
    /// craft hands `skiff_locomotion` / `boat_locomotion` its feel - while
    /// the `cand` row RE-DERIVES one through [`under_candidate`] from the
    /// tuple written here. The two arrive by different routes, so any
    /// difference between them is a bug in one of the two. Run after the
    /// port: identical on all ten, to every printed digit.
    ///
    /// The last row is the CONTROL: the sloop under a doubled mass factor
    /// and nothing else, which must come out identical - `drive_force` and
    /// `turn_torque` are both mass times an acceleration, and avian's
    /// inertia is mass times the box, so the mass cancels out of every
    /// number on this card. It is the one knob the owner should NOT be
    /// asked to turn for feel.
    #[test]
    #[ignore = "probe for #1381: what the recommended feel tuples would do"]
    fn probe_what_the_candidate_feels_would_do() {
        // craft, the candidate, and a word on what it is reaching for.
        let candidates: Vec<(&str, u64, Candidate, &str)> = vec![
            (
                "Longship",
                129,
                Candidate {
                    mass_factor_scale: 1.0,
                    drive_accel: 9.8,
                    turn_accel: 5.0,
                    linear_damping: 1.4,
                    angular_damping: 6.5,
                },
                "keeps the fleet's fastest hull, stops her out-turning a yacht",
            ),
            (
                "Steam tug",
                13,
                Candidate {
                    mass_factor_scale: 1.0,
                    drive_accel: 8.5,
                    turn_accel: 5.5,
                    linear_damping: 2.4,
                    angular_damping: 7.0,
                },
                "bollard pull and a screw under her rudder: shoves hard, turns hard, caps at 7 knots",
            ),
            (
                "Junk",
                177,
                Candidate {
                    mass_factor_scale: 1.0,
                    drive_accel: 7.2,
                    turn_accel: 4.5,
                    linear_damping: 1.6,
                    angular_damping: 6.5,
                },
                "a loaded battened trader, no longer the sloop bit for bit",
            ),
            (
                "Runabout",
                5,
                Candidate {
                    mass_factor_scale: 1.0,
                    drive_accel: 13.0,
                    turn_accel: 7.0,
                    linear_damping: 1.0,
                    angular_damping: 4.5,
                },
                "keeps her speed, carves instead of spinning",
            ),
            (
                "Scow",
                23,
                Candidate {
                    mass_factor_scale: 1.0,
                    drive_accel: 3.6,
                    turn_accel: 1.4,
                    linear_damping: 2.4,
                    angular_damping: 9.0,
                },
                "poled at walking pace and barely steerable, no longer the tug bit for bit",
            ),
            (
                "Dune buggy",
                36,
                Candidate {
                    mass_factor_scale: 1.0,
                    drive_accel: 11.0,
                    turn_accel: 3.6,
                    linear_damping: 0.8,
                    angular_damping: 3.5,
                },
                "light and darty",
            ),
            (
                "Armoured car",
                7,
                Candidate {
                    mass_factor_scale: 1.0,
                    drive_accel: 3.8,
                    turn_accel: 1.7,
                    linear_damping: 0.55,
                    angular_damping: 5.0,
                },
                "ponderous: slow to gather way, slow to shed it",
            ),
            (
                "Cyclecar",
                27,
                Candidate {
                    mass_factor_scale: 1.0,
                    drive_accel: 11.5,
                    turn_accel: 4.0,
                    linear_damping: 0.8,
                    angular_damping: 3.2,
                },
                "the fleet's most agile skiff",
            ),
            (
                "Wagon",
                2,
                Candidate {
                    mass_factor_scale: 1.0,
                    drive_accel: 1.9,
                    turn_accel: 1.7,
                    linear_damping: 0.45,
                    angular_damping: 4.0,
                },
                "a trotting cart horse, not a roadster",
            ),
            (
                "Rover",
                3,
                Candidate {
                    mass_factor_scale: 1.0,
                    drive_accel: 2.5,
                    turn_accel: 2.4,
                    linear_damping: 0.9,
                    angular_damping: 3.0,
                },
                "slow, deliberate, and she pivots",
            ),
            (
                "Sloop (CONTROL)",
                19,
                Candidate {
                    mass_factor_scale: 2.0,
                    drive_accel: 9.0,
                    turn_accel: 7.0,
                    linear_damping: 1.5,
                    angular_damping: 6.0,
                },
                "the mass factor doubled and nothing else: every number must hold",
            ),
        ];
        println!(
            "{:<18} {:<5} {:>6} {:>6} {:>6} {:>6} {:>7} {:>6} {:>6} {:>6} {:>6}",
            "craft",
            "when",
            "mass",
            "top",
            "km/h",
            "t90",
            "yaw/s",
            "circle",
            "lens",
            "coast",
            "bank"
        );
        for (name, seed, candidate, why) in candidates {
            let record = AvatarRecord::default_for_seed(seed);
            let mut after = record.clone();
            after.locomotion = under_candidate(&record.locomotion, &candidate);
            let line = |tag: &str, c: &DriveCard| {
                println!(
                    "{:<18} {:<5} {:>6.0} {:>6.2} {:>6.1} {:>6.2} {:>7.1} {:>6.1} {:>6.1} {:>6.2} {:>6}",
                    name,
                    tag,
                    c.mass,
                    c.top_speed,
                    c.top_speed * 3.6,
                    c.to_ninety,
                    c.yaw_rate,
                    c.circle,
                    c.lengths,
                    c.coast_time,
                    c.bank_degrees
                        .map_or("-".to_string(), |b| format!("{b:.1}")),
                );
            };
            line("now", &drive_card(&record, Window::FULL));
            line("cand", &drive_card(&after, Window::FULL));
            println!("                   -> {why}");
        }
    }

    /// PRINT-ONLY. What each of the twelve craft types actually does under
    /// the wheel, measured by running the game's own drive systems on a body
    /// built through the game's own spawn path (#1381 phase 1). Feeds
    /// `target/dump/vehicles2026-09/feel/drive_card.txt`, the card the owner
    /// drives by, and the per-type feel guard #1381 cut from it.
    ///
    /// WHY NOT ARITHMETIC. Top speed is `drive accel / linear damping` to a
    /// fair first approximation, but turn_torque is `mass x turn_accel` and
    /// the body answers with `torque / INERTIA`, which avian takes from the
    /// collider's box - so the same turn_accel turns a 4.4 m longship far
    /// more slowly than a 2.4 m runabout, and no closed form in this file
    /// would have said so. Three probes in this crate have been caught
    /// measuring their own arithmetic (`probe_how_fast_a_player_can_change_
    /// speed`'s doc); this one runs the systems.
    #[test]
    #[ignore = "probe for #1381: what the twelve craft types do under the wheel"]
    fn probe_what_each_craft_type_does_under_the_wheel() {
        println!(
            "{:<14} {:>5} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>7} {:>6} {:>6} {:>6} {:>6}",
            "craft",
            "len",
            "mass",
            "top",
            "km/h",
            "t50",
            "t90",
            "turn",
            "yaw/s",
            "circle",
            "lens",
            "coast",
            "bank"
        );
        for (craft, seed) in seed_per_craft() {
            let record = AvatarRecord::default_for_seed(seed);
            let c = drive_card(&record, Window::FULL);
            println!(
                "{:<14} {:>5.2} {:>6.0} {:>6.2} {:>6.1} {:>6.2} {:>6.2} {:>6.2} {:>7.1} {:>6.1} {:>6.1} {:>6.2} {:>6}  seed {seed}",
                craft.label(),
                c.hull_len,
                c.mass,
                c.top_speed,
                c.top_speed * 3.6,
                c.to_half,
                c.to_ninety,
                c.turn_speed,
                c.yaw_rate,
                c.circle,
                c.lengths,
                c.coast_time,
                c.bank_degrees
                    .map_or("-".to_string(), |b| format!("{b:.1}")),
            );
            println!(
                "               coast {:.1} m to a tenth of top speed; the turn is steady to \
                 {:.4}% over its last second",
                c.coast_distance,
                c.yaw_drift * 100.0
            );
        }
    }

    // -----------------------------------------------------------------------
    // The per-type feel guard, measured half (#1381 work item D)
    // -----------------------------------------------------------------------

    /// Assert that `a` clears `b` by at least `margin`, naming the claim it
    /// is part of.
    fn clears(claim: &str, a_name: &str, a: f32, b_name: &str, b: f32, margin: f32) {
        assert!(
            a >= b * (1.0 + margin),
            "{claim}: {a_name} {a:.2} is not {:.0}% clear of {b_name} {b:.2}",
            margin * 100.0
        );
    }

    /// The card of every craft type, by label, measured through the real
    /// drive systems at [`Window::GUARD`].
    fn fleet_cards() -> std::collections::BTreeMap<&'static str, DriveCard> {
        seed_per_craft()
            .into_iter()
            .map(|(craft, seed)| {
                let record = AvatarRecord::default_for_seed(seed);
                (craft.label(), drive_card(&record, Window::GUARD))
            })
            .collect()
    }

    /// THE PER-TYPE FEEL GUARD, measured half (#1381). The fleet drives in
    /// the order the owner agreed on 2026-09-20, asked of the real
    /// `apply_hover_boat_drive` / `apply_car_drive` over avian rather than
    /// of the feel table.
    ///
    /// Its table half is
    /// `pds::avatar::default_visuals::tests::no_two_craft_types_in_a_family_
    /// share_a_feel`, and this is the half that matters: two types can carry
    /// different tuples and still drive alike, and two types carrying the
    /// SAME tuple drive differently anyway, because the body answers a
    /// torque with `torque / inertia` and avian takes the inertia from the
    /// collider box. At 0211f30 the junk carried the sloop's tuple bit for
    /// bit and yawed 80.6 deg/s to her 112.0.
    ///
    /// # Why relations with a margin, and never digits
    ///
    /// Local glibc is 2.43 and CI's is 2.39, and the two disagree in the
    /// last ulp of `f32` `sin` and `acos` (memory `project_libm_nudge_shim`),
    /// which is enough to move a printed digit of a turning circle. A
    /// relation with a ten percent margin cannot flip on an ulp. The margins
    /// below are the measured ones rounded DOWN to a tenth, and each
    /// assertion says what it is claiming in words.
    ///
    /// # Why this window
    ///
    /// [`Window::GUARD`] is 16 s of straight line and 4 s of turn against
    /// the card's 20 and 20, with no coast - see its doc for the arithmetic.
    /// The first two assertions here are the proof that it was enough: every
    /// craft's speed and yaw rate must still be moving by under a tenth of a
    /// percent over their last second, so a window that were too short fails
    /// loudly instead of quietly measuring a transient.
    #[test]
    fn the_fleet_drives_in_the_agreed_order() {
        let c = fleet_cards();
        let at = |name: &str| -> &DriveCard {
            c.get(name)
                .unwrap_or_else(|| panic!("{name} is in the seeded fleet"))
        };

        // THE CONTROL FOR THE WINDOW. Every reading below is a steady-state
        // one or it is nothing.
        for (name, card) in &c {
            assert!(
                card.speed_drift < 0.001,
                "{name}: the straight line had not settled - speed still moving \
                 {:.4}% over its last second",
                card.speed_drift * 100.0
            );
            assert!(
                card.yaw_drift < 0.001,
                "{name}: the turn had not settled - yaw rate still moving {:.4}% \
                 over its last second",
                card.yaw_drift * 100.0
            );
        }

        // ---- THE BOATS ----

        // The runabout is the fastest boat, and it is not close: a planing
        // mahogany hull against five displacement ones.
        let boats = ["Sloop", "Longship", "Steam tug", "Junk", "Runabout", "Scow"];
        for other in boats.iter().filter(|b| **b != "Runabout") {
            clears(
                "the runabout is the fastest boat",
                "the runabout",
                at("Runabout").top_speed,
                other,
                at(other).top_speed,
                0.5,
            );
        }
        // ...and the scow the slowest, being poled at walking pace. She is
        // the slowest of all twelve, which is the stronger claim.
        for other in c.keys().filter(|b| **b != "Scow") {
            clears(
                "the scow is the slowest craft in the fleet",
                other,
                at(other).top_speed,
                "the scow",
                at("Scow").top_speed,
                0.5,
            );
        }

        // A long, shallow, keel-less hull out-runs a keelboat and skids in
        // the turn. Both halves, because either alone would let the port
        // drift back to a faster longship that also out-turns a yacht.
        clears(
            "the longship out-runs the sloop",
            "the longship",
            at("Longship").top_speed,
            "the sloop",
            at("Sloop").top_speed,
            0.1,
        );
        clears(
            "...and loses to her in the turn",
            "the sloop",
            at("Sloop").yaw_rate,
            "the longship",
            at("Longship").yaw_rate,
            0.3,
        );
        clears(
            "...in her own lengths too",
            "the longship's circle",
            at("Longship").lengths,
            "the sloop's",
            at("Sloop").lengths,
            0.3,
        );

        // A tug has a screw right under her rudder and a barge has a pole.
        clears(
            "the tug out-turns the scow",
            "the tug",
            at("Steam tug").yaw_rate,
            "the scow",
            at("Scow").yaw_rate,
            0.5,
        );
        clears(
            "...and comes round in fewer of her own lengths",
            "the scow's circle",
            at("Scow").lengths,
            "the tug's",
            at("Steam tug").lengths,
            0.5,
        );

        // ---- THE SKIFFS ----

        // The cyclecar is the most agile thing on land, not merely the
        // fastest: the highest yaw rate of any skiff.
        //
        // NOT ASSERTED, and said here rather than left to be rediscovered:
        // the agreed order called her "the tightest skiff circle", and on
        // the measured card she is not. The ROVER comes round in 5.5 m to
        // her 16.6, and among the skiffs that carve rather than pivot the
        // wagon's 17.7 m is only 6.6% off hers - inside any margin an ulp
        // cannot flip. Her yaw rate is the claim that survives measuring.
        let skiffs = [
            "Roadster",
            "Dune buggy",
            "Armoured car",
            "Cyclecar",
            "Wagon",
            "Rover",
        ];
        for other in skiffs.iter().filter(|s| **s != "Cyclecar") {
            clears(
                "the cyclecar is the most agile skiff",
                "the cyclecar",
                at("Cyclecar").yaw_rate,
                other,
                at(other).yaw_rate,
                0.1,
            );
        }

        // A servo rover drives each wheel: it pivots, it does not corner.
        for other in skiffs.iter().filter(|s| **s != "Rover") {
            clears(
                "the rover turns in the fewest of her own lengths",
                other,
                at(other).lengths,
                "the rover",
                at("Rover").lengths,
                1.0,
            );
        }

        // The wagon is the slowest skiff to gather way - a claim that one
        // shared damping could not express at all, because top speed and
        // time-to-speed were the same knob until #1381 gave `SkiffFeel` its
        // own linear damping. The armoured car is next, and both are clear
        // of the 2.89 s the other four share.
        for other in skiffs.iter().filter(|s| **s != "Wagon") {
            clears(
                "the wagon is the slowest skiff to gather way",
                "the wagon",
                at("Wagon").to_ninety,
                other,
                at(other).to_ninety,
                0.1,
            );
        }
        for other in skiffs
            .iter()
            .filter(|s| **s != "Wagon" && **s != "Armoured car")
        {
            clears(
                "the armoured car is ponderous: second slowest to gather way",
                "the armoured car",
                at("Armoured car").to_ninety,
                other,
                at(other).to_ninety,
                0.1,
            );
        }

        // NOT the wagon, and the agreed order said it was: measured, the
        // ROVER is the slowest skiff on top speed (2.78 m/s to the wagon's
        // 4.23). The wagon's claim is the acceleration one above.
        for other in skiffs.iter().filter(|s| **s != "Rover") {
            clears(
                "the rover is the slowest skiff",
                other,
                at(other).top_speed,
                "the rover",
                at("Rover").top_speed,
                0.3,
            );
        }
    }
}
