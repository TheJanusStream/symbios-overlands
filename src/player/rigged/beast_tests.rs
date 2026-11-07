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

/// A metre-grid heightmap 64 m across, centred on the origin as the world's
/// is, its ground at `height` of each world `(x, z)`.
fn terrain(height: impl Fn(f32, f32) -> f32) -> crate::terrain::FinishedHeightMap {
    let mut map = bevy_symbios_ground::HeightMap::new(65, 65, 1.0);
    for z in 0..65 {
        for x in 0..65 {
            map.set(x, z, height(x as f32 - 32.0, z as f32 - 32.0));
        }
    }
    crate::terrain::FinishedHeightMap(map, None)
}

/// A built `body` hung under a chassis at `place` the way the game hangs
/// one, filled once over `ground`, and the [`Drive`] the fill wrote.
///
/// The body goes on as a bare [`AvatarBody`], with nothing drawn: the fill
/// reads the rig and nothing else of it. The ground is physics as well as a
/// heightmap (#1833): its heightfield collider on a `TerrainMesh`, as the
/// game spawns it, stepped until avian's collider trees hold it. Neither
/// chassis carries a collider here, so the fill asks what each rests on as
/// it asks of a peer - its body's lying capsule cast down onto the world.
fn filled_over(
    ground: crate::terrain::FinishedHeightMap,
    water: Option<WaterSurfaces>,
    under: Under,
    record: AvatarRecord,
    body: &EngineAvatarRecord,
    place: Transform,
) -> Drive {
    filled_drawn(ground, water, under, record, body, place, place)
}

/// [`filled_over`], with the chassis drawn at `drawn` this frame - its
/// `Transform` - while its `GlobalTransform` still says `place`, as the one
/// last frame propagated does (#1833).
fn filled_drawn(
    ground: crate::terrain::FinishedHeightMap,
    water: Option<WaterSurfaces>,
    under: Under,
    record: AvatarRecord,
    body: &EngineAvatarRecord,
    place: Transform,
    drawn: Transform,
) -> Drive {
    let offset = crate::player::visuals::rigged_root_offset(&record);
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        bevy::asset::AssetPlugin::default(),
        avian3d::prelude::PhysicsPlugins::default(),
    ));
    // Avian's collider cache listens for mesh events.
    app.init_asset::<Mesh>();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        FRAME,
    )));
    app.init_resource::<crate::player::RigHold>();
    app.world_mut().spawn(terrain_body(&ground));
    app.insert_resource(ground);
    if let Some(water) = water {
        app.insert_resource(water);
    }
    app.finish();
    app.cleanup();
    for _ in 0..3 {
        app.update();
    }
    let chassis = chassis(&mut app, under, record, place.translation);
    app.world_mut()
        .entity_mut(chassis)
        .insert((drawn, GlobalTransform::from(place)));
    let root = app
        .world_mut()
        .spawn((
            RiggedRoot,
            Drive::default(),
            RiggedTrail::default(),
            crate::player::visuals::rigged_root_transform(offset),
            AvatarBody {
                avatar: built(body, 64),
            },
            ChildOf(chassis),
        ))
        .id();
    app.world_mut()
        .run_system_once(fill_rigged_drive)
        .expect("the fill runs");
    app.world().get::<Drive>(root).expect("a drive").clone()
}

/// The terrain's static body as the game spawns it
/// (`terrain::heightmap::spawn_terrain_mesh`): the heightfield collider of
/// `ground` at the origin, marked as the terrain.
fn terrain_body(ground: &crate::terrain::FinishedHeightMap) -> impl Bundle {
    (
        Transform::IDENTITY,
        avian3d::prelude::RigidBody::Static,
        bevy_symbios_ground::build_heightfield_collider(&ground.0),
        crate::terrain::TerrainMesh,
    )
}

/// How far the rigged root of a `record` chassis hangs under its centre: a
/// beast's by its record's column ([`WaterColumn::of`]), as the fill hangs it.
fn drop_of(record: &AvatarRecord) -> f32 {
    let crate::pds::LocomotionConfig::Humanoid(p) = &record.locomotion else {
        panic!("on foot")
    };
    WaterColumn::of(p).drop
}

/// The patch's ground at `(x, z)` of the body's own frame.
fn patch_at(patch: &bevy_symbios_avatar::GroundPatch, x: f32, z: f32) -> f32 {
    patch
        .ground(Vec3::new(x, 0.0, z))
        .expect("a patch always answers")
        .position
        .y
}

/// **A beast on a slope is handed the slope, in its own frame** (#1833).
/// Each family, on its owner's client and a peer's, faces several ways on
/// ground rising a fifth of a metre per metre toward its front, and then
/// toward its left: through the rigged root's half turn the patch rises
/// toward the body's `+Z` in the first case and its `+X` in the second -
/// the engine's forward and left - and is level across the other way. A
/// half turn dropped from the mapping reads both backwards, which is the
/// mirrored case this is tested on. And the patch is the terrain itself: at
/// every corner of the body's reach it is the heightmap under that point,
/// less the root's height.
#[test]
fn a_beast_on_a_slope_is_handed_the_slope_in_its_own_frame() {
    const GRADE: f32 = 0.2;
    for (family, under) in [
        (Family::Dog, Under::Local),
        (Family::Cat, Under::Peer),
        (Family::Horse, Under::Local),
        (Family::Horse, Under::Peer),
    ] {
        let record = beast_record(family);
        let body = default_beast(family);
        let rig = built(&body, 64).rig;
        let reach = bevy_symbios_avatar::GroundPatch::reach(&rig).expect("it stands");
        for yaw in [0.0f32, 1.3, 3.5] {
            let turn = Quat::from_rotation_y(yaw);
            // The chassis' own forward is its `-Z` and its left its `-X`.
            for (rises, toward) in [("front", Vec3::NEG_Z), ("left", Vec3::NEG_X)] {
                let uphill = turn * toward;
                let height = |x: f32, z: f32| 4.0 + GRADE * (x * uphill.x + z * uphill.z);
                let at = Vec2::new(3.0, -2.0);
                let place = Transform::from_xyz(at.x, height(at.x, at.y) + drop_of(&record), at.y)
                    .with_rotation(turn);
                let ground = terrain(height);
                assert!((ground.world_height_at(at.x, at.y) - height(at.x, at.y)).abs() < 1e-4);
                let drive = filled_over(ground, None, under, record.clone(), &body, place);
                let patch = drive.ground.as_ref().unwrap_or_else(|| {
                    panic!("{family:?} {under:?} at {yaw}: no patch on the terrain")
                });
                let (along, across) = (
                    patch_at(patch, 0.0, 0.5) - patch_at(patch, 0.0, -0.5),
                    patch_at(patch, 0.5, 0.0) - patch_at(patch, -0.5, 0.0),
                );
                let (up, level) = if rises == "front" {
                    (along, across)
                } else {
                    (across, along)
                };
                assert!(
                    (up - GRADE).abs() < 1e-3 && level.abs() < 1e-3,
                    "{family:?} {under:?} at {yaw}, rising toward its {rises}: a metre of the \
                     patch rises {along:.4} toward the front and {across:.4} toward the left"
                );
                // Every corner of the reach, against the heightmap itself.
                let root = GlobalTransform::from(place).mul_transform(
                    crate::player::visuals::rigged_root_transform(drop_of(&record)),
                );
                for corner in [reach.min, reach.max, Vec2::new(reach.min.x, reach.max.y)] {
                    let world = root.transform_point(Vec3::new(corner.x, 0.0, corner.y));
                    let expected = height(world.x, world.z) - world.y;
                    let got = patch_at(patch, corner.x, corner.y);
                    assert!(
                        (got - expected).abs() < 1e-3,
                        "{family:?} {under:?} at {yaw}: {got} at {corner} where the terrain is \
                         {expected}"
                    );
                }
            }
        }
    }
}

/// **Only a beast standing on the terrain is handed it** (#1833). The same
/// dog on the same slope: half a metre above it, in the air with nothing
/// under it, it keeps the level floor; and so it does in water deep enough
/// to swim, whose stroke has no floor. (Resting on a deck, a road or a floor
/// the heightmap does not know is the physics' to say, and
/// `a_beast_resting_on_a_deck_or_a_ribbon_is_handed_no_patch` says it.) A person
/// standing on that slope keeps it too while
/// [`super::underfoot::PEOPLE_STAND_ON_TERRAIN`] is off, the owner's default,
/// and is handed it once it is on.
#[test]
fn only_a_beast_on_the_terrain_is_handed_it() {
    let slope = |_: f32, z: f32| -3.0 - 0.2 * z;
    let record = beast_record(Family::Dog);
    let body = default_beast(Family::Dog);
    let on = |lift: f32| Transform::from_xyz(0.0, slope(0.0, 0.0) + drop_of(&record) + lift, 0.0);
    for under in [Under::Local, Under::Peer] {
        let standing = filled_over(terrain(slope), None, under, record.clone(), &body, on(0.0));
        assert!(
            standing.ground.is_some(),
            "{under:?}: on the terrain, and given none"
        );
        let above = filled_over(terrain(slope), None, under, record.clone(), &body, on(0.5));
        assert!(
            above.ground.is_none(),
            "{under:?}: half a metre over it, and given it"
        );
        // The pond's surface is at 0 and the ground three metres under it.
        let swimming = filled_over(
            terrain(slope),
            Some(pond()),
            under,
            record.clone(),
            &body,
            on(0.0),
        );
        assert!(
            swimming.swimming,
            "{under:?}: three metres down, and not swimming"
        );
        assert!(
            swimming.ground.is_none(),
            "{under:?}: swimming, and given a floor"
        );
    }
    let person = person_record("did:plc:terrain-person");
    let offset = crate::player::visuals::rigged_root_offset(&person);
    let drive = filled_over(
        terrain(slope),
        None,
        Under::Local,
        person,
        &engine_default_for_did("did:plc:terrain-person"),
        Transform::from_xyz(0.0, slope(0.0, 0.0) + offset, 0.0),
    );
    assert_eq!(
        drive.ground.is_some(),
        super::underfoot::PEOPLE_STAND_ON_TERRAIN,
        "a person's ground is not the one the setting asks for"
    );
}

/// A world for the ground under a beast resting in it (#1833), in the game's
/// order of systems: `ground` laid as the game lays it - its heightfield
/// collider on a `TerrainMesh` as well as the heightmap - and `solids`, static
/// boxes standing on it, each its half-extents and its pose, under avian at
/// the game's fixed step.
fn grounded(ground: &dyn Fn(f32, f32) -> f32, solids: &[(Vec3, Transform)]) -> App {
    let ground = terrain(ground);
    let mut app = app();
    app.add_plugins(avian3d::prelude::PhysicsPlugins::default());
    app.world_mut().spawn(terrain_body(&ground));
    for &(half, at) in solids {
        app.world_mut().spawn((
            at,
            avian3d::prelude::RigidBody::Static,
            avian3d::prelude::Collider::cuboid(half.x * 2.0, half.y * 2.0, half.z * 2.0),
        ));
    }
    app.insert_resource(ground);
    app.finish();
    app.cleanup();
    app.update();
    app
}

/// A static box spanning `min` to `max`, as [`grounded`] takes one.
fn block(min: Vec3, max: Vec3) -> (Vec3, Transform) {
    (
        (max - min) * 0.5,
        Transform::from_translation((min + max) * 0.5),
    )
}

/// A `family` beast's chassis set down at `place` in `app`, a body built
/// under it, and its root, once both have settled.
///
/// On its owner's client the chassis is the game's own - the local player's
/// root with its record's preset, a dynamic lying capsule held level - left
/// to come to rest under gravity before the body lands, so what it rests on
/// is what the physics says it rests on. A peer has no collider and is
/// played out exactly at `place`, so a peer's `place` is where a local
/// chassis came to rest ([`rest_place`]).
fn standing(app: &mut App, under: Under, family: Family, place: Transform) -> (Entity, Entity) {
    let record = beast_record(family);
    let offset = crate::player::visuals::rigged_root_offset(&record);
    let chassis = match under {
        Under::Local => {
            app.insert_resource(LiveAvatarRecord(record.clone()));
            let chassis = app
                .world_mut()
                .spawn(crate::player::spawn::chassis_root_bundle(place))
                .id();
            let mut commands = app.world_mut().commands();
            crate::player::preset::build_preset_components(
                &mut commands,
                chassis,
                &record.locomotion,
            );
            app.world_mut().flush();
            chassis
        }
        Under::Peer => app.world_mut().spawn((place, peer(record))).id(),
    };
    run(app, 0.5);
    land(app, chassis, &default_beast(family), 64, offset);
    run(app, 1.0);
    (chassis, root_of(app, chassis))
}

/// Where a `family` beast's own chassis, set down at `place` over `ground`
/// and `solids`, comes to rest: the pose a peer of it is played out at.
fn rest_place(
    ground: &dyn Fn(f32, f32) -> f32,
    solids: &[(Vec3, Transform)],
    family: Family,
    place: Transform,
) -> Transform {
    let mut app = grounded(ground, solids);
    let (chassis, _) = standing(&mut app, Under::Local, family, place);
    *app.world().get::<Transform>(chassis).expect("a chassis")
}

/// A `family` beast standing at `place`, set down just over where its own
/// chassis rests there, on either client: the app, the chassis and the root.
fn beast_resting(
    ground: &dyn Fn(f32, f32) -> f32,
    solids: &[(Vec3, Transform)],
    under: Under,
    family: Family,
    place: Transform,
) -> (App, Entity, Entity) {
    let place = match under {
        Under::Local => place,
        Under::Peer => rest_place(ground, solids, family, place),
    };
    let mut app = grounded(ground, solids);
    let (chassis, root) = standing(&mut app, under, family, place);
    (app, chassis, root)
}

/// The lowest point of each foot a body stands on, as it is drawn: for each
/// limb it stands on ([`Rig::ground_contacts`]), the lowest reach of its
/// extremity joints - a joint's centre less its radius through its section,
/// as [`lowest_foot`] reads one at rest.
fn drawn_soles(app: &App, root: Entity) -> Vec<Vec3> {
    let rig = rig_of(app, root);
    let at = drawn(app, root);
    rig.ground_contacts()
        .into_iter()
        .map(|limb| {
            rig.extremity_joints(limb)
                .into_iter()
                .map(|joint| {
                    at[joint] - Vec3::Y * rig.joints[joint].radius * rig.joints[joint].scale.y
                })
                .min_by(|a, b| a.y.total_cmp(&b.y))
                .expect("a foot has an extremity")
        })
        .collect()
}

/// A beast's lying capsule, read off the body `family` defaults to.
fn capsule_of(family: Family) -> crate::pds::avatar::locomotion::LyingCapsule {
    crate::pds::avatar::locomotion::LyingCapsule::of(&built(&default_beast(family), 64).rig)
        .expect("a beast lies along its trunk")
}

/// **A beast on a level floor cut into a hillside stands on the floor**
/// (#1833) - the floor a seeded building's pad is, set at the highest
/// terrain under it without the heightmap being flattened
/// (`world_builder::compile::pad`). A horse and a dog rest on such a floor
/// with their front at its uphill edge, where the hill rises past it, on a
/// grade of a fifth and on 20 degrees, on either client.
///
/// The physics has each resting on the floor, so it is handed the floor's
/// ground (#1834) - under its feet the floor's top, not the hill under it -
/// and every foot is on it: within the window a beast on the level floor
/// stands its feet in at rest (`lowest_foot`, -15 to +5 mm, the pad joints'
/// own round undersides), widened by the 5 mm the contact solver lets a
/// resting capsule sink. A band against the heightmap once handed these the
/// hill under the floor, and their feet went down through it: the horse's
/// hind soles 0.43 m on a fifth, 0.79 m on 20 degrees.
#[test]
fn a_beast_on_a_floor_cut_into_a_hillside_stands_on_the_floor() {
    for grade in [0.2f32, 20f32.to_radians().tan()] {
        // The hill rises toward -Z, the way a chassis faces at no yaw.
        let ground = move |_: f32, z: f32| -grade * z;
        let top = 4.0 * grade;
        let floor = block(
            Vec3::new(-4.0, -4.0 * grade - 1.0, -4.0),
            Vec3::new(4.0, top, 4.0),
        );
        for family in [Family::Horse, Family::Dog] {
            let capsule = capsule_of(family);
            let record = beast_record(family);
            // Its front a hand short of the edge: the rig's +Z is the
            // chassis' -Z, uphill.
            let place = Transform::from_xyz(
                0.0,
                top + drop_of(&record) + 0.01,
                -4.0 + capsule.to + capsule.radius + 0.05,
            );
            let stance =
                bevy_symbios_avatar::GroundPatch::stance(&built(&default_beast(family), 64).rig)
                    .expect("it stands");
            for under in [Under::Local, Under::Peer] {
                let (app, _, root) = beast_resting(&ground, &[floor], under, family, place);
                let posed = *app.world().get::<GlobalTransform>(root).expect("placed");
                let drive = app.world().get::<Drive>(root).expect("a drive");
                for corner in [stance.min, stance.max] {
                    let got = drive.ground.as_ref().map_or(posed.translation().y, |_| {
                        patched_world(drive, &posed, corner.x, corner.y)
                    });
                    assert!(
                        (got - top).abs() < 1e-3,
                        "{family:?} {under:?} on grade {grade}: on the floor, and handed {got:.3} \
                         under its feet, the floor's top {top:.3}"
                    );
                }
                for sole in drawn_soles(&app, root) {
                    let off = sole.y - top;
                    assert!(
                        (-0.02..=0.005).contains(&off),
                        "{family:?} {under:?} on grade {grade}: a foot at {sole} stands \
                         {:+.1} mm off the floor",
                        off * 1000.0
                    );
                }
            }
        }
    }
}

/// The ground a patch hands a body at `(x, z)` of its own frame, in the
/// world: the patch's height there plus the root's own.
fn patched_world(drive: &Drive, root: &GlobalTransform, x: f32, z: f32) -> f32 {
    patch_at(drive.ground.as_ref().expect("a patch"), x, z) + root.translation().y
}

/// **A beast at a pad's uphill edge puts its fore feet on the pad** (#1833).
/// A pad is set at the highest terrain under it, so at its uphill edge its
/// top is the terrain's height and downhill it stands over it. A horse and a
/// dog stand on the terrain just uphill of one on 20 degrees, facing down
/// the hill with their fore feet 0.4 m over the pad: resting on the terrain
/// alone, each is handed the terrain - and over the pad, the pad's top, read
/// by a ray, where the heightmap is 0.15 m under it. Its fore feet stand on
/// the pad and not in it, on either client.
#[test]
fn a_beast_at_a_pads_uphill_edge_puts_its_fore_feet_on_the_pad() {
    let grade = 20f32.to_radians().tan();
    let ground = move |_: f32, z: f32| -grade * z;
    let pad = block(
        Vec3::new(-3.0, -6.0 * grade - 1.0, 0.0),
        Vec3::new(3.0, 0.0, 6.0),
    );
    for family in [Family::Horse, Family::Dog] {
        let capsule = capsule_of(family);
        let record = beast_record(family);
        let rig = built(&default_beast(family), 64).rig;
        let stance = bevy_symbios_avatar::GroundPatch::stance(&rig).expect("it stands");
        // Facing +Z, down the hill: the rig's +Z is the world's.
        let fore = 0.4;
        let z = fore - stance.max.y;
        // Dropped onto the hill from a hand over its rear end, uphill.
        let rear = -grade * (z + capsule.from - capsule.radius);
        let place = Transform::from_xyz(0.0, rear + drop_of(&record) + 0.05, z)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::PI));
        for under in [Under::Local, Under::Peer] {
            let (app, _, root) = beast_resting(&ground, &[pad], under, family, place);
            let posed = *app.world().get::<GlobalTransform>(root).expect("placed");
            let drive = app.world().get::<Drive>(root).expect("a drive");
            assert!(
                drive.ground.is_some(),
                "{family:?} {under:?}: on the terrain, and handed none"
            );
            for x in [stance.min.x, stance.max.x] {
                let world = posed.transform_point(Vec3::new(x, 0.0, stance.max.y));
                let got = patched_world(drive, &posed, x, stance.max.y);
                let terrain_y = -grade * world.z;
                assert!(
                    got.abs() < 5e-3 && got - terrain_y > 0.1,
                    "{family:?} {under:?}: over the pad at {world} the ground is {got:.3}, \
                     the pad's top 0 and the terrain {terrain_y:.3}"
                );
            }
            let soles = drawn_soles(&app, root);
            let fore_soles = soles.iter().filter(|sole| sole.z > 0.05);
            for sole in fore_soles {
                assert!(
                    (-0.02..=0.03).contains(&sole.y),
                    "{family:?} {under:?}: a fore foot over the pad stands at {sole}"
                );
            }
        }
    }
}

/// **A tall obstacle beside a beast lifts no foot onto it** (#1833). A wall
/// two metres high stands a hand to the side of a horse on a slope, inside
/// its feet's reach: every sample of its patch, over the wall or not, is the
/// terrain under it. A ray down over the wall starts inside it, and a foot is
/// never lifted onto a surface higher than a step - read as a hit where it
/// started, it would have stood that foot on the air 0.45 m up.
#[test]
fn a_tall_obstacle_beside_a_beast_lifts_no_foot_onto_it() {
    let grade = 0.2;
    let hill = move |_: f32, z: f32| -grade * z;
    let family = Family::Horse;
    let capsule = capsule_of(family);
    let record = beast_record(family);
    let side = capsule.radius + 0.15;
    let wall = block(Vec3::new(side, -3.0, -3.0), Vec3::new(side + 1.0, 3.0, 3.0));
    // Dropped onto the hill from a hand over its front end, uphill.
    let front = grade * (capsule.to + capsule.radius);
    let place = Transform::from_xyz(0.0, front + drop_of(&record) + 0.05, 0.0);
    for under in [Under::Local, Under::Peer] {
        let (app, _, root) = beast_resting(&hill, &[wall], under, family, place);
        let posed = *app.world().get::<GlobalTransform>(root).expect("placed");
        let drive = app.world().get::<Drive>(root).expect("a drive");
        let patch = drive
            .ground
            .as_ref()
            .unwrap_or_else(|| panic!("{under:?}: on the terrain, and handed none"));
        let mut over = 0;
        for row in 0..patch.side() {
            for column in 0..patch.side() {
                let at = patch.sample_at(column, row);
                let world = posed.transform_point(Vec3::new(at.x, 0.0, at.y));
                over += usize::from((side..side + 1.0).contains(&world.x));
                let got = patch.heights()[row * patch.side() + column] + world.y;
                assert!(
                    (got - hill(world.x, world.z)).abs() < 1e-3,
                    "{under:?}: at {world} the ground is {got:.3}, the terrain {:.3}",
                    hill(world.x, world.z)
                );
            }
        }
        assert!(over > 10, "{under:?}: the wall is not inside the reach");
    }
}

/// **A beast resting on a deck is handed the deck's top** (#1833, #1834). A
/// deck laid 8 cm over a 20-degree hill and parallel to it - a boardwalk, or
/// a ribbon draped lower than a road's 0.2 m - takes a horse and a dog
/// resting on it, on either client: what they rest on is not the terrain, so
/// they are handed the structure's ground, and over the deck it is the
/// deck's top, never the terrain under it. A band 5 cm either side of the
/// terrain under the collider passed both as on the terrain - the lying
/// capsule rests on its uphill end, nearer the terrain there than the deck's
/// own 8 cm - and the patch stood their feet in the deck, on the terrain
/// under it; until #1834 they kept the level floor, through the slope.
#[test]
fn a_beast_resting_on_a_deck_or_a_ribbon_is_handed_its_top() {
    let grade = 20f32.to_radians().tan();
    let ground = move |_: f32, z: f32| -grade * z;
    let tilt = grade.atan();
    let half = Vec3::new(2.0, 0.025, 4.0);
    // Its top the plane 8 cm over the hill, through a centre under it.
    let centre_y = 0.08 - (tilt.cos() + grade * tilt.sin()) * half.y;
    let deck = (
        half,
        Transform::from_xyz(0.0, centre_y, 0.0).with_rotation(Quat::from_rotation_x(tilt)),
    );
    for family in [Family::Horse, Family::Dog] {
        let record = beast_record(family);
        // Dropped onto the deck from a hand over it at its front end, uphill.
        let capsule = capsule_of(family);
        let front = 0.08 + grade * (capsule.to + capsule.radius);
        let place = Transform::from_xyz(0.0, front + drop_of(&record) + 0.05, 0.0);
        for under in [Under::Local, Under::Peer] {
            let (app, chassis, root) = beast_resting(&ground, &[deck], under, family, place);
            let at = app
                .world()
                .get::<Transform>(chassis)
                .expect("a chassis")
                .translation;
            assert!(
                at.z.abs() < 0.5,
                "{family:?} {under:?}: slid off the deck to {at}"
            );
            let posed = *app.world().get::<GlobalTransform>(root).expect("placed");
            let drive = app.world().get::<Drive>(root).expect("a drive");
            assert!(
                drive.ground.is_some(),
                "{family:?} {under:?}: resting on the deck, and handed no patch"
            );
            let mut over = 0;
            for (world, got) in patch_samples(drive, &posed) {
                // Well inside the deck's top, the sloped box's own corners
                // aside.
                if world.x.abs() < half.x - 0.1 && world.z.abs() < (half.z - 0.2) * tilt.cos() {
                    over += 1;
                    let top = ground(world.x, world.z) + 0.08;
                    assert!(
                        (got - top).abs() < 2e-3,
                        "{family:?} {under:?}: over the deck at {world} the ground is {got:.3}, \
                         its top {top:.3} and the terrain under it {:.3}",
                        ground(world.x, world.z)
                    );
                }
            }
            assert!(
                over > 20,
                "{family:?} {under:?}: the deck is not under the reach"
            );
        }
    }
}

/// **A chassis is sampled where it is drawn this frame** (#1833). A peer
/// played out to the brow of a hill - its `Transform` this frame - while its
/// `GlobalTransform` still says where last frame's propagation left it, a
/// metre and a half back on the flat, is handed the ground under where it is
/// drawn: at every corner of its reach the terrain there. Sampled through the
/// stale pose, as it was, the patch was the flat's for a body drawn going
/// over the brow, its front feet over the air. On either client, since the
/// local chassis' `Transform` is the one avian's interpolation has eased
/// this frame.
#[test]
fn a_chassis_is_sampled_where_it_is_drawn_this_frame() {
    // Flat to the brow at z = 0, then falling away a third of a metre a metre.
    let hill = |_: f32, z: f32| if z < 0.0 { 2.0 } else { 2.0 - 0.33 * z };
    let family = Family::Horse;
    let record = beast_record(family);
    let body = default_beast(family);
    let reach = bevy_symbios_avatar::GroundPatch::reach(&built(&body, 64).rig).expect("it stands");
    // Facing +Z, over the brow.
    let turn = Quat::from_rotation_y(std::f32::consts::PI);
    let at = |z: f32| Transform::from_xyz(0.0, 2.0 + drop_of(&record), z).with_rotation(turn);
    let (drawn, stale) = (at(0.3), at(-1.5));
    let root_at = |chassis: Transform| {
        GlobalTransform::from(chassis).mul_transform(crate::player::visuals::rigged_root_transform(
            drop_of(&record),
        ))
    };
    for under in [Under::Local, Under::Peer] {
        let drive = filled_drawn(
            terrain(hill),
            None,
            under,
            record.clone(),
            &body,
            stale,
            drawn,
        );
        let mut apart = 0.0f32;
        for corner in [reach.min, reach.max, Vec2::new(reach.min.x, reach.max.y)] {
            let expect = |root: GlobalTransform| {
                let world = root.transform_point(Vec3::new(corner.x, 0.0, corner.y));
                hill(world.x, world.z) - world.y
            };
            let got = patch_at(
                drive
                    .ground
                    .as_ref()
                    .unwrap_or_else(|| panic!("{under:?}: handed none")),
                corner.x,
                corner.y,
            );
            assert!(
                (got - expect(root_at(drawn))).abs() < 1e-3,
                "{under:?}: {got:.3} at {corner}, where the ground under the drawn body is \
                 {:.3} and under the stale pose {:.3}",
                expect(root_at(drawn)),
                expect(root_at(stale))
            );
            apart = apart.max((expect(root_at(drawn)) - expect(root_at(stale))).abs());
        }
        assert!(
            apart > 0.2,
            "the two poses stand on the same ground: {apart}"
        );
    }
}

/// One pass of a beast walking a line, set down each frame as the render
/// tool's walker is ([`crate::player::Underfoot::set_down`]), and the worst
/// one-frame rise or fall of a planted foot: on the frames its patch came or
/// went, and on every other frame.
fn walked_pop(
    ground: &dyn Fn(f32, f32) -> f32,
    solids: &[(Vec3, Transform)],
    family: Family,
    from: f32,
    to: f32,
) -> (f32, f32, usize) {
    const PACE: f32 = 1.2;
    let record = beast_record(family);
    let drop = drop_of(&record);
    let rig = built(&default_beast(family), 64).rig;
    let turn = if to > from {
        Quat::from_rotation_y(std::f32::consts::PI)
    } else {
        Quat::IDENTITY
    };
    let mut app = grounded(ground, solids);
    // Over the higher of the terrain and where it rested last, as the walker.
    let mut rested: Option<f32> = None;
    let mut set_down = |app: &mut App, z: f32| {
        let over = rested.map_or(ground(0.0, z), |rested| rested.max(ground(0.0, z)));
        let on = Transform::from_xyz(0.0, over + drop, z).with_rotation(turn);
        let rig = rig.clone();
        let fell = app
            .world_mut()
            .run_system_once(move |underfoot: crate::player::Underfoot| {
                let root = GlobalTransform::from(on)
                    .mul_transform(crate::player::visuals::rigged_root_transform(drop));
                underfoot.set_down(&rig, &root).map(|(fell, _)| fell)
            })
            .expect("the cast runs")
            .expect("it comes to rest");
        rested = Some(over - fell);
        on.with_translation(Vec3::new(0.0, over - fell + drop, z))
    };
    let start = set_down(&mut app, from);
    let (chassis, root) = standing(&mut app, Under::Peer, family, start);
    let frames = ((to - from).abs() / PACE / FRAME) as usize;
    let mut was = drawn_soles(&app, root);
    let mut patched = app
        .world()
        .get::<Drive>(root)
        .expect("a drive")
        .ground
        .is_some();
    let (mut at_toggle, mut elsewhere, mut toggles) = (0.0f32, 0.0f32, 0);
    for frame in 1..=frames {
        let z = from + (to - from).signum() * PACE * FRAME * frame as f32;
        let place = set_down(&mut app, z);
        *app.world_mut()
            .get_mut::<Transform>(chassis)
            .expect("a chassis") = place;
        app.update();
        let now = drawn_soles(&app, root);
        let has = app
            .world()
            .get::<Drive>(root)
            .expect("a drive")
            .ground
            .is_some();
        let toggled = has != patched;
        toggles += usize::from(toggled);
        patched = has;
        for (a, b) in was.iter().zip(&now) {
            let planted = Vec2::new(a.x - b.x, a.z - b.z).length() < 2e-3;
            if planted {
                let jump = (b.y - a.y).abs();
                if toggled {
                    at_toggle = at_toggle.max(jump);
                } else {
                    elsewhere = elsewhere.max(jump);
                }
            }
        }
        was = now;
    }
    (at_toggle, elsewhere, toggles)
}

/// **A measurement (#1833): does a planted foot pop as a beast's patch comes
/// or goes?** A horse and a dog walk from the terrain onto a level pad cut
/// into a 20-degree hill at its uphill edge, and back off it, set down each
/// frame as the walker is; the worst one-frame jump of a planted foot is
/// printed for the frames the patch came or went and for every other frame.
#[test]
#[ignore = "a measurement, printed: run by hand"]
fn probe_a_beast_walking_onto_a_pad() {
    let grade = 20f32.to_radians().tan();
    let ground = move |_: f32, z: f32| -grade * z;
    let pad = block(
        Vec3::new(-3.0, -6.0 * grade - 1.0, 0.0),
        Vec3::new(3.0, 0.0, 6.0),
    );
    for family in [Family::Horse, Family::Dog] {
        for (way, from, to) in [("onto", -3.0, 3.0), ("off", 3.0, -3.0)] {
            let (at_toggle, elsewhere, toggles) = walked_pop(&ground, &[pad], family, from, to);
            println!(
                "{family:?} walking {way} the pad: {toggles} toggles, a planted foot jumps up to \
                 {:.1} mm on them and {:.1} mm on any other frame",
                at_toggle * 1000.0,
                elsewhere * 1000.0
            );
        }
    }
}

/// **A measurement (#1833): what the ground under a beast costs a frame.** A
/// horse standing on a hill at a pad's uphill edge, its fore feet over the
/// pad, is filled a few thousand times: in open terrain, where nothing is
/// gathered and no ray is cast, and beside a building - the pad and forty
/// posts on it, each a collider of its own - where every sample over one of
/// them is read by a ray. And on flat ground, in the open and between the
/// streets of a street grid that is one trimesh of 28,672 triangles, its
/// bounds the whole terrain (review finding 2). Printed per fill, with the
/// structures gathered.
///
/// Then a peer's own cost (finding 3), in open terrain on the hill: the
/// capsule's cast alone, and a whole fill a frame for a peer standing still,
/// which reads its last cast back, and for one walking, which casts every
/// frame.
#[test]
#[ignore = "a measurement, printed: run by hand"]
fn probe_the_cost_of_the_ground_beside_a_building() {
    const FILLS: u32 = 4000;
    let grade = 20f32.to_radians().tan();
    let hill = move |_: f32, z: f32| -grade * z;
    let flat = |_: f32, _: f32| 0.0;
    let mut building = vec![block(
        Vec3::new(-3.0, -6.0 * grade - 1.0, 0.0),
        Vec3::new(3.0, 0.0, 6.0),
    )];
    for i in 0..40 {
        let (x, z) = (-2.8 + 0.7 * (i % 8) as f32, 1.0 + 1.1 * (i / 8) as f32);
        building.push(block(
            Vec3::new(x, 0.0, z),
            Vec3::new(x + 0.15, 2.5, z + 0.15),
        ));
    }
    let family = Family::Horse;
    let capsule = capsule_of(family);
    let record = beast_record(family);
    let rig = built(&default_beast(family), 64).rig;
    let stance = bevy_symbios_avatar::GroundPatch::stance(&rig).expect("it stands");
    let z = 0.4 - stance.max.y;
    let rear = -grade * (z + capsule.from - capsule.radius);
    let on_hill = Transform::from_xyz(0.0, rear + drop_of(&record) + 0.05, z)
        .with_rotation(Quat::from_rotation_y(std::f32::consts::PI));
    // Between the streets at x and z of 0 and 10, which run 1.5 m either side.
    let between = Transform::from_xyz(5.0, drop_of(&record) + 0.01, 5.0)
        .with_rotation(Quat::from_rotation_y(0.4));
    let grid = || vec![street_grid(10.0, 3.0, 0.0625, 0.2)];
    // A scene: its name, its ground, its boxes, its trimeshes, and the pose.
    type Scene = (
        &'static str,
        Box<dyn Fn(f32, f32) -> f32>,
        Vec<(Vec3, Transform)>,
        Vec<avian3d::prelude::Collider>,
        Transform,
    );
    let scenes: [Scene; 4] = [
        (
            "open terrain",
            Box::new(hill),
            Vec::new(),
            Vec::new(),
            on_hill,
        ),
        (
            "beside a building",
            Box::new(hill),
            building,
            Vec::new(),
            on_hill,
        ),
        ("open flat", Box::new(flat), Vec::new(), Vec::new(), between),
        (
            "between streets",
            Box::new(flat),
            Vec::new(),
            grid(),
            between,
        ),
    ];
    for (label, ground, solids, trimeshes, place) in scenes {
        let mut app = grounded_with(&*ground, &solids, trimeshes);
        let (chassis, root) = standing(&mut app, Under::Local, family, place);
        let rig = rig_of(&app, root);
        let posed = *app.world().get::<GlobalTransform>(root).expect("placed");
        let (per_fill, patched, gathered) = app
            .world_mut()
            .run_system_once(
                move |underfoot: crate::player::Underfoot,
                      heightmap: Res<crate::terrain::FinishedHeightMap>| {
                    let mut drive = Drive::default();
                    let reach = bevy_symbios_avatar::GroundPatch::reach(&rig).expect("it stands");
                    let structures = underfoot.structures_over(&posed, reach, &heightmap);
                    let gathered = structures.len();
                    // The samples a ray is cast for: those over a structure.
                    let side = (reach.size().max_element() / 0.25).ceil() as usize + 1;
                    let origin = reach.center() - Vec2::splat((side - 1) as f32 * 0.125);
                    let rays = (0..side * side)
                        .filter(|i| {
                            let at =
                                origin + Vec2::new((i % side) as f32, (i / side) as f32) * 0.25;
                            let world = posed.transform_point(Vec3::new(at.x, 0.0, at.y));
                            structures.iter().any(|s| {
                                (s.aabb.min.x..=s.aabb.max.x).contains(&world.x)
                                    && (s.aabb.min.z..=s.aabb.max.z).contains(&world.z)
                            })
                        })
                        .count();
                    let mut memory = super::underfoot::Resting::default();
                    let start = std::time::Instant::now();
                    for _ in 0..FILLS {
                        underfoot.fill(
                            &mut drive,
                            &rig,
                            chassis,
                            &posed,
                            Some(&heightmap),
                            &mut memory,
                            FRAME,
                        );
                    }
                    (
                        start.elapsed() / FILLS,
                        drive.ground.is_some(),
                        (gathered, rays, side * side),
                    )
                },
            )
            .expect("the fill runs");
        let (gathered, rays, samples) = gathered;
        println!(
            "{label}: {per_fill:?} a fill, {gathered} structures gathered, {rays} of {samples} \
             samples read by a ray, patched: {patched}"
        );
    }
    // A peer, played out where its owner's chassis came to rest.
    let place = rest_place(&hill, &[], family, on_hill);
    let mut app = grounded(&hill, &[]);
    let (chassis, root) = standing(&mut app, Under::Peer, family, place);
    let rig = rig_of(&app, root);
    let posed = *app.world().get::<GlobalTransform>(root).expect("placed");
    let (cast, still, walking, patched) = app
        .world_mut()
        .run_system_once(
            move |underfoot: crate::player::Underfoot,
                  heightmap: Res<crate::terrain::FinishedHeightMap>| {
                let shape = crate::pds::avatar::locomotion::LyingCapsule::of(&rig)
                    .expect("a trunk")
                    .collider();
                let start = std::time::Instant::now();
                for _ in 0..FILLS {
                    std::hint::black_box(underfoot.resting_cast(&shape, &posed));
                }
                let cast = start.elapsed() / FILLS;
                let mut drive = Drive::default();
                let mut timed = |step: Vec3| {
                    let mut memory = super::underfoot::Resting::default();
                    let start = std::time::Instant::now();
                    for i in 0..FILLS {
                        let at =
                            posed.mul_transform(Transform::from_translation(step * (i % 2) as f32));
                        underfoot.fill(
                            &mut drive,
                            &rig,
                            chassis,
                            &at,
                            Some(&heightmap),
                            &mut memory,
                            FRAME,
                        );
                    }
                    start.elapsed() / FILLS
                };
                let still = timed(Vec3::ZERO);
                // Two centimetres a frame back and forth: a cast every frame.
                let walking = timed(Vec3::Z * 0.02);
                (cast, still, walking, drive.ground.is_some())
            },
        )
        .expect("the fill runs");
    println!(
        "a peer in open terrain: the cast alone {cast:?}; a fill a frame standing still \
         {still:?}, walking {walking:?}; patched: {patched}"
    );
}

/// Every sample of `drive`'s patch, posed at `root`, in the world: where it
/// is and the ground the patch hands a foot there.
fn patch_samples(drive: &Drive, root: &GlobalTransform) -> Vec<(Vec3, f32)> {
    let patch = drive.ground.as_ref().expect("a patch");
    let mut samples = Vec::new();
    for row in 0..patch.side() {
        for column in 0..patch.side() {
            let at = patch.sample_at(column, row);
            let world = root.transform_point(Vec3::new(at.x, 0.0, at.y));
            samples.push((
                world,
                patch.heights()[row * patch.side() + column] + world.y,
            ));
        }
    }
    samples
}

/// **A beast under a low slab stands on the terrain, not on it** (#1833
/// review, finding 1). A cat on flat ground under a static slab spanning
/// 0.32 to 0.35 m - its back at 0.29 - and a dog under one from 0.39 to
/// 0.44 m, a bench or a low table: a ray down from a step over the body met
/// the slab's top, and the patch stood the cat's soles on it at +0.346. Met
/// again from the terrain, the slab's underside stands clear of the ground,
/// so every sample is the terrain and every drawn foot stands on it, on
/// either client. And a boardwalk ahead of the cat, its underside 3 cm off
/// the ground - too little room for a paw - is stood on.
#[test]
fn a_beast_under_a_low_slab_stands_on_the_terrain() {
    let flat = |_: f32, _: f32| 0.0;
    for (family, low, high) in [(Family::Cat, 0.32, 0.35), (Family::Dog, 0.39, 0.44)] {
        let capsule = capsule_of(family);
        assert!(
            capsule.radius * 2.0 < low - 0.01,
            "{family:?}: its back at {:.3} meets the slab",
            capsule.radius * 2.0
        );
        let slab = block(Vec3::new(-3.0, low, -3.0), Vec3::new(3.0, high, 3.0));
        let place = Transform::from_xyz(0.0, drop_of(&beast_record(family)) + 0.01, 0.0);
        for under in [Under::Local, Under::Peer] {
            let (app, _, root) = beast_resting(&flat, &[slab], under, family, place);
            let posed = *app.world().get::<GlobalTransform>(root).expect("placed");
            let drive = app.world().get::<Drive>(root).expect("a drive");
            assert!(
                drive.ground.is_some(),
                "{family:?} {under:?}: on the terrain under the slab, and handed none"
            );
            for (world, got) in patch_samples(drive, &posed) {
                assert!(
                    got.abs() < 1e-3,
                    "{family:?} {under:?}: under the slab at {world} the ground is {got:.3}"
                );
            }
            for sole in drawn_soles(&app, root) {
                assert!(
                    (-0.02..=0.03).contains(&sole.y),
                    "{family:?} {under:?}: a foot under the slab stands at {sole}"
                );
            }
        }
    }
    // The cat faces -Z: its front is the rig's +Z.
    let family = Family::Cat;
    let capsule = capsule_of(family);
    let front = capsule.to + capsule.radius;
    let boardwalk = block(
        Vec3::new(-3.0, 0.03, -front - 3.0),
        Vec3::new(3.0, 0.08, -front - 0.03),
    );
    let place = Transform::from_xyz(0.0, drop_of(&beast_record(family)) + 0.01, 0.0);
    for under in [Under::Local, Under::Peer] {
        let (app, _, root) = beast_resting(&flat, &[boardwalk], under, family, place);
        let posed = *app.world().get::<GlobalTransform>(root).expect("placed");
        let drive = app.world().get::<Drive>(root).expect("a drive");
        assert!(
            drive.ground.is_some(),
            "{under:?}: on the terrain beside the boardwalk, and handed none"
        );
        let mut over = 0;
        for (world, got) in patch_samples(drive, &posed) {
            if world.z < -front - 0.04 && world.x.abs() < 2.9 {
                over += 1;
                assert!(
                    (got - 0.08).abs() < 1e-3,
                    "{under:?}: over the boardwalk at {world} the ground is {got:.3}, its top 0.08"
                );
            }
        }
        assert!(over > 0, "{under:?}: the boardwalk is not inside the reach");
    }
}

/// A street grid as one trimesh, as `terrain::roads` builds a network's deck
/// (#1833): strips `width` wide every `spacing` metres along both axes of the
/// 64 m terrain, a sheet `height` over it, cut into `segment`-metre lengths
/// two triangles across.
fn street_grid(spacing: f32, width: f32, segment: f32, height: f32) -> avian3d::prelude::Collider {
    let (mut vertices, mut indices) = (Vec::new(), Vec::new());
    let segments = (64.0 / segment).round() as usize;
    let lines = (31.0 / spacing).floor() as i32;
    for line in -lines..=lines {
        let middle = line as f32 * spacing;
        for along_x in [true, false] {
            for i in 0..segments {
                let (a, b) = (-32.0 + i as f32 * segment, -32.0 + (i + 1) as f32 * segment);
                let (l, r) = (middle - width * 0.5, middle + width * 0.5);
                let corners = [(a, l), (b, l), (b, r), (a, r)].map(|(u, v)| {
                    if along_x {
                        Vec3::new(u, height, v)
                    } else {
                        Vec3::new(v, height, u)
                    }
                });
                let first = vertices.len() as u32;
                vertices.extend(corners);
                indices.push([first, first + 2, first + 1]);
                indices.push([first, first + 3, first + 2]);
            }
        }
    }
    avian3d::prelude::Collider::trimesh(vertices, indices)
}

/// `grounded`, with `trimeshes` laid as static bodies too, and stepped until
/// avian's collider trees hold them.
fn grounded_with(
    ground: &dyn Fn(f32, f32) -> f32,
    solids: &[(Vec3, Transform)],
    trimeshes: Vec<avian3d::prelude::Collider>,
) -> App {
    let mut app = grounded(ground, solids);
    for trimesh in trimeshes {
        app.world_mut().spawn((
            Transform::IDENTITY,
            avian3d::prelude::RigidBody::Static,
            trimesh,
        ));
    }
    app.update();
    app
}

/// **A road network is gathered only where its triangles are** (#1833
/// review, finding 2). A street grid is one trimesh whose bounds span the
/// whole terrain, so gathered by bounds a horse anywhere in it had all 256
/// samples rayed against it. Gathered by its shape meeting the box a foot
/// stands in, nothing is gathered for a horse between the streets, at any
/// heading; and a horse whose reach takes in a street gathers it - but not
/// a canopy three metres over it - and is handed the street's sheet where
/// it reaches over it.
#[test]
fn a_road_network_is_gathered_only_where_its_triangles_are() {
    const STREET: f32 = 0.2;
    let flat = |_: f32, _: f32| 0.0;
    let canopy = block(Vec3::new(-30.0, 3.0, -30.0), Vec3::new(30.0, 3.1, 30.0));
    let mut app = grounded_with(&flat, &[canopy], vec![street_grid(10.0, 3.0, 1.0, STREET)]);
    let rig = built(&default_beast(Family::Horse), 64).rig;
    let reach = bevy_symbios_avatar::GroundPatch::reach(&rig).expect("it stands");
    let gathered = |app: &mut App, root: GlobalTransform| {
        app.world_mut()
            .run_system_once(
                move |underfoot: crate::player::Underfoot,
                      heightmap: Res<crate::terrain::FinishedHeightMap>| {
                    underfoot.structures_over(&root, reach, &heightmap).len()
                },
            )
            .expect("the gathering runs")
    };
    // Between the streets at x and z of 0 and 10, which run 1.5 m either side.
    for yaw in [0.0f32, 0.8, 2.0] {
        let root = GlobalTransform::from(
            Transform::from_xyz(5.0, 0.0, 5.0).with_rotation(Quat::from_rotation_y(yaw)),
        );
        assert_eq!(
            gathered(&mut app, root),
            0,
            "at {yaw}: between the streets, and something gathered"
        );
    }
    // The street along x at z = 0 inside the rear half of the reach.
    let z = 1.5 - reach.min.y * 0.5;
    let root = GlobalTransform::from(Transform::from_xyz(5.0, 0.0, z));
    assert_eq!(gathered(&mut app, root), 1, "beside a street, not it alone");
    let mut drive = Drive::default();
    let drive = app
        .world_mut()
        .run_system_once(
            move |underfoot: crate::player::Underfoot,
                  heightmap: Res<crate::terrain::FinishedHeightMap>| {
                underfoot.fill_resting(
                    &mut drive,
                    &rig,
                    &root,
                    Some(&heightmap),
                    crate::player::RestsOn::Terrain,
                );
                drive.clone()
            },
        )
        .expect("the fill runs");
    let over = patched_world(&drive, &root, 0.0, reach.min.y * 0.75);
    assert!(
        (over - STREET).abs() < 1e-3,
        "over the street the ground is {over:.3}, its sheet {STREET}"
    );
}

/// **A peer's cast is kept while it stands still, and not past its moving**
/// (#1833 review, finding 3). A dog peer standing on the terrain is cast
/// once and read back on the frames after - never more than
/// [`super::underfoot::RECAST_EVERY`] running - and cast again on the frame
/// it moves 2 cm. Walking at 1.2 m/s onto a floor laid 3 cm over the
/// terrain ahead of it, its answer changes on the very frame a fresh cast
/// says it rests on the floor; creeping onto it 3 mm a frame, within the
/// frames a centimetre takes. Its patch carries none of the terrain's ground
/// a hold and an ease after that (#1835: [`super::underfoot::HOLD_SECS`] and
/// [`super::underfoot::BLEND_SECS`]), not on that frame - by then it is the
/// floor's (#1834).
#[test]
fn a_peers_cast_is_kept_while_it_stands_still() {
    let flat = |_: f32, _: f32| 0.0;
    let family = Family::Dog;
    let capsule = capsule_of(family);
    let drop = drop_of(&beast_record(family));
    // The floor ahead, its edge a hand in front of the dog's front end.
    let edge = -(capsule.to + capsule.radius) - 0.1;
    let floor = block(Vec3::new(-3.0, -0.5, -6.0), Vec3::new(3.0, 0.03, edge));
    let shape = capsule.collider();
    for (pace, lag) in [(1.2 * FRAME, 0usize), (0.003, 3)] {
        let mut app = grounded(&flat, &[floor]);
        let place = Transform::from_xyz(0.0, drop, 0.0);
        let (chassis, root) = standing(&mut app, Under::Peer, family, place);
        let reused = |app: &App| {
            app.world()
                .get::<RiggedTrail>(root)
                .expect("a trail")
                .rest
                .reused()
        };
        let patched = |app: &App| {
            app.world()
                .get::<Drive>(root)
                .expect("a drive")
                .ground
                .is_some()
        };
        let on_terrain = |app: &App| {
            app.world()
                .get::<RiggedTrail>(root)
                .expect("a trail")
                .rest
                .held()
                .0
                > 0.0
        };
        assert!(patched(&app), "on the terrain, and handed none");
        let mut seen = Vec::new();
        for _ in 0..3 * super::underfoot::RECAST_EVERY {
            app.update();
            seen.push(reused(&app).expect("a cast kept"));
        }
        assert!(
            seen.iter().any(|&n| n > 0) && seen.iter().all(|&n| n < super::underfoot::RECAST_EVERY),
            "standing still, the cast was reused {seen:?} frames running"
        );
        let mut at = place;
        at.translation.z -= 0.02;
        *app.world_mut()
            .get_mut::<Transform>(chassis)
            .expect("a chassis") = at;
        app.update();
        assert_eq!(reused(&app), Some(0), "moved 2 cm, and not cast again");
        // On toward the floor until a fresh cast has it on the floor.
        let (mut flipped, mut dropped) = (None, None);
        for frame in 0..400usize {
            at.translation.z -= pace;
            *app.world_mut()
                .get_mut::<Transform>(chassis)
                .expect("a chassis") = at;
            app.update();
            let posed = *app.world().get::<GlobalTransform>(root).expect("placed");
            let shape = shape.clone();
            let fresh = app
                .world_mut()
                .run_system_once(move |underfoot: crate::player::Underfoot| {
                    underfoot.resting_cast(&shape, &posed)
                })
                .expect("the cast runs");
            if fresh == crate::player::RestsOn::Structure && flipped.is_none() {
                flipped = Some(frame);
            }
            if !on_terrain(&app) && dropped.is_none() {
                dropped = Some(frame);
            }
            if flipped.is_some() && dropped.is_some() {
                break;
            }
        }
        let (flipped, dropped) = (
            flipped.expect("never reached the floor"),
            dropped.expect("never stopped being handed the terrain"),
        );
        println!(
            "at {:.1} mm a frame: on the floor at frame {flipped}, the patch gone at {dropped}",
            pace * 1000.0
        );
        // The patch then holds and eases out (#1835): it is gone once the
        // answer has held a hold's time and the ease has run its course.
        let eased =
            ((super::underfoot::HOLD_SECS + super::underfoot::BLEND_SECS) / FRAME).ceil() as usize;
        assert!(
            dropped + 1 >= flipped + eased && dropped - flipped <= lag + eased + 1,
            "at {pace} m a frame: on the floor at frame {flipped}, and handed the terrain until \
             {dropped}"
        );
    }
}

mod flight;
mod standing_still;
mod structure_ground;
mod winged;
