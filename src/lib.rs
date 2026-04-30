//! Symbios Overlands — library crate.
//!
//! This is the home for every gameplay module. The companion binary in
//! `src/main.rs` is a thin shim that just calls [`run`]; having a library
//! target lets integration tests in `tests/` import the public API directly.
//!
//! [`run`] wires every gameplay plugin, initialises the shared ECS resources,
//! and coordinates the three-stage state machine (`Login` → `Loading` →
//! `InGame`). The loading gate explicitly waits on **all four** loading
//! tasks — heightmap generation, the ATProto PDS room-record fetch, the
//! avatar-record fetch, *and* the inventory-record fetch — before entering
//! `InGame`, so slower PDS round-trips cannot be silently dropped and
//! gameplay never runs with half-loaded recipes.

use avian3d::PhysicsPlugins;
use bevy::light::{CascadeShadowConfigBuilder, GlobalAmbientLight, NotShadowCaster};
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};

pub mod avatar;
pub mod boot_params;
pub mod camera;
pub mod clouds;
pub mod config;
pub mod editor_gizmo;
pub mod logout;
pub mod network;
pub mod oauth;
pub mod pds;
pub mod player;
pub mod protocol;
pub mod social;
pub mod splat;
pub mod state;
pub mod terrain;
pub mod ui;
pub mod water;
pub mod world_builder;

use pds::{AvatarRecord, RoomRecord};

/// Marker for the unlit sky cuboid spawned in `setup_lighting`. The world
/// compiler uses this to retint the sky material when a room record's
/// `environment.sky_color` changes.
#[derive(Component)]
pub struct SkyBox;

/// Pin the sky cuboid to the active camera each frame so its faces are
/// always equidistant from the viewer.
///
/// The cuboid is sized for a "looks-far-enough" backdrop (4 km × 2 km × 2 km
/// at the default `SKY_SCALE`), but it is fixed-size, not infinite. Without
/// this follow it stays anchored at world origin, so once the player moves
/// off-centre the closest face approaches the camera and its edges show up
/// as hard seams against the more-distant adjacent faces. Pinning the
/// cuboid's centre to the camera keeps every face at a constant distance
/// (half the cuboid's side on that axis) regardless of where the player
/// roams, which is what a backdrop should do anyway.
fn track_skybox_to_camera(
    camera: Query<&GlobalTransform, (With<Camera3d>, Without<SkyBox>)>,
    mut skybox: Query<&mut Transform, With<SkyBox>>,
) {
    let Ok(cam_tx) = camera.single() else {
        return;
    };
    let cam = cam_tx.translation();
    for mut transform in skybox.iter_mut() {
        transform.translation = cam;
    }
}

pub use clouds::CloudLayer;

/// Format elapsed seconds as a `MM:SS` (or `H:MM:SS`) timestamp string.
pub fn format_elapsed_ts(elapsed_secs: f64) -> String {
    let total = elapsed_secs as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}
use state::{
    AppState, ChatHistory, CurrentRoomDid, DiagnosticsLog, InventoryPublishFeedback,
    LiveAvatarRecord, LiveInventoryRecord, LocalSettings, PublishFeedback, RoomRecordRecovery,
    StoredAvatarRecord, StoredInventoryRecord, StoredRoomRecord,
};

/// Build the Bevy `App`, register every plugin, and block on
/// `App::run()`. Pulled out of `main.rs` so integration tests in `tests/`
/// can import the library's module tree without pulling in the binary's
/// execution entry point.
pub fn run() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    // Pulled before `App::new()` so the native `clap::Parser::parse()` can
    // emit `--help` / `--version` and exit cleanly without bringing up a
    // Bevy window first. WASM reads from the URL bar — no I/O risk.
    let boot = boot_params::detect();

    let fc = config::camera::fog::COLOR;
    App::new()
        .insert_resource(ClearColor(Color::srgba(fc[0], fc[1], fc[2], fc[3])))
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Symbios Overlands".into(),
                        prevent_default_event_handling: false,
                        ..default()
                    }),
                    ..default()
                })
                // `webrtc_ice::agent::agent_internal` emits a `WARN` every
                // ~200ms during ICE bring-up whenever the agent has zero
                // candidate pairs ("pingAllCandidates called with no
                // candidate pairs"). This is expected behaviour — candidate
                // gathering + signalling of the remote side takes several
                // seconds, and the agent keeps retrying the pairing loop in
                // the meantime. Demote the whole agent_internal module to
                // `error` so the handshake log stays readable; genuine ICE
                // failures still surface via the `webrtc_ice` module's other
                // error-level events.
                .set(LogPlugin {
                    filter: format!(
                        "{},webrtc_ice::agent::agent_internal=error",
                        bevy::log::DEFAULT_FILTER
                    ),
                    ..default()
                }),
        )
        .add_plugins(EguiPlugin::default())
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(transform_gizmo_bevy::TransformGizmoPlugin)
        .add_plugins(MaterialPlugin::<clouds::CloudMaterial>::default())
        .add_plugins(terrain::TerrainPlugin)
        .add_plugins(world_builder::WorldBuilderPlugin)
        .add_plugins(player::PlayerPlugin)
        .add_plugins(camera::CameraPlugin)
        .add_plugins(network::NetworkPlugin)
        .add_plugins(avatar::AvatarPlugin)
        .add_plugins(social::SocialPlugin)
        .add_plugins(logout::LogoutPlugin)
        .add_plugins(editor_gizmo::EditorGizmoPlugin)
        .init_state::<AppState>()
        .init_resource::<ChatHistory>()
        .init_resource::<DiagnosticsLog>()
        .init_resource::<LocalSettings>()
        .init_resource::<PublishFeedback>()
        .init_resource::<InventoryPublishFeedback>()
        .init_resource::<ui::inventory::PendingGeneratorDrop>()
        .init_resource::<state::PendingOutgoingOffers>()
        .init_resource::<ui::login::LoginError>()
        .init_resource::<ui::room::RoomEditorState>()
        .init_resource::<ui::avatar::AvatarEditorState>()
        .init_resource::<editor_gizmo::GizmoFramePref>()
        .init_resource::<oauth::OauthClientRes>()
        .insert_resource(boot)
        .add_systems(
            EguiPrimaryContextPass,
            ui::login::login_ui.run_if(in_state(AppState::Login)),
        )
        .add_systems(
            Update,
            (
                ui::login::poll_begin_auth_task,
                ui::login::poll_complete_auth_task,
                #[cfg(target_arch = "wasm32")]
                ui::login::check_wasm_callback,
                #[cfg(target_arch = "wasm32")]
                ui::login::check_wasm_resume,
                #[cfg(target_arch = "wasm32")]
                ui::login::poll_resume_task,
                #[cfg(not(target_arch = "wasm32"))]
                ui::login::poll_native_callback,
            )
                .run_if(in_state(AppState::Login)),
        )
        .add_systems(
            OnEnter(AppState::Loading),
            (
                start_room_record_fetch,
                start_avatar_record_fetch,
                start_inventory_record_fetch,
            ),
        )
        .add_systems(
            Update,
            (
                poll_room_record_task,
                poll_avatar_record_task,
                poll_inventory_record_task,
                fire_pending_record_retries,
                check_loading_complete,
            )
                .chain()
                .run_if(in_state(AppState::Loading)),
        )
        .add_systems(
            EguiPrimaryContextPass,
            loading_ui.run_if(in_state(AppState::Loading)),
        )
        .add_systems(
            EguiPrimaryContextPass,
            (
                ui::diagnostics::diagnostics_ui,
                ui::chat::chat_ui,
                ui::people::people_ui,
                ui::people::incoming_offer_ui,
                ui::avatar::avatar_ui,
                ui::room::room_admin_ui,
                ui::inventory::inventory_ui,
            )
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            Update,
            (
                ui::room::poll_publish_tasks,
                ui::avatar::poll_publish_avatar_tasks,
                ui::inventory::poll_publish_inventory_tasks,
                ui::inventory::handle_generator_drop,
            )
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(Startup, setup_lighting)
        .add_systems(
            Update,
            (clouds::track_cloud_layer_to_camera, track_skybox_to_camera),
        )
        .run();
}

fn setup_lighting(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cloud_materials: ResMut<Assets<clouds::CloudMaterial>>,
) {
    let lp = config::lighting::LIGHT_POS;
    // Start with the config default; `world_builder::compile_room_record`
    // patches the light with the `environment.sun_color` from the active
    // `RoomRecord` as soon as the recipe is compiled on InGame entry.
    let sc = config::lighting::SUN_COLOR;

    let cascade_shadow_config = CascadeShadowConfigBuilder {
        first_cascade_far_bound: config::lighting::CASCADE_FIRST_FAR,
        maximum_distance: config::lighting::CASCADE_MAX_DIST,
        ..default()
    }
    .build();

    commands.spawn((
        DirectionalLight {
            color: Color::srgb(sc[0], sc[1], sc[2]),
            shadows_enabled: true,
            illuminance: config::lighting::ILLUMINANCE,
            ..default()
        },
        Transform::from_xyz(lp[0], lp[1], lp[2]).looking_at(Vec3::ZERO, Vec3::Y),
        cascade_shadow_config,
    ));

    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: config::lighting::AMBIENT_BRIGHTNESS,
        ..default()
    });

    // Sky — large unlit cuboid tinted by the distance fog.
    let sky_c = config::lighting::SKY_COLOR;
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(2.0, 1.0, 2.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(sky_c[0], sky_c[1], sky_c[2]),
            unlit: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_scale(Vec3::splat(config::lighting::SKY_SCALE)),
        NotShadowCaster,
        SkyBox,
    ));

    // Cloud-deck — single horizontal `Plane3d` rendered through a custom
    // `MaterialExtension` over `StandardMaterial`. The mesh is large enough
    // (PLANE_HALF_EXTENT, default 4 km) that the plane edge sits well past
    // any reasonable `fog_visibility`, so the shader's horizon fade is the
    // only thing the camera ever sees at the plane boundary. Uniforms are
    // initialised from `Environment::default()` and re-patched by
    // `world_builder::compile::apply_environment_state` whenever the active
    // `RoomRecord` changes — same retint pattern as the `SkyBox` cuboid.
    let cc = config::lighting::clouds::COLOR;
    let csh = config::lighting::clouds::SHADOW_COLOR;
    let fc = config::camera::fog::COLOR;
    // Initial sun direction matches the directional light spawned above:
    // the light looks from `LIGHT_POS` toward the origin, so the unit
    // vector *toward* the sun is `normalize(LIGHT_POS)`. The world
    // compiler will refresh this each change tick from the live transform.
    let sun_v = Vec3::from_array(config::lighting::LIGHT_POS).normalize_or(Vec3::Y);
    let cloud_mat = cloud_materials.add(clouds::CloudMaterial {
        base: StandardMaterial {
            // The fragment shader replaces all colour math, so the base
            // colour is only used by fallback paths (e.g. shadow caster,
            // never wired here because shadows + prepass are disabled).
            base_color: Color::srgba(cc[0], cc[1], cc[2], 1.0),
            unlit: true,
            // Cull neither side — the underside is what the player sees
            // from below the deck, the topside is what they'd see if they
            // climbed above it on a tall airship.
            cull_mode: None,
            alpha_mode: AlphaMode::Blend,
            ..default()
        },
        extension: clouds::CloudExtension {
            uniforms: clouds::CloudUniforms {
                color: Vec4::new(cc[0], cc[1], cc[2], 1.0),
                shadow_color: Vec4::new(csh[0], csh[1], csh[2], 1.0),
                fog_color: Vec4::new(fc[0], fc[1], fc[2], fc[3]),
                sun_dir: Vec4::new(sun_v.x, sun_v.y, sun_v.z, 0.0),
                wind_dir: Vec2::from_array(config::lighting::clouds::WIND_DIR),
                cover: config::lighting::clouds::COVER,
                density: config::lighting::clouds::DENSITY,
                softness: config::lighting::clouds::SOFTNESS,
                speed: config::lighting::clouds::SPEED,
                scale: config::lighting::clouds::SCALE,
            },
        },
    });
    let half = config::lighting::clouds::PLANE_HALF_EXTENT;
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(half * 2.0, half * 2.0))),
        MeshMaterial3d(cloud_mat),
        Transform::from_xyz(0.0, config::lighting::clouds::HEIGHT, 0.0),
        NotShadowCaster,
        CloudLayer,
    ));
}

fn loading_ui(mut contexts: bevy_egui::EguiContexts) {
    bevy_egui::egui::CentralPanel::default().show(contexts.ctx_mut().unwrap(), |ui| {
        ui.centered_and_justified(|ui| {
            ui.heading("Generating the overlands…");
            ui.spinner();
        });
    });
}

// ---------------------------------------------------------------------------
// Room record loading (runs during AppState::Loading)
// ---------------------------------------------------------------------------

/// In-flight `fetch_room_record` task attached to a throwaway entity so the
/// `Loading` poll system can drain it without a dedicated resource.
///
/// The task result preserves the distinction between *no record* (404) and
/// *couldn't reach the PDS*, so the poll system only falls through to the
/// default homeworld on the former. Falling through on a transient network
/// failure is catastrophic: the owner would silently be staged on the blank
/// default, and a "Publish to PDS" click would overwrite their real
/// room with the default.
#[derive(Component)]
struct RoomRecordTask {
    task: bevy::tasks::Task<Result<Option<RoomRecord>, pds::FetchError>>,
    /// Zero for the initial fetch; incremented on each transient-failure
    /// respawn so `spawn_room_record_fetch` can pick a backoff delay.
    attempt: u32,
}

/// Exponential backoff for transient `fetch_room_record` failures. Without
/// a delay, a DNS error or immediate `ConnRefused` returns so fast that
/// the retry runs in the same or next frame, producing a busy loop that
/// burns a full CPU core and floods the log with warnings. Doubling from
/// 1 s up to a 60 s ceiling yields ~a minute-of-retries over six
/// attempts while still converging quickly when the PDS recovers.
fn room_record_backoff_secs(attempt: u32) -> u64 {
    if attempt == 0 {
        0
    } else {
        (1u64 << attempt.min(6)).min(60)
    }
}

/// Hard cap on record-fetch retries. The backoff saturates at 60 s after
/// six attempts, so twelve attempts buys roughly ten minutes of real-time
/// retrying against a flaky PDS — past that, persistent failure is
/// overwhelmingly more likely than a transient hiccup. Without this cap,
/// a misbehaving endpoint would spin the IoTaskPool indefinitely; on
/// `wasm32` it would also pile up an unbounded sequence of setTimeout
/// futures waiting in the browser event loop.
const MAX_RECORD_FETCH_ATTEMPTS: u32 = 12;

/// In-flight retry timer for a record fetch. The previous design parked
/// the backoff sleep *inside* the spawned `IoTaskPool` task — but
/// `tokio::time::sleep` awaited inside `block_on(fut)` holds the
/// underlying OS thread idle for the duration of the sleep, because
/// `block_on` dedicates one pool thread per task tree. Several flaky
/// fetches in retry simultaneously would saturate `IoTaskPool` (whose
/// thread count is small) and stall every other I/O job in the engine.
///
/// The fix is to defer the retry on Bevy's frame loop instead: when the
/// poll system decides to retry, it spawns one of these markers; the
/// `fire_pending_record_retries` system below watches `Time` and only
/// then dispatches the actual `IoTaskPool` task. The sleeping period
/// occupies a tiny ECS entity rather than a precious worker thread.
#[derive(Component)]
struct PendingRoomRecordRetry {
    did: String,
    attempt: u32,
    fire_at_secs: f64,
}

#[derive(Component)]
struct PendingAvatarRecordRetry {
    did: String,
    attempt: u32,
    fire_at_secs: f64,
}

fn spawn_room_record_fetch(commands: &mut Commands, did: String, attempt: u32) {
    // `IoTaskPool` is the correct home for blocking HTTP calls — the
    // `AsyncComputeTaskPool` is sized to the CPU-core count and must not be
    // starved by threads blocked on network sockets.
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            let client = config::http::default_client();
            pds::fetch_room_record(&client, &did).await
        };
        #[cfg(target_arch = "wasm32")]
        {
            fut.await
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(fut)
        }
    });
    commands.spawn(RoomRecordTask { task, attempt });
}

/// Kick off the async ATProto `getRecord` fetch for the room the client is
/// visiting. Runs exactly once on entry to `AppState::Loading`; the result is
/// picked up by `poll_room_record_task` on subsequent frames.
fn start_room_record_fetch(mut commands: Commands, room_did: Res<CurrentRoomDid>) {
    spawn_room_record_fetch(&mut commands, room_did.0.clone(), 0);
}

/// Drain a finished `RoomRecordTask`, install the resulting `RoomRecord` as a
/// Bevy resource, and synthesise the default recipe if the owner has never
/// published one (a 404 is not an error — it means a blank homeworld).
///
/// A non-404 failure (DNS timeout, 5xx, garbled JSON) retries the fetch
/// instead of substituting the default. This matters because the owner's
/// editor workflow is "load record → edit → publish": if we installed the
/// default on a transient error, a save-and-publish click would silently
/// clobber the owner's real room with the blank default.
fn poll_room_record_task(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut RoomRecordTask)>,
    room_did: Res<CurrentRoomDid>,
    mut diagnostics: ResMut<DiagnosticsLog>,
    time: Res<Time>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        let prev_attempt = task.attempt;

        commands.entity(entity).despawn();

        let mut record = match result {
            Ok(Some(r)) => r,
            Ok(None) => {
                // Zero-configuration homeworld: a 404 from the PDS means the
                // owner has not customised their overland yet, so we
                // synthesise the canonical default recipe keyed to their DID.
                info!("No room record on PDS — using default homeworld");
                pds::RoomRecord::default_for_did(&room_did.0)
            }
            Err(pds::FetchError::Decode(msg)) => {
                // A decode failure is *not* transient: the stored record
                // exists but is incompatible with the current schema (e.g.
                // lexicon drift, partially-migrated field). Retrying will
                // never recover — the loading screen would hang forever and
                // spam the diagnostics log. Fall through to the default
                // homeworld so the session progresses, and surface a
                // `RoomRecordRecovery` marker so the world editor can show
                // the owner a "Reset PDS to default" affordance.
                let elapsed = time.elapsed_secs_f64();
                diagnostics.push(
                    elapsed,
                    format!("Stored room record incompatible ({msg}) — falling back to default"),
                );
                warn!(
                    "Stored room record could not be decoded ({}) — using default and entering recovery mode",
                    msg
                );
                commands.insert_resource(RoomRecordRecovery { reason: msg });
                pds::RoomRecord::default_for_did(&room_did.0)
            }
            Err(err) => {
                // Transient failure (DNS timeout, 5xx, DID resolution hiccup):
                // do NOT substitute the default. Log it, re-queue the fetch
                // with an exponential backoff, and keep the Loading state
                // active so the owner cannot accidentally overwrite their
                // room with a blank default on a network blip. Without the
                // backoff, an instantly-failing error (e.g. ConnRefused)
                // would return so fast that the retry fires in the same
                // frame, busy-looping on the IoTaskPool and flooding the
                // diagnostics log.
                let next_attempt = prev_attempt.saturating_add(1);
                let elapsed = time.elapsed_secs_f64();
                if next_attempt > MAX_RECORD_FETCH_ATTEMPTS {
                    // Persistent failure: stop hammering the endpoint and
                    // surface a recovery banner so the owner can reset to
                    // the default without risking a silent clobber.
                    diagnostics.push(
                        elapsed,
                        format!(
                            "Room record fetch failed ({err:?}) — giving up after {MAX_RECORD_FETCH_ATTEMPTS} attempts"
                        ),
                    );
                    warn!(
                        "Room record fetch exhausted {} attempts: {:?} — entering recovery mode",
                        MAX_RECORD_FETCH_ATTEMPTS, err
                    );
                    commands.insert_resource(RoomRecordRecovery {
                        reason: format!("PDS unreachable: {err:?}"),
                    });
                    pds::RoomRecord::default_for_did(&room_did.0)
                } else {
                    let backoff = room_record_backoff_secs(next_attempt);
                    diagnostics.push(
                        elapsed,
                        format!(
                            "Room record fetch failed ({err:?}) — retrying in {backoff}s (attempt {next_attempt})"
                        ),
                    );
                    warn!(
                        "Room record fetch failed: {:?} — retrying in {}s (attempt {})",
                        err, backoff, next_attempt
                    );
                    // Defer the retry through the frame-loop timer so the
                    // backoff doesn't park an `IoTaskPool` worker thread.
                    commands.spawn(PendingRoomRecordRetry {
                        did: room_did.0.clone(),
                        attempt: next_attempt,
                        fire_at_secs: elapsed + backoff as f64,
                    });
                    continue;
                }
            }
        };
        record.sanitize();
        info!(
            "Room record loaded: {} generators, {} placements",
            record.generators.len(),
            record.placements.len()
        );
        // Install both the live resource (mutated by the world editor) and
        // the stored snapshot (consulted by "Load from PDS" to undo
        // uncommitted edits). The two start identical — any divergence is
        // authored by the owner.
        commands.insert_resource(StoredRoomRecord(record.clone()));
        commands.insert_resource(record);
    }
}

// ---------------------------------------------------------------------------
// Avatar record loading (runs during AppState::Loading, in parallel with
// the room fetch — both must complete before entering InGame so the local
// player has a definitive starting pose *and* recipe).
// ---------------------------------------------------------------------------

/// In-flight `fetch_avatar_record` task for the *local* player's own
/// avatar. Mirrors [`RoomRecordTask`]: a component attached to a throwaway
/// entity drained by [`poll_avatar_record_task`].
#[derive(Component)]
struct AvatarRecordTask {
    did: String,
    task: bevy::tasks::Task<Result<Option<AvatarRecord>, pds::FetchError>>,
    attempt: u32,
}

fn spawn_avatar_record_fetch(commands: &mut Commands, did: String, attempt: u32) {
    let pool = bevy::tasks::IoTaskPool::get();
    let did_for_fetch = did.clone();
    let task = pool.spawn(async move {
        let fut = async {
            let client = config::http::default_client();
            pds::fetch_avatar_record(&client, &did_for_fetch).await
        };
        #[cfg(target_arch = "wasm32")]
        {
            fut.await
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(fut)
        }
    });
    commands.spawn(AvatarRecordTask { did, task, attempt });
}

/// Fire any retry markers whose backoff has elapsed. Runs on Bevy's main
/// frame loop, so an idle 60 s exponential-backoff window costs only the
/// `Time` resource read per frame instead of a permanently-sleeping
/// worker thread. See [`PendingRoomRecordRetry`] for the rationale.
fn fire_pending_record_retries(
    mut commands: Commands,
    room_pending: Query<(Entity, &PendingRoomRecordRetry)>,
    avatar_pending: Query<(Entity, &PendingAvatarRecordRetry)>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    for (entity, pending) in room_pending.iter() {
        if now >= pending.fire_at_secs {
            let did = pending.did.clone();
            let attempt = pending.attempt;
            commands.entity(entity).despawn();
            spawn_room_record_fetch(&mut commands, did, attempt);
        }
    }
    for (entity, pending) in avatar_pending.iter() {
        if now >= pending.fire_at_secs {
            let did = pending.did.clone();
            let attempt = pending.attempt;
            commands.entity(entity).despawn();
            spawn_avatar_record_fetch(&mut commands, did, attempt);
        }
    }
}

/// Kick off the async `getRecord` fetch for the local player's avatar.
/// Silently no-ops if the user never logged in (session absent), in which
/// case [`check_loading_complete`] will also refuse to advance — we never
/// reach Loading without a session in normal flow.
fn start_avatar_record_fetch(
    mut commands: Commands,
    session: Option<Res<bevy_symbios_multiuser::auth::AtprotoSession>>,
) {
    let Some(sess) = session else {
        warn!("start_avatar_record_fetch: no session — local avatar will not load");
        return;
    };
    spawn_avatar_record_fetch(&mut commands, sess.did.clone(), 0);
}

/// Drain a finished `AvatarRecordTask`, install both the live and stored
/// resources, and synthesise a DID-derived default on a 404. Transient
/// failures retry with exponential backoff so a network blip cannot
/// silently clobber the user's published avatar with the default.
fn poll_avatar_record_task(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut AvatarRecordTask)>,
    mut diagnostics: ResMut<DiagnosticsLog>,
    time: Res<Time>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        let prev_attempt = task.attempt;
        let did = task.did.clone();
        commands.entity(entity).despawn();

        let mut record = match result {
            Ok(Some(r)) => r,
            Ok(None) => {
                info!("No avatar record on PDS — using DID-hashed default");
                AvatarRecord::default_for_did(&did)
            }
            Err(pds::FetchError::Decode(msg)) => {
                // Decode failure is permanent, not transient: the stored
                // record exists but its schema is incompatible with the
                // current `AvatarRecord` (lexicon drift, partially-migrated
                // field, bincode/JSON mismatch). Retrying will never
                // recover, so fall straight through to the DID-hashed
                // default — otherwise Loading hangs forever and the
                // diagnostics log fills with identical decode warnings.
                // The owner can re-publish from the avatar editor to
                // overwrite the incompatible record with the new schema.
                let elapsed = time.elapsed_secs_f64();
                diagnostics.push(
                    elapsed,
                    format!("Stored avatar record incompatible ({msg}) — falling back to default"),
                );
                warn!(
                    "Stored avatar record could not be decoded ({}) — using DID-hashed default",
                    msg
                );
                AvatarRecord::default_for_did(&did)
            }
            Err(err) => {
                // Transient failure — retry with backoff rather than
                // installing the default. Installing the default on a
                // network error would let a subsequent "Publish" click
                // silently clobber the user's real avatar. After
                // `MAX_RECORD_FETCH_ATTEMPTS` we stop retrying so a dead
                // PDS can't drive a permanent busy-loop against the user's
                // CPU or the remote endpoint.
                let next_attempt = prev_attempt.saturating_add(1);
                let elapsed = time.elapsed_secs_f64();
                if next_attempt > MAX_RECORD_FETCH_ATTEMPTS {
                    diagnostics.push(
                        elapsed,
                        format!(
                            "Avatar record fetch failed ({err:?}) — giving up after {MAX_RECORD_FETCH_ATTEMPTS} attempts, using default"
                        ),
                    );
                    warn!(
                        "Avatar record fetch exhausted {} attempts: {:?} — falling back to default",
                        MAX_RECORD_FETCH_ATTEMPTS, err
                    );
                    AvatarRecord::default_for_did(&did)
                } else {
                    let backoff = room_record_backoff_secs(next_attempt);
                    diagnostics.push(
                        elapsed,
                        format!(
                            "Avatar record fetch failed ({err:?}) — retrying in {backoff}s (attempt {next_attempt})"
                        ),
                    );
                    warn!(
                        "Avatar record fetch failed: {:?} — retrying in {}s (attempt {})",
                        err, backoff, next_attempt
                    );
                    // Defer the retry through the frame-loop timer so the
                    // backoff doesn't park an `IoTaskPool` worker thread.
                    commands.spawn(PendingAvatarRecordRetry {
                        did,
                        attempt: next_attempt,
                        fire_at_secs: elapsed + backoff as f64,
                    });
                    continue;
                }
            }
        };
        record.sanitize();
        commands.insert_resource(LiveAvatarRecord(record.clone()));
        commands.insert_resource(StoredAvatarRecord(record));
    }
}

// ---------------------------------------------------------------------------
// Inventory record loading (runs during AppState::Loading, in parallel with
// room + avatar fetches). Unlike those two, the inventory fetch is best-effort:
// transient failures fall through to an empty stash rather than retrying,
// because nothing gameplay-critical reads the inventory — the owner can
// re-open the Inventory window after login if they want to retry by
// publishing a saved item.
// ---------------------------------------------------------------------------

#[derive(Component)]
struct InventoryRecordTask(
    bevy::tasks::Task<Result<Option<crate::pds::InventoryRecord>, crate::pds::FetchError>>,
);

fn start_inventory_record_fetch(
    mut commands: Commands,
    session: Option<Res<bevy_symbios_multiuser::auth::AtprotoSession>>,
) {
    let Some(sess) = session else {
        return;
    };
    let pool = bevy::tasks::IoTaskPool::get();
    let did = sess.did.clone();
    let task = pool.spawn(async move {
        let fut = async {
            let client = config::http::default_client();
            pds::fetch_inventory_record(&client, &did).await
        };
        #[cfg(target_arch = "wasm32")]
        {
            fut.await
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(fut)
        }
    });
    commands.spawn(InventoryRecordTask(task));
}

fn poll_inventory_record_task(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut InventoryRecordTask)>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };
        commands.entity(entity).despawn();
        let mut record = match result {
            Ok(Some(r)) => r,
            _ => crate::pds::InventoryRecord::default(),
        };
        record.sanitize();
        commands.insert_resource(LiveInventoryRecord(record.clone()));
        commands.insert_resource(StoredInventoryRecord(record));
    }
}

/// Transition out of `Loading` only once *every* resource the first
/// `InGame` frame relies on is present:
///
/// - [`terrain::FinishedHeightMap`] — collider is solid
/// - [`RoomRecord`] — live room recipe (world builder consumes this)
/// - [`StoredRoomRecord`] — committed snapshot used by the Load-from-PDS button
/// - [`LiveAvatarRecord`] — live avatar driving `spawn_local_player`
/// - [`StoredAvatarRecord`] — committed snapshot used by the Load-from-PDS button
/// - [`LiveInventoryRecord`] / [`StoredInventoryRecord`] — owner's Generator stash
///
/// Advancing early leaves the poll systems orphaned (they only run in
/// `Loading`), which would strand a slower PDS round-trip and leave the
/// owner unable to edit what was never fetched.
#[allow(clippy::too_many_arguments)]
fn check_loading_complete(
    finished_hm: Option<Res<terrain::FinishedHeightMap>>,
    room_record: Option<Res<RoomRecord>>,
    stored_room: Option<Res<StoredRoomRecord>>,
    live_avatar: Option<Res<LiveAvatarRecord>>,
    stored_avatar: Option<Res<StoredAvatarRecord>>,
    live_inventory: Option<Res<LiveInventoryRecord>>,
    stored_inventory: Option<Res<StoredInventoryRecord>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if finished_hm.is_some()
        && room_record.is_some()
        && stored_room.is_some()
        && live_avatar.is_some()
        && stored_avatar.is_some()
        && live_inventory.is_some()
        && stored_inventory.is_some()
    {
        next_state.set(AppState::InGame);
    }
}
