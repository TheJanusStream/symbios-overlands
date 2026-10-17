//! The sky (#1671): a gradient from the room's sky colour overhead to its
//! fog at the horizon, the sun's disc and halo, and at night the moon and
//! the stars - all read from what a room record already stores, so every
//! room has it, the ones published before it included.
//!
//! The sky is the cuboid `setup_lighting` spawns around the camera, drawn by
//! [`SkyMaterial`] through `assets/shaders/sky.wgsl`. The shader lights the
//! disc with the scene's own directional light and hazes everything with
//! the camera's own distance fog, along an imaginary path that is longest at
//! the horizon: the far terrain, which the same fog covers, fades into the
//! sky without a seam. What the record adds is [`SkyUniforms`], derived by
//! [`sky_uniforms`]:
//!
//! * the sky overhead, `sky_color`. Before this it was all but invisible:
//!   the fog covered the old cuboid whole, so every sky was the fog colour;
//! * how deep the haze is, from `fog_visibility`;
//! * the moon, read off a key light dimmer than daylight - `sun_illuminance` -
//!   under a dark sky;
//! * the stars, read off a dark sky overhead and dimmed by `cloud_cover`.
//!
//! The record stores no night, so the last two are thresholds measured on
//! the seeded rooms; `config::lighting::sky` has them and where they come
//! from. WebGL2-safe: one uniform block, no textures.

use bevy::{
    math::Vec4,
    mesh::MeshVertexBufferLayoutRef,
    pbr::{Material, MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};

use crate::config::lighting::sky as cfg;
use crate::pds::Environment;

const SKY_SHADER_PATH: &str = "shaders/sky.wgsl";

/// An optical depth past which nothing of the sky's own colour is left
/// (`exp(-50)` is 2e-22): where a fog with no visibility at all lands.
const FOGGED_THROUGH: f32 = 50.0;

/// What `sky.wgsl` is given: two `Vec4`s, a 32-byte block - WebGL2 rejects
/// a uniform block that is not a multiple of 16 bytes
/// (`the_uniform_block_is_16_byte_aligned`).
#[derive(Debug, Clone, Copy, Default, PartialEq, ShaderType)]
pub struct SkyUniforms {
    /// The sky overhead, linear RGB; `w` unused.
    pub zenith: Vec4,
    /// `x` the haze's optical depth straight up; `y` the moon's share of the
    /// disc (0 the sun, 1 the moon); `z` the stars' brightness (0 none, 1 a
    /// clear night); `w` the horizon's distance in metres - 2 km, where the
    /// old sky cuboid's walls stood (2.0-2.8 km from the camera), so a
    /// room's fog makes of its horizon what it made of them.
    pub look: Vec4,
}

/// The sky cuboid's material. The fragment shader replaces every colour
/// Bevy would have computed, so there is no base material underneath.
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug, Default)]
pub struct SkyMaterial {
    #[uniform(0)]
    pub uniforms: SkyUniforms,
}

impl Material for SkyMaterial {
    /// Draws the cuboid at the far plane (#1673): see `sky.wgsl`'s `vertex`.
    fn vertex_shader() -> ShaderRef {
        SKY_SHADER_PATH.into()
    }

    fn fragment_shader() -> ShaderRef {
        SKY_SHADER_PATH.into()
    }

    /// Kept out of the depth prepass: nothing there wants the sky, and the
    /// water's shore foam reads "no geometry behind me" where only sky is.
    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    /// The camera is inside the cuboid, looking at its inner faces. The sky
    /// stands at the far plane (`sky.wgsl`'s `vertex`, #1673), where the
    /// depth buffer starts, so a depth it wrote would change nothing: it
    /// writes none.
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}

/// The sky's uniforms for a room's environment. Pure, so what each room
/// gets is testable without a GPU.
pub(crate) fn sky_uniforms(env: &Environment) -> SkyUniforms {
    let [r, g, b] = env.sky_color.0;
    let zenith = Color::srgb(r, g, b).to_linear();
    let luminance = 0.2126 * zenith.red + 0.7152 * zenith.green + 0.0722 * zenith.blue;
    // The fog's own extinction at its visibility, as Bevy derives it.
    let beta = bevy::pbr::FogFalloff::koschmieder(
        env.fog_visibility.0,
        bevy::pbr::FogFalloff::REVISED_KOSCHMIEDER_CONTRAST_THRESHOLD,
    );
    // A visibility the sanitiser would never pass (it holds 10-10,000 m):
    // none at all is fog through and through, a negative one or one that is
    // not a number no fog.
    let haze = beta * cfg::HAZE_M;
    let haze = if haze.is_nan() {
        0.0
    } else {
        haze.clamp(0.0, FOGGED_THROUGH)
    };
    // The moon is a dim light under a dark sky: a dim light alone could be a
    // low sun on a heavily clouded day.
    let dim = 1.0 - smoothstep(cfg::MOON_LUX, cfg::SUN_LUX, env.sun_illuminance.0);
    let dark = 1.0
        - smoothstep(
            cfg::MOON_SKY_LUMINANCE,
            cfg::NO_MOON_SKY_LUMINANCE,
            luminance,
        );
    let moon = dim * dark;
    // A cover that is not a number reads as clear.
    let cover = env.cloud_cover.0;
    let cover = if cover.is_nan() {
        0.0
    } else {
        cover.clamp(0.0, 1.0)
    };
    let clear = 1.0 - cfg::STARS_CLOUD_DIMMING * cover;
    let stars =
        (1.0 - smoothstep(
            cfg::STARS_FULL_LUMINANCE,
            cfg::STARS_NONE_LUMINANCE,
            luminance,
        )) * clear;
    SkyUniforms {
        zenith: Vec4::new(zenith.red, zenith.green, zenith.blue, 1.0),
        look: Vec4::new(haze, moon, stars, crate::config::lighting::SKY_SCALE),
    }
}

/// GLSL's `smoothstep`, with a value that is not a number read as the
/// lower edge, 0 - the record is sanitised, so that is a fault upstream,
/// and it must not reach a uniform.
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = (x - edge0) / (edge1 - edge0);
    let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
    t * t * (3.0 - 2.0 * t)
}

/// Marker for the sky cuboid `setup_lighting` spawns. The world compiler
/// finds the sky's material through it when a room record changes.
#[derive(Component)]
pub struct SkyBox;

/// Keep the sky cuboid around the active camera every frame.
///
/// The shader reads only the view direction, so where the faces sit never
/// shows - but the cuboid is finite (4 km × 2 km × 4 km at the default
/// `SKY_SCALE`), and a camera that walked out of one fixed at the world
/// origin would see it from outside.
pub(crate) fn track_skybox_to_camera(
    camera: Query<&GlobalTransform, (crate::camera::IsWorldCamera, Without<SkyBox>)>,
    mut skybox: Query<&mut Transform, With<SkyBox>>,
) {
    let Ok(cam_tx) = camera.single() else {
        return;
    };
    let cam = cam_tx.translation();
    for mut transform in skybox.iter_mut() {
        transform.translation = cam;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{Fp, Fp3};

    /// A room with this sky and sun and a clear sky above them.
    fn env(sky: [f32; 3], lux: f32) -> Environment {
        Environment {
            sky_color: Fp3(sky),
            sun_illuminance: Fp(lux),
            cloud_cover: Fp(0.0),
            ..Environment::default()
        }
    }

    /// The block's size is what the WebGL2 uniform binding is validated
    /// against, and only a browser would otherwise say it broke.
    #[test]
    fn the_uniform_block_is_16_byte_aligned() {
        let size = SkyUniforms::min_size().get();
        assert_eq!(
            size % 16,
            0,
            "SkyUniforms is {size} bytes - WebGL2 needs a multiple of 16"
        );
    }

    /// The seeded rooms the thresholds were measured on, one of each kind
    /// (#1671): a daylight sky gets the sun and no stars, a Cyberpunk night
    /// the moon and every star, and a dusk sky the sun with none yet.
    #[test]
    fn a_night_room_gets_the_moon_and_stars_and_a_day_room_the_sun() {
        // Seed 3, Rural Farmland: sky (0.61, 0.71, 0.79), 13,495 lx.
        let day = sky_uniforms(&env([0.61, 0.71, 0.79], 13_495.0));
        assert_eq!((day.look.y, day.look.z), (0.0, 0.0));
        // Seed 31, Cyberpunk: sky (0.20, 0.17, 0.23), 958 lx.
        let night = sky_uniforms(&env([0.20, 0.17, 0.23], 958.0));
        assert_eq!((night.look.y, night.look.z), (1.0, 1.0));
        // Seed 1, Steampunk dusk: sky (0.34, 0.40, 0.44), 5,756 lx - a
        // pale sun, nearly all sun, and no stars in a sky that bright.
        let dusk = sky_uniforms(&env([0.34, 0.40, 0.44], 5_756.0));
        assert!(dusk.look.y > 0.0 && dusk.look.y < 0.1, "{}", dusk.look.y);
        assert_eq!(dusk.look.z, 0.0);
    }

    /// Dimming the key light only ever moves the disc toward the moon, and
    /// darkening the sky only ever brings the stars out.
    #[test]
    fn the_moon_and_the_stars_rise_as_the_light_falls() {
        let mut last = (0.0, 0.0);
        for k in 0..=40 {
            let f = 1.0 - k as f32 / 40.0;
            let u = sky_uniforms(&env([0.6 * f, 0.7 * f, 0.8 * f], 12_000.0 * f));
            assert!(u.look.y >= last.0 && u.look.z >= last.1, "{k}: {u:?}");
            last = (u.look.y, u.look.z);
        }
        assert_eq!(last, (1.0, 1.0));
    }

    /// The zenith is the record's sky colour in linear light, and the haze
    /// thickens as the room's fog closes in: a 200 m volcanic haze lets
    /// less of the sky through overhead than a 600 m alpine air.
    #[test]
    fn the_zenith_is_the_sky_colour_and_the_haze_follows_the_fog() {
        let mut e = env([1.0, 0.5, 0.0], 12_000.0);
        e.fog_visibility = Fp(350.0);
        let u = sky_uniforms(&e);
        let lin = Color::srgb(1.0, 0.5, 0.0).to_linear();
        assert_eq!(u.zenith, Vec4::new(lin.red, lin.green, lin.blue, 1.0));
        let overhead = (-u.look.x).exp();
        // exp(-16 * 2.996 / 350) = 0.872.
        assert!((0.86..0.88).contains(&overhead), "{overhead}");
        e.fog_visibility = Fp(200.0);
        let hazy = sky_uniforms(&e).look.x;
        e.fog_visibility = Fp(600.0);
        let clear = sky_uniforms(&e).look.x;
        assert!(
            hazy > u.look.x && u.look.x > clear,
            "{hazy} {} {clear}",
            u.look.x
        );
    }

    /// A dim light under a bright sky is a low sun on a clouded day, not the
    /// moon: the generator can dim a daylight sun to about 5,000 lx.
    #[test]
    fn a_dim_light_under_a_bright_sky_is_still_the_sun() {
        let overcast_day = sky_uniforms(&env([0.61, 0.71, 0.79], 4_969.0));
        assert_eq!(overcast_day.look.y, 0.0);
        // The same light under a night sky is the moon's, mostly.
        let night = sky_uniforms(&env([0.20, 0.17, 0.23], 4_969.0));
        assert!(night.look.y > 0.4, "{}", night.look.y);
    }

    /// Cloud dims the stars: an overcast night shows few, a clear one all.
    #[test]
    fn an_overcast_night_shows_fewer_stars() {
        let mut night = env([0.20, 0.17, 0.23], 958.0);
        let clear = sky_uniforms(&night).look.z;
        night.cloud_cover = Fp(1.0);
        let overcast = sky_uniforms(&night).look.z;
        assert_eq!(clear, 1.0);
        assert!(
            (overcast - (1.0 - cfg::STARS_CLOUD_DIMMING)).abs() < 1e-6,
            "{overcast}"
        );
    }

    /// A value that is not a number, or a visibility out of range - faults,
    /// since the record is sanitised - never reaches the GPU as one.
    #[test]
    fn a_non_finite_light_never_reaches_the_gpu_as_nan() {
        let u = sky_uniforms(&env([0.6, 0.7, 0.8], f32::NAN));
        assert!(u.look.is_finite(), "{u:?}");
        for visibility in [0.0, -50.0, f32::NAN] {
            let mut e = env([0.6, 0.7, 0.8], 12_000.0);
            e.fog_visibility = Fp(visibility);
            e.cloud_cover = Fp(f32::NAN);
            let u = sky_uniforms(&e);
            assert!(u.look.is_finite(), "{visibility}: {u:?}");
            assert!(
                (0.0..=FOGGED_THROUGH).contains(&u.look.x),
                "{visibility}: {u:?}"
            );
        }
    }
}
