//! The headless render app: subject/job resources, camera + scene
//! setup, the framing/warmup drive loop, GPU readback capture, and the
//! contact-sheet and clip writers.
//!
//! ## Two shapes of output
//!
//! A **sheet** is the original instrument: N tile cameras (four angles per
//! row) render once, after a warm-up, and their tiles are laid into one
//! PNG. A **clip** is one camera moved along a [`CameraRig`] over
//! `--frames` captures, written as a GIF (`gif.rs`). `--world` always uses
//! the single rig camera, so a world still is a one-frame clip's first
//! frame written as a PNG.
//!
//! ## The clock
//!
//! Nothing in this app runs on wall time. `Time<Virtual>` is paused at
//! startup and [`tick_clock`] advances it by hand - every frame while the
//! scene builds and warms up, and during a clip **once per captured frame**
//! ([`Clock::once`], armed by the drive loop on the frame before each shot,
//! and only once the scene has caught up with the frame before: see
//! [`clip_step`]). Wind, clouds, water, particles and the walker's gait all
//! read the shader globals or `Time`, so a clip's frame `k` is the scene at
//! exactly `k / fps` seconds no matter how many app frames the readback of
//! frame `k - 1` took, and no matter how fast the machine is.

use std::collections::HashMap;
use std::time::Duration;

use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::camera::primitives::Aabb;
use bevy::ecs::message::MessageWriter;
use bevy::platform::time::Instant;
use bevy::prelude::*;
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::time::Virtual;

use bevy_symbios_avatar::{AvatarBody as BuiltBody, AvatarJoints, AvatarPose, spawn_avatar};
use symbios_avatar::{Ground, Pose, Speed, Walk};

use crate::pds::avatar::wardrobe::engine_default_for_seed;
use crate::pds::avatar::{AttachmentRecord, ResolvedAttachment};
use crate::pds::{Environment, Generator, Placement, RoomRecord, TransformData};
use crate::player::attachments::{ensure_joint_visibility, placements};
use crate::player::visuals::{AvatarSpawnDeps, spawn_visual_tree};
use crate::state::LiveRoomRecord;
use crate::terrain::FinishedHeightMap;
use crate::world_builder::particles::{Particle, ParticleEmitterMarker};

use super::rig::{CameraRig, Focus, delay_cs, half_fov_x, play_elev_deg, progress, px_per_metre};
use super::world::{
    ShutterGate, Walker, WorldReadiness, WorldSpec, resolve_focus, spawn_world_camera,
};
use super::{ANGLES, FOV, OUT_DIR, WARMUP, gif};

/// What to render: a single generator tree, an `--ages` lineup of variants of
/// one tree (one grid row each), a whole seeded room, or the world itself.
pub(super) enum Subject {
    Single(Box<Generator>),
    Lineup(Vec<Generator>),
    Room(Box<RoomRecord>),
    /// `--terrain` (#994): the room's real heightmap under its four-layer
    /// splat, shot as grazing landscape views across `view_m` metres.
    ///
    /// The record is handed to the terrain systems as a `LiveRoomRecord`
    /// rather than spawned here, so what appears is what the game builds.
    Terrain {
        record: Box<RoomRecord>,
        view_m: f32,
    },
    /// `--world`: the seeded room compiled by the game's own pipeline -
    /// terrain, streets, district, placements, water, sky. See `world.rs`.
    World(Box<WorldSpec>),
    /// `--wear` (#1088): rigged bodies wearing one attachment. One grid row
    /// per body seed × pose in [`WEAR_POSES`], the item engine-seated at
    /// `socket` exactly as a worn identity-offset record is in-game.
    Wear {
        seeds: Vec<u64>,
        item: Box<Generator>,
        socket: symbios_avatar::Socket,
        /// The entry's [`WearFit`](crate::catalogue::WearFit) declaration
        /// (#1089), stamped onto the built record exactly as the in-game
        /// Wear button stamps it - so the sheet shows the fitted sizes the
        /// game would.
        fit: Option<crate::catalogue::WearFit>,
    },
}

/// The pose set every `--wear` body is sheeted in: the rest stance, and two
/// opposite extremes of a walk cycle - where a hip or hand item meets the
/// swinging limbs. Deterministic (a gait pose is a pure function of its
/// cycle), so sheets diff across runs. A supine sleep row is deliberately
/// absent: sleeping is an overlands locomotion state driven by the live
/// animator, not an engine pose this tool can evaluate statically.
const WEAR_POSES: [WearPose; 3] = [WearPose::Rest, WearPose::Walk(0.15), WearPose::Walk(0.65)];

/// Walking pace the walk rows are posed at, in metres per second.
const WEAR_WALK_PACE: f32 = 1.4;

/// Texture atlas for `--wear` bodies - the game's draft rung, because a
/// sheet of N bodies at the full 1024 atlas is all cost and no judgement.
const WEAR_ATLAS: u32 = 256;

#[derive(Clone, Copy)]
enum WearPose {
    Rest,
    Walk(f32),
}

impl WearPose {
    /// Evaluate this pose against a built body's rig - [`Pose::rest`], or
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

/// Worn props waiting for their body's joints to exist: `spawn_avatar`
/// inserts [`AvatarJoints`] at the command flush after [`setup`], so the
/// dressing happens on the next frame in [`dress_wear_bodies`] - well inside
/// the warm-up window.
#[derive(Component)]
pub(super) struct PendingWear(pub(super) Vec<ResolvedAttachment>);

/// World-space X distance between `Lineup` slots. Far enough apart that no
/// subject can bleed into a neighbouring slot's tiles, and the slot of a mesh
/// resolves from its world position alone (`round(x / SLOT_SPACING)`).
const SLOT_SPACING: f32 = 1000.0;

/// The framing query: every mesh entity that isn't a tile camera or a live
/// particle quad. Aliased because it appears in three signatures and the
/// inline form trips `clippy::type_complexity`.
type SubjectQuery<'w, 's> = Query<
    'w,
    's,
    (&'static GlobalTransform, &'static Aabb),
    (Without<TileCam>, Without<Particle>, Without<GroundPlane>),
>;

/// The slot-placement query: a line-up slot's chassis entity, which
/// `--play-view` moves once its bounds have resolved. Filtered off the two
/// other `Transform` queries in [`drive`] so the three stay disjoint.
type PlaySlotQuery<'w, 's> =
    Query<'w, 's, (&'static mut Transform, &'static PlaySlot), (Without<TileCam>, Without<Walker>)>;

/// Fraction of a neighbour's own angular half-width left as clear air between
/// two line-up slots. Small: the point of the shot is that the craft are side
/// by side at one range, and air spent between them is angle spent off the
/// lens axis.
const PLAY_GAP: f32 = 1.18;

/// Frames to wait for every lineup slot's AABB before framing falls back to a
/// tiny placeholder bound for the missing slots (a degenerate variant - e.g.
/// an iteration count whose derivation produced no meshes - must not hang the
/// tool).
const FRAME_GRACE: u32 = 300;

/// Frames `--terrain` waits for the splat pass before giving up. Generous
/// because the work behind it is a heightmap job plus four texture bakes on
/// the compute pool, and it is a hard failure rather than a fallback: a
/// terrain render that quietly captured the placeholder colour would be a
/// picture of nothing, and it would look like a finished render.
const TERRAIN_GRACE: u32 = 4000;

/// Wall-clock budget for a `--world` compile to settle. Real seconds rather
/// than frames, because the road extrusion and the texture bakes run on
/// background tasks the frame loop merely polls.
const WORLD_BUDGET: Duration = Duration::from_secs(600);

/// Consecutive frames a world must report settled before it is believed.
/// The lot layer's debounce and the road re-mesh's are 0.3 s of clock;
/// forty frames at any supported `--fps` is longer than both.
const WORLD_QUIET: u32 = 40;

/// How often the world wait logs where it is.
const WORLD_LOG_EVERY: Duration = Duration::from_secs(3);

/// Where a play-view slot's origin goes, and on whose authority.
pub(super) enum Ride {
    /// Read off the subject's own locomotion record - the game's answer, and
    /// the only one the view can make a claim about.
    Derived(f32),
    /// Given by `--ride-height`, because there was nothing to read.
    Told(f32),
    /// Nothing to read and nothing given: stand the subject on its own drawn
    /// bounds. The honest fallback for an airship (it holds itself up with
    /// thrust and has no ground ride height at all), for the reference
    /// figure (it has feet), and for a `--generator` prototype before anyone
    /// has worked out where it should float.
    Bounds,
}

impl Ride {
    /// The height, when one is known.
    pub(super) fn height(&self) -> Option<f32> {
        match self {
            Self::Derived(h) | Self::Told(h) => Some(*h),
            Self::Bounds => None,
        }
    }

    /// What the log says about where this slot ended up.
    fn why(&self) -> &'static str {
        match self {
            Self::Derived(_) => "derived ride height",
            Self::Told(_) => "told by --ride-height",
            Self::Bounds => "resting on its own bounds",
        }
    }
}

/// `--play-view` (#1360): the game's chase camera, over a ground plane, with
/// every line-up slot stood where the game stands it.
pub(super) struct PlayView {
    /// Where each slot's origin goes, in line-up order.
    pub(super) ride: Vec<Ride>,
    /// The frame actually rendered, for the pixels-per-metre the log quotes
    /// and the horizontal angle the line-up's spread is checked against.
    pub(super) frame: (u32, u32),
}

#[derive(Resource)]
pub(super) struct RenderJob {
    pub(super) subject: Subject,
    /// `--play-view`, when the shot is at the chase camera's range.
    pub(super) play: Option<PlayView>,
    pub(super) out: String,
    /// Per-tile (or per-frame) pixel size, width × height.
    pub(super) tile: (u32, u32),
    /// `--elev` for the sheet cameras: elevation in degrees above the
    /// subject centre. `None` keeps the default low orbit (see
    /// [`cam_offset`]).
    pub(super) elev: Option<f32>,
    /// The single-camera rig - every `--world` shot and every clip.
    pub(super) rig: CameraRig,
    /// Frames in the clip; 1 is a still.
    pub(super) frames: u32,
    pub(super) fps: f32,
    /// Also dump a clip's frames as PNGs beside the GIF.
    pub(super) keep_frames: bool,
    /// `--dither`: the GIF encoder's ordered-dither amplitude.
    pub(super) dither: f32,
    /// `--downscale`: how many times smaller a single-camera shot is written
    /// than it renders (1 is full size).
    pub(super) downscale: u32,
}

impl RenderJob {
    /// Whether this job drives one rig camera rather than the tile set.
    pub(super) fn single_camera(&self) -> bool {
        matches!(self.subject, Subject::World(_)) || self.frames > 1 || self.play.is_some()
    }
}

#[derive(Component)]
pub(super) struct TileCam(pub(super) usize);

/// The studio floor. Marked rather than recognised by its size, so framing
/// can exclude it without the "any mesh wider than 80 m is the ground"
/// guess that a big subject would trip.
#[derive(Component)]
pub(super) struct GroundPlane;

/// A line-up slot's chassis entity, by index. `--play-view` re-places these
/// once every slot's bounds have resolved - a subject cannot be stood at the
/// right distance until it is known how wide it is.
#[derive(Component)]
pub(super) struct PlaySlot(usize);

#[derive(Resource)]
pub(super) struct Targets(Vec<Handle<Image>>);

/// The app's hand-driven clock. See the module docs.
#[derive(Resource)]
pub(super) struct Clock {
    /// Seconds per step: `1 / fps`.
    pub(super) step: f32,
    /// Step every frame (building, warming up).
    pub(super) run: bool,
    /// Step on the next frame only (a clip frame is about to be shot).
    pub(super) once: bool,
    /// Whether this frame's [`tick_clock`] stepped it: what a script counts
    /// its frames by.
    pub(super) stepped: bool,
    /// Seconds stepped so far.
    pub(super) elapsed: f32,
}

/// Inserted on the way into warm-up: the clock second the first capture
/// will happen at, which is what the walker's lead-in counts back from.
#[derive(Resource)]
pub(super) struct ClipTiming {
    pub(super) capture_start: f32,
}

/// Inserted when a clip's first frame is shot: from here on, a script's
/// steps after `start` play, one per captured frame.
#[derive(Resource)]
pub(super) struct ClipStarted;

/// Stop the virtual clock so only [`tick_clock`] moves it.
pub(super) fn pause_virtual_time(mut time: ResMut<Time<Virtual>>) {
    time.pause();
}

/// Advance the paused virtual clock by one step and republish it as the
/// generic `Time` every system (and the render extract) reads - `First`,
/// after Bevy's own time system has copied the paused (zero-delta) clock.
pub(super) fn tick_clock(
    mut clock: ResMut<Clock>,
    mut virt: ResMut<Time<Virtual>>,
    mut generic: ResMut<Time>,
) {
    clock.stepped = clock.run || clock.once;
    if !clock.stepped {
        return;
    }
    clock.once = false;
    virt.advance_by(Duration::from_secs_f32(clock.step));
    *generic = virt.as_generic();
    clock.elapsed += clock.step;
}

/// Where the drive loop is.
pub(super) enum Phase {
    /// Waiting for the subject to exist (bounds, splat, or a settled world).
    Framing,
    /// Letting textures bake and FX reach steady state; the clock runs.
    Warmup { left: u32 },
    /// Every tile camera read back once.
    Sheet,
    /// One frame at a time: `next` is the frame to shoot, `pending` the
    /// readback in flight for the one before it, and `armed` whether the
    /// clock has been set to step for `next`.
    Clip {
        next: u32,
        pending: Option<Entity>,
        armed: bool,
    },
}

/// What the warm-up does this frame - see [`warmup_verdict`].
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Warmup {
    /// Frames still to run.
    Counting,
    /// The count is done and a script still has setup steps to play: keep the
    /// clock running for them and shoot nothing.
    HeldForScript,
    /// The count is done but the scene is still catching up: this many
    /// texture bakes are airborne, or a compile pass is running. Hold the
    /// shutter (and the clock) until both have landed.
    HeldForScene { bakes: usize, compiling: bool },
    /// Shoot.
    Ready,
}

/// The warm-up's rule (#1351, #1353). The count runs down regardless of
/// bakes - the plumes need their frames whether or not a texture is airborne -
/// and only the shutter waits on them, because a material whose bake has not
/// landed wears a flat fallback colour, and a picture of that looks finished.
/// The count is a number of frames; a bake is a number of seconds on a thread
/// pool the frame loop merely polls, so no count is long enough on a fast
/// enough GPU. A script's setup steps play after the count, on the running
/// clock, and the scene is waited for last, because a setup step can start
/// work of its own: selecting a Catalogue entry stages its picture.
pub(super) fn warmup_verdict(
    left: u32,
    bakes_in_flight: usize,
    compiling: bool,
    setup_pending: bool,
) -> Warmup {
    if left > 0 {
        Warmup::Counting
    } else if setup_pending {
        Warmup::HeldForScript
    } else if bakes_in_flight > 0 || compiling {
        Warmup::HeldForScene {
            bakes: bakes_in_flight,
            compiling,
        }
    } else {
        Warmup::Ready
    }
}

/// What the drive loop does between two captures of a clip - see
/// [`clip_step`].
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ClipStep {
    /// Every frame is shot: write the clip.
    Finish,
    /// The scene is still catching up with the last frame: wait.
    Hold,
    /// Set the clock to step at the top of the next frame.
    Arm,
    /// The clock stepped this frame: shoot.
    Shoot,
}

/// The clip's rule between two captures (#1353). A frame is armed, the clock
/// set to step once, only while the scene is quiet ([`ShutterGate::busy`]):
/// a gizmo release commits the record and the placement is rebuilt over the
/// next few frames, and a frame shot in the middle of that shows the building
/// gone. The shot follows its arming unconditionally, so the step and the
/// capture stay one frame apart and frame `k` is still the scene at exactly
/// `k / fps` seconds, however long the scene took to settle in between.
pub(super) fn clip_step(next: u32, frames: u32, armed: bool, busy: bool) -> ClipStep {
    if armed {
        ClipStep::Shoot
    } else if next >= frames {
        ClipStep::Finish
    } else if busy {
        ClipStep::Hold
    } else {
        ClipStep::Arm
    }
}

/// What framing decided for the rig camera.
struct Framing {
    /// The point the rig orbits (unless the focus is the walker, which is
    /// re-read per frame).
    focus: Vec3,
    /// Camera distance when the rig leaves it to the subject's bounds.
    auto_dist: f32,
    /// Elevation when the rig leaves it to the historic low orbit.
    auto_elev: f32,
}

#[derive(Resource)]
pub(super) struct Capture {
    phase: Phase,
    /// Frames spent waiting pre-framing (grace timers; the world's quiet run).
    waited: u32,
    since: Instant,
    last_log: Instant,
    framing: Option<Framing>,
    tile_of: HashMap<Entity, usize>,
    results: Vec<Option<Vec<u8>>>,
    frames: Vec<Vec<u8>>,
}

impl Default for Capture {
    fn default() -> Self {
        Self {
            phase: Phase::Framing,
            waited: 0,
            since: Instant::now(),
            last_log: Instant::now(),
            framing: None,
            tile_of: HashMap::new(),
            results: Vec::new(),
            frames: Vec::new(),
        }
    }
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
    editor: Option<Res<super::editor::EditorHost>>,
) {
    // Lighting / clear colour: neutral studio for a single subject, the room's
    // own atmosphere for a room. A world gets the game's atmosphere from
    // `register_headless_atmosphere` and the record's environment patch.
    let ambient = match &job.subject {
        Subject::Room(record) | Subject::Terrain { record, .. } => {
            let env = &record.environment;
            commands.insert_resource(ClearColor(srgb3(env.sky_color.0)));
            env.ambient_brightness.0.max(80.0)
        }
        Subject::World(_) => 0.0,
        Subject::Single(_) | Subject::Lineup(_) | Subject::Wear { .. } => 600.0,
    };

    // One off-screen target per camera: the rig camera alone, or a row of
    // the four angles per lineup slot (a single subject is one slot).
    let cameras = if job.single_camera() {
        1
    } else {
        let rows = match &job.subject {
            Subject::Lineup(variants) => variants.len(),
            Subject::Wear { seeds, .. } => seeds.len() * WEAR_POSES.len(),
            _ => 1,
        };
        rows * ANGLES.len()
    };
    let mut targets = Vec::with_capacity(cameras);
    for i in 0..cameras {
        let target = images.add(new_target(job.tile));
        targets.push(target.clone());
        if matches!(job.subject, Subject::World(_)) {
            spawn_world_camera(&mut commands, target, editor.is_some());
            continue;
        }
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

    // `--play-view` stands its subjects on a floor, which is the only reason
    // a hover gap or a tyre contact reads at all. Big enough that the
    // near-horizontal top of the frame never runs off its far edge.
    if job.play.is_some() {
        spawn_ground(&mut commands, &mut meshes, &mut materials, PLAY_FLOOR);
    }
    match &job.subject {
        Subject::Single(generator) => {
            spawn_sun(&mut commands, job.play.is_some());
            let chassis = commands.spawn((Transform::default(), PlaySlot(0))).id();
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
            spawn_sun(&mut commands, job.play.is_some());
            for (slot, generator) in variants.iter().enumerate() {
                let chassis = commands
                    .spawn((
                        Transform::from_xyz(slot as f32 * SLOT_SPACING, 0.0, 0.0),
                        PlaySlot(slot),
                    ))
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
            fit,
        } => {
            spawn_sun(&mut commands, false);
            for (row, (seed, pose_spec)) in seeds
                .iter()
                .flat_map(|&seed| WEAR_POSES.iter().map(move |&p| (seed, p)))
                .enumerate()
            {
                // No far tier, unlike `--walker` (#1358): a wear sheet is a
                // studio framing a metre or two from the head, so a far tier
                // here would be built and never drawn. Leaving it off is also
                // what keeps this sheet a control - the same bodies the take
                // was measured against, with nothing extra spawned.
                let avatar = symbios_avatar::Avatar::build_with(
                    &engine_default_for_seed(seed),
                    &symbios_avatar::AvatarConfig {
                        atlas: WEAR_ATLAS,
                        ..Default::default()
                    },
                )
                .unwrap_or_else(|| panic!("seeded body {seed} did not build"));
                let pose = pose_spec.evaluate(&avatar.rig);
                let mut worn = AttachmentRecord::with_fit((**item).clone(), *socket, *fit);
                worn.sanitize();
                // The game's facing bridge: engine bodies face +Z, the
                // orbit's "front" angle assumes -Z (`rigged_root_transform`).
                let root = commands
                    .spawn((
                        Transform::from_xyz(row as f32 * SLOT_SPACING, 0.0, 0.0)
                            .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
                        Visibility::default(),
                        AvatarPose(pose),
                        PendingWear(vec![ResolvedAttachment {
                            rkey: format!("wear-{row}"),
                            record: worn,
                        }]),
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
        Subject::Terrain { record, .. } => {
            // No `spawn_ground`: the terrain systems build the real one. The
            // sun matters more here than in any other mode - a grazing light
            // is what makes a repeating normal map legible as a repeat.
            spawn_env_sun(&mut commands, &record.environment);
        }
        // The world is compiled by the registered pipelines, not spawned.
        Subject::World(_) => {}
        Subject::Room(record) => {
            spawn_env_sun(&mut commands, &record.environment);
            spawn_ground(&mut commands, &mut meshes, &mut materials, ROOM_FLOOR);
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

/// Dress every body whose joints have landed: the same `placements`
/// seating the game uses (engine seat + outward yaw), the prop spawned
/// under its carrying joint's entity through the avatar-mode visual
/// pipeline. Runs every frame but each body is dressed once - the
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
        for (joint, transform, attachment) in placements(&body.avatar, &wear.0) {
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
            // true entity density - previously only Absolute placements
            // spawned, hiding the forests that dominate seeded rooms (#810/
            // #811).
            //
            // Poses come from the compiler's own sampler (#912) so the sheet
            // shows the real clustering, scale and tilt rather than a
            // lookalike. The terrain-dependent filters - biome allow-list,
            // slope cutoff, terrain snapping - cannot run without a
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

/// The studio sun. `shadows` casts them, which only `--play-view` asks for:
/// a craft that hovers and a craft that is beached are the same picture
/// without a contact shadow on the floor, and telling those two apart is the
/// whole reason that mode has a floor at all.
fn spawn_sun(commands: &mut Commands, shadows: bool) {
    let mut sun = commands.spawn((
        DirectionalLight {
            illuminance: 11_000.0,
            shadow_maps_enabled: shadows,
            ..default()
        },
        Transform::from_xyz(3.0, 6.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    if shadows {
        // Cascades cut for a studio, not a landscape: everything worth a
        // shadow is inside a few metres of the floor.
        sun.insert(
            bevy::light::CascadeShadowConfigBuilder {
                first_cascade_far_bound: 12.0,
                maximum_distance: 60.0,
                ..default()
            }
            .build(),
        );
    }
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

/// Half-extent (m) of the floor a `--room` sheet stands its structures on.
const ROOM_FLOOR: f32 = 80.0;
/// Half-extent (m) of the `--play-view` floor. The chase camera's pitch
/// leaves the top of the frame a fraction of a degree below the horizon, so
/// a short floor would show its own far edge as a line across the picture.
const PLAY_FLOOR: f32 = 600.0;

fn spawn_ground(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    half_extent: f32,
) {
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(half_extent)))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.30, 0.33, 0.27),
            perceptual_roughness: 0.95,
            ..default()
        })),
        Transform::default(),
        GroundPlane,
    ));
}

/// The drive loop: frame, warm up, shoot. See [`Phase`].
#[allow(clippy::too_many_arguments)]
pub(super) fn drive(
    mut commands: Commands,
    mut capture: ResMut<Capture>,
    mut clock: ResMut<Clock>,
    targets: Res<Targets>,
    job: Res<RenderJob>,
    subject: SubjectQuery,
    emitters: Query<&GlobalTransform, With<ParticleEmitterMarker>>,
    mut cams: Query<(&mut Transform, &TileCam)>,
    mut slots: PlaySlotQuery,
    walkers: Query<(&Transform, &Walker), Without<TileCam>>,
    terrain_ready: Option<Res<crate::terrain::SplatApplied>>,
    world: WorldReadiness,
    gate: ShutterGate,
    heightmap: Option<Res<FinishedHeightMap>>,
    record: Option<Res<LiveRoomRecord>>,
    mut exit: MessageWriter<AppExit>,
) {
    match capture.phase {
        Phase::Framing => {
            let framed = match &job.subject {
                Subject::World(_) => frame_world(&mut capture, &job, &world, heightmap.as_deref()),
                Subject::Terrain { view_m, .. } => frame_terrain(
                    &mut capture,
                    &job,
                    *view_m,
                    terrain_ready.is_some(),
                    &mut cams,
                ),
                _ => frame_subject(
                    &mut capture,
                    &job,
                    &targets,
                    &subject,
                    &emitters,
                    &mut cams,
                    &mut slots,
                ),
            };
            if framed {
                info!("framed after {} frames; warming up", capture.waited);
                capture.phase = Phase::Warmup { left: WARMUP };
                clock.run = true;
                commands.insert_resource(ClipTiming {
                    capture_start: clock.elapsed + WARMUP as f32 * clock.step,
                });
                // The rig camera goes to its shot pose NOW, as the sheet
                // cameras do, not on the capture frame (#1351). Until it
                // moves it sits on the spawn placeholder 3 m from the origin,
                // and whatever that placeholder cannot see - a palm's crown
                // 7 m up - is never drawn for this view even after the
                // camera moves: the foliage's wind material is attached while
                // the mesh is outside the view, and the renderer specializes
                // a mesh for a view when the mesh changes, not when the view
                // first sees it. Frame 0 re-aims anyway; this is the warm-up.
                if job.single_camera() {
                    let walker = lead_walker(&walkers);
                    aim_rig(
                        &capture,
                        &job,
                        &mut cams,
                        walker,
                        record.as_deref(),
                        heightmap.as_deref(),
                        0,
                    );
                }
            }
        }
        Phase::Warmup { left } => {
            match warmup_verdict(left, gate.bakes(), gate.compiling(), gate.setup_pending()) {
                Warmup::Counting => {
                    // Keep the rig on its shot pose through the warm-up, not
                    // just at its start (#1351): the walker spawns a frame
                    // after framing, and a body that is built while it is
                    // outside the view is not drawn for that view when the
                    // camera finally turns to it. Following it here is what
                    // puts its meshes in view as they are built.
                    if job.single_camera() {
                        let walker = lead_walker(&walkers);
                        aim_rig(
                            &capture,
                            &job,
                            &mut cams,
                            walker,
                            record.as_deref(),
                            heightmap.as_deref(),
                            0,
                        );
                    }
                    capture.phase = Phase::Warmup { left: left - 1 };
                    return;
                }
                Warmup::HeldForScript => {
                    // The script's setup steps count the clock's steps, so
                    // the clock keeps running while they play.
                    clock.run = true;
                    assert!(
                        capture.since.elapsed() < WORLD_BUDGET,
                        "the --editor-script setup steps never finished within {WORLD_BUDGET:?}"
                    );
                    if capture.last_log.elapsed() >= WORLD_LOG_EVERY {
                        capture.last_log = Instant::now();
                        info!("warm-up done; playing the script's setup steps");
                    }
                    return;
                }
                Warmup::HeldForScene { bakes, compiling } => {
                    // A texture is still baking - one a single subject
                    // dispatched at spawn, or one the world dispatched late -
                    // or a compile pass is running. Hold the clock with the
                    // shutter, so the walker's lead-in and the plumes stay
                    // where the count left them and the clip's timing is
                    // still exact.
                    clock.run = false;
                    assert!(
                        capture.since.elapsed() < WORLD_BUDGET,
                        "the scene never settled within {WORLD_BUDGET:?}: {bakes} procedural \
                         texture bake(s) airborne, compiling {compiling}"
                    );
                    if capture.last_log.elapsed() >= WORLD_LOG_EVERY {
                        capture.last_log = Instant::now();
                        let pass = if compiling { " and a compile pass" } else { "" };
                        info!(
                            "warm-up done; holding the shutter for {bakes} texture bake(s){pass}"
                        );
                    }
                    return;
                }
                Warmup::Ready => clock.run = true,
            }
            let walker = lead_walker(&walkers);
            if job.single_camera() {
                aim_rig(
                    &capture,
                    &job,
                    &mut cams,
                    walker,
                    record.as_deref(),
                    heightmap.as_deref(),
                    0,
                );
            }
            if job.frames > 1 {
                // Lockstep from here: the clock steps once per captured frame.
                clock.run = false;
                commands.insert_resource(ClipStarted);
                let e = commands.spawn(Readback::texture(targets.0[0].clone())).id();
                capture.phase = Phase::Clip {
                    next: 1,
                    pending: Some(e),
                    armed: false,
                };
            } else {
                capture.results = vec![None; targets.0.len()];
                for (i, target) in targets.0.iter().enumerate() {
                    let e = commands.spawn(Readback::texture(target.clone())).id();
                    capture.tile_of.insert(e, i);
                }
                capture.phase = Phase::Sheet;
            }
        }
        Phase::Sheet => {}
        Phase::Clip {
            next,
            pending: None,
            armed,
        } => match clip_step(next, job.frames, armed, gate.busy()) {
            ClipStep::Finish => {
                let result = finish_clip(&capture, &job);
                match result {
                    Ok(()) => exit.write(AppExit::Success),
                    Err(e) => {
                        error!("clip save failed: {e}");
                        exit.write(AppExit::error())
                    }
                };
            }
            ClipStep::Hold => {
                assert!(
                    capture.since.elapsed() < WORLD_BUDGET,
                    "frame {next}: the scene never settled within {WORLD_BUDGET:?} ({} texture \
                     bake(s) airborne, compiling {})",
                    gate.bakes(),
                    gate.compiling()
                );
                if capture.last_log.elapsed() >= WORLD_LOG_EVERY {
                    capture.last_log = Instant::now();
                    info!(
                        "frame {next}: holding the shutter while the scene catches up ({} texture \
                         bake(s), compiling {})",
                        gate.bakes(),
                        gate.compiling()
                    );
                }
            }
            ClipStep::Arm => {
                // The clock steps at the top of the next frame, and that
                // frame is the one shot.
                clock.once = true;
                capture.phase = Phase::Clip {
                    next,
                    pending: None,
                    armed: true,
                };
            }
            ClipStep::Shoot => {
                let walker = lead_walker(&walkers);
                aim_rig(
                    &capture,
                    &job,
                    &mut cams,
                    walker,
                    record.as_deref(),
                    heightmap.as_deref(),
                    next,
                );
                if let Some((transform, _)) = cams.iter().next() {
                    info!(
                        "frame {next}/{}: t={:.2}s camera ({:.1}, {:.1}, {:.1}) walker {}",
                        job.frames,
                        clock.elapsed,
                        transform.translation.x,
                        transform.translation.y,
                        transform.translation.z,
                        walker.map_or("none".to_string(), |(at, _)| format!(
                            "({:.1}, {:.1}, {:.1})",
                            at.x, at.y, at.z
                        )),
                    );
                }
                let e = commands.spawn(Readback::texture(targets.0[0].clone())).id();
                capture.phase = Phase::Clip {
                    next: next + 1,
                    pending: Some(e),
                    armed: false,
                };
            }
        },
        Phase::Clip {
            pending: Some(_), ..
        } => {}
    }
}

/// The body the rig follows: the first `--walker` seed, whichever order
/// the query hands the group back in (#1352).
fn lead_walker(walkers: &Query<(&Transform, &Walker), Without<TileCam>>) -> Option<(Vec3, Vec3)> {
    walkers
        .iter()
        .find(|(_, w)| w.is_lead())
        .map(|(t, w)| (t.translation, w.dir()))
}

/// `--world`: wait for the compile to settle, then fix the rig's focus.
fn frame_world(
    capture: &mut Capture,
    job: &RenderJob,
    world: &WorldReadiness,
    heightmap: Option<&FinishedHeightMap>,
) -> bool {
    if world.settled() {
        capture.waited += 1;
    } else {
        capture.waited = 0;
    }
    if capture.last_log.elapsed() >= WORLD_LOG_EVERY {
        capture.last_log = Instant::now();
        info!(
            "world: {} ({:.0} s, quiet {}/{})",
            world.status(),
            capture.since.elapsed().as_secs_f32(),
            capture.waited,
            WORLD_QUIET
        );
    }
    assert!(
        capture.since.elapsed() < WORLD_BUDGET,
        "world never settled within {WORLD_BUDGET:?}: {}",
        world.status()
    );
    if capture.waited < WORLD_QUIET {
        return false;
    }
    let Subject::World(spec) = &job.subject else {
        unreachable!("frame_world is only called for a world subject");
    };
    let focus = resolve_focus(job.rig.focus, Some(&spec.record), heightmap, None, None);
    capture.framing = Some(Framing {
        focus,
        auto_dist: 150.0,
        auto_elev: 28.0,
    });
    true
}

/// `--terrain` (#994) frames itself rather than auto-framing. Two reasons,
/// and both are about the render being an instrument: the subject's AABB is
/// the whole kilometre-wide heightmap, so auto-framing would answer a
/// question nobody asked, and a fixed camera is what makes two renders
/// - before a change and after it - comparable at all.
fn frame_terrain(
    capture: &mut Capture,
    job: &RenderJob,
    view_m: f32,
    ready: bool,
    cams: &mut Query<(&mut Transform, &TileCam)>,
) -> bool {
    if !ready {
        capture.waited += 1;
        assert!(
            capture.waited < TERRAIN_GRACE,
            "terrain never finished: the splat pass has not applied after {TERRAIN_GRACE} \
             frames - a heightmap or texture-bake job did not land"
        );
        return false;
    }
    for (mut transform, cam) in cams.iter_mut() {
        let a = ANGLES[cam.0 % ANGLES.len()].to_radians();
        // Grazing on purpose. A repeat reads worst along the ground,
        // where one tile's features line up with the next; a top-down
        // view flatters it.
        let pos = cam_offset(a, view_m, view_m, Some(job.elev.unwrap_or(9.0)));
        *transform = Transform::from_translation(pos).looking_at(Vec3::ZERO, Vec3::Y);
    }
    capture.framing = Some(Framing {
        focus: Vec3::ZERO,
        auto_dist: view_m,
        auto_elev: job.elev.unwrap_or(9.0),
    });
    true
}

/// Auto-frame the cameras on the subject's world AABB once it resolves
/// (Bevy computes mesh `Aabb`s a frame after spawn). A lineup frames each
/// slot's row on that slot's own centre but with one shared camera
/// distance, so relative subject size across rows stays honest. A clip
/// records the framing for the rig instead of placing the tile cameras.
fn frame_subject(
    capture: &mut Capture,
    job: &RenderJob,
    targets: &Targets,
    subject: &SubjectQuery,
    emitters: &Query<&GlobalTransform, With<ParticleEmitterMarker>>,
    cams: &mut Query<(&mut Transform, &TileCam)>,
    slots: &mut PlaySlotQuery,
) -> bool {
    capture.waited += 1;
    if job.play.is_some() {
        return frame_play_view(capture, job, subject, slots);
    }
    let rows = if job.single_camera() {
        1
    } else {
        targets.0.len() / ANGLES.len()
    };
    if rows == 1 {
        // A subject that never resolves an AABB - a grammar that errored
        // or derived to nothing - would otherwise spin here forever, so
        // fall back to a placeholder bound and capture the empty frame.
        let bounds = subject_bounds(subject, emitters)
            .or_else(|| (capture.waited > FRAME_GRACE).then_some((Vec3::Y * 0.5, 0.5)));
        let Some((center, radius)) = bounds else {
            return false;
        };
        let dist = radius / (FOV * 0.5).tan() * 1.2 + radius * 0.5;
        if job.single_camera() {
            capture.framing = Some(Framing {
                focus: center,
                auto_dist: dist,
                // The historic low orbit, as an angle: a `0.7 × radius`
                // rise at full distance.
                auto_elev: (radius * 0.7).atan2(dist).to_degrees(),
            });
            return true;
        }
        for (mut transform, cam) in cams.iter_mut() {
            let a = ANGLES[cam.0].to_radians();
            let pos = center + cam_offset(a, dist, radius, job.elev);
            *transform = Transform::from_translation(pos).looking_at(center, Vec3::Y);
        }
        return true;
    }
    let Some(slots) = lineup_bounds(subject, rows, capture.waited > FRAME_GRACE) else {
        return false;
    };
    let slots: Vec<(Vec3, f32)> = slots.into_iter().map(centre_radius).collect();
    let max_radius = slots.iter().map(|s| s.1).fold(0.1f32, f32::max);
    let dist = max_radius / (FOV * 0.5).tan() * 1.2 + max_radius * 0.5;
    for (mut transform, cam) in cams.iter_mut() {
        let center = slots[cam.0 / ANGLES.len()].0;
        let a = ANGLES[cam.0 % ANGLES.len()].to_radians();
        let pos = center + cam_offset(a, dist, max_radius, job.elev);
        *transform = Transform::from_translation(pos).looking_at(center, Vec3::Y);
    }
    true
}

/// Where each line-up slot sits on the play view's arc: the angle, about the
/// camera's vertical axis, that separates it from the slot on the lens axis.
///
/// Every subject the *same* distance from the camera, rather than strung
/// along a line through the middle one. A 14 m line of craft shot from 12 m
/// puts its outermost subject 16 % further away than its innermost, and a
/// view whose one claim is "this is the range the player sees it at" cannot
/// spend 16 % of that claim on the layout. On an arc the claim is exact and
/// the cost is only that the outer slots are seen slightly more from the
/// side - which, since each is also yawed by its own arc angle, is the
/// three-quarter view a chase camera gives anyway.
///
/// `radii` are the slots' horizontal bounding radii and `horiz` the camera's
/// horizontal leg; spacing is each pair's angular half-widths plus
/// [`PLAY_GAP`], and the whole fan is then centred on the lens axis.
fn play_arc(radii: &[f32], horiz: f32) -> Vec<f32> {
    let ang: Vec<f32> = radii.iter().map(|r| (r / horiz.max(1e-3)).atan()).collect();
    let mut theta = Vec::with_capacity(ang.len());
    let mut at = 0.0;
    for (i, a) in ang.iter().enumerate() {
        if i > 0 {
            at += (ang[i - 1] + a) * PLAY_GAP;
        }
        theta.push(at);
    }
    // Negated on the way out so slot 0 is the LEFTMOST subject, which is the
    // order the line-up was typed in and the order the tool prints.
    let mid = (theta.first().copied().unwrap_or(0.0) + theta.last().copied().unwrap_or(0.0)) * 0.5;
    theta.iter().map(|t| mid - t).collect()
}

/// `--play-view` (#1360): stand every slot where the game stands it, on the
/// arc of the chase camera's own distance, and hand the rig that distance and
/// pitch.
fn frame_play_view(
    capture: &mut Capture,
    job: &RenderJob,
    subject: &SubjectQuery,
    slots: &mut PlaySlotQuery,
) -> bool {
    let play = job
        .play
        .as_ref()
        .expect("frame_play_view needs a play view");
    let Some(bounds) = lineup_bounds(subject, play.ride.len(), capture.waited > FRAME_GRACE) else {
        return false;
    };
    // `pose_at` divides a *framed* distance by `--zoom` and leaves an
    // explicit `--dist` absolute, so the framing hands over the undivided
    // number and the placement below uses the one the camera will really be
    // at.
    let framed = job.rig.dist.map_or(super::rig::PLAY_DIST, |(d, _)| d);
    let dist = match job.rig.dist {
        Some(_) => framed,
        None => framed / job.rig.zoom.max(0.01),
    };
    let pitch = job
        .rig
        .elev
        .map_or(play_elev_deg(), |(e, _)| e)
        .to_radians();
    let horiz = dist * pitch.cos();
    let yaw = job.rig.yaw.to_radians();
    // The camera's ground position, given that the rig looks at the origin:
    // `pose_at` puts it `horiz` out along the yaw.
    let cam_xz = Vec3::new(horiz * yaw.sin(), 0.0, horiz * yaw.cos());
    let radii: Vec<f32> = bounds
        .iter()
        .map(|(min, max)| 0.5 * (max.x - min.x).hypot(max.z - min.z))
        .collect();
    let theta = play_arc(&radii, horiz);

    // Heights first: they decide where the camera looks, and the camera's
    // height in turn decides how far out each slot has to stand for its
    // ORIGIN to be exactly `dist` away - which is the game's own relation
    // (the chase camera orbits the chassis origin at `ORBIT_RADIUS`).
    let (mut low, mut high) = (f32::INFINITY, f32::NEG_INFINITY);
    let heights: Vec<f32> = bounds
        .iter()
        .zip(&play.ride)
        .map(|((min, _), ride)| ride.height().unwrap_or(-min.y))
        .collect();
    for ((min, max), &y) in bounds.iter().zip(&heights) {
        low = low.min(min.y + y);
        high = high.max(max.y + y);
    }
    let look_y = (low + high) * 0.5;
    let cam_y = dist * pitch.sin() + look_y;
    let toward_look = -Vec3::new(yaw.sin(), 0.0, yaw.cos());
    let mut placed = vec![Vec3::ZERO; bounds.len()];
    for (i, (&y, &t)) in heights.iter().zip(&theta).enumerate() {
        let leg = (dist * dist - (cam_y - y).powi(2))
            .max((0.1 * dist).powi(2))
            .sqrt();
        let ground = cam_xz + Quat::from_rotation_y(t) * (toward_look * leg);
        placed[i] = Vec3::new(ground.x, y, ground.z);
    }
    for (mut transform, slot) in slots.iter_mut() {
        let Some(&at) = placed.get(slot.0) else {
            continue;
        };
        *transform =
            Transform::from_translation(at).with_rotation(Quat::from_rotation_y(theta[slot.0]));
    }

    let spread = ((theta.last().copied().unwrap_or(0.0) - theta.first().copied().unwrap_or(0.0))
        .abs()
        + radii.first().copied().unwrap_or(0.0).atan2(horiz)
        + radii.last().copied().unwrap_or(0.0).atan2(horiz))
    .to_degrees();
    let frame_deg = half_fov_x(play.frame.0, play.frame.1).to_degrees() * 2.0;
    info!(
        "play view: {} m at {:.1} deg, {:.0} px/m on a {}x{} frame; {} subject(s) over \
         {spread:.1} deg of a {frame_deg:.1} deg frame",
        dist,
        pitch.to_degrees(),
        px_per_metre(play.frame.1, dist),
        play.frame.0,
        play.frame.1,
        bounds.len(),
    );
    if spread > frame_deg {
        warn!(
            "play view: the line-up spans {spread:.1} deg and the frame is {frame_deg:.1} deg \
             wide - the outer subjects are cropped. Render fewer of them, or accept a \
             wider frame at the same distance."
        );
    }
    let camera = Vec3::new(cam_xz.x, cam_y, cam_xz.z);
    for (i, at) in placed.iter().enumerate() {
        info!(
            "  slot {i}: origin {:.3} m up ({}), footprint radius {:.2} m, origin {:.2} m \
             from the camera",
            at.y,
            play.ride[i].why(),
            radii[i],
            (*at - camera).length(),
        );
    }
    capture.framing = Some(Framing {
        focus: Vec3::new(0.0, look_y, 0.0),
        auto_dist: framed,
        auto_elev: pitch.to_degrees(),
    });
    true
}

/// Put the rig camera at its pose for frame `frame`. `walker` is the
/// body's (position, heading) when one exists; a walker focus orbits it
/// with the yaw measured from behind, everything else orbits the framed
/// point.
fn aim_rig(
    capture: &Capture,
    job: &RenderJob,
    cams: &mut Query<(&mut Transform, &TileCam)>,
    walker: Option<(Vec3, Vec3)>,
    record: Option<&LiveRoomRecord>,
    heightmap: Option<&FinishedHeightMap>,
    frame: u32,
) {
    let Some(framing) = &capture.framing else {
        return;
    };
    let (focus, yaw_base) = match job.rig.focus {
        Focus::Walker => (
            resolve_focus(
                Focus::Walker,
                record.map(|r| &r.0),
                heightmap,
                walker.map(|(at, _)| at),
                Some(framing.focus),
            ),
            CameraRig::yaw_behind(walker.map_or(Vec3::NEG_Z, |(_, dir)| dir)),
        ),
        _ => (framing.focus, 0.0),
    };
    let (pos, look) = job.rig.pose_at(
        focus,
        yaw_base,
        progress(frame, job.frames),
        (framing.auto_dist, framing.auto_elev),
    );
    for (mut transform, _) in cams.iter_mut() {
        *transform = Transform::from_translation(pos).looking_at(look, Vec3::Y);
    }
}

/// Write the clip: the GIF, and the PNG frames beside it on request, both at
/// `--downscale`.
fn finish_clip(capture: &Capture, job: &RenderJob) -> Result<(), String> {
    let n = job.downscale.max(1);
    let (w, h) = (job.tile.0 / n, job.tile.1 / n);
    let shrunk: Vec<Vec<u8>>;
    let frames: &[Vec<u8>] = if n > 1 {
        shrunk = capture
            .frames
            .iter()
            .map(|frame| gif::downscale_rgba(frame, job.tile.0, job.tile.1, n))
            .collect::<Result<_, _>>()?;
        &shrunk
    } else {
        &capture.frames
    };
    let stem = job.out.trim_end_matches(".gif").trim_end_matches(".png");
    let gif_path = format!("{stem}.gif");
    if let Some(parent) = std::path::Path::new(&gif_path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if job.keep_frames {
        let dir = format!("{stem}-frames");
        gif::write_png_frames(&dir, w, h, frames)?;
        info!("wrote {} PNG frames under {dir}", frames.len());
    }
    gif::write_gif(&gif_path, w, h, frames, delay_cs(job.fps), job.dither)?;
    let bytes = std::fs::metadata(&gif_path).map(|m| m.len()).unwrap_or(0);
    info!(
        "wrote {gif_path} ({} frames, {w}×{h}, {} cs/frame, {:.1} MiB)",
        frames.len(),
        delay_cs(job.fps),
        bytes as f64 / (1024.0 * 1024.0)
    );
    Ok(())
}

/// Per-slot bounds of a lineup → one world-space (min, max) per row, slot
/// resolved from each mesh's world X (`round(x / SLOT_SPACING)`). Returns
/// `None` until every slot has at least one resolved AABB, unless `force` -
/// then still-empty slots get a tiny placeholder bound at their slot origin
/// so a degenerate variant can't hang the render.
///
/// The box rather than a centre and a radius, because `--play-view` needs the
/// *bottom* of a slot (to stand a subject that has no ride height on its own
/// feet) and its *footprint* (to space the line-up), and neither survives the
/// collapse to a bounding sphere.
fn lineup_bounds(q: &SubjectQuery, rows: usize, force: bool) -> Option<Vec<(Vec3, Vec3)>> {
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
            let at = Vec3::new(slot as f32 * SLOT_SPACING, 0.5, 0.0);
            slots.push((at - Vec3::splat(0.25), at + Vec3::splat(0.25)));
        } else {
            slots.push((min, max));
        }
    }
    Some(slots)
}

/// A slot's framing pair: centre, and the bounding-sphere radius the sheet
/// cameras fit to.
fn centre_radius((min, max): (Vec3, Vec3)) -> (Vec3, f32) {
    ((min + max) * 0.5, ((max - min) * 0.5).length().max(0.1))
}

/// Where a tile camera sits relative to the framed centre. `elev` (degrees,
/// from `--elev`) puts it on a true elevation arc; without it the camera
/// keeps the historic low orbit - a fixed `0.7 * radius` rise at full
/// distance, i.e. roughly 13° - which reads a facade well but cannot see
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
/// The ground plane is excluded by its [`GroundPlane`] marker (through
/// [`SubjectQuery`]) so a room frames on its buildings rather than its floor,
/// and live [`Particle`] quads are excluded so a drifting smoke plume can't
/// jitter the framing from run to run.
///
/// Emitter *anchors* are folded in as points instead. An FX-heavy prop -
/// a fire whose smoke column is authored 2 m above a 0.9 m barrel - is
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

/// A readback landed: a sheet tile, or the clip frame in flight.
pub(super) fn on_capture(
    trigger: On<ReadbackComplete>,
    mut commands: Commands,
    job: Res<RenderJob>,
    mut capture: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    let event = trigger.event();
    match capture.phase {
        Phase::Clip {
            next,
            pending: Some(e),
            ..
        } if e == event.entity => {
            capture.frames.push(event.data.clone());
            commands.entity(e).despawn();
            // The drive loop arms the next frame once the scene is quiet.
            capture.phase = Phase::Clip {
                next,
                pending: None,
                armed: false,
            };
        }
        Phase::Sheet => {
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
            let saved = shrink_still(&capture.results, &job)
                .and_then(|(results, tile)| save_contact_sheet(&results, tile, &job.out));
            match saved {
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
        _ => {}
    }
}

/// Captured tiles and the size each one is.
type Tiles = (Vec<Option<Vec<u8>>>, (u32, u32));

/// A single-camera still at `--downscale`: its one tile shrunk. A sheet of
/// tiles is written at full size.
fn shrink_still(results: &[Option<Vec<u8>>], job: &RenderJob) -> Result<Tiles, String> {
    let n = job.downscale;
    if n <= 1 || !job.single_camera() {
        return Ok((results.to_vec(), job.tile));
    }
    let (w, h) = job.tile;
    let shrunk = results
        .iter()
        .map(|tile| {
            tile.as_ref()
                .map(|data| gif::downscale_rgba(data, w, h, n))
                .transpose()
        })
        .collect::<Result<_, _>>()?;
    Ok((shrunk, (w / n, h / n)))
}

/// Tile the RGBA captures into one PNG: `ANGLES.len()` columns per row, one
/// row per lineup slot (a single subject is one row - the original horizontal
/// strip; a single camera is one tile).
fn save_contact_sheet(
    results: &[Option<Vec<u8>>],
    (tw, th): (u32, u32),
    path: &str,
) -> Result<(), String> {
    let (tw_us, th_us) = (tw as usize, th as usize);
    let cols = ANGLES.len().min(results.len()).max(1);
    let rows = results.len().div_ceil(cols);
    let sheet_w = tw * cols as u32;
    let stride = sheet_w as usize * 4;
    let mut sheet = vec![0u8; stride * th_us * rows];
    for (i, captured) in results.iter().enumerate() {
        let data = captured.as_ref().ok_or("missing tile")?;
        if data.len() < tw_us * th_us * 4 {
            return Err(format!("tile {i} short: {} bytes", data.len()));
        }
        let (row, col) = (i / cols, i % cols);
        for y in 0..th_us {
            let src = &data[y * tw_us * 4..(y + 1) * tw_us * 4];
            let dst = (row * th_us + y) * stride + col * tw_us * 4;
            sheet[dst..dst + tw_us * 4].copy_from_slice(src);
        }
    }
    std::fs::create_dir_all(OUT_DIR).map_err(|e| e.to_string())?;
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    image::save_buffer(
        path,
        &sheet,
        sheet_w,
        th * rows as u32,
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

fn new_target((width, height): (u32, u32)) -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width,
            height,
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    /// #1360. The line-up's ground plan: the order it was typed in, centred
    /// on the lens axis, and wide enough apart that no subject overlaps its
    /// neighbour.
    #[test]
    fn a_play_view_lineup_reads_left_to_right_and_is_centred_on_the_lens() {
        // Four subjects of very different sizes, at the chase camera's
        // horizontal leg.
        let horiz = super::super::rig::PLAY_DIST * play_elev_deg().to_radians().cos();
        let radii = [1.73, 1.54, 1.95, 0.31];
        let theta = play_arc(&radii, horiz);
        assert_eq!(theta.len(), radii.len());
        // Slot 0 leftmost: yaw decreases left to right in this convention
        // (yaw 180 looks along +Z, so a smaller yaw swings to the right of
        // the picture), so the arc angles run downward.
        for pair in theta.windows(2) {
            assert!(pair[0] > pair[1], "slots out of order: {theta:?}");
        }
        // Centred: the fan's two ends are equal and opposite.
        let ends = theta[0] + theta[theta.len() - 1];
        assert!(ends.abs() < 1e-5, "fan is off-axis by {ends} rad");
        // Neighbours clear each other - the gap between two slots' centres
        // exceeds the sum of their angular half-widths.
        for i in 0..radii.len() - 1 {
            let gap = (theta[i] - theta[i + 1]).abs();
            let want = (radii[i] / horiz).atan() + (radii[i + 1] / horiz).atan();
            assert!(gap > want, "slots {i}/{} overlap: {gap} < {want}", i + 1);
        }
        // One subject needs no fan at all.
        assert_eq!(play_arc(&[1.5], horiz), vec![0.0]);
        assert!(play_arc(&[], horiz).is_empty());
    }

    /// A slot with no ride height to read stands on its own feet; one with a
    /// height stands at it, whoever supplied it. The rule that keeps a
    /// hovering hull off the dirt and a mannequin's soles on it.
    #[test]
    fn a_rides_height_is_only_known_when_something_knew_it() {
        assert_eq!(Ride::Derived(0.83).height(), Some(0.83));
        assert_eq!(Ride::Told(0.35).height(), Some(0.35));
        assert_eq!(Ride::Bounds.height(), None);
        // The log says which, because "where the game puts it" and "where I
        // was told to put it" are different claims.
        assert_ne!(Ride::Derived(1.0).why(), Ride::Told(1.0).why());
    }

    /// The spawn-time placeholder every tile camera starts on.
    const PLACEHOLDER: Vec3 = Vec3::new(0.0, 1.0, 3.0);

    /// A clip's one camera leaves the placeholder the frame the subject is
    /// framed - the start of the warm-up - not the frame of the first
    /// capture (#1351). No GPU: the drive loop only needs the subject's
    /// bounds, which a bare `Aabb` entity supplies.
    #[test]
    fn a_clip_camera_is_aimed_when_the_subject_is_framed_not_when_shot() {
        let mut world = World::new();
        world.init_resource::<Capture>();
        world.init_resource::<bevy::ecs::message::Messages<AppExit>>();
        world.insert_resource(Clock {
            step: 0.1,
            run: false,
            once: false,
            stepped: false,
            elapsed: 0.0,
        });
        world.insert_resource(Targets(vec![Handle::default()]));
        let generator = crate::catalogue::by_slug("pagoda")
            .expect("the pagoda ships")
            .build("did:render:test");
        world.insert_resource(RenderJob {
            subject: Subject::Single(Box::new(generator)),
            play: None,
            out: String::new(),
            tile: (256, 256),
            elev: None,
            rig: CameraRig {
                focus: Focus::Subject,
                lift: 0.0,
                dist: None,
                elev: None,
                yaw: 180.0,
                sweep: 360.0,
                zoom: 1.0,
            },
            frames: 12,
            fps: 10.0,
            keep_frames: false,
            dither: 0.0,
            downscale: 1,
        });
        let cam = world
            .spawn((
                TileCam(0),
                Transform::from_translation(PLACEHOLDER).looking_at(Vec3::ZERO, Vec3::Y),
            ))
            .id();
        // A 10 m tall subject: its crown is well outside the placeholder's view.
        world.spawn((
            GlobalTransform::default(),
            Aabb::from_min_max(Vec3::new(-2.0, 0.0, -2.0), Vec3::new(2.0, 10.0, 2.0)),
        ));

        world
            .run_system_once(drive)
            .expect("the drive loop runs headless");

        let capture = world.resource::<Capture>();
        assert!(
            matches!(capture.phase, Phase::Warmup { left: WARMUP }),
            "one frame with bounds present frames the subject and enters warm-up"
        );
        let at = world.get::<Transform>(cam).unwrap().translation;
        assert!(
            (at - PLACEHOLDER).length() > 1.0,
            "the camera has left the placeholder at the start of warm-up: {at}"
        );
        // And it is on the rig: yaw 180 puts it on the -Z side of the
        // subject's centre, at the framed distance.
        let framing = capture.framing.as_ref().expect("framing recorded");
        assert!(at.z < framing.focus.z, "{at} vs {}", framing.focus);
        assert!(
            ((at - framing.focus).length() - framing.auto_dist).abs() < 1e-2,
            "{at} is not {} m from {}",
            framing.auto_dist,
            framing.focus
        );
    }

    #[test]
    fn the_count_runs_down_whatever_is_baking_and_only_the_shutter_waits() {
        // Frames left: the count proceeds even with bakes airborne or setup
        // steps unplayed, because the plumes need the frames either way.
        assert_eq!(warmup_verdict(3, 4, true, true), Warmup::Counting);
        assert_eq!(warmup_verdict(1, 0, false, false), Warmup::Counting);
        // Count done and setup steps left: they play first, on the clock.
        assert_eq!(warmup_verdict(0, 2, true, true), Warmup::HeldForScript);
        // Count done, bakes airborne or a compile running: held, and the log
        // knows which.
        assert_eq!(
            warmup_verdict(0, 2, false, false),
            Warmup::HeldForScene {
                bakes: 2,
                compiling: false
            }
        );
        assert_eq!(
            warmup_verdict(0, 0, true, false),
            Warmup::HeldForScene {
                bakes: 0,
                compiling: true
            }
        );
        // Count done, nothing pending: shoot.
        assert_eq!(warmup_verdict(0, 0, false, false), Warmup::Ready);
    }

    #[test]
    fn a_clip_frame_is_armed_only_on_a_quiet_scene_and_shot_on_the_next() {
        // Quiet: arm, then shoot on the next frame whatever the scene does in
        // between, so the clock step and the capture stay one frame apart.
        assert_eq!(clip_step(3, 10, false, false), ClipStep::Arm);
        assert_eq!(clip_step(3, 10, true, true), ClipStep::Shoot);
        // Busy and not armed: hold, however long it takes.
        assert_eq!(clip_step(3, 10, false, true), ClipStep::Hold);
        // Every frame shot: finish, busy or not.
        assert_eq!(clip_step(10, 10, false, true), ClipStep::Finish);
    }
}
