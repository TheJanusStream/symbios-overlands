//! `--driver FILE` (#1546): a car driven through a `--world` on the game's
//! own physics, and filmed as it goes.
//!
//! The ground is the world `--world` compiles - its terrain on the
//! heightfield collider and every solid placement on its static collider -
//! and the car is the one the game puts under a player wearing the file's
//! avatar: the local player's chassis with its preset's physics
//! ([`crate::player::spawn_headless_chassis`]) and the body's visuals,
//! pushed by the game's own fixed-step car systems
//! ([`crate::player::register_headless_car`]) on avian, at the game's 64
//! steps a second. The tool only does the driver's part: it holds the keys
//! `--drive-keys` names, segment by segment, in the `agent drive` form
//! (`W@4 W+D@0.6 none@2`).
//!
//! The keys change on the physics' own steps, counted in whole steps of the
//! fixed clock from the clip's first frame less `--drive-lead`, and the
//! chassis is eased between steps as the game eases a player's
//! (`TransformInterpolation`), which is what keeps a slowed shot smooth.
//! What the car does with them is the game's down to its one frame-rate
//! dependence: the car systems read the pose the last frame left
//! (`GlobalTransform`), not the physics' own, as they do in the game - so a
//! run filmed at another `--fps` or `--time-scale` differs by a few
//! hundredths of a second of airtime and a fraction of a degree, as the game
//! does between displays. `--drive-log` writes the run, step by step.
//!
//! The game's layers over a moving car come too: the body's lean and idle
//! shiver (the car's gait, seeded by the world's DID as the game seeds its
//! owner's own car) and the contact effects the game raises for its local
//! player ([`crate::interaction::plugin::register_headless_contacts`]) - a
//! splash in water; no ground dust, which the game's classifier gives only a
//! body whose underside touches the ground, and a car's box rides on its
//! springs (#1549).

use std::io::Write;

use bevy::prelude::*;
use serde_json::Value;

use crate::pds::avatar::AvatarBody;
use crate::pds::{AvatarRecord, LocomotionConfig, RoomRecord};
use crate::player::CarContact;
use crate::player::visuals::{AvatarSpawnDeps, spawn_avatar_visuals};
use crate::state::{CurrentRoomDid, LiveRoomRecord};
use crate::terrain::FinishedHeightMap;

use super::headless::{ClipTiming, Clock};
use super::rigged::{kind_of, place, read_part, unwrap_answer};

/// Seconds the car stands on its springs after it is set down, before the
/// drive's lead-in may start: dropped from a metre up, it has settled by
/// then.
pub(super) const SETTLE_S: f32 = 2.5;

/// The keys a drive may name - the car's own, by the names `agent drive`
/// gives them.
const KEY_NAMES: [(&str, KeyCode); 7] = [
    ("W", KeyCode::KeyW),
    ("A", KeyCode::KeyA),
    ("S", KeyCode::KeyS),
    ("D", KeyCode::KeyD),
    ("Q", KeyCode::KeyQ),
    ("E", KeyCode::KeyE),
    ("SPACE", KeyCode::Space),
];

/// One stretch of a drive: these keys, held for `secs` seconds.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct KeySegment {
    pub(super) keys: Vec<KeyCode>,
    pub(super) secs: f32,
}

/// Parse `--drive-keys`: `KEYS@SECONDS` segments, separated by spaces or
/// given one per flag - the keys joined by `+`, `none` for none, as
/// `agent drive` takes them (`W@4 W+D@0.6 none@2`).
pub(super) fn parse_drive_keys(raw: &[String]) -> Result<Vec<KeySegment>, String> {
    let mut segments = Vec::new();
    for word in raw.iter().flat_map(|r| r.split_whitespace()) {
        let (keys, secs) = word
            .rsplit_once('@')
            .ok_or_else(|| format!("{word:?} is not KEYS@SECONDS (W@2, W+D@0.5, none@1)"))?;
        let secs = secs
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|s| s.is_finite() && *s > 0.0)
            .ok_or_else(|| format!("{word:?}: {secs:?} is not a positive number of seconds"))?;
        let keys = keys.trim();
        let keys = if keys.is_empty() || keys.eq_ignore_ascii_case("none") {
            Vec::new()
        } else {
            let mut named = Vec::new();
            for name in keys.split('+') {
                let upper = name.trim().to_ascii_uppercase();
                let key = KEY_NAMES
                    .iter()
                    .find(|(n, _)| *n == upper)
                    .map(|(_, key)| *key)
                    .ok_or_else(|| {
                        format!(
                            "{word:?}: {name:?} is not one of the car's keys, W, A, S, D, Q, E, SPACE"
                        )
                    })?;
                if !named.contains(&key) {
                    named.push(key);
                }
            }
            named
        };
        segments.push(KeySegment { keys, secs });
    }
    if segments.is_empty() {
        return Err(
            "--drive-keys names no segment: a drive is KEYS@SECONDS, W@4 none@2".to_owned(),
        );
    }
    Ok(segments)
}

/// `--driver`: the car, where it starts and what its driver presses.
#[derive(Resource, Clone, Debug)]
pub(super) struct DriverSpec {
    /// The avatar the car is - a generator body on car locomotion -
    /// sanitised as the game sanitises an avatar it fetches.
    pub(super) record: AvatarRecord,
    /// Where the car is set down, `x,z`; the record's landing when absent.
    pub(super) from: Option<[f32; 2]>,
    /// The compass bearing it faces there (north, `-Z`, is 0; east 90); the
    /// landing's own facing when absent.
    pub(super) bearing: Option<f32>,
    /// What the driver presses, in order.
    pub(super) keys: Vec<KeySegment>,
    /// Seconds of driving before the first captured frame.
    pub(super) lead: f32,
    /// Where `--drive-log` writes the run, one JSON line a physics step.
    pub(super) log: Option<String>,
}

impl DriverSpec {
    /// The keys held on physics step `step` of the drive, steps of `hz` a
    /// second counted from its first: each segment's own for its seconds
    /// rounded to whole steps, end to end, and none before the first or after
    /// the last.
    pub(super) fn keys_at_step(&self, step: i64, hz: f64) -> &[KeyCode] {
        if step < 0 {
            return &[];
        }
        let mut end = 0.0;
        for segment in &self.keys {
            end += f64::from(segment.secs);
            if step < (end * hz).round() as i64 {
                return &segment.keys;
            }
        }
        &[]
    }

    /// Seconds of warm-up the car needs before the first captured frame:
    /// it settles, then drives its lead-in.
    pub(super) fn warmup_secs(&self) -> f32 {
        SETTLE_S + self.lead.max(0.0)
    }
}

/// The avatar in the file at `path`, which must be a car: `{record, ...}`
/// as `rec.py pull avatar` writes it, or an `agent avatar get ""` answer.
pub(super) fn read_driver(path: &str) -> Result<AvatarRecord, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read it: {e}"))?;
    parse_driver(&text)
}

/// The car avatar in `text` (see [`read_driver`]), sanitised as the game
/// sanitises a fetched one. A refusal names the JSON pointer that is wrong.
pub(super) fn parse_driver(text: &str) -> Result<AvatarRecord, String> {
    let document: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
    let (avatar, at) = unwrap_answer(document)?;
    let Value::Object(mut parts) = avatar else {
        return Err(format!(
            "{}: {}, not an object - an avatar is {{record, body, worn}}",
            place(at),
            kind_of(&avatar)
        ));
    };
    let record_json = parts
        .remove("record")
        .ok_or_else(|| format!("{at}/record: missing - the avatar record"))?;
    let mut record: AvatarRecord =
        read_part(&record_json, &format!("{at}/record"), "an avatar record")?;
    if !matches!(record.body, AvatarBody::Generator(_)) {
        return Err(format!(
            "{at}/record/body: not a generator body - a car's body is a tree of parts; a \
             rigged person walks (`--walker-avatar`)"
        ));
    }
    if !matches!(record.locomotion, LocomotionConfig::Car(_)) {
        return Err(format!(
            "{at}/record/locomotion: not a car - `--driver` drives a body on wheels, with \
             the car's systems"
        ));
    }
    record.sanitize();
    Ok(record)
}

/// Register the driven car on a `--world` app: avian's physics over the
/// compiled world's colliders, the game's car systems with the avatar as the
/// local player's record they read, the keys pressed before them on every
/// physics step, the log after it, and the car set down once the world has
/// settled.
pub(super) fn register(app: &mut App, spec: DriverSpec) {
    app.add_plugins(avian3d::prelude::PhysicsPlugins::default())
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(crate::state::LiveAvatarRecord(spec.record.clone()))
        .insert_resource(spec);
    crate::player::register_headless_car(app);
    crate::interaction::plugin::register_headless_contacts(app);
    app.add_systems(
        FixedUpdate,
        press_driver_keys.before(crate::player::HeadlessCarSystems),
    )
    .add_systems(FixedLast, log_driver)
    .add_systems(Update, spawn_driver.run_if(resource_exists::<ClipTiming>));
}

/// The driven car's chassis.
#[derive(Component)]
pub(super) struct Driver {
    /// The fixed clock's step the drive's first segment starts on.
    start_step: i64,
    /// The keys pressed now.
    held: Vec<KeyCode>,
}

/// The fixed clock's step `time` is on, and its rate: whole steps of its
/// timestep since it started, counted exactly rather than by dividing floats.
fn fixed_step(time: &Time) -> (i64, f64) {
    let step = time.delta().as_nanos().max(1);
    ((time.elapsed().as_nanos() / step) as i64, 1e9 / step as f64)
}

/// Where the car is set down: on the ground at `from` (or the landing),
/// tilted to the slope there and turned to its bearing, a metre up - the
/// local player's spawn (`player::spawn::spawn_local_player`).
fn start_pose(spec: &DriverSpec, room: &RoomRecord, heightmap: &FinishedHeightMap) -> Transform {
    let landing = room.default_landing.as_ref();
    let hm = &heightmap.0;
    let extent = (hm.width() - 1) as f32 * hm.scale();
    let half = extent * 0.5;
    let [x, z] = spec
        .from
        .or_else(|| landing.map(|l| l.pos.0))
        .unwrap_or([0.0, 0.0])
        .map(|v| v.clamp(-half, half));
    // The landing's own height, a deck or a roof, when it is the landing the
    // car is set down on - the game's drop-pin form (`pos=x,y,z`).
    let pinned = landing
        .filter(|_| spec.from.is_none())
        .and_then(|l| l.y.map(|y| y.0));
    // A bearing turns the chassis the other way round from a yaw: the game
    // faces a landing of `yaw_deg` along `rotation_y(yaw_deg)`, and north
    // (-Z) is bearing 0 with east (+X) at 90.
    let yaw_deg = spec
        .bearing
        .map(|b| -b)
        .or_else(|| landing.map(|l| l.yaw_deg.0))
        .unwrap_or(0.0);
    let normal = hm.get_normal_at((x + half).clamp(0.0, extent), (z + half).clamp(0.0, extent));
    let tilt = Quat::from_rotation_arc(Vec3::Y, Vec3::from_array(normal));
    let y = pinned.unwrap_or_else(|| {
        heightmap.world_height_at(x, z) + crate::config::rover::SPAWN_HEIGHT_OFFSET
    });
    Transform::from_xyz(x, y, z).with_rotation(tilt * Quat::from_rotation_y(yaw_deg.to_radians()))
}

/// Set the car down once the world has settled (the clip timing is the
/// signal - the drive loop inserts it on the way into warm-up), so it lands
/// on finished ground and its lead-in ends exactly at the first frame.
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_driver(
    mut commands: Commands,
    spec: Res<DriverSpec>,
    timing: Res<ClipTiming>,
    clocks: (Res<Clock>, Res<Time<Virtual>>, Res<Time<Fixed>>),
    room: (Res<LiveRoomRecord>, Res<CurrentRoomDid>),
    heightmap: Res<FinishedHeightMap>,
    existing: Query<(), With<Driver>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut deps: AvatarSpawnDeps,
) {
    if !existing.is_empty() {
        return;
    }
    let (room, did) = room;
    let at = start_pose(&spec, &room.0, &heightmap);
    let chassis =
        crate::player::spawn_headless_chassis(&mut commands, at, &spec.record.locomotion, &did.0);
    spawn_avatar_visuals(
        &mut commands,
        chassis,
        &spec.record.body,
        None,
        &mut meshes,
        &mut materials,
        &mut images,
        &mut deps,
        false,
    );
    // The first segment's step, on the exact virtual clock rather than the
    // tool's own float one (which drifts from it by milliseconds over a long
    // build): the seconds from now to the clip's first frame, less the lead,
    // added to where the virtual clock stands, rounded to a fixed step.
    let (clock, virt, fixed) = clocks;
    let hz = 1.0 / fixed.timestep().as_secs_f64();
    let start = virt.elapsed_secs_f64() + f64::from(timing.capture_start - clock.elapsed)
        - f64::from(spec.lead);
    let start_step = (start * hz).round() as i64;
    commands.entity(chassis).insert(Driver {
        start_step,
        held: Vec::new(),
    });
    if let Some(path) = &spec.log {
        // A fresh log a run: every line after this one is this drive's.
        if let Err(e) = std::fs::write(path, "") {
            error!("--drive-log {path:?}: {e}");
        }
    }
    info!(
        "driver: set down at ({:.1}, {:.1}, {:.1}), its keys from step {start_step} \
         (t={start:.3}s), the first frame at t={:.2}s",
        at.translation.x, at.translation.y, at.translation.z, timing.capture_start
    );
}

/// Hold the keys the drive holds now - on the physics' own step, before the
/// car's systems read them, so a key goes down on the step its segment
/// starts whatever the frame rate.
pub(super) fn press_driver_keys(
    spec: Res<DriverSpec>,
    time: Res<Time>,
    mut drivers: Query<&mut Driver>,
    mut keyboard: ResMut<ButtonInput<KeyCode>>,
) {
    let Ok(mut driver) = drivers.single_mut() else {
        return;
    };
    let (step, hz) = fixed_step(&time);
    let want = spec.keys_at_step(step - driver.start_step, hz);
    if want == driver.held.as_slice() {
        return;
    }
    for key in &driver.held {
        if !want.contains(key) {
            keyboard.release(*key);
        }
    }
    for key in want {
        keyboard.press(*key);
    }
    driver.held = want.to_vec();
}

/// One line of `--drive-log` a physics step, after the step (`FixedLast`,
/// once avian has moved the car): when it is
/// (seconds into the drive), the keys, where the car is and how it moves,
/// its compass bearing, pitch and roll, and how it stands on the ground.
#[allow(clippy::type_complexity)]
pub(super) fn log_driver(
    spec: Res<DriverSpec>,
    time: Res<Time>,
    drivers: Query<(
        &Driver,
        &avian3d::prelude::Position,
        &avian3d::prelude::Rotation,
        &avian3d::prelude::LinearVelocity,
        &CarContact,
    )>,
) {
    let Some(path) = &spec.log else {
        return;
    };
    let Ok((driver, position, rotation, velocity, contact)) = drivers.single() else {
        return;
    };
    let (step, hz) = fixed_step(&time);
    let line = log_line(
        (step - driver.start_step) as f64 / hz,
        &driver.held,
        position.0,
        rotation.0,
        velocity.0,
        contact,
    );
    let written = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .and_then(|mut file| writeln!(file, "{line}"));
    if let Err(e) = written {
        error!("--drive-log {path:?}: {e}");
    }
}

/// A `--drive-log` line (see [`log_driver`]).
fn log_line(
    t: f64,
    keys: &[KeyCode],
    at: Vec3,
    rotation: Quat,
    velocity: Vec3,
    contact: &CarContact,
) -> Value {
    let forward = rotation * Vec3::NEG_Z;
    let up = rotation * Vec3::Y;
    let right = rotation * Vec3::X;
    let round = |v: f32, places: i32| {
        let k = 10f32.powi(places);
        (v * k).round() / k
    };
    let names: Vec<&str> = keys
        .iter()
        .filter_map(|key| KEY_NAMES.iter().find(|(_, k)| k == key).map(|(n, _)| *n))
        .collect();
    serde_json::json!({
        "t": (t * 1000.0).round() / 1000.0,
        "keys": if names.is_empty() { "none".to_owned() } else { names.join("+") },
        "pos": [round(at.x, 3), round(at.y, 3), round(at.z, 3)],
        "vel": [round(velocity.x, 3), round(velocity.y, 3), round(velocity.z, 3)],
        "speed": round(velocity.length(), 3),
        "bearing": round(forward.x.atan2(-forward.z).to_degrees().rem_euclid(360.0), 2),
        "pitch": round(forward.y.clamp(-1.0, 1.0).asin().to_degrees(), 2),
        "roll": round((-right.y).atan2(up.y).to_degrees(), 2),
        "wheels": contact.wheels,
        "near_ground": contact.near_ground,
        "lying": contact.lying,
    })
}

/// The car the rig follows: where its chassis is, and its heading on the
/// ground (its forward, flattened).
pub(super) fn driver_pose(transform: &Transform) -> (Vec3, Vec3) {
    let forward = transform.rotation * Vec3::NEG_Z;
    let flat = Vec3::new(forward.x, 0.0, forward.z).normalize_or(Vec3::NEG_Z);
    (transform.translation, flat)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first seeded avatar that is a car.
    fn a_car() -> AvatarRecord {
        (0..512)
            .map(AvatarRecord::default_for_seed)
            .find(|r| matches!(r.locomotion, LocomotionConfig::Car(_)))
            .expect("the seeded fleet has a car in its first 512 seeds")
    }

    fn spec(keys: &str) -> DriverSpec {
        DriverSpec {
            record: a_car(),
            from: None,
            bearing: None,
            keys: parse_drive_keys(&[keys.to_owned()]).expect("the keys parse"),
            lead: 1.0,
            log: None,
        }
    }

    /// The `agent drive` form: segments by spaces or by flag, keys by `+`,
    /// any case, `none` for none; a key the car does not read, a segment
    /// with no `@`, and a time that is not a positive number are refused by
    /// name.
    #[test]
    fn drive_keys_read_as_agent_drive_writes_them() {
        let segments =
            parse_drive_keys(&["W@4 w+d@0.6".to_owned(), "none@2".to_owned()]).expect("they parse");
        assert_eq!(
            segments,
            [
                KeySegment {
                    keys: vec![KeyCode::KeyW],
                    secs: 4.0
                },
                KeySegment {
                    keys: vec![KeyCode::KeyW, KeyCode::KeyD],
                    secs: 0.6
                },
                KeySegment {
                    keys: Vec::new(),
                    secs: 2.0
                },
            ]
        );
        for bad in ["W@0", "W@-1", "W", "SHIFT@1", "W@x", ""] {
            assert!(
                parse_drive_keys(&[bad.to_owned()]).is_err(),
                "{bad:?} should be refused"
            );
        }
    }

    /// Every name a drive may use is a key the car reads: the two tables
    /// cannot drift apart without this failing.
    #[test]
    fn every_drive_key_is_one_the_car_reads() {
        for (name, key) in KEY_NAMES {
            assert!(
                crate::player::sim::CAR_KEYS.iter().any(|k| k.key == key),
                "{name} ({key:?}) is not in the car's key table"
            );
        }
    }

    /// The keys held on a step are the segment's whose whole steps hold it:
    /// none before the drive starts or after it ends, a boundary belonging
    /// to the segment that starts there, and a segment's seconds rounded to
    /// whole steps of the clock (0.5 s at 64 a second is 32 steps).
    #[test]
    fn the_keys_held_follow_the_segments_end_to_end() {
        let s = spec("W@2 W+A@0.5 none@1");
        let hz = 64.0;
        assert_eq!(s.keys_at_step(-1, hz), &[] as &[KeyCode]);
        assert_eq!(s.keys_at_step(0, hz), &[KeyCode::KeyW]);
        assert_eq!(s.keys_at_step(127, hz), &[KeyCode::KeyW]);
        assert_eq!(s.keys_at_step(128, hz), &[KeyCode::KeyW, KeyCode::KeyA]);
        assert_eq!(s.keys_at_step(159, hz), &[KeyCode::KeyW, KeyCode::KeyA]);
        assert_eq!(s.keys_at_step(160, hz), &[] as &[KeyCode]);
        assert_eq!(s.keys_at_step(1000, hz), &[] as &[KeyCode]);
        assert_eq!(s.warmup_secs(), SETTLE_S + 1.0);
    }

    /// The step a fixed clock is on is counted in whole timesteps, exactly:
    /// 640 steps of 1/64 s is step 640 at 64 a second, not 639.999.
    #[test]
    fn the_fixed_step_is_counted_exactly() {
        let mut fixed = Time::<Fixed>::from_hz(64.0);
        let dt = fixed.timestep();
        for _ in 0..640 {
            fixed.advance_by(dt);
        }
        let (step, hz) = fixed_step(&fixed.as_generic());
        assert_eq!(step, 640);
        assert!((hz - 64.0).abs() < 1e-9, "{hz}");
    }

    /// A rigged person is not a car, nor is a car body on a hover-boat's
    /// locomotion; a car avatar in the `agent avatar get ""` form reads, and
    /// is sanitised on the way in.
    #[test]
    fn only_a_car_avatar_drives() {
        let car = a_car();
        let file = serde_json::json!({ "record": car, "body": null, "worn": [] });
        let read = parse_driver(&file.to_string()).expect("a car reads");
        assert!(matches!(read.locomotion, LocomotionConfig::Car(_)));
        let answer = serde_json::json!({
            "ok": true,
            "result": { "pointer": "", "value": file },
        });
        assert!(
            parse_driver(&answer.to_string()).is_ok(),
            "the answer form reads"
        );

        let mut boat = car.clone();
        boat.locomotion = LocomotionConfig::HoverBoat(Box::default());
        let file = serde_json::json!({ "record": boat, "body": null, "worn": [] });
        let refused = parse_driver(&file.to_string()).expect_err("a hover-boat is refused");
        assert!(refused.contains("/record/locomotion"), "{refused}");

        let person = AvatarRecord::wearing("self");
        let file = serde_json::json!({ "record": person, "body": null, "worn": [] });
        let refused = parse_driver(&file.to_string()).expect_err("a person is refused");
        assert!(refused.contains("/record/body"), "{refused}");
    }

    /// The log's angles: a car facing east and nose up 10 degrees reads
    /// bearing 90 and pitch 10, rolled right 5 degrees reads roll 5.
    #[test]
    fn the_log_reads_bearing_pitch_and_roll() {
        let facing_east = Quat::from_rotation_y(-90f32.to_radians());
        let nose_up = facing_east * Quat::from_rotation_x(10f32.to_radians());
        let line = log_line(
            1.0,
            &[KeyCode::KeyW],
            Vec3::ZERO,
            nose_up,
            Vec3::X,
            &CarContact::default(),
        );
        assert_eq!(line["bearing"], 90.0);
        assert_eq!(line["pitch"], 10.0);
        assert_eq!(line["keys"], "W");
        let rolled = facing_east * Quat::from_rotation_z(-5f32.to_radians());
        let line = log_line(
            0.0,
            &[],
            Vec3::ZERO,
            rolled,
            Vec3::ZERO,
            &CarContact::default(),
        );
        assert_eq!(line["roll"], 5.0);
        assert_eq!(line["keys"], "none");
    }
}
