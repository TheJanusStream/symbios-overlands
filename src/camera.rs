//! Third-person orbit camera driven by `bevy_panorbit_camera`.
//!
//! Configures distance fog (which also tints the sky cuboid and clear
//! colour) and a follow system that tracks the local player's chassis:
//! the camera's `target_focus` is kept on the chassis each frame, and
//! its `target_yaw` is rotated by the delta of the chassis yaw so
//! steering rotates the world around the player instead of whipping
//! the view around.
//!
//! The player may choose an orthographic camera (#1603,
//! [`crate::state::LocalSettings::orthographic_camera`]): nothing shrinks
//! with distance. The orbit is the same free orbit - the crate turns its
//! zoom into the projection's scale - with three adjustments:
//!
//! - **The lens is calibrated to the orbit's radius** ([`orthographic_lens`]):
//!   at its focus it frames what the perspective lens frames from that far
//!   away, so the radius keeps its meaning - the zoom limits, the shadow
//!   reach, the AI agent's camera commands - and a switch keeps the view.
//! - **The camera stands where a perspective one would**
//!   ([`stand_orthographic_camera`]): the crate parks an orthographic camera
//!   halfway to its far plane, kilometres off, and every distance taken from
//!   the camera - draw distances, the hair's far tier, the fog, the shadow
//!   cascades, the listening ear, the cloud deck overhead - would follow it
//!   there.
//! - **It looks down at least 20 degrees** ([`cfg::ORTHO_PITCH_LOWER_LIMIT`]):
//!   an orthographic view has no horizon, so a level look shows a wall of
//!   terrain under a flat band of sky.
//!
//! The orthographic view is a box as wide at the camera as at its focus, so
//! it reaches back behind the camera ([`cfg::ORTHO_BACK_SHARE`]) - cut off
//! at the camera, it would slice the ground and the buildings beside it -
//! and the cloud deck is hidden under it (`clouds::hide_cloud_deck`):
//! looking down, an orthographic view never shows the sky the deck hangs
//! in, and a box that tall would hold the deck between the camera and the
//! ground. Distances are still taken from where the camera stands, so a
//! detail tier judged for the perspective lens - the hair's far tier - turns
//! with distance as before, though nothing on screen shrinks.

use bevy::audio::SpatialListener;
use bevy::camera::ScalingMode;
#[cfg(not(target_arch = "wasm32"))]
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::{post_process::bloom::Bloom, prelude::*};
use bevy_egui::{EguiGlobalSettings, PrimaryEguiContext};
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
            // #1317: this plugin, not bevy_egui, decides which camera egui
            // draws through. `insert_resource` is order-independent against
            // `EguiPlugin`, whose `init_resource` never overwrites a value.
            .insert_resource(egui_global_settings())
            .add_systems(Startup, spawn_orbit_camera)
            .add_systems(
                Update,
                follow_local_player.run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                PostUpdate,
                (
                    gate_camera_on_gui.before(PanOrbitCameraSystemSet),
                    // Before the crate, so it lays a new lens out on the
                    // frame it is chosen (#1603).
                    follow_projection_setting.before(PanOrbitCameraSystemSet),
                    // After the crate has written the camera Transform and
                    // before the terrain clamp reads it (#1603).
                    stand_orthographic_camera
                        .after(PanOrbitCameraSystemSet)
                        .before(clamp_camera_to_terrain)
                        .before(bevy::transform::TransformSystems::Propagate)
                        .before(bevy::camera::CameraUpdateSystems),
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

/// Which button pans, and under which modifier key (#1242 f166).
///
/// Pan was bound to the middle button and nothing else, so a laptop or
/// trackpad user - explicitly in scope, and the audience the 1280x720
/// layout work is for - could not perform one of the three camera
/// controls the Controls sheet advertises.
///
/// The obvious fix does not work, and the shape of the crate is why.
/// `pan_pressed` is `modifier_pan.pressed() && mouse.pressed(button_pan)`,
/// so setting `modifier_pan: Some(AltLeft)` on its own does NOT add
/// Alt+right-drag - it makes plain middle-drag stop panning and asks for
/// Alt+MIDDLE instead. `orbit_pressed` additionally requires
/// `!modifier_pan.pressed()`, which is the piece that makes this work:
/// with Alt held, moving `button_pan` onto the right button suppresses
/// orbit and enables pan on the SAME button, and with Alt released the
/// binding goes back to plain middle-drag with no modifier.
///
/// Read at drag START only (see [`gate_camera_on_gui`]): swapping the
/// binding under a held button would flip an orbit into a pan mid-gesture.
fn pan_binding(alt_held: bool) -> (MouseButton, Option<KeyCode>) {
    if alt_held {
        (MouseButton::Right, Some(KeyCode::AltLeft))
    } else {
        (MouseButton::Middle, None)
    }
}

/// How the cursor behaves during a camera drag (#1242 f171).
///
/// The pointer is never confined anywhere in the app, and in the BROWSER
/// build that rations the only look-around gesture by screen width: winit
/// derives its web delta from `movementX/Y`, which goes to zero once the
/// OS cursor pins at the screen edge. (On native the deltas are raw
/// `DeviceEvent::MouseMotion` and are not clamped at all - which is the
/// correction the review's own refuter made, and why this is a browser
/// fix wearing native clothes.)
///
/// `Locked` on wasm because `Confined` is not a thing the web backend
/// implements - pointer lock is; `Confined` on native, which is the
/// gentler of the two and enough, since native motion is already
/// unbounded. The cursor is hidden either way: a pointer visibly stuck
/// against the screen edge while the view keeps turning is its own small
/// lie.
fn drag_cursor_grab(dragging: bool) -> (bevy::window::CursorGrabMode, bool) {
    if !dragging {
        return (bevy::window::CursorGrabMode::None, true);
    }
    #[cfg(target_arch = "wasm32")]
    let held = bevy::window::CursorGrabMode::Locked;
    #[cfg(not(target_arch = "wasm32"))]
    let held = bevy::window::CursorGrabMode::Confined;
    (held, false)
}

/// Our replacement for `bevy_panorbit_camera`'s `bevy_egui` feature (which
/// is deliberately disabled - see Cargo.toml): block camera input while the
/// GUI wants the pointer, EXCEPT that a held right or middle button always
/// controls the camera - an orbit (#702) or pan (#853) must never die
/// because the drag started over (or crossed) an editor window.
/// Scroll-zoom stays blocked while hovering a window on purpose: the wheel
/// is how egui scrolls its own panels.
///
/// KEYBOARD focus is deliberately NOT part of the gate (#1242 f165). It
/// used to be, and it protected nothing: `PanOrbitCamera` is configured
/// with no keyboard bindings at all, while `egui_wants_keyboard_input()`
/// is true for as long as any text field holds focus - anywhere on screen,
/// including with the cursor far out over the 3D world. Chat focuses its
/// input when it opens and re-focuses after every send, so in the app's
/// most common overlay state the wheel simply stopped zooming, with
/// right-drag still working: the failure read as "the wheel is broken".
/// The Alt modifier below is the one keyboard input the camera has, and it
/// is gated by the crate, not by this.
///
/// Mirrors the crate's own two-frame trick: `wants_pointer_input()` flips
/// true one frame late on a click into a window, so both the previous and
/// current frame must be GUI-free before camera input is allowed.
fn gate_camera_on_gui(
    mut contexts: Query<&mut bevy_egui::EguiContext>,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut cameras: Query<&mut PanOrbitCamera>,
    mut cursors: Query<&mut bevy::window::CursorOptions, With<bevy::window::PrimaryWindow>>,
    mut prev_gui_wants: Local<bool>,
) {
    let mut gui_wants = false;
    for mut ctx in contexts.iter_mut() {
        let ctx = ctx.get_mut();
        gui_wants |= ctx.egui_wants_pointer_input();
    }
    let dragging = mouse.any_pressed([MouseButton::Right, MouseButton::Middle]);
    let enable = dragging || (!gui_wants && !*prev_gui_wants);
    *prev_gui_wants = gui_wants;
    // Only while no camera button is down, so a held drag keeps the
    // meaning it started with (#1242 f166).
    let rebind = (!dragging).then(|| {
        pan_binding(keyboard.pressed(KeyCode::AltLeft) || keyboard.pressed(KeyCode::AltRight))
    });
    for mut cam in cameras.iter_mut() {
        // Manual change-detect: writing every frame would dirty the
        // component and defeat the crate's own change tracking.
        if cam.enabled != enable {
            cam.enabled = enable;
        }
        if let Some((button_pan, modifier_pan)) = rebind
            && (cam.button_pan != button_pan || cam.modifier_pan != modifier_pan)
        {
            cam.button_pan = button_pan;
            cam.modifier_pan = modifier_pan;
        }
    }
    // The cursor treatment follows an ENABLED drag: a right-drag that egui
    // owns (resizing a window) must not confine the pointer.
    let (grab, visible) = drag_cursor_grab(dragging && enable);
    for mut cursor in cursors.iter_mut() {
        if cursor.grab_mode != grab {
            cursor.grab_mode = grab;
        }
        if cursor.visible != visible {
            cursor.visible = visible;
        }
    }
}

/// How (whether) the orbit camera avoids the terrain (#872). Referenced
/// from [`crate::state::LocalSettings`], persisted machine-locally.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum CameraGroundAvoidance {
    /// No clamping - the camera may dip under terrain when orbiting low.
    Off,
    /// Keep the CAMERA's own position above ground + clearance; terrain
    /// between the avatar and the camera may occlude the view but never
    /// pulls the camera in. The default since #872: the original
    /// whole-ray check zoomed in aggressively at near-horizontal pitch
    /// (the focus sits ~1 m over ground, so early ray samples hug the
    /// clearance line) and across intermediate ridges, both with the
    /// camera itself nowhere near the ground.
    #[default]
    CameraOnly,
    /// The pre-#872 behavior: additionally pull in whenever any point of
    /// the focus→camera ray dips under the clearance line, so terrain
    /// never occludes the avatar.
    FullRay,
}

impl CameraGroundAvoidance {
    /// Settings-picker label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::CameraOnly => "Camera",
            Self::FullRay => "Camera + view",
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
    clearance: f32,
    mut ground_y: impl FnMut(f32, f32) -> f32,
) -> f32 {
    let steps = cfg::TERRAIN_CLAMP_SAMPLES;
    let step = dist / steps as f32;
    for i in 1..=steps {
        let t = step * i as f32;
        let p = focus + dir * t;
        if p.y < ground_y(p.x, p.z) + clearance {
            return (t - step).max(cfg::TERRAIN_CLAMP_MIN_DIST);
        }
    }
    dist
}

/// Largest distance ≤ `dist` at which the CAMERA position itself clears
/// ground + `clearance` (#872, [`CameraGroundAvoidance::CameraOnly`]):
/// walk inward from the desired distance and stop at the first clear
/// sample. A desired position already in the clear returns `dist`
/// untouched - the camera is never pulled in while it has headroom,
/// which is exactly the false positive the whole-ray check suffered.
fn clamp_distance_camera_only(
    focus: Vec3,
    dir: Vec3,
    dist: f32,
    clearance: f32,
    mut ground_y: impl FnMut(f32, f32) -> f32,
) -> f32 {
    let steps = cfg::TERRAIN_CLAMP_SAMPLES;
    let step = dist / steps as f32;
    let mut t = dist;
    while t > cfg::TERRAIN_CLAMP_MIN_DIST {
        let p = focus + dir * t;
        if p.y >= ground_y(p.x, p.z) + clearance {
            return t;
        }
        t -= step;
    }
    cfg::TERRAIN_CLAMP_MIN_DIST
}

/// Pull the camera in along its focus→camera ray when it would dip under
/// the terrain (#853) - orbiting low or zooming out over a slope used to
/// show the world's underside. Runs after `PanOrbitCameraSystemSet` has
/// written the camera `Transform` and only rewrites `translation`:
/// sliding along the ray toward the focus preserves the exact look
/// direction, and the crate recomputes the transform from its own
/// yaw/pitch/radius state next frame, so no feedback loop forms.
fn clamp_camera_to_terrain(
    heightmap: Option<Res<FinishedHeightMap>>,
    settings: Res<crate::state::LocalSettings>,
    mut cameras: Query<(&PanOrbitCamera, &mut Transform), IsWorldCamera>,
) {
    let Some(hm) = heightmap else {
        return;
    };
    let mode = settings.camera_ground_avoidance;
    if mode == CameraGroundAvoidance::Off {
        return;
    }
    for (cam, mut tf) in cameras.iter_mut() {
        let clear = clear_of_terrain(cam.focus, tf.translation, &settings, &hm);
        // Written only when it moves, as before: a write stamps the change
        // tick whether or not the value changed.
        if clear != tf.translation {
            tf.translation = clear;
        }
    }
}

/// Where an orbit camera at `translation` around `focus` may stand, under
/// the ground-avoidance the player chose: slid in along its focus ray until
/// it clears the terrain, or left where it is. The one rule for the player's
/// camera and for any view that has to stand where the player's would - the
/// agent's third-person snapshot (#1420).
pub(crate) fn clear_of_terrain(
    focus: Vec3,
    translation: Vec3,
    settings: &crate::state::LocalSettings,
    hm: &FinishedHeightMap,
) -> Vec3 {
    // Sanitized here rather than trusting the prefs file: a hand-edited
    // clearance of NaN/negative would otherwise poison every clamp.
    let clearance = settings.camera_ground_clearance_m.clamp(0.0, 50.0);
    let offset = translation - focus;
    let dist = offset.length();
    if dist <= cfg::TERRAIN_CLAMP_MIN_DIST {
        return translation;
    }
    let dir = offset / dist;
    // The ground as drawn, a Berlin region's far field included (#1585).
    let ground = |x: f32, z: f32| hm.world_height_at(x, z);
    let clamped = match settings.camera_ground_avoidance {
        CameraGroundAvoidance::Off => dist,
        CameraGroundAvoidance::CameraOnly => {
            clamp_distance_camera_only(focus, dir, dist, clearance, ground)
        }
        CameraGroundAvoidance::FullRay => {
            clamp_distance_along_ray(focus, dir, dist, clearance, ground)
        }
    };
    if clamped < dist {
        focus + dir * clamped
    } else {
        translation
    }
}

/// The camera the player looks through - the one that means "the camera"
/// everywhere else in the crate (#1300).
///
/// Every system that asks where the view is - movement's forward vector,
/// the gizmo's pick ray, a nametag's projection, the drop raycast, the
/// skybox and cloud deck that follow the eye - used to identify it as
/// `With<Camera3d>`, which was correct only for as long as the app had
/// exactly ONE `Camera3d`. #1288's item preview added a second, and
/// **every one of those queries broke at once and silently**: eleven of
/// them resolve with `single()`, which then returns `Err(MultipleEntities)`
/// and falls through to a default - so avatar movement quietly switched
/// from camera-relative to absolute world axes, and the scene context menu
/// stopped opening at all.
///
/// So the identification is positive now: a query names this marker, and a
/// camera that is not the player's view cannot answer by accident. The
/// rule is enforced by the `every_camera_query_says_which_camera` scan -
/// which exists because the failure mode here is a silent fallback, not a
/// panic, and a third camera would have cost another sitting to find.
///
/// The same accident has a second face (#1317). bevy_egui, left to its
/// default, hangs its primary context on the FIRST camera it meets, and
/// with two cameras spawning in `Startup` that is a coin toss the wasm
/// build lost: egui drew through the inactive preview camera, and every
/// login surface vanished without an error. So the egui context is named
/// on this camera's spawn as well, and [`egui_global_settings`] turns the
/// automatic pick off.
#[derive(Component)]
pub struct WorldCamera;

/// Query filter for "the player's view": [`WorldCamera`] and nothing else.
///
/// An alias rather than the pair spelled out at each of the twelve call
/// sites, because clippy's `type_complexity` is right about what those
/// signatures had become - and because one name is one place to change if
/// the app ever grows a second legitimate world view (a portal, a
/// mirror). Compose it where a site needs more:
/// `(IsWorldCamera, Without<SkyBox>)`.
pub type IsWorldCamera = (With<Camera3d>, With<WorldCamera>);

/// bevy_egui's global settings, with its automatic primary-context pick
/// turned OFF (#1317).
///
/// The default hands `PrimaryEguiContext` to the first entity an
/// `Added<Camera>` query yields on the first frame. This crate spawns two
/// cameras in `Startup` with no ordering between them - the world camera
/// below and #1288's item-preview camera - so which one egui draws through
/// was decided by archetype order. Native usually got the world camera; the
/// wasm build got the preview camera, which starts inactive, so bevy_egui
/// dropped the view before its pass ever ran. Nothing panicked and nothing
/// logged: the UI systems kept running, laid out for a 256 px screen, and
/// drew into nowhere. Two sessions went into finding that.
///
/// The remedy bevy_egui documents is the one applied: no automatic pick,
/// and [`PrimaryEguiContext`] spelled out on the camera that means it. The
/// other half - a camera spawn is explicit about egui or gets none - is
/// held by `the_world_camera_owns_the_egui_context_whichever_camera_spawns_first`.
pub fn egui_global_settings() -> EguiGlobalSettings {
    EguiGlobalSettings {
        auto_create_primary_context: false,
        ..default()
    }
}

/// The world camera's perspective lens: Bevy's default 45 degrees, its far
/// plane past a Berlin region's horizon ([`cfg::FAR_PLANE_M`]).
pub(crate) fn perspective_lens() -> PerspectiveProjection {
    PerspectiveProjection {
        far: cfg::FAR_PLANE_M,
        ..default()
    }
}

/// How tall a slice of the world [`perspective_lens`] shows a metre in
/// front of it (#1603): `2 tan(fov / 2)`, 0.83 for Bevy's 45 degrees.
fn height_per_metre() -> f32 {
    2.0 * (PerspectiveProjection::default().fov / 2.0).tan()
}

/// The world camera's orthographic lens (#1603), framing at its focus what
/// [`perspective_lens`] frames from `distance` away: its `scale` IS that
/// distance. `bevy_panorbit_camera` writes an orthographic camera's orbit
/// radius into its scale, so the radius keeps its meaning under either
/// lens - the zoom limits, the shadow reach and the AI agent's camera
/// commands with it - and a switch between the lenses keeps the view.
pub(crate) fn orthographic_lens(distance: f32) -> OrthographicProjection {
    OrthographicProjection {
        near: -cfg::ORTHO_BACK_SHARE * distance,
        far: cfg::FAR_PLANE_M,
        scaling_mode: ScalingMode::FixedVertical {
            viewport_height: height_per_metre(),
        },
        scale: distance,
        ..OrthographicProjection::default_3d()
    }
}

/// Give the world camera the lens the player chose (#1603): perspective, or
/// [`orthographic_lens`] at the orbit's radius, under which the orbit may
/// look no less than [`cfg::ORTHO_PITCH_LOWER_LIMIT`] down - the crate eases
/// a lower pitch up to it. Runs before the orbit crate, which lays the new
/// lens out on the same frame (`force_update`). Writes nothing while the
/// lens already matches.
fn follow_projection_setting(
    settings: Res<crate::state::LocalSettings>,
    mut cameras: Query<(&mut Projection, &mut PanOrbitCamera), IsWorldCamera>,
) {
    let orthographic = settings.orthographic_camera;
    for (mut projection, mut orbit) in cameras.iter_mut() {
        if matches!(*projection, Projection::Orthographic(_)) == orthographic {
            continue;
        }
        let radius = orbit.radius.unwrap_or(cfg::ORBIT_RADIUS);
        *projection = if orthographic {
            Projection::Orthographic(orthographic_lens(radius))
        } else {
            Projection::Perspective(perspective_lens())
        };
        orbit.pitch_lower_limit = Some(if orthographic {
            cfg::ORTHO_PITCH_LOWER_LIMIT
        } else {
            cfg::PITCH_LOWER_LIMIT
        });
        orbit.force_update = true;
    }
}

/// Stand an orthographic world camera where a perspective one framing the
/// same view would stand (#1603): its lens's scale - the orbit's radius -
/// from its focus, back along its view. The orbit crate parks an
/// orthographic camera halfway to its far plane, kilometres off, and every
/// distance taken from the camera would follow it there: draw distances,
/// the hair's far tier, the fog, the shadow cascades, the listening ear.
///
/// Only a camera FARTHER than that from its focus is moved: the crate has
/// just parked it, having moved. One nearer is where the terrain clamp
/// pulled it in, as a perspective camera stays pulled in until the orbit
/// next moves. The lens's near plane follows its scale, which the crate
/// writes and leaves the near plane behind ([`cfg::ORTHO_BACK_SHARE`]).
/// Writes only what moved.
fn stand_orthographic_camera(
    mut cameras: Query<(&PanOrbitCamera, &mut Projection, &mut Transform), IsWorldCamera>,
) {
    for (orbit, mut projection, mut transform) in cameras.iter_mut() {
        let Projection::Orthographic(lens) = &*projection else {
            continue;
        };
        let scale = lens.scale;
        let near = -cfg::ORTHO_BACK_SHARE * scale;
        let stale_near = lens.near != near;
        if stale_near && let Projection::Orthographic(lens) = &mut *projection {
            lens.near = near;
        }
        if transform.translation.distance(orbit.focus) > scale + 1e-3 {
            transform.translation = orbit.focus + transform.rotation * Vec3::Z * scale;
        }
    }
}

/// The world camera's atmospheric haze at the config defaults - what the
/// camera spawns with before the first room's `Environment` re-tints it
/// (`world_builder::compile::apply_environment_state` patches every
/// `DistanceFog` it finds). Shared with the headless render tool's `--world`
/// camera, so a world sheet starts from the same air the game does.
pub(crate) fn default_distance_fog() -> DistanceFog {
    let fc = cfg::fog::COLOR;
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
    }
}

fn spawn_orbit_camera(mut commands: Commands) {
    let pos = cfg::INITIAL_POS;
    commands.spawn((
        Camera3d::default(),
        WorldCamera,
        // #1317: the primary egui context lives HERE, by name. See
        // `egui_global_settings` for why it must not be left to bevy_egui.
        PrimaryEguiContext,
        // WebGL2's `glow` backend has no `tex_storage_2d_multisample`
        // entrypoint, so Bevy's default `Msaa::Sample4` panics during
        // render-target allocation as soon as the first frame renders
        // (panicked at glow-0.16.0/.../web_sys.rs: "Tex storage 2D
        // multisample is not supported"). Native and WebGPU paths handle
        // MSAA fine; only WebGL2 needs the opt-out. Disabling on every
        // wasm build is the safe superset - modern browsers exposing
        // WebGPU still work with MSAA off, and we don't depend on
        // anti-aliased edges anywhere visually critical.
        #[cfg(target_arch = "wasm32")]
        Msaa::Off,
        // Bevy's default perspective far plane is 1000 m, which clips the
        // cloud-deck plane (at altitude ~250 m, half-extent 4 km) before
        // the shader's horizon-fade has a chance to dissolve it, and a
        // Berlin region's far field and sky reach far past that (#1585).
        // The projection is infinite reverse-Z, so the far plane bounds
        // frustum culling only and costs no depth precision; see
        // `FAR_PLANE_M`.
        Projection::from(perspective_lens()),
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
        // path uses `textureLoad` on a depth texture - which naga's GLSL
        // backend rejects with "WGSL `textureLoad` from depth textures is
        // not supported in GLSL", panicking pipeline creation for every
        // alpha-blend PBR material (cloud, water). The shoreline-foam
        // block in water.wgsl is the only consumer in this codebase and
        // is already `#ifdef DEPTH_PREPASS`-guarded, so omitting the
        // component on wasm32 cleanly disables the feature - shore foam
        // is the only visual loss on WebGL2, and only on water bodies
        // whose room record sets `shore_foam_width > 0`.
        #[cfg(not(target_arch = "wasm32"))]
        DepthPrepass,
        GizmoCamera,
        PanOrbitCamera {
            radius: Some(cfg::ORBIT_RADIUS),
            pitch: Some(cfg::ORBIT_PITCH),
            button_orbit: MouseButton::Right,
            // Re-bound per frame by `gate_camera_on_gui` so Alt+right-drag
            // pans too (#1242 f166) - this is the no-modifier resting
            // state it returns to.
            button_pan: MouseButton::Middle,
            // Two fingers on a trackpad are the other half of the same
            // gap: without this a laptop had middle-drag pan it could not
            // perform AND no pinch zoom.
            trackpad_pinch_to_zoom_enabled: true,
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
        default_distance_fog(),
        Bloom::NATURAL, // Enable Bloom
        // Spatial-audio listener for every positional voice (#262); inert
        // for non-spatial audio, so this is purely additive.
        camera_listener(),
    ));
}

/// The camera's spatial listener: ears a head-width apart (Bevy's 4 m default
/// over-pans), handed to rodio the other way round (#1561).
///
/// rodio 0.22's `Spatial`, the source bevy_audio plays every positional voice
/// through, gives the larger gain to the ear FARTHER from the sound (its
/// `source/spatial.rs`, the two `*_diff_modifier`s). Given the ears the right
/// way round, every construct hum, engine and contact cue on the listener's
/// right played up to 6 dB louder on its left. Given them swapped, each sound
/// is back on its own side by the same margin. 0.22.2 is rodio's latest
/// release; were it to fix the sign, the swap would mirror every sound again,
/// and `a_sound_on_the_right_is_louder_in_the_right_ear`, which plays a tone
/// through rodio's own `Spatial`, fails then.
pub(crate) fn camera_listener() -> SpatialListener {
    let mut listener = SpatialListener::new(audio_cfg::LISTENER_EAR_GAP);
    std::mem::swap(
        &mut listener.left_ear_offset,
        &mut listener.right_ear_offset,
    );
    listener
}

/// Keep the orbit camera glued to the local chassis.
///
/// Reads the chassis `Transform`, not `GlobalTransform` (#670): the root
/// is parentless, and the avatar's interpolation easing writes the
/// smoothed pose to `Transform` in `RunFixedMainLoop`, before `Update`,
/// so this system sees the *same-frame eased* pose. `GlobalTransform` is
/// only refreshed by `PostUpdate` propagation (and by Avian just before
/// each fixed step), so it lags a frame and its staleness oscillates at
/// the fixed-vs-refresh beat - feeding it into the focus lerp was the
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
    // heading is genuinely undefined - freeze yaw inheritance there
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

    /// The orthographic lens frames, at its focus, what the perspective
    /// lens frames from its scale away (#1603): a point half the framed
    /// height above the focus lands on the top edge of the view under both,
    /// at the closest zoom, at rest and at the farthest.
    #[test]
    fn the_orthographic_lens_frames_what_the_perspective_lens_does() {
        use bevy::camera::CameraProjection;
        for distance in [
            cfg::ZOOM_LOWER_LIMIT,
            cfg::ORBIT_RADIUS,
            cfg::ZOOM_UPPER_LIMIT,
        ] {
            let mut orthographic = orthographic_lens(distance);
            orthographic.update(1920.0, 1080.0);
            let mut perspective = perspective_lens();
            perspective.update(1920.0, 1080.0);
            let top = Vec4::new(0.0, distance * height_per_metre() / 2.0, -distance, 1.0);
            for (lens, clip) in [
                ("orthographic", orthographic.get_clip_from_view()),
                ("perspective", perspective.get_clip_from_view()),
            ] {
                let ndc = clip * top;
                let y = ndc.y / ndc.w;
                assert!((y - 1.0).abs() < 1e-4, "{lens} at {distance} m: {y}");
            }
        }
    }

    /// An orthographic camera's view reaches back far enough behind it that,
    /// looking down at the pitch floor on flat ground, the foot of the frame
    /// starts above the focus - not under the ground, where the frame showed
    /// a band of sky (#1603, the critic's finding) - at the closest zoom, at
    /// rest and at the farthest.
    #[test]
    fn an_orthographic_view_clears_the_ground_at_the_pitch_floor() {
        use bevy::camera::CameraProjection;
        let pitch = cfg::ORTHO_PITCH_LOWER_LIMIT;
        for distance in [
            cfg::ZOOM_LOWER_LIMIT,
            cfg::ORBIT_RADIUS,
            cfg::ZOOM_UPPER_LIMIT,
        ] {
            let mut lens = orthographic_lens(distance);
            lens.update(1920.0, 1080.0);
            let focus = Vec3::new(0.0, 1.0, 0.0);
            let eye = focus + Vec3::new(0.0, pitch.sin(), pitch.cos()) * distance;
            let camera = Transform::from_translation(eye).looking_at(focus, Vec3::Y);
            // The near face's bottom edge, at view depth `-near`.
            let foot = camera.transform_point(Vec3::new(0.0, lens.area.min.y, -lens.near));
            assert!(
                foot.y > focus.y,
                "at {distance} m the view starts {} m under the focus",
                focus.y - foot.y
            );
        }
    }

    /// Choosing the orthographic camera swaps in its lens at the orbit's
    /// radius, raises the pitch floor and has the orbit lay it out at once;
    /// choosing perspective again puts both back (#1603). Nothing is written
    /// while the lens already matches.
    #[test]
    fn the_lens_follows_the_setting() {
        let mut app = App::new();
        app.insert_resource(crate::state::LocalSettings::default());
        app.add_systems(Update, follow_projection_setting);
        let camera = app
            .world_mut()
            .spawn((
                WorldCamera,
                Projection::Perspective(perspective_lens()),
                PanOrbitCamera {
                    radius: Some(30.0),
                    pitch_lower_limit: Some(cfg::PITCH_LOWER_LIMIT),
                    ..default()
                },
            ))
            .id();
        app.update();
        let changed = |app: &App| {
            app.world()
                .entity(camera)
                .get_ref::<Projection>()
                .expect("a lens")
                .last_changed()
        };
        let before = changed(&app);
        app.update();
        assert_eq!(changed(&app), before, "a matching lens is not written");

        app.world_mut()
            .resource_mut::<crate::state::LocalSettings>()
            .orthographic_camera = true;
        app.update();
        let Some(Projection::Orthographic(lens)) = app.world().get::<Projection>(camera) else {
            panic!("an orthographic lens");
        };
        assert_eq!(lens.scale, 30.0, "at the orbit's radius");
        let orbit = app.world().get::<PanOrbitCamera>(camera).expect("an orbit");
        assert_eq!(orbit.pitch_lower_limit, Some(cfg::ORTHO_PITCH_LOWER_LIMIT));
        assert!(orbit.force_update, "laid out on the frame it is chosen");

        app.world_mut()
            .get_mut::<PanOrbitCamera>(camera)
            .expect("an orbit")
            .force_update = false;
        app.world_mut()
            .resource_mut::<crate::state::LocalSettings>()
            .orthographic_camera = false;
        app.update();
        assert!(matches!(
            app.world().get::<Projection>(camera),
            Some(Projection::Perspective(_))
        ));
        let orbit = app.world().get::<PanOrbitCamera>(camera).expect("an orbit");
        assert_eq!(orbit.pitch_lower_limit, Some(cfg::PITCH_LOWER_LIMIT));
        assert!(orbit.force_update);
    }

    /// An orthographic world camera the orbit crate has parked halfway to
    /// its far plane stands its lens's scale from its focus, back along its
    /// view, as a perspective one would (#1603); one the terrain clamp has
    /// pulled in is left there; a perspective camera is the crate's alone.
    #[test]
    fn an_orthographic_camera_stands_where_a_perspective_one_would() {
        let mut app = App::new();
        app.add_systems(Update, stand_orthographic_camera);
        let focus = Vec3::new(5.0, 2.0, -3.0);
        let rotation = Quat::from_euler(EulerRot::YXZ, 0.7, -0.5, 0.0);
        let parked = focus + rotation * Vec3::Z * (cfg::FAR_PLANE_M / 2.0);
        let camera = app
            .world_mut()
            .spawn((
                WorldCamera,
                Projection::Orthographic(orthographic_lens(40.0)),
                PanOrbitCamera { focus, ..default() },
                Transform {
                    translation: parked,
                    rotation,
                    ..default()
                },
            ))
            .id();
        app.update();
        let at = |app: &App| *app.world().get::<Transform>(camera).expect("placed");
        let stand = focus + rotation * Vec3::Z * 40.0;
        assert!(
            at(&app).translation.distance(stand) < 1e-2,
            "{}",
            at(&app).translation
        );
        assert_eq!(at(&app).rotation, rotation, "its view is the crate's");

        let pulled = focus + rotation * Vec3::Z * 15.0;
        app.world_mut()
            .get_mut::<Transform>(camera)
            .expect("placed")
            .translation = pulled;
        app.update();
        assert_eq!(at(&app).translation, pulled, "the clamp's pull is kept");

        // The crate zooms out by writing the scale alone: the near plane
        // follows it.
        if let Some(Projection::Orthographic(lens)) =
            app.world_mut().get_mut::<Projection>(camera).as_deref_mut()
        {
            lens.scale = 60.0;
        }
        app.update();
        let Some(Projection::Orthographic(lens)) = app.world().get::<Projection>(camera) else {
            panic!("an orthographic lens");
        };
        assert_eq!(lens.near, -cfg::ORTHO_BACK_SHARE * 60.0);

        *app.world_mut()
            .get_mut::<Projection>(camera)
            .expect("a lens") = Projection::Perspective(perspective_lens());
        app.world_mut()
            .get_mut::<Transform>(camera)
            .expect("placed")
            .translation = parked;
        app.update();
        assert_eq!(at(&app).translation, parked, "perspective is the crate's");
    }

    /// #1561: a sound to the listener's right is louder in its right ear, one
    /// to its left in its left, and one straight ahead in neither.
    ///
    /// Played through rodio's own `Spatial` - what bevy_audio wraps every
    /// positional voice in - with the ears where bevy_audio puts them: the
    /// camera listener's offsets through the camera's transform, handed over
    /// left then right (bevy_audio's `audio_output.rs`). The camera stands
    /// turned and off the origin so the transform is part of what is checked.
    /// rodio 0.22 gives the larger gain to the farther ear, which is why the
    /// listener's ears are swapped; a rodio that fixed its sign fails this.
    /// bevy_audio's placement is written out here rather than run - it needs
    /// an audio device - so a bevy that changed the order it hands the ears
    /// over in would not fail it: re-read `audio_output.rs` at a Bevy bump.
    #[test]
    fn a_sound_on_the_right_is_louder_in_the_right_ear() {
        use rodio::Source as _;
        use std::num::NonZero;

        let listener = camera_listener();
        let camera = GlobalTransform::from(
            Transform::from_xyz(3.0, 2.0, -4.0).looking_to(Vec3::new(1.0, 0.0, 1.0), Vec3::Y),
        );
        let left_ear = camera.transform_point(listener.left_ear_offset);
        let right_ear = camera.transform_point(listener.right_ear_offset);
        // (left, right) as rodio plays a steady tone from `emitter`.
        let heard = |emitter: Vec3| -> (f32, f32) {
            let tone = rodio::buffer::SamplesBuffer::new(
                NonZero::new(1).expect("one channel"),
                NonZero::new(22_050).expect("a rate"),
                vec![1.0; 8],
            );
            let mut spatial = rodio::source::Spatial::new(
                tone,
                emitter.to_array(),
                left_ear.to_array(),
                right_ear.to_array(),
            );
            assert_eq!(spatial.channels().get(), 2);
            let left = spatial.next().expect("a left sample");
            let right = spatial.next().expect("a right sample");
            (left, right)
        };

        let centre = camera.translation();
        for metres in [1.0, 5.0, 20.0] {
            let (left, right) = heard(centre + camera.right() * metres);
            assert!(
                right > left,
                "{metres} m to the right: left {left}, right {right}"
            );
            let (left, right) = heard(centre + camera.left() * metres);
            assert!(
                left > right,
                "{metres} m to the left: left {left}, right {right}"
            );
        }
        let (left, right) = heard(centre + camera.forward() * 5.0);
        assert!(
            (left - right).abs() <= 1e-6 * left.abs().max(right.abs()),
            "straight ahead is centred: left {left}, right {right}"
        );
    }

    /// Strip `//` line comments, so the scan below does not read its own
    /// prose - this module explains the rule using the very needle it
    /// bans, and every marker doc in the crate names it too.
    ///
    /// A `//` inside a string literal would over-strip. That costs
    /// coverage, never a false pass, which is the trade every lexer-lite
    /// scan in this crate makes.
    fn without_comments(source: &str) -> String {
        source
            .lines()
            .map(|line| line.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Every `Query<...>` type in `source`, as balanced angle-bracket
    /// text.
    ///
    /// Whole types, not lines: half these queries are multi-line now, and
    /// a line-window scan would read `With<Camera3d>,` on its own and see
    /// none of the filter it belongs to. That is the shape of scan that
    /// has lied here before.
    fn query_types(source: &str) -> Vec<String> {
        let code = without_comments(source);
        let chars: Vec<char> = code.chars().collect();
        let mut out = Vec::new();
        let mut from = 0usize;
        while let Some(rel) = code[from..].find("Query<") {
            // Byte offset -> char index: the sources are ASCII in type
            // position, but the prose around them is not.
            let start = code[..from + rel].chars().count() + "Query".len();
            let mut depth = 0i32;
            let mut end = start;
            for (i, c) in chars.iter().enumerate().skip(start) {
                match c {
                    '<' => depth += 1,
                    '>' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if end > start {
                out.push(chars[start..=end].iter().collect());
            }
            from += rel + "Query<".len();
        }
        out
    }

    /// #1300. Every query that identifies a camera by `Camera3d` must also
    /// say WHICH camera it means.
    ///
    /// The bug this exists to stop is not a crash. #1288 added a second
    /// `Camera3d` for the item preview, and eleven queries that meant "the
    /// player's view" resolved with `single()` - which quietly began
    /// returning `Err(MultipleEntities)` and falling through to a default.
    /// Avatar movement switched from camera-relative to absolute world
    /// axes; the scene context menu stopped opening; nametags, the drop
    /// raycast, the skybox and the cloud deck all went with them. Nothing
    /// panicked and nothing logged.
    ///
    /// A query that genuinely wants every camera in the world has to say
    /// so by naming a marker anyway - there is no silent third option, and
    /// that is the whole point.
    #[test]
    fn every_camera_query_says_which_camera() {
        // `IsWorldCamera` contains `WorldCamera`, so the alias satisfies
        // this by name as well as by meaning. `LookCamera` is the agent's
        // snapshot camera (#1420), a third `Camera3d` for a few frames.
        let markers = ["WorldCamera", "PreviewCamera", "LookCamera"];
        let mut unmarked: Vec<String> = Vec::new();
        let mut seen = 0usize;
        for path in crate::ui::fonts::glyph_coverage_tests::rust_sources_under("src") {
            let source = std::fs::read_to_string(&path).expect("source readable");
            let code = crate::ui::fonts::glyph_coverage_tests::non_test_source(&source);
            for query in query_types(code) {
                // RE-POINTED, not lowered (#1300). The sites used to
                // spell `With<Camera3d>` out; clippy's `type_complexity`
                // pushed them behind `IsWorldCamera`, so keying only on
                // `Camera3d` would have found one query - the alias - and
                // called the crate clean. A query is "about a camera" if
                // it names the component OR any of the answers, and it
                // passes only by naming an answer.
                let about_a_camera =
                    query.contains("Camera3d") || markers.iter().any(|m| query.contains(m));
                if !about_a_camera {
                    continue;
                }
                seen += 1;
                if markers.iter().any(|m| query.contains(m)) {
                    continue;
                }
                let rel = path.display().to_string();
                let rel = rel.rsplit_once("src/").map(|(_, r)| r).unwrap_or(&rel);
                unmarked.push(format!("src/{rel}: {}", query.replace('\n', " ")));
            }
        }
        assert!(
            unmarked.is_empty(),
            "these queries pick a camera by Camera3d alone, so a second \
             camera answers them too. Name the one you mean ({}):\n  {}",
            markers.join(" or "),
            unmarked.join("\n  ")
        );
        // A FLOOR, not a count. The scan's failure mode is reading
        // nothing - a change to the lexer, or a query wrapped in a shape
        // it does not recognise, and every site passes because none was
        // found. There were twelve when this was written; if the number
        // drops, re-point the scan rather than lowering the floor.
        assert!(
            seen >= 12,
            "the scan found only {seen} camera queries and has gone blind"
        );
    }

    /// #1300, the other half: the scan above is only worth its run if it
    /// can actually see a violation. A marker-less query is exactly what
    /// shipped, so this proves the lexer finds one across the line breaks
    /// the real sites are wrapped over.
    #[test]
    fn the_camera_scan_sees_a_query_that_names_no_camera() {
        let offender = "fn s(cameras: Query<\n    (&Camera, &GlobalTransform),\n    \
                        With<Camera3d>,\n>) {}";
        let found = query_types(offender);
        assert_eq!(found.len(), 1, "one query, read whole: {found:?}");
        assert!(found[0].contains("Camera3d"), "the filter came with it");
        assert!(
            !found[0].contains("WorldCamera"),
            "and it names no camera, which is the failure"
        );

        let fixed = "fn s(cameras: Query<\n    (&Camera, &GlobalTransform),\n    \
                     (With<Camera3d>, With<WorldCamera>),\n>) {}";
        assert!(query_types(fixed)[0].contains("WorldCamera"));

        // And the shape the crate actually ships: the component name is
        // gone, absorbed into the alias, and the scan must still count it.
        let aliased = "fn s(cameras: Query<&GlobalTransform, IsWorldCamera>) {}";
        assert!(
            query_types(aliased)[0].contains("WorldCamera"),
            "IsWorldCamera must satisfy the rule by name, or every real \
             call site goes uncounted"
        );
    }

    /// #670 guard: the follow target must come from `Transform` - the
    /// same-frame eased pose - not `GlobalTransform`. `MinimalPlugins`
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

    /// #853: mid-loop (forward near-vertical) the heading is undefined -
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

        let clamped =
            clamp_distance_along_ray(focus, dir, dist, cfg::TERRAIN_CLEARANCE, flat_ground);
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
            clamp_distance_along_ray(focus, up_dir, dist, cfg::TERRAIN_CLEARANCE, flat_ground),
            dist
        );
    }

    /// The #872 camera-only mode: a camera with headroom is NEVER pulled
    /// in, regardless of what the ray between it and the focus crosses -
    /// the two false-positive modes of the whole-ray check.
    #[test]
    fn camera_only_clamp_ignores_terrain_under_the_ray() {
        // Focus just over flat ground (an avatar), camera high up: a
        // ridge under the middle of the ray dips the whole-ray check but
        // must not move the camera-only one.
        let focus = Vec3::new(0.0, 6.0, 0.0);
        let dir = Vec3::new(0.6, 0.8, 0.0).normalize();
        let dist = 20.0;
        // Ridge at x ∈ [4, 8] towering to y = 14; flat y = 5 elsewhere.
        let ridged = |x: f32, _z: f32| if (4.0..=8.0).contains(&x) { 14.0 } else { 5.0 };
        let camera = focus + dir * dist;
        assert!(camera.y > 14.0 + cfg::TERRAIN_CLEARANCE, "camera is clear");
        assert_eq!(
            clamp_distance_camera_only(focus, dir, dist, cfg::TERRAIN_CLEARANCE, ridged),
            dist,
            "a clear camera must not be pulled in by ray-intermediate terrain"
        );
        assert!(
            clamp_distance_along_ray(focus, dir, dist, cfg::TERRAIN_CLEARANCE, ridged) < dist,
            "sanity: the whole-ray check DOES clamp on the same scene"
        );
    }

    #[test]
    fn camera_only_clamp_pulls_in_a_buried_camera_minimally() {
        // Descending ray: the desired camera position is under ground;
        // the clamp walks inward only as far as the first clear sample.
        let focus = Vec3::new(0.0, 20.0, 0.0);
        let dir = Vec3::new(
            std::f32::consts::FRAC_1_SQRT_2,
            -std::f32::consts::FRAC_1_SQRT_2,
            0.0,
        );
        let dist = 24.0;
        let flat_ground = |_x: f32, _z: f32| 5.0;
        let clamped =
            clamp_distance_camera_only(focus, dir, dist, cfg::TERRAIN_CLEARANCE, flat_ground);
        assert!(clamped < dist, "a buried camera must be pulled in");
        let p = focus + dir * clamped;
        assert!(
            p.y >= 5.0 + cfg::TERRAIN_CLEARANCE - 1e-4,
            "clamped camera {p:?} is below the clearance line"
        );

        // Fully buried ray (focus below clearance, pointing down): the
        // clamp bottoms out at the minimum distance rather than looping.
        let sunk = clamp_distance_camera_only(
            Vec3::new(0.0, 4.0, 0.0),
            Vec3::NEG_Y,
            dist,
            cfg::TERRAIN_CLEARANCE,
            flat_ground,
        );
        assert_eq!(sunk, cfg::TERRAIN_CLAMP_MIN_DIST);
    }

    /// #1242 f166. Sequence: a laptop trackpad user reads
    /// "Middle-drag - pan camera" and has no middle button. The obvious
    /// fix - `modifier_pan: Some(AltLeft)` - makes it WORSE: the crate's
    /// `pan_pressed` is `modifier && pressed(button_pan)`, so plain
    /// middle-drag would stop panning and Alt+MIDDLE would be asked for
    /// instead. Moving the button under the modifier is what actually
    /// adds Alt+right-drag, and `orbit_pressed`'s `!modifier_pan.pressed()`
    /// is what keeps the two off each other.
    #[test]
    fn alt_moves_pan_onto_the_right_button_and_releases_it_again() {
        assert_eq!(pan_binding(false), (MouseButton::Middle, None));
        assert_eq!(
            pan_binding(true),
            (MouseButton::Right, Some(KeyCode::AltLeft))
        );
        // The resting state must carry NO modifier, or middle-drag - the
        // binding the sheet has always advertised - stops working.
        assert_eq!(pan_binding(false).1, None);
    }

    /// #1242 f171, as its refuter corrected it: the grab is a BROWSER fix
    /// (winit derives the web delta from `movementX/Y`, which goes to zero
    /// once the OS cursor pins at the screen edge; native motion is raw
    /// device motion and already unbounded). It must be off whenever no
    /// camera drag is live, or the pointer stays captured.
    #[test]
    fn the_cursor_is_only_captured_while_a_drag_is_live() {
        let (grab, visible) = drag_cursor_grab(false);
        assert_eq!(grab, bevy::window::CursorGrabMode::None);
        assert!(visible, "a released drag must give the pointer back");

        let (grab, visible) = drag_cursor_grab(true);
        assert_ne!(grab, bevy::window::CursorGrabMode::None);
        assert!(
            !visible,
            "a pointer visibly stuck at the screen edge while the view turns is its own lie"
        );
    }

    /// Which of the two `Startup` camera spawns applies first (#1317).
    #[derive(Clone, Copy, Debug)]
    enum SpawnOrder {
        WorldFirst,
        PreviewFirst,
    }

    /// The app's two cameras and bevy_egui's context picker, and nothing
    /// else - the shape `run()` ships, minus everything that needs a
    /// window. `MinimalPlugins` carries no render world, and no component
    /// either spawn puts on its camera needs one to exist.
    fn boot_with_two_cameras(order: SpawnOrder, settings: EguiGlobalSettings) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(settings)
            .init_resource::<Assets<Image>>()
            .init_resource::<bevy_egui::EguiUserTextures>();
        match order {
            SpawnOrder::WorldFirst => {
                app.add_systems(
                    Startup,
                    (spawn_orbit_camera, crate::item_preview::setup_preview).chain(),
                );
            }
            SpawnOrder::PreviewFirst => {
                app.add_systems(
                    Startup,
                    (crate::item_preview::setup_preview, spawn_orbit_camera).chain(),
                );
            }
        }
        // Registered exactly as `EguiPlugin` registers it, gate included.
        app.add_systems(
            PreUpdate,
            bevy_egui::setup_primary_egui_context_system
                .run_if(|s: Res<EguiGlobalSettings>| s.auto_create_primary_context),
        );
        app.update();
        app
    }

    /// `(is the world camera, is the preview camera)` for every entity
    /// that carries `PrimaryEguiContext` after the first frame.
    fn primary_context_holders(app: &mut App) -> Vec<(bool, bool)> {
        let mut holders = app.world_mut().query_filtered::<(
            Has<WorldCamera>,
            Has<crate::item_preview::PreviewCamera>,
        ), With<PrimaryEguiContext>>();
        holders.iter(app.world()).collect()
    }

    /// #1317. Exactly one primary egui context, on the world camera, no
    /// matter whose spawn lands first.
    #[test]
    fn the_world_camera_owns_the_egui_context_whichever_camera_spawns_first() {
        for order in [SpawnOrder::WorldFirst, SpawnOrder::PreviewFirst] {
            let mut app = boot_with_two_cameras(order, egui_global_settings());
            assert_eq!(
                primary_context_holders(&mut app),
                vec![(true, false)],
                "{order:?}: the primary egui context must sit on the world \
                 camera and nowhere else"
            );
        }
    }

    /// #1317, the control: with bevy_egui's default left on, the preview
    /// camera spawning first is enough to hand it a primary context. That
    /// is the shipped wasm failure - egui drawing through an inactive
    /// off-screen camera - so this is the proof that the test above can
    /// see the thing it guards against.
    #[test]
    fn auto_create_primary_context_is_a_spawn_order_race() {
        let mut app =
            boot_with_two_cameras(SpawnOrder::PreviewFirst, EguiGlobalSettings::default());
        let holders = primary_context_holders(&mut app);
        assert!(
            holders.iter().any(|&(_, preview)| preview),
            "with the automatic pick on, the preview camera should have won \
             a context of its own; holders: {holders:?}"
        );
    }
}
