//! A beast standing still on the terrain, frame by frame (#1835): what the
//! physics says it rests on, whether it is handed the terrain, and which way
//! its trunk is drawn, on slopes and rolling ground, on its owner's client
//! and on a peer's, at a display's 60 and 144 frames a second against the
//! game's 64 Hz physics.

use super::*;
use crate::player::rigged::underfoot::RestsOn;
use avian3d::prelude::{ContactGraph, Sleeping};
use symbios_avatar::Limb;

/// How many fixed steps the frame ran.
#[derive(Resource, Default)]
struct Steps(u32);

fn count_step(mut steps: ResMut<Steps>) {
    steps.0 += 1;
}

/// One frame of a beast standing still, read after it is drawn.
#[derive(Clone, Copy, Debug)]
struct Frame {
    /// Fixed steps the frame ran.
    steps: u32,
    /// Whether the body was handed a patch.
    patched: bool,
    /// Whether the body stands by the terrain (#1834): on a structure it is
    /// handed a patch too, the structure's ground.
    on_terrain: bool,
    /// The trunk's pitch, roll and yaw as drawn, degrees: the line from the
    /// hips' midpoint to the shoulders', and the one from the right girdle
    /// joints' midpoint to the left's.
    pitch: f32,
    roll: f32,
    yaw: f32,
    /// What the owner's contacts say the chassis rests on, as the fill read
    /// them this frame (`None` for a peer).
    contacts: Option<RestsOn>,
    /// The chassis' contact pairs with the terrain: touching, deepest
    /// penetration of any point (metres, negative apart), and the most
    /// upward normal.
    touching: bool,
    deepest: f32,
    normal_y: f32,
    sleeping: bool,
    /// The furthest any joint moved since the last frame, metres.
    moved: f32,
}

/// The trunk's pitch, roll and yaw as drawn, in degrees.
fn trunk(app: &App, root: Entity) -> (f32, f32, f32) {
    let rig = rig_of(app, root);
    let at = drawn(app, root);
    let mean = |pick: &dyn Fn(Limb) -> bool| {
        let tops: Vec<Vec3> = Limb::ALL
            .into_iter()
            .filter(|&limb| pick(limb))
            .filter_map(|limb| rig.limb_chain(limb))
            .map(|[top, _, _]| at[top])
            .collect();
        tops.iter().sum::<Vec3>() / tops.len() as f32
    };
    let (fore, hind) = (mean(&Limb::is_fore), mean(&|limb| !limb.is_fore()));
    let (left, right) = (mean(&Limb::is_left), mean(&|limb| !limb.is_left()));
    let along = fore - hind;
    let across = left - right;
    let flat = |v: Vec3| Vec2::new(v.x, v.z).length();
    (
        along.y.atan2(flat(along)).to_degrees(),
        across.y.atan2(flat(across)).to_degrees(),
        along.x.atan2(along.z).to_degrees(),
    )
}

/// What the contact graph says of `chassis` against the terrain this frame.
fn contacts_of(app: &mut App, chassis: Entity) -> (Option<RestsOn>, bool, f32, f32) {
    let rests = app
        .world_mut()
        .run_system_once(
            move |contacts: Res<ContactGraph>,
                  sensors: Query<Entity, With<avian3d::prelude::Sensor>>,
                  terrain: Query<(), With<crate::terrain::TerrainMesh>>| {
                let mut on = false;
                for (other, _) in crate::player::resting_under(
                    &contacts,
                    &sensors,
                    chassis,
                    crate::player::UNDER_NORMAL_Y,
                ) {
                    if !terrain.contains(other) {
                        return RestsOn::Elsewhere;
                    }
                    on = true;
                }
                if on {
                    RestsOn::Terrain
                } else {
                    RestsOn::Elsewhere
                }
            },
        )
        .expect("the read runs");
    let graph = app.world().resource::<ContactGraph>();
    let (mut touching, mut deepest, mut normal_y) = (false, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for pair in graph.contact_pairs_with(chassis) {
        touching |= pair.is_touching();
        for manifold in &pair.manifolds {
            normal_y = normal_y.max(manifold.normal.y.abs());
            for point in &manifold.points {
                deepest = deepest.max(point.penetration);
            }
        }
    }
    (Some(rests), touching, deepest, normal_y)
}

/// A world for a beast to stand still in: [`grounded`], with the walk's own
/// fixed-step systems for the owner's chassis, as `PlayerPlugin` chains them,
/// and a count of the fixed steps each frame runs.
fn still_world(
    ground: &dyn Fn(f32, f32) -> f32,
    solids: &[(Vec3, Transform)],
    under: Under,
) -> App {
    let mut app = grounded(ground, solids);
    app.init_resource::<Steps>()
        .add_systems(FixedPostUpdate, count_step);
    if under == Under::Peer {
        return app;
    }
    app.init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(WaterSurfaces { planes: Vec::new() })
        .init_resource::<crate::player::humanoid::JumpQueued>()
        .add_systems(
            FixedUpdate,
            (
                crate::player::preset::sync_chassis,
                crate::player::humanoid::end_lying_turns,
                crate::player::humanoid::apply_humanoid_walk,
                crate::player::humanoid::clear_jump_queue,
            )
                .chain(),
        );
    app
}

/// A `family` beast standing still at `place` over `ground` on either client,
/// its frames run at `hz`: the app, the chassis and the root, once it has
/// settled there a second. A peer stands where the owner's chassis came to
/// rest.
fn standing_still(
    ground: &dyn Fn(f32, f32) -> f32,
    solids: &[(Vec3, Transform)],
    under: Under,
    family: Family,
    place: Transform,
    hz: f32,
) -> (App, Entity, Entity) {
    let place = match under {
        Under::Local => place,
        Under::Peer => {
            let (app, chassis, _) = standing_still(ground, solids, Under::Local, family, place, hz);
            *app.world().get::<Transform>(chassis).expect("a chassis")
        }
    };
    let mut app = still_world(ground, solids, under);
    let (chassis, root) = standing(&mut app, under, family, place);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        1.0 / hz,
    )));
    for _ in 0..hz as usize {
        app.update();
    }
    (app, chassis, root)
}

/// Run `frames` frames, calling `before` ahead of each with its index, and
/// read each after it is drawn.
fn frames_of(
    app: &mut App,
    chassis: Entity,
    root: Entity,
    under: Under,
    frames: usize,
    mut before: impl FnMut(&mut App, usize),
) -> Vec<Frame> {
    let mut log = Vec::with_capacity(frames);
    let mut was = drawn(app, root);
    for frame in 0..frames {
        before(app, frame);
        app.world_mut().resource_mut::<Steps>().0 = 0;
        app.update();
        let (pitch, roll, yaw) = trunk(app, root);
        let now = drawn(app, root);
        let moved = super::moved(&was, &now);
        was = now;
        let (contacts, touching, deepest, normal_y) = match under {
            Under::Local => contacts_of(app, chassis),
            Under::Peer => (None, false, f32::NAN, f32::NAN),
        };
        log.push(Frame {
            steps: app.world().resource::<Steps>().0,
            patched: app
                .world()
                .get::<Drive>(root)
                .expect("a drive")
                .ground
                .is_some(),
            on_terrain: held(app, root).1,
            pitch,
            roll,
            yaw,
            contacts,
            touching,
            deepest,
            normal_y,
            sleeping: app.world().get::<Sleeping>(chassis).is_some(),
            moved,
        });
    }
    log
}

/// The largest one-frame change of the trunk's pitch, roll or yaw over `log`,
/// degrees, and the frames on which it changed more than `over`.
fn jumps(log: &[Frame], over: f32) -> (f32, Vec<usize>) {
    let mut worst = 0.0f32;
    let mut found = Vec::new();
    for (i, pair) in log.windows(2).enumerate() {
        let turned = (pair[1].pitch - pair[0].pitch)
            .abs()
            .max((pair[1].roll - pair[0].roll).abs())
            .max(turned_by(pair[0].yaw, pair[1].yaw));
        worst = worst.max(turned);
        if turned > over {
            found.push(i + 1);
        }
    }
    (worst, found)
}

fn turned_by(a: f32, b: f32) -> f32 {
    let d = (b - a).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// A browser's frame `frame` at `hz`, seconds: a quarter either side of its
/// period, and every 97th frame a 70 ms hitch, which runs four or five fixed
/// steps at once.
fn browser_frame(hz: f32, frame: usize) -> f32 {
    if frame % 97 == 96 {
        return 0.07;
    }
    // A fixed scramble of the frame's index, so a run is the same every time.
    let scrambled = (frame as u32).wrapping_mul(2_654_435_761) >> 16;
    let unit = scrambled as f32 / 65_535.0 * 2.0 - 1.0;
    (1.0 + 0.25 * unit) / hz
}

/// A ground's height at each world `(x, z)`.
type Ground = Box<dyn Fn(f32, f32) -> f32>;

/// The grounds a beast stands still on, by name: slopes rising toward the
/// chassis' forward and off to its left, so the trunk both pitches and rolls,
/// and rolling ground.
fn grounds() -> Vec<(String, Ground)> {
    let mut grounds: Vec<(String, Ground)> = Vec::new();
    for degrees in [0.0f32, 10.0, 20.0, 25.0] {
        let grade = degrees.to_radians().tan();
        // Uphill toward -Z turned a third of a radian toward -X.
        let uphill = Vec2::new(-0.33f32.sin(), -0.33f32.cos());
        grounds.push((
            format!("{degrees} deg"),
            Box::new(move |x, z| grade * (x * uphill.x + z * uphill.y)),
        ));
    }
    grounds.push((
        "rolling".to_string(),
        Box::new(|x, z| 1.2 * (x / 4.0).sin() * (z / 5.0).cos() + 0.4 * (z / 3.0).sin()),
    ));
    grounds
}

/// **A measurement (#1835): does a beast standing still twitch?** A dog and
/// a horse stand 10 s on every ground of [`grounds`], on its owner's client
/// and a peer's, at 60 and 144 frames a second. Printed per run: how many
/// frames had no fixed step or two, how often the patch came or went, how
/// often the owner's contacts said anything but the terrain, how many frames
/// the trunk turned more than half a degree, and the worst; and each such
/// frame with what the physics said around it.
#[test]
#[ignore = "a measurement, printed: run by hand"]
fn probe_a_beast_standing_still() {
    for (name, ground) in grounds() {
        for family in [Family::Dog, Family::Horse] {
            let record = beast_record(family);
            let place = Transform::from_xyz(0.0, ground(0.0, 0.0) + drop_of(&record) + 0.02, 0.0);
            for under in [Under::Local, Under::Peer] {
                for (hz, jitter) in [
                    (60.0f32, false),
                    (144.0, false),
                    (60.0, true),
                    (144.0, true),
                ] {
                    let (mut app, chassis, root) =
                        standing_still(&*ground, &[], under, family, place, hz);
                    let log = frames_of(
                        &mut app,
                        chassis,
                        root,
                        under,
                        (10.0 * hz) as usize,
                        |app, frame| {
                            if jitter {
                                app.insert_resource(TimeUpdateStrategy::ManualDuration(
                                    Duration::from_secs_f32(browser_frame(hz, frame)),
                                ));
                            }
                        },
                    );
                    let toggles = log
                        .windows(2)
                        .filter(|pair| pair[0].patched != pair[1].patched)
                        .count();
                    let elsewhere = log
                        .iter()
                        .filter(|frame| frame.contacts == Some(RestsOn::Elsewhere))
                        .count();
                    let unpatched = log.iter().filter(|frame| !frame.patched).count();
                    let idle = log.iter().filter(|frame| frame.steps == 0).count();
                    let double = log.iter().filter(|frame| frame.steps >= 2).count();
                    let asleep = log.iter().filter(|frame| frame.sleeping).count();
                    let (worst, found) = jumps(&log, 0.5);
                    let mut moves: Vec<f32> = log.iter().map(|frame| frame.moved).collect();
                    moves.sort_by(f32::total_cmp);
                    let (deep_lo, deep_hi) = log
                        .iter()
                        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), frame| {
                            (lo.min(frame.deepest), hi.max(frame.deepest))
                        });
                    println!(
                        "{name} {family:?} {under:?} {hz} Hz{}: joints move median {:.2} mm, \
                         worst {:.2} mm; deepest {:+.4}..{:+.4}",
                        if jitter { " jittered" } else { "" },
                        moves[moves.len() / 2] * 1000.0,
                        moves[moves.len() - 1] * 1000.0,
                        deep_lo,
                        deep_hi,
                    );
                    println!(
                        "    {idle} frames no step, {double} \
                         two; unpatched {unpatched}, {toggles} toggles, contacts elsewhere \
                         {elsewhere}, asleep {asleep}; {} frames turn > 0.5 deg, worst {worst:.3} \
                         deg; pitch {:.2} roll {:.2}",
                        found.len(),
                        log[0].pitch,
                        log[0].roll,
                    );
                    for &i in found.iter().take(6) {
                        let from = i.saturating_sub(2);
                        for (j, &f) in log.iter().enumerate().take(i + 2).skip(from) {
                            println!(
                                "    frame {j}{}: steps {} patched {} contacts {:?} touching {} \
                                 deepest {:+.4} normal {:.3} asleep {} pitch {:.3} roll {:.3} \
                                 yaw {:.3} moved {:.2} mm",
                                if j == i { "*" } else { " " },
                                f.steps,
                                f.patched,
                                f.contacts,
                                f.touching,
                                f.deepest,
                                f.normal_y,
                                f.sleeping,
                                f.pitch,
                                f.roll,
                                f.yaw,
                                f.moved * 1000.0
                            );
                        }
                    }
                }
            }
        }
    }
}

/// The joint that moved furthest between two readings, and its zone.
fn furthest(rig: &Rig, from: &[Vec3], to: &[Vec3]) -> (usize, Zone, f32) {
    let (joint, far) = from
        .iter()
        .zip(to)
        .map(|(a, b)| a.distance(*b))
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .expect("a joint");
    (joint, rig.joints[joint].zone, far)
}

/// **A measurement (#1835): what a beast's decision does while it walks.** A
/// dog and a horse on its owner's client walk two seconds up, down and across
/// a 20-degree hill and over rolling ground, at 60 and 144 frames a second,
/// and stand two more. Printed: how many frames the contacts said anything but
/// the terrain, how often the patch came or went, and the trunk's worst
/// one-frame turn on the frames the patch came or went and on the others.
#[test]
#[ignore = "a measurement, printed: run by hand"]
fn probe_a_beast_walking_the_hill() {
    let grounds = grounds();
    for (name, ground) in grounds.iter().filter(|(name, _)| name != "0 deg") {
        for family in [Family::Dog, Family::Horse] {
            let record = beast_record(family);
            let place = Transform::from_xyz(0.0, ground(0.0, 0.0) + drop_of(&record) + 0.02, 0.0);
            for key in [KeyCode::KeyW, KeyCode::KeyS, KeyCode::KeyA] {
                for hz in [60.0f32, 144.0] {
                    let (mut app, chassis, root) =
                        standing_still(&**ground, &[], Under::Local, family, place, hz);
                    let rig = rig_of(&app, root);
                    let frames = (4.0 * hz) as usize;
                    let mut was = drawn(&app, root);
                    let mut spikes = Vec::new();
                    let log = frames_of(&mut app, chassis, root, Under::Local, frames, |app, i| {
                        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
                        if i < frames / 2 {
                            input.press(key);
                        } else {
                            input.release(key);
                        }
                        let now = drawn(app, root);
                        let (joint, zone, far) = furthest(&rig, &was, &now);
                        if far > 0.01 && i > frames / 2 + hz as usize / 2 {
                            spikes.push((i, joint, zone, far));
                        }
                        was = now;
                    });
                    let elsewhere = log
                        .iter()
                        .filter(|frame| frame.contacts == Some(RestsOn::Elsewhere))
                        .count();
                    let (mut at_toggle, mut otherwise, mut toggles) = (0.0f32, 0.0f32, 0);
                    for pair in log.windows(2) {
                        let turned = (pair[1].pitch - pair[0].pitch)
                            .abs()
                            .max((pair[1].roll - pair[0].roll).abs());
                        if pair[0].patched != pair[1].patched {
                            toggles += 1;
                            at_toggle = at_toggle.max(turned);
                        } else {
                            otherwise = otherwise.max(turned);
                        }
                    }
                    println!(
                        "{name} {family:?} walking {key:?} {hz} Hz: contacts elsewhere {elsewhere} \
                         of {frames}, {toggles} toggles, trunk turns up to {at_toggle:.2} deg on \
                         them and {otherwise:.2} deg otherwise; standing, {} joint spikes over \
                         1 cm: {:?}",
                        spikes.len(),
                        spikes.iter().take(4).collect::<Vec<_>>()
                    );
                    let big = log.windows(2).position(|pair| {
                        pair[0].patched == pair[1].patched
                            && (pair[1].pitch - pair[0].pitch)
                                .abs()
                                .max((pair[1].roll - pair[0].roll).abs())
                                > 3.0
                    });
                    let first = log
                        .iter()
                        .position(|frame| frame.contacts == Some(RestsOn::Elsewhere))
                        .or(big.map(|i| i + 1));
                    if let Some(i) = first {
                        let from = i.saturating_sub(1);
                        for (j, &f) in log.iter().enumerate().take(i + 4).skip(from) {
                            println!(
                                "    frame {j}: steps {} patched {} contacts {:?} touching {} \
                                 deepest {:+.4} normal {:.3} pitch {:.2} roll {:.2} yaw {:.2}",
                                f.steps,
                                f.patched,
                                f.contacts,
                                f.touching,
                                f.deepest,
                                f.normal_y,
                                f.pitch,
                                f.roll,
                                f.yaw
                            );
                        }
                    }
                }
            }
        }
    }
}

/// How far a still beast's trunk may turn in a frame, degrees (#1835): three
/// times the most its idle turns it in a frame, 0.08 degrees on any ground of
/// [`grounds`] at either rate, and a seventy-fifth of the 19 degrees a frame
/// read on the level floor snapped it on the hill of [`hill`].
const STILL_TURN: f32 = 0.25;

/// A 20-degree hill rising toward the chassis' forward and off to its left.
fn hill() -> impl Fn(f32, f32) -> f32 + Copy {
    let grade = 20f32.to_radians().tan();
    let uphill = Vec2::new(-0.33f32.sin(), -0.33f32.cos());
    move |x, z| grade * (x * uphill.x + z * uphill.y)
}

/// How much of the terrain `root`'s patch carries, and whether it stands by
/// the terrain.
fn held(app: &App, root: Entity) -> (f32, bool) {
    app.world()
        .get::<RiggedTrail>(root)
        .expect("a trail")
        .rest
        .held()
}

/// **A beast standing still on a hill holds its trunk still** (#1835). A dog
/// and a horse stand 10 s on 20 degrees, on its owner's client and a peer's,
/// at 60 and 144 frames a second against the game's 64 Hz physics: the patch
/// is handed over on every frame and the trunk turns no more than
/// [`STILL_TURN`] in any. The physics does not lose a capsule at rest - it is
/// the turning and walking one that a step loses, which the guards after this
/// one put through - so this holds of the code before #1835 too: it is the
/// floor under the others, that a still beast's idle is still.
#[test]
fn a_beast_standing_still_on_a_hill_holds_its_trunk() {
    let ground = hill();
    for family in [Family::Dog, Family::Horse] {
        let record = beast_record(family);
        let place = Transform::from_xyz(0.0, ground(0.0, 0.0) + drop_of(&record) + 0.02, 0.0);
        for under in [Under::Local, Under::Peer] {
            for hz in [60.0f32, 144.0] {
                let (mut app, chassis, root) =
                    standing_still(&ground, &[], under, family, place, hz);
                let log = frames_of(
                    &mut app,
                    chassis,
                    root,
                    under,
                    (10.0 * hz) as usize,
                    |_, _| {},
                );
                assert!(
                    log.iter().all(|frame| frame.patched),
                    "{family:?} {under:?} at {hz} Hz: handed the level floor standing on the hill"
                );
                let (worst, found) = jumps(&log, STILL_TURN);
                assert!(
                    found.is_empty(),
                    "{family:?} {under:?} at {hz} Hz: the trunk turned up to {worst:.3} deg in a \
                     frame standing still, on frames {found:?}"
                );
            }
        }
    }
}

/// **An answer lost for a frame or three moves nothing** (#1835). A dog and
/// a horse stand on 20 degrees, on either client at 60 and 144 frames a
/// second, and the terrain under them is marked outgoing - what a tile swap
/// does, and what the fill reads as resting on something other than the
/// terrain - for one frame and then for three, a peer cast afresh on each:
/// the raw answer is the level floor's on those frames. The answer stood by
/// never leaves the terrain, the patch carries all of it on every frame, and
/// the trunk turns no more than [`STILL_TURN`] in any of the second around
/// them. Read raw, a lost frame snapped the trunk level and back, 19 degrees
/// in a frame each way, on either client, one frame lost or three.
#[test]
fn an_answer_lost_for_a_frame_or_three_moves_nothing() {
    let ground = hill();
    for family in [Family::Dog, Family::Horse] {
        let record = beast_record(family);
        let place = Transform::from_xyz(0.0, ground(0.0, 0.0) + drop_of(&record) + 0.02, 0.0);
        for under in [Under::Local, Under::Peer] {
            for hz in [60.0f32, 144.0] {
                let (mut app, chassis, root) =
                    standing_still(&ground, &[], under, family, place, hz);
                let terrain = app
                    .world_mut()
                    .query_filtered::<Entity, With<crate::terrain::TerrainMesh>>()
                    .single(app.world())
                    .expect("one terrain");
                for lost in [1usize, 3] {
                    let start = hz as usize / 4;
                    let mut shares = Vec::new();
                    let log = frames_of(&mut app, chassis, root, under, hz as usize, |app, i| {
                        let losing = (start..start + lost).contains(&i);
                        if losing {
                            app.world_mut()
                                .entity_mut(terrain)
                                .insert(crate::terrain::OutgoingTerrain);
                        } else {
                            app.world_mut()
                                .entity_mut(terrain)
                                .remove::<crate::terrain::OutgoingTerrain>();
                        }
                        // A peer's answer goes through on the frame it changes.
                        if (start..=start + lost).contains(&i) {
                            app.world_mut()
                                .get_mut::<RiggedTrail>(root)
                                .expect("a trail")
                                .rest
                                .forget_cast();
                        }
                        if i > 0 {
                            shares.push(held(app, root));
                        }
                    });
                    let (worst, found) = jumps(&log, STILL_TURN);
                    println!(
                        "{family:?} {under:?} at {hz} Hz, {lost} frames lost: the trunk turning \
                         up to {worst:.3} deg in a frame"
                    );
                    assert!(
                        shares.iter().all(|&(share, on)| share == 1.0 && on),
                        "{family:?} {under:?} at {hz} Hz, {lost} frames lost: the patch carried \
                         {shares:?}"
                    );
                    assert!(
                        log.iter().all(|frame| frame.patched) && found.is_empty(),
                        "{family:?} {under:?} at {hz} Hz, {lost} frames lost: the trunk turned up \
                         to {worst:.3} deg in a frame, on frames {found:?}"
                    );
                }
            }
        }
    }
}

/// **An owner's beast frozen on a hill keeps its hill** (#1894). The Avatar
/// editor's freeze (`player::freeze_local_avatar_while_editing`) disables the
/// chassis' body, which takes every contact it had: read off its contacts, a
/// frozen owner rests on nothing, and past the hold the patch eased out to the
/// level floor under a gizmo - feet sunk into the hill on one side and over it
/// on the other - and back in after the release. A dog and a horse stand on 20
/// degrees at 60 frames a second, frozen for three seconds - many times the
/// hold and the ease - and released: the chassis has no contact while frozen
/// (the fixture), and the answer stood by is the terrain with the whole of its
/// share on every frame, through the freeze and after it, and the trunk turns
/// no more than [`STILL_TURN`] in any frame.
#[test]
fn an_owners_beast_frozen_on_a_hill_keeps_its_hill() {
    let ground = hill();
    let hz = 60.0f32;
    let (freeze, release) = (hz as usize / 4, hz as usize / 4 + 3 * hz as usize);
    for family in [Family::Dog, Family::Horse] {
        let record = beast_record(family);
        let place = Transform::from_xyz(0.0, ground(0.0, 0.0) + drop_of(&record) + 0.02, 0.0);
        let (mut app, chassis, root) =
            standing_still(&ground, &[], Under::Local, family, place, hz);
        let mut shares = Vec::new();
        let mut disabled = Vec::new();
        let log = frames_of(
            &mut app,
            chassis,
            root,
            Under::Local,
            release + hz as usize,
            |app, i| {
                if i > 0 {
                    shares.push(held(app, root));
                }
                app.world_mut()
                    .resource_mut::<crate::player::RigHold>()
                    .still = (freeze..release).contains(&i);
                app.world_mut()
                    .run_system_once(crate::player::freeze_local_avatar_while_editing)
                    .expect("the freeze runs");
                disabled.push(
                    app.world()
                        .get::<avian3d::prelude::RigidBodyDisabled>(chassis)
                        .is_some(),
                );
            },
        );
        shares.push(held(&app, root));
        assert!(
            disabled
                .iter()
                .enumerate()
                .all(|(i, &off)| off == (freeze..release).contains(&i)),
            "{family:?}: the fixture: the freeze did not hold frames {freeze} to {release} alone"
        );
        assert!(
            log[freeze + 1..release]
                .iter()
                .all(|frame| frame.contacts == Some(RestsOn::Elsewhere) && !frame.touching),
            "{family:?}: the fixture: a frozen chassis kept a contact"
        );
        let lost: Vec<(usize, (f32, bool))> = shares
            .iter()
            .copied()
            .enumerate()
            .filter(|&(_, (share, on))| share != 1.0 || !on)
            .collect();
        assert!(
            lost.is_empty(),
            "{family:?}: frozen from frame {freeze} to {release}, the patch carried (frame, \
             (share, stands by the terrain)) {lost:?}"
        );
        let (worst, found) = jumps(&log, STILL_TURN);
        assert!(
            log.iter().all(|frame| frame.patched) && found.is_empty(),
            "{family:?}: the trunk turned up to {worst:.3} deg in a frame, on frames {found:?}"
        );
    }
}

/// A `family` beast on its owner's client set down at `place` over `ground`
/// and `solids`, walked by `key` held for the first of `(held, seconds)` at
/// `hz`: every frame of it, from the first the key is down.
fn walked(
    ground: &dyn Fn(f32, f32) -> f32,
    solids: &[(Vec3, Transform)],
    family: Family,
    place: Transform,
    key: KeyCode,
    timing: (f32, f32),
    hz: f32,
) -> Vec<Frame> {
    walked_tuned(ground, solids, family, place, key, timing, hz, |_| {})
}

/// [`walked`], with `tune` run over the app before the first frame.
#[allow(clippy::too_many_arguments)]
fn walked_tuned(
    ground: &dyn Fn(f32, f32) -> f32,
    solids: &[(Vec3, Transform)],
    family: Family,
    place: Transform,
    key: KeyCode,
    (held, seconds): (f32, f32),
    hz: f32,
    tune: impl FnOnce(&mut App),
) -> Vec<Frame> {
    let (mut app, chassis, root) = standing_still(ground, solids, Under::Local, family, place, hz);
    tune(&mut app);
    let pressed = (held * hz) as usize;
    frames_of(
        &mut app,
        chassis,
        root,
        Under::Local,
        (seconds * hz) as usize,
        |app, i| {
            let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            if i < pressed {
                input.press(key);
            } else {
                input.release(key);
            }
        },
    )
}

/// The trunk's largest one-frame change of pitch or roll over `pairs` of
/// frames of `log`, degrees.
fn worst_turn(log: &[Frame], pairs: impl Iterator<Item = usize>) -> f32 {
    pairs
        .filter(|&i| i + 1 < log.len())
        .map(|i| {
            (log[i + 1].pitch - log[i].pitch)
                .abs()
                .max((log[i + 1].roll - log[i].roll).abs())
        })
        .fold(0.0, f32::max)
}

/// **A step the physics loses an owner's beast in moves its trunk no more
/// than any other** (#1835). Turning round on the spot and walking on a hill
/// or over rolling ground, the owner's own capsule is not touching the
/// terrain for a fixed step now and then, and its contacts say it rests on
/// nothing. On avian 0.7, before #1820, a dog walking down 10 degrees turning
/// round as it went, and a horse walking ahead and to its left over rolling
/// ground - each spinning on after its key was let go, as beasts did then -
/// read 21 frames off such steps between them at 60 and 144 frames a second
/// (at least one is asserted, so the guard is not empty). Read raw, each
/// snapped the trunk level and back, 9 to 16 degrees in a frame where the walk
/// itself turns it under 4. Held, the frames on and just after a lost step
/// turn the trunk no more than half a degree past the most any other frame of
/// the walk does, and the patch is handed over on every frame.
///
/// The walks run on avian 0.7's contact tolerance, 5 mm. Avian 0.8 raised
/// the default to 2 cm, and the capsule's small hops over these grounds now
/// stay inside it: on 0.8's own tolerance the six walks lose no step at all,
/// and the guard would put nothing through. A step can still be lost - a
/// hop past 2 cm, a bump the walk is not shown here - and the hold is what
/// meets it, so the walks are run where they lose steps rather than the
/// floor below being dropped. On 0.8 at 5 mm only the horse's walks lost
/// any: 9 frames between them, where 0.7 lost 21 - every one over a second
/// after its key was let go, while the horse spun on toward the creep its
/// own spin made. A beast let go stops turning since #1820, and those walks
/// lose none; a cat setting off to its left across 10 degrees, turning as
/// it goes, loses one at 0.6 s (two at 144 Hz), the walk that puts the hold
/// through now.
///
/// **A knife edge**: that cat walk alone keeps `lost > 0`, on a single lost
/// step at 60 Hz (and two at 144). A physics, gait or contact change can tip
/// it to none, and the guard then fails with nothing wrong in the hold. When
/// it does, do not drop the floor: run the test with `--nocapture` (each walk
/// prints its lost frames), find a walk that loses a step again - another
/// family, key or ground, or a tighter contact tolerance - and add it to the
/// list, so the hold is put through by something.
#[test]
fn a_step_the_physics_loses_moves_the_trunk_no_more_than_any_other() {
    let grounds = grounds();
    let ground = |name: &str| {
        &grounds
            .iter()
            .find(|(named, _)| named == name)
            .expect("a ground")
            .1
    };
    let mut lost = 0;
    for (name, family, key) in [
        ("10 deg", Family::Dog, KeyCode::KeyS),
        ("rolling", Family::Horse, KeyCode::KeyW),
        ("rolling", Family::Horse, KeyCode::KeyA),
        ("10 deg", Family::Cat, KeyCode::KeyA),
    ] {
        let ground = ground(name);
        let record = beast_record(family);
        let place = Transform::from_xyz(0.0, ground(0.0, 0.0) + drop_of(&record) + 0.02, 0.0);
        for hz in [60.0f32, 144.0] {
            let log = walked_tuned(&**ground, &[], family, place, key, (2.0, 4.0), hz, |app| {
                app.world_mut()
                    .resource_mut::<avian3d::prelude::NarrowPhaseConfig>()
                    .contact_tolerance = 0.005;
            });
            let near: Vec<usize> = log
                .iter()
                .enumerate()
                .filter(|(_, frame)| frame.contacts == Some(RestsOn::Elsewhere))
                .map(|(i, _)| i)
                .collect();
            lost += near.len();
            let close = |i: usize| {
                near.iter()
                    .any(|&at| (at.saturating_sub(1)..=at + 2).contains(&i))
            };
            let (on, off) = (
                worst_turn(&log, (0..log.len()).filter(|&i| close(i))),
                worst_turn(&log, (0..log.len()).filter(|&i| !close(i))),
            );
            println!(
                "{family:?} walking {key:?} on {name} at {hz} Hz: {} frames lost, the trunk \
                 turning {on:.2} deg by them and {off:.2} on any other",
                near.len()
            );
            assert!(
                log.iter().all(|frame| frame.patched),
                "{family:?} walking {key:?} on {name} at {hz} Hz: handed the level floor on the \
                 terrain"
            );
            assert!(
                on <= off + 0.5,
                "{family:?} walking {key:?} on {name} at {hz} Hz: the trunk turned {on:.2} deg in \
                 a frame by the {} steps the physics lost it, and at most {off:.2} on any other",
                near.len()
            );
        }
    }
    assert!(
        lost > 0,
        "the physics lost no step: this guard puts nothing through"
    );
}

/// A level pad cut into a 20-degree hill at its uphill edge, as a seeded
/// building's is: the hill rising toward -Z, and the pad from z = 0, where
/// its top is the terrain's, to z = 6, standing ever higher over it.
fn pad_on_a_hill() -> (impl Fn(f32, f32) -> f32 + Copy, (Vec3, Transform)) {
    let grade = 20f32.to_radians().tan();
    (
        move |_: f32, z: f32| -grade * z,
        block(
            Vec3::new(-3.0, -6.0 * grade - 1.0, 0.0),
            Vec3::new(3.0, 0.0, 6.0),
        ),
    )
}

/// A `family` beast on its owner's client walking 4 s at `hz` down the hill of
/// [`pad_on_a_hill`] onto its pad, or up off the pad onto the hill: the trunk's
/// worst one-frame turn, degrees, from a little before its chassis first
/// touches what it is walking onto to a little after the ease has run its
/// course, how often it came onto the terrain or left it, and every frame.
fn walked_onto_the_pad(family: Family, onto: bool, hz: f32) -> (f32, usize, Vec<Frame>) {
    let (ground, pad) = pad_on_a_hill();
    let record = beast_record(family);
    // Facing down the hill onto the pad, walked back (S, toward +Z); facing
    // up it off the pad, walked ahead (W). Neither turns round.
    let (z, turn, key) = if onto {
        (
            -2.5,
            Quat::from_rotation_y(std::f32::consts::PI),
            KeyCode::KeyS,
        )
    } else {
        (2.5, Quat::IDENTITY, KeyCode::KeyW)
    };
    let over = if onto { ground(0.0, z) } else { 0.0 };
    let place = Transform::from_xyz(0.0, over + drop_of(&record) + 0.02, z).with_rotation(turn);
    let log = walked(&ground, &[pad], family, place, key, (4.0, 4.0), hz);
    let toggles = log
        .windows(2)
        .filter(|pair| pair[0].on_terrain != pair[1].on_terrain)
        .count();
    let toggle = log
        .windows(2)
        .position(|pair| pair[0].on_terrain != pair[1].on_terrain)
        .map_or(log.len(), |i| i + 1);
    // The first frame the contacts say something else than they did.
    let touched = log
        .iter()
        .position(|frame| frame.contacts != log[0].contacts)
        .unwrap_or(toggle);
    let margin = (0.08 * hz) as usize;
    let window = touched.min(toggle).saturating_sub(margin)..toggle + (0.25 * hz) as usize;
    (worst_turn(&log, window), toggles, log)
}

/// **A beast walking onto a level pad on a hill turns its trunk level over a
/// fifth of a second, not in a frame** (#1835). A dog and a horse on their
/// owner's client walk down a 20-degree hill onto a pad cut into it at its
/// uphill edge, and up off it onto the hill, at 60 and 144 frames a second.
/// Each comes onto the pad or off it - leaving the terrain or coming onto it
/// once, the pad's ground easing in for the hill's or out (#1834) - and
/// from a little before its chassis touches the pad or the hill to a little
/// after the ease has run, no frame turns the trunk more than the ease does
/// in a frame at most, and half a degree for the stride: 1.5 times a frame's
/// share of the agreed fifth of a second, of the hill's whole 20 degrees -
/// more than any trunk pitches with it - 2.5 degrees at 60 Hz and 1.04 at
/// 144. Read raw, the trunk turned 4.4 to 5.4 degrees in the frame the
/// chassis touched the pad; eased, 1.7 at most.
#[test]
fn a_beast_walking_onto_a_pad_on_a_hill_turns_its_trunk_smoothly() {
    for family in [Family::Dog, Family::Horse] {
        for onto in [true, false] {
            for hz in [60.0f32, 144.0] {
                let (worst, toggles, _) = walked_onto_the_pad(family, onto, hz);
                // The agreed ease, not the constant: a shorter one fails this.
                let ease = 1.5 / hz / 0.2 * 20.0;
                let way = if onto { "onto" } else { "off" };
                println!(
                    "{family:?} walking {way} the pad at {hz} Hz: {toggles} toggles, the trunk \
                     turning {worst:.2} deg by them, {ease:.2} for the ease"
                );
                assert_eq!(
                    toggles, 1,
                    "{family:?} walking {way} the pad at {hz} Hz: it came onto the terrain or \
                     left it {toggles} times"
                );
                assert!(
                    worst <= ease + 0.5,
                    "{family:?} walking {way} the pad at {hz} Hz: the trunk turned {worst:.2} \
                     deg in a frame as it came {way}, against {ease:.2} for the ease"
                );
            }
        }
    }
}

/// **A beast walking from a hill onto a pad and back keeps a patch, and
/// turns its trunk from the one to the other smoothly** (#1834). A dog and a
/// horse on their owner's client walk down the 20-degree hill of
/// [`pad_on_a_hill`] onto its pad and up off it, at 30, 60 and 144 frames a
/// second: they are handed a patch on every frame - the hill's ground, then
/// the pad's, eased from one to the other - and from a little before the
/// chassis touches what it walks onto to a little after the ease has run, no
/// frame turns the trunk more than the ease's own most in a frame of a trunk
/// turned 19 degrees by the hill, 2.4 degrees at 60 Hz and in proportion to
/// the frame at the others ([`super::super::underfoot::BLEND_SECS`]). Until
/// #1834 the patch went on the pad, the level floor in its place.
#[test]
fn a_beast_walking_between_a_hill_and_a_pad_keeps_a_patch() {
    for family in [Family::Dog, Family::Horse] {
        for onto in [true, false] {
            for hz in [30.0f32, 60.0, 144.0] {
                let (worst, _, log) = walked_onto_the_pad(family, onto, hz);
                let way = if onto { "onto" } else { "off" };
                let most = 2.4 * 60.0 / hz;
                let unpatched = log.iter().filter(|frame| !frame.patched).count();
                println!(
                    "{family:?} walking {way} the pad at {hz} Hz: {unpatched} frames unpatched, \
                     the trunk turning {worst:.2} deg in a frame at most"
                );
                assert_eq!(
                    unpatched,
                    0,
                    "{family:?} walking {way} the pad at {hz} Hz: handed the level floor on \
                     {unpatched} of {} frames",
                    log.len()
                );
                assert!(
                    worst <= most,
                    "{family:?} walking {way} the pad at {hz} Hz: the trunk turned {worst:.2} deg \
                     in a frame, past {most:.2}"
                );
            }
        }
    }
}

/// **A measurement (#1835): the trunk frame by frame as a beast walks onto
/// the pad of [`pad_on_a_hill`] and off it.** Printed: the frame it came
/// onto the terrain or left it, and each frame's turn of pitch and roll
/// around it.
#[test]
#[ignore = "a measurement, printed: run by hand"]
fn probe_walking_onto_the_pad() {
    let (ground, pad) = pad_on_a_hill();
    for family in [Family::Dog, Family::Horse] {
        let record = beast_record(family);
        for onto in [true, false] {
            let (z, turn, key) = if onto {
                (
                    -2.5,
                    Quat::from_rotation_y(std::f32::consts::PI),
                    KeyCode::KeyS,
                )
            } else {
                (2.5, Quat::IDENTITY, KeyCode::KeyW)
            };
            let over = if onto { ground(0.0, z) } else { 0.0 };
            let place =
                Transform::from_xyz(0.0, over + drop_of(&record) + 0.02, z).with_rotation(turn);
            for hz in [60.0f32, 144.0] {
                let log = walked(&ground, &[pad], family, place, key, (4.0, 4.0), hz);
                let plain = walked(&ground, &[], family, place, key, (4.0, 4.0), hz);
                let skip = (0.6 * hz) as usize;
                let toggle = log
                    .windows(2)
                    .position(|pair| pair[0].on_terrain != pair[1].on_terrain)
                    .map_or(0, |i| i + 1);
                let elsewhere = log
                    .iter()
                    .position(|frame| frame.contacts == Some(RestsOn::Elsewhere));
                let worst_at = (skip..log.len() - 1).max_by(|&a, &b| {
                    worst_turn(&log, a..a + 1).total_cmp(&worst_turn(&log, b..b + 1))
                });
                let plain_at = (skip..plain.len() - 1).max_by(|&a, &b| {
                    worst_turn(&plain, a..a + 1).total_cmp(&worst_turn(&plain, b..b + 1))
                });
                println!(
                    "{family:?} {} at {hz} Hz: first elsewhere {elsewhere:?}, toggle \
                     {toggle}, worst {:.2} at {worst_at:?}, plain worst {:.2} at {plain_at:?}",
                    if onto { "onto" } else { "off" },
                    worst_turn(&log, skip..log.len()),
                    worst_turn(&plain, skip..plain.len()),
                );
                let from = elsewhere.unwrap_or(toggle).saturating_sub(4);
                let series: Vec<String> = (from..(from + (0.5 * hz) as usize).min(log.len() - 1))
                    .map(|i| format!("{:.2}", worst_turn(&log, i..i + 1)))
                    .collect();
                println!("    from {from}: {}", series.join(" "));
            }
        }
    }
}

/// The drawn soles of `root`'s body as far as each stands under its rigged
/// root: the deepest, metres, positive under it.
fn deepest_sole(app: &App, root: Entity) -> f32 {
    let at = app
        .world()
        .get::<GlobalTransform>(root)
        .expect("a placed root")
        .translation()
        .y;
    drawn_soles(app, root)
        .into_iter()
        .map(|sole| at - sole.y)
        .fold(f32::NEG_INFINITY, f32::max)
}

/// One frame of a beast let fall onto a hill: whether it was falling (the
/// owner's contacts, or a peer's pose, off the ground), how far its chassis
/// stood over where it rests, the deepest sole under its root, and whether it
/// was handed a patch.
#[derive(Clone, Copy, Debug)]
struct Fall {
    falling: bool,
    over: f32,
    deepest: f32,
    /// The most any leg is stretched, against how it stood before the lift
    /// ([`leg_stretches`]).
    stretch: f32,
    /// How far the trunk has sunk toward the root, against how it stood
    /// before the lift ([`trunk_height`]), metres.
    sunk: f32,
    patched: bool,
}

/// How high over its root `root`'s body carries its trunk as it is drawn: the
/// midpoint of the hips and shoulders the legs it stands on hang from.
fn trunk_height(app: &App, root: Entity) -> f32 {
    let rig = rig_of(app, root);
    let at = drawn(app, root);
    let tops: Vec<f32> = rig
        .ground_contacts()
        .into_iter()
        .filter_map(|limb| rig.limb_chain(limb).map(|[top, ..]| at[top].y))
        .collect();
    let over = app
        .world()
        .get::<GlobalTransform>(root)
        .expect("a placed root")
        .translation()
        .y;
    tops.iter().sum::<f32>() / tops.len() as f32 - over
}

/// How far each leg `root`'s body stands on is stretched as it is drawn: its
/// sole's distance from its own hip or shoulder, over the leg's reach
/// ([`Rig::limb_reach`], which leaves the foot out, so a leg standing reads
/// about 1.3).
fn leg_stretches(app: &App, root: Entity) -> Vec<f32> {
    let rig = rig_of(app, root);
    let at = drawn(app, root);
    rig.ground_contacts()
        .into_iter()
        .filter_map(|limb| {
            let [top, ..] = rig.limb_chain(limb)?;
            let sole = rig
                .extremity_joints(limb)
                .into_iter()
                .map(|joint| at[joint])
                .min_by(|a, b| a.y.total_cmp(&b.y))?;
            Some(sole.distance(at[top]) / rig.limb_reach(limb)?)
        })
        .collect()
}

/// A `family` beast on `under`'s client standing still on [`hill`] at `hz`,
/// lifted `by` metres in a frame - as a respawn sets a body down, a metre
/// over the ground - and let fall back onto it: each frame from the lift. A
/// peer is played out falling as its owner's chassis does.
fn fallen(under: Under, family: Family, by: f32, hz: f32) -> Vec<Fall> {
    let ground = hill();
    let record = beast_record(family);
    let place = Transform::from_xyz(0.0, ground(0.0, 0.0) + drop_of(&record) + 0.02, 0.0);
    let (mut app, chassis, root) = standing_still(&ground, &[], under, family, place, hz);
    let rest = *app.world().get::<Transform>(chassis).expect("a chassis");
    let stood = leg_stretches(&app, root);
    let carried = trunk_height(&app, root);
    let mut log = Vec::new();
    let (mut height, mut speed) = (by, 0.0f32);
    for frame in 0..(1.5 * hz) as usize {
        match under {
            Under::Local if frame == 0 => {
                let lifted = rest.translation + Vec3::Y * by;
                let world = app.world_mut();
                world
                    .get_mut::<Transform>(chassis)
                    .expect("a chassis")
                    .translation = lifted;
                world
                    .get_mut::<avian3d::prelude::Position>(chassis)
                    .expect("a position")
                    .0 = lifted;
                world
                    .get_mut::<avian3d::prelude::LinearVelocity>(chassis)
                    .expect("a velocity")
                    .0 = Vec3::ZERO;
            }
            Under::Local => {}
            Under::Peer => {
                if frame > 0 {
                    speed += 9.81 / hz;
                    height = (height - speed / hz).max(0.0);
                }
                app.world_mut()
                    .get_mut::<Transform>(chassis)
                    .expect("a chassis")
                    .translation = rest.translation + Vec3::Y * height;
            }
        }
        app.update();
        let falling = match under {
            Under::Local => contacts_of(&mut app, chassis).0 != Some(RestsOn::Terrain),
            Under::Peer => height > 0.0,
        };
        let world = app.world();
        log.push(Fall {
            falling,
            over: world
                .get::<Transform>(chassis)
                .expect("a chassis")
                .translation
                .y
                - rest.translation.y,
            deepest: deepest_sole(&app, root),
            stretch: leg_stretches(&app, root)
                .iter()
                .zip(&stood)
                .map(|(now, was)| now / was)
                .fold(0.0, f32::max),
            sunk: carried - trunk_height(&app, root),
            patched: world.get::<Drive>(root).expect("a drive").ground.is_some(),
        });
    }
    log
}

/// How far a falling body's legs may be stretched against how they stood on
/// the hill before the lift ([`leg_stretches`]): a fifth past it. Since
/// symbios-avatar 0.19.6 (its #414) the flight draws a paw the trunk's pitch
/// would sink onto the plane the body left, which reaches a horse peer's leg
/// to 1.10 of how it stood at 144 Hz (every other case 1.00 to 1.08). Feet
/// held on the terrain a metre under a falling body would stretch a leg about
/// twice over, and the whole body sinking onto it is [`FALLING_SINK`]'s to
/// catch (#1836).
const FALLING_STRETCH: f32 = 1.2;

/// How far a falling body's trunk may sink toward its root against how it
/// was carried on the hill before the lift ([`trunk_height`]), metres: the
/// engine's flight carries a horse peer's 0.21 m lower at most, and held on
/// the terrain under it the trunk went down with the feet, the whole metre
/// the body fell.
const FALLING_SINK: f32 = 0.3;

/// How far over the ground a falling body is clear of it, metres: further
/// than a peer resting on it may fall, the band the fill reads the terrain as
/// under a body within (#1835 review).
const CLEAR: f32 = 0.07;

/// **A beast falling onto a hill is not stood on it until it lands, and is
/// stood on it when it does** (#1835 review, findings 1 and 2). A dog and a
/// horse standing on 20 degrees are lifted a metre in a frame, as a respawn
/// sets a body down, and fall back onto it, on either client at 60 and 144
/// frames a second.
///
/// On no frame it is clear of the ground ([`CLEAR`]) is it handed a patch,
/// no leg is then stretched past [`FALLING_STRETCH`] of how it stood, and the
/// trunk is carried no more than [`FALLING_SINK`] lower over the root than it
/// was. Held on the terrain falling away under it, the body was drawn down on
/// the ground as it fell: the trunk a whole metre down toward the root, its
/// feet 1.26 m under it, its legs no longer than they stood.
///
/// **Not how far under the root a sole is.** A peer played out a metre up in
/// one frame reads as launched, and the engine flies it in a leap with the
/// trunk pitched as it took off from the hill, 25 degrees: its hind feet hang
/// under the pitched hips, 0.64 m under the root through symbios-avatar
/// 0.19.4 and 0.97 m since its flight rides the take-off pose, each leg at
/// 1.06 times the length it stood at - a pose, not a fault. And it is handed
/// the hill within two frames of coming to rest on it: held on the level
/// floor instead, its soles stood on the air over the hill and in it for a
/// third of a second.
#[test]
fn a_beast_falling_onto_a_hill_is_stood_on_it_when_it_lands() {
    for family in [Family::Dog, Family::Horse] {
        for under in [Under::Local, Under::Peer] {
            for hz in [60.0f32, 144.0] {
                let log = fallen(under, family, 1.0, hz);
                // Contacts are a step old: at 144 Hz the frames of the lift
                // may run no step and still read the body on the ground.
                let start = log.iter().position(|fall| fall.falling).unwrap_or(0);
                let fell = log[start..].iter().take_while(|fall| fall.falling).count();
                let clear: Vec<Fall> = log
                    .iter()
                    .copied()
                    .filter(|fall| fall.over > CLEAR)
                    .collect();
                let deepest = clear
                    .iter()
                    .map(|fall| fall.deepest)
                    .fold(f32::NEG_INFINITY, f32::max);
                let stretched = clear
                    .iter()
                    .map(|fall| fall.stretch)
                    .fold(f32::NEG_INFINITY, f32::max);
                let sunk = clear
                    .iter()
                    .map(|fall| fall.sunk)
                    .fold(f32::NEG_INFINITY, f32::max);
                let patched = clear.iter().filter(|fall| fall.patched).count();
                let landed = log[start + fell..].iter().position(|fall| fall.patched);
                println!(
                    "{family:?} {under:?} at {hz} Hz: clear of the ground {} frames, patched on \
                     {patched}, a leg stretched {stretched:.2} times as it stood, the trunk sunk \
                     {sunk:.3} m toward the root, the deepest sole {deepest:.3} m under the \
                     root; handed the hill {landed:?} frames after landing",
                    clear.len()
                );
                assert!(
                    start <= 3 && clear.len() > 2 && start + fell < log.len(),
                    "{family:?} {under:?} at {hz} Hz: fell {fell} frames of {}, from frame \
                     {start}, {} of them clear",
                    log.len(),
                    clear.len()
                );
                assert_eq!(
                    patched, 0,
                    "{family:?} {under:?} at {hz} Hz: handed the terrain falling clear of it"
                );
                assert!(
                    stretched <= FALLING_STRETCH && sunk <= FALLING_SINK,
                    "{family:?} {under:?} at {hz} Hz: a leg stretched {stretched:.2} times as it \
                     stood and the trunk sunk {sunk:.3} m as the body fell"
                );
                assert!(
                    landed.is_some_and(|after| after <= 2),
                    "{family:?} {under:?} at {hz} Hz: handed the hill {landed:?} frames after \
                     landing on it"
                );
            }
        }
    }
}

/// **A beast standing elsewhere is let go of the terrain in good time**
/// (#1835 review): the hold and the ease bounded from above, by the agreed
/// figures and not the constants. A dog on either client stands on 20 degrees
/// at 60 and 144 frames a second, and the terrain is marked outgoing for good:
/// the raw answer is the level floor's from then on, with the terrain still
/// under it. Its patch carries all of the terrain for the agreed 0.15 s hold
/// less a frame, has begun to ease out by two frames after it, and is gone
/// two frames after the agreed 0.2 s ease.
#[test]
fn a_beast_standing_elsewhere_is_let_go_of_the_terrain_in_good_time() {
    let ground = hill();
    let record = beast_record(Family::Dog);
    let place = Transform::from_xyz(0.0, ground(0.0, 0.0) + drop_of(&record) + 0.02, 0.0);
    for under in [Under::Local, Under::Peer] {
        for hz in [60.0f32, 144.0] {
            let (mut app, _, root) = standing_still(&ground, &[], under, Family::Dog, place, hz);
            let terrain = app
                .world_mut()
                .query_filtered::<Entity, With<crate::terrain::TerrainMesh>>()
                .single(app.world())
                .expect("one terrain");
            app.world_mut()
                .entity_mut(terrain)
                .insert(crate::terrain::OutgoingTerrain);
            app.world_mut()
                .get_mut::<RiggedTrail>(root)
                .expect("a trail")
                .rest
                .forget_cast();
            let frame = 1.0 / hz;
            let mut log = Vec::new();
            for _ in 0..(0.6 * hz) as usize {
                app.update();
                let patched = app
                    .world()
                    .get::<Drive>(root)
                    .expect("a drive")
                    .ground
                    .is_some();
                log.push((held(&app, root).0, patched));
            }
            let at = |seconds: f32| log[((seconds / frame).round() as usize).min(log.len() - 1)];
            println!(
                "{under:?} at {hz} Hz: share {:.2} at 0.15 s less a frame, {:.2} two frames on, \
                 patched {} two frames past the ease",
                at(0.15 - frame).0,
                at(0.15 + 2.0 * frame).0,
                at(0.35 + 2.0 * frame).1
            );
            assert_eq!(
                at(0.15 - frame).0,
                1.0,
                "{under:?} at {hz} Hz: eased out before the hold ran"
            );
            assert!(
                at(0.15 + 2.0 * frame).0 < 1.0,
                "{under:?} at {hz} Hz: still holding two frames after the agreed hold"
            );
            assert!(
                !at(0.35 + 2.0 * frame).1,
                "{under:?} at {hz} Hz: still handed the terrain two frames after the agreed ease"
            );
        }
    }
}

/// **A beast set down somewhere else stands by what is under it there from
/// its first frame** (#1835 review, finding 3). A dog peer standing on flat
/// terrain is set down 5 m away on a floor laid 3 cm over it - close enough
/// over the terrain to be held on it, had it walked there - at 60 and 144
/// frames a second: it is handed the floor's ground, the whole of it and
/// none of the terrain's, on the first frame it is drawn there (#1834), not
/// a hold and an ease later.
#[test]
fn a_beast_set_down_elsewhere_stands_by_what_is_there_at_once() {
    let flat = |_: f32, _: f32| 0.0;
    let floor = block(Vec3::new(-3.0, -0.5, -8.0), Vec3::new(3.0, 0.03, -3.0));
    let drop = drop_of(&beast_record(Family::Dog));
    for hz in [60.0f32, 144.0] {
        let place = Transform::from_xyz(0.0, drop, 0.0);
        let (mut app, chassis, root) =
            standing_still(&flat, &[floor], Under::Peer, Family::Dog, place, hz);
        assert!(
            held(&app, root).1,
            "at {hz} Hz: not on the terrain to begin with"
        );
        app.world_mut()
            .get_mut::<Transform>(chassis)
            .expect("a chassis")
            .translation = Vec3::new(0.0, 0.03 + drop, -5.0);
        app.update();
        let rest = &app.world().get::<RiggedTrail>(root).expect("a trail").rest;
        assert!(
            rest.held() == (0.0, false) && rest.stands_by() == (Some(RestsOn::Structure), 1.0),
            "at {hz} Hz: set down on the floor, and still handed the terrain: {:?} {:?}",
            rest.held(),
            rest.stands_by()
        );
    }
}

/// Every frame of a `family` beast on `under`'s client standing still 2 s at
/// `place` over `ground` at `hz`, after it has settled there.
fn stood(
    ground: &dyn Fn(f32, f32) -> f32,
    under: Under,
    family: Family,
    place: Transform,
    hz: f32,
) -> Vec<Frame> {
    let (mut app, chassis, root) = standing_still(ground, &[], under, family, place, hz);
    frames_of(
        &mut app,
        chassis,
        root,
        under,
        (2.0 * hz) as usize,
        |_, _| {},
    )
}

/// **A horse across a steep hill or on a crest stays on the terrain** (#1835
/// end review, finding 1). A horse's capsule is wide - its radius 0.44 m - and
/// lying across a hill it touches it on its uphill side, its centre line
/// higher over the ground than the 7 cm a body on the terrain may be, and
/// over a crest's kink it touches the kink between two samples. Standing side
/// on to 31 and 35 degrees, and across a crest whose kink is 0.3 m off its
/// middle on 25 and 30, on either client at 60 and 144 frames a second, and
/// walking across 31 and 33 degrees on its owner's, it is handed the terrain
/// on every frame. Read straight down the centre line, it was handed the level
/// floor on every frame standing and on half of them walking, its trunk
/// turning 29 to 32 degrees in the frame it turned side on.
#[test]
fn a_horse_across_a_steep_hill_or_on_a_crest_stays_on_the_terrain() {
    let family = Family::Horse;
    let drop = drop_of(&beast_record(family));
    let side_on = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    for under in [Under::Local, Under::Peer] {
        for hz in [60.0f32, 144.0] {
            for degrees in [31.0f32, 35.0] {
                let grade = degrees.to_radians().tan();
                let ground = move |_: f32, z: f32| -grade * z;
                let place = Transform::from_xyz(0.0, drop + 0.05, 0.0).with_rotation(side_on);
                let log = stood(&ground, under, family, place, hz);
                let unpatched = log.iter().filter(|frame| !frame.patched).count();
                assert_eq!(
                    unpatched,
                    0,
                    "side on to {degrees} deg, {under:?} at {hz} Hz: handed the level floor on \
                     {unpatched} of {} frames",
                    log.len()
                );
            }
            for degrees in [25.0f32, 30.0] {
                let grade = degrees.to_radians().tan();
                let ground = move |_: f32, z: f32| 2.0 - grade * z.abs();
                let place = Transform::from_xyz(0.0, 2.0 + drop + 0.05, 0.3);
                let log = stood(&ground, under, family, place, hz);
                let unpatched = log.iter().filter(|frame| !frame.patched).count();
                assert_eq!(
                    unpatched,
                    0,
                    "on a {degrees} deg crest, {under:?} at {hz} Hz: handed the level floor on \
                     {unpatched} of {} frames",
                    log.len()
                );
            }
        }
    }
    for degrees in [31.0f32, 33.0] {
        let grade = degrees.to_radians().tan();
        let ground = move |_: f32, z: f32| -grade * z;
        let place = Transform::from_xyz(0.0, drop + 0.05, 0.0);
        for hz in [60.0f32, 144.0] {
            let log = walked(&ground, &[], family, place, KeyCode::KeyA, (2.0, 4.0), hz);
            let unpatched = log.iter().filter(|frame| !frame.patched).count();
            assert_eq!(
                unpatched,
                0,
                "walking across {degrees} deg at {hz} Hz: handed the level floor on {unpatched} \
                 of {} frames",
                log.len()
            );
        }
    }
}

/// A `family` beast on its owner's client walking 3 s at `hz` sideways off
/// the side of the pad of [`pad_on_a_hill`] - at `degrees` - onto the hill
/// below it: every frame of the walk.
fn stepped_off_the_side(family: Family, degrees: f32, hz: f32) -> Vec<Frame> {
    let grade = degrees.to_radians().tan();
    let ground = move |_: f32, z: f32| -grade * z;
    let pad = block(
        Vec3::new(-3.0, -6.0 * grade - 1.0, 0.0),
        Vec3::new(3.0, 0.0, 6.0),
    );
    let capsule = capsule_of(family);
    // Facing +X, its length along the pad's side edge's normal, a hand in
    // from the edge and far enough down the pad that its uphill side clears
    // the hill above it.
    let (x, z) = (
        3.0 - capsule.to - capsule.radius - 0.1,
        capsule.radius + 0.1,
    );
    let place = Transform::from_xyz(x, drop_of(&beast_record(family)) + 0.02, z)
        .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2));
    walked(
        &ground,
        &[pad],
        family,
        place,
        KeyCode::KeyD,
        (3.0, 3.0),
        hz,
    )
}

/// **A beast stepping off a pad onto the hill beside it eases onto the hill**
/// (#1835 end review, finding 2). A dog and a horse on their owner's client
/// walk sideways off the side of a level pad cut into a 20- and a 25-degree
/// hill, onto the hill a step below, at 60 and 144 frames a second. It comes
/// onto the terrain once, the pad's ground easing out as the hill's eases in
/// (#1834), and from a little before its share starts to rise to a
/// little after it is whole, no frame turns the trunk more than the ease does
/// at most - 1.5 times a frame's share of the agreed fifth of a second, of the
/// whole turn the hill asks of it, 29 to 37 degrees of roll side on to it -
/// or than the walk does on any other frame, and half a degree. Taken at once,
/// the trunk turned 11 to 18 degrees in the frame the patch came on, against
/// 1.3 to 7.3 on every other; eased, 4.4 at most at 60 Hz and 1.9 at 144, the
/// ease's own.
#[test]
fn a_beast_stepping_off_a_pad_onto_a_hill_eases_onto_it() {
    for degrees in [20.0f32, 25.0] {
        for family in [Family::Dog, Family::Horse] {
            for hz in [60.0f32, 144.0] {
                let log = stepped_off_the_side(family, degrees, hz);
                let toggles = log
                    .windows(2)
                    .filter(|pair| pair[0].on_terrain != pair[1].on_terrain)
                    .count();
                let came = log
                    .windows(2)
                    .position(|pair| !pair[0].on_terrain && pair[1].on_terrain)
                    .map_or(log.len(), |i| i + 1);
                let margin = (0.08 * hz) as usize;
                let window = came.saturating_sub(margin)..came + ((0.2 + 0.08) * hz) as usize;
                let skip = (0.6 * hz) as usize;
                let on = worst_turn(&log, window.clone());
                let off = worst_turn(&log, (skip..log.len()).filter(|i| !window.contains(i)));
                // The whole turn the hill asks of the trunk, across the ease.
                let (before, after) = (log[window.start], log[window.end.min(log.len() - 1)]);
                let whole = (after.pitch - before.pitch)
                    .abs()
                    .max((after.roll - before.roll).abs());
                // The agreed ease, not the constant: a shorter one fails this.
                let ease = 1.5 / hz / 0.2 * whole;
                println!(
                    "{family:?} off the side of the {degrees} deg pad at {hz} Hz: {toggles} \
                     toggles, patched at {came}, the trunk turning {on:.2} deg by it of {whole:.1} \
                     in all, {ease:.2} for the ease, and {off:.2} on any other frame"
                );
                assert!(
                    toggles == 1,
                    "{family:?} off the {degrees} deg pad at {hz} Hz: it came onto the terrain \
                     or left it {toggles} times"
                );
                assert!(
                    on <= off.max(ease) + 0.5,
                    "{family:?} off the {degrees} deg pad at {hz} Hz: the trunk turned {on:.2} \
                     deg in a frame coming onto the hill, {whole:.1} in all, against {ease:.2} \
                     for the ease and {off:.2} on any other frame"
                );
            }
        }
    }
}

/// **A step back onto the terrain in an ease out moves nothing** (#1835 end
/// review, finding 3). A dog on its owner's client stands on 20 degrees at 60
/// and 144 frames a second while the terrain is marked outgoing, so its patch
/// is held and eases out; halfway through the ease the mark is lifted for one
/// frame, and the contacts say the terrain for it. The share moves no more in
/// any frame than the ease does, and the trunk turns no more on and after
/// that frame than the ease itself turns it. Set straight back to the whole
/// terrain, the trunk turned the whole of its pitch in a frame, and again as
/// the hold that followed ran out.
#[test]
fn a_step_back_onto_the_terrain_in_an_ease_moves_nothing() {
    let ground = hill();
    let record = beast_record(Family::Dog);
    let place = Transform::from_xyz(0.0, ground(0.0, 0.0) + drop_of(&record) + 0.02, 0.0);
    for hz in [60.0f32, 144.0] {
        let (mut app, chassis, root) =
            standing_still(&ground, &[], Under::Local, Family::Dog, place, hz);
        let terrain = app
            .world_mut()
            .query_filtered::<Entity, With<crate::terrain::TerrainMesh>>()
            .single(app.world())
            .expect("one terrain");
        let blip = ((0.15 + 0.1) * hz) as usize;
        let mut shares = Vec::new();
        let log = frames_of(
            &mut app,
            chassis,
            root,
            Under::Local,
            (0.8 * hz) as usize,
            |app, i| {
                if i > 0 {
                    shares.push(held(app, root).0);
                }
                if i == blip {
                    app.world_mut()
                        .entity_mut(terrain)
                        .remove::<crate::terrain::OutgoingTerrain>();
                } else {
                    app.world_mut()
                        .entity_mut(terrain)
                        .insert(crate::terrain::OutgoingTerrain);
                }
            },
        );
        let step = 1.0 / hz / super::super::underfoot::BLEND_SECS;
        let jump = shares
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0f32, f32::max);
        let (on, eased) = (worst_turn(&log, blip..log.len()), worst_turn(&log, 0..blip));
        println!(
            "at {hz} Hz: the share moving {jump:.3} a frame at most against the ease's {step:.3}; \
             the trunk turning {on:.2} deg from the step on and {eased:.2} in the ease before it"
        );
        assert!(
            jump <= step + 1e-4,
            "at {hz} Hz: the share moved {jump:.3} in a frame, the ease {step:.3}"
        );
        assert!(
            on <= eased + 0.1,
            "at {hz} Hz: the trunk turned {on:.2} deg in a frame from the step back on, and \
             {eased:.2} in the ease before it"
        );
    }
}

/// **A hitch at a gallop is not a body set down elsewhere** (#1835 end
/// review, finding 4). A dog peer played out galloping at 12 m/s along a
/// 20-degree hill at 60 frames a second has a frame of a quarter of a second,
/// 3 m of travel and past the 2 m a set-down is told by, on which the terrain
/// under it is marked outgoing, so the cast says it rests elsewhere: it is
/// held on the terrain through it, as any lost step is. Told a set-down by
/// the distance alone, it forgot the hold and stood on the level floor for
/// the frame, its trunk snapping level and back.
#[test]
fn a_hitch_at_a_gallop_is_not_a_set_down() {
    let ground = hill();
    let record = beast_record(Family::Dog);
    // Along the hill, level: across the way it rises.
    let along = Vec3::new(0.33f32.cos(), 0.0, -0.33f32.sin());
    let facing = Quat::from_rotation_y((-along.x).atan2(-along.z));
    let place = Transform::from_xyz(-20.0 * along.x, 0.0, -20.0 * along.z);
    let place = place
        .with_translation(
            place
                .translation
                .with_y(ground(place.translation.x, place.translation.z))
                + Vec3::Y * (drop_of(&record) + 0.02),
        )
        .with_rotation(facing);
    let hz = 60.0f32;
    let (mut app, chassis, root) =
        standing_still(&ground, &[], Under::Peer, Family::Dog, place, hz);
    let terrain = app
        .world_mut()
        .query_filtered::<Entity, With<crate::terrain::TerrainMesh>>()
        .single(app.world())
        .expect("one terrain");
    let start = *app.world().get::<Transform>(chassis).expect("a chassis");
    let (speed, mut travelled) = (12.0f32, 0.0f32);
    for frame in 0..90usize {
        let hitch = frame == 60;
        let delta = if hitch { 0.25 } else { 1.0 / hz };
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            delta,
        )));
        travelled += speed * delta;
        app.world_mut()
            .get_mut::<Transform>(chassis)
            .expect("a chassis")
            .translation = start.translation + along * travelled;
        if hitch {
            app.world_mut()
                .entity_mut(terrain)
                .insert(crate::terrain::OutgoingTerrain);
        } else {
            app.world_mut()
                .entity_mut(terrain)
                .remove::<crate::terrain::OutgoingTerrain>();
        }
        app.update();
        if frame >= 30 {
            assert_eq!(
                held(&app, root),
                (1.0, true),
                "frame {frame}{}: not held on the terrain",
                if hitch { ", the hitch" } else { "" }
            );
        }
    }
}

/// **A measurement (#1835): a beast's fall onto a hill, frame by frame, as the
/// engine drives it.** A horse on either client at 60 Hz is lifted a metre in
/// a frame and let fall onto 20 degrees, as
/// [`a_beast_falling_onto_a_hill_is_stood_on_it_when_it_lands`] does: printed
/// for each frame, what the engine is driving, its leap's clock, whether the
/// body is handed a patch, how far it is over its rest, the trunk's pitch, the
/// deepest sole under the root, and each leg's stretch - its sole's distance
/// from its own hip or shoulder over its own reach.
#[test]
#[ignore = "a measurement, printed: run by hand"]
fn probe_a_beast_falling_onto_a_hill() {
    let ground = hill();
    for family in [Family::Horse, Family::Dog] {
        let record = beast_record(family);
        let place = Transform::from_xyz(0.0, ground(0.0, 0.0) + drop_of(&record) + 0.02, 0.0);
        for under in [Under::Peer, Under::Local] {
            let hz = 60.0f32;
            let (mut app, chassis, root) = standing_still(&ground, &[], under, family, place, hz);
            let rig = rig_of(&app, root);
            let rest = *app.world().get::<Transform>(chassis).expect("a chassis");
            let (mut height, mut speed) = (1.0f32, 0.0f32);
            println!("{family:?} {under:?}:");
            for frame in 0..40usize {
                match under {
                    Under::Local if frame == 0 => {
                        let lifted = rest.translation + Vec3::Y;
                        let world = app.world_mut();
                        world.get_mut::<Transform>(chassis).expect("c").translation = lifted;
                        world
                            .get_mut::<avian3d::prelude::Position>(chassis)
                            .expect("p")
                            .0 = lifted;
                        world
                            .get_mut::<avian3d::prelude::LinearVelocity>(chassis)
                            .expect("v")
                            .0 = Vec3::ZERO;
                    }
                    Under::Local => {}
                    Under::Peer => {
                        if frame > 0 {
                            speed += 9.81 / hz;
                            height = (height - speed / hz).max(0.0);
                        }
                        app.world_mut()
                            .get_mut::<Transform>(chassis)
                            .expect("c")
                            .translation = rest.translation + Vec3::Y * height;
                    }
                }
                app.update();
                let at = drawn(&app, root);
                let stretch: Vec<String> = rig
                    .ground_contacts()
                    .into_iter()
                    .filter_map(|limb| {
                        let [top, ..] = rig.limb_chain(limb)?;
                        let sole = rig
                            .extremity_joints(limb)
                            .into_iter()
                            .map(|joint| at[joint])
                            .min_by(|a, b| a.y.total_cmp(&b.y))?;
                        Some(format!(
                            "{:.2}",
                            sole.distance(at[top]) / rig.limb_reach(limb)?
                        ))
                    })
                    .collect();
                let world = app.world();
                let driver = world.get::<AvatarDriver>(root).expect("a driver");
                let drive = world.get::<Drive>(root).expect("a drive");
                let (pitch, _, _) = trunk(&app, root);
                println!(
                    "  {frame}: {:?} air {:?} patched {} vy {:+.2} over {:.3} pitch {:.1} deepest \
                     {:.3} stretch {}",
                    driver.source(),
                    driver
                        .airborne()
                        .map(|air| (air.elapsed, air.falling, air.landed)),
                    drive.ground.is_some(),
                    drive.velocity.y,
                    world.get::<Transform>(chassis).expect("c").translation.y - rest.translation.y,
                    pitch,
                    deepest_sole(&app, root),
                    stretch.join(" ")
                );
            }
        }
    }
}
