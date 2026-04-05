use avian3d::PhysicsPlugins;
use bevy::light::GlobalAmbientLight;
use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};

mod avatar;
mod camera;
pub mod config;
mod logout;
mod network;
mod protocol;
mod rover;
mod splat;
mod state;
mod terrain;
mod ui;
mod water;

use state::{AppState, ChatHistory, DiagnosticsLog, LocalAirshipParams, LocalPhysicsParams};

fn main() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    let fc = config::camera::fog::COLOR;
    App::new()
        .insert_resource(ClearColor(Color::srgba(fc[0], fc[1], fc[2], fc[3])))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Symbios Overlands".into(),
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
            )
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(Startup, setup_lighting)
        .run();
}

fn setup_lighting(mut commands: Commands) {
    let lp = config::lighting::LIGHT_POS;
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            illuminance: config::lighting::ILLUMINANCE,
            ..default()
        },
        Transform::from_xyz(lp[0], lp[1], lp[2]).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: config::lighting::AMBIENT_BRIGHTNESS,
        ..default()
    });
}

fn loading_ui(mut contexts: bevy_egui::EguiContexts) {
    bevy_egui::egui::CentralPanel::default().show(contexts.ctx_mut().unwrap(), |ui| {
        ui.centered_and_justified(|ui| {
            ui.heading("Generating the overlands…");
            ui.spinner();
        });
    });
}
