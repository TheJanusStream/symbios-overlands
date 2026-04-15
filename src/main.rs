//! Symbios Overlands — application entry point.
//!
//! Wires every gameplay plugin, initialises the shared ECS resources, and
//! coordinates the three-stage state machine (`Login` → `Loading` → `InGame`).
//! The loading gate here explicitly waits on **both** the heightmap generation
//! task *and* the ATProto PDS room-record fetch before entering `InGame` so
//! slower PDS round-trips cannot be silently dropped.

use avian3d::PhysicsPlugins;
use bevy::light::{CascadeShadowConfigBuilder, GlobalAmbientLight, NotShadowCaster};
use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};

mod avatar;
mod camera;
pub mod config;
mod logout;
mod network;
pub mod pds;
mod protocol;
mod rover;
mod social;
mod splat;
mod state;
mod terrain;
mod ui;
mod water;
mod world_builder;

use pds::RoomRecord;

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
    AppState, ChatHistory, CurrentRoomDid, DiagnosticsLog, LocalAirshipParams, LocalPhysicsParams,
    PublishFeedback, RoomRecordRecovery,
};

fn main() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    let fc = config::camera::fog::COLOR;
    App::new()
        .insert_resource(ClearColor(Color::srgba(fc[0], fc[1], fc[2], fc[3])))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Symbios Overlands".into(),
                prevent_default_event_handling: false,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(terrain::TerrainPlugin)
        .add_plugins(world_builder::WorldBuilderPlugin)
        .add_plugins(rover::RoverPlugin)
        .add_plugins(camera::CameraPlugin)
        .add_plugins(network::NetworkPlugin)
        .add_plugins(avatar::AvatarPlugin)
        .add_plugins(social::SocialPlugin)
        .add_plugins(logout::LogoutPlugin)
        .init_state::<AppState>()
        .init_resource::<ChatHistory>()
        .init_resource::<DiagnosticsLog>()
        .init_resource::<LocalAirshipParams>()
        .init_resource::<LocalPhysicsParams>()
        .init_resource::<PublishFeedback>()
        .init_resource::<ui::login::LoginError>()
        .add_systems(
            EguiPrimaryContextPass,
            (ui::login::login_ui, ui::login::poll_auth_task).run_if(in_state(AppState::Login)),
        )
        .add_systems(OnEnter(AppState::Loading), start_room_record_fetch)
        .add_systems(
            Update,
            (poll_room_record_task, check_loading_complete)
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
                ui::airship::airship_ui,
                ui::physics::physics_ui,
                ui::room::room_admin_ui,
            )
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            Update,
            ui::room::poll_publish_tasks.run_if(in_state(AppState::InGame)),
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
struct RoomRecordTask(bevy::tasks::Task<Result<Option<RoomRecord>, pds::FetchError>>);

/// Kick off the async ATProto `getRecord` fetch for the room the client is
/// visiting. Runs exactly once on entry to `AppState::Loading`; the result is
/// picked up by `poll_room_record_task` on subsequent frames.
fn start_room_record_fetch(mut commands: Commands, room_did: Res<CurrentRoomDid>) {
    let did = room_did.0.clone();
    // `IoTaskPool` is the correct home for blocking HTTP calls — the
    // `AsyncComputeTaskPool` is sized to the CPU-core count and must not be
    // starved by threads blocked on network sockets.
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            let client = reqwest::Client::new();
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
    commands.spawn(RoomRecordTask(task));
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
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };

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
                // do NOT substitute the default. Log it, re-queue the fetch,
                // and keep the Loading state active so the owner cannot
                // accidentally overwrite their room with a blank default on a
                // network blip.
                let elapsed = time.elapsed_secs_f64();
                diagnostics.push(
                    elapsed,
                    format!("Room record fetch failed ({err:?}) — retrying"),
                );
                warn!("Room record fetch failed: {:?} — retrying", err);
                let did = room_did.0.clone();
                let pool = bevy::tasks::IoTaskPool::get();
                let retry = pool.spawn(async move {
                    let fut = async {
                        let client = reqwest::Client::new();
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
                commands.spawn(RoomRecordTask(retry));
                continue;
            }
        };
        record.sanitize();
        info!(
            "Room record loaded: {} generators, {} placements",
            record.generators.len(),
            record.placements.len()
        );
        commands.insert_resource(record);
    }
}

/// Transition out of `Loading` only once BOTH the heightmap generation task
/// *and* the ATProto PDS room-record fetch have finished.  If we advanced on
/// terrain alone the network task would be orphaned — the poll systems only
/// run in `AppState::Loading`, so a slower PDS round-trip would be silently
/// dropped and the room owner could never edit their own environment.
fn check_loading_complete(
    finished_hm: Option<Res<terrain::FinishedHeightMap>>,
    room_record: Option<Res<RoomRecord>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if finished_hm.is_some() && room_record.is_some() {
        next_state.set(AppState::InGame);
    }
}
