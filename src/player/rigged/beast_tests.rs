//! A beast through the fill, a rebuild and a peer's playout (#1805): the
//! game's own schedule shape, a body built at a low atlas, and the two
//! chassis a body hangs under.

use super::tests::finished;
use super::*;
use crate::pds::AvatarRecord;
use crate::pds::avatar::locomotion::LocomotionPreset as _;
use crate::pds::avatar::locomotion::{beast_record, default_beast};
use crate::pds::avatar::wardrobe::{EngineAvatarRecord, engine_default_for_did};
use crate::player::emote::Emote;
use crate::player::humanoid::{WaterColumn, WaterState, water_state};
use crate::state::{LiveAvatarRecord, LocalPlayer, RemotePeer};
use crate::water::{WaterPlane, WaterSurfaces};
use avian3d::prelude::LinearVelocity;
use bevy::ecs::system::RunSystemOnce;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::time::TimeUpdateStrategy;
use bevy_symbios_avatar::{AvatarBody, AvatarDriver, AvatarJoints, Drive};
use bevy_symbios_multiuser::prelude::*;
use std::time::Duration;
use symbios_avatar::anim::emote;
use symbios_avatar::plan::Family;
use symbios_avatar::{Avatar, AvatarConfig, Rig, Zone};

const FRAME: f32 = 1.0 / 60.0;
const FAMILIES: [Family; 3] = [Family::Dog, Family::Cat, Family::Horse];

/// Whose chassis a body hangs under: the two read their record and their
/// speed differently.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Under {
    Local,
    Peer,
}

/// The Avatar crate's own order, with the two systems of this module that
/// matter here in the places `PlayerPlugin` puts them, at a fixed sixtieth.
fn app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        bevy::asset::AssetPlugin::default(),
        bevy::transform::TransformPlugin,
        bevy_symbios_avatar::AvatarPlugin,
    ));
    app.init_asset::<Mesh>();
    app.init_asset::<StandardMaterial>();
    app.init_asset::<Image>();
    app.init_asset::<SkinnedMeshInverseBindposes>();
    app.init_resource::<crate::player::RigHold>();
    app.add_systems(
        Update,
        land_rigged_builds.in_set(bevy_symbios_avatar::AvatarSystems::Build),
    );
    app.add_systems(
        Update,
        fill_rigged_drive
            .before(bevy_symbios_avatar::drive_avatar_bodies)
            .in_set(bevy_symbios_avatar::AvatarSystems::Animate),
    );
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        FRAME,
    )));
    app
}

/// A record on foot in a person's upright capsule. A seeded record is on foot
/// only one time in six.
fn person_record(did: &str) -> AvatarRecord {
    let mut record = AvatarRecord::default_for_did(did);
    record.locomotion = crate::pds::HumanoidParams::default().into_config();
    record
}

fn peer(record: AvatarRecord) -> RemotePeer {
    RemotePeer {
        peer_id: serde_json::from_str("\"00000000-0000-0000-0000-000000000001\"")
            .expect("a well-formed uuid"),
        did: None,
        handle: None,
        muted: false,
        avatar: Some(record),
        build: None,
        connected_at: 0.0,
    }
}

/// A chassis at `at`, with `record` as its owner's or a peer's.
fn chassis(app: &mut App, under: Under, record: AvatarRecord, at: Vec3) -> Entity {
    let place = Transform::from_translation(at);
    match under {
        Under::Local => {
            app.insert_resource(LiveAvatarRecord(record));
            app.world_mut()
                .spawn((place, LocalPlayer, LinearVelocity::ZERO))
                .id()
        }
        Under::Peer => app.world_mut().spawn((place, peer(record))).id(),
    }
}

fn built(body: &EngineAvatarRecord, atlas: u32) -> Avatar {
    Avatar::build_with(
        body,
        &AvatarConfig {
            atlas,
            ..Default::default()
        },
    )
    .expect("the body builds")
}

/// Hands `chassis` a finished build of `body`, hung `offset` under it, and
/// runs the frame it lands in.
fn land(app: &mut App, chassis: Entity, body: &EngineAvatarRecord, atlas: u32, offset: f32) {
    let avatar = built(body, atlas);
    let task = finished(crate::offload::GenResult::Avatar(Some(Box::new(avatar))));
    app.world_mut().entity_mut(chassis).insert(RiggedBuild {
        target: body.clone(),
        atlas,
        offset,
        kicked_at: 0.0,
        announced: false,
        task,
    });
    app.update();
    assert!(app.world().get::<RiggedBuild>(chassis).is_none());
}

fn root_of(app: &mut App, chassis: Entity) -> Entity {
    let mut roots = app
        .world_mut()
        .query_filtered::<(Entity, &ChildOf), With<RiggedRoot>>();
    let found: Vec<Entity> = roots
        .iter(app.world())
        .filter(|(_, child_of)| child_of.parent() == chassis)
        .map(|(root, _)| root)
        .collect();
    assert_eq!(found.len(), 1, "one body under a chassis");
    found[0]
}

fn ask(app: &mut App, root: Entity, gesture: &str) {
    app.world_mut()
        .get_mut::<Drive>(root)
        .expect("a drive")
        .gesture = Some(gesture.to_string());
}

fn run(app: &mut App, seconds: f32) {
    for _ in 0..(seconds / FRAME).ceil() as usize {
        app.update();
    }
}

fn rig_of(app: &App, root: Entity) -> Rig {
    app.world()
        .get::<AvatarBody>(root)
        .expect("a body")
        .avatar
        .rig
        .clone()
}

fn gesture_of(app: &App, root: Entity) -> Option<String> {
    app.world()
        .get::<AvatarDriver>(root)
        .expect("a driver")
        .gesture()
        .map(|(name, _)| name.to_string())
}

fn gesture_under(app: &mut App, chassis: Entity) -> Option<String> {
    let root = root_of(app, chassis);
    gesture_of(app, root)
}

/// Every joint where it is drawn, in the world.
fn drawn(app: &App, root: Entity) -> Vec<Vec3> {
    app.world()
        .get::<AvatarJoints>(root)
        .expect("joints")
        .0
        .iter()
        .map(|&joint| {
            app.world()
                .get::<GlobalTransform>(joint)
                .expect("a placed joint")
                .translation()
        })
        .collect()
}

/// The furthest any joint moved between two readings of the same rig.
fn moved(from: &[Vec3], to: &[Vec3]) -> f32 {
    assert_eq!(from.len(), to.len());
    from.iter()
        .zip(to)
        .map(|(a, b)| a.distance(*b))
        .fold(0.0, f32::max)
}

/// The furthest any joint moves in a frame over `frames` frames, from
/// `was`, following the body under `chassis` wherever it lands.
fn worst_step(app: &mut App, chassis: Entity, was: &mut Vec<Vec3>, frames: usize) -> f32 {
    let mut worst = 0.0f32;
    for _ in 0..frames {
        app.update();
        let root = root_of(app, chassis);
        let now = drawn(app, root);
        worst = worst.max(moved(was, &now));
        *was = now;
    }
    worst
}

/// The pelvis' height over where it stands at rest: under zero in a sit.
fn pelvis_drop(app: &App, root: Entity, chassis: Entity) -> f32 {
    let rig = rig_of(app, root);
    let pelvis = rig
        .joints
        .iter()
        .position(|joint| joint.parent.is_none())
        .expect("a root joint");
    let ground = app
        .world()
        .get::<GlobalTransform>(root)
        .expect("a placed root")
        .translation()
        .y;
    let _ = chassis;
    drawn(app, root)[pelvis].y - ground - rig.joints[pelvis].position.y
}

/// **A sitting beast stays seated through the draft-to-full rebuild, and no
/// joint jumps** (#1805, D14), on its owner's client and on a peer's, through
/// the game's order of systems: the frame a build lands is drawn with the new
/// body already posed where the old one was. Getting up from there works, so
/// the seat is the driver's own and not a pose stuck on the body. Rebuilt
/// with a fresh driver, it stands up.
#[test]
fn a_sitting_beast_stays_seated_through_a_rebuild() {
    for under in [Under::Local, Under::Peer] {
        let record = beast_record(Family::Dog);
        let body = default_beast(Family::Dog);
        let offset = crate::player::visuals::rigged_root_offset(&record);
        let mut app = app();
        let chassis = chassis(&mut app, under, record, Vec3::Y * offset);
        land(&mut app, chassis, &body, 64, offset);
        run(&mut app, 0.5);
        let root = root_of(&mut app, chassis);
        let rig = rig_of(&app, root);
        ask(&mut app, root, "Sit");
        run(
            &mut app,
            emote::seconds(&rig, "Sit").expect("a state") + 0.5,
        );
        let drop = pelvis_drop(&app, root, chassis);
        assert!(drop < -0.03, "{under:?}: the dog sat only {drop:.3} m");
        let mut was = drawn(&app, root);
        let settled = worst_step(&mut app, chassis, &mut was, 30);

        land(&mut app, chassis, &body, 128, offset);
        let new_root = root_of(&mut app, chassis);
        assert_ne!(new_root, root, "{under:?}: the body was rebuilt");
        let landed = moved(&was, &drawn(&app, new_root));
        was = drawn(&app, new_root);
        let after = worst_step(&mut app, chassis, &mut was, 30);
        println!(
            "{under:?}: a frame's worst joint step: {:.2} mm seated, {:.2} landing, {:.2} after",
            settled * 1000.0,
            landed * 1000.0,
            after * 1000.0
        );
        let allowed = settled + 0.001;
        assert!(
            landed <= allowed && after <= allowed,
            "{under:?}: a joint jumped {:.1} mm, {:.1} mm after, where a seated frame moves \
             {:.1} mm",
            landed * 1000.0,
            after * 1000.0,
            settled * 1000.0
        );
        assert_eq!(
            gesture_of(&app, new_root).as_deref(),
            Some("Sit"),
            "{under:?}: the rebuild got the dog up"
        );
        let seated = pelvis_drop(&app, new_root, chassis);
        assert!(
            (seated - drop).abs() < 0.002,
            "{under:?}: the dog sat at {drop:.3} m and sits at {seated:.3} m rebuilt"
        );

        // And it is still a seat the driver holds: it can get up.
        ask(&mut app, new_root, emote::GET_UP);
        run(&mut app, 3.0);
        assert_eq!(
            gesture_of(&app, new_root),
            None,
            "{under:?}: did not get up"
        );
        let up = pelvis_drop(&app, new_root, chassis);
        assert!(up > -0.02, "{under:?}: got up to {up:.3} m");
    }
}

/// **A beast part way down carries on down** (#1805): the rebuild lands in the
/// middle of the sit, and nothing moves further than the sit itself does in a
/// frame.
#[test]
fn a_beast_going_down_carries_on_through_a_rebuild() {
    let record = beast_record(Family::Dog);
    let body = default_beast(Family::Dog);
    let offset = crate::player::visuals::rigged_root_offset(&record);
    let mut app = app();
    let chassis = chassis(&mut app, Under::Peer, record, Vec3::Y * offset);
    land(&mut app, chassis, &body, 64, offset);
    run(&mut app, 0.5);
    let root = root_of(&mut app, chassis);
    let rig = rig_of(&app, root);
    let down = emote::seconds(&rig, "Sit").expect("a state");
    ask(&mut app, root, "Sit");
    run(&mut app, down * 0.5);
    let mut was = drawn(&app, root);
    let sitting = worst_step(&mut app, chassis, &mut was, 3);
    land(&mut app, chassis, &body, 128, offset);
    let new_root = root_of(&mut app, chassis);
    let landed = moved(&was, &drawn(&app, new_root));
    assert!(
        landed < sitting * 1.5 + 0.001,
        "the landing moved a joint {:.1} mm, where the sit moves {:.1} mm a frame",
        landed * 1000.0,
        sitting * 1000.0
    );
    run(&mut app, down);
    assert_eq!(gesture_of(&app, new_root).as_deref(), Some("Sit"));
    assert!(pelvis_drop(&app, new_root, chassis) < -0.03);
}

/// **A person keeps the fresh driver a rebuild has always given them, and only
/// beasts carry** (#1805): a bow in progress ends with the old body, across
/// either change of kind too; and of two beasts in one room, only the one that
/// sat is seated after both rebuild (per entity, the viewer's #53 lesson).
#[test]
fn only_a_beast_carries_its_driver_and_only_its_own() {
    let person = engine_default_for_did("did:plc:rebuild-test");
    let beast = default_beast(Family::Dog);
    let mut app = app();
    let record = person_record("did:plc:rebuild-test");

    // A person's bow does not survive.
    let someone = chassis(&mut app, Under::Peer, record.clone(), Vec3::ZERO);
    land(&mut app, someone, &person, 64, 0.9);
    run(&mut app, 0.2);
    let root = root_of(&mut app, someone);
    ask(&mut app, root, Emote::Bow.gesture_name());
    run(&mut app, 0.2);
    assert!(gesture_of(&app, root).is_some(), "the person bows");
    land(&mut app, someone, &person, 128, 0.9);
    assert_eq!(
        gesture_under(&mut app, someone),
        None,
        "a rebuilt person has a fresh driver"
    );

    // A sitting beast turned person has nothing to sit with, and back.
    let swapped = chassis(&mut app, Under::Peer, record.clone(), Vec3::X * 5.0);
    land(&mut app, swapped, &beast, 64, 0.2);
    let root = root_of(&mut app, swapped);
    ask(&mut app, root, "Sit");
    run(&mut app, 0.2);
    assert_eq!(gesture_of(&app, root).as_deref(), Some("Sit"));
    land(&mut app, swapped, &person, 64, 0.9);
    let root = root_of(&mut app, swapped);
    assert_eq!(gesture_of(&app, root), None, "beast to person carries");
    ask(&mut app, root, Emote::Bow.gesture_name());
    run(&mut app, 0.2);
    assert!(gesture_of(&app, root).is_some());
    land(&mut app, swapped, &beast, 64, 0.2);
    assert_eq!(
        gesture_under(&mut app, swapped),
        None,
        "person to beast carries"
    );

    // Two beasts, one seated.
    let sitter = chassis(&mut app, Under::Peer, record.clone(), Vec3::Z * 5.0);
    let stander = chassis(&mut app, Under::Peer, record, Vec3::Z * -5.0);
    land(&mut app, sitter, &beast, 64, 0.2);
    land(&mut app, stander, &beast, 64, 0.2);
    let sitting = root_of(&mut app, sitter);
    ask(&mut app, sitting, "Sit");
    run(&mut app, 0.2);
    land(&mut app, stander, &beast, 128, 0.2);
    land(&mut app, sitter, &beast, 128, 0.2);
    assert_eq!(gesture_under(&mut app, sitter).as_deref(), Some("Sit"));
    assert_eq!(gesture_under(&mut app, stander), None);
}

/// A seat on `rig` from a driver that has only ever known it: the pose a
/// carried driver must come to.
fn seated_afresh(rig: &Rig) -> symbios_avatar::Pose {
    let mut driver = symbios_avatar::anim::driver::Driver::seeded(3);
    let mut pose = None;
    for frame in 0..600 {
        let inputs = symbios_avatar::anim::driver::Inputs {
            delta: FRAME,
            gesture: (frame == 0).then_some("Sit"),
            ..Default::default()
        };
        pose = driver
            .drive(rig, &inputs, symbios_avatar::anim::driver::level_ground)
            .map(|driven| driven.pose);
    }
    pose.expect("a pose")
}

/// **A driver carried to another beast's rig runs, and sits as that beast
/// does** (#1805): the reading of the engine that says carrying is safe,
/// driven. Round the families and some rolls, seated and then walking and
/// stopping on the new rig, where the idle takes its arrival from the pose
/// carried over. The pose drawn is the new rig's size and finite, and a seat
/// carried onto a new rig is that rig's own to the millimetre, so no rest
/// solved for the old rig is standing in for it. Every beast the engine rolls
/// has the same number of joints today, which this reports, so a change of
/// count is still only read and not driven.
#[test]
fn a_carried_driver_runs_on_any_other_beast() {
    let mut shapes: Vec<EngineAvatarRecord> = FAMILIES
        .iter()
        .map(|&family| default_beast(family))
        .collect();
    let mut counts = std::collections::BTreeSet::new();
    for seed in 0..40i64 {
        let mut record = default_beast(Family::Dog);
        record.reroll(seed);
        counts.insert(
            Rig::from_skeleton(&record.skeleton())
                .expect("a beast rigs")
                .len(),
        );
        if seed < 3 {
            shapes.push(record);
        }
    }
    println!("joint counts among the rolled beasts: {counts:?}");

    let mut app = app();
    let chassis = chassis(
        &mut app,
        Under::Peer,
        beast_record(Family::Dog),
        Vec3::Y * 0.2,
    );
    land(&mut app, chassis, &shapes[0], 64, 0.2);
    let check = |app: &mut App, when: &str| {
        let root = root_of(app, chassis);
        let rig = rig_of(app, root);
        let pose = &app
            .world()
            .get::<bevy_symbios_avatar::AvatarPose>(root)
            .expect("a pose")
            .0;
        assert_eq!(pose.rotations.len(), rig.len(), "{when}");
        assert!(pose.translation.is_finite(), "{when}");
        assert!(pose.rotations.iter().all(|turn| turn.is_finite()), "{when}");
    };
    for next in 1..=shapes.len() {
        let shape = shapes[next % shapes.len()].clone();
        // Seated, onto this rig.
        let root = root_of(&mut app, chassis);
        let sit = emote::seconds(&rig_of(&app, root), "Sit").expect("a state");
        ask(&mut app, root, "Sit");
        run(&mut app, sit + 0.5);
        land(&mut app, chassis, &shape, 64, 0.2);
        run(&mut app, 0.5);
        check(&mut app, "seated");
        let root = root_of(&mut app, chassis);
        assert_eq!(gesture_of(&app, root).as_deref(), Some("Sit"));
        let rig = rig_of(&app, root);
        let afresh = seated_afresh(&rig);
        let carried = &app
            .world()
            .get::<bevy_symbios_avatar::AvatarPose>(root)
            .expect("a pose")
            .0;
        let off = carried.translation.distance(afresh.translation);
        assert!(
            off < 0.001,
            "a seat carried onto rig {next} stands {:.1} mm from that rig's own",
            off * 1000.0
        );
        ask(&mut app, root, emote::GET_UP);
        run(&mut app, 4.0);
        assert_eq!(gesture_of(&app, root), None);
        // Walking, onto the same rig again, and stopping there.
        for _ in 0..30 {
            app.world_mut()
                .get_mut::<Transform>(chassis)
                .expect("a chassis")
                .translation
                .z += 1.2 * FRAME;
            app.update();
        }
        land(&mut app, chassis, &shape, 64, 0.2);
        check(&mut app, "walking");
        run(&mut app, 0.5);
        check(&mut app, "stopped");
    }
}

/// The dry ground at y = 0 under a peer hung by a body's build: the lowest
/// point of each of its feet, from the rig at rest, in the world.
fn lowest_foot(app: &App, root: Entity) -> f32 {
    let rig = rig_of(app, root);
    let ground = app
        .world()
        .get::<GlobalTransform>(root)
        .expect("a placed root")
        .translation()
        .y;
    rig.joints
        .iter()
        .filter(|joint| matches!(joint.zone, Zone::Extremity(_)) && !joint.marker)
        .map(|joint| ground + joint.position.y - joint.radius * joint.scale.y)
        .fold(f32::MAX, f32::min)
}

/// **A beast peer arrives standing on the ground** (#1805): hung by its own
/// collider under a chassis played out at its owner's height, far from the
/// origin, with its first frame read as no speed at all rather than a launch.
#[test]
fn a_beast_peer_arrives_on_the_ground() {
    for family in FAMILIES {
        let record = beast_record(family);
        let offset = crate::player::visuals::rigged_root_offset(&record);
        let mut app = app();
        let chassis = chassis(
            &mut app,
            Under::Peer,
            record,
            Vec3::new(120.0, offset, -80.0),
        );
        app.update();
        land(&mut app, chassis, &default_beast(family), 64, offset);
        let root = root_of(&mut app, chassis);
        run(&mut app, 0.1);
        let foot = lowest_foot(&app, root);
        assert!(
            (-0.015..=0.005).contains(&foot),
            "a {family:?} peer's feet stand {:+.1} mm off the ground",
            foot * 1000.0
        );
        let drive = app.world().get::<Drive>(root).expect("a drive");
        assert_eq!(drive.velocity, Vec3::ZERO, "a {family:?} arrives launched");
        let driver = app.world().get::<AvatarDriver>(root).expect("a driver");
        assert!(
            driver.airborne().is_none(),
            "a {family:?} arrives in the air"
        );
    }
}

/// **A peer's body stands on its chassis' ground when the shape and the body
/// disagree** (#1805). The locomotion is broadcast live and the body only on
/// Save, so a person turned beast plays out at the beast's height while the
/// person still hangs where their build put them; and turned back, the other
/// way. The hang follows the collider, each way, out and back, on either
/// client; and a person who has never lain is left where their build hung them
/// whatever the record says, to the bit.
#[test]
fn a_body_hangs_from_the_collider_its_record_says() {
    for under in [Under::Local, Under::Peer] {
        let person = person_record("did:plc:hang-test");
        let tall = crate::player::visuals::rigged_root_offset(&person);
        let lying = beast_record(Family::Dog);
        let low = crate::player::visuals::rigged_root_offset(&lying);
        assert!(low < tall * 0.5);

        let set = |app: &mut App, chassis: Entity, record: &AvatarRecord| match under {
            Under::Local => {
                app.insert_resource(LiveAvatarRecord(record.clone()));
            }
            Under::Peer => {
                app.world_mut()
                    .get_mut::<RemotePeer>(chassis)
                    .expect("a peer")
                    .avatar = Some(record.clone());
            }
        };
        let hang = |app: &App, root: Entity| {
            app.world()
                .get::<Transform>(root)
                .expect("a root")
                .translation
                .y
        };

        // A person, whose owner has just switched to four legs.
        let mut app = app();
        let chassis = chassis(&mut app, under, person.clone(), Vec3::Y * tall);
        let body = engine_default_for_did("did:plc:hang-test");
        land(&mut app, chassis, &body, 64, tall);
        run(&mut app, 0.1);
        let root = root_of(&mut app, chassis);
        assert_eq!(hang(&app, root).to_bits(), (-tall).to_bits());
        set(&mut app, chassis, &lying);
        app.update();
        assert_eq!(
            hang(&app, root),
            -low,
            "{under:?}: a person under a lying shape, in the ground"
        );
        // Switched back before saving.
        set(&mut app, chassis, &person);
        app.update();
        assert_eq!(hang(&app, root), -tall, "{under:?}: left at a beast's hang");

        // A person's capsule edit moves nothing until it is rebuilt, as ever.
        let mut taller = person.clone();
        let crate::pds::LocomotionConfig::Humanoid(p) = &mut taller.locomotion else {
            panic!("on foot")
        };
        p.capsule_length.0 += 0.2;
        set(&mut app, chassis, &taller);
        app.update();
        assert_eq!(
            hang(&app, root),
            -tall,
            "{under:?}: a person's hang moved for an upright record"
        );

        // A beast, whose owner has switched back: its feet stay on the ground
        // under a chassis gone up to a person's height.
        let mut app = crate::player::rigged::beast_tests::app();
        let chassis2 = self::chassis(&mut app, under, lying.clone(), Vec3::Y * low);
        land(&mut app, chassis2, &default_beast(Family::Dog), 64, low);
        run(&mut app, 0.1);
        let root = root_of(&mut app, chassis2);
        assert_eq!(hang(&app, root), -low);
        set(&mut app, chassis2, &person);
        app.update();
        assert_eq!(hang(&app, root), -tall, "{under:?}: a beast in the air");
    }
}

/// **A body lands hung where its record says now, not where it said at the
/// kick** (#1805). A build spans frames: a person's kicked while the record lay
/// would land at a beast's hang after their owner had switched back, and float,
/// with nothing owing a rebuild and an upright record the fill leaves alone.
#[test]
fn a_body_lands_at_the_hang_its_record_says_now() {
    for under in [Under::Local, Under::Peer] {
        let person = person_record("did:plc:landing-test");
        let tall = crate::player::visuals::rigged_root_offset(&person);
        let low = crate::player::visuals::rigged_root_offset(&beast_record(Family::Dog));
        let mut app = app();
        let chassis = chassis(&mut app, under, person, Vec3::Y * tall);
        let body = engine_default_for_did("did:plc:landing-test");
        land(&mut app, chassis, &body, 64, low);
        app.update();
        let root = root_of(&mut app, chassis);
        let hang = app
            .world()
            .get::<Transform>(root)
            .expect("a root")
            .translation
            .y;
        assert_eq!(hang.to_bits(), (-tall).to_bits(), "{under:?}: floating");
    }
}

/// How fast a slow peer plays out, and whether a resting beast rides it out:
/// the real jitter buffer fed what a sender under the throttle's line sends, a
/// sample every 30th tick of a chassis going one way at `creep` m/s, read at
/// `hz`, for `seconds`. Answers the fastest the playout travelled in a frame,
/// over the frame it took, and the fastest the fill read off it.
fn creeping_peer(
    creep: f32,
    hz: f64,
    seconds: f32,
    gesture: Option<&str>,
    body: &EngineAvatarRecord,
    record: AvatarRecord,
) -> (App, Entity, f32, f32) {
    #[derive(Resource, Default)]
    struct Fastest(f32, Option<Vec3>);
    fn play_out(
        time: Res<Time>,
        cfg: Res<crate::network::SmootherConfigRes>,
        mut peers: Query<(&mut Transform, &mut TransformBuffer)>,
        mut fastest: ResMut<Fastest>,
    ) {
        for (mut place, mut buffer) in &mut peers {
            if let Some((position, rotation)) = buffer.smoothed_at(time.elapsed_secs_f64(), &cfg.0)
            {
                place.translation = position;
                place.rotation = rotation;
                if let Some(was) = fastest.1 {
                    let along = (position - was) * Vec3::new(1.0, 0.0, 1.0);
                    fastest.0 = fastest.0.max(along.length() / time.delta_secs().max(1e-6));
                }
                fastest.1 = Some(position);
            }
        }
    }
    let mut app = app();
    let offset = crate::player::visuals::rigged_root_offset(&record);
    app.insert_resource(crate::network::SmootherConfigRes::default());
    app.init_resource::<Fastest>();
    app.add_systems(
        Update,
        play_out
            .before(fill_rigged_drive)
            .before(land_rigged_builds),
    );
    let at = Vec3::new(1000.0, offset, -2000.0);
    let chassis = chassis(&mut app, Under::Peer, record, at);
    app.world_mut()
        .entity_mut(chassis)
        .insert(TransformBuffer::default());
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
        1.0 / hz,
    )));
    land(&mut app, chassis, body, 64, offset);
    let cfg = app
        .world()
        .resource::<crate::network::SmootherConfigRes>()
        .0;
    let send = cfg.expected_send_interval_secs
        * f64::from(crate::config::network::STATIONARY_BROADCAST_DIVISOR);
    let root = root_of(&mut app, chassis);
    let mut sent = 0.0f64;
    let mut gone = 0.0f32;
    let mut feed = |app: &mut App, speed: f32| {
        let now = app.world().resource::<Time>().elapsed_secs_f64();
        while sent + send <= now {
            sent += send;
            gone += speed * send as f32;
            let accepted = app
                .world_mut()
                .get_mut::<TransformBuffer>(chassis)
                .expect("a buffer")
                .push_sample(at + Vec3::X * gone, Quat::IDENTITY, now, &cfg);
            assert!(accepted);
        }
    };
    // Settled, and only then asked to rest, and all the way down before the
    // chassis starts to go.
    for _ in 0..(1.5 * hz) as usize {
        feed(&mut app, 0.0);
        app.update();
    }
    if let Some(gesture) = gesture {
        ask(&mut app, root, gesture);
        for _ in 0..(3.0 * hz) as usize {
            feed(&mut app, 0.0);
            app.update();
        }
        assert_eq!(gesture_of(&app, root).as_deref(), Some(gesture));
    }
    let mut read = 0.0f32;
    for _ in 0..(f64::from(seconds) * hz) as usize {
        feed(&mut app, creep);
        app.update();
        let velocity = app.world().get::<Drive>(root).expect("a drive").velocity;
        read = read.max(Vec2::new(velocity.x, velocity.z).length());
    }
    let fastest = app.world().resource::<Fastest>().0;
    (app, root, fastest, read)
}

/// **A peer creeping down a slope does not get a resting beast up** (#1805). A
/// dog sat on 28 degrees creeps 12 mm/s with its owner's client reading that,
/// but a sender so slow sends twice a second, and the playout crosses most of
/// the gap in a frame: well over the engine's 0.1 m/s, at 60 and 144 Hz alike.
/// The beast stays down from there up to the speed the sender's throttle lets
/// go at, reading no speed at all, a long way past any one hold. A perfectly
/// still sender plays out at nothing. A person's differenced speed is not
/// touched: it reads what the playout does.
#[test]
fn a_creeping_peer_stays_at_rest() {
    let dog = default_beast(Family::Dog);
    for (creep, hz) in [(0.012, 60.0), (0.012, 144.0), (0.09, 60.0), (0.09, 144.0)] {
        let (app, root, played, read) =
            creeping_peer(creep, hz, 8.0, Some("Sit"), &dog, beast_record(Family::Dog));
        println!("creeping {creep} m/s at {hz} Hz plays out at up to {played:.3} m/s");
        assert!(
            played > 0.1,
            "the premise: over the engine's line, at {played:.3} m/s"
        );
        assert_eq!(
            gesture_of(&app, root).as_deref(),
            Some("Sit"),
            "a creep of {creep} m/s got a peer's dog up at {hz} Hz"
        );
        assert_eq!(read, 0.0, "the fill read a creep of {creep} m/s at {hz} Hz");
    }
    let (app, root, played, _) =
        creeping_peer(0.0, 60.0, 2.0, Some("Sit"), &dog, beast_record(Family::Dog));
    assert_eq!(played, 0.0);
    assert_eq!(gesture_of(&app, root).as_deref(), Some("Sit"));

    let person = engine_default_for_did("did:plc:still-test");
    let (_, _, played, read) = creeping_peer(
        0.012,
        60.0,
        2.0,
        None,
        &person,
        person_record("did:plc:still-test"),
    );
    assert!(played > 0.1);
    assert!(read > 0.1, "a person's differenced speed was held: {read}");
}

/// **A peer that really walks off gets up, and reads its real speed** (#1805):
/// the hold is on a chassis that has stopped, not on the beast. At a stride,
/// and at the slowest walk a slope's creep could not be: the time since it
/// stopped is what a creep is judged over, so a long rest costs a second
/// crossing of the hold at most.
#[test]
fn a_peer_walking_off_gets_up_at_its_real_speed() {
    let record = beast_record(Family::Dog);
    let offset = crate::player::visuals::rigged_root_offset(&record);
    let mut app = app();
    let chassis = chassis(&mut app, Under::Peer, record, Vec3::Y * offset);
    land(&mut app, chassis, &default_beast(Family::Dog), 64, offset);
    run(&mut app, 0.5);
    let root = root_of(&mut app, chassis);
    for (speed, late) in [(1.0, 5), (0.2, 20)] {
        ask(&mut app, root, "Sit");
        run(&mut app, 1.5);
        assert_eq!(gesture_of(&app, root).as_deref(), Some("Sit"));
        let mut frames_to_speed = None;
        for frame in 0..90 {
            app.world_mut()
                .get_mut::<Transform>(chassis)
                .expect("a chassis")
                .translation
                .z += speed * FRAME;
            app.update();
            let read = app.world().get::<Drive>(root).expect("a drive").velocity.z;
            if (read - speed).abs() < 0.01 && frames_to_speed.is_none() {
                frames_to_speed = Some(frame);
            }
        }
        let frames = frames_to_speed.expect("it never read its speed");
        assert!(
            frames <= late,
            "at {speed} m/s it read its speed {frames} frames late"
        );
        assert_eq!(
            gesture_of(&app, root),
            None,
            "a peer walking at {speed} m/s stays seated"
        );
        run(&mut app, 0.2);
    }

    // Stopped somewhere new, it is still again there.
    run(&mut app, 0.2);
    ask(&mut app, root, "Sit");
    run(&mut app, 1.5);
    assert_eq!(gesture_of(&app, root).as_deref(), Some("Sit"));
    for frame in 0..60 {
        app.world_mut()
            .get_mut::<Transform>(chassis)
            .expect("a chassis")
            .translation
            .x += if frame % 2 == 0 { 0.003 } else { -0.003 };
        app.update();
    }
    assert_eq!(
        gesture_of(&app, root).as_deref(),
        Some("Sit"),
        "a tremor got up a peer that had walked there"
    );
}

fn pond() -> WaterSurfaces {
    WaterSurfaces {
        planes: vec![WaterPlane {
            world_from_local: Transform::IDENTITY,
            local_half_extents: Vec2::splat(500.0),
            flow_strength: 0.0,
            owner: WaterPlane::NO_OWNER,
            cells: None,
        }],
    }
}

/// **A beast is drawn swimming at the depth its controller swims** (#1805):
/// up through the water and back down, on both clients and for every family,
/// the fill answers exactly as the controller's own classification does. Read
/// off an upright column, the dog was drawn swimming from 0.31 m with its
/// controller wading until 0.60.
#[test]
fn a_beast_is_drawn_swimming_where_its_controller_swims() {
    for under in [Under::Local, Under::Peer] {
        for family in FAMILIES {
            let record = beast_record(family);
            let crate::pds::LocomotionConfig::Humanoid(p) = &record.locomotion else {
                panic!("on foot")
            };
            let p: &crate::pds::HumanoidParams = p;
            let column = WaterColumn::of(p);
            let offset = crate::player::visuals::rigged_root_offset(&record);
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.init_resource::<crate::player::RigHold>();
            app.insert_resource(pond());
            let chassis = chassis(&mut app, under, record.clone(), Vec3::ZERO);
            let root = app
                .world_mut()
                .spawn((
                    RiggedRoot,
                    Drive::default(),
                    RiggedTrail::default(),
                    crate::player::visuals::rigged_root_transform(offset),
                    ChildOf(chassis),
                ))
                .id();
            let mut was = false;
            let mut swam_from = None;
            let steps = (0..=130).chain((0..=130).rev());
            for step in steps {
                let share = step as f32 / 100.0;
                let y = column.drop - share * column.height;
                let place = Transform::from_xyz(0.0, y, 0.0);
                let mut entity = app.world_mut().entity_mut(chassis);
                entity.insert(GlobalTransform::from(place));
                app.world_mut()
                    .resource_mut::<Time>()
                    .advance_by(Duration::from_secs_f32(FRAME));
                app.world_mut()
                    .run_system_once(fill_rigged_drive)
                    .expect("the fill runs");
                let drawn = app.world().get::<Drive>(root).expect("a drive").swimming;
                let surfaces = app.world().resource::<WaterSurfaces>();
                let controller = matches!(
                    water_state(was, y, Vec2::ZERO, column, surfaces),
                    WaterState::Swimming { .. }
                );
                assert_eq!(
                    drawn, controller,
                    "{under:?} {family:?} at {share:.2} of its height, was {was}"
                );
                if drawn && swam_from.is_none() {
                    swam_from = Some(share * column.height);
                }
                was = drawn;
            }
            let from = swam_from.expect("it swims");
            println!("a {family:?} is drawn swimming from {from:.2} m of water");
            assert!(from > column.height * 0.84);
        }
    }
}

/// **A beast surfaces with its head out** (#1805, #1324): anywhere its
/// controller has it afloat, from the float line to its back just awash, for
/// every family and some rolls, treading and swimming along, the whole of its
/// head clears the water. It reads by the column's top, not its capsule's: a
/// beast's capsule is under water for the whole of that range.
#[test]
fn a_beast_surfaces_with_its_head_out() {
    let mut bodies: Vec<(String, EngineAvatarRecord)> = FAMILIES
        .iter()
        .map(|&family| (format!("{family:?}"), default_beast(family)))
        .collect();
    for seed in 0..6i64 {
        let mut record = default_beast(Family::Dog);
        record.reroll(seed);
        bodies.push((format!("roll {seed}"), record));
    }
    for (name, body) in &bodies {
        for pace in [0.0, 1.0] {
            for share in [0.8, 1.0] {
                let mut record = AvatarRecord::default_for_did("did:plc:surface-test");
                record.locomotion =
                    crate::pds::avatar::locomotion::fit_on_foot(body, &Default::default())
                        .into_config();
                let crate::pds::LocomotionConfig::Humanoid(p) = &record.locomotion else {
                    panic!("on foot")
                };
                let p: &crate::pds::HumanoidParams = p;
                let column = WaterColumn::of(p);
                let floated = column.drop - share * column.height;
                let mut app = app();
                app.insert_resource(pond());
                let chassis = chassis(&mut app, Under::Peer, record, Vec3::Y * floated);
                land(&mut app, chassis, body, 64, column.drop);
                let root = root_of(&mut app, chassis);
                // In from the deep, as a swimmer comes up: the two thresholds
                // are a memory, and a body handed the float line cold wades.
                app.world_mut()
                    .get_mut::<Drive>(root)
                    .expect("a drive")
                    .swimming = true;
                let rig = rig_of(&app, root);
                let mut chin = f32::MAX;
                for frame in 0..240 {
                    app.world_mut()
                        .get_mut::<Transform>(chassis)
                        .expect("a chassis")
                        .translation
                        .z += pace * FRAME;
                    app.update();
                    if frame < 120 {
                        continue;
                    }
                    assert!(
                        app.world().get::<Drive>(root).expect("a drive").swimming,
                        "{name} stopped swimming"
                    );
                    let at = drawn(&app, root);
                    for (index, joint) in rig.joints.iter().enumerate() {
                        if joint.zone == Zone::Head && !joint.marker {
                            chin = chin.min(at[index].y - joint.radius * joint.scale.y);
                        }
                    }
                }
                let lift = app.world().get::<RiggedTrail>(root).expect("a trail").lift;
                println!(
                    "{name} afloat at {share} of its {:.3} m, {pace} m/s: chin {chin:+.3} m, \
                     lift {lift:.3}",
                    column.height
                );
                assert!(
                    chin > 0.0,
                    "{name} at {pace} m/s, {share} under, swims with its chin {:+.1} mm under \
                     the surface",
                    chin * 1000.0
                );
            }
        }
    }
}

/// **A beast's column says where it is afloat, and how far it may be lifted**
/// (#1805): a body's head can be under water while its back is still out only
/// where its column is much taller than its head reaches, which no beast rolled
/// so far is, so this one is given such a column. Its capsule is under water
/// and its column's top is not, so it is afloat; it is raised to hold its crown
/// over the water, by more than its capsule's radius.
#[test]
fn a_beast_afloat_is_read_by_the_top_of_its_column() {
    let mut record = beast_record(Family::Cat);
    let crate::pds::LocomotionConfig::Humanoid(p) = &mut record.locomotion else {
        panic!("on foot")
    };
    let radius = p.capsule_radius.0;
    p.shape = crate::pds::avatar::locomotion::OnFootShape::Lying {
        height: crate::pds::types::Fp(1.6),
        ahead: crate::pds::types::Fp(0.0),
    };
    let floated = radius - 1.5;
    let mut app = app();
    app.insert_resource(pond());
    let chassis = chassis(&mut app, Under::Peer, record, Vec3::Y * floated);
    land(&mut app, chassis, &default_beast(Family::Cat), 64, radius);
    let root = root_of(&mut app, chassis);
    app.world_mut()
        .get_mut::<Drive>(root)
        .expect("a drive")
        .swimming = true;
    run(&mut app, 3.0);
    let lift = app.world().get::<RiggedTrail>(root).expect("a trail").lift;
    println!("a cat in a tall column is lifted {lift:.3} m over a capsule of radius {radius:.3}");
    assert!(lift > radius * 1.5, "lifted only {lift:.3} m");
    let rig = rig_of(&app, root);
    let at = drawn(&app, root);
    let crown = rig
        .joints
        .iter()
        .enumerate()
        .filter(|(_, joint)| joint.zone == Zone::Head)
        .map(|(index, _)| at[index].y)
        .fold(f32::MIN, f32::max);
    assert!(crown > 0.05, "its crown is at {crown:+.3} m");
}

/// **What is carried is all of what the motion remembers** (#1805): a swimmer
/// in the depths where only having swum keeps it swimming stays swimming, a
/// lifted body keeps its lift and does not drop, and a peer walking through a
/// rebuild never reads as stopped: no frame leaves its gait.
#[test]
fn a_rebuild_keeps_a_beasts_swim_its_lift_and_its_stride() {
    // Between the thresholds, in water that does not start a swim.
    let record = beast_record(Family::Dog);
    let body = default_beast(Family::Dog);
    let crate::pds::LocomotionConfig::Humanoid(p) = &record.locomotion else {
        panic!("on foot")
    };
    let p: &crate::pds::HumanoidParams = p;
    let column = WaterColumn::of(p);
    let mut app = app();
    app.insert_resource(pond());
    let swimmer = chassis(
        &mut app,
        Under::Peer,
        record.clone(),
        Vec3::Y * (column.drop - 0.7 * column.height),
    );
    land(&mut app, swimmer, &body, 64, column.drop);
    let root = root_of(&mut app, swimmer);
    app.world_mut()
        .get_mut::<Drive>(root)
        .expect("a drive")
        .swimming = true;
    run(&mut app, 0.2);
    land(&mut app, swimmer, &body, 128, column.drop);
    let root = root_of(&mut app, swimmer);
    assert!(
        app.world().get::<Drive>(root).expect("a drive").swimming,
        "the rebuild forgot it was swimming"
    );

    // Lifted: raised, rebuilt, and drawn no lower for it.
    let mut tall = record.clone();
    let crate::pds::LocomotionConfig::Humanoid(p) = &mut tall.locomotion else {
        panic!("on foot")
    };
    p.shape = crate::pds::avatar::locomotion::OnFootShape::Lying {
        height: crate::pds::types::Fp(2.0),
        ahead: crate::pds::types::Fp(0.0),
    };
    let radius = p.capsule_radius.0;
    let mut app = self::app();
    app.insert_resource(pond());
    let lifted = chassis(&mut app, Under::Peer, tall, Vec3::Y * (radius - 1.9));
    land(&mut app, lifted, &body, 64, radius);
    let root = root_of(&mut app, lifted);
    app.world_mut()
        .get_mut::<Drive>(root)
        .expect("a drive")
        .swimming = true;
    run(&mut app, 3.0);
    assert!(app.world().get::<RiggedTrail>(root).expect("a trail").lift > 0.1);
    let was = app
        .world()
        .get::<Transform>(root)
        .expect("a root")
        .translation
        .y;
    land(&mut app, lifted, &body, 128, radius);
    let new_root = root_of(&mut app, lifted);
    let now = app
        .world()
        .get::<Transform>(new_root)
        .expect("a root")
        .translation
        .y;
    assert!(
        (now - was).abs() < 0.005,
        "the root hung at {was:.3} and hangs at {now:.3} rebuilt"
    );

    // A lifted beast with no on-foot record to hang by: its root is the only
    // thing that says where it hangs, lift and all.
    let mut app = self::app();
    app.insert_resource(pond());
    let lifted = chassis(&mut app, Under::Peer, record.clone(), Vec3::Y * -0.95);
    app.world_mut()
        .get_mut::<RemotePeer>(lifted)
        .expect("a peer")
        .avatar = None;
    land(&mut app, lifted, &body, 64, 1.0);
    let root = root_of(&mut app, lifted);
    app.world_mut()
        .get_mut::<Drive>(root)
        .expect("a drive")
        .swimming = true;
    run(&mut app, 3.0);
    let lift = app.world().get::<RiggedTrail>(root).expect("a trail").lift;
    assert!(lift > 0.1, "lifted only {lift:.3} m");
    let was = app
        .world()
        .get::<Transform>(root)
        .expect("a root")
        .translation
        .y;
    land(&mut app, lifted, &body, 128, 1.0);
    let new_root = root_of(&mut app, lifted);
    let now = app
        .world()
        .get::<Transform>(new_root)
        .expect("a root")
        .translation
        .y;
    assert!(
        (now - was).abs() < 0.005,
        "with no record, the root hung at {was:.3} and hangs at {now:.3} rebuilt"
    );

    // Walking through it.
    let offset = crate::player::visuals::rigged_root_offset(&record);
    let mut app = self::app();
    let walker = chassis(&mut app, Under::Peer, record, Vec3::Y * offset);
    land(&mut app, walker, &body, 64, offset);
    let stride = |app: &mut App| {
        app.world_mut()
            .get_mut::<Transform>(walker)
            .expect("a chassis")
            .translation
            .z += 1.0 * FRAME;
    };
    for _ in 0..60 {
        stride(&mut app);
        app.update();
    }
    let root = root_of(&mut app, walker);
    assert_eq!(
        app.world()
            .get::<AvatarDriver>(root)
            .expect("a driver")
            .source(),
        symbios_avatar::anim::driver::Source::Gait
    );
    stride(&mut app);
    land(&mut app, walker, &body, 128, offset);
    for _ in 0..5 {
        let root = root_of(&mut app, walker);
        assert_eq!(
            app.world()
                .get::<AvatarDriver>(root)
                .expect("a driver")
                .source(),
            symbios_avatar::anim::driver::Source::Gait,
            "a walking peer stopped for the rebuild"
        );
        stride(&mut app);
        app.update();
    }
}

/// How much a resting beast's own chassis trembles (#1805): the sender's half
/// of whether a peer's playout could stir it. `cargo nextest run
/// --cargo-profile test-release --run-ignored only -E
/// 'test(probe_a_resting_chassis)' --no-capture`.
#[test]
#[ignore = "a measurement, not a guard"]
fn probe_a_resting_chassis() {
    use crate::player::sim::WalkBench;
    for family in FAMILIES {
        let record = beast_record(family);
        for slope in [false, true] {
            let mut bench = if slope {
                let mut bench = WalkBench::new(&record, Vec3::new(0.0, 3.0, 0.0));
                bench.block(
                    Transform::from_xyz(0.0, 1.0, 0.0)
                        .with_rotation(Quat::from_rotation_x(10f32.to_radians())),
                    Vec3::new(30.0, 1.0, 30.0),
                );
                bench
            } else {
                WalkBench::standing(&record, 0.0, 0.0)
            };
            bench.run(3.0);
            let start = bench.position();
            let mut was = start;
            let (mut step, mut far) = (0.0f32, 0.0f32);
            for _ in 0..640 {
                bench.step();
                let now = bench.position();
                step = step.max(Vec2::new(now.x - was.x, now.z - was.z).length());
                far = far.max(Vec2::new(now.x - start.x, now.z - start.z).length());
                was = now;
            }
            println!(
                "a {family:?} at rest, slope {slope}: at y {:.3}, largest step {:.4} mm, furthest \
                 {:.4} mm in 10 s",
                start.y,
                step * 1000.0,
                far * 1000.0
            );
        }
    }
}

/// How many triangles a beast's fur costs, at every distance (#1805, D12): the
/// numbers for the owner. `cargo nextest run --cargo-profile test-release
/// --run-ignored only -E 'test(probe_a_beasts_triangles)' --no-capture`.
#[test]
#[ignore = "a measurement, not a guard"]
fn probe_a_beasts_triangles() {
    let faces = |mesh: &symbios_avatar::PolyMesh| {
        mesh.faces
            .iter()
            .map(|face| face.len().saturating_sub(2))
            .sum::<usize>()
    };
    let triangles = |avatar: &Avatar| {
        let mut count = std::collections::BTreeMap::<&str, usize>::new();
        for drawn in &avatar.meshes {
            *count.entry(drawn.kind.name()).or_default() += faces(&drawn.mesh);
        }
        count
    };
    for family in FAMILIES {
        let avatar = built(&default_beast(family), 256);
        println!(
            "default {family:?}: {:?}, far hair {}, fur {}",
            triangles(&avatar),
            avatar.parts.far_hair.is_some(),
            avatar.parts.fur.is_some()
        );
    }
    let mut rows: Vec<(usize, usize)> = Vec::new();
    for seed in 0..48i64 {
        let mut record = default_beast(Family::Dog);
        record.reroll(seed);
        let Some(avatar) = Avatar::build_with(
            &record,
            &AvatarConfig {
                atlas: 64,
                ..Default::default()
            },
        ) else {
            continue;
        };
        let count = triangles(&avatar);
        rows.push((
            count.get("hair").copied().unwrap_or(0),
            count.values().sum::<usize>(),
        ));
    }
    rows.sort_unstable();
    println!(
        "{} rolled beasts, (fur, total) triangles: least {:?}, median {:?}, most {:?}, heaviest total {:?}",
        rows.len(),
        rows.first(),
        rows.get(rows.len() / 2),
        rows.last(),
        rows.iter().map(|row| row.1).max()
    );
    let person = built(&engine_default_for_did("did:plc:triangle-test"), 256);
    println!("a seeded person, near: {:?}", triangles(&person));
    let far = Avatar::build_with(
        &engine_default_for_did("did:plc:triangle-test"),
        &AvatarConfig {
            atlas: 256,
            far_hair: true,
            ..Default::default()
        },
    )
    .expect("the body builds");
    println!(
        "its far hair: {:?}",
        far.parts
            .far_hair
            .as_ref()
            .map(|growth| faces(&growth.mesh))
    );
}

/// What `text` does to the body under `chassis`, through the table and the
/// system a chat line reaches a body by, and the frame that plays it.
fn say(app: &mut App, chassis: Entity, text: &str) {
    if let Some(request) = crate::player::emote::request_for(chassis, text) {
        app.world_mut().write_message(request);
    }
    app.world_mut()
        .run_system_once(start_emotes)
        .expect("start_emotes runs");
    app.update();
}

/// A body with the emote plumbing, stood still for a moment.
fn speaking(record: AvatarRecord, body: &EngineAvatarRecord) -> (App, Entity, Entity) {
    let offset = crate::player::visuals::rigged_root_offset(&record);
    let mut app = app();
    app.add_message::<crate::player::emote::EmoteRequest>();
    let chassis = chassis(&mut app, Under::Peer, record, Vec3::Y * offset);
    land(&mut app, chassis, body, 64, offset);
    run(&mut app, 0.2);
    let root = root_of(&mut app, chassis);
    (app, chassis, root)
}

/// **A chat line plays what the body that said it knows** (#1806): a beast's
/// words play a beast's gestures and a person's a person's, and neither's name
/// ever reaches the other body. A person's four are unchanged.
#[test]
fn a_chat_line_plays_what_its_body_knows() {
    for (text, plays) in [
        ("hello", Some("Wag")),
        ("yes", Some("Head Tilt")),
        ("thanks", Some("Play Bow")),
        ("sit", Some("Sit")),
        ("lie down", Some("Lie Down")),
        ("no", None),
        ("up", None),
        ("the weather is fine", None),
    ] {
        let (mut app, chassis, root) =
            speaking(beast_record(Family::Dog), &default_beast(Family::Dog));
        say(&mut app, chassis, text);
        assert_eq!(
            gesture_of(&app, root).as_deref(),
            plays,
            "a beast: {text:?}"
        );
    }
    let did = "did:plc:emote-test";
    for (text, plays) in [
        ("hello", Some("Greeting")),
        ("yes", Some("Head Nod")),
        ("no", Some("Reject")),
        ("thanks", Some("Bow")),
        ("sit", None),
        ("lie down", None),
        ("get up", None),
        ("the weather is fine", None),
    ] {
        let (mut app, chassis, root) = speaking(person_record(did), &engine_default_for_did(did));
        say(&mut app, chassis, text);
        assert_eq!(
            gesture_of(&app, root).as_deref(),
            plays,
            "a person: {text:?}"
        );
    }
}

/// **Every word a beast hears plays on every family, and is a beast's own**
/// (#1806): the engine falls through to a person's gesture on a beast, so a
/// name off a person's table would still resolve here. The beast's roster is
/// what says it is not one.
#[test]
fn every_beast_emote_is_a_gesture_of_its_own() {
    use crate::player::emote::BeastEmote;
    use symbios_avatar::anim::gesture;
    for family in FAMILIES {
        let rig = built(&default_beast(family), 64).rig;
        for emote in BeastEmote::ALL {
            let name = emote.gesture_name();
            assert!(
                gesture::for_body(&rig, name).is_some(),
                "{family:?}: {name}"
            );
            assert!(emote::ROSTER.contains(&name), "{name} is not a beast's");
        }
    }
}

/// **Told to sit, a beast stays down until it is told to get up, and gets up
/// where it is** (#1806): said in chat, as an owner says it, in either form.
#[test]
fn a_beast_told_to_sit_stays_until_told_to_get_up() {
    for (down, up) in [("sit", "get up"), ("lie down", "up")] {
        let (mut app, chassis, root) =
            speaking(beast_record(Family::Dog), &default_beast(Family::Dog));
        let at = app
            .world()
            .get::<Transform>(chassis)
            .expect("a chassis")
            .translation;
        say(&mut app, chassis, down);
        let told = gesture_of(&app, root).expect("it starts");
        run(&mut app, 8.0);
        assert_eq!(
            gesture_of(&app, root).as_deref(),
            Some(told.as_str()),
            "{down:?}"
        );
        say(&mut app, chassis, up);
        run(&mut app, 4.0);
        assert_eq!(gesture_of(&app, root), None, "still down after {up:?}");
        let now = app
            .world()
            .get::<Transform>(chassis)
            .expect("a chassis")
            .translation;
        assert_eq!(now, at, "{up:?} moved the chassis");
    }
}

/// The stage for what a beast wears: the game's own dresser after the fill,
/// on the spawn path it needs.
fn dressing_app() -> App {
    let mut app = crate::player::visuals::spawn_path_app();
    app.add_plugins((
        bevy::time::TimePlugin,
        bevy::transform::TransformPlugin,
        bevy_symbios_avatar::AvatarPlugin,
    ));
    app.init_resource::<crate::diagnostics::MetricsRegistry>();
    app.init_resource::<crate::player::RigHold>();
    app.add_systems(
        Update,
        (
            land_rigged_builds,
            crate::player::attachments::sync_rigged_attachments,
        )
            .chain()
            .in_set(bevy_symbios_avatar::AvatarSystems::Build),
    );
    app.add_systems(
        Update,
        fill_rigged_drive
            .before(bevy_symbios_avatar::drive_avatar_bodies)
            .in_set(bevy_symbios_avatar::AvatarSystems::Animate),
    );
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        FRAME,
    )));
    app
}

/// A record on foot wearing the catalogue's `slugs` on `body`.
fn wearing(family: Family, body: &EngineAvatarRecord, slugs: &[&str]) -> AvatarRecord {
    let mut record = AvatarRecord::wearing("3jzfcijpj2z2a");
    record.locomotion = beast_record(family).locomotion;
    let attachments = slugs
        .iter()
        .enumerate()
        .map(|(index, slug)| {
            let entry = crate::catalogue::ENTRIES
                .iter()
                .find(|entry| entry.slug() == *slug)
                .expect("a catalogue entry");
            let mut worn = crate::pds::avatar::AttachmentRecord::with_fit(
                entry.build("did:plc:wear"),
                entry.wear_socket().expect("wearable"),
                entry.wear_fit(),
            );
            worn.sanitize();
            crate::pds::avatar::ResolvedAttachment {
                rkey: format!("3jzfcijpj2z2{index}"),
                record: worn,
            }
        })
        .collect();
    if let Some(rig) = record.body.rigged_mut() {
        rig.resolved = Some(crate::pds::avatar::ResolvedRig {
            body: body.clone(),
            attachments,
        });
    }
    record
}

/// Every worn prop standing in the world: the joint it hangs from and its
/// place, in the order the joints are.
fn props_on(app: &mut App, root: Entity) -> Vec<(usize, Transform, GlobalTransform)> {
    let joints = app
        .world()
        .get::<AvatarJoints>(root)
        .expect("a body's joints")
        .0
        .clone();
    let mut query = app.world_mut().query_filtered::<(
        &ChildOf,
        &Transform,
        &GlobalTransform,
    ), With<crate::player::attachments::AttachmentRoot>>();
    let mut props: Vec<(usize, Transform, GlobalTransform)> = query
        .iter(app.world())
        .map(|(child_of, local, global)| {
            let joint = joints
                .iter()
                .position(|joint| *joint == child_of.parent())
                .expect("a prop on one of this body's joints");
            (joint, *local, *global)
        })
        .collect();
    props.sort_by_key(|(joint, _, _)| *joint);
    props
}

/// How many worn props stand in the whole world, on this body or any other.
fn props_anywhere(app: &mut App) -> usize {
    let mut query = app
        .world_mut()
        .query_filtered::<(), With<crate::player::attachments::AttachmentRoot>>();
    query.iter(app.world()).count()
}

/// **What a beast refuses stays off it, on its owner's client and a peer's,
/// and through a change of body** (#1807): a banner, a lantern and a satchel
/// on a record come to nothing on a beast and come back on the person it
/// turns into; the collar does the reverse. A saved outfit neither crashes
/// nor draws what it should not on the way.
#[test]
fn a_beast_and_a_person_each_leave_off_what_is_not_theirs() {
    let slugs = [
        "sashimono",
        "ships_lantern",
        "satchel",
        "beast_collar",
        "circlet",
    ];
    let person = engine_default_for_did("did:plc:wear-person");
    let beast = default_beast(Family::Dog);
    for under in [Under::Local, Under::Peer] {
        let record = wearing(Family::Dog, &beast, &slugs);
        let offset = crate::player::visuals::rigged_root_offset(&record);
        let mut app = dressing_app();
        let chassis = chassis(&mut app, under, record, Vec3::Y * offset);
        land(&mut app, chassis, &beast, 64, offset);
        app.update();
        assert_eq!(
            props_anywhere(&mut app),
            2,
            "{under:?}: a beast wears its collar and the circlet, and nothing else"
        );

        // The owner changes body: the record changes with it, as the editor
        // leaves it, and the build lands.
        let as_person = wearing(Family::Dog, &person, &slugs);
        match under {
            Under::Local => {
                app.insert_resource(LiveAvatarRecord(as_person));
            }
            Under::Peer => {
                app.world_mut()
                    .get_mut::<RemotePeer>(chassis)
                    .expect("a peer")
                    .avatar = Some(as_person);
            }
        }
        land(&mut app, chassis, &person, 64, offset);
        app.update();
        assert_eq!(
            props_anywhere(&mut app),
            4,
            "{under:?}: a person wears all but the collar"
        );

        // And back.
        let as_beast = wearing(Family::Dog, &beast, &slugs);
        match under {
            Under::Local => {
                app.insert_resource(LiveAvatarRecord(as_beast));
            }
            Under::Peer => {
                app.world_mut()
                    .get_mut::<RemotePeer>(chassis)
                    .expect("a peer")
                    .avatar = Some(as_beast);
            }
        }
        land(&mut app, chassis, &beast, 64, offset);
        app.update();
        assert_eq!(
            props_anywhere(&mut app),
            2,
            "{under:?}: back on the beast, the banner is off it again"
        );
    }
}

/// **Props survive a carried rebuild of a sitting beast** (#1807): the
/// draft-to-full rebuild replaces the root, the joints and everything hung on
/// them, while the driver is carried over. The fresh root is dressed the same
/// frame, every prop on one of its joints and in the place it was, and the
/// owner's still answer to the new root in the editor.
#[test]
fn what_a_sitting_beast_wears_survives_its_rebuild() {
    let slugs = ["beast_collar", "saddle_pad", "ear_bow"];
    for under in [Under::Local, Under::Peer] {
        let body = default_beast(Family::Dog);
        let record = wearing(Family::Dog, &body, &slugs);
        let offset = crate::player::visuals::rigged_root_offset(&record);
        let mut app = dressing_app();
        let chassis = chassis(&mut app, under, record, Vec3::Y * offset);
        land(&mut app, chassis, &body, 64, offset);
        run(&mut app, 0.5);
        let root = root_of(&mut app, chassis);
        let rig = rig_of(&app, root);
        ask(&mut app, root, "Sit");
        run(
            &mut app,
            emote::seconds(&rig, "Sit").expect("a state") + 0.5,
        );
        let seated = props_on(&mut app, root);
        assert_eq!(seated.len(), 3, "{under:?}: all three worn while it sits");

        land(&mut app, chassis, &body, 128, offset);
        let new_root = root_of(&mut app, chassis);
        assert_ne!(new_root, root, "{under:?}: the body was rebuilt");
        assert_eq!(
            props_anywhere(&mut app),
            3,
            "{under:?}: the rebuild dressed the fresh root and left no ghosts"
        );
        // A frame for the transforms to reach the fresh props.
        app.update();
        let rebuilt = props_on(&mut app, new_root);
        assert_eq!(rebuilt.len(), 3, "{under:?}: all three on the new joints");
        for ((joint, local, global), (new_joint, new_local, new_global)) in
            seated.iter().zip(&rebuilt)
        {
            assert_eq!(joint, new_joint, "{under:?}: rides the same joint");
            assert_eq!(local, new_local, "{under:?}: the seat moved");
            let moved = (global.translation() - new_global.translation()).length();
            assert!(
                moved < 0.01,
                "{under:?}: a prop on joint {joint} jumped {moved:.3} m in the rebuild"
            );
        }
        assert_eq!(
            gesture_of(&app, new_root).as_deref(),
            Some("Sit"),
            "{under:?}: the rebuild got the dog up"
        );
        let mut owned = app
            .world_mut()
            .query::<&crate::player::attachments::LocalAttachment>();
        let owned: Vec<_> = owned
            .iter(app.world())
            .map(|local| local.rigged_root)
            .collect();
        match under {
            Under::Local => assert_eq!(owned, vec![new_root; 3]),
            Under::Peer => assert!(owned.is_empty(), "a peer's outfit is never editable"),
        }
    }
}

/// **A collar and a pad go where the body goes when it rests** (#1807): the
/// seat is a place on a joint, so a dog's sit and a horse's lying down carry
/// it. Whatever the clip does to the neck and the spine, the collar stays on
/// the neck's bone, still ringing it, and the pad stays over the chest with
/// its face to the same side of the spine as the skin's.
#[test]
fn a_collar_and_a_pad_follow_a_resting_body() {
    let posed = |app: &mut App, root: Entity, joint: usize| {
        let entity = app.world().get::<AvatarJoints>(root).expect("joints").0[joint];
        *app.world()
            .get::<GlobalTransform>(entity)
            .expect("a posed joint")
    };
    for (family, gesture) in [
        (Family::Dog, "Sit"),
        (Family::Cat, "Sit"),
        (Family::Dog, "Lie Down"),
        (Family::Horse, "Lie Down"),
        (Family::Horse, "Sit"),
    ] {
        let body = default_beast(family);
        let record = wearing(family, &body, &["beast_collar", "saddle_pad"]);
        let offset = crate::player::visuals::rigged_root_offset(&record);
        let mut app = dressing_app();
        let chassis = chassis(&mut app, Under::Local, record, Vec3::Y * offset);
        land(&mut app, chassis, &body, 64, offset);
        run(&mut app, 0.5);
        let root = root_of(&mut app, chassis);
        let rig = rig_of(&app, root);
        let neck = rig.in_zone(Zone::Neck)[0];
        let parent = rig.joints[neck].parent.expect("a neck's parent");
        let head = rig.in_zone(Zone::Head)[0];
        let chest = rig.in_zone(Zone::Chest);
        ask(&mut app, root, gesture);
        run(
            &mut app,
            emote::seconds(&rig, gesture).expect("a state") + 0.5,
        );
        let props = props_on(&mut app, root);
        let collar = props.iter().find(|prop| prop.0 == neck).expect("a collar");
        let pad = props.iter().find(|prop| prop.0 == chest[1]).expect("a pad");

        // The collar's centre, against the neck as posed. At rest it stands
        // off the joint by the section's offset, no more: a pose must keep it
        // there, since the joint is what carries it, and turn the ring no
        // further from the bones either side than the neck itself bends.
        let (from, joint, to) = (
            posed(&mut app, root, parent).translation(),
            posed(&mut app, root, neck).translation(),
            posed(&mut app, root, head).translation(),
        );
        let centre = collar.2.translation();
        let off = (centre - joint).length();
        let rest_off = collar.1.translation.length() * 1.05 + 0.002;
        assert!(
            off <= rest_off,
            "{family:?} {gesture}: the collar stands {off:.3} m off its joint, {rest_off:.3} at rest"
        );
        let ring = collar.2.rotation() * Vec3::Y;
        let bisect = ((joint - from).normalize() + (to - joint).normalize()).normalize();
        let angle = ring.dot(bisect).clamp(-1.0, 1.0).acos().to_degrees();
        println!("{family:?} {gesture}: the ring stands {angle:.1} deg off the neck's line");
        assert!(
            angle < 45.0,
            "{family:?} {gesture}: the ring stands {angle:.1} deg off the neck's line"
        );

        // The pad stays on the top of the chest: over its two joints in the
        // chest's own up, the way it was at rest.
        let (near, far) = (
            posed(&mut app, root, chest[0]),
            posed(&mut app, root, chest[1]),
        );
        let middle = (near.translation() + far.translation()) * 0.5;
        let up = pad.2.rotation() * Vec3::Y;
        let over = (pad.2.translation() - middle).dot(up);
        let rest_over = pad.1.translation.length();
        println!(
            "{family:?} {gesture}: the pad stands {over:.3} m over the spine ({rest_over:.3} at \
             rest), and faces {up}"
        );
        assert!(
            over > rest_over * 0.6 && over < rest_over * 1.2,
            "{family:?} {gesture}: the pad stands {over:.3} m over the spine, {rest_over:.3} at \
             rest"
        );
    }
}
