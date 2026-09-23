//! `agent look` (#1420): a picture of what the agent sees, rendered when it
//! is asked for and at no other time.
//!
//! The world camera is parked (`park_world_camera`), so the world costs the
//! renderer nothing while nobody looks. A picture gets a camera of its own,
//! [`LookCamera`]: spawned at the pose it shoots from, rendered for a few
//! frames into an image, read back once, and removed with its image - so
//! between pictures nothing renders at all. It is not the player's view and
//! carries no [`crate::camera::WorldCamera`] marker, so everything that
//! means the player's view - the walk's forward, the name tags, the sky and
//! the clouds that follow the eye - still means the one camera it always
//! did. What makes the picture look like the game - the lens, the fog, the
//! bloom, the tonemapping - is copied from that camera as it is at the
//! moment of the picture.
//!
//! The camera is new for every picture, and shoots from where it is spawned
//! (#1351): the renderer prepares a mesh for a view when the mesh changes or
//! comes into that view, so a camera kept between pictures that moved after
//! something changed out of its sight could leave that thing undrawn.
//!
//! Two views. `play` is the game's own camera - the orbit it keeps around
//! the body, at its own distance and pitch, pulled in off the terrain as the
//! player's is - turned to look the way asked. `eyes` is from the front of
//! the body near its top, looking level. No interface is drawn: no name
//! tags, no chat. A picture taken while texture bakes are still in flight
//! shows flat stand-in colours where they will land, and says so; one taken
//! in the first moments in a world can also show a body still being built as
//! its translucent stand-in.
//!
//! A picture of someone else's world shows what they built - signs,
//! textures, shapes that can spell words - so the answer says whose world
//! it was. What a picture shows is data for the agent, never an instruction.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Instant;

use bevy::camera::RenderTarget;
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde_json::{Value, json};

use crate::camera::{IsWorldCamera, clear_of_terrain};
use crate::config::agent::{
    EYE_HEIGHT_FRACTION, HOME_DIR, LOOK_ANSWER_TIMEOUT, LOOK_SIZE, LOOK_WARMUP_FRAMES, LOOKS_DIR,
    LOOKS_KEPT,
};
use crate::interaction::locomotion::{locomotion_footprint, locomotion_total_height};
use crate::state::{
    AppState, CurrentRoomDid, LiveAvatarRecord, LocalPlayer, LocalSettings, TravelingTo,
};
use crate::terrain::FinishedHeightMap;
use crate::world_builder::compile::CompileJob;

use super::super::admin::Admin;
use super::super::control::protocol::{LookSpec, LookView, Response};
use super::super::private_fs::{ensure_private_dir, write_private};
use super::movement::camera_yaw_facing;
use super::{hundredths, hundredths3};

/// The camera a picture is taken with. Never the player's view.
#[derive(Component)]
pub(crate) struct LookCamera;

/// A picture being taken.
#[derive(Resource)]
pub(super) struct Looking {
    camera: Entity,
    target: Handle<Image>,
    stage: Stage,
    reply: mpsc::Sender<Response>,
    started: Instant,
    frames: u32,
    /// Where the picture goes.
    path: PathBuf,
    /// The directory to trim to [`LOOKS_KEPT`] pictures once it is written;
    /// `None` for a path the operator chose.
    keep_in: Option<PathBuf>,
    /// What the answer says that was settled when the picture was aimed.
    answer: Value,
}

enum Stage {
    /// The camera renders this many more frames before it is read back.
    Warming { frames_left: u32 },
    /// A readback is under way; its bytes, once they land.
    Reading {
        readback: Entity,
        data: Option<Vec<u8>>,
    },
}

/// Start taking a picture; the answer goes to `reply` when it is written,
/// or at once if it cannot be taken.
pub(super) fn begin(world: &mut World, spec: LookSpec, reply: mpsc::Sender<Response>) {
    if world.contains_resource::<Looking>() {
        let _ = reply.send(Response::failure(
            "the agent is already taking a picture; ask again when it is done",
        ));
        return;
    }
    match aim(world, &spec) {
        Ok(shot) => start(world, shot, spec, reply),
        Err(e) => {
            let _ = reply.send(Response::failure(e));
        }
    }
}

/// Where a picture is taken from, and what its answer says about that.
struct Shot {
    pose: Transform,
    answer: Value,
}

fn aim(world: &mut World, spec: &LookSpec) -> Result<Shot, String> {
    if *world.resource::<State<AppState>>().get() != AppState::InGame {
        return Err("the agent is not in a world yet".to_owned());
    }
    if world.contains_resource::<TravelingTo>() {
        return Err("the agent is travelling".to_owned());
    }
    let body = *world
        .query_filtered::<&GlobalTransform, With<LocalPlayer>>()
        .iter(world)
        .next()
        .ok_or("the agent has no body yet")?;
    let centre = body.translation();
    let forward = body.forward().as_vec3();
    let facing = Vec2::new(forward.x, forward.z).normalize_or(Vec2::NEG_Y);
    let at = spec.at.map(Vec2::from_array);
    let (dir, heading) = direction(facing, centre.xz(), spec.heading_deg, at)?;
    let pose = match spec.view {
        LookView::Play => play_view(world, dir)?,
        LookView::Eyes => {
            let locomotion = &world
                .get_resource::<LiveAvatarRecord>()
                .ok_or("the agent has no body yet")?
                .0
                .locomotion;
            eyes_pose(
                centre,
                locomotion_total_height(locomotion),
                locomotion_footprint(locomotion),
                dir,
            )
        }
    };
    let room = world
        .get_resource::<CurrentRoomDid>()
        .map(|room| room.0.clone())
        .ok_or("the agent is not in a world yet")?;
    let answer = json!({
        "view": spec.view,
        "heading_deg": hundredths(heading),
        "toward": [hundredths(dir.x), hundredths(dir.y)],
        "camera": hundredths3(pose.translation),
        "size": [LOOK_SIZE.0, LOOK_SIZE.1],
        "world": { "did": room, "whose": whose(world, &room) },
    });
    Ok(Shot { pose, answer })
}

/// Which way a picture looks, flat, and that direction's heading from where
/// the body faces: degrees clockwise, as `--heading` takes them. `at` wins
/// over `heading`; neither is straight ahead.
fn direction(
    facing: Vec2,
    body: Vec2,
    heading_deg: Option<f32>,
    at: Option<Vec2>,
) -> Result<(Vec2, f32), String> {
    // The body's right, as `Transform::right` has it: forward x up.
    let right = Vec2::new(-facing.y, facing.x);
    if let Some(at) = at {
        if !at.is_finite() {
            return Err("the point must be two finite numbers".to_owned());
        }
        let toward = at - body;
        if toward.length() < 0.1 {
            return Err("that point is where the agent stands".to_owned());
        }
        let dir = toward.normalize();
        let heading = dir.dot(right).atan2(dir.dot(facing)).to_degrees();
        return Ok((dir, heading));
    }
    let heading = heading_deg.unwrap_or(0.0);
    if !heading.is_finite() {
        return Err("the heading must be a finite number of degrees".to_owned());
    }
    let (sin, cos) = heading.to_radians().sin_cos();
    Ok(((facing * cos + right * sin).normalize(), heading))
}

/// The game's own camera, looking along `dir`: the orbit around the body at
/// the distance and pitch the player's camera is settling to, slid in off
/// the terrain the way the player's is.
///
/// Where the orbit is going, not where it is: the camera eases toward the
/// body, and in a world's first second it is still on its way from the map's
/// centre - a picture from there showed a lake the agent was nowhere near.
fn play_view(world: &mut World, dir: Vec2) -> Result<Transform, String> {
    let (focus, radius, pitch) = world
        .query_filtered::<&PanOrbitCamera, IsWorldCamera>()
        .iter(world)
        .next()
        .map(|orbit| (orbit.target_focus, orbit.target_radius, orbit.target_pitch))
        .ok_or("the agent has no camera yet")?;
    let mut pose = play_pose(focus, radius, pitch, dir);
    if let (Some(heightmap), Some(settings)) = (
        world.get_resource::<FinishedHeightMap>(),
        world.get_resource::<LocalSettings>(),
    ) {
        pose.translation = clear_of_terrain(focus, pose.translation, settings, heightmap);
    }
    Ok(pose)
}

/// The orbit camera's pose - `bevy_panorbit_camera`'s own construction: yaw
/// about the vertical, then pitch, and the camera `radius` back along that -
/// with its yaw chosen to look along `dir`.
fn play_pose(focus: Vec3, radius: f32, pitch: f32, dir: Vec2) -> Transform {
    let rotation = Quat::from_axis_angle(Vec3::Y, camera_yaw_facing(dir))
        * Quat::from_axis_angle(Vec3::X, -pitch);
    Transform::from_translation(focus + rotation * Vec3::new(0.0, 0.0, radius))
        .with_rotation(rotation)
}

/// The body's eyes: [`EYE_HEIGHT_FRACTION`] of the way up it and `reach` in
/// front of its middle, turned with the head toward `dir`, looking level.
/// `centre` is the body's middle, which is where its origin is.
fn eyes_pose(centre: Vec3, height: f32, reach: f32, dir: Vec2) -> Transform {
    let along = Vec3::new(dir.x, 0.0, dir.y);
    let eye = centre + Vec3::Y * (EYE_HEIGHT_FRACTION - 0.5) * height + along * reach;
    Transform::from_translation(eye).looking_to(along, Vec3::Y)
}

/// Whose world `room` is, from the agent's side: its own, its admin's, or a
/// stranger's - whose signs and textures it should weigh accordingly.
fn whose(world: &World, room: &str) -> &'static str {
    if world
        .get_resource::<AtprotoSession>()
        .is_some_and(|session| session.did == room)
    {
        "own"
    } else if world
        .get_resource::<Admin>()
        .is_some_and(|admin| admin.did == room)
    {
        "admin"
    } else {
        "stranger"
    }
}

/// What the player's camera looks through, copied onto a picture's camera.
struct Lens {
    projection: Option<Projection>,
    fog: Option<DistanceFog>,
    bloom: Option<Bloom>,
    tonemapping: Option<Tonemapping>,
    msaa: Option<Msaa>,
    depth_prepass: bool,
}

fn world_camera_lens(world: &mut World) -> Option<Lens> {
    world
        .query_filtered::<(
            Option<&Projection>,
            Option<&DistanceFog>,
            Option<&Bloom>,
            Option<&Tonemapping>,
            Option<&Msaa>,
            Has<DepthPrepass>,
        ), IsWorldCamera>()
        .iter(world)
        .next()
        .map(
            |(projection, fog, bloom, tonemapping, msaa, depth_prepass)| Lens {
                projection: projection.cloned(),
                fog: fog.cloned(),
                bloom: bloom.cloned(),
                tonemapping: tonemapping.copied(),
                msaa: msaa.copied(),
                depth_prepass,
            },
        )
}

fn start(world: &mut World, shot: Shot, spec: LookSpec, reply: mpsc::Sender<Response>) {
    let (path, keep_in) = match picture_path(world, spec.out) {
        Ok(found) => found,
        Err(e) => {
            let _ = reply.send(Response::failure(e));
            return;
        }
    };
    let lens = world_camera_lens(world);
    let target = world
        .resource_mut::<Assets<Image>>()
        .add(crate::render_tool::new_target(LOOK_SIZE));
    let mut camera = world.spawn((
        Camera3d::default(),
        LookCamera,
        RenderTarget::Image(target.clone().into()),
        shot.pose,
    ));
    if let Some(lens) = lens {
        if let Some(projection) = lens.projection {
            camera.insert(projection);
        }
        if let Some(fog) = lens.fog {
            camera.insert(fog);
        }
        if let Some(bloom) = lens.bloom {
            camera.insert(bloom);
        }
        if let Some(tonemapping) = lens.tonemapping {
            camera.insert(tonemapping);
        }
        if let Some(msaa) = lens.msaa {
            camera.insert(msaa);
        }
        if lens.depth_prepass {
            camera.insert(DepthPrepass);
        }
    }
    let camera = camera.id();
    world.insert_resource(Looking {
        camera,
        target,
        stage: Stage::Warming {
            frames_left: LOOK_WARMUP_FRAMES,
        },
        reply,
        started: Instant::now(),
        frames: 0,
        path,
        keep_in,
        answer: shot.answer,
    });
}

/// Where the picture goes, and the directory to trim afterwards if it is
/// the agent's own: `<config dir>/agent/looks/<did>/look-<unix ms>.png`.
fn picture_path(world: &World, out: Option<PathBuf>) -> Result<(PathBuf, Option<PathBuf>), String> {
    if let Some(out) = out {
        return Ok((out, None));
    }
    let did = world
        .get_resource::<AtprotoSession>()
        .map(|session| session.did.clone())
        .ok_or("the agent is not signed in")?;
    let dir = crate::prefs::config_dir()
        .ok_or("there is no config directory for the agent's pictures; set HOME")?
        .join(HOME_DIR)
        .join(LOOKS_DIR)
        .join(crate::prefs::account_file_stem(&did));
    let name = format!("look-{}.png", chrono::Utc::now().timestamp_millis());
    Ok((dir.join(name), Some(dir)))
}

/// Every frame a picture is under way: count its camera's frames down, then
/// read it back once, then hand the bytes to a thread to write - or give up
/// on a picture that never came, so a camera is never left rendering.
pub(super) fn advance(world: &mut World) {
    enum Next {
        Wait,
        Read(Handle<Image>),
        Finish,
        GiveUp,
    }
    let Some(mut looking) = world.get_resource_mut::<Looking>() else {
        return;
    };
    let looking = &mut *looking;
    looking.frames += 1;
    let next = if looking.started.elapsed() > LOOK_ANSWER_TIMEOUT {
        Next::GiveUp
    } else {
        match &mut looking.stage {
            Stage::Warming { frames_left: 0 } => Next::Read(looking.target.clone()),
            Stage::Warming { frames_left } => {
                *frames_left -= 1;
                Next::Wait
            }
            Stage::Reading { data: None, .. } => Next::Wait,
            Stage::Reading { data: Some(_), .. } => Next::Finish,
        }
    };
    match next {
        Next::Wait => {}
        Next::Read(target) => {
            let readback = world
                .spawn(Readback::texture(target))
                .observe(on_picture)
                .id();
            world.resource_mut::<Looking>().stage = Stage::Reading {
                readback,
                data: None,
            };
        }
        Next::Finish => {
            let looking = world.remove_resource::<Looking>().expect("checked above");
            finish(world, looking);
        }
        Next::GiveUp => {
            let looking = world.remove_resource::<Looking>().expect("checked above");
            clear_away(world, &looking);
            let _ = looking.reply.send(Response::failure(format!(
                "the picture was not ready within {} s",
                LOOK_ANSWER_TIMEOUT.as_secs()
            )));
        }
    }
}

/// A readback landed. It fires every frame while its entity lives; the
/// first set of bytes is the picture.
fn on_picture(trigger: On<ReadbackComplete>, looking: Option<ResMut<Looking>>) {
    let Some(mut looking) = looking else {
        return;
    };
    let event = trigger.event();
    if let Stage::Reading { readback, data } = &mut looking.stage
        && *readback == event.entity
        && data.is_none()
    {
        *data = Some(event.data.clone());
    }
}

/// The camera, its image and its readback go: nothing renders any more.
fn clear_away(world: &mut World, looking: &Looking) {
    world.despawn(looking.camera);
    if let Stage::Reading { readback, .. } = looking.stage {
        world.despawn(readback);
    }
    world
        .resource_mut::<Assets<Image>>()
        .remove(&looking.target);
}

/// The picture is in: clear the camera away, note what the world was still
/// doing, and write the file off the frame loop.
fn finish(world: &mut World, looking: Looking) {
    clear_away(world, &looking);
    let Looking {
        stage,
        reply,
        started,
        frames,
        path,
        keep_in,
        mut answer,
        ..
    } = looking;
    let Stage::Reading {
        data: Some(data), ..
    } = stage
    else {
        return;
    };
    answer["bakes_in_flight"] = json!(
        world
            .query_filtered::<(), With<bevy_symbios_texture::async_gen::PendingTexture>>()
            .iter(world)
            .count()
    );
    answer["world_building"] = json!(
        world
            .get_resource::<CompileJob>()
            .is_some_and(|job| job.progress().is_some())
    );
    answer["frames"] = json!(frames);
    let written = std::thread::Builder::new()
        .name("agent-look".into())
        .spawn(move || {
            let response = match save_picture(&path, keep_in.as_deref(), &data, LOOK_SIZE) {
                Ok(()) => {
                    if let Some(dir) = keep_in {
                        trim_pictures(&dir, LOOKS_KEPT);
                    }
                    let took = started.elapsed();
                    info!(
                        "Took a picture in {} ms over {frames} frames",
                        took.as_millis()
                    );
                    answer["path"] = json!(path);
                    answer["took_ms"] = json!(u64::try_from(took.as_millis()).unwrap_or(u64::MAX));
                    Response::success(answer)
                }
                Err(e) => Response::failure(format!("writing the picture: {e}")),
            };
            let _ = reply.send(response);
        });
    if let Err(e) = written {
        error!("No thread to write a picture on: {e}");
    }
}

/// Write the picture only its owner can read - a picture of someone's world
/// is as private as the log beside it. `own_dir` is the agent's own picture
/// directory, kept owner-only; a path the operator chose has its missing
/// directories made and nothing re-permissioned.
fn save_picture(
    path: &Path,
    own_dir: Option<&Path>,
    rgba: &[u8],
    size: (u32, u32),
) -> Result<(), String> {
    let png = encode_png(rgba, size)?;
    match own_dir {
        Some(dir) => ensure_private_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?,
        None => {
            let dir = path.parent().ok_or("the picture's path has no directory")?;
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
    }
    write_private(path, &png).map_err(|e| format!("{}: {e}", path.display()))
}

/// The readback - sRGB RGBA rows - as an RGB PNG.
fn encode_png(rgba: &[u8], (width, height): (u32, u32)) -> Result<Vec<u8>, String> {
    let pixels = width as usize * height as usize;
    if rgba.len() != pixels * 4 {
        return Err(format!(
            "the readback is {} bytes, not the {} a {width}x{height} picture has",
            rgba.len(),
            pixels * 4
        ));
    }
    let rgb: Vec<u8> = rgba
        .chunks_exact(4)
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    let mut png = Vec::new();
    image::ImageEncoder::write_image(
        image::codecs::png::PngEncoder::new(&mut png),
        &rgb,
        width,
        height,
        image::ExtendedColorType::Rgb8,
    )
    .map_err(|e| e.to_string())?;
    Ok(png)
}

/// Keep the newest `keep` pictures in `dir` and remove the rest. Their names
/// carry the time they were taken, so the oldest sort first.
fn trim_pictures(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut pictures: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("look-") && name.ends_with(".png"))
        })
        .collect();
    pictures.sort();
    let surplus = pictures.len().saturating_sub(keep);
    for old in &pictures[..surplus] {
        // INTENTIONAL: best effort - a picture that will not go is tried
        // again after the next one.
        let _ = std::fs::remove_file(old);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-4;

    /// A body facing +Z has -X on its right (forward x up), the frame
    /// `status` reports in: 90 is right, -90 left, 180 behind.
    #[test]
    fn a_heading_turns_clockwise_from_the_facing() {
        let facing = Vec2::new(0.0, 1.0);
        let turned = |deg: f32| direction(facing, Vec2::ZERO, Some(deg), None).unwrap().0;
        assert!(turned(0.0).distance(facing) < EPS);
        assert!(turned(90.0).distance(Vec2::new(-1.0, 0.0)) < EPS);
        assert!(turned(-90.0).distance(Vec2::new(1.0, 0.0)) < EPS);
        assert!(turned(180.0).distance(-facing) < EPS);
        let ahead = direction(facing, Vec2::ZERO, None, None).unwrap();
        assert_eq!(ahead, (facing, 0.0), "no heading is straight ahead");
    }

    /// A point to look at gives the direction to it and says where it lies
    /// from the facing, in the heading's own terms.
    #[test]
    fn a_point_gives_its_direction_and_heading() {
        let facing = Vec2::new(0.0, 1.0);
        let (dir, heading) = direction(
            facing,
            Vec2::new(10.0, 10.0),
            None,
            Some(Vec2::new(0.0, 10.0)),
        )
        .unwrap();
        assert!(dir.distance(Vec2::new(-1.0, 0.0)) < EPS, "{dir}");
        assert!((heading - 90.0).abs() < 1e-3, "to the right: {heading}");
        assert!(direction(facing, Vec2::ZERO, None, Some(Vec2::new(0.01, 0.0))).is_err());
        assert!(direction(facing, Vec2::ZERO, Some(f32::NAN), None).is_err());
    }

    /// The play view is the orbit camera by what an orbit camera IS: it
    /// looks straight at its focus, from `radius` away, `pitch` above the
    /// level, along the direction asked. Checked against those properties,
    /// not against the formula that built the pose.
    #[test]
    fn the_play_pose_is_an_orbit_looking_the_way_asked() {
        let focus = Vec3::new(5.0, 20.0, -3.0);
        let (radius, pitch) = (12.0, 0.4);
        for dir in [
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(-0.6, -0.8),
        ] {
            let pose = play_pose(focus, radius, pitch, dir);
            let forward = pose.forward().as_vec3();
            let to_focus = focus - pose.translation;
            assert!((to_focus.length() - radius).abs() < 1e-3, "{dir}: distance");
            assert!(
                forward.distance(to_focus.normalize()) < EPS,
                "{dir}: looks at the focus"
            );
            assert!(
                (forward.y + pitch.sin()).abs() < EPS,
                "{dir}: looks down by the pitch"
            );
            let flat = Vec2::new(forward.x, forward.z).normalize();
            assert!(flat.distance(dir) < EPS, "{dir}: looks the way asked");
        }
    }

    /// The eyes are near the top of the body and in front of it, looking
    /// level the way asked - a 1.75 m body sees from about 1.63 m.
    #[test]
    fn the_eyes_are_at_the_front_of_the_head_looking_level() {
        let centre = Vec3::new(0.0, 10.0 + 1.75 / 2.0, 0.0);
        let pose = eyes_pose(centre, 1.75, 0.45, Vec2::new(1.0, 0.0));
        let above_feet = pose.translation.y - 10.0;
        assert!((above_feet - 0.93 * 1.75).abs() < 1e-3, "{above_feet}");
        assert!((pose.translation.x - 0.45).abs() < EPS, "in front");
        assert!(
            pose.forward().as_vec3().distance(Vec3::X) < EPS,
            "level, along +X"
        );
    }

    /// The agent's own directory keeps the newest pictures; anything else in
    /// it is left alone.
    #[test]
    fn old_pictures_are_trimmed_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("sa-looks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a directory");
        for stamp in [1000, 1003, 1001, 1002] {
            std::fs::write(dir.join(format!("look-{stamp}.png")), b"png").expect("written");
        }
        std::fs::write(dir.join("notes.txt"), b"mine").expect("written");

        trim_pictures(&dir, 2);

        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .expect("listed")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["look-1002.png", "look-1003.png", "notes.txt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A readback of the wrong size is refused rather than written as a
    /// sheared picture; one of the right size comes back as the PNG of it,
    /// alpha dropped.
    #[test]
    fn a_readback_becomes_an_rgb_png_of_its_own_size() {
        let refused = encode_png(&[0; 16], (64, 64)).expect_err("refused");
        assert!(refused.contains("bytes"), "{refused}");

        let rgba: Vec<u8> = (0..64 * 2).flat_map(|i| [i as u8, 7, 9, 255]).collect();
        let png = encode_png(&rgba, (64, 2)).expect("encoded");
        let decoded = image::load_from_memory(&png).expect("a PNG");
        assert_eq!((decoded.width(), decoded.height()), (64, 2));
        let rgb = decoded.as_rgb8().expect("RGB, not RGBA");
        assert_eq!(rgb.get_pixel(5, 1).0, [69, 7, 9]);
    }
}
