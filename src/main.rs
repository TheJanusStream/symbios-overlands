use avian3d::PhysicsPlugins;
use bevy::light::{CascadeShadowConfigBuilder, GlobalAmbientLight, NotShadowCaster};
use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};

mod avatar;
mod camera;
pub mod config;
mod logout;
mod network;
mod protocol;
mod rover;
mod social;
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

fn setup_lighting(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let lp = config::lighting::LIGHT_POS;
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
