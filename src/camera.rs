use bevy::prelude::*;
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};

use crate::config::camera as cfg;
use crate::state::{AppState, LocalPlayer};

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PanOrbitCameraPlugin)
            .add_systems(Startup, spawn_orbit_camera)
            .add_systems(
                Update,
                follow_local_player.run_if(in_state(AppState::InGame)),
            );
    }
}

fn spawn_orbit_camera(mut commands: Commands) {
    let pos = cfg::INITIAL_POS;
    commands.spawn((
        Camera3d::default(),
        PanOrbitCamera {
            radius: Some(cfg::ORBIT_RADIUS),
            pitch: Some(cfg::ORBIT_PITCH),
            button_orbit: MouseButton::Right,
            button_pan: MouseButton::Middle,
            ..default()
        },
        Transform::from_xyz(pos[0], pos[1], pos[2]).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn follow_local_player(
    player_query: Query<&GlobalTransform, With<LocalPlayer>>,
    mut camera_query: Query<&mut PanOrbitCamera>,
) {
    let Ok(player_tf) = player_query.single() else {
        return;
    };
    let Ok(mut cam) = camera_query.single_mut() else {
        return;
    };
    cam.target_focus = player_tf.translation();
}
