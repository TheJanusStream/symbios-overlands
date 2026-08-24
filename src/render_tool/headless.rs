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

use bevy_symbios_avatar::{AvatarBody as BuiltBody, AvatarJoints, AvatarPose, spawn_avatar};
use symbios_avatar::{Ground, Pose, Speed, Walk};

use crate::pds::avatar::wardrobe::engine_default_for_seed;
use crate::pds::avatar::{AttachmentRecord, ResolvedAttachment};
use crate::pds::{Environment, Generator, Placement, RoomRecord, TransformData};
use crate::player::attachments::{ensure_joint_visibility, placements};
use crate::player::visuals::{AvatarSpawnDeps, spawn_visual_tree};
use crate::world_builder::particles::{Particle, ParticleEmitterMarker};

use super::{ANGLES, FOV, OUT_DIR, WARMUP};

/// What to render: a single generator tree, an `--ages` lineup of variants of
/// one tree (one grid row each), or a whole seeded room.
pub(super) enum Subject {
    Single(Box<Generator>),
    Lineup(Vec<Generator>),
    Room(Box<RoomRecord>),
    /// `--wear` (#1088): rigged bodies wearing one attachment. One grid row
    /// per body seed × pose in [`WEAR_POSES`], the item engine-seated at
    /// `socket` exactly as a worn identity-offset record is in-game.
    Wear {
        seeds: Vec<u64>,
        item: Box<Generator>,
        socket: symbios_avatar::Socket,
    },
}

/// The pose set every `--wear` body is sheeted in: the rest stance, and two
/// opposite extremes of a walk cycle — where a hip or hand item meets the
/// swinging limbs. Deterministic (a gait pose is a pure function of its
/// cycle), so sheets diff across runs. A supine sleep row is deliberately
/// absent: sleeping is an overlands locomotion state driven by the live
/// animator, not an engine pose this tool can evaluate statically.
const WEAR_POSES: [WearPose; 3] = [WearPose::Rest, WearPose::Walk(0.15), WearPose::Walk(0.65)];

/// Walking pace the walk rows are posed at, in metres per second.
const WEAR_WALK_PACE: f32 = 1.4;

/// Texture atlas for `--wear` bodies — the game's draft rung, because a
/// sheet of N bodies at the full 1024 atlas is all cost and no judgement.
const WEAR_ATLAS: u32 = 256;

#[derive(Clone, Copy)]
enum WearPose {
    Rest,
    Walk(f32),
}

impl WearPose {
    /// Evaluate this pose against a built body's rig — [`Pose::rest`], or
    /// the engine's own walk drive at a fixed cycle on a level floor (the
    /// recipe `player::rigged` uses live, minus the per-frame state).
    fn evaluate(self, rig: &symbios_avatar::Rig) -> Pose {
        let mut pose = Pose::rest(rig);
        if let Self::Walk(cycle) = self {
            let speed = Speed::new(rig, WEAR_WALK_PACE);
            let gait = speed.gait(rig);
            let stride = speed.stride(rig);
            Walk::at(cycle).drive(rig, &mut pose, &gait, &stride, |point| {
                Some(Ground::level(Vec3::new(point.x, 0.0, point.z)))
            });
        }
        pose
    }
}

/// One worn prop waiting for its body's joints to exist: `spawn_avatar`
/// inserts [`AvatarJoints`] at the command flush after [`setup`], so the
/// dressing happens on the next frame in [`dress_wear_bodies`] — well inside
/// the warm-up window.
#[derive(Component)]
pub(super) struct PendingWear(ResolvedAttachment);

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
    mut bindposes: ResMut<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>,
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
        Subject::Single(_) | Subject::Lineup(_) | Subject::Wear { .. } => 600.0,
    };

    // One off-screen target + orbiting camera per tile: a row of the four
    // angles per lineup slot (a single subject is one slot).
    let rows = match &job.subject {
        Subject::Lineup(variants) => variants.len(),
        Subject::Wear { seeds, .. } => seeds.len() * WEAR_POSES.len(),
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
            spawn_visual_tree(
                &mut commands,
                chassis,
                generator,
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
                spawn_visual_tree(
                    &mut commands,
                    chassis,
                    generator,
                    &mut meshes,
                    &mut materials,
                    &mut images,
                    &mut deps,
                    false,
                );
            }
        }
        Subject::Wear {
            seeds,
            item,
            socket,
        } => {
            spawn_neutral_sun(&mut commands);
            for (row, (seed, pose_spec)) in seeds
                .iter()
                .flat_map(|&seed| WEAR_POSES.iter().map(move |&p| (seed, p)))
                .enumerate()
            {
                let avatar = symbios_avatar::Avatar::build_with(
                    &engine_default_for_seed(seed),
                    &symbios_avatar::AvatarConfig {
                        atlas: WEAR_ATLAS,
                        ..Default::default()
                    },
                )
                .unwrap_or_else(|| panic!("seeded body {seed} did not build"));
                let pose = pose_spec.evaluate(&avatar.rig);
                let mut worn = AttachmentRecord::new((**item).clone(), *socket);
                worn.sanitize();
                // The game's facing bridge: engine bodies face +Z, the
                // orbit's "front" angle assumes -Z (`rigged_root_transform`).
                let root = commands
                    .spawn((
                        Transform::from_xyz(row as f32 * SLOT_SPACING, 0.0, 0.0)
                            .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
                        Visibility::default(),
                        AvatarPose(pose),
                        PendingWear(ResolvedAttachment {
                            rkey: format!("wear-{row}"),
                            record: worn,
                        }),
                    ))
                    .id();
                spawn_avatar(
                    &mut commands,
                    root,
                    avatar,
                    0.0,
                    &mut meshes,
                    &mut materials,
                    &mut images,
                    &mut bindposes,
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

/// Dress every `--wear` body whose joints have landed: the same
/// `placements` seating the game uses (engine seat + outward yaw), the prop
/// spawned under its carrying joint's entity through the avatar-mode visual
/// pipeline. Runs every frame but each body is dressed once — the
/// [`PendingWear`] component is the queue and is removed on the way out.
pub(super) fn dress_wear_bodies(
    mut commands: Commands,
    pending: Query<(Entity, &BuiltBody, &AvatarJoints, &PendingWear)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut deps: AvatarSpawnDeps,
) {
    for (root, body, joints, wear) in &pending {
        ensure_joint_visibility(&mut commands, joints);
        let desired = std::slice::from_ref(&wear.0);
        for (joint, transform, attachment) in placements(&body.avatar, desired) {
            let Some(&carrier) = joints.0.get(joint) else {
                continue;
            };
            let prop = commands
                .spawn((transform, Visibility::default(), ChildOf(carrier)))
                .id();
            spawn_visual_tree(
                &mut commands,
                prop,
                &attachment.record.item,
                &mut meshes,
                &mut materials,
                &mut images,
                &mut deps,
                false,
            );
        }
        commands.entity(root).remove::<PendingWear>();
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
                spawn_visual_tree(
                    commands, chassis, generator, meshes, materials, images, deps, false,
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
                    spawn_visual_tree(
                        commands, chassis, generator, meshes, materials, images, deps, false,
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
            shadow_maps_enabled: false,
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
            shadow_maps_enabled: false,
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
