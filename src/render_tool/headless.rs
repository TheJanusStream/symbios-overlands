//! The headless render app: subject/job resources, camera + scene
//! setup, the framing/warmup drive loop, GPU readback capture, and the
//! contact-sheet writer.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::camera::primitives::Aabb;
use bevy::ecs::message::MessageWriter;
use bevy::prelude::*;
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};

use crate::pds::{Environment, Generator, Placement, RoomRecord, TransformData};
use crate::player::visuals::{AvatarSpawnDeps, spawn_avatar_visuals};

use super::{ANGLES, FOV, OUT_DIR, WARMUP};

/// What to render: a single generator tree, or a whole seeded room.
pub(super) enum Subject {
    Single(Box<Generator>),
    Room(Box<RoomRecord>),
}

#[derive(Resource)]
pub(super) struct RenderJob {
    pub(super) subject: Subject,
    pub(super) out: String,
    pub(super) size: u32,
}

#[derive(Component)]
pub(super) struct TileCam(usize);

#[derive(Resource)]
pub(super) struct Targets(Vec<Handle<Image>>);

#[derive(Resource, Default)]
pub(super) struct Frames(u32);

#[derive(Resource, Default)]
pub(super) struct Capture {
    framed: bool,
    started: bool,
    tile_of: HashMap<Entity, usize>,
    results: Vec<Option<Vec<u8>>>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn setup(
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
    use rand_chacha::rand_core::{RngCore, SeedableRng};

    // Uniform f32 in [0, 1) — the same minimal idiom the seeded-defaults
    // derivers use (the app links rand_core, not the full `rand` traits).
    fn unit(rng: &mut rand_chacha::ChaCha8Rng) -> f32 {
        (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    for placement in &record.placements {
        match placement {
            Placement::Absolute {
                generator_ref,
                transform,
                ..
            } => {
                let Some(generator) = record.generators.get(generator_ref) else {
                    continue;
                };
                let chassis = commands.spawn(to_transform(transform)).id();
                spawn_avatar_visuals(
                    commands, chassis, generator, None, meshes, materials, images, deps, false,
                );
            }
            // Expand scatters at full count so `--room` renders (and, with
            // `--features alloc-trace`, allocation-profiles) the region at its
            // true entity density — previously only Absolute placements
            // spawned, hiding the forests that dominate seeded rooms (#810/
            // #811). No terrain exists headless, so instances sit on the
            // ground plane with a seeded yaw instead of terrain-snapping.
            Placement::Scatter {
                generator_ref,
                bounds: crate::pds::ScatterBounds::Circle { center, radius },
                count,
                local_seed,
                ..
            } => {
                let Some(generator) = record.generators.get(generator_ref) else {
                    continue;
                };
                let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(*local_seed);
                for _ in 0..*count {
                    let angle = unit(&mut rng) * std::f32::consts::TAU;
                    let dist = radius.0 * unit(&mut rng).sqrt();
                    let yaw = unit(&mut rng) * std::f32::consts::TAU;
                    let chassis = commands
                        .spawn(
                            Transform::from_xyz(
                                center.0[0] + dist * angle.cos(),
                                0.0,
                                center.0[1] + dist * angle.sin(),
                            )
                            .with_rotation(Quat::from_rotation_y(yaw)),
                        )
                        .id();
                    spawn_avatar_visuals(
                        commands, chassis, generator, None, meshes, materials, images, deps, false,
                    );
                }
            }
            _ => {}
        }
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

pub(super) fn drive(
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

pub(super) fn on_capture(
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
