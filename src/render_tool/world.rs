//! `--world`: the whole seeded room, built by the game's own pipeline.
//!
//! Every other subject this tool draws is spawned by hand through the spawn
//! path; a world is *compiled*. The login backdrop (`crate::attract`) proved
//! the real terrain + world-builder pipeline runs without a player, a
//! session or a network - it needs a [`LiveRoomRecord`] and its gates
//! opened - so this mode registers exactly those pipelines outside the app:
//! the heightmap + splat chain, the road re-mesh and the lot layer, the
//! placement compile with its scatter sampler and water volumes, and the
//! sun / sky / cloud deck the room's `Environment` re-tints. What appears
//! is what the game builds for that seed, streets and grown district
//! included, under the game's own camera components (fog, bloom, prepass,
//! shadows).
//!
//! Readiness is a *settled* world, not a first compile: the lot layer
//! writes buildings into the record after the roads land, which triggers a
//! second compile pass, and the road extrusion runs on a background task.
//! [`WorldReadiness::settled`] asks all four questions at once, and the
//! drive loop additionally waits for the answer to hold for a run of
//! frames, so a debounce that has not fired yet cannot pass as done.
//!
//! `--walker <seed>` adds one rigged seeded body walking across that
//! world - from the gateway forecourt toward the spawn by default - driven
//! by the same `Drive` / `AvatarDriver` pair the game hangs a local player
//! on, so what the clip shows is the engine's own gait on the real ground.

use std::f32::consts::PI;

use bevy::camera::RenderTarget;
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::ecs::system::SystemParam;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy_symbios_avatar::{AvatarDriver, Drive, spawn_avatar};

use crate::camera::WorldCamera;
use crate::pds::RoomRecord;
use crate::pds::avatar::wardrobe::engine_default_for_seed;
use crate::pds::avatar::{AttachmentRecord, ResolvedAttachment};
use crate::state::{CurrentRoomDid, LiveRoomRecord, LocalSettings};
use crate::terrain::{FinishedHeightMap, RoadPanelStats, SplatApplied};
use crate::world_builder::WorldCompiled;
use crate::world_builder::compile::CompileJob;

use super::headless::{ClipTiming, Clock, PendingWear, TileCam};
use super::rig::Focus;

/// The world subject: the record the pipeline compiles, and the DID it is
/// filed under (what the room's own portal faces skip fetching).
pub(super) struct WorldSpec {
    pub(super) record: RoomRecord,
    pub(super) did: String,
}

/// `--walker`: one rigged body crossing the world.
#[derive(Resource, Clone, Debug)]
pub(super) struct WalkerSpec {
    /// The seed the body is rolled from - the same derivation a fresh
    /// account's default look takes.
    pub(super) seed: u64,
    /// Walking pace in metres per second.
    pub(super) pace: f32,
    /// Where the walk starts, `x,z`; the record's default landing when
    /// absent (or the origin's near side if the record has none).
    pub(super) from: Option<[f32; 2]>,
    /// What the walk heads toward, `x,z`; the room origin when absent. The
    /// body does not stop there - the line through both points is the path.
    pub(super) to: Option<[f32; 2]>,
    /// Catalogue wearables the body is dressed in, by slug.
    pub(super) wear: Vec<String>,
    /// Seconds of walking before the first captured frame, so a clip opens
    /// mid-stride rather than on the first step.
    pub(super) lead: f32,
}

/// The body's chassis: where the walk started, which way it goes, and
/// when it starts. The rigged root hangs under this entity exactly as the
/// game hangs one under a physics chassis.
#[derive(Component)]
pub(super) struct Walker {
    from: Vec3,
    dir: Vec3,
    pace: f32,
    /// Clock seconds at which the body starts moving.
    start: f32,
    /// The rigged root carrying the [`Drive`].
    root: Entity,
}

impl Walker {
    /// The body's horizontal heading, world space.
    pub(super) fn dir(&self) -> Vec3 {
        self.dir
    }
}

/// The four facts a settled world is made of.
///
/// Every field is optional because the drive loop takes this param in
/// every mode, and only `--world` registers the road pipeline; a missing
/// resource reads as "not settled", which is the truthful answer.
#[derive(SystemParam)]
pub(super) struct WorldReadiness<'w> {
    compiled: Option<Res<'w, WorldCompiled>>,
    job: Option<Res<'w, CompileJob>>,
    splat: Option<Res<'w, SplatApplied>>,
    roads: Option<Res<'w, RoadPanelStats>>,
}

impl WorldReadiness<'_> {
    /// At least one compile pass has landed, no pass is running, the
    /// ground shows its splat, and no road re-mesh or lot re-derive is
    /// armed or in flight.
    pub(super) fn settled(&self) -> bool {
        self.compiled.is_some()
            && self.job.as_ref().is_some_and(|j| j.progress().is_none())
            && self.splat.is_some()
            && self.roads.as_ref().is_some_and(|r| !r.pending)
    }

    /// One line for the progress log and the timeout panic.
    pub(super) fn status(&self) -> String {
        format!(
            "compiled={} compile_progress={:?} splat={} roads_pending={:?} streets={:?} buildings={:?}",
            self.compiled.is_some(),
            self.job.as_ref().and_then(|j| j.progress()),
            self.splat.is_some(),
            self.roads.as_ref().map(|r| r.pending),
            self.roads.as_ref().map(|r| r.streets),
            self.roads.as_ref().map(|r| r.buildings),
        )
    }
}

/// Register the game pipelines the world compile needs, on top of the
/// spawn-path resources every mode registers.
pub(super) fn register(app: &mut App, spec: &WorldSpec, walker: Option<WalkerSpec>) {
    crate::terrain::register_headless_terrain(app);
    crate::terrain::register_headless_roads(app);
    crate::world_builder::register_headless_compile(app);
    crate::register_headless_atmosphere(app);
    app.init_resource::<LocalSettings>()
        .init_resource::<crate::diagnostics::MetricsRegistry>()
        .insert_resource(LiveRoomRecord(spec.record.clone()))
        .insert_resource(CurrentRoomDid(spec.did.clone()));
    if let Some(walker) = walker {
        app.insert_resource(walker);
    }
}

/// The game camera, aimed at an off-screen target: the components
/// `camera::spawn_orbit_camera` gives the player's view, minus the orbit
/// controller, the egui context and the audio listener - none of which has
/// a reader here. Tile 0, so the drive loop steers it like any other.
pub(super) fn spawn_world_camera(commands: &mut Commands, target: Handle<Image>) {
    commands.spawn((
        Camera3d::default(),
        WorldCamera,
        RenderTarget::Image(target.into()),
        Msaa::Sample4,
        // The game's 12 km far plane: the cloud deck and the sky cuboid
        // must stay inside the frustum from a 150 m orbit.
        Projection::from(PerspectiveProjection {
            far: 12_000.0,
            ..default()
        }),
        // Shore foam reads the opaque prepass depth (see `camera.rs`).
        DepthPrepass,
        crate::camera::default_distance_fog(),
        Bloom::NATURAL,
        TileCam(0),
        Transform::from_xyz(0.0, 100.0, 150.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// Resolve a [`Focus`] to the world point the rig orbits.
///
/// Everything that names a place on the ground samples the heightmap;
/// without one (not yet landed, or a subject that has no terrain) the
/// ground is `y = 0`. A focus that names something absent - the walker
/// before it spawns, a record with no landing - falls back to the origin.
pub(super) fn resolve_focus(
    focus: Focus,
    record: Option<&RoomRecord>,
    heightmap: Option<&FinishedHeightMap>,
    walker: Option<Vec3>,
    subject: Option<Vec3>,
) -> Vec3 {
    let ground = |x: f32, z: f32| heightmap.map_or(0.0, |h| h.world_height_at(x, z));
    match focus {
        Focus::Origin => Vec3::new(0.0, ground(0.0, 0.0), 0.0),
        Focus::Landing => match record.and_then(|r| r.default_landing.as_ref()) {
            Some(landing) => {
                let [x, z] = landing.pos.0;
                Vec3::new(x, landing.y.map_or(ground(x, z), |y| y.0), z)
            }
            None => Vec3::new(0.0, ground(0.0, 0.0), 0.0),
        },
        Focus::Settlement => match record.and_then(settlement_centre) {
            Some(c) => Vec3::new(c.x, ground(c.x, c.y), c.y),
            None => Vec3::new(0.0, ground(0.0, 0.0), 0.0),
        },
        Focus::Walker => walker.unwrap_or_else(|| Vec3::new(0.0, ground(0.0, 0.0), 0.0)),
        Focus::Subject => subject.unwrap_or_else(|| Vec3::new(0.0, ground(0.0, 0.0), 0.0)),
        Focus::Point { x, y, z } => Vec3::new(x, y.unwrap_or_else(|| ground(x, z)), z),
    }
}

/// The ground-plane centroid of the record's `Absolute` placements - the
/// built-up band a world shot should centre on. `None` for a record that
/// places nothing absolutely.
fn settlement_centre(record: &RoomRecord) -> Option<Vec2> {
    let mut sum = Vec2::ZERO;
    let mut n = 0u32;
    for placement in &record.placements {
        if let crate::pds::Placement::Absolute { transform, .. } = placement {
            sum += Vec2::new(transform.translation.0[0], transform.translation.0[2]);
            n += 1;
        }
    }
    (n > 0).then(|| sum / n as f32)
}

/// Where the walk starts and which way it goes, on the ground plane.
fn walker_path(spec: &WalkerSpec, record: &RoomRecord) -> (Vec2, Vec2) {
    let from = spec
        .from
        .map(Vec2::from_array)
        .or_else(|| {
            record
                .default_landing
                .as_ref()
                .map(|l| Vec2::from_array(l.pos.0))
        })
        .unwrap_or(Vec2::new(0.0, 14.0));
    let to = spec.to.map(Vec2::from_array).unwrap_or(Vec2::ZERO);
    let dir = (to - from).normalize_or(Vec2::NEG_Y);
    (from, dir)
}

/// Spawn the walker once the world has settled (the clip timing is the
/// signal - the drive loop inserts it on the way into warm-up), so its
/// first steps are taken on finished ground and its lead-in ends exactly at
/// the first captured frame.
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_walker(
    mut commands: Commands,
    spec: Res<WalkerSpec>,
    timing: Res<ClipTiming>,
    record: Res<LiveRoomRecord>,
    heightmap: Res<FinishedHeightMap>,
    existing: Query<(), With<Walker>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>,
) {
    if !existing.is_empty() {
        return;
    }
    let (from2, dir2) = walker_path(&spec, &record.0);
    let from = Vec3::new(
        from2.x,
        heightmap.world_height_at(from2.x, from2.y),
        from2.y,
    );
    let dir = Vec3::new(dir2.x, 0.0, dir2.y);
    let avatar = symbios_avatar::Avatar::build_with(
        &engine_default_for_seed(spec.seed),
        &symbios_avatar::AvatarConfig::default(),
    )
    .unwrap_or_else(|| panic!("--walker {}: the seeded body did not build", spec.seed));

    let worn: Vec<ResolvedAttachment> = spec
        .wear
        .iter()
        .enumerate()
        .map(|(i, slug)| {
            let entry = crate::catalogue::by_slug(slug)
                .unwrap_or_else(|| panic!("--walker-wear {slug:?}: no catalogue entry"));
            let socket = entry.wear_socket().unwrap_or_else(|| {
                panic!("--walker-wear {slug:?}: entry is not wearable (no wear_socket())")
            });
            let mut record = AttachmentRecord::with_fit(
                entry.build("did:render:walker"),
                socket,
                entry.wear_fit(),
            );
            record.sanitize();
            ResolvedAttachment {
                rkey: format!("walker-{i}"),
                record,
            }
        })
        .collect();

    let chassis = commands
        .spawn((
            Transform::from_translation(from).looking_to(dir, Vec3::Y),
            Visibility::default(),
        ))
        .id();
    // The game's facing bridge (`player::rigged::rigged_root_transform`):
    // engine bodies face `+Z`, the chassis is aimed with `looking_to`, whose
    // `-Z` faces the travel, so the root turns half round between them.
    let mut root = commands.spawn((
        Transform::from_rotation(Quat::from_rotation_y(PI)),
        Visibility::default(),
        AvatarDriver::seeded(spec.seed),
        Drive::default(),
        ChildOf(chassis),
    ));
    if !worn.is_empty() {
        root.insert(PendingWear(worn));
    }
    let root = root.id();
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
    commands.entity(chassis).insert(Walker {
        from,
        dir,
        pace: spec.pace,
        start: timing.capture_start - spec.lead,
        root,
    });
    info!(
        "walker {}: from ({:.1}, {:.1}) heading ({:.2}, {:.2}) at {:.2} m/s, walking from t={:.2}s",
        spec.seed,
        from.x,
        from.z,
        dir.x,
        dir.z,
        spec.pace,
        timing.capture_start - spec.lead
    );
}

/// Move the walker along its line at the clock's pace and hand the driver
/// what the game's `fill_rigged_drive` would: a place, a velocity and the
/// chassis yaw carried through the root's half turn.
///
/// Runs before the avatar plugin's `Animate` set so the pose the frame
/// renders is the pose for this position. Height is the heightmap's - the
/// body walks the raw ground the way the game's capsule rides it, and only
/// the horizontal velocity is handed over: the signed vertical is the
/// engine's airborne state machine, and a slope is not a jump.
pub(super) fn step_walkers(
    clock: Res<Clock>,
    heightmap: Option<Res<FinishedHeightMap>>,
    mut walkers: Query<(&Walker, &mut Transform)>,
    mut drives: Query<&mut Drive>,
) {
    let Some(heightmap) = heightmap else {
        return;
    };
    for (walker, mut transform) in &mut walkers {
        let walked = (clock.elapsed - walker.start).max(0.0);
        let flat = walker.from + walker.dir * walker.pace * walked;
        let at = Vec3::new(flat.x, heightmap.world_height_at(flat.x, flat.z), flat.z);
        *transform = Transform::from_translation(at).looking_to(walker.dir, Vec3::Y);
        if let Ok(mut drive) = drives.get_mut(walker.root) {
            drive.velocity = if clock.elapsed > walker.start {
                walker.dir * walker.pace
            } else {
                Vec3::ZERO
            };
            drive.at = at;
            drive.facing = walker.dir.x.atan2(walker.dir.z);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> WalkerSpec {
        WalkerSpec {
            seed: 3,
            pace: 1.4,
            from: None,
            to: None,
            wear: Vec::new(),
            lead: 1.5,
        }
    }

    #[test]
    fn a_seeded_room_walk_runs_from_the_landing_toward_the_origin() {
        let record = RoomRecord::default_for_seed(3, "did:render:3");
        let landing = record
            .default_landing
            .as_ref()
            .expect("a seeded room has a landing");
        let (from, dir) = walker_path(&spec(), &record);
        assert_eq!(from, Vec2::from_array(landing.pos.0));
        // Heads at the origin: the direction is the landing's own bearing,
        // negated.
        let expect = (-from).normalize();
        assert!((dir - expect).length() < 1e-4, "{dir} vs {expect}");
    }

    #[test]
    fn explicit_endpoints_win_and_a_zero_length_path_still_has_a_heading() {
        let record = RoomRecord::default_for_seed(3, "did:render:3");
        let mut s = spec();
        s.from = Some([10.0, 10.0]);
        s.to = Some([10.0, 0.0]);
        let (from, dir) = walker_path(&s, &record);
        assert_eq!(from, Vec2::new(10.0, 10.0));
        assert!((dir - Vec2::new(0.0, -1.0)).length() < 1e-6);
        s.to = Some([10.0, 10.0]);
        let (_, dir) = walker_path(&s, &record);
        assert_eq!(dir, Vec2::NEG_Y);
    }

    #[test]
    fn the_settlement_focus_is_the_mean_of_the_absolute_placements() {
        let record = RoomRecord::default_for_seed(3, "did:render:3");
        let c = settlement_centre(&record).expect("a seeded room places structures");
        let at = resolve_focus(Focus::Settlement, Some(&record), None, None, None);
        assert_eq!((at.x, at.z), (c.x, c.y));
        // It is a mean of the structures, not the spawn: a seeded settlement
        // stands off the origin by construction.
        assert!(c.length() > 1.0, "{c}");
    }

    #[test]
    fn focus_keywords_resolve_against_the_record_and_fall_back_to_the_origin() {
        let record = RoomRecord::default_for_seed(3, "did:render:3");
        let landing = record.default_landing.as_ref().unwrap();
        let at = resolve_focus(Focus::Landing, Some(&record), None, None, None);
        assert_eq!((at.x, at.z), (landing.pos.0[0], landing.pos.0[1]));
        assert_eq!(at.y, 0.0, "no heightmap: the ground is y = 0");
        assert_eq!(
            resolve_focus(Focus::Walker, Some(&record), None, None, None),
            Vec3::ZERO
        );
        assert_eq!(
            resolve_focus(
                Focus::Walker,
                None,
                None,
                Some(Vec3::new(1.0, 2.0, 3.0)),
                None
            ),
            Vec3::new(1.0, 2.0, 3.0)
        );
        assert_eq!(
            resolve_focus(
                Focus::Point {
                    x: 5.0,
                    y: Some(7.0),
                    z: -5.0
                },
                None,
                None,
                None,
                None
            ),
            Vec3::new(5.0, 7.0, -5.0)
        );
    }
}
