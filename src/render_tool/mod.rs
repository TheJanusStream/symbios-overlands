//! Headless render tool — renders any subject (avatar / catalogue item /
//! primitive / whole seeded room) through the **real** spawn path
//! ([`crate::player::visuals::spawn_avatar_visuals`], which routes every node
//! kind — primitives, Shape grammar, L-system — through the same machinery the
//! game uses) into a multi-angle **contact-sheet** PNG. Lets the agent
//! self-validate geometry/materials without manual in-game screenshots.
//!
//! Lives in the library (not the `render` bin) so it can reach the
//! crate-internal `SpawnCtx`/cache resources; the bin is a one-line shim.
//!
//! ```text
//! cargo run --bin render -- --avatar 1          # seed or DID
//! cargo run --bin render -- --catalogue villa   # any catalogue slug
//! cargo run --bin render -- --prim tube         # a single primitive kind
//! cargo run --bin render -- --room 3            # the seeded settlement
//! # → /tmp/avatar-render/<label>.png  (front / ¾ / side / back tiles)
//! ```

use std::collections::HashMap;
use std::time::Duration;

use bevy::app::{AppExit, ScheduleRunnerPlugin};
use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::camera::primitives::Aabb;
use bevy::ecs::message::MessageWriter;
use bevy::prelude::*;
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::window::ExitCondition;
use clap::Parser;

use crate::pds::avatar::default_visuals::{build_for_did, build_for_seed};
use crate::pds::types::{Fp, Fp2};
use crate::pds::{Environment, Generator, GeneratorKind, Placement, RoomRecord, TransformData};
use crate::player::visuals::{AvatarSpawnDeps, spawn_avatar_visuals};

/// Camera yaw per tile (degrees), left→right: front, ¾, side, back. Avatars /
/// vehicles face local -Z, so the camera sits on the -Z side (`cos 180 = -1`)
/// to face the subject.
const ANGLES: [f32; 4] = [180.0, 135.0, 90.0, 0.0];
/// Default perspective FOV (matches Bevy's `PerspectiveProjection` default).
const FOV: f32 = std::f32::consts::FRAC_PI_4;
/// Frames to run (after framing) before capturing, so procedural textures
/// finish baking + patching into their materials.
const WARMUP: u32 = 200;
const OUT_DIR: &str = "/tmp/avatar-render";

#[derive(Parser)]
#[command(about = "Headless contact-sheet renderer for avatars / catalogue / primitives / rooms")]
struct Args {
    /// Avatar subject: a u64 seed or a DID string.
    #[arg(long)]
    avatar: Option<String>,
    /// List the first `--family-count` seeds whose
    /// [`ChassisFamily`](crate::seeded_defaults::ChassisFamily) matches
    /// (`humanoid` | `boat` | `airship` | `skiff`) and exit — a survey aid for
    /// the avatar overhaul: pick seeds from the printed list, then render each
    /// with `--avatar <seed>`. Highest precedence (prints, never renders).
    #[arg(long)]
    family_seeds: Option<String>,
    /// How many seeds `--family-seeds` prints.
    #[arg(long, default_value_t = 8)]
    family_count: usize,
    /// Catalogue subject: an entry slug (e.g. `villa`, `bench`, `wizard_tower`).
    #[arg(long)]
    catalogue: Option<String>,
    /// Render a [`Generator`] deserialized from a JSON file. Lets the agent
    /// iterate on an L-system grammar (or any generator) without recompiling
    /// the crate: `--dump` a catalogue entry to seed the JSON, edit the
    /// grammar / scalars, re-render. Highest precedence.
    #[arg(long)]
    generator: Option<String>,
    /// With `--catalogue <slug>` or `--avatar <seed|did>`: print that subject's
    /// built [`Generator`] as pretty JSON to stdout and exit (a valid seed file
    /// for `--generator`, enabling a no-recompile geometry-iteration loop).
    #[arg(long, default_value_t = false)]
    dump: bool,
    /// Primitive subject: a kind tag (`cuboid`, `sphere`, `tube`, `bevel`, …).
    #[arg(long)]
    prim: Option<String>,
    /// Room subject: a u64 seed or DID — renders the seeded settlement cluster.
    #[arg(long)]
    room: Option<String>,
    /// Road-graph diagnostics: a u64 seed or DID — reproduces the room's
    /// heightmap, builds the *meshed* road graph, and prints topology +
    /// geometry-risk stats (degree histogram, dead-end spurs, spurious-junction
    /// and spike-risk counts), then exits. A no-render dump to size road-network
    /// data filtering. Runs before any render app stands up.
    #[arg(long)]
    road_dump: Option<String>,
    /// Torture/cut overrides for a `--prim` subject (for testing the prim
    /// system). `--shear x,z` · `--twist rad` · `--taper x,z` · `--pathcut a,b`
    /// · `--profilecut a,b` · `--hollow h`.
    #[arg(long)]
    shear: Option<String>,
    #[arg(long)]
    twist: Option<f32>,
    #[arg(long)]
    taper: Option<String>,
    #[arg(long)]
    pathcut: Option<String>,
    #[arg(long)]
    profilecut: Option<String>,
    #[arg(long)]
    hollow: Option<f32>,
    /// Per-tile pixel side. Forced to a multiple of 64 (no GPU row padding).
    #[arg(long, default_value_t = 512)]
    size: u32,
    /// Output PNG path (defaults to `/tmp/avatar-render/<label>.png`).
    #[arg(long)]
    out: Option<String>,
}

/// What to render: a single generator tree, or a whole seeded room.
enum Subject {
    Single(Box<Generator>),
    Room(Box<RoomRecord>),
}

#[derive(Resource)]
struct RenderJob {
    subject: Subject,
    out: String,
    size: u32,
}

#[derive(Component)]
struct TileCam(usize);

#[derive(Resource)]
struct Targets(Vec<Handle<Image>>);

#[derive(Resource, Default)]
struct Frames(u32);

#[derive(Resource, Default)]
struct Capture {
    framed: bool,
    started: bool,
    tile_of: HashMap<Entity, usize>,
    results: Vec<Option<Vec<u8>>>,
}

/// CLI entry point (called by the `render` bin).
pub fn run() {
    let args = Args::parse();

    // `--family-seeds <fam>`: print the first N seeds mapping to a chassis
    // family and exit — a survey aid, never renders.
    if let Some(fam) = &args.family_seeds {
        print_family_seeds(fam, args.family_count);
        return;
    }

    // `--road-dump <seed|did>`: print the room's road-graph diagnostics and
    // exit — a no-render topology/geometry-risk dump for the road-filtering work.
    if let Some(room) = &args.road_dump {
        dump_road_graph(room);
        return;
    }

    // `--dump`: serialize the subject's generator to stdout (a valid
    // `--generator` seed) and exit before standing up the render app. Supports
    // a catalogue slug or an avatar seed/DID so either can drive the fast
    // no-recompile geometry loop.
    if args.dump {
        let g = if let Some(slug) = args.catalogue.as_deref() {
            crate::catalogue::by_slug(slug)
                .unwrap_or_else(|| panic!("unknown catalogue slug {slug:?}"))
                .build("did:render:tool")
        } else if let Some(avatar) = args.avatar.as_deref() {
            match avatar.parse::<u64>() {
                Ok(seed) => build_for_seed(seed, &format!("did:render:{seed}")).0,
                Err(_) => build_for_did(avatar).0,
            }
        } else {
            panic!("--dump requires --catalogue <slug> or --avatar <seed|did>");
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&g).expect("generator serialize")
        );
        return;
    }

    let (subject, label) = resolve_subject(&args);
    let out = args
        .out
        .clone()
        .unwrap_or_else(|| format!("{OUT_DIR}/{label}.png"));
    let size = (args.size / 64).max(1) * 64;

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .disable::<bevy::winit::WinitPlugin>(),
    )
    .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::ZERO));
    // Resources + texture/material plugins the real spawn path reads.
    crate::world_builder::register_headless_spawn(&mut app);
    app.insert_resource(ClearColor(Color::srgb(0.52, 0.55, 0.70)))
        .insert_resource(RenderJob { subject, out, size })
        .init_resource::<Frames>()
        .init_resource::<Capture>()
        .add_systems(Startup, setup)
        .add_systems(Update, drive)
        .run();
}

/// Build the subject + a filename label from the CLI args.
/// Precedence: `--generator` → `--room` → `--prim` → `--catalogue` → `--avatar` → seed 7.
fn resolve_subject(args: &Args) -> (Subject, String) {
    if let Some(path) = &args.generator {
        let json = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read generator {path:?}: {e}"));
        let generator: Generator =
            serde_json::from_str(&json).unwrap_or_else(|e| panic!("parse generator {path:?}: {e}"));
        let label = std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("generator")
            .to_string();
        return (Subject::Single(Box::new(generator)), format!("gen-{label}"));
    }
    if let Some(room) = &args.room {
        let record = match room.parse::<u64>() {
            Ok(seed) => RoomRecord::default_for_seed(seed, &format!("did:render:{seed}")),
            Err(_) => RoomRecord::default_for_did(room),
        };
        let label = format!("room-{}", room.replace([':', '/'], "_"));
        return (Subject::Room(Box::new(record)), label);
    }
    if let Some(tag) = &args.prim {
        let mut kind =
            primitive_for_tag(tag).unwrap_or_else(|| panic!("unknown primitive tag {tag:?}"));
        apply_prim_overrides(&mut kind, args);
        return (
            Subject::Single(Box::new(Generator::from_kind(kind))),
            format!("prim-{}", tag.to_lowercase()),
        );
    }
    if let Some(slug) = &args.catalogue {
        let entry = crate::catalogue::by_slug(slug)
            .unwrap_or_else(|| panic!("unknown catalogue slug {slug:?}"));
        return (
            Subject::Single(Box::new(entry.build("did:render:tool"))),
            format!("cat-{slug}"),
        );
    }
    let avatar = args.avatar.clone().unwrap_or_else(|| "7".to_string());
    let (generator, label) = match avatar.parse::<u64>() {
        Ok(seed) => (
            build_for_seed(seed, &format!("did:render:{seed}")).0,
            format!("seed-{seed}"),
        ),
        Err(_) => (build_for_did(&avatar).0, avatar.replace([':', '/'], "_")),
    };
    (Subject::Single(Box::new(generator)), label)
}

/// Print the first `count` u64 seeds whose
/// [`ChassisFamily`](crate::seeded_defaults::ChassisFamily) matches `fam`
/// (case-insensitive `humanoid` | `boat` | `airship` | `skiff`). A survey aid
/// for the avatar overhaul — seeds map 25 % to each family, so scanning a few
/// thousand always finds enough.
fn print_family_seeds(fam: &str, count: usize) {
    use crate::seeded_defaults::ChassisFamily;
    let want = match fam.to_lowercase().as_str() {
        "humanoid" => ChassisFamily::Humanoid,
        "boat" => ChassisFamily::Boat,
        "airship" => ChassisFamily::Airship,
        "skiff" => ChassisFamily::Skiff,
        other => panic!("unknown family {other:?} (humanoid|boat|airship|skiff)"),
    };
    let seeds: Vec<u64> = (0u64..1_000_000)
        .filter(|&s| ChassisFamily::for_seed(s) == want)
        .take(count)
        .collect();
    println!("{want:?} seeds: {seeds:?}");
}

/// Reproduce a room's heightmap + road config and print the road-graph
/// diagnostics (see [`crate::urban::road_graph_diagnostics`]) to stdout. The
/// room is the seeded default for a `u64` seed or a DID string — the same
/// derivation `--room` uses — so the heightmap and road network match what the
/// game renders for that room.
fn dump_road_graph(room: &str) {
    let record = match room.parse::<u64>() {
        Ok(seed) => RoomRecord::default_for_seed(seed, &format!("did:render:{seed}")),
        Err(_) => RoomRecord::default_for_did(room),
    };
    let Some(config) = crate::pds::find_road_config(&record).cloned() else {
        println!(
            "room {room:?}: no road config — this room grows no roads (try a road-growing theme seed)"
        );
        return;
    };
    if !config.enabled {
        println!("room {room:?}: road config present but disabled");
        return;
    }
    println!(
        "room {room:?}: minor_spacing {:.1} m, major_spacing {:.1} m",
        config.minor_spacing.0, config.major_spacing.0
    );
    let hm = crate::terrain::rebuild_heightmap_for_record(&record);
    match crate::urban::road_graph_diagnostics(&hm, &config) {
        Some(stats) => print!("{}", stats.report(room)),
        None => println!(
            "room {room:?}: road graph produced no network (district window too small or tracer empty)"
        ),
    }
}

/// Resolve a primitive tag (case-insensitive) to a default kind. Wraps
/// [`GeneratorKind::default_primitive_for_tag`], which is title-cased.
/// Parse a `"a,b"` pair into `[f32; 2]` (missing components default to 0).
fn parse2(s: &str) -> [f32; 2] {
    let mut it = s.split(',').map(|x| x.trim().parse::<f32>().unwrap_or(0.0));
    [it.next().unwrap_or(0.0), it.next().unwrap_or(0.0)]
}

/// Apply the CLI torture/cut overrides to a `--prim` subject for testing.
fn apply_prim_overrides(kind: &mut GeneratorKind, args: &Args) {
    let Some(t) = kind.torture_mut() else {
        return;
    };
    if let Some(s) = &args.shear {
        t.shear = Fp2(parse2(s));
    }
    if let Some(v) = args.twist {
        t.twist = Fp(v);
    }
    if let Some(s) = &args.taper {
        t.taper = Fp2(parse2(s));
    }
    if let Some(s) = &args.pathcut {
        t.path_cut = Fp2(parse2(s));
    }
    if let Some(s) = &args.profilecut {
        t.profile_cut = Fp2(parse2(s));
    }
    if let Some(h) = args.hollow {
        t.hollow = Fp(h);
    }
}

fn primitive_for_tag(tag: &str) -> Option<GeneratorKind> {
    let mut chars = tag.chars();
    let titled: String = match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => return None,
    };
    GeneratorKind::default_primitive_for_tag(&titled)
}

#[allow(clippy::too_many_arguments)]
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut deps: AvatarSpawnDeps,
    job: Res<RenderJob>,
) {
    // Lighting / clear colour: neutral studio for a single subject, the room's
    // own atmosphere for a room.
    let ambient = match &job.subject {
        Subject::Room(record) => {
            let env = &record.environment;
            commands.insert_resource(ClearColor(srgb3(env.sky_color.0)));
            env.ambient_brightness.0.max(80.0)
        }
        Subject::Single(_) => 600.0,
    };

    // One off-screen target + orbiting camera per angle.
    let mut targets = Vec::with_capacity(ANGLES.len());
    for (i, _) in ANGLES.iter().enumerate() {
        let target = images.add(new_target(job.size));
        targets.push(target.clone());
        commands.spawn((
            Camera3d::default(),
            RenderTarget::Image(target.into()),
            Msaa::Off,
            AmbientLight {
                color: Color::WHITE,
                brightness: ambient,
                ..default()
            },
            TileCam(i),
            // Placeholder; `drive` reframes once the subject's bounds resolve.
            Transform::from_xyz(0.0, 1.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
        ));
    }
    commands.insert_resource(Targets(targets));

    match &job.subject {
        Subject::Single(generator) => {
            spawn_neutral_sun(&mut commands);
            let chassis = commands.spawn(Transform::default()).id();
            spawn_avatar_visuals(
                &mut commands,
                chassis,
                generator,
                None,
                &mut meshes,
                &mut materials,
                &mut images,
                &mut deps,
                false,
            );
        }
        Subject::Room(record) => {
            spawn_env_sun(&mut commands, &record.environment);
            spawn_ground(&mut commands, &mut meshes, &mut materials);
            spawn_room(
                &mut commands,
                record,
                &mut meshes,
                &mut materials,
                &mut images,
                &mut deps,
            );
        }
    }
}

/// Spawn every `Absolute` placement (the settlement structures) at its anchor
/// through the real spawn path. `Scatter` placements (trees / rocks) need the
/// terrain-aware scatter expansion and are skipped in this overview render.
#[allow(clippy::too_many_arguments)]
fn spawn_room(
    commands: &mut Commands,
    record: &RoomRecord,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    deps: &mut AvatarSpawnDeps,
) {
    for placement in &record.placements {
        let Placement::Absolute {
            generator_ref,
            transform,
            ..
        } = placement
        else {
            continue;
        };
        let Some(generator) = record.generators.get(generator_ref) else {
            continue;
        };
        let chassis = commands.spawn(to_transform(transform)).id();
        spawn_avatar_visuals(
            commands, chassis, generator, None, meshes, materials, images, deps, false,
        );
    }
}

fn spawn_neutral_sun(commands: &mut Commands) {
    commands.spawn((
        DirectionalLight {
            illuminance: 11_000.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(3.0, 6.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn spawn_env_sun(commands: &mut Commands, env: &Environment) {
    // The light shines from `sun_position` toward the world origin.
    let sun_pos = Vec3::from_array(env.sun_position.0);
    let pos = if sun_pos.length_squared() > 1e-3 {
        sun_pos
    } else {
        Vec3::new(3.0, 6.0, 4.0)
    };
    commands.spawn((
        DirectionalLight {
            color: srgb3(env.sun_color.0),
            illuminance: env.sun_illuminance.0.max(2_000.0),
            shadows_enabled: false,
            ..default()
        },
        Transform::from_translation(pos).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn spawn_ground(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(80.0)))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.30, 0.33, 0.27),
            perceptual_roughness: 0.95,
            ..default()
        })),
        Transform::default(),
    ));
}

fn drive(
    mut commands: Commands,
    mut frames: ResMut<Frames>,
    mut capture: ResMut<Capture>,
    targets: Res<Targets>,
    subject: Query<(&GlobalTransform, &Aabb), Without<TileCam>>,
    mut cams: Query<(&mut Transform, &TileCam)>,
) {
    // Auto-frame the cameras on the subject's world AABB once it resolves
    // (Bevy computes mesh `Aabb`s a frame after spawn).
    if !capture.framed {
        if let Some((center, radius)) = subject_bounds(&subject) {
            let dist = radius / (FOV * 0.5).tan() * 1.2 + radius * 0.5;
            for (mut transform, cam) in &mut cams {
                let a = ANGLES[cam.0].to_radians();
                let pos = center + Vec3::new(dist * a.sin(), radius * 0.7, dist * a.cos());
                *transform = Transform::from_translation(pos).looking_at(center, Vec3::Y);
            }
            capture.framed = true;
        }
        return;
    }

    frames.0 += 1;
    if capture.started || frames.0 < WARMUP {
        return;
    }
    capture.started = true;
    capture.results = vec![None; targets.0.len()];
    for (i, target) in targets.0.iter().enumerate() {
        let e = commands
            .spawn(Readback::texture(target.clone()))
            .observe(on_capture)
            .id();
        capture.tile_of.insert(e, i);
    }
}

/// Union the world-space AABB of every mesh entity → (centre, bounding radius).
/// The ground plane is excluded so a room frames on its buildings, not the
/// 160 m floor.
fn subject_bounds(q: &Query<(&GlobalTransform, &Aabb), Without<TileCam>>) -> Option<(Vec3, f32)> {
    let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    let mut any = false;
    for (gt, aabb) in q.iter() {
        // Skip the wide ground plane (huge X/Z, ~zero Y extent).
        if aabb.half_extents.x > 40.0 || aabb.half_extents.z > 40.0 {
            continue;
        }
        any = true;
        let c = Vec3::from(aabb.center);
        let h = Vec3::from(aabb.half_extents);
        for sx in [-1.0f32, 1.0] {
            for sy in [-1.0f32, 1.0] {
                for sz in [-1.0f32, 1.0] {
                    let w = gt.transform_point(c + Vec3::new(sx * h.x, sy * h.y, sz * h.z));
                    min = min.min(w);
                    max = max.max(w);
                }
            }
        }
    }
    if !any {
        return None;
    }
    Some(((min + max) * 0.5, ((max - min) * 0.5).length().max(0.1)))
}

fn on_capture(
    trigger: On<ReadbackComplete>,
    job: Res<RenderJob>,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    let event = trigger.event();
    let Some(&tile) = capture.tile_of.get(&event.entity) else {
        return;
    };
    if capture.results[tile].is_some() {
        return;
    }
    capture.results[tile] = Some(event.data.clone());
    if capture.results.iter().any(|r| r.is_none()) {
        return;
    }
    match save_contact_sheet(&capture.results, job.size, &job.out) {
        Ok(()) => {
            info!("wrote {} ({} tiles)", job.out, capture.results.len());
            exit.write(AppExit::Success);
        }
        Err(e) => {
            error!("contact sheet save failed: {e}");
            exit.write(AppExit::error());
        }
    }
}

/// Tile the per-angle RGBA captures horizontally into one PNG.
fn save_contact_sheet(results: &[Option<Vec<u8>>], tile: u32, path: &str) -> Result<(), String> {
    let t = tile as usize;
    let sheet_w = tile * results.len() as u32;
    let stride = sheet_w as usize * 4;
    let mut sheet = vec![0u8; stride * t];
    for (i, captured) in results.iter().enumerate() {
        let data = captured.as_ref().ok_or("missing tile")?;
        if data.len() < t * t * 4 {
            return Err(format!("tile {i} short: {} bytes", data.len()));
        }
        for y in 0..t {
            let src = &data[y * t * 4..(y + 1) * t * 4];
            let dst = y * stride + i * t * 4;
            sheet[dst..dst + t * 4].copy_from_slice(src);
        }
    }
    std::fs::create_dir_all(OUT_DIR).map_err(|e| e.to_string())?;
    image::save_buffer(path, &sheet, sheet_w, tile, image::ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())
}

fn to_transform(t: &TransformData) -> Transform {
    Transform {
        translation: Vec3::from_array(t.translation.0),
        rotation: Quat::from_array(t.rotation.0),
        scale: Vec3::from_array(t.scale.0),
    }
}

fn srgb3(c: [f32; 3]) -> Color {
    Color::srgb(c[0], c[1], c[2])
}

fn new_target(size: u32) -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[133, 140, 178, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage =
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC | TextureUsages::TEXTURE_BINDING;
    image
}
