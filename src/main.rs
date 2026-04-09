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

use pds::RoomRecord;
use state::{
    AppState, ChatHistory, CurrentRoomDid, DiagnosticsLog, LocalAirshipParams, LocalPhysicsParams,
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
        .add_systems(
            EguiPrimaryContextPass,
            (ui::login::login_ui, ui::login::poll_auth_task).run_if(in_state(AppState::Login)),
        )
        .add_systems(OnEnter(AppState::Loading), start_room_record_fetch)
        .add_systems(
            Update,
            poll_room_record_task.run_if(in_state(AppState::Loading)),
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
        .add_systems(OnEnter(AppState::InGame), apply_room_sun_color)
        .run();
}

fn setup_lighting(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    room_record: Option<Res<RoomRecord>>,
) {
    let lp = config::lighting::LIGHT_POS;
    let sc = room_record
        .as_ref()
        .map(|r| r.sun_color)
        .unwrap_or(config::lighting::SUN_COLOR);

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

#[derive(Component)]
struct RoomRecordTask(bevy::tasks::Task<Option<RoomRecord>>);

fn start_room_record_fetch(mut commands: Commands, room_did: Res<CurrentRoomDid>) {
    let did = room_did.0.clone();
    let pool = bevy::tasks::AsyncComputeTaskPool::get();
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

fn poll_room_record_task(mut commands: Commands, mut tasks: Query<(Entity, &mut RoomRecordTask)>) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };

        commands.entity(entity).despawn();

        let record = result.unwrap_or_default();
        info!(
            "Room record loaded: water_offset={}, sun={:?}",
            record.water_level_offset, record.sun_color
        );
        commands.insert_resource(record);
    }
}

/// Apply the room record's sun colour to the directional light when entering
/// InGame.  `setup_lighting` runs at Startup before the record is fetched, so
/// this system patches the light once the record is available.
fn apply_room_sun_color(
    room_record: Option<Res<RoomRecord>>,
    mut lights: Query<&mut DirectionalLight>,
) {
    let Some(record) = room_record else { return };
    let c = record.sun_color;
    for mut light in lights.iter_mut() {
        light.color = Color::srgb(c[0], c[1], c[2]);
    }
}
