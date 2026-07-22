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
use crate::world_builder::particles::{Particle, ParticleEmitterMarker};

use super::{ANGLES, FOV, OUT_DIR, WARMUP};

/// What to render: a single generator tree, an `--ages` lineup of variants of
/// one tree (one grid row each), or a whole seeded room.
pub(super) enum Subject {
    Single(Box<Generator>),
    Lineup(Vec<Generator>),
    Room(Box<RoomRecord>),
}

/// World-space X distance between `Lineup` slots. Far enough apart that no
/// subject can bleed into a neighbouring slot's tiles, and the slot of a mesh
/// resolves from its world position alone (`round(x / SLOT_SPACING)`).
const SLOT_SPACING: f32 = 1000.0;

/// The framing query: every mesh entity that isn't a tile camera or a live
/// particle quad. Aliased because it appears in three signatures and the
/// inline form trips `clippy::type_complexity`.
type SubjectQuery<'w, 's> =
    Query<'w, 's, (&'static GlobalTransform, &'static Aabb), (Without<TileCam>, Without<Particle>)>;

/// Frames to wait for every lineup slot's AABB before framing falls back to a
/// tiny placeholder bound for the missing slots (a degenerate variant — e.g.
/// an iteration count whose derivation produced no meshes — must not hang the
/// tool).
const FRAME_GRACE: u32 = 300;

#[derive(Resource)]
pub(super) struct RenderJob {
    pub(super) subject: Subject,
    pub(super) out: String,
    pub(super) size: u32,
    /// `--elev`: camera elevation in degrees above the subject centre.
    /// `None` keeps the default low orbit (see [`cam_offset`]).
    pub(super) elev: Option<f32>,
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
    /// Frames spent waiting for subject AABBs pre-framing (lineup grace timer).
    waited: u32,
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
        Subject::Single(_) | Subject::Lineup(_) => 600.0,
    };

    // One off-screen target + orbiting camera per tile: a row of the four
    // angles per lineup slot (a single subject is one slot).
    let rows = match &job.subject {
        Subject::Lineup(variants) => variants.len(),
        _ => 1,
    };
    let mut targets = Vec::with_capacity(rows * ANGLES.len());
    for i in 0..rows * ANGLES.len() {
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
        Subject::Lineup(variants) => {
            spawn_neutral_sun(&mut commands);
            for (slot, generator) in variants.iter().enumerate() {
                let chassis = commands
                    .spawn(Transform::from_xyz(slot as f32 * SLOT_SPACING, 0.0, 0.0))
                    .id();
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
            // #811).
            //
            // Poses come from the compiler's own sampler (#912) so the sheet
            // shows the real clustering, scale and tilt rather than a
            // lookalike. The terrain-dependent filters — biome allow-list,
            // slope cutoff, terrain snapping — cannot run without a
            // heightmap, so instances sit on the ground plane and no sample
            // is rejected; the sheet is therefore denser than the game, which
            // is the right bias for judging arrangement.
            Placement::Scatter {
                generator_ref,
                bounds,
                count,
                local_seed,
                random_yaw,
                naturalness,
                ..
            } => {
                let Some(generator) = record.generators.get(generator_ref) else {
                    continue;
                };
                let mut preview = crate::world_builder::compile::ScatterPreview::new(
                    bounds,
                    *count,
                    *local_seed,
                    naturalness,
                    *random_yaw,
                );
                for _ in 0..*count {
                    let chassis = commands.spawn(preview.next_pose()).id();
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

#[allow(clippy::too_many_arguments)]
pub(super) fn drive(
    mut commands: Commands,
    mut frames: ResMut<Frames>,
    mut capture: ResMut<Capture>,
    targets: Res<Targets>,
    job: Res<RenderJob>,
    subject: SubjectQuery,
    emitters: Query<&GlobalTransform, With<ParticleEmitterMarker>>,
    mut cams: Query<(&mut Transform, &TileCam)>,
) {
    // Auto-frame the cameras on the subject's world AABB once it resolves
    // (Bevy computes mesh `Aabb`s a frame after spawn). A lineup frames each
    // slot's row on that slot's own centre but with one shared camera
    // distance, so relative subject size across rows stays honest.
    if !capture.framed {
        capture.waited += 1;
        let rows = targets.0.len() / ANGLES.len();
        if rows == 1 {
            // A subject that never resolves an AABB — a grammar that errored
            // or derived to nothing — would otherwise spin here forever, so
            // fall back to a placeholder bound and capture the empty frame.
            let bounds = subject_bounds(&subject, &emitters)
                .or_else(|| (capture.waited > FRAME_GRACE).then_some((Vec3::Y * 0.5, 0.5)));
            if let Some((center, radius)) = bounds {
                let dist = radius / (FOV * 0.5).tan() * 1.2 + radius * 0.5;
                for (mut transform, cam) in &mut cams {
                    let a = ANGLES[cam.0].to_radians();
                    let pos = center + cam_offset(a, dist, radius, job.elev);
                    *transform = Transform::from_translation(pos).looking_at(center, Vec3::Y);
                }
                capture.framed = true;
            }
            return;
        }
        if let Some(slots) = lineup_bounds(&subject, rows, capture.waited > FRAME_GRACE) {
            let max_radius = slots.iter().map(|s| s.1).fold(0.1f32, f32::max);
            let dist = max_radius / (FOV * 0.5).tan() * 1.2 + max_radius * 0.5;
            for (mut transform, cam) in &mut cams {
                let center = slots[cam.0 / ANGLES.len()].0;
                let a = ANGLES[cam.0 % ANGLES.len()].to_radians();
                let pos = center + cam_offset(a, dist, max_radius, job.elev);
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

/// Per-slot bounds of a lineup → one (centre, bounding radius) per row, slot
/// resolved from each mesh's world X (`round(x / SLOT_SPACING)`). Returns
/// `None` until every slot has at least one resolved AABB, unless `force` —
/// then still-empty slots get a tiny placeholder bound at their slot origin
/// so a degenerate variant can't hang the render.
fn lineup_bounds(q: &SubjectQuery, rows: usize, force: bool) -> Option<Vec<(Vec3, f32)>> {
    let mut mins = vec![Vec3::splat(f32::INFINITY); rows];
    let mut maxs = vec![Vec3::splat(f32::NEG_INFINITY); rows];
    for (gt, aabb) in q.iter() {
        let c = Vec3::from(aabb.center);
        let h = Vec3::from(aabb.half_extents);
        let slot = (gt.transform_point(c).x / SLOT_SPACING).round();
        if slot < 0.0 || slot as usize >= rows {
            continue;
        }
        let slot = slot as usize;
        for sx in [-1.0f32, 1.0] {
            for sy in [-1.0f32, 1.0] {
                for sz in [-1.0f32, 1.0] {
                    let w = gt.transform_point(c + Vec3::new(sx * h.x, sy * h.y, sz * h.z));
                    mins[slot] = mins[slot].min(w);
                    maxs[slot] = maxs[slot].max(w);
                }
            }
        }
    }
    let mut slots = Vec::with_capacity(rows);
    for (slot, (min, max)) in mins.into_iter().zip(maxs).enumerate() {
        if min.x > max.x {
            if !force {
                return None;
            }
            slots.push((Vec3::new(slot as f32 * SLOT_SPACING, 0.5, 0.0), 0.5));
        } else {
            slots.push(((min + max) * 0.5, ((max - min) * 0.5).length().max(0.1)));
        }
    }
    Some(slots)
}

/// Where a tile camera sits relative to the framed centre. `elev` (degrees,
/// from `--elev`) puts it on a true elevation arc; without it the camera
/// keeps the historic low orbit — a fixed `0.7 * radius` rise at full
/// distance, i.e. roughly 13° — which reads a facade well but cannot see
/// into anything open-topped.
fn cam_offset(yaw: f32, dist: f32, radius: f32, elev: Option<f32>) -> Vec3 {
    match elev {
        Some(deg) => {
            let e = deg.to_radians();
            let horiz = dist * e.cos();
            Vec3::new(horiz * yaw.sin(), dist * e.sin(), horiz * yaw.cos())
        }
        None => Vec3::new(dist * yaw.sin(), radius * 0.7, dist * yaw.cos()),
    }
}

/// Union the world-space AABB of every mesh entity → (centre, bounding radius).
/// The ground plane is excluded so a room frames on its buildings, not the
/// 160 m floor, and live [`Particle`] quads are excluded so a drifting smoke
/// plume can't jitter the framing from run to run.
///
/// Emitter *anchors* are folded in as points instead. An FX-heavy prop —
/// a fire whose smoke column is authored 2 m above a 0.9 m barrel — is
/// mostly not geometry, and framing on the geometry alone crops the very
/// thing an FX review is looking at. The anchors are static, so unlike the
/// particles they cost nothing in stability.
fn subject_bounds(
    q: &SubjectQuery,
    emitters: &Query<&GlobalTransform, With<ParticleEmitterMarker>>,
) -> Option<(Vec3, f32)> {
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
    for gt in emitters.iter() {
        let p = gt.translation();
        min = min.min(p);
        max = max.max(p);
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

/// Tile the RGBA captures into one PNG: `ANGLES.len()` columns per row, one
/// row per lineup slot (a single subject is one row — the original horizontal
/// strip).
fn save_contact_sheet(results: &[Option<Vec<u8>>], tile: u32, path: &str) -> Result<(), String> {
    let t = tile as usize;
    let cols = ANGLES.len().min(results.len()).max(1);
    let rows = results.len().div_ceil(cols);
    let sheet_w = tile * cols as u32;
    let stride = sheet_w as usize * 4;
    let mut sheet = vec![0u8; stride * t * rows];
    for (i, captured) in results.iter().enumerate() {
        let data = captured.as_ref().ok_or("missing tile")?;
        if data.len() < t * t * 4 {
            return Err(format!("tile {i} short: {} bytes", data.len()));
        }
        let (row, col) = (i / cols, i % cols);
        for y in 0..t {
            let src = &data[y * t * 4..(y + 1) * t * 4];
            let dst = (row * t + y) * stride + col * t * 4;
            sheet[dst..dst + t * 4].copy_from_slice(src);
        }
    }
    std::fs::create_dir_all(OUT_DIR).map_err(|e| e.to_string())?;
    image::save_buffer(
        path,
        &sheet,
        sheet_w,
        tile * rows as u32,
        image::ExtendedColorType::Rgba8,
    )
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
