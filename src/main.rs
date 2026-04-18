//! Symbios Overlands — application entry point.
//!
//! Wires every gameplay plugin, initialises the shared ECS resources, and
//! coordinates the three-stage state machine (`Login` → `Loading` → `InGame`).
//! The loading gate here explicitly waits on **both** the heightmap generation
//! task *and* the ATProto PDS room-record fetch before entering `InGame` so
//! slower PDS round-trips cannot be silently dropped.

use avian3d::PhysicsPlugins;
use bevy::light::{CascadeShadowConfigBuilder, GlobalAmbientLight, NotShadowCaster};
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};

mod avatar;
mod camera;
pub mod config;
mod logout;
mod network;
pub mod pds;
mod player;
mod protocol;
mod social;
mod splat;
mod state;
mod terrain;
mod ui;
mod water;
mod world_builder;

use pds::{AvatarRecord, RoomRecord};

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
    AppState, ChatHistory, CurrentRoomDid, DiagnosticsLog, LiveAvatarRecord, LocalSettings,
    PublishFeedback, RoomRecordRecovery, StoredAvatarRecord, StoredRoomRecord,
};

fn main() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

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
        .add_plugins(terrain::TerrainPlugin)
        .add_plugins(world_builder::WorldBuilderPlugin)
        .add_plugins(player::PlayerPlugin)
        .add_plugins(camera::CameraPlugin)
        .add_plugins(network::NetworkPlugin)
        .add_plugins(avatar::AvatarPlugin)
        .add_plugins(social::SocialPlugin)
        .add_plugins(logout::LogoutPlugin)
        .init_state::<AppState>()
        .init_resource::<ChatHistory>()
        .init_resource::<DiagnosticsLog>()
        .init_resource::<LocalSettings>()
        .init_resource::<PublishFeedback>()
        .init_resource::<ui::login::LoginError>()
        .add_systems(
            EguiPrimaryContextPass,
            (ui::login::login_ui, ui::login::poll_auth_task).run_if(in_state(AppState::Login)),
        )
        .add_systems(
            OnEnter(AppState::Loading),
            (start_room_record_fetch, start_avatar_record_fetch),
        )
        .add_systems(
            Update,
            (
                poll_room_record_task,
                poll_avatar_record_task,
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
                ui::avatar::avatar_ui,
                ui::room::room_admin_ui,
            )
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            Update,
            (
                ui::room::poll_publish_tasks,
                ui::avatar::poll_publish_avatar_tasks,
            )
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(Startup, setup_lighting)
        .run();
}

fn setup_lighting(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
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
        Mesh3d(meshes.add(Cuboid::new(2.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(sky_c[0], sky_c[1], sky_c[2]),
            unlit: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_scale(Vec3::splat(config::lighting::SKY_SCALE)),
        NotShadowCaster,
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

/// Pause the fetch future for `delay_secs` before retrying, using the
/// correct timer primitive for each target. On native we block the
/// single-threaded tokio runtime the task drives; on `wasm32` we rely on
/// `gloo-timers`, which schedules a JS `setTimeout` and resolves a future
/// that yields control back to the browser event loop — without this,
/// the retry queued by `poll_*_record_task` would re-enter the fetch at
/// the application framerate and hammer the PDS at 60–144 rps.
async fn record_fetch_delay(delay_secs: u64) {
    if delay_secs == 0 {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
    }
    #[cfg(target_arch = "wasm32")]
    {
        gloo_timers::future::TimeoutFuture::new((delay_secs as u32).saturating_mul(1000)).await;
    }
}

fn spawn_room_record_fetch(commands: &mut Commands, did: String, attempt: u32) {
    let delay_secs = room_record_backoff_secs(attempt);
    // `IoTaskPool` is the correct home for blocking HTTP calls — the
    // `AsyncComputeTaskPool` is sized to the CPU-core count and must not be
    // starved by threads blocked on network sockets.
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            record_fetch_delay(delay_secs).await;
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
                    spawn_room_record_fetch(&mut commands, room_did.0.clone(), next_attempt);
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
    let delay_secs = room_record_backoff_secs(attempt);
    let pool = bevy::tasks::IoTaskPool::get();
    let did_for_fetch = did.clone();
    let task = pool.spawn(async move {
        let fut = async {
            record_fetch_delay(delay_secs).await;
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
                    spawn_avatar_record_fetch(&mut commands, did, next_attempt);
                    continue;
                }
            }
        };
        record.sanitize();
        commands.insert_resource(LiveAvatarRecord(record.clone()));
        commands.insert_resource(StoredAvatarRecord(record));
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
///
/// Advancing early leaves the poll systems orphaned (they only run in
/// `Loading`), which would strand a slower PDS round-trip and leave the
/// owner unable to edit what was never fetched.
fn check_loading_complete(
    finished_hm: Option<Res<terrain::FinishedHeightMap>>,
    room_record: Option<Res<RoomRecord>>,
    stored_room: Option<Res<StoredRoomRecord>>,
    live_avatar: Option<Res<LiveAvatarRecord>>,
    stored_avatar: Option<Res<StoredAvatarRecord>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if finished_hm.is_some()
        && room_record.is_some()
        && stored_room.is_some()
        && live_avatar.is_some()
        && stored_avatar.is_some()
    {
        next_state.set(AppState::InGame);
    }
}
