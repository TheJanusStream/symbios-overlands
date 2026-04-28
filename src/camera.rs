//! Third-person orbit camera driven by `bevy_panorbit_camera`.
//!
//! Configures distance fog (which also tints the sky cuboid and clear
//! colour) and a follow system that tracks the local rover: the camera's
//! `target_focus` is kept on the chassis each frame, and its `target_yaw`
//! is rotated by the delta of the rover's yaw so steering rotates the world
//! around the player instead of whipping the view around.

use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::{post_process::bloom::Bloom, prelude::*};
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use transform_gizmo_bevy::GizmoCamera;

use crate::config::camera as cfg;
use crate::player::VehicleChassis;
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
    let fc = cfg::fog::COLOR;
    commands.spawn((
        Camera3d::default(),
        // Bevy's default perspective far plane is 1000 m, which clips the
        // cloud-deck plane (at altitude ~250 m, half-extent 4 km) before
        // the shader's horizon-fade has a chance to dissolve it. Pushing
        // far out to 12 km keeps the entire deck and the SkyBox cuboid at
        // SKY_SCALE = 2000 m well inside the frustum at every camera
        // pitch, while reverse-Z depth-precision stays comfortable for
        // foreground gameplay (Bevy uses reverse-Z by default in 0.18).
        Projection::from(PerspectiveProjection {
            far: 12_000.0,
            ..default()
        }),
        GizmoCamera,
        PanOrbitCamera {
            radius: Some(cfg::ORBIT_RADIUS),
            pitch: Some(cfg::ORBIT_PITCH),
            button_orbit: MouseButton::Right,
            button_pan: MouseButton::Middle,
            ..default()
        },
        Transform::from_xyz(pos[0], pos[1], pos[2]).looking_at(Vec3::ZERO, Vec3::Y),
        DistanceFog {
            color: Color::srgba(fc[0], fc[1], fc[2], fc[3]),
            directional_light_color: Color::srgba(
                cfg::fog::DIRECTIONAL_LIGHT_COLOR[0],
                cfg::fog::DIRECTIONAL_LIGHT_COLOR[1],
                cfg::fog::DIRECTIONAL_LIGHT_COLOR[2],
                cfg::fog::DIRECTIONAL_LIGHT_COLOR[3],
            ),
            directional_light_exponent: cfg::fog::DIRECTIONAL_LIGHT_EXPONENT,
            falloff: FogFalloff::from_visibility_colors(
                cfg::fog::VISIBILITY,
                Color::srgb(
                    cfg::fog::EXTINCTION_COLOR[0],
                    cfg::fog::EXTINCTION_COLOR[1],
                    cfg::fog::EXTINCTION_COLOR[2],
                ),
                Color::srgb(
                    cfg::fog::INSCATTERING_COLOR[0],
                    cfg::fog::INSCATTERING_COLOR[1],
                    cfg::fog::INSCATTERING_COLOR[2],
                ),
            ),
        },
        Bloom::NATURAL, // Enable Bloom
    ));
}

fn follow_local_player(
    player_query: Query<(&GlobalTransform, Option<&VehicleChassis>), With<LocalPlayer>>,
    mut camera_query: Query<&mut PanOrbitCamera>,
    mut prev_yaw: Local<Option<f32>>,
) {
    let Ok((player_tf, vehicle)) = player_query.single() else {
        return;
    };
    let Ok(mut cam) = camera_query.single_mut() else {
        return;
    };
    cam.target_focus = player_tf.translation();

    // Only inherit yaw when driving a vehicle preset (hover-boat, airplane,
    // helicopter, car). On the humanoid preset the physics body never
    // rotates, and we want the mouse to orbit freely without snapping when
    // the visual rig turns to face movement.
    if vehicle.is_some() {
        let (_, rotation, _) = player_tf.to_scale_rotation_translation();
        let (vehicle_yaw, _, _) = rotation.to_euler(EulerRot::YXZ);
        if let Some(prev) = *prev_yaw {
            let delta = {
                use std::f32::consts::{PI, TAU};
                let d = (vehicle_yaw - prev).rem_euclid(TAU);
                if d > PI { d - TAU } else { d }
            };
            cam.target_yaw += delta;
        }
        *prev_yaw = Some(vehicle_yaw);
    } else {
        *prev_yaw = None;
    }
}
