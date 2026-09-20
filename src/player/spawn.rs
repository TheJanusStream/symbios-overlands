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
/// live spawn path and the #670 regression test so the two can't drift.
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
fn chassis_root_bundle(transform: Transform) -> impl Bundle {
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
        yaw_drift: f32,
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
    fn drive_card(record: &AvatarRecord) -> DriveCard {
        let (mut app, entity) = drive_app(record);
        let (hull_len, mass) = match &record.locomotion {
            LocomotionConfig::HoverBoat(p) => (p.chassis_half_extents.0[2] * 2.0, p.mass.0),
            LocomotionConfig::Car(p) => (p.chassis_half_extents.0[2] * 2.0, p.mass.0),
            _ => panic!("the card is for the two driven families"),
        };

        // 1. From rest, W held. 20 s is far past converged for every type.
        let steps = (DRIVE_HZ * 20.0) as usize;
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

        // 2. W and A held together, from that speed: the steady turn. The
        // yaw rate a second before the reading says whether it IS steady -
        // an un-converged turn would make the circle below fiction.
        for _ in 0..(DRIVE_HZ * 19.0) as usize {
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
        let bank_degrees = matches!(record.locomotion, LocomotionConfig::Car(_))
            .then(|| crate::player::gait::skiff_bank(yaw_rate, turn_speed).to_degrees());

        // 3. Everything released: the coast down to a tenth of her top speed.
        let mut coast_time = f32::NAN;
        let mut coast_distance = 0.0;
        for step in 0..(DRIVE_HZ * 60.0) as usize {
            drive_step(&mut app, &[]);
            let (speed, _) = motion(&app, entity);
            coast_distance += speed / DRIVE_HZ as f32;
            if coast_time.is_nan() && speed <= 0.1 * top_speed {
                coast_time = (step + 1) as f32 / DRIVE_HZ as f32;
                break;
            }
        }

        DriveCard {
            turn_speed,
            yaw_drift,
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

    /// PRINT-ONLY. The recommended candidate tuples of
    /// `target/dump/vehicles2026-09/feel/drive_card.txt`, run through the
    /// same probe that measured the craft as she is, so the card's "what it
    /// would do" column is measured and not predicted (#1381 phase 1).
    ///
    /// The first row of each pair is the craft at HEAD. The last row is the
    /// CONTROL: the sloop under a doubled mass factor and nothing else, which
    /// must come out identical - `drive_force` and `turn_torque` are both
    /// mass times an acceleration, so the mass cancels out of every number on
    /// this card. It is the one knob the owner should NOT be asked to turn
    /// for feel.
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
            line("now", &drive_card(&record));
            line("cand", &drive_card(&after));
            println!("                   -> {why}");
        }
    }

    /// PRINT-ONLY. What each of the twelve craft types actually does under
    /// the wheel, measured by running the game's own drive systems on a body
    /// built through the game's own spawn path (#1381 phase 1). Feeds
    /// `target/dump/vehicles2026-09/feel/drive_card.txt`, the card the owner
    /// drives by, and the per-type feel guard #1382 cuts from it.
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
            let c = drive_card(&record);
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
}
