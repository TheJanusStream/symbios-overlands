//! Sky light (#1680): the sky lights the world. Every camera marked
//! [`SkyLit`] carries an `EnvironmentMapLight` built from the room's own sky,
//! so a roof takes the sky's colour from above, a wall the horizon's from the
//! side and an eave the ground's from below, and anything glossy reflects the
//! sky - where Bevy's ambient light had been one colour from every direction,
//! and its reflection a flat sheen of that colour.
//!
//! The maps are made on the CPU from [`SkyModel`], the sky `sky.wgsl` draws
//! (`sky_common::sky_toward`) without its sun, moon and stars: the
//! directional light already brings the sun, and a disc in the map would
//! count it twice. Below the horizon the model is the ground, the horizon
//! dimmed. WebGL2 has no compute to filter a map on the GPU, and needs none
//! here - the sky is smooth:
//!
//! * the diffuse map is the sky's order-2 spherical harmonics, a couple of
//!   thousand samples, evaluated per texel;
//! * the specular map is the sky itself at 32 texels a face, its mips box
//!   filtered - Bevy reads mip `k` as perceptual roughness `k / (n - 1)`.
//!
//! A whole regeneration is under a millisecond natively, so it follows an
//! editor drag frame by frame.
//!
//! **What stays the room's.** The diffuse map is scaled so an upward face -
//! the ground, a roof, most of what is on screen - is lit as Bevy's ambient
//! light at the room's `ambient_brightness` lit it (Bevy's two paths light a
//! matte surface differently; `config::lighting::sky_light` has the
//! factors): rooms keep their level, walls and undersides gain their
//! shape. The specular map is the sky
//! at its own brightness, so a reflection is as bright as the sky it shows.
//! A sky-lit camera's own ambient light is turned off (`AmbientLight` on the
//! camera, brightness 0); the global one stays for the item previews, which
//! have no sky.
//!
//! The two maps add two textures (and on WebGPU a sampler) to every lit
//! pipeline. On the WebGL2 build the terrain's goes from 12 of the GLES
//! backend's 16 slots to 14 (measured, #1672 - see
//! `crate::splat::SplatExtension`). The WebGPU build, which also binds the
//! depth prepass and the stains overlay, reaches 16 of WebGPU's minimum 16
//! sampled textures: it drew whole under those limits
//! (`WGPU_SETTINGS_PRIO=webgpu`, 2026-10-16), with nothing to spare.

use std::f32::consts::PI;

use bevy::{
    asset::RenderAssetUsages,
    camera::Exposure,
    light::{AmbientLight, EnvironmentMapLight},
    pbr::FogFalloff,
    prelude::*,
    render::render_resource::{
        Extent3d, TextureDimension, TextureFormat, TextureUsages, TextureViewDescriptor,
        TextureViewDimension,
    },
};

use crate::config::lighting::sky_light as cfg;
use crate::pds::Environment;

/// Marker for a camera the sky lights. [`update_sky_light`] gives every one
/// the sky's `EnvironmentMapLight` and turns its own ambient light off.
#[derive(Component, Default, Clone, Copy)]
pub struct SkyLit;

/// The two maps every [`SkyLit`] camera shares, rewritten in place when the
/// room's environment changes.
#[derive(Resource)]
pub struct SkyLight {
    diffuse: Handle<Image>,
    specular: Handle<Image>,
}

/// The `EnvironmentMapLight` intensity that shows a map value as it is: the
/// maps are in the sky shader's display units, and Bevy multiplies indirect
/// light by the camera's exposure - Bevy's default, which no camera here
/// overrides.
fn intensity() -> f32 {
    1.0 / Exposure::default().exposure()
}

/// The sky the maps are made of: what `sky_common::sky_toward` draws toward
/// a direction, without the sun's glow, and the ground below the horizon.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SkyModel {
    /// The sky overhead, linear RGB.
    zenith: Vec3,
    /// The horizon: the room's atmospheric fog of the sky at the old sky
    /// cuboid's distance, as the sky shader's `fogged` computes it.
    horizon: Vec3,
    /// The haze's optical depth straight up.
    haze: f32,
}

impl SkyModel {
    pub(crate) fn from_environment(env: &Environment) -> Self {
        let sky = crate::sky::sky_uniforms(env);
        let zenith = sky.zenith.truncate();
        let [fr, fg, fb, fa] = env.fog_color.0;
        let fog = Color::srgba(fr, fg, fb, fa).to_linear();
        let [er, eg, eb] = env.fog_extinction.0;
        let [ir, ig, ib] = env.fog_inscattering.0;
        // The room's fog as `apply_environment_state` gives it to the camera,
        // so the coefficients are Bevy's own.
        let (extinction, inscattering) = match FogFalloff::from_visibility_colors(
            env.fog_visibility.0,
            Color::srgb(er, eg, eb),
            Color::srgb(ir, ig, ib),
        ) {
            FogFalloff::Atmospheric {
                extinction,
                inscattering,
            } => (extinction, inscattering),
            _ => (Vec3::ZERO, Vec3::ZERO),
        };
        let distance = sky.look.w;
        // Bevy's `atmospheric_fog` with no sun glow: the body extinguished
        // by `1 - exp(-d * be)`, the fog scattered in by `1 - exp(-d * bi)`,
        // both by the fog colour's alpha.
        let extinguished = Vec3::ONE - (-extinction * distance).exp();
        let scattered = Vec3::ONE - (-inscattering * distance).exp();
        let fog_rgb = Vec3::new(fog.red, fog.green, fog.blue);
        let horizon =
            zenith * (Vec3::ONE - extinguished * fog.alpha) + fog_rgb * scattered * fog.alpha;
        Self {
            zenith,
            horizon: if horizon.is_finite() { horizon } else { zenith },
            haze: sky.look.x,
        }
    }

    /// The radiance toward a unit world direction: the sky above the
    /// horizon, hazed by the air mass as the sky shader hazes it, and below
    /// it the ground, fading from the horizon to its share of it.
    pub(crate) fn radiance(&self, dir: Vec3) -> Vec3 {
        let up = dir.y;
        if up <= 0.0 {
            let ground = self.horizon * cfg::GROUND_SHARE;
            return self
                .horizon
                .lerp(ground, smoothstep(0.0, cfg::GROUND_FADE, -up));
        }
        let air_mass = 1.0 / up.max(1.0 / cfg::MAX_AIR_MASS);
        let haze = 1.0 - (-self.haze * air_mass).exp();
        self.zenith.lerp(self.horizon, haze)
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn luminance(c: Vec3) -> f32 {
    c.dot(Vec3::new(0.2126, 0.7152, 0.0722))
}

/// The real order-2 spherical-harmonic basis at a unit direction.
fn sh_basis(d: Vec3) -> [f32; 9] {
    let (x, y, z) = (d.x, d.y, d.z);
    [
        0.282_095,
        0.488_603 * y,
        0.488_603 * z,
        0.488_603 * x,
        1.092_548 * x * y,
        1.092_548 * y * z,
        0.315_392 * (3.0 * z * z - 1.0),
        1.092_548 * x * z,
        0.546_274 * (x * x - y * y),
    ]
}

/// A radiance's order-2 spherical harmonics, from evenly spread directions
/// (a Fibonacci sphere).
fn project_sh(radiance: impl Fn(Vec3) -> Vec3) -> [Vec3; 9] {
    let n = cfg::SH_SAMPLES;
    let golden = PI * (3.0 - 5.0_f32.sqrt());
    let mut coeffs = [Vec3::ZERO; 9];
    for i in 0..n {
        let y = 1.0 - 2.0 * (i as f32 + 0.5) / n as f32;
        let r = (1.0 - y * y).max(0.0).sqrt();
        let phi = golden * i as f32;
        let dir = Vec3::new(r * phi.cos(), y, r * phi.sin());
        let light = radiance(dir);
        for (c, b) in coeffs.iter_mut().zip(sh_basis(dir)) {
            *c += light * b;
        }
    }
    coeffs.map(|c| c * (4.0 * PI / n as f32))
}

fn sky_sh(model: &SkyModel) -> [Vec3; 9] {
    project_sh(|dir| model.radiance(dir))
}

/// What a white matte surface facing `normal` sends back under the sky:
/// its irradiance over pi (Ramamoorthi and Hanrahan's cosine convolution
/// of the harmonics), never below zero.
fn diffuse_from_sh(coeffs: &[Vec3; 9], normal: Vec3) -> Vec3 {
    const BAND: [f32; 9] = [
        PI,
        2.0 * PI / 3.0,
        2.0 * PI / 3.0,
        2.0 * PI / 3.0,
        PI / 4.0,
        PI / 4.0,
        PI / 4.0,
        PI / 4.0,
        PI / 4.0,
    ];
    let mut e = Vec3::ZERO;
    for ((c, b), a) in coeffs.iter().zip(sh_basis(normal)).zip(BAND) {
        e += *c * (a * b);
    }
    (e / PI).max(Vec3::ZERO)
}

/// The unit cube-space direction through texel `(i, j)` of face `face`
/// (+X, -X, +Y, -Y, +Z, -Z, the order wgpu stores a cube's layers in).
fn cube_dir(face: usize, i: u32, j: u32, size: u32) -> Vec3 {
    let a = 2.0 * (i as f32 + 0.5) / size as f32 - 1.0;
    let b = 2.0 * (j as f32 + 0.5) / size as f32 - 1.0;
    match face {
        0 => Vec3::new(1.0, -b, -a),
        1 => Vec3::new(-1.0, -b, a),
        2 => Vec3::new(a, 1.0, b),
        3 => Vec3::new(a, -1.0, -b),
        4 => Vec3::new(a, -b, 1.0),
        _ => Vec3::new(-a, -b, -1.0),
    }
    .normalize()
}

/// The world direction a cube texel holds: Bevy samples a cube map with the
/// direction's z negated ("cube maps are left-handed").
fn world_dir(cube: Vec3) -> Vec3 {
    Vec3::new(cube.x, cube.y, -cube.z)
}

fn push_texel(data: &mut Vec<u8>, c: Vec3) {
    for v in [c.x, c.y, c.z, 1.0] {
        data.extend_from_slice(&half::f16::from_f32(v).to_le_bytes());
    }
}

/// A cube image of `size` texels a face and `mips` levels, its data laid
/// out face by face, each face's levels in turn (wgpu's `LayerMajor`).
///
/// Bound for sampling only, no `COPY_DST`: Bevy re-uploads an image whose
/// descriptor did not change by writing into the old texture, and that
/// write (`GpuImage::prepare_asset`, bevy_render 0.19) is mip 0 alone, read
/// as six dense mip-0 layers - it would scramble the specular map's faces
/// and leave its blurrier levels on the first sky. Without the usage Bevy
/// makes a new texture from the whole buffer each time.
fn cube_image(size: u32, mips: u32, data: Vec<u8>) -> Image {
    let mut image = Image::default();
    image.texture_descriptor.size = Extent3d {
        width: size,
        height: size,
        depth_or_array_layers: 6,
    };
    image.texture_descriptor.dimension = TextureDimension::D2;
    image.texture_descriptor.format = TextureFormat::Rgba16Float;
    image.texture_descriptor.mip_level_count = mips;
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING;
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    image.asset_usage = RenderAssetUsages::default();
    image.data = Some(data);
    image
}

/// The diffuse map: the sky's light on a matte surface, per normal, scaled
/// so an upward face is lit as Bevy's ambient light at the room's
/// `ambient_brightness` lit it, its colour drawn toward grey by
/// [`cfg::DIFFUSE_SATURATION`].
pub(crate) fn diffuse_texels(model: &SkyModel, ambient_brightness: f32) -> Vec<Vec3> {
    let coeffs = sky_sh(model);
    let up = luminance(diffuse_from_sh(&coeffs, Vec3::Y));
    let target =
        ambient_brightness.max(0.0) * Exposure::default().exposure() * cfg::BEVY_AMBIENT_DIFFUSE
            / cfg::ENV_MAP_DIFFUSE;
    // A sky with no light to speak of has no hue to spread either: the
    // room's ambient level falls on every face alike.
    let flat = up < cfg::MIN_UP_LUMINANCE;
    let scale = if flat { 0.0 } else { target / up };
    let size = cfg::DIFFUSE_SIZE;
    let mut texels = Vec::with_capacity((6 * size * size) as usize);
    for face in 0..6 {
        for j in 0..size {
            for i in 0..size {
                let lit = if flat {
                    Vec3::splat(target)
                } else {
                    diffuse_from_sh(&coeffs, world_dir(cube_dir(face, i, j, size))) * scale
                };
                let grey = Vec3::splat(luminance(lit));
                texels.push(grey.lerp(lit, cfg::DIFFUSE_SATURATION));
            }
        }
    }
    texels
}

/// The specular map's levels, finest first: the sky at
/// [`cfg::SPECULAR_SIZE`] texels a face, and each level after the one before
/// it averaged 2 x 2, down to one texel. Indexed `[mip][face][texel]`.
pub(crate) fn specular_levels(model: &SkyModel) -> Vec<Vec<Vec<Vec3>>> {
    let size = cfg::SPECULAR_SIZE;
    let base: Vec<Vec<Vec3>> = (0..6)
        .map(|face| {
            let mut texels = Vec::with_capacity((size * size) as usize);
            for j in 0..size {
                for i in 0..size {
                    texels.push(model.radiance(world_dir(cube_dir(face, i, j, size))));
                }
            }
            texels
        })
        .collect();
    let mut levels = vec![base];
    let mut side = size;
    while side > 1 {
        let half = side / 2;
        let next = levels
            .last()
            .expect("the base level is there")
            .iter()
            .map(|face| {
                let mut texels = Vec::with_capacity((half * half) as usize);
                for j in 0..half {
                    for i in 0..half {
                        let at = |x: u32, y: u32| face[(y * side + x) as usize];
                        texels.push(
                            (at(2 * i, 2 * j)
                                + at(2 * i + 1, 2 * j)
                                + at(2 * i, 2 * j + 1)
                                + at(2 * i + 1, 2 * j + 1))
                                * 0.25,
                        );
                    }
                }
                texels
            })
            .collect();
        levels.push(next);
        side = half;
    }
    levels
}

/// The two cube images for a room's environment.
fn sky_images(env: &Environment) -> (Image, Image) {
    let model = SkyModel::from_environment(env);
    let mut diffuse = Vec::new();
    for c in diffuse_texels(&model, env.ambient_brightness.0) {
        push_texel(&mut diffuse, c);
    }
    let levels = specular_levels(&model);
    let mut specular = Vec::new();
    for face in 0..6 {
        for level in &levels {
            for c in &level[face] {
                push_texel(&mut specular, *c);
            }
        }
    }
    (
        cube_image(cfg::DIFFUSE_SIZE, 1, diffuse),
        cube_image(cfg::SPECULAR_SIZE, levels.len() as u32, specular),
    )
}

/// Startup: the maps for an environment the record has not yet replaced.
pub(crate) fn setup_sky_light(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let (diffuse, specular) = sky_images(&Environment::default());
    commands.insert_resource(SkyLight {
        diffuse: images.add(diffuse),
        specular: images.add(specular),
    });
}

/// Rewrite the maps when the room's drawn sky or ambient level changes -
/// the drawn environment (`crate::time_of_day`) follows the record, the
/// editor's preview lane frame by frame through a drag, and a turning
/// day's hour (#1677) - and give every new [`SkyLit`] camera the sky's light
/// in place of its own ambient.
///
/// Only when they change: the record changes for a moved tree too, and the
/// preview lane fires for a drag on any editor tab, and each rewrite makes
/// two new textures ([`cube_image`] says why).
pub(crate) fn update_sky_light(
    drawn: Option<Res<crate::time_of_day::DrawnEnvironment>>,
    sky_light: Option<Res<SkyLight>>,
    mut images: ResMut<Assets<Image>>,
    cameras: Query<Entity, (With<SkyLit>, Without<EnvironmentMapLight>)>,
    mut commands: Commands,
    mut built: Local<Option<(SkyModel, f32)>>,
) {
    let Some(sky_light) = sky_light else {
        return;
    };
    if let Some(drawn) = drawn
        && drawn.is_changed()
    {
        let env = &drawn.env;
        let wanted = (SkyModel::from_environment(env), env.ambient_brightness.0);
        if built.as_ref() == Some(&wanted) {
            return;
        }
        *built = Some(wanted);
        let (diffuse, specular) = sky_images(env);
        if let Some(mut image) = images.get_mut(&sky_light.diffuse) {
            *image = diffuse;
        }
        if let Some(mut image) = images.get_mut(&sky_light.specular) {
            *image = specular;
        }
    }
    for camera in &cameras {
        commands.entity(camera).try_insert((
            EnvironmentMapLight {
                diffuse_map: sky_light.diffuse.clone(),
                specular_map: sky_light.specular.clone(),
                intensity: intensity(),
                ..default()
            },
            AmbientLight {
                brightness: 0.0,
                ..default()
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{Fp, Fp3, Fp4};

    fn approx(a: Vec3, b: Vec3, tol: f32) -> bool {
        (a - b).abs().max_element() <= tol
    }

    /// A sky the same in every direction lights every face with exactly
    /// that sky - the projection and the cosine convolution together.
    #[test]
    fn a_uniform_sky_lights_every_face_alike() {
        let coeffs = project_sh(|_| Vec3::splat(0.4));
        for n in [
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::X,
            Vec3::Z,
            Vec3::new(0.6, 0.8, 0.0),
        ] {
            let lit = diffuse_from_sh(&coeffs, n);
            assert!(approx(lit, Vec3::splat(0.4), 2e-3), "{n}: {lit}");
        }
    }

    /// The sky is lighter above than below, so a roof is lit more than a
    /// wall and a wall more than an eave - the shape the flat ambient light
    /// never gave.
    #[test]
    fn an_upward_face_is_lit_most_and_a_downward_one_least() {
        let env = Environment::default();
        let model = SkyModel::from_environment(&env);
        let coeffs = sky_sh(&model);
        let up = luminance(diffuse_from_sh(&coeffs, Vec3::Y));
        let side = luminance(diffuse_from_sh(&coeffs, Vec3::X));
        let down = luminance(diffuse_from_sh(&coeffs, Vec3::NEG_Y));
        assert!(up > side && side > down && down > 0.0, "{up} {side} {down}");
    }

    /// The room's level is kept where most of the screen is: an upward face
    /// is lit as Bevy's ambient light at `ambient_brightness` lit it - its
    /// level times the exposure, through Bevy's two matte factors.
    #[test]
    fn an_upward_face_keeps_the_rooms_ambient_level() {
        let env = Environment {
            ambient_brightness: Fp(400.0),
            ..Environment::default()
        };
        let model = SkyModel::from_environment(&env);
        let texels = diffuse_texels(&model, 400.0);
        // The +Y face's centre texels face straight up.
        let size = cfg::DIFFUSE_SIZE as usize;
        let up_face = &texels[2 * size * size..3 * size * size];
        let centre = up_face[(size / 2) * size + size / 2];
        let want = 400.0 * Exposure::default().exposure() * cfg::BEVY_AMBIENT_DIFFUSE
            / cfg::ENV_MAP_DIFFUSE;
        assert!(
            (luminance(centre) - want).abs() < want * 0.02,
            "{} vs {want}",
            luminance(centre)
        );
    }

    /// The model is the sky shader's: overhead the sky colour hazed by the
    /// optical depth straight up, at the horizon the fogged horizon, and
    /// below it the ground's share.
    #[test]
    fn the_model_is_the_sky_the_shader_draws() {
        let env = Environment {
            sky_color: Fp3([0.3, 0.5, 0.9]),
            fog_color: Fp4([0.6, 0.6, 0.6, 1.0]),
            ..Environment::default()
        };
        let model = SkyModel::from_environment(&env);
        let sky = crate::sky::sky_uniforms(&env);
        let zenith = sky.zenith.truncate();
        let haze = 1.0 - (-sky.look.x).exp();
        assert!(approx(
            model.radiance(Vec3::Y),
            zenith.lerp(model.horizon, haze),
            1e-6
        ));
        assert!(approx(model.radiance(Vec3::X), model.horizon, 1e-6));
        assert!(approx(
            model.radiance(Vec3::NEG_Y),
            model.horizon * cfg::GROUND_SHARE,
            1e-6
        ));
    }

    /// Bevy samples a cube map with z negated: the +Y face holds the sky
    /// overhead, and world +Z sits on the cube's -Z face.
    #[test]
    fn the_cube_faces_hold_the_world_directions_bevy_reads_them_at() {
        let centre = |face| world_dir(cube_dir(face, 0, 0, 1));
        assert!(approx(centre(0), Vec3::X, 1e-6));
        assert!(approx(centre(1), Vec3::NEG_X, 1e-6));
        assert!(approx(centre(2), Vec3::Y, 1e-6));
        assert!(approx(centre(3), Vec3::NEG_Y, 1e-6));
        assert!(approx(centre(4), Vec3::NEG_Z, 1e-6));
        assert!(approx(centre(5), Vec3::Z, 1e-6));
    }

    /// Bevy writes a re-uploaded image with `COPY_DST` into the old texture
    /// at mip 0 only, which scrambles a cube with mips: the maps must not
    /// carry the usage (see [`cube_image`]).
    #[test]
    fn the_maps_are_never_rewritten_in_place() {
        let (diffuse, specular) = sky_images(&Environment::default());
        for image in [&diffuse, &specular] {
            assert!(
                !image
                    .texture_descriptor
                    .usage
                    .contains(TextureUsages::COPY_DST)
            );
            assert!(
                image
                    .texture_descriptor
                    .usage
                    .contains(TextureUsages::TEXTURE_BINDING)
            );
        }
    }

    /// The bytes are laid out as Bevy is told: face by face, each face's
    /// levels in turn - so each face's first texel sits a whole face's mip
    /// chain after the last one's.
    #[test]
    fn each_face_starts_where_layer_major_puts_it() {
        let env = Environment::default();
        let (_, specular) = sky_images(&env);
        let levels = specular_levels(&SkyModel::from_environment(&env));
        let s = cfg::SPECULAR_SIZE;
        let per_face: usize = (0..levels.len() as u32)
            .map(|m| ((s >> m) * (s >> m)) as usize)
            .sum();
        let data = specular.data.as_ref().expect("the specular map has data");
        for (face, texels) in levels[0].iter().enumerate() {
            let at = face * per_face * 8;
            let red = half::f16::from_le_bytes([data[at], data[at + 1]]).to_f32();
            let want = half::f16::from_f32(texels[0].x).to_f32();
            assert_eq!(red, want, "face {face}");
        }
    }

    /// The images are the sizes Bevy is told they are: six faces, a full
    /// mip chain for the specular map, eight bytes a texel.
    #[test]
    fn the_images_hold_what_their_descriptors_say() {
        let (diffuse, specular) = sky_images(&Environment::default());
        let d = cfg::DIFFUSE_SIZE as usize;
        assert_eq!(diffuse.data.as_ref().map(Vec::len), Some(6 * d * d * 8));
        let s = cfg::SPECULAR_SIZE;
        let mips = s.ilog2() + 1;
        assert_eq!(specular.texture_descriptor.mip_level_count, mips);
        let per_face: u32 = (0..mips).map(|m| (s >> m) * (s >> m)).sum();
        assert_eq!(
            specular.data.as_ref().map(Vec::len),
            Some(6 * per_face as usize * 8)
        );
    }
}
