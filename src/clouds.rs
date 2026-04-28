//! Cloud-deck `MaterialExtension` for the procedural sky-cloud layer.
//!
//! Renders a single horizontal `Plane3d` at altitude `Environment::cloud_height`
//! using a custom WGSL fragment shader (`assets/shaders/cloud.wgsl`) that
//! synthesises domain-warped FBM clouds, threshold-shaped by `cover`, softened
//! by `softness`, drifting with `wind_dir * speed`, lit by the directional sun
//! direction, and faded into the room's distance-fog colour at the horizon.
//!
//! Designed to run on WebGL2 — pure fragment work, no compute, no storage
//! textures, no prepass dependency. Mirrors the [`crate::water`] module's
//! `MaterialExtension` pattern so the world compiler can mutate uniforms in
//! place without rebuilding the material asset.

use bevy::{
    math::{Vec2, Vec4},
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

const CLOUD_SHADER_PATH: &str = "shaders/cloud.wgsl";

/// GPU uniform block shared with `cloud.wgsl`. `Vec4`s are placed first to
/// satisfy the 16-byte alignment WGSL imposes; scalars pack at the end.
/// `cloud_color`, `cloud_shadow_color`, `fog_color`, and `sun_dir` are
/// stored as `Vec4` rather than `Vec3` to dodge the 12-vs-16-byte alignment
/// pitfall — alpha channels are unused.
#[derive(Debug, Clone, Default, ShaderType)]
pub struct CloudUniforms {
    /// sRGB sunlit-top tint (rgb), alpha unused.
    pub color: Vec4,
    /// sRGB underside / shadowed tint (rgb), alpha unused.
    pub shadow_color: Vec4,
    /// sRGB horizon-fade target (the room's distance-fog colour), alpha unused.
    pub fog_color: Vec4,
    /// Unit world-space direction toward the sun (xyz), w unused. Patched
    /// from the runtime `DirectionalLight` transform every change tick.
    pub sun_dir: Vec4,
    /// 2D drift direction in world XZ (need not be unit length; the shader
    /// normalises an epsilon-padded copy).
    pub wind_dir: Vec2,
    /// `0` clear blue, `1` total overcast.
    pub cover: f32,
    /// Opacity multiplier for clouds that survive the `cover` threshold.
    pub density: f32,
    /// Edge-softness band around the cover threshold.
    pub softness: f32,
    /// Drift speed (m/s) along `wind_dir`.
    pub speed: f32,
    /// World-metres per UV unit — controls feature size of the noise.
    pub scale: f32,
}

/// [`MaterialExtension`] that drives `cloud.wgsl`.
///
/// Bind-group slots (group `MATERIAL_BIND_GROUP`, 100 +):
/// - 100 [`CloudUniforms`] uniform
///
/// Shadows and prepass are both disabled — the cloud deck must not occlude
/// anything in the depth prepass (it's transparent) and casting shadows from
/// a procedurally noised plane would only project a flat slab onto the
/// terrain, which is worse than nothing.
#[derive(Asset, TypePath, AsBindGroup, Clone, Default, Debug)]
pub struct CloudExtension {
    #[uniform(100)]
    pub uniforms: CloudUniforms,
}

impl MaterialExtension for CloudExtension {
    fn fragment_shader() -> ShaderRef {
        CLOUD_SHADER_PATH.into()
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

/// Convenience alias for the full extended-material type used by the cloud
/// deck plane.
pub type CloudMaterial = ExtendedMaterial<StandardMaterial, CloudExtension>;

/// Marker component on the single cloud-deck plane spawned in
/// `setup_lighting`. The world compiler's `apply_environment_state` system
/// uses this to find the plane each change tick and update its uniform
/// block plus altitude in place, mirroring how the `SkyBox` marker is used
/// to retint the unlit sky cuboid.
#[derive(Component)]
pub struct CloudLayer;

/// Pin the cloud-deck plane's XZ to the active camera each frame so the
/// finite mesh is always centred on the viewer.
///
/// The shader's horizon fade is camera-relative — it dissolves the deck on a
/// circle of radius `~5.671 × (cloud_height − cam_height)` around the camera.
/// Without this follow, the mesh stays anchored at world origin, so as the
/// player drifts off-centre the closest mesh edge can fall *inside* that
/// fade circle and the deck terminates in a hard line on whichever side the
/// player has moved toward, while the other three edges (still well past the
/// fade radius) continue to dissolve cleanly.
///
/// The FBM is sampled in world XZ with a time-driven scroll, so the clouds
/// stay world-anchored regardless of where the mesh sits — moving the mesh
/// only relocates the sampling window, not the cloud silhouettes. `.y` is
/// owned by `apply_environment_state` (it's driven by the `cloud_height`
/// uniform) and deliberately not touched here.
pub fn track_cloud_layer_to_camera(
    camera: Query<&GlobalTransform, (With<Camera3d>, Without<CloudLayer>)>,
    mut cloud_layer: Query<&mut Transform, With<CloudLayer>>,
) {
    let Ok(cam_tx) = camera.single() else {
        return;
    };
    let cam = cam_tx.translation();
    for mut transform in cloud_layer.iter_mut() {
        transform.translation.x = cam.x;
        transform.translation.z = cam.z;
    }
}
