//! Third-person orbit camera driven by `bevy_panorbit_camera`.
//!
//! Configures distance fog (which also tints the sky cuboid and clear
//! colour) and a follow system that tracks the local player's chassis:
//! the camera's `target_focus` is kept on the chassis each frame, and
//! its `target_yaw` is rotated by the delta of the chassis yaw so
//! steering rotates the world around the player instead of whipping
//! the view around.

use bevy::audio::SpatialListener;
#[cfg(not(target_arch = "wasm32"))]
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::{post_process::bloom::Bloom, prelude::*};
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin, PanOrbitCameraSystemSet};
use transform_gizmo_bevy::GizmoCamera;

use crate::config::camera as cfg;
use crate::config::interaction::audio as audio_cfg;
use crate::player::VehicleChassis;
use crate::state::{AppState, LocalPlayer};
use crate::terrain::FinishedHeightMap;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PanOrbitCameraPlugin)
            .add_systems(Startup, spawn_orbit_camera)
            .add_systems(
                Update,
                follow_local_player.run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                PostUpdate,
                (
                    gate_camera_on_gui.before(PanOrbitCameraSystemSet),
                    // After the crate has written the camera Transform,
                    // before propagation snapshots it for rendering.
                    clamp_camera_to_terrain
                        .after(PanOrbitCameraSystemSet)
                        .before(bevy::transform::TransformSystems::Propagate)
                        .run_if(in_state(AppState::InGame)),
                ),
            );
    }
}

/// Our replacement for `bevy_panorbit_camera`'s `bevy_egui` feature (which
/// is deliberately disabled — see Cargo.toml): block camera input while the
/// GUI wants the pointer/keyboard, EXCEPT that a held right or middle
/// button always controls the camera — an orbit (#702) or pan (#853) must
/// never die because the drag started over (or crossed) an editor window.
/// Scroll-zoom stays blocked while hovering a window on purpose: the wheel
/// is how egui scrolls its own panels.
///
/// Mirrors the crate's own two-frame trick: `wants_pointer_input()` flips
/// true one frame late on a click into a window, so both the previous and
/// current frame must be GUI-free before camera input is allowed.
fn gate_camera_on_gui(
    mut contexts: Query<&mut bevy_egui::EguiContext>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut cameras: Query<&mut PanOrbitCamera>,
    mut prev_gui_wants: Local<bool>,
) {
    let mut gui_wants = false;
    for mut ctx in contexts.iter_mut() {
        let ctx = ctx.get_mut();
        gui_wants |= ctx.wants_pointer_input() || ctx.wants_keyboard_input();
    }
    let enable = mouse.any_pressed([MouseButton::Right, MouseButton::Middle])
        || (!gui_wants && !*prev_gui_wants);
    *prev_gui_wants = gui_wants;
    for mut cam in cameras.iter_mut() {
        // Manual change-detect: writing every frame would dirty the
        // component and defeat the crate's own change tracking.
        if cam.enabled != enable {
            cam.enabled = enable;
        }
    }
}

/// Distance along a focus→camera ray at which the ray first dips below
/// the ground line, sampled at [`cfg::TERRAIN_CLAMP_SAMPLES`] points and
/// backed off one step; `dist` when the whole ray is clear. Pure over a
/// ground-height closure so it unit-tests without a real heightmap.
fn clamp_distance_along_ray(
    focus: Vec3,
    dir: Vec3,
    dist: f32,
    mut ground_y: impl FnMut(f32, f32) -> f32,
) -> f32 {
    let steps = cfg::TERRAIN_CLAMP_SAMPLES;
    let step = dist / steps as f32;
    for i in 1..=steps {
        let t = step * i as f32;
        let p = focus + dir * t;
        if p.y < ground_y(p.x, p.z) + cfg::TERRAIN_CLEARANCE {
            return (t - step).max(cfg::TERRAIN_CLAMP_MIN_DIST);
        }
    }
    dist
}

/// Pull the camera in along its focus→camera ray when it would dip under
/// the terrain (#853) — orbiting low or zooming out over a slope used to
/// show the world's underside. Runs after `PanOrbitCameraSystemSet` has
/// written the camera `Transform` and only rewrites `translation`:
/// sliding along the ray toward the focus preserves the exact look
/// direction, and the crate recomputes the transform from its own
/// yaw/pitch/radius state next frame, so no feedback loop forms.
fn clamp_camera_to_terrain(
    heightmap: Option<Res<FinishedHeightMap>>,
    mut cameras: Query<(&PanOrbitCamera, &mut Transform), With<Camera3d>>,
) {
    let Some(hm) = heightmap else {
        return;
    };
    for (cam, mut tf) in cameras.iter_mut() {
        let focus = cam.focus;
        let offset = tf.translation - focus;
        let dist = offset.length();
        if dist <= cfg::TERRAIN_CLAMP_MIN_DIST {
            continue;
        }
        let dir = offset / dist;
        let clamped = clamp_distance_along_ray(focus, dir, dist, |x, z| hm.world_height_at(x, z));
        if clamped < dist {
            tf.translation = focus + dir * clamped;
        }
    }
}

fn spawn_orbit_camera(mut commands: Commands) {
    let pos = cfg::INITIAL_POS;
    let fc = cfg::fog::COLOR;
    commands.spawn((
        Camera3d::default(),
        // WebGL2's `glow` backend has no `tex_storage_2d_multisample`
        // entrypoint, so Bevy's default `Msaa::Sample4` panics during
        // render-target allocation as soon as the first frame renders
        // (panicked at glow-0.16.0/.../web_sys.rs: "Tex storage 2D
        // multisample is not supported"). Native and WebGPU paths handle
        // MSAA fine; only WebGL2 needs the opt-out. Disabling on every
        // wasm build is the safe superset — modern browsers exposing
        // WebGPU still work with MSAA off, and we don't depend on
        // anti-aliased edges anywhere visually critical.
        #[cfg(target_arch = "wasm32")]
        Msaa::Off,
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
        // Opaque depth prepass. The transparent water material is
        // `AlphaMode::Blend` and keeps `enable_prepass() -> false`, so
        // it never *writes* prepass depth (writing it would occlude
        // every fragment the main pass blends underneath). It only
        // *reads* this opaque-geometry depth texture, to resolve the
        // water-to-bottom distance for the shoreline-foam band (#257).
        // Cost: opaque scene geometry now runs a depth-only pre-pass;
        // non-water materials are otherwise visually unchanged.
        //
        // WebGL2 caveat: enabling the prepass also defines `DEPTH_PREPASS`
        // for the main-pass PBR shaders, and Bevy's prepass-depth read
        // path uses `textureLoad` on a depth texture — which naga's GLSL
        // backend rejects with "WGSL `textureLoad` from depth textures is
        // not supported in GLSL", panicking pipeline creation for every
        // alpha-blend PBR material (cloud, water). The shoreline-foam
        // block in water.wgsl is the only consumer in this codebase and
        // is already `#ifdef DEPTH_PREPASS`-guarded, so omitting the
        // component on wasm32 cleanly disables the feature — shore foam
        // is the only visual loss on WebGL2, and only on water bodies
        // whose room record sets `shore_foam_width > 0`.
        #[cfg(not(target_arch = "wasm32"))]
        DepthPrepass,
        GizmoCamera,
        PanOrbitCamera {
            radius: Some(cfg::ORBIT_RADIUS),
            pitch: Some(cfg::ORBIT_PITCH),
            button_orbit: MouseButton::Right,
            button_pan: MouseButton::Middle,
            // Zoom + pitch envelope (#853): without limits the wheel
            // could zoom through the avatar or out past the fog, and a
            // low orbit dived straight under the ground plane (the
            // terrain clamp handles the slope-dependent remainder).
            zoom_lower_limit: cfg::ZOOM_LOWER_LIMIT,
            zoom_upper_limit: Some(cfg::ZOOM_UPPER_LIMIT),
            pitch_lower_limit: Some(cfg::PITCH_LOWER_LIMIT),
            pitch_upper_limit: Some(cfg::PITCH_UPPER_LIMIT),
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
        // Spatial-audio listener for contact-effect cues (#262). Ears a
        // head-width apart (Bevy's 4 m default over-pans); inert for
        // non-spatial audio, so this is purely additive.
        SpatialListener::new(audio_cfg::LISTENER_EAR_GAP),
    ));
}

/// Keep the orbit camera glued to the local chassis.
///
/// Reads the chassis `Transform`, not `GlobalTransform` (#670): the root
/// is parentless, and the avatar's interpolation easing writes the
/// smoothed pose to `Transform` in `RunFixedMainLoop`, before `Update`,
/// so this system sees the *same-frame eased* pose. `GlobalTransform` is
/// only refreshed by `PostUpdate` propagation (and by Avian just before
/// each fixed step), so it lags a frame and its staleness oscillates at
/// the fixed-vs-refresh beat — feeding it into the focus lerp was the
/// rubber-band half of the own-avatar stutter.
fn follow_local_player(
    player_query: Query<(&Transform, Option<&VehicleChassis>), With<LocalPlayer>>,
    mut camera_query: Query<&mut PanOrbitCamera>,
    mut prev_yaw: Local<Option<f32>>,
) {
    let Ok((player_tf, vehicle)) = player_query.single() else {
        return;
    };
    let Ok(mut cam) = camera_query.single_mut() else {
        return;
    };
    cam.target_focus = player_tf.translation;

    // Only inherit yaw when driving a vehicle preset (hover-boat, airplane,
    // helicopter, car). On the humanoid preset the physics body never
    // rotates, and we want the mouse to orbit freely without snapping when
    // the visual rig turns to face movement.
    //
    // Heading comes from the projected forward vector, not
    // `to_euler(YXZ)` (#853): the Euler yaw term degenerates at pitch
    // ±90°, so an airplane loop used to whip the camera π at the
    // vertical. Near the pole the projection has no magnitude and the
    // heading is genuinely undefined — freeze yaw inheritance there
    // (keep `prev_yaw`) and resume accumulating once the nose comes back
    // down; a full loop then contributes its true net yaw instead of a
    // flip.
    if vehicle.is_some() {
        let fwd = player_tf.rotation * Vec3::NEG_Z;
        if fwd.y.abs() <= cfg::YAW_FREEZE_FORWARD_Y {
            let vehicle_yaw = (-fwd.x).atan2(-fwd.z);
            if let Some(prev) = *prev_yaw {
                let delta = {
                    use std::f32::consts::{PI, TAU};
                    let d = (vehicle_yaw - prev).rem_euclid(TAU);
                    if d > PI { d - TAU } else { d }
                };
                cam.target_yaw += delta;
            }
            *prev_yaw = Some(vehicle_yaw);
        }
    } else {
        *prev_yaw = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::MinimalPlugins;

    /// #670 guard: the follow target must come from `Transform` — the
    /// same-frame eased pose — not `GlobalTransform`. `MinimalPlugins`
    /// registers no transform propagation, so a regression back to
    /// `GlobalTransform` would read the never-propagated identity here
    /// and miss the spawned position.
    #[test]
    fn follow_reads_the_same_frame_transform() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, follow_local_player);
        let pos = Vec3::new(5.0, 2.0, 7.0);
        app.world_mut()
            .spawn((Transform::from_translation(pos), LocalPlayer));
        app.world_mut().spawn(PanOrbitCamera::default());

        app.update();

        let mut cams = app.world_mut().query::<&PanOrbitCamera>();
        let cam = cams.single(app.world()).unwrap();
        assert_eq!(
            cam.target_focus, pos,
            "focus must track the player's same-frame Transform"
        );
    }

    /// Vehicle yaw rides the same `Transform` read: the first frame only
    /// records the reference yaw, later frames accumulate the wrapped
    /// delta into `target_yaw`.
    #[test]
    fn vehicle_yaw_delta_accumulates_from_transform() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, follow_local_player);
        let player = app
            .world_mut()
            .spawn((
                Transform::from_rotation(Quat::from_rotation_y(0.3)),
                LocalPlayer,
                VehicleChassis,
            ))
            .id();
        app.world_mut().spawn(PanOrbitCamera::default());

        app.update();
        app.world_mut()
            .entity_mut(player)
            .get_mut::<Transform>()
            .unwrap()
            .rotation = Quat::from_rotation_y(0.8);
        app.update();

        let mut cams = app.world_mut().query::<&PanOrbitCamera>();
        let cam = cams.single(app.world()).unwrap();
        assert!(
            (cam.target_yaw - 0.5).abs() < 1e-5,
            "target_yaw must accumulate the wrapped yaw delta, got {}",
            cam.target_yaw
        );
    }

    /// #853: mid-loop (forward near-vertical) the heading is undefined —
    /// yaw inheritance must freeze instead of whipping the camera, and
    /// resume accumulating from the pre-loop reference when the nose
    /// comes back down.
    #[test]
    fn vertical_forward_freezes_yaw_inheritance() {
        use std::f32::consts::FRAC_PI_2;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(Update, follow_local_player);
        let player = app
            .world_mut()
            .spawn((
                Transform::from_rotation(Quat::from_rotation_y(0.3)),
                LocalPlayer,
                VehicleChassis,
            ))
            .id();
        app.world_mut().spawn(PanOrbitCamera::default());
        app.update();

        // Nose straight up (forward = +Y): the degenerate stretch. The
        // old to_euler extraction produced an arbitrary yaw here.
        app.world_mut()
            .entity_mut(player)
            .get_mut::<Transform>()
            .unwrap()
            .rotation = Quat::from_rotation_x(FRAC_PI_2);
        app.update();
        {
            let mut cams = app.world_mut().query::<&PanOrbitCamera>();
            let cam = cams.single(app.world()).unwrap();
            assert_eq!(
                cam.target_yaw, 0.0,
                "yaw must not move while forward is vertical"
            );
        }

        // Nose back down at a new heading: the delta from the PRE-loop
        // reference (0.3 → 0.8) accumulates, nothing from the vertical.
        app.world_mut()
            .entity_mut(player)
            .get_mut::<Transform>()
            .unwrap()
            .rotation = Quat::from_rotation_y(0.8);
        app.update();
        let mut cams = app.world_mut().query::<&PanOrbitCamera>();
        let cam = cams.single(app.world()).unwrap();
        assert!(
            (cam.target_yaw - 0.5).abs() < 1e-5,
            "yaw must resume from the pre-loop reference, got {}",
            cam.target_yaw
        );
    }

    /// #853: the terrain clamp pulls the camera in along the ray, one
    /// sample short of the first below-ground point, and leaves clear
    /// rays untouched.
    #[test]
    fn terrain_clamp_stops_short_of_the_ground() {
        let focus = Vec3::new(0.0, 10.0, 0.0);
        // Descending ray at 45°: hits y = ground(5.0) + clearance
        // somewhere past the midpoint of a 16 m ray.
        let dir = Vec3::new(
            std::f32::consts::FRAC_1_SQRT_2,
            -std::f32::consts::FRAC_1_SQRT_2,
            0.0,
        );
        let dist = 16.0;
        let flat_ground = |_x: f32, _z: f32| 5.0;

        let clamped = clamp_distance_along_ray(focus, dir, dist, flat_ground);
        assert!(
            clamped < dist,
            "a ray dipping under ground must be shortened"
        );
        // Every sample up to the clamped distance stays above ground +
        // clearance (the guarantee the renderer relies on).
        let p = focus + dir * clamped;
        assert!(
            p.y >= 5.0 + cfg::TERRAIN_CLEARANCE - 1e-4,
            "clamped point {p:?} is below the clearance line"
        );

        // A ray that never dips below ground is untouched.
        let up_dir = Vec3::new(0.0, 1.0, 0.0);
        assert_eq!(
            clamp_distance_along_ray(focus, up_dir, dist, flat_ground),
            dist
        );
    }
}
