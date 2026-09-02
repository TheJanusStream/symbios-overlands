use super::build::rigged_root_transform;
use super::*;
use crate::pds::AvatarRecord;
use crate::pds::avatar::ResolvedRig;
use crate::pds::avatar::wardrobe::engine_default_for_did;
use crate::state::{LiveAvatarRecord, LocalPlayer};
use bevy::ecs::system::RunSystemOnce;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy_symbios_avatar::{AvatarBody as BuiltBody, AvatarClosure, AvatarPose};
use symbios_avatar::Limb;
use symbios_avatar::anim::{gait, gesture};

/// A minimal world carrying every store the spawn path touches — the
/// same skeleton `tests/freeze_rigid_body.rs` builds.
fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()));
    app.init_asset::<Mesh>();
    app.init_asset::<StandardMaterial>();
    app.init_asset::<Image>();
    app.init_asset::<SkinnedMeshInverseBindposes>();
    app.add_message::<crate::player::emote::EmoteRequest>();
    app
}

fn rigged_record(resolved: ResolvedRig) -> AvatarRecord {
    let mut record = AvatarRecord::wearing("3jzfcijpj2z2a");
    if let Some(rig) = record.body.rigged_mut() {
        rig.resolved = Some(resolved);
    }
    record
}

/// #1066: the body must face where the chassis is going.
///
/// The engine and Bevy disagree about forward — `landmark::FORWARD` is
/// `+Z` (the glTF/VRM convention), Bevy's is `-Z`, and the chassis is
/// aimed with `Transform::looking_to`, which points *its* `-Z` down the
/// direction of travel. Without the half turn on the rigged root the
/// avatar ran backwards.
///
/// Asserted against the engine's OWN forward landmark rather than
/// against a re-derivation of it: `Socket::Chest`'s anchor is defined to
/// face `+FORWARD`, so if upstream ever changes which way that points,
/// this fails instead of silently agreeing with a stale constant.
#[test]
fn a_rigged_body_faces_the_way_its_chassis_travels() {
    let avatar = symbios_avatar::Avatar::build_with(
        &engine_default_for_did("did:plc:facing-test"),
        &symbios_avatar::AvatarConfig {
            atlas: 64,
            ..Default::default()
        },
    )
    .expect("the seeded default engine body builds");

    let chest = symbios_avatar::Socket::Chest
        .anchor(&avatar.rig)
        .expect("a humanoid rig has a chest");
    // Sanity: the engine really does put the chest on +Z. If this trips,
    // the bug is upstream and the correction below is aimed wrong.
    assert!(
        chest.direction.dot(Vec3::Z) > 0.9,
        "engine chest anchor is no longer +Z forward: {}",
        chest.direction
    );

    // In chassis space, after the root's correction.
    let facing = rigged_root_transform(0.9).rotation * chest.direction;
    assert!(
        facing.dot(Vec3::NEG_Z) > 0.9,
        "the body's chest must point down Bevy's forward (-Z), the axis \
         `looking_to` aims at the direction of travel; got {facing}"
    );
    // And the turn must be a pure yaw — a body tipped or rolled here
    // would plant its feet through the floor.
    let up = rigged_root_transform(0.9).rotation * Vec3::Y;
    assert!(
        up.dot(Vec3::Y) > 0.999,
        "the correction tilted the body: {up}"
    );
}

/// A chassis with a rigged root under it, carrying motion state.
fn chassis_with_body(app: &mut App) -> (Entity, Entity) {
    let chassis = app
        .world_mut()
        .spawn((Transform::default(), GlobalTransform::default()))
        .id();
    let root = app
        .world_mut()
        .spawn((RiggedRoot, RiggedMotion::default(), ChildOf(chassis)))
        .id();
    (chassis, root)
}

/// How far a foot's sole is pitched from its own rest attitude, in degrees,
/// positive toe-up.
///
/// Mirrors the engine's own `sole_pitch`: measured on the POSED body
/// between the rearmost and foremost joints past the ankle, and referenced
/// to the rest attitude rather than to level, because this body's foot
/// nodes run a few degrees uphill at rest and 0 has to mean "carried as it
/// stands".
fn sole_pitch(rig: &symbios_avatar::Rig, pose: &Pose, limb: Limb) -> f32 {
    let joints = rig.extremity_joints(limb);
    let sole = &joints[1..];
    let along = |&joint: &usize| rig.joints[joint].position.z;
    let rear = *sole
        .iter()
        .min_by(|a, b| along(a).total_cmp(&along(b)))
        .expect("a foot");
    let fore = *sole
        .iter()
        .max_by(|a, b| along(a).total_cmp(&along(b)))
        .expect("a foot");
    let angle = |run: Vec3| {
        run.y
            .atan2((run.x * run.x + run.z * run.z).sqrt())
            .to_degrees()
    };
    let posed = pose.forward(rig);
    angle(posed.positions[fore] - posed.positions[rear])
        - angle(rig.joints[fore].position - rig.joints[rear].position)
}

#[test]
fn the_procedural_walk_lands_toe_up_and_leaves_toe_down() {
    // **#1069.** This drove `gait::step` and `gait::swing_arms` and stopped,
    // never `gait::roll_feet` — so every procedurally-driven body walked
    // with its soles held at their rest attitude: no heel-strike, no
    // toe-off, the whole foot tilting with the shin at full stride. The
    // engine treats the three as one drive sequence and `examples/walkaudit`
    // has always called all three; this had two of them.
    //
    // **Driven through `drive_rigged_motion` and read off the `AvatarPose`
    // the body is actually drawn in.** Written first as a loop that called
    // step/swing_arms/plant/roll itself and asserted on that — which proves
    // nothing about this file, because deleting the roll from the system
    // under test leaves such a test passing on its own copy of the
    // sequence. A test that reimplements its subject measures its own
    // arithmetic.
    //
    // The defect was invisible in the app for a separate reason worth
    // recording: the gait only drives a body when the clip library is
    // empty, which on native never happens. It would have appeared the
    // moment #1067 removed the clips, looking like a regression the removal
    // caused. `test_app` ships an empty library, which is exactly the wasm
    // pre-fetch case and the post-#1067 case.
    let mut app = test_app();
    let chassis = app
        .world_mut()
        .spawn((Transform::default(), GlobalTransform::default()))
        .id();
    let avatar = symbios_avatar::Avatar::build_with(
        &engine_default_for_did("did:plc:ankle-test"),
        &symbios_avatar::AvatarConfig {
            atlas: 64,
            ..Default::default()
        },
    )
    .expect("the seeded default engine body builds");
    let mut built = Some(avatar);
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  mut images: ResMut<Assets<Image>>,
                  mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>| {
                let Some(avatar) = built.take() else {
                    return;
                };
                install_built_body(
                    &mut commands,
                    chassis,
                    0.9,
                    avatar,
                    &[],
                    &mut meshes,
                    &mut materials,
                    &mut images,
                    &mut bindposes,
                );
            },
        )
        .expect("runs");
    let mut roots = app.world_mut().query_filtered::<Entity, With<RiggedRoot>>();
    let root = roots.single(app.world()).expect("one rigged root");
    let rig = app
        .world()
        .get::<BuiltBody>(root)
        .expect("the body landed")
        .avatar
        .rig
        .clone();

    // Walk the chassis forward. No `LinearVelocity` and no transform
    // propagation here, so the speed the driver reads is the one this moves
    // the `GlobalTransform` by — which is the remote-peer path, and enough
    // to select the gait.
    const STEP_SECS: f32 = 1.0 / 60.0;
    const PACE: f32 = 1.3;
    let (mut lowest, mut highest) = (f32::MAX, f32::MIN);
    for frame in 0..150 {
        let at = Vec3::Z * (PACE * STEP_SECS * frame as f32);
        let mut chassis_mut = app.world_mut().entity_mut(chassis);
        // Aimed into the travel the way the app's controller steers a
        // chassis (`looking_to`, whose −Z faces the movement), not a bare
        // translation — since the foothold ledger (engine #277) the drive
        // is handed the transform the pose renders under, and a chassis
        // marching +Z while facing world −Z is a permanent moonwalk no
        // app state produces: the ledger rightly holds its plants where
        // the RENDER puts them, and the old bare-translation harness read
        // that as half a metre of skate. Aimed, the chassis' π yaw and
        // the rigged root's own half turn (#1066) cancel, so the
        // instruments' plain `at + posed` world reads stay exact.
        let aimed = Transform::from_translation(at).looking_to(Vec3::Z, Vec3::Y);
        *chassis_mut.get_mut::<Transform>().unwrap() = aimed;
        *chassis_mut.get_mut::<GlobalTransform>().unwrap() = GlobalTransform::from(aimed);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(STEP_SECS));
        app.world_mut()
            .run_system_once(drive_rigged_motion)
            .expect("runs");

        let motion = app.world().get::<RiggedMotion>(root).expect("motion state");
        // Only once the gait is actually the source: the first frame has no
        // previous position, so it reads as standing.
        if motion.source != MotionSource::Gait {
            continue;
        }
        let pose = &app.world().get::<AvatarPose>(root).expect("a pose").0;
        for limb in [Limb::HindLeft, Limb::HindRight] {
            let pitch = sole_pitch(&rig, pose, limb);
            lowest = lowest.min(pitch);
            highest = highest.max(pitch);
        }
    }
    assert!(
        lowest < f32::MAX,
        "the gait never drove the body — the test never measured anything"
    );

    // The literature's bands, which the engine's constants are set against:
    // heel-strike ~15-25 degrees toe-up, push-off ~15-20 toe-down. Asserted
    // loosely, because what is guarded here is that the stage RUNS — a sole
    // held flat all cycle reads -0.0 to 0.0 and is a shuffle. What actually
    // arrives through this path is -17.2 to 20.1 degrees, which is
    // `examples/walkaudit`'s own reading upstream: the app is now driving
    // the gait the engine's instrument measures.
    assert!(
        highest > 10.0,
        "the foot never landed toe-up: peak pitch {highest:.1} deg — roll_feet is not running"
    );
    assert!(
        lowest < -10.0,
        "the foot never left toe-down: lowest pitch {lowest:.1} deg — roll_feet is not running"
    );
}

/// Drives one body along `+Z` at a fixed ground speed and reports what the
/// drawn pose did: the widest fore-and-aft split between the feet, and the
/// highest the pelvis rode above its standing height.
///
/// Both are read off the `AvatarPose` the body is actually drawn in rather
/// than recomputed here — a test that reimplements its subject measures its
/// own arithmetic, which is the lesson
/// `the_procedural_walk_lands_toe_up_and_leaves_toe_down` records above.
fn walked_at(metres_per_second: f32) -> (f32, f32) {
    let mut app = test_app();
    let chassis = app
        .world_mut()
        .spawn((Transform::default(), GlobalTransform::default()))
        .id();
    let avatar = symbios_avatar::Avatar::build_with(
        &engine_default_for_did("did:plc:speed-test"),
        &symbios_avatar::AvatarConfig {
            atlas: 64,
            ..Default::default()
        },
    )
    .expect("the seeded default engine body builds");
    let mut built = Some(avatar);
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  mut images: ResMut<Assets<Image>>,
                  mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>| {
                let Some(avatar) = built.take() else {
                    return;
                };
                install_built_body(
                    &mut commands,
                    chassis,
                    0.9,
                    avatar,
                    &[],
                    &mut meshes,
                    &mut materials,
                    &mut images,
                    &mut bindposes,
                );
            },
        )
        .expect("runs");
    let mut roots = app.world_mut().query_filtered::<Entity, With<RiggedRoot>>();
    let root = roots.single(app.world()).expect("one rigged root");
    let rig = app
        .world()
        .get::<BuiltBody>(root)
        .expect("the body landed")
        .avatar
        .rig
        .clone();
    let pelvis = rig
        .joints
        .iter()
        .position(|joint| joint.parent.is_none())
        .expect("a root joint");
    let standing = rig.joints[pelvis].position.y;
    let feet: Vec<usize> = [Limb::HindLeft, Limb::HindRight]
        .into_iter()
        .map(|limb| rig.in_zone(symbios_avatar::Zone::Extremity(limb))[0])
        .collect();

    const STEP_SECS: f32 = 1.0 / 60.0;
    let (mut split, mut crest) = (0.0f32, f32::MIN);
    for frame in 0..240 {
        let at = Vec3::Z * (metres_per_second * STEP_SECS * frame as f32);
        let mut chassis_mut = app.world_mut().entity_mut(chassis);
        // Aimed into the travel the way the app's controller steers a
        // chassis (`looking_to`, whose −Z faces the movement), not a bare
        // translation — since the foothold ledger (engine #277) the drive
        // is handed the transform the pose renders under, and a chassis
        // marching +Z while facing world −Z is a permanent moonwalk no
        // app state produces: the ledger rightly holds its plants where
        // the RENDER puts them, and the old bare-translation harness read
        // that as half a metre of skate. Aimed, the chassis' π yaw and
        // the rigged root's own half turn (#1066) cancel, so the
        // instruments' plain `at + posed` world reads stay exact.
        let aimed = Transform::from_translation(at).looking_to(Vec3::Z, Vec3::Y);
        *chassis_mut.get_mut::<Transform>().unwrap() = aimed;
        *chassis_mut.get_mut::<GlobalTransform>().unwrap() = GlobalTransform::from(aimed);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(STEP_SECS));
        app.world_mut()
            .run_system_once(drive_rigged_motion)
            .expect("runs");
        let motion = app.world().get::<RiggedMotion>(root).expect("motion state");
        if motion.source != MotionSource::Gait {
            continue;
        }
        let pose = &app.world().get::<AvatarPose>(root).expect("a pose").0;
        let posed = pose.forward(&rig);
        split = split.max((posed.positions[feet[0]].z - posed.positions[feet[1]].z).abs());
        crest = crest.max(posed.positions[pelvis].y - standing);
    }
    assert!(crest > f32::MIN, "the gait never drove the body");
    (split, crest)
}

/// Accelerates one body across the walk-run transition and reports the
/// largest single-frame step the leading contact took **through its own
/// step**, as a fraction of one.
///
/// **Not a distance, and that is the point** (#1071). A foot legitimately
/// travels every frame, and at a run it travels a long way; millimetres
/// cannot separate a body moving fast from a body whose clock was
/// relabelled under it. What a change of gait must not do is move the
/// contact to a different part of its STEP, so the reading is taken on an
/// axis that does not move when the duty does — half for the stance, half
/// for the swing — which is exactly the quantity `phase_matched` preserves
/// and the quantity that jumps when nothing preserves it.
///
/// Taken off `RiggedMotion::cycle` as the drive actually left it, under
/// the gait the drive actually built, so it measures this file rather than
/// a re-derivation of it.
fn worst_phase_step(from: f32, to: f32, seconds: f32, fps: f32) -> f32 {
    let mut app = test_app();
    let chassis = app
        .world_mut()
        .spawn((Transform::default(), GlobalTransform::default()))
        .id();
    let avatar = symbios_avatar::Avatar::build_with(
        &engine_default_for_did("did:plc:transition-test"),
        &symbios_avatar::AvatarConfig {
            atlas: 64,
            ..Default::default()
        },
    )
    .expect("the seeded default engine body builds");
    let mut built = Some(avatar);
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  mut images: ResMut<Assets<Image>>,
                  mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>| {
                let Some(avatar) = built.take() else {
                    return;
                };
                install_built_body(
                    &mut commands,
                    chassis,
                    0.9,
                    avatar,
                    &[],
                    &mut meshes,
                    &mut materials,
                    &mut images,
                    &mut bindposes,
                );
            },
        )
        .expect("runs");
    let mut roots = app.world_mut().query_filtered::<Entity, With<RiggedRoot>>();
    let root = roots.single(app.world()).expect("one rigged root");
    let rig = app
        .world()
        .get::<BuiltBody>(root)
        .expect("the body landed")
        .avatar
        .rig
        .clone();
    let feet: Vec<usize> = [Limb::HindLeft, Limb::HindRight]
        .into_iter()
        .map(|limb| rig.in_zone(symbios_avatar::Zone::Extremity(limb))[0])
        .collect();

    let step_secs = 1.0 / fps;
    let frames = (seconds / step_secs) as usize;
    let mut moves: Vec<f32> = Vec::new();
    let mut worst_detail = (0.0f32, 0.0f32, Vec3::ZERO, 0.0f32, false, 0.0f32);
    let mut last_phase: Option<f32> = None;
    let mut phase_jump = (0.0f32, 0.0f32, 0.0f32, false);
    let mut before: Option<Vec<Vec3>> = None;
    let mut at = Vec3::ZERO;
    let mut crossed = (false, false);
    for frame in 0..frames {
        // A ramp in speed, integrated into a position the driver reads its
        // own speed back off — the remote-peer path, and the one that
        // exercises the transition without a velocity component to fake.
        let pace = from + (to - from) * (frame as f32 / frames as f32);
        at += Vec3::Z * (pace * step_secs);
        let mut chassis_mut = app.world_mut().entity_mut(chassis);
        // Aimed into the travel the way the app's controller steers a
        // chassis (`looking_to`, whose −Z faces the movement), not a bare
        // translation — since the foothold ledger (engine #277) the drive
        // is handed the transform the pose renders under, and a chassis
        // marching +Z while facing world −Z is a permanent moonwalk no
        // app state produces: the ledger rightly holds its plants where
        // the RENDER puts them, and the old bare-translation harness read
        // that as half a metre of skate. Aimed, the chassis' π yaw and
        // the rigged root's own half turn (#1066) cancel, so the
        // instruments' plain `at + posed` world reads stay exact.
        let aimed = Transform::from_translation(at).looking_to(Vec3::Z, Vec3::Y);
        *chassis_mut.get_mut::<Transform>().unwrap() = aimed;
        *chassis_mut.get_mut::<GlobalTransform>().unwrap() = GlobalTransform::from(aimed);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(step_secs));
        app.world_mut()
            .run_system_once(drive_rigged_motion)
            .expect("runs");
        let motion = app.world().get::<RiggedMotion>(root).expect("motion state");
        if motion.source != MotionSource::Gait {
            continue;
        }
        // **The speed the DRIVER fed the gait, not a re-derivation from
        // the raw ramp.** Since #1192 the pace the clock advances on is
        // eased, so a gait rebuilt here from the chassis' instantaneous
        // speed crosses the walk-run duty step on a different frame than
        // the driven one did — and a phase read against the wrong duty
        // reported a near-full-step relabel that never reached a body.
        // The module's own rule: measure the subject, not this file's
        // arithmetic.
        let Some(speed) = motion.gaiting else {
            continue;
        };
        crossed = (
            crossed.0 || !speed.is_running(),
            crossed.1 || speed.is_running(),
        );
        let pose = &app.world().get::<AvatarPose>(root).expect("a pose").0;
        let posed = pose.forward(&rig);
        let now: Vec<Vec3> = feet.iter().map(|&joint| posed.positions[joint]).collect();
        // Where the leading contact is in its OWN step, on an axis that
        // does not move when the duty does: half for the stance, half for
        // the swing. This is what a change of gait must not relabel.
        let phase = match speed.gait(&rig).phase(0, motion.cycle) {
            symbios_avatar::anim::gait::Phase::Stance(t) => t * 0.5,
            symbios_avatar::anim::gait::Phase::Swing(t) => 0.5 + t * 0.5,
        };
        if let Some(was) = last_phase {
            let ahead: f32 = (phase - was).rem_euclid(1.0);
            if ahead > phase_jump.0 {
                phase_jump = (ahead, pace, speed.duty(), speed.is_running());
            }
        }
        last_phase = Some(phase);
        if let Some(prev) = &before {
            let step = now
                .iter()
                .zip(prev)
                .map(|(now, before)| now.distance(*before))
                .fold(0.0f32, f32::max);
            if step > worst_detail.0 {
                let which = now
                    .iter()
                    .zip(prev)
                    .max_by(|a, b| a.0.distance(*a.1).total_cmp(&b.0.distance(*b.1)))
                    .expect("two feet");
                worst_detail = (
                    step,
                    pace,
                    (*which.0 - *which.1),
                    speed.duty(),
                    speed.is_running(),
                    motion.cycle,
                );
            }
            moves.push(step);
        }
        before = Some(now);
    }
    assert!(
        crossed.0 && crossed.1,
        "the sweep never crossed the walk-run transition — it measured one gait"
    );
    assert!(moves.len() > 30, "too few samples to have a median");
    let worst = moves.iter().copied().fold(0.0f32, f32::max);
    let _ = (worst, worst_detail);
    moves.sort_by(f32::total_cmp);
    phase_jump.0
}

/// Walks a body up to speed, stops the chassis dead, and reports the
/// furthest a foot that was BEARING WEIGHT at that moment then slid
/// through the world, in metres.
///
/// A planted contact is pinned to the ground: everything it does out here
/// is a skate. The chassis holds still after the stop, so the world and
/// the body's own frame differ by a constant and a foot's motion in one is
/// its motion in the other.
fn skid_through_a_stop(walk_frames: usize) -> (f32, f32, usize) {
    skid_through_a_decelerating_stop(walk_frames, 0)
}

/// How far a foot standing on the ground slides while the body CHANGES
/// SPEED without ever stopping, in metres.
///
/// **The control is the whole instrument** (#277). A walking body slides
/// its feet a little at the best of times, so a figure from a decelerating
/// walk means nothing until the same body walking at a CONSTANT speed
/// through the same window has been read the same way. Pass `from == to`
/// for that control.
///
/// The body never drops below `IDLE_BELOW`, so the idle never engages and
/// none of #276 is in the path: whatever this measures belongs to the gait.
fn skate_through_a_speed_change(walk_frames: usize, from: f32, to: f32, ramp_frames: usize) -> f32 {
    skate_through(walk_frames, from, to, ramp_frames, None)
}

/// As [`skate_through_a_speed_change`], with an emote optionally laid
/// over the walk for the whole measured window — the system-level half of
/// the #329 adoption: the Bow's vocabulary is the body line, pelvis and
/// hips included, and the promise that the planted soles stay planted
/// through it belongs to the DRIVE (the settle tail plants stance
/// contacts after the overlay, #253's order), which only a full-drive
/// instrument can ask.
fn skate_through(
    walk_frames: usize,
    from: f32,
    to: f32,
    ramp_frames: usize,
    over: Option<Emote>,
) -> f32 {
    let mut app = test_app();
    let chassis = app
        .world_mut()
        .spawn((Transform::default(), GlobalTransform::default()))
        .id();
    let avatar = symbios_avatar::Avatar::build_with(
        &engine_default_for_did("did:plc:stop-test"),
        &symbios_avatar::AvatarConfig {
            atlas: 64,
            ..Default::default()
        },
    )
    .expect("the seeded default engine body builds");
    let mut built = Some(avatar);
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  mut images: ResMut<Assets<Image>>,
                  mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>| {
                let Some(avatar) = built.take() else {
                    return;
                };
                install_built_body(
                    &mut commands,
                    chassis,
                    0.9,
                    avatar,
                    &[],
                    &mut meshes,
                    &mut materials,
                    &mut images,
                    &mut bindposes,
                );
            },
        )
        .expect("runs");
    let mut roots = app.world_mut().query_filtered::<Entity, With<RiggedRoot>>();
    let root = roots.single(app.world()).expect("one rigged root");
    let rig = app
        .world()
        .get::<BuiltBody>(root)
        .expect("the body landed")
        .avatar
        .rig
        .clone();
    // **The SOLE joints, not the ankle** (#277). `extremity_joints` puts the
    // joint the foot hangs from first and the sole points after it, and the
    // ankle is the wrong point to ask: a planted foot rolls heel to toe
    // through its stance, which translates the ankle horizontally while the
    // sole under it has not moved at all. Asking the ankle read 53.5 mm on
    // a body walking at a DEAD CONSTANT speed, which is the roll and not a
    // skate — and it read the same 53.5 at every phase and under every
    // speed change, which is what a measurement of the wrong thing looks
    // like when the wrong thing is deterministic.
    // **And the SOLE POINTS under them, not the joints themselves**
    // (#1082). A sole joint sits above the sole it belongs to, so a foot
    // pitching about a sole point still translates every joint over it:
    // reading the joints billed that roll as a skate and left a flat 10 to
    // 13 mm on every speed, against 3 mm for the points beneath them. This
    // is the same trap the ankle correction above records, one level down.
    // The sole point is the joint's rest position dropped to the ground
    // plane the body was built standing on, carried into the pose by the
    // ankle it hangs from — which is how `roll_feet` itself models a sole.
    let feet: Vec<(usize, Vec<usize>)> = [Limb::HindLeft, Limb::HindRight]
        .into_iter()
        .filter_map(|limb| {
            let joints = rig.extremity_joints(limb);
            let (&ankle, sole) = (joints.first()?, joints.get(1..)?);
            Some((ankle, sole.to_vec()))
        })
        .collect();

    const STEP_SECS: f32 = 1.0 / 60.0;
    let mut at = Vec3::ZERO;
    let frame = |app: &mut App, at: Vec3| {
        let mut chassis_mut = app.world_mut().entity_mut(chassis);
        // Aimed into the travel the way the app's controller steers a
        // chassis (`looking_to`, whose −Z faces the movement), not a bare
        // translation — since the foothold ledger (engine #277) the drive
        // is handed the transform the pose renders under, and a chassis
        // marching +Z while facing world −Z is a permanent moonwalk no
        // app state produces: the ledger rightly holds its plants where
        // the RENDER puts them, and the old bare-translation harness read
        // that as half a metre of skate. Aimed, the chassis' π yaw and
        // the rigged root's own half turn (#1066) cancel, so the
        // instruments' plain `at + posed` world reads stay exact.
        let aimed = Transform::from_translation(at).looking_to(Vec3::Z, Vec3::Y);
        *chassis_mut.get_mut::<Transform>().unwrap() = aimed;
        *chassis_mut.get_mut::<GlobalTransform>().unwrap() = GlobalTransform::from(aimed);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(STEP_SECS));
        app.world_mut()
            .run_system_once(drive_rigged_motion)
            .expect("runs");
    };
    for _ in 0..walk_frames {
        at += Vec3::Z * (from * STEP_SECS);
        frame(&mut app, at);
    }
    // The emote begins exactly where the measurement does, so the window
    // covers its whole play (1.5 s of the 2 s measured) plus its ending
    // blend.
    if let Some(emote) = over {
        app.world_mut()
            .get_mut::<RiggedMotion>(root)
            .expect("motion state")
            .gesture = Some(ActiveGesture {
            emote,
            elapsed: 0.0,
        });
    }
    let mut track: Vec<Vec<Vec<Vec3>>> = Vec::with_capacity(120);
    let mut down: Vec<Vec<bool>> = Vec::with_capacity(120);
    for step in 0..120 {
        let speed = if step < ramp_frames {
            let done = (step as f32 + 0.5) / ramp_frames as f32;
            from + (to - from) * done
        } else {
            to
        };
        at += Vec3::Z * (speed * STEP_SECS);
        frame(&mut app, at);
        let motion = app.world().get::<RiggedMotion>(root).expect("motion");
        let stance: Vec<bool> = {
            let gait = motion.gaiting.map(|speed| speed.gait(&rig));
            let cycle = motion.cycle;
            [Limb::HindLeft, Limb::HindRight]
                .iter()
                .map(|limb| {
                    gait.as_ref().is_some_and(|gait| {
                        gait.limbs
                            .iter()
                            .position(|which| which == limb)
                            .is_some_and(|index| gait.phase(index, cycle).is_stance())
                    })
                })
                .collect()
        };
        let pose = &app.world().get::<AvatarPose>(root).expect("a pose").0;
        let posed = pose.forward(&rig);
        track.push(
            feet.iter()
                .map(|(ankle, sole)| {
                    sole.iter()
                        .map(|&joint| {
                            let rest = rig.joints[joint].position;
                            at + posed.positions[*ankle]
                                + posed.rotations[*ankle]
                                    * (Vec3::new(rest.x, 0.0, rest.z) - rig.joints[*ankle].position)
                        })
                        .collect()
                })
                .collect(),
        );
        down.push(stance);
    }
    assert_eq!(
        app.world()
            .get::<RiggedMotion>(root)
            .expect("motion")
            .source,
        MotionSource::Gait,
        "the body must still be walking, or this is measuring a stop"
    );

    // **Each sole point judged while IT is on the ground, against itself.**
    // Two gates, because a stance foot is not one rigid thing: the gait
    // says the FOOT is bearing, and each sole point's own height says
    // whether that point is the one in contact right now. Contact transfers
    // heel to toe through a stance, and a heel that lifts has moved without
    // sliding — so a point is only asked to hold still while it is down.
    //
    // Per point against itself, never the lowest point of the moment: an
    // argmin whose identity moves compares a heel against a toe, which is
    // the reading `Walk::settle` records as a quarter of a metre of
    // correction on flat ground where there were four millimetres.
    const CLEARANCE: f32 = 0.005;
    let mut skate = 0.0f32;
    for which in 0..feet.len() {
        for point in 0..feet[which].1.len() {
            let floor = track
                .iter()
                .map(|frame| frame[which][point].y)
                .fold(f32::MAX, f32::min);
            let mut anchor = None;
            for (frame, stance) in track.iter().zip(&down) {
                let world = frame[which][point];
                if stance[which] && world.y - floor <= CLEARANCE {
                    let from = *anchor.get_or_insert(world);
                    skate = skate.max(Vec3::new(world.x - from.x, 0.0, world.z - from.z).length());
                } else {
                    anchor = None;
                }
            }
        }
    }
    skate
}

/// How far a planted sole may slide at a constant speed, in metres.
///
/// Eight millimetres. The swept figure is 1.4 to 4.1 and the defect this
/// guards against read 16.6 to 17.7 on the same instrument, so this sits
/// between the two rather than just above the passing number.
const STEADY_SKATE_CEILING: f32 = 0.008;

#[test]
fn a_walking_body_holds_its_planted_sole_at_every_pace() {
    // **#278, and the acceptance is the CURVE rather than a figure.** The
    // issue was filed on two speeds — 26.3 mm at 1.4 m/s against 0.8 at
    // 0.7, thirty-three times the skate for twice the pace — and a pair
    // cannot show a shape. Swept, the shape is a THRESHOLD between 1.0 and
    // 1.2 m/s, which is neither the square nor the crouch the issue offered
    // as candidates.
    //
    // Two things were wrong and they have to be told apart, because each
    // accounts for about half of the filed number:
    //
    // ```text
    //   m/s                       0.4   0.7   1.0   1.2   1.4   1.6   1.8
    //   sole JOINTS, unfixed      1.4   0.9  12.7  28.3  25.3  29.4  25.8
    //   sole POINTS, unfixed      1.5   0.9   2.0  16.6  17.5  17.1  17.7
    //   sole POINTS, fixed        3.0   2.8   4.1   3.9   3.6   3.4   1.4
    // ```
    //
    // The first row is what this file used to measure: a sole JOINT sits
    // above the sole it belongs to, so a foot pitching about a point still
    // translates every joint over it, and that added about ten millimetres
    // at every speed (#1082). The second row is the defect itself. The
    // third is with engine #278 landed — `Walk::settle` no longer rolls the
    // ankles when it has not planted, which is what this file was asking
    // for by driving the head with `footing: None` and settling separately.
    //
    // Swept rather than pinned at one pace, which is what the issue asked
    // for: a guard at 1.4 alone would have passed throughout the defect's
    // life at 0.7.
    for metres in [0.4f32, 0.7, 1.0, 1.2, 1.4, 1.6, 1.8] {
        let skate = skate_through_a_speed_change(120, metres, metres, 1);
        assert!(
            skate < STEADY_SKATE_CEILING,
            "at a constant {metres} m/s a planted sole slid {:.1} mm, against a ceiling \
             of {:.1}",
            skate * 1000.0,
            STEADY_SKATE_CEILING * 1000.0
        );
    }
}

#[test]
fn a_bow_over_a_walk_keeps_its_planted_soles() {
    // **The system-level half of the #329 adoption** (the clip-level
    // half, with the reasoning, is in
    // `a_gesture_leaves_the_legs_to_the_locomotion_layer`). The Bow
    // pitches the pelvis and swings the hip sockets, so at the clip
    // level the leg chain moves — and the drive's settle tail then
    // plants the stance contacts after the overlay, which is the
    // promise a walking body actually makes: bow mid-walk, and the
    // sole bearing your weight stays essentially put.
    //
    // The ceiling is NOT the steady-pace one, and the difference is the
    // gesture blend, priced deliberately: a gesture starts and ends
    // through a 0.15 s inertializer (#1068 — dropping it snaps 75-100 mm
    // of joint travel into one frame), and the blend is applied AFTER
    // the settle tail, so through those two windows the drawn foot is a
    // mix of the settled walk and the bowed walk. A wave moves no leg,
    // so its blend dragged nothing; the whole-body bow gives the blend
    // ~13 mm of hip line to mix across, measured 13.9 mm here against
    // 4.1 steady. The guard holds the whole action under 20 mm — a raw
    // uncompensated leg track reads an order of magnitude past that.
    let bowed = skate_through(120, 1.0, 1.0, 1, Some(Emote::Bow));
    assert!(
        bowed < 0.020,
        "bowing over a walk slid a planted sole {:.1} mm, against a ceiling of 20.0 \
         (13.9 measured: the gesture blend's own mixing, see the comment)",
        bowed * 1000.0,
    );
}

#[test]
#[ignore = "probe for #277: does changing speed slide a planted foot"]
fn probe_whether_changing_speed_slides_a_planted_foot() {
    // **Two controls, one per end speed** (#277). A ramp case ENDS at a
    // different speed than it started, and the first run read a
    // decelerating body as skating LESS than the steady one — which says
    // nothing about changing speed and everything about ending up at
    // 0.7 m/s. Without both controls the ramp columns are uninterpretable.
    for walk in [100usize, 110, 120] {
        let fast = skate_through_a_speed_change(walk, 1.4, 1.4, 1);
        let slow = skate_through_a_speed_change(walk, 0.7, 0.7, 1);
        let eased = skate_through_a_speed_change(walk, 1.4, 0.7, 15);
        let quick = skate_through_a_speed_change(walk, 1.4, 0.7, 3);
        let faster = skate_through_a_speed_change(walk, 0.7, 1.4, 3);
        println!(
            "walk {walk}: STEADY 1.4 {:.1} mm, 0.7 {:.1} | 1.4->0.7 slow {:.1} quick {:.1} \
             | 0.7->1.4 quick {:.1}",
            fast * 1000.0,
            slow * 1000.0,
            eased * 1000.0,
            quick * 1000.0,
            faster * 1000.0
        );
    }
}

/// The same, with the chassis brought to rest over `ramp_frames` instead of
/// stopped dead.
///
/// **Because a dead stop is a worst case no player produces.** The chassis
/// is physics-driven and decelerates, so `planar` falls through
/// `IDLE_BELOW` gradually and the gait's own stride shrinks with it on the
/// way down. Whether a hold is worth taking depends entirely on how fast
/// the body is still nominally travelling while it holds, so a governor
/// judged only against an instantaneous stop is judged against the one
/// profile that makes waiting maximally expensive.
/// The idle seed every stop instrument stands its body on (#1194): the value
/// `RiggedMotion::default()` draws first in a fresh process, so the figure is
/// the one the instrument always read for its first sim when run alone.
const INSTRUMENT_SEED: u64 = 7;

fn skid_through_a_decelerating_stop(walk_frames: usize, ramp_frames: usize) -> (f32, f32, usize) {
    let mut app = test_app();
    let chassis = app
        .world_mut()
        .spawn((Transform::default(), GlobalTransform::default()))
        .id();
    let avatar = symbios_avatar::Avatar::build_with(
        &engine_default_for_did("did:plc:stop-test"),
        &symbios_avatar::AvatarConfig {
            atlas: 64,
            ..Default::default()
        },
    )
    .expect("the seeded default engine body builds");
    let mut built = Some(avatar);
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  mut images: ResMut<Assets<Image>>,
                  mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>| {
                let Some(avatar) = built.take() else {
                    return;
                };
                install_built_body(
                    &mut commands,
                    chassis,
                    0.9,
                    avatar,
                    &[],
                    &mut meshes,
                    &mut materials,
                    &mut images,
                    &mut bindposes,
                );
            },
        )
        .expect("runs");
    let mut roots = app.world_mut().query_filtered::<Entity, With<RiggedRoot>>();
    let root = roots.single(app.world()).expect("one rigged root");
    // **The body's seed is pinned, because the figure this reads is a
    // function of it** (#1194). `install_built_body` seeds each body's idle
    // off a process-wide counter, and the idle's seed decides when its
    // settling weight shift fires and which leg it moves first — the exact
    // mechanism (engine #276) that steps the stopped foot home and so the
    // whole of what this measures. Under `cargo test` the counter's value is
    // how many bodies the OTHER tests in the process had made first, which is
    // why the same stop read 58, 63 and 82 mm from one binary depending on
    // the filter (and 58.83 then 58.16 on two runs of the same filter). Seeded
    // here, every phase of the sweep stands on the same idle and every
    // context reads one figure.
    app.world_mut()
        .entity_mut(root)
        .insert(RiggedMotion::seeded(INSTRUMENT_SEED));
    let rig = app
        .world()
        .get::<BuiltBody>(root)
        .expect("the body landed")
        .avatar
        .rig
        .clone();
    let feet: Vec<usize> = [Limb::HindLeft, Limb::HindRight]
        .into_iter()
        .map(|limb| rig.in_zone(symbios_avatar::Zone::Extremity(limb))[0])
        .collect();

    const STEP_SECS: f32 = 1.0 / 60.0;
    const PACE: f32 = 1.4;
    let mut at = Vec3::ZERO;
    let frame = |app: &mut App, at: Vec3| {
        let mut chassis_mut = app.world_mut().entity_mut(chassis);
        // Aimed into the travel the way the app's controller steers a
        // chassis (`looking_to`, whose −Z faces the movement), not a bare
        // translation — since the foothold ledger (engine #277) the drive
        // is handed the transform the pose renders under, and a chassis
        // marching +Z while facing world −Z is a permanent moonwalk no
        // app state produces: the ledger rightly holds its plants where
        // the RENDER puts them, and the old bare-translation harness read
        // that as half a metre of skate. Aimed, the chassis' π yaw and
        // the rigged root's own half turn (#1066) cancel, so the
        // instruments' plain `at + posed` world reads stay exact.
        let aimed = Transform::from_translation(at).looking_to(Vec3::Z, Vec3::Y);
        *chassis_mut.get_mut::<Transform>().unwrap() = aimed;
        *chassis_mut.get_mut::<GlobalTransform>().unwrap() = GlobalTransform::from(aimed);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(STEP_SECS));
        app.world_mut()
            .run_system_once(drive_rigged_motion)
            .expect("runs");
    };
    for _ in 0..walk_frames {
        at += Vec3::Z * (PACE * STEP_SECS);
        frame(&mut app, at);
    }
    // Which feet are carrying the body at the instant it stops, and where
    // they are. Asked of the gait the drive is actually running.
    let motion = app.world().get::<RiggedMotion>(root).expect("motion");
    assert_eq!(
        motion.source,
        MotionSource::Gait,
        "the body must be walking"
    );
    let gait = Speed::new(&rig, PACE).gait(&rig);
    let planted: Vec<usize> = gait
        .limbs
        .iter()
        .enumerate()
        .filter(|(index, _)| gait.phase(*index, motion.cycle).is_stance())
        .filter_map(|(_, limb)| {
            [Limb::HindLeft, Limb::HindRight]
                .iter()
                .position(|which| which == limb)
        })
        .collect();
    assert!(!planted.is_empty(), "a walking body has a foot down");
    let ahead = gait.until_handoff(motion.cycle);
    let mut held = 0usize;
    // **World positions, not body-local ones** — `at` is added back in.
    // With a dead stop the two differ by a constant and the distinction
    // does not matter, which is why the original reading could omit it.
    // The moment the chassis is still MOVING during the measurement (the
    // deceleration ramp below), a foot correctly planted in the world
    // travels backward in the body's frame at walking pace, and a
    // body-local reading calls that a skate. It is not one.
    let start: Vec<Vec3> = {
        let pose = &app.world().get::<AvatarPose>(root).expect("a pose").0;
        let posed = pose.forward(&rig);
        planted
            .iter()
            .map(|&foot| at + posed.positions[feet[foot]])
            .collect()
    };

    // Now the chassis stops. The foot that was down must stay where the
    // ground is while the body works out that it has stopped.
    //
    // **Measured within a STANCE EPISODE, not from the stop instant**, and
    // the distinction is the difference between a skate and a step. A
    // planted foot is pinned, so everything it does is a skate — but a foot
    // is only planted while the gait says it is. Under the deceleration
    // ramp the body is still genuinely walking, so the foot this follows
    // completes its stance and then SWINGS, and a reading taken from the
    // stop instant counts that swing: the first version of the ramp read
    // 753.6 mm at a half-second ramp, which is one stride length (731 mm)
    // and not a defect at all.
    //
    // So displacement is reset every time the foot leaves the ground and
    // measured afresh from where it next lands. At a dead stop there is
    // exactly one episode and it begins at the stop, which is why the
    // ratcheted figure is unchanged by this: the harness got stricter
    // somewhere it was never exercised.
    // **Collected, then judged**, because whether a foot was DOWN on a
    // given frame is only knowable against the lowest that foot gets — and
    // that is not known until the window is over. See the verdict below.
    let mut track: Vec<Vec<Vec3>> = Vec::with_capacity(60);
    for held_frame in 0..60 {
        // The deceleration ramp, linear in speed down to rest. Zero
        // `ramp_frames` is the dead stop the ratchet is measured at.
        if held_frame < ramp_frames {
            let left = 1.0 - (held_frame as f32 + 0.5) / ramp_frames as f32;
            at += Vec3::Z * (PACE * STEP_SECS * left);
        }
        frame(&mut app, at);
        let motion = app.world().get::<RiggedMotion>(root).expect("motion");
        if motion.source == MotionSource::Gait {
            held += 1;
        }
        // Whether each followed foot is bearing weight THIS frame, asked of
        // the gait the drive is actually running rather than inferred from
        // the foot's height. A body with no gait left is standing, and a
        // standing foot is as pinned as a stance one — that is the interval
        // the blend drags it through, and it is the whole reading.
        let pose = &app.world().get::<AvatarPose>(root).expect("a pose").0;
        let posed = pose.forward(&rig);
        track.push(
            planted
                .iter()
                .map(|&foot| at + posed.positions[feet[foot]])
                .collect(),
        );
    }

    // **A foot ON THE GROUND may not move; a foot in the air may.** That is
    // the whole definition of a skate, it is what a viewer actually sees,
    // and it needs no bookkeeping from the drive — which is the point,
    // because the drive's own answer changed underneath this harness. It
    // used to be enough to ask the gait, treating a body with no gait left
    // as standing on both feet. Since engine #276 an idle deliberately
    // LIFTS an unweighted foot to step it home, and the old rule counted
    // that step as a skate: it read 127 mm and rising across one recovery
    // on a body doing exactly the right thing.
    //
    // Grounded is judged per foot against the lowest that foot reaches in
    // the window, which is where it stands, rather than against a floor
    // height this harness would have to be told.
    const CLEARANCE: f32 = 0.005;
    let mut skid = 0.0f32;
    for which in 0..planted.len() {
        let floor = track
            .iter()
            .map(|frame| frame[which].y)
            .fold(f32::MAX, f32::min);
        let mut anchor = Some(start[which]);
        for frame in &track {
            let world = frame[which];
            if world.y - floor <= CLEARANCE {
                let from = *anchor.get_or_insert(world);
                skid = skid.max(world.distance(from));
            } else {
                anchor = None;
            }
        }
    }
    (skid, ahead, held)
}

#[test]
#[ignore = "probe for engine #266: prints the stop-cost matrix, asserts nothing"]
fn probe_the_stop_cost_against_how_fast_the_body_stopped() {
    for ramp in [0usize, 6, 15, 30] {
        let worst = (100..=130)
            .map(|frames| skid_through_a_decelerating_stop(frames, ramp).0)
            .fold(0.0f32, f32::max);
        let best = (100..=130)
            .map(|frames| skid_through_a_decelerating_stop(frames, ramp).0)
            .fold(f32::MAX, f32::min);
        println!(
            "ramp {:>2} frames ({:.2}s): worst {:.1} mm, best {:.1} mm",
            ramp,
            ramp as f32 / 60.0,
            worst * 1000.0,
            best * 1000.0
        );
    }
}

#[test]
fn a_stop_does_not_skate() {
    // **This was a ratchet on a known defect and is now a guard on a fixed
    // one** (#1071, engine #266 and #276). A body that stopped walking used
    // to blend from mid-stride into a stand, dragging the foot that was
    // bearing its weight — pinned to the ground, so every millimetre of it
    // a skate. It depended entirely on WHEN in the step the body stopped:
    // 15.4 mm near midstance and 297.9 mm at the worst phase.
    //
    // TWO FIXES WERE TRIED ON THE TRANSITION'S TIMING AND BOTH LOST. Hold
    // the change until the next handoff: 351.3 mm. Hold it until the next
    // midstance, which engine #266 proved is the moment the drag is
    // identically zero: 322.2 mm. Neither can win, because a body that
    // holds keeps striding while whatever stopped it has stopped — the WAIT
    // IS ITSELF A SKATE, at about 1.23 m per cycle held on this body, and
    // the best moment only saves 366 mm.
    //
    // What fixed it was not a moment but a mechanism (engine #276): the
    // idle is handed the stance the body arrived in, pins the contacts that
    // were bearing weight, and steps them home one at a time on its own
    // weight shifts — a foot only ever moving while it is unloaded and off
    // the ground.
    //
    // Swept over a whole cycle's worth of stopping phases, because a single
    // phase measures one point of a curve that varies by twenty to one —
    // and the first version of this test did exactly that and read the same
    // 203.2 mm with a wait in and with it out.
    //
    // Measured like for like under the current metric: 225.3 mm before the
    // fix, 35.2 after.
    //
    // **THE THRESHOLD WAS SET FOR CROSS-ENVIRONMENT SPREAD, NOT FOR THE
    // MEASUREMENT, FROM #1182 UNTIL THE 0.5.1 ADOPTION BELOW.** This
    // simulation is deterministic — a fixed seed, a fixed 1/60 s step, no
    // wall clock — and it still read differently depending on where it
    // was COMPILED:
    //
    //     25.4 mm   this repo's dev box (Gentoo-packaged rustc 1.96.1)
    //     35.2 mm   whatever box recorded the figure above
    //     50.1 mm   a GitHub ubuntu-latest runner (official rustup 1.96.1)
    //
    // Same source, same lockfile, same profile; a 2x spread. Rust
    // documents `f32` transcendentals as platform-dependent, and the gait
    // is built out of them, so the rig's arithmetic is only reproducible
    // against a fixed toolchain BUILD, not merely a fixed version. That is
    // #1132's finding — recorded there against terrain and scatter —
    // reaching locomotion.
    //
    // Ruled out while chasing it, so nobody re-runs these: it is not the
    // test profile (dev and test-release agree here), not the floating
    // lockfile (a fresh CI-style resolve reads the same 25.4 mm, and the
    // glam that moved is a different major from the 0.32.1 bevy_math and
    // symbios-avatar actually link), and not in-process interference (the
    // full threaded suite agrees with the test run alone).
    //
    // **TIGHTENED FROM 100 mm AT THE symbios-avatar 0.5.1 ADOPTION, AND
    // THEN TO ONE FIGURE UNDER #1194** (#1183, engine #323/#324). The
    // engine's whole anim path now computes its transcendentals through
    // the pure-Rust `libm`, the same bits on every toolchain build, and
    // the plan was to sit this guard just above the resulting single
    // figure. Tightening is what EXPOSED that the figure was not single
    // even on one box: from one binary it read 62.80 mm alone, 58.16 after
    // the player::rigged:: tests and 82.09 after all of player:: or the
    // full lib suite (CI's exact invocation).
    //
    // That was ONE global, not the two the spread suggested: the
    // process-wide counter `RiggedMotion::default` seeds each body's idle
    // from. Traced per sim (#1194), the skid is a function of the stop
    // phase and the idle seed alone — the same (phase, seed) pair read the
    // same in every context — and the seed was simply how many bodies the
    // process had made before this one: 7..37 alone, and whatever the
    // other tests' interleaving left it at otherwise (the rigged:: filter
    // read 58.83 and then 58.16 on consecutive runs; player:: read 58.83
    // serialised against 82.09 threaded). Phase dominates — two of the 31
    // stopping phases read 55-82 mm and the rest 15-40 — and the seed moves
    // the bad phase by ±25 mm through when the idle's settling shift fires
    // and which leg it takes first. `skid_through_a_decelerating_stop` now
    // stands its body on `INSTRUMENT_SEED`, and every context reads the one
    // figure below.
    //
    // (57.85 rather than the old 25.41 alone is the routing itself: slerp
    // came off glam's per-backend SIMD polynomial and `to_axis_angle`'s
    // atan2 off the platform libm — the arithmetic changed, and
    // reproducible was never going to mean smaller.)
    //
    // So the guard is at 60 mm, 2 mm above the one figure and nearly
    // 4-fold below the 225.3 mm regression it exists to catch —
    //
    //     57.85 mm   alone, after player::rigged::, after all of player::
    //                — one figure on INSTRUMENT_SEED (2026-09-02, this
    //                box, symbios-avatar 0.5.1)
    //
    // If CI reads past it, read the printed figure against that line: a
    // millimetre or two is an unroutable `bevy_math`/`avian` glam site
    // (#1183/#323's controlled experiments), the high 50s to 80s is a body
    // standing on an unpinned seed, and the pre-fix regime reads hundreds.
    let worst = (100..=130)
        .map(|frames| skid_through_a_stop(frames).0)
        .fold(0.0f32, f32::max);
    // The reading itself, so every run records its figure instead of only
    // its verdict — the silent environment margin was #1183's complaint,
    // and the figure-per-context table on #1194 only exists because a run
    // printed one.
    println!("worst stop skid: {:.2} mm", worst * 1000.0);
    assert!(
        worst < 0.06,
        "a foot standing on the ground slid {:.1} mm through a stop, against 57.85 mm \
         on INSTRUMENT_SEED in every process context (#1194) and 225.3 mm before \
         engine #276 — a millimetre or two is an unroutable glam site (#1183/#323), \
         the high 50s to 80s is a body standing on an unpinned seed, and the pre-fix \
         regime reads hundreds.",
        worst * 1000.0
    );
}

#[test]
fn crossing_the_walk_run_boundary_does_not_relabel_the_clock() {
    // **#1071's item 3, and the defect it removes is invisible in a
    // number and obvious in a body.** The duty falls all the way along
    // the speed axis and STEPS at the transition — about 0.55 to 0.35 —
    // so a cycle fraction handed across unchanged means a different part
    // of the step on the other side, and a foot in mid-swing arrives
    // planted. `transition::carry_cycle` maps it exactly.
    //
    // Asked the only way a discontinuity can be told from a fast motion:
    // sample twice as finely and see whether the largest step halves.
    // Measured on this body, accelerating 1.4 -> 3.2 m/s over three
    // seconds, as the largest step the leading contact takes through its
    // own step between two frames:
    //
    //   without the carry   0.121, 0.112, 0.108  at 60, 120, 240 fps
    //   with it             0.050, 0.025, 0.013
    //
    // The first converges on a tenth of a step that no amount of sampling
    // removes, at pace 1.72 m/s and duty 0.350 — the transition frame
    // itself. The second is exactly the cadence and halves with it.
    let steps: Vec<f32> = [60.0, 120.0, 240.0]
        .into_iter()
        .map(|fps| worst_phase_step(1.4, 3.2, 3.0, fps))
        .collect();
    // Six tenths rather than a half: halving is what a smooth motion does
    // exactly, and the slack is for where two samplings straddle the peak
    // differently.
    for pair in steps.windows(2) {
        assert!(
            pair[1] <= pair[0] * 0.6,
            "the leading contact's phase stepped {:.4} of a step and then {:.4} at twice \
             the frame rate — a step that does not halve when the sampling doubles is a \
             cliff, and the clock was relabelled under the body",
            pair[0],
            pair[1],
        );
    }
}

#[test]
fn a_faster_body_takes_a_longer_step_and_eventually_leaves_the_ground() {
    // **#1070, and the defect it removes is a one-liner with a long shadow.**
    // This file pinned the stride at `Stride::for_body(rig, 1.0)` and
    // expressed speed by bending the CADENCE alone, so a sprinting avatar
    // took a stroller's step very quickly. Everything scaled by the stride
    // inherited that: the trunk lean is scaled by the stride against the
    // legs taking it, and with the stride pinned that ratio never moved, so
    // #239's new lean was a constant in the app.
    //
    // Now the speed axis answers all of it (symbios-avatar #240): a faster
    // body takes a LONGER step, and past the Froude number where the
    // inverted pendulum stops working it runs rather than speed-walking.
    // Both are read off the drawn pose.
    let (slow_split, slow_crest) = walked_at(1.0);
    let (brisk_split, _) = walked_at(1.8);
    let (_, fast_crest) = walked_at(3.0);

    // **Both samples are walks, and that is deliberate.** A foot's split is
    // its EXCURSION — how far it slides back under the body across one
    // stance — and that legitimately FALLS when the body starts running,
    // because a running foot is down for a third of the cycle instead of
    // two thirds. Comparing a walk against a run here reads the gait change
    // as a shorter stride and asserts the opposite of the truth.
    assert!(
        brisk_split > slow_split * 1.15,
        "a body at 1.8 m/s split its feet {brisk_split:.3} m against {slow_split:.3} at \
         1.0 — the stride is still pinned"
    );
    // A walking body never rises above its standing height; a running one
    // is a projectile between steps and does. That is the cleanest sign in
    // the drawn pose that the gait changed on its own.
    assert!(
        slow_crest <= 1e-3,
        "a walking body rode {:.1} mm above standing height",
        slow_crest * 1000.0
    );
    assert!(
        fast_crest > 0.005,
        "a body at 3 m/s never left the ground: crest {:.1} mm — it is still walking",
        fast_crest * 1000.0
    );
}

#[test]
fn a_chat_keyword_gestures_the_sender_and_nobody_else() {
    // #1068. The request names a chassis; every OTHER body in the room has
    // to stay still, which is the property that makes this readable as
    // "that person waved" rather than as the room twitching.
    let mut app = test_app();
    let (sender, sender_body) = chassis_with_body(&mut app);
    let (_bystander, bystander_body) = chassis_with_body(&mut app);

    let request = crate::player::emote::request_for(sender, "hello everyone")
        .expect("\"hello\" asks for a greeting");
    app.world_mut().write_message(request);
    app.world_mut()
        .run_system_once(start_emotes)
        .expect("start_emotes runs");

    let gestured = |app: &App, body: Entity| {
        app.world()
            .entity(body)
            .get::<RiggedMotion>()
            .expect("the body keeps its motion state")
            .gesture
            .is_some()
    };
    assert!(gestured(&app, sender_body), "the sender should gesture");
    assert!(
        !gestured(&app, bystander_body),
        "a body the request did not name must not gesture"
    );
}

#[test]
fn a_flood_of_keywords_gestures_once() {
    // The rate limit, and the reason it is measured from the START of a
    // gesture: a peer pasting "hi hi hi" must wave once and then stand
    // there. Both requests are delivered in one run, so this also covers
    // two keywords arriving in the same frame.
    let mut app = test_app();
    let (chassis, body) = chassis_with_body(&mut app);

    for text in ["hi", "hello again", "hey"] {
        let request = crate::player::emote::request_for(chassis, text).expect("asks");
        app.world_mut().write_message(request);
    }
    app.world_mut()
        .run_system_once(start_emotes)
        .expect("start_emotes runs");

    // Advance the gesture as playback would, and assert the flood does not
    // rewind it. **Asserted on `elapsed` rather than on `gestured_at`,
    // because the clock does not move under `run_system_once`** — the
    // stamp is identical whether the cooldown ran or not, so the first
    // version of this test passed with the guard deleted. Playback
    // progress is the one thing a restart cannot fake.
    {
        let mut motion = app.world_mut().entity_mut(body);
        let mut motion = motion.get_mut::<RiggedMotion>().unwrap();
        assert!(motion.gesture.is_some(), "the first keyword should gesture");
        motion.gesture.as_mut().unwrap().elapsed = 0.4;
    }

    for text in ["hi", "hey"] {
        let request = crate::player::emote::request_for(chassis, text).expect("asks");
        app.world_mut().write_message(request);
    }
    app.world_mut()
        .run_system_once(start_emotes)
        .expect("start_emotes runs");

    let motion = app.world().entity(body).get::<RiggedMotion>().unwrap();
    let gesture = motion.gesture.expect("the gesture is still running");
    assert_eq!(
        gesture.elapsed, 0.4,
        "a keyword inside the cooldown restarted the gesture from the top"
    );
}

#[test]
fn a_gesture_leaves_the_legs_to_the_locomotion_layer() {
    // **The load-bearing claim of the emote layer** (#1068): a gesture
    // plays over a walk rather than replacing it, so the joints that
    // carry the body must come out of the application untouched while the
    // upper body takes it. Without this a wave while walking would stand
    // the body still and slide its feet along the ground.
    //
    // Under the clips this was `overlay_gesture`'s rule, enforced with a
    // joint mask; since #1067 it is a property of the goal-space format —
    // a clip writes only the parts its tracks address — and THIS is the
    // test that keeps it one: the engine is free to add tracks to a
    // gesture, and one that grows a leg or root track fails here before
    // a walking body ever slides a foot.
    let avatar = symbios_avatar::Avatar::build_with(
        &engine_default_for_did("did:plc:emote-test"),
        &symbios_avatar::AvatarConfig {
            atlas: 64,
            ..Default::default()
        },
    )
    .expect("the default body builds");
    let rig = &avatar.rig;

    // A walk pose, so the legs hold something a gesture could destroy.
    let mut walking = Pose::rest(rig);
    let speed = Speed::new(rig, 1.4);
    let gait = speed.gait(rig);
    let stride = speed.stride(rig);
    gait::step(rig, &mut walking, &gait, &stride, 0.25, |_| None);
    gait::swing_arms(rig, &mut walking, &gait, &stride, 0.25);

    // Compared by DOT rather than by `Quat::angle_between`, which is
    // `acos` of the dot and so loses all its precision exactly where this
    // test looks: acos(0.9999999) is already 3.4e-4, so two bit-identical
    // rotations read as a third of a milliradian apart and every carrying
    // joint failed.
    let apart = |a: Quat, b: Quat| 1.0 - a.dot(b).abs();
    // Judged against the rig's own zones — the same question
    // `gait::swing_arms` asks to decide which limbs are legs, so a body
    // plan nobody has written yet answers it correctly too.
    //
    // **The contract moved with engine 0.5 (#329), and this guard moved
    // with its meaning.** It used to demand the pelvis and every leg
    // joint's LOCAL rotation stay bit-identical, which was true of the
    // folded 0.4 bow and is exactly what #329 removed: a bow is one
    // ankle-to-crown line now, so the gesture pitches the pelvis — hip
    // extension — and counter-rotates the limbs so the legs keep the pose
    // the step authored. What the locomotion layer actually owns is WHERE
    // THE LEGS ARE: every joint a ground contact hangs its chain on must
    // stay put in space, which a leg track without compensation cannot
    // fake and the distributed bow preserves by construction.
    let carried: Vec<usize> = rig
        .ground_contacts()
        .into_iter()
        .flat_map(|limb| {
            [
                symbios_avatar::Zone::UpperLimb(limb),
                symbios_avatar::Zone::LowerLimb(limb),
                symbios_avatar::Zone::Extremity(limb),
            ]
        })
        .flat_map(|zone| rig.in_zone(zone))
        .collect();
    assert!(
        carried.len() > 3,
        "a biped should carry itself on more than {} joints",
        carried.len()
    );
    let planted = walking.forward(rig);

    // Every emote in the roster, over the same walk, at three points of
    // its play — the claim is about the set, not about the wave, and a
    // track added to any one of them is exactly what this exists to
    // catch. Mid-gesture alone would miss a key that returns to zero by
    // the middle.
    for emote in Emote::ALL {
        let clip = gesture::by_name(emote.gesture_name()).expect("the roster covers it");
        for through in [0.25, 0.5, 0.9] {
            let mut posed = walking.clone();
            clip.apply(rig, &mut posed, through);
            assert_eq!(
                posed.translation, walking.translation,
                "{emote:?} moved the root at {through} — a Root track has no \
                 business in an emote"
            );
            let gestured = posed.forward(rig);
            // The Bow is the one emote whose vocabulary is the whole body
            // line (engine #329): it pitches the pelvis — hip extension —
            // which swings the hip sockets on an arc, so its leg chain
            // legitimately translates ~13 mm at the clip level. The
            // planted feet are the SYSTEM's promise, not the clip's: the
            // settle tail plants stance contacts after the overlay (#253
            // order), guarded through the full drive by
            // `a_bow_over_a_walk_keeps_its_planted_soles`. What the clip
            // level still owes is a ceiling — a raw leg track without the
            // distribution's compensation swings a foot by hundreds of
            // millimetres, and that must never come back.
            let allowance = if emote == Emote::Bow { 5e-2 } else { 2e-3 };
            for &joint in &carried {
                let moved = gestured.positions[joint].distance(planted.positions[joint]);
                assert!(
                    moved < allowance,
                    "joint {joint} ({:?}) carries the body and {emote:?} moved it \
                     {:.1} mm at {through} — a leg belongs to the locomotion layer",
                    rig.joints[joint].zone,
                    moved * 1000.0
                );
            }
            let upper_moved = (0..posed.rotations.len())
                .filter(|&joint| !carried.contains(&joint))
                .filter(|&joint| apart(posed.rotations[joint], walking.rotations[joint]) > 1e-6)
                .count();
            if through == 0.5 {
                assert!(
                    upper_moved > 0,
                    "{emote:?} changed nothing at all mid-play — the gesture is not \
                     applying"
                );
            }
        }
    }
}

#[test]
fn kick_starts_one_build_and_tears_down_when_the_body_stops_being_rigged() {
    bevy::tasks::AsyncComputeTaskPool::get_or_init(Default::default);
    let mut app = test_app();
    let resolved = ResolvedRig {
        body: engine_default_for_did("did:plc:rigged-test"),
        attachments: Vec::new(),
    };
    app.insert_resource(LiveAvatarRecord(rigged_record(resolved)));
    let chassis = app
        .world_mut()
        .spawn((
            LocalPlayer,
            Transform::default(),
            GlobalTransform::default(),
        ))
        .id();

    app.world_mut()
        .run_system_once(kick_rigged_builds)
        .expect("runs");
    let build = app.world().get::<RiggedBuild>(chassis);
    assert!(build.is_some(), "a resolved rigged body kicks a build");

    // A second pass must not stack a second task.
    app.world_mut()
        .run_system_once(kick_rigged_builds)
        .expect("runs");
    assert!(app.world().get::<RiggedBuild>(chassis).is_some());

    // Switching the record back to a generator body drops the build and
    // every rigged residue. (The dropped task cancels with it.)
    app.insert_resource(LiveAvatarRecord(AvatarRecord::default_for_seed(7)));
    app.world_mut()
        .run_system_once(kick_rigged_builds)
        .expect("runs");
    assert!(app.world().get::<RiggedBuild>(chassis).is_none());
    assert!(app.world().get::<RiggedApplied>(chassis).is_none());
}

/// #1135: once a body is standing at the full atlas under the record it
/// was built from, the per-frame `AvatarRecord` deep compare stops.
///
/// Sequence: a chassis with a rigged record, its full-atlas body already
/// installed, and no editing going on — a peer standing in a room, which
/// is the state a session spends nearly all of its frames in.
#[test]
fn a_standing_full_atlas_body_latches_out_of_the_per_frame_compare() {
    bevy::tasks::AsyncComputeTaskPool::get_or_init(Default::default);
    let mut app = test_app();
    let body = engine_default_for_did("did:plc:rigged-test");
    let resolved = ResolvedRig {
        body: body.clone(),
        attachments: Vec::new(),
    };
    app.insert_resource(LiveAvatarRecord(rigged_record(resolved)));
    let chassis = app
        .world_mut()
        .spawn((
            LocalPlayer,
            Transform::default(),
            GlobalTransform::default(),
            // What a landed full-atlas build leaves behind.
            RiggedApplied {
                record: body,
                atlas: symbios_avatar::AvatarConfig::default().atlas,
            },
        ))
        .id();
    // …under a root, which is the other half of "already standing".
    app.world_mut()
        .spawn((RiggedRoot, Transform::default(), ChildOf(chassis)));

    // First pass reconciles and latches; it must not kick a build.
    app.world_mut()
        .run_system_once(kick_rigged_builds)
        .expect("runs");
    assert!(
        app.world().get::<RiggedBuild>(chassis).is_none(),
        "a matching full-atlas body must not be rebuilt"
    );
    assert!(
        app.world().get::<RiggedSteady>(chassis).is_some(),
        "a reconciled chassis did not latch — the deep compare runs every frame"
    );

    // Standing still keeps the latch.
    app.world_mut()
        .run_system_once(kick_rigged_builds)
        .expect("runs");
    assert!(app.world().get::<RiggedSteady>(chassis).is_some());
    assert!(app.world().get::<RiggedBuild>(chassis).is_none());
}

/// And the latch releases on a record edit, which is the failure a
/// cheaper gate would ship: an avatar that stops following its own
/// editor.
#[test]
fn editing_the_record_releases_the_latch_and_kicks_a_rebuild() {
    bevy::tasks::AsyncComputeTaskPool::get_or_init(Default::default);
    let mut app = test_app();
    let body = engine_default_for_did("did:plc:rigged-test");
    app.insert_resource(LiveAvatarRecord(rigged_record(ResolvedRig {
        body: body.clone(),
        attachments: Vec::new(),
    })));
    let chassis = app
        .world_mut()
        .spawn((
            LocalPlayer,
            Transform::default(),
            GlobalTransform::default(),
            RiggedApplied {
                record: body,
                atlas: symbios_avatar::AvatarConfig::default().atlas,
            },
        ))
        .id();
    app.world_mut()
        .spawn((RiggedRoot, Transform::default(), ChildOf(chassis)));
    app.world_mut()
        .run_system_once(kick_rigged_builds)
        .expect("latching pass");
    assert!(app.world().get::<RiggedSteady>(chassis).is_some());

    // A different body under the same chassis — an editor slider, or a
    // peer's next broadcast.
    app.insert_resource(LiveAvatarRecord(rigged_record(ResolvedRig {
        body: engine_default_for_did("did:plc:someone-else"),
        attachments: Vec::new(),
    })));
    app.world_mut()
        .run_system_once(kick_rigged_builds)
        .expect("runs");
    assert!(
        app.world().get::<RiggedBuild>(chassis).is_some(),
        "the record changed and no rebuild was kicked — the latch swallowed the edit"
    );
}

/// The settle ladder (#1059) is time-driven with no record change behind
/// it, so a DRAFT-atlas body must keep being re-evaluated. This is the
/// case the issue warns a pure `Changed<>` gate would break, and the
/// reason the latch requires the full atlas rather than merely a match.
#[test]
fn a_draft_atlas_body_does_not_latch_so_the_settle_rung_still_arrives() {
    bevy::tasks::AsyncComputeTaskPool::get_or_init(Default::default);
    let mut app = test_app();
    let body = engine_default_for_did("did:plc:rigged-test");
    app.insert_resource(LiveAvatarRecord(rigged_record(ResolvedRig {
        body: body.clone(),
        attachments: Vec::new(),
    })));
    let chassis = app
        .world_mut()
        .spawn((
            LocalPlayer,
            Transform::default(),
            GlobalTransform::default(),
            RiggedApplied {
                record: body,
                atlas: DRAFT_ATLAS,
            },
        ))
        .id();
    app.world_mut()
        .spawn((RiggedRoot, Transform::default(), ChildOf(chassis)));

    app.world_mut()
        .run_system_once(kick_rigged_builds)
        .expect("runs");
    assert!(
        app.world().get::<RiggedSteady>(chassis).is_none(),
        "a draft-atlas body latched — its full-atlas rung is owed on a TIMER, \
         and a latched chassis would never look at the clock again"
    );
    // With no `RiggedSettle` stamped, `settled` is true immediately, so
    // the full-atlas rung is owed right now and gets kicked.
    assert!(
        app.world().get::<RiggedBuild>(chassis).is_some(),
        "the settle ladder's full-atlas rung was never claimed"
    );
}

#[test]
fn a_landed_body_hangs_off_an_offset_root_and_drives_to_a_pose() {
    let mut app = test_app();
    let chassis = app
        .world_mut()
        .spawn((Transform::default(), GlobalTransform::default()))
        .id();

    // Built directly at a draft atlas: the test owns the build so it can
    // afford one, and `install_built_body` is exactly what landing runs.
    let engine_record = engine_default_for_did("did:plc:rigged-test");
    let avatar = symbios_avatar::Avatar::build_with(
        &engine_record,
        &symbios_avatar::AvatarConfig {
            atlas: 128,
            ..Default::default()
        },
    )
    .expect("the seeded default engine body builds");
    let offset = 0.9;
    // `Avatar` withholds `Clone` on purpose (megabytes of texture), and a
    // closure system must be `FnMut` — so the one build is taken out of an
    // `Option` on the single run.
    let mut built = Some(avatar);
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  mut images: ResMut<Assets<Image>>,
                  mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>| {
                let Some(avatar) = built.take() else {
                    return;
                };
                install_built_body(
                    &mut commands,
                    chassis,
                    offset,
                    avatar,
                    &[],
                    &mut meshes,
                    &mut materials,
                    &mut images,
                    &mut bindposes,
                );
            },
        )
        .expect("runs");

    let mut roots = app
        .world_mut()
        .query_filtered::<(Entity, &Transform, &ChildOf), With<RiggedRoot>>();
    let (root, transform, child_of) = roots.single(app.world()).expect("one rigged root");
    assert_eq!(child_of.parent(), chassis);
    assert!(
        (transform.translation.y + offset).abs() < 1e-6,
        "the ground plane sits half the collider below the chassis centre"
    );
    assert!(
        !app.world()
            .get::<bevy_symbios_avatar::AvatarJoints>(root)
            .expect("joints spawned")
            .0
            .is_empty()
    );

    // Drive one frame: zero speed is the Idle source (#1067), which must
    // still write a pose — the body breathes and the blink is alive.
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(std::time::Duration::from_millis(16));
    app.world_mut()
        .run_system_once(drive_rigged_motion)
        .expect("runs");
    assert!(app.world().get::<AvatarPose>(root).is_some());
    assert!(app.world().get::<AvatarClosure>(root).is_some());
}

/// What one driven jump did, read off the drawn body.
struct Jumped {
    /// Whether the walk cycle ever advanced while the body was in flight.
    marched: bool,
    /// Whether a foot was ever at the floor while the body was in flight.
    planted: bool,
    /// How high the lowest foot got at the apex, in metres.
    highest: f32,
    /// Whether the landing was ever reached.
    landed: bool,
    /// How far the root sank below its standing height during the landing.
    sank: f32,
    /// How far the lowest foot went under the floor during the landing.
    buried: f32,
}

/// Drives one body through a whole jump on a chassis that moves the way
/// avian moves one — an impulse, then gravity, caught by a floor `ledge`
/// metres below the one it left — and reports what the drawn body did: whether the walk cycle ever advanced in the air,
/// whether a foot was ever planted in the air, how far the lowest foot
/// rose at the apex, and whether the landing was ever reached.
fn jumped(launch: f32, ledge: f32) -> Jumped {
    let mut app = test_app();
    let chassis = app
        .world_mut()
        .spawn((Transform::default(), GlobalTransform::default()))
        .id();
    let avatar = symbios_avatar::Avatar::build_with(
        &engine_default_for_did("did:plc:leap-test"),
        &symbios_avatar::AvatarConfig {
            atlas: 64,
            ..Default::default()
        },
    )
    .expect("the seeded default engine body builds");
    let mut built = Some(avatar);
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  mut images: ResMut<Assets<Image>>,
                  mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>| {
                let Some(avatar) = built.take() else {
                    return;
                };
                install_built_body(
                    &mut commands,
                    chassis,
                    0.9,
                    avatar,
                    &[],
                    &mut meshes,
                    &mut materials,
                    &mut images,
                    &mut bindposes,
                );
            },
        )
        .expect("runs");
    let mut roots = app.world_mut().query_filtered::<Entity, With<RiggedRoot>>();
    let root = roots.single(app.world()).expect("one rigged root");
    let rig = app
        .world()
        .get::<BuiltBody>(root)
        .expect("the body landed")
        .avatar
        .rig
        .clone();
    let feet: Vec<usize> = [Limb::HindLeft, Limb::HindRight]
        .into_iter()
        .map(|limb| rig.in_zone(symbios_avatar::Zone::Extremity(limb))[0])
        .collect();

    const STEP_SECS: f32 = 1.0 / 60.0;
    const GRAVITY: f32 = 9.81;
    // Walk in first, so the body is in a gait when it leaves the ground
    // and the walk cycle has something to advance.
    let mut at = Vec3::ZERO;
    let frame = |app: &mut App, at: Vec3| {
        let mut chassis_mut = app.world_mut().entity_mut(chassis);
        // Aimed into the travel the way the app's controller steers a
        // chassis (`looking_to`, whose −Z faces the movement), not a bare
        // translation — since the foothold ledger (engine #277) the drive
        // is handed the transform the pose renders under, and a chassis
        // marching +Z while facing world −Z is a permanent moonwalk no
        // app state produces: the ledger rightly holds its plants where
        // the RENDER puts them, and the old bare-translation harness read
        // that as half a metre of skate. Aimed, the chassis' π yaw and
        // the rigged root's own half turn (#1066) cancel, so the
        // instruments' plain `at + posed` world reads stay exact.
        let aimed = Transform::from_translation(at).looking_to(Vec3::Z, Vec3::Y);
        *chassis_mut.get_mut::<Transform>().unwrap() = aimed;
        *chassis_mut.get_mut::<GlobalTransform>().unwrap() = GlobalTransform::from(aimed);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(STEP_SECS));
        app.world_mut()
            .run_system_once(drive_rigged_motion)
            .expect("runs");
    };
    for _ in 0..90 {
        at += Vec3::Z * (1.4 * STEP_SECS);
        frame(&mut app, at);
    }

    // The jump: the chassis rises and falls under gravity, travelling
    // forward all the while, and is caught at the height it left.
    // The floor the body arrives on, which for a step off a ledge is
    // below the one it left.
    let landing_y = at.y - ledge;
    let mut vertical = launch;
    let (mut marched, mut planted, mut highest, mut landed) = (false, false, f32::MIN, false);
    // How far the drawn ROOT sank below where it stands, and how far the
    // lowest foot went under the floor, once the body was back on it.
    let (mut sank, mut buried) = (0.0f32, 0.0f32);
    for _ in 0..600 {
        vertical -= GRAVITY * STEP_SECS;
        at += Vec3::new(0.0, vertical * STEP_SECS, 1.4 * STEP_SECS);
        let caught = at.y <= landing_y;
        if caught {
            at.y = landing_y;
            vertical = 0.0;
        }
        let before = app.world().get::<RiggedMotion>(root).expect("motion").cycle;
        frame(&mut app, at);
        let motion = app.world().get::<RiggedMotion>(root).expect("motion");
        let source = motion.source;
        let in_flight = motion
            .airborne
            .is_some_and(|air| !air.leap.stage_at(&rig, air.elapsed).is_grounded());
        landed |= motion.airborne.is_some_and(|air| air.landed);
        if in_flight {
            marched |= (motion.cycle - before).abs() > 1e-6;
            let pose = &app.world().get::<AvatarPose>(root).expect("a pose").0;
            let posed = pose.forward(&rig);
            let lowest = feet
                .iter()
                .map(|&joint| posed.positions[joint].y)
                .fold(f32::MAX, f32::min);
            highest = highest.max(lowest);
            // A foot at the floor while the body is in the air is a foot
            // the plant grabbed.
            planted |= lowest.abs() < 1e-4;
        }
        if motion.airborne.is_some_and(|air| air.landed) {
            let pose = &app.world().get::<AvatarPose>(root).expect("a pose").0;
            sank = sank.max(-pose.translation.y);
            let posed = pose.forward(&rig);
            let lowest = feet
                .iter()
                .map(|&joint| posed.positions[joint].y)
                .fold(f32::MAX, f32::min);
            buried = buried.max(-lowest);
        }
        // Once the body has landed and stood up, the jump is over.
        if caught && source != MotionSource::Leap && landed {
            break;
        }
    }
    assert!(highest > f32::MIN, "the body never went airborne at all");
    Jumped {
        marched,
        planted,
        highest,
        landed,
        sank,
        buried,
    }
}

#[test]
fn a_body_in_the_air_does_not_march_or_plant_a_foot() {
    // **#1072, and the apex is the whole of it.** This file called a body
    // airborne when |v_y| exceeded 3.5 m/s, which is TRUE at launch, FALSE
    // through the middle of the jump — at the apex the vertical speed is
    // zero, which is the most airborne a body ever is — and true again on
    // the way down. So the walk cycle resumed and the feet were planted at
    // the top of every jump. Airborne had to become a state.
    //
    // Driven at the preset's own launch speed: 450 N·s of impulse on an
    // 80 kg humanoid is 5.6 m/s, which is a 1.6 m apex and 1.15 s of
    // flight — a long time to be marching.
    let jump = jumped(5.6, 0.0);
    let Jumped {
        marched,
        planted,
        highest,
        landed,
        ..
    } = jump;
    assert!(
        !marched,
        "the walk cycle advanced while the body was in the air"
    );
    assert!(
        !planted,
        "a foot was planted on the floor while the body was in the air"
    );
    // The tuck: the engine draws the feet up a fifth of the leg's reach at
    // mid-flight, so a foot at the apex sits well above where it stands.
    assert!(
        highest > 0.05,
        "the feet never drew up under the body: the highest the lowest \
         foot reached was {:.1} mm above the floor",
        highest * 1000.0
    );
    assert!(landed, "the leap never reached its landing");
}

#[test]
fn a_landing_bends_the_legs_and_does_not_bury_the_body() {
    // **#1073, reported as 'the landings end up underground' and it was.**
    // A landing was built as `Leap::falling(drop)`, whose height carries
    // `-drop` for the whole stage — because in the engine's model the body
    // really is that much lower, having landed on a floor below the one it
    // left. Here the chassis has already carried it down, so the two added
    // and the root went under by the entire fall height: 1.6 m on the
    // preset's own jump. The legs then could not reach up to the floor and
    // the body stayed buried through the landing.
    //
    // Two readings, both of the symptom rather than of the arithmetic
    // behind it: how far the ROOT sank, which must be a leg compressing
    // and no more, and how far the lowest FOOT went under the floor, which
    // must be nothing.
    let jump = jumped(5.6, 0.0);
    assert!(jump.landed, "the leap never reached its landing");
    // The engine clamps a leg to `MAX_SQUASH` = 0.45 of its own reach, and
    // this body's legs are about 0.9 m, so half a metre is past anything a
    // landing can legitimately ask for and nowhere near the 1.6 m the bug
    // produced.
    assert!(
        jump.sank < 0.5,
        "the root sank {:.0} mm into the floor on landing — a leg compressing \
         cannot account for that",
        jump.sank * 1000.0,
    );
    assert!(
        jump.buried < 0.01,
        "a foot ended {:.0} mm under the floor on landing",
        jump.buried * 1000.0,
    );
    // And the landing must actually be a landing: a body that absorbs
    // nothing has not bent its legs at all.
    assert!(
        jump.sank > 0.02,
        "the root sank {:.0} mm — the landing is not absorbing anything",
        jump.sank * 1000.0,
    );
}

#[test]
fn stepping_off_a_ledge_flies_before_it_lands() {
    // The second defect in the same line (#1073). A body that walks off an
    // edge never launches, and `Leap::new(0.0)` has a flight of
    // `(0 + sqrt(0)) / g` — zero — so `stage_at` divided by an epsilon and
    // reported a LANDING from the first airborne frame: feet planted in
    // mid-air, all the way down. Built from the speed instead, a fall gets
    // a real arc.
    //
    // Driven with no launch at all, which is the case the jump test cannot
    // see because its launch is 5.6 m/s.
    // Two metres down, and no push off the edge at all.
    let fall = jumped(0.0, 2.0);
    assert!(
        !fall.planted,
        "a foot was planted in mid-air while the body was falling"
    );
    assert!(
        !fall.marched,
        "the walk cycle advanced while the body was falling"
    );
    assert!(fall.landed, "the fall never reached its landing");
    assert!(
        fall.buried < 0.01,
        "a foot ended {:.0} mm under the floor after a fall",
        fall.buried * 1000.0,
    );
}

/// Drives one body through deep water at `pace` and reports what the drawn
/// body did: whether a foot was ever planted, and how far the trunk was
/// pitched onto its front at the end.
///
/// **No walk-cycle reading here, deliberately.** `RiggedMotion::cycle` is
/// the stroke's clock while a body is swimming, so it advancing proves
/// nothing either way; what says no gait ran is the source assertion
/// inside the loop and the planted foot below it.
///
/// The pitch is read as the angle between the body's own long axis — root
/// to head on the posed skeleton — and the world's vertical, so it is a
/// property of the drawn body rather than a number handed back by the
/// thing under test.
fn swam(pace: f32) -> (bool, f32) {
    let mut app = test_app();
    // A pond whose surface is well above the body: a chassis at y = 0 with
    // a 1.8 m capsule has its head at 0.9, so a surface at 5 m is
    // unambiguously deep water.
    app.insert_resource(crate::water::WaterSurfaces {
        planes: vec![crate::water::WaterPlane {
            world_from_local: Transform::from_xyz(0.0, 5.0, 0.0),
            local_half_extents: Vec2::splat(200.0),
            flow_strength: 0.0,
            owner: crate::water::WaterPlane::NO_OWNER,
        }],
    });
    let chassis = app
        .world_mut()
        .spawn((Transform::default(), GlobalTransform::default()))
        .id();
    let avatar = symbios_avatar::Avatar::build_with(
        &engine_default_for_did("did:plc:swim-test"),
        &symbios_avatar::AvatarConfig {
            atlas: 64,
            ..Default::default()
        },
    )
    .expect("the seeded default engine body builds");
    let mut built = Some(avatar);
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  mut images: ResMut<Assets<Image>>,
                  mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>| {
                let Some(avatar) = built.take() else {
                    return;
                };
                install_built_body(
                    &mut commands,
                    chassis,
                    0.9,
                    avatar,
                    &[],
                    &mut meshes,
                    &mut materials,
                    &mut images,
                    &mut bindposes,
                );
            },
        )
        .expect("runs");
    let mut roots = app.world_mut().query_filtered::<Entity, With<RiggedRoot>>();
    let root = roots.single(app.world()).expect("one rigged root");
    let rig = app
        .world()
        .get::<BuiltBody>(root)
        .expect("the body landed")
        .avatar
        .rig
        .clone();
    let feet: Vec<usize> = [Limb::HindLeft, Limb::HindRight]
        .into_iter()
        .map(|limb| rig.in_zone(symbios_avatar::Zone::Extremity(limb))[0])
        .collect();
    let pelvis = rig
        .joints
        .iter()
        .position(|joint| joint.parent.is_none())
        .expect("a root joint");
    let head = *rig
        .in_zone(symbios_avatar::Zone::Head)
        .first()
        .expect("a head");

    const STEP_SECS: f32 = 1.0 / 60.0;
    let mut at = Vec3::ZERO;
    let mut planted = false;
    let mut pitch = 0.0f32;
    for _ in 0..180 {
        at += Vec3::Z * (pace * STEP_SECS);
        let mut chassis_mut = app.world_mut().entity_mut(chassis);
        // Aimed into the travel the way the app's controller steers a
        // chassis (`looking_to`, whose −Z faces the movement), not a bare
        // translation — since the foothold ledger (engine #277) the drive
        // is handed the transform the pose renders under, and a chassis
        // marching +Z while facing world −Z is a permanent moonwalk no
        // app state produces: the ledger rightly holds its plants where
        // the RENDER puts them, and the old bare-translation harness read
        // that as half a metre of skate. Aimed, the chassis' π yaw and
        // the rigged root's own half turn (#1066) cancel, so the
        // instruments' plain `at + posed` world reads stay exact.
        let aimed = Transform::from_translation(at).looking_to(Vec3::Z, Vec3::Y);
        *chassis_mut.get_mut::<Transform>().unwrap() = aimed;
        *chassis_mut.get_mut::<GlobalTransform>().unwrap() = GlobalTransform::from(aimed);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(STEP_SECS));
        app.world_mut()
            .run_system_once(drive_rigged_motion)
            .expect("runs");
        let motion = app.world().get::<RiggedMotion>(root).expect("motion");
        assert_eq!(
            motion.source,
            MotionSource::Swim,
            "a body under the surface must be swimming"
        );
        let pose = &app.world().get::<AvatarPose>(root).expect("a pose").0;
        let posed = pose.forward(&rig);
        planted |= feet
            .iter()
            .any(|&joint| posed.positions[joint].y.abs() < 1e-4);
        let along = posed.positions[head] - posed.positions[pelvis];
        pitch = along
            .normalize_or(Vec3::Y)
            .dot(Vec3::Y)
            .clamp(-1.0, 1.0)
            .acos();
    }
    (planted, pitch.to_degrees())
}

#[test]
fn a_swimming_body_treads_at_rest_and_lies_down_to_travel() {
    // **#1074.** The controller has had three water modes since the
    // humanoid locomotion work and the animation layer knew about none of
    // them: a body crossing a pond walked through it, striding at whatever
    // its horizontal speed implied.
    //
    // The engine's swim is one axis from a tread to a crawl, so the claim
    // worth guarding is that BOTH ends arrive: a body holding station
    // hangs upright and sculls, and a travelling one lies along the water.
    // A caller that pinned the effort would get one of the two and pass any
    // test that only looked at the other.
    let (planted, upright) = swam(0.0);
    assert!(!planted, "a treading body planted a foot on the bottom");
    // Full effort on this body is 0.7 of its length a second — about 1.2
    // m/s — so 1.4 is comfortably prone.
    let (_, prone) = swam(1.4);
    assert!(
        upright < 25.0,
        "a body treading water hung {upright:.0} deg off vertical — a tread \
         is upright"
    );
    assert!(
        prone > 60.0,
        "a body swimming at 1.4 m/s lay {prone:.0} deg off vertical — a \
         crawl lies along the water"
    );
}

#[test]
fn a_standing_body_breathes_instead_of_freezing() {
    // **The statue regression, which is what #1067 risked.** Under the
    // clips a standing body played Idle_A; with them gone the replacement
    // is the engine's idle (#246), and the failure mode of forgetting to
    // wire it is silent: `Rest` writes a perfectly valid pose every frame
    // — the same one, forever. So the guard is change over time, read off
    // the drawn pose: a breathing body's joints move between frames a
    // second apart, a statue's do not. Blinking cannot satisfy it — the
    // eyes are geometry closure, not pose rotations.
    let mut app = test_app();
    let chassis = app
        .world_mut()
        .spawn((Transform::default(), GlobalTransform::default()))
        .id();
    let avatar = symbios_avatar::Avatar::build_with(
        &engine_default_for_did("did:plc:idle-test"),
        &symbios_avatar::AvatarConfig {
            atlas: 64,
            ..Default::default()
        },
    )
    .expect("the seeded default engine body builds");
    let mut built = Some(avatar);
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  mut images: ResMut<Assets<Image>>,
                  mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>| {
                let Some(avatar) = built.take() else {
                    return;
                };
                install_built_body(
                    &mut commands,
                    chassis,
                    0.9,
                    avatar,
                    &[],
                    &mut meshes,
                    &mut materials,
                    &mut images,
                    &mut bindposes,
                );
            },
        )
        .expect("runs");
    let mut roots = app.world_mut().query_filtered::<Entity, With<RiggedRoot>>();
    let root = roots.single(app.world()).expect("one rigged root");

    // The chassis never moves; the body is standing. Sampled once a
    // second for several seconds, because a breath is slow — adjacent
    // 16 ms frames of a breathing body are nearly identical too.
    let mut samples: Vec<Pose> = Vec::new();
    for frame in 0..240 {
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(1.0 / 60.0));
        app.world_mut()
            .run_system_once(drive_rigged_motion)
            .expect("runs");
        if frame % 60 == 0 {
            samples.push(
                app.world()
                    .get::<AvatarPose>(root)
                    .expect("a pose")
                    .0
                    .clone(),
            );
        }
    }
    let motion = app.world().get::<RiggedMotion>(root).expect("motion state");
    assert_eq!(
        motion.source,
        MotionSource::Idle,
        "a standing body's source must be the idle"
    );
    let apart = |a: Quat, b: Quat| 1.0 - a.dot(b).abs();
    let moved = samples
        .windows(2)
        .map(|pair| {
            (0..pair[0].rotations.len())
                .map(|joint| apart(pair[0].rotations[joint], pair[1].rotations[joint]))
                .fold(0.0f32, f32::max)
        })
        .fold(0.0f32, f32::max);
    assert!(
        moved > 1e-6,
        "four seconds of standing drew a bit-identical skeleton — the idle is not \
         running and every standing body in the room is a statue"
    );
}

#[test]
fn a_chat_keyword_changes_the_pose_the_body_is_actually_drawn_in() {
    // **End to end through the real systems** (#1068), because every other
    // test here proves a piece: the keyword scan, the targeting, the
    // cooldown and the overlay arithmetic each pass on their own while the
    // wiring between them could still be wrong. This is the one that fails
    // if `drive_rigged_motion` never reaches the gesture branch — the exact
    // defect that would otherwise only show up in the running app.
    let mut app = test_app();
    let chassis = app
        .world_mut()
        .spawn((Transform::default(), GlobalTransform::default()))
        .id();
    let avatar = symbios_avatar::Avatar::build_with(
        &engine_default_for_did("did:plc:emote-drive-test"),
        &symbios_avatar::AvatarConfig {
            atlas: 128,
            ..Default::default()
        },
    )
    .expect("the seeded default engine body builds");
    let mut built = Some(avatar);
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  mut images: ResMut<Assets<Image>>,
                  mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>| {
                let Some(avatar) = built.take() else {
                    return;
                };
                install_built_body(
                    &mut commands,
                    chassis,
                    0.9,
                    avatar,
                    &[],
                    &mut meshes,
                    &mut materials,
                    &mut images,
                    &mut bindposes,
                );
            },
        )
        .expect("runs");
    let mut roots = app.world_mut().query_filtered::<Entity, With<RiggedRoot>>();
    let root = roots.single(app.world()).expect("one rigged root");

    // A frame with no keyword: the idle baseline this is measured against.
    let frame = |app: &mut App| {
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_millis(16));
        app.world_mut().run_system_once(start_emotes).expect("runs");
        app.world_mut()
            .run_system_once(drive_rigged_motion)
            .expect("runs");
        app.world()
            .get::<AvatarPose>(root)
            .expect("a pose")
            .0
            .clone()
    };
    let idle = frame(&mut app);

    // Now say hello, through the same helper the chat and network layers
    // call — not by poking `RiggedMotion` directly, which would skip the
    // half of the path most likely to be miswired.
    let request = crate::player::emote::request_for(chassis, "hello!")
        .expect("\"hello\" asks for a greeting");
    app.world_mut().write_message(request);
    let waving = frame(&mut app);

    assert!(
        app.world()
            .get::<RiggedMotion>(root)
            .expect("motion state")
            .gesture
            .is_some(),
        "the greeting never started"
    );
    let apart = |a: Quat, b: Quat| 1.0 - a.dot(b).abs();
    let moved = (0..idle.rotations.len())
        .filter(|joint| apart(idle.rotations[*joint], waving.rotations[*joint]) > 1e-6)
        .count();
    assert!(
        moved > 0,
        "the body was drawn in the same pose with and without the greeting"
    );
}

/// **#1106, reproduced.** Selecting a PART of a worn item snapped the
/// body to the bind pose: the part had just been detached to world
/// space at its animated pose, so its parent moved out from under it
/// and the selection itself visibly shifted the part. A part selection
/// must hold the body exactly where it stands — the pose the driver
/// wrote last frame is the pose it leaves on the root, bit for bit —
/// while a whole-prop selection still pins the bind pose (#1062).
#[test]
fn selecting_a_part_holds_the_pose_as_it_stands() {
    let mut app = test_app();
    let chassis = app
        .world_mut()
        .spawn((
            crate::state::LocalPlayer,
            Transform::default(),
            GlobalTransform::default(),
        ))
        .id();
    let avatar = symbios_avatar::Avatar::build_with(
        &engine_default_for_did("did:plc:part-hold-test"),
        &symbios_avatar::AvatarConfig {
            atlas: 64,
            ..Default::default()
        },
    )
    .expect("the seeded default engine body builds");
    let mut built = Some(avatar);
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  mut images: ResMut<Assets<Image>>,
                  mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>| {
                let Some(avatar) = built.take() else {
                    return;
                };
                install_built_body(
                    &mut commands,
                    chassis,
                    0.9,
                    avatar,
                    &[],
                    &mut meshes,
                    &mut materials,
                    &mut images,
                    &mut bindposes,
                );
            },
        )
        .expect("runs");
    let mut roots = app.world_mut().query_filtered::<Entity, With<RiggedRoot>>();
    let root = roots.single(app.world()).expect("one rigged root");
    let rig = app
        .world()
        .get::<BuiltBody>(root)
        .expect("the body landed")
        .avatar
        .rig
        .clone();

    // Walk into a mid-stride pose, as the #1069 test does.
    const STEP_SECS: f32 = 1.0 / 60.0;
    const PACE: f32 = 1.3;
    for frame in 0..90 {
        let at = Vec3::Z * (PACE * STEP_SECS * frame as f32);
        let mut chassis_mut = app.world_mut().entity_mut(chassis);
        // Aimed into the travel the way the app's controller steers a
        // chassis (`looking_to`, whose −Z faces the movement), not a bare
        // translation — since the foothold ledger (engine #277) the drive
        // is handed the transform the pose renders under, and a chassis
        // marching +Z while facing world −Z is a permanent moonwalk no
        // app state produces: the ledger rightly holds its plants where
        // the RENDER puts them, and the old bare-translation harness read
        // that as half a metre of skate. Aimed, the chassis' π yaw and
        // the rigged root's own half turn (#1066) cancel, so the
        // instruments' plain `at + posed` world reads stay exact.
        let aimed = Transform::from_translation(at).looking_to(Vec3::Z, Vec3::Y);
        *chassis_mut.get_mut::<Transform>().unwrap() = aimed;
        *chassis_mut.get_mut::<GlobalTransform>().unwrap() = GlobalTransform::from(aimed);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(STEP_SECS));
        app.world_mut()
            .run_system_once(drive_rigged_motion)
            .expect("runs");
    }
    let stride = app
        .world()
        .get::<AvatarPose>(root)
        .expect("a pose")
        .0
        .clone();
    let rest = Pose::rest(&rig);
    let drift = |a: &Pose, b: &Pose| {
        let (a, b) = (a.forward(&rig), b.forward(&rig));
        a.positions
            .iter()
            .zip(&b.positions)
            .map(|(p, q)| p.distance(*q))
            .fold(0.0_f32, f32::max)
    };
    assert!(
        drift(&stride, &rest) > 0.02,
        "the walk must have taken the body away from rest for the hold to mean anything"
    );

    // Select a PART: the pose must not move by a single bit.
    let mut editor = crate::ui::avatar::AvatarEditorState::default();
    editor.select_attachment_part_from_scene_pick(String::from("3jzfcijpj2z2a"), vec![0]);
    app.insert_resource(editor);
    for _ in 0..30 {
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(STEP_SECS));
        app.world_mut()
            .run_system_once(drive_rigged_motion)
            .expect("runs");
    }
    let held = &app.world().get::<AvatarPose>(root).expect("a pose").0;
    assert_eq!(
        drift(held, &stride),
        0.0,
        "a part selection re-posed the body (max joint travel {} m) — the detached part \
         was left behind by its own parent",
        drift(held, &stride)
    );

    // A WHOLE-prop selection still pins the bind pose (#1062).
    let mut editor = crate::ui::avatar::AvatarEditorState::default();
    editor.select_attachment_from_scene_pick(String::from("3jzfcijpj2z2a"));
    app.insert_resource(editor);
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(std::time::Duration::from_secs_f32(STEP_SECS));
    app.world_mut()
        .run_system_once(drive_rigged_motion)
        .expect("runs");
    let pinned = &app.world().get::<AvatarPose>(root).expect("a pose").0;
    assert_eq!(
        drift(pinned, &rest),
        0.0,
        "the whole-prop hold is the bind pose"
    );
}
