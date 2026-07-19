//! Attract-mode world backdrop behind the login screen (#897).
//!
//! While the app sits in [`AppState::Login`], this plugin seeds a demo
//! [`LiveRoomRecord`] from a per-visit random DID string and lets the
//! *real* terrain + world-builder pipeline compile it, so the login
//! screen orbits a genuine seeded overland — terrain, splat textures,
//! water, settlements, roads — instead of a flat backdrop. The login
//! UI keeps painting its sky-gradient fallback until the terrain mesh
//! lands (and forever, when the [`LocalSettings::login_world_backdrop`]
//! toggle is off).
//!
//! ## How it plugs into the pipeline
//!
//! The terrain and world-builder system groups are gated on
//! [`world_pipeline_active`] — historically `not(in_state(Login))` —
//! which the [`AttractScene`] marker resource widens into `Login`. The
//! compile arm additionally ORs on [`AttractScene`] (there is no
//! loading screen to protect in `Login`, so `WorldCompileArmed`'s
//! one-frame delay is irrelevant here). Everything else (avatar spawn,
//! ambient audio bake, networking, editor) stays gated on its own
//! session resources and states and never runs for the demo world.
//!
//! ## Teardown
//!
//! `OnExit(Login)` — whether into `Loading` after a successful auth or
//! never (process exit) — [`end_attract_scene`] despawns every
//! [`RoomEntity`](crate::world_builder::RoomEntity), drops the demo
//! record and the `WorldCompiled` / `WorldCompileArmed` markers (a
//! stale `WorldCompiled` would let the loading gate unveil an
//! uncompiled world), and hands the camera back at gameplay framing.
//! [`crate::terrain`] registers its own `cleanup_terrain` on the same
//! transition (gated on [`AttractScene`]), so heightmap, splat and
//! road state reset exactly like the #849 abort path. The session
//! resources the login flow inserted (`RelayHost`, the OAuth session)
//! are deliberately untouched — this is *not* a logout.
//!
//! ## Camera
//!
//! [`drive_attract_camera`] steers the existing `PanOrbitCamera`
//! through its `target_*` fields, so the crate's own smoothing does
//! the easing. Input stays enabled: a user who right-drags merely
//! deflects the orbit for a moment — pitch and radius spring back and
//! the yaw keeps drifting.

use bevy::prelude::*;
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraSystemSet};

use crate::pds::RoomRecord;
use crate::state::{AppState, LiveRoomRecord, LocalSettings};
use crate::terrain::FinishedHeightMap;
use crate::ui::login::{BeginAuthTask, CompleteAuthTask, LoginUiLatch};

/// Marker resource: the world currently compiled from [`LiveRoomRecord`]
/// is the login screen's demo world, not a session's room. Its presence
/// widens the world pipeline gates into `Login` (see
/// [`world_pipeline_active`]) and arms the `OnExit(Login)` teardown.
#[derive(Resource)]
pub struct AttractScene;

/// Run condition for the terrain / world-builder system groups: the
/// historical `not(in_state(Login))` gate, widened to also pass while
/// the attract backdrop is active.
pub fn world_pipeline_active(
    state: Res<State<AppState>>,
    attract: Option<Res<AttractScene>>,
) -> bool {
    *state.get() != AppState::Login || attract.is_some()
}

/// Seed the demo world once the idle login screen settles. Runs every
/// `Update` frame in `Login` so it also catches the states the
/// `OnEnter` moment can't: a logout landing here after teardown, or a
/// cancelled wasm session-resume freeing the screen up.
///
/// The guards all mean "this Login state is (or may be) transient":
/// burning a multi-second world build behind a screen that is about to
/// redirect would slow the *real* login down for nothing.
#[allow(clippy::too_many_arguments)]
pub fn start_attract_scene(
    mut commands: Commands,
    settings: Res<LocalSettings>,
    attract: Option<Res<AttractScene>>,
    record: Option<Res<LiveRoomRecord>>,
    begin_tasks: Query<(), With<BeginAuthTask>>,
    complete_tasks: Query<(), With<CompleteAuthTask>>,
    boot: Option<Res<crate::boot_params::BootParams>>,
    latch: Res<LoginUiLatch>,
    #[cfg(not(target_arch = "wasm32"))] native_wait: Option<
        Res<crate::oauth::NativeCallbackReceiver>,
    >,
) {
    if !settings.login_world_backdrop || attract.is_some() || record.is_some() {
        return;
    }
    if !begin_tasks.is_empty() || !complete_tasks.is_empty() {
        return;
    }
    // An armed autosubmit deep link fires on the first idle frame; only
    // once it has fired (and possibly failed back to the form) is the
    // screen genuinely idle.
    if let Some(boot) = boot.as_deref()
        && boot.autosubmit
        && !latch.autosubmitted
    {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    if native_wait.is_some() {
        return;
    }
    // A persisted wasm session resumes on its own; the marker clears if
    // the user backs out via "Not you?", and the attract world starts
    // on the next frame.
    #[cfg(target_arch = "wasm32")]
    if crate::oauth::wasm::load_persisted().is_some() {
        return;
    }

    // A different world every visit: the wall clock (chrono is
    // wasm-safe, unlike `std::time`) hashed through the same
    // `default_for_did` path a first-visit room takes, so the demo
    // showcases exactly what a fresh user would get.
    let demo_did = format!("did:attract:{:x}", chrono::Utc::now().timestamp_millis());
    info!("Attract backdrop: seeding demo world from {demo_did}");
    commands.insert_resource(LiveRoomRecord(RoomRecord::default_for_did(&demo_did)));
    commands.insert_resource(AttractScene);
}

/// Slow cinematic orbit around the demo world's centre. Writes only the
/// `PanOrbitCamera` `target_*` fields — the crate's smoothing turns the
/// per-frame nudges into an even glide, and any user input deflection
/// eases back on its own.
pub fn drive_attract_camera(
    time: Res<Time>,
    heightmap: Option<Res<FinishedHeightMap>>,
    mut cams: Query<&mut PanOrbitCamera>,
) {
    use crate::config::attract as cfg;
    let focus_h = heightmap
        .as_ref()
        .map(|h| h.world_height_at(0.0, 0.0))
        .unwrap_or(0.0);
    for mut cam in cams.iter_mut() {
        cam.target_focus = Vec3::new(0.0, focus_h + cfg::FOCUS_LIFT, 0.0);
        cam.target_radius = cfg::ORBIT_RADIUS;
        cam.target_pitch = cfg::ORBIT_PITCH;
        cam.target_yaw += cfg::YAW_RATE * time.delta_secs();
    }
}

/// Tear the demo world down when `Login` is left. See the module docs
/// for the split of responsibilities with `terrain::cleanup_terrain`
/// (registered on the same transition by [`crate::terrain`]).
pub fn end_attract_scene(
    mut commands: Commands,
    attract: Option<Res<AttractScene>>,
    room_entities: Query<Entity, With<crate::world_builder::RoomEntity>>,
    mut cams: Query<&mut PanOrbitCamera>,
) {
    if attract.is_none() {
        return;
    }
    for e in &room_entities {
        commands.entity(e).despawn();
    }
    commands.remove_resource::<LiveRoomRecord>();
    commands.remove_resource::<crate::world_builder::WorldCompiled>();
    commands.remove_resource::<crate::world_builder::WorldCompileArmed>();
    commands.remove_resource::<AttractScene>();
    // Hand the camera back at gameplay framing so the first `InGame`
    // frame doesn't start from a 150 m bird's eye; `follow_local_player`
    // takes the focus over from there.
    for mut cam in cams.iter_mut() {
        cam.target_focus = Vec3::ZERO;
        cam.target_radius = crate::config::camera::ORBIT_RADIUS;
        cam.target_pitch = crate::config::camera::ORBIT_PITCH;
    }
}

pub struct AttractPlugin;

impl Plugin for AttractPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            start_attract_scene.run_if(in_state(AppState::Login)),
        )
        .add_systems(
            PostUpdate,
            drive_attract_camera
                .before(PanOrbitCameraSystemSet)
                .run_if(in_state(AppState::Login).and(resource_exists::<AttractScene>)),
        )
        .add_systems(OnExit(AppState::Login), end_attract_scene);
    }
}
