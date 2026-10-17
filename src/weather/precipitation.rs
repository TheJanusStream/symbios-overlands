//! What falls (#1678): rain and snow as one volume around the camera.
//!
//! One mesh of [`cfg::DROPS`] quads, each a streak of rain or a flake of
//! snow, placed, fallen and drifted by its vertex shader
//! (`assets/shaders/precipitation.wgsl`) from the time alone - thousands of
//! them in one draw call, with no entity each and nothing for the CPU to do
//! frame by frame. Each quad's corners carry where in the volume it starts
//! and a rank: in rain or snow of amount `a` the quads ranked under `a` fall,
//! the rest are folded away, so a heavier weather is a denser one. The
//! volume wraps round the camera in world space, so what falls stands still
//! as the camera moves and falls as the time runs.
//!
//! The streaks and the flakes take the sky's light and the key light's, and
//! drift on the room's wind (`cloud_wind_dir`, `cloud_speed`): the material
//! is written only when the drawn environment is (`crate::drawn_environment`)
//! and the volume is hidden while nothing falls, so a dry room pays nothing.
//! In a storm the streaks slant harder in its gust fronts
//! (`storm_common.wgsl`, the storm carried in the light's `w`), and a
//! lightning flash lights them (`lightning`, #1684).
//!
//! An orthographic view (#1603) folds every quad away: its eye stands far
//! off, and the volume round it would be a square of rain in the middle of
//! the frame. The volume knows nothing of shelter, so it falls through
//! roofs (#1685). WebGL2-safe: one uniform block, no texture.

use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::NoFrustumCulling,
    light::NotShadowCaster,
    math::Vec4,
    mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology},
    pbr::{Material, MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};

use super::WeatherNow;
use crate::config::lighting::weather::precipitation as cfg;
use crate::drawn_environment::{DrawEnvironmentSet, DrawnEnvironment};
use crate::pds::Environment;

const PRECIPITATION_SHADER_PATH: &str = "shaders/precipitation.wgsl";

// Four corners a quad, each indexed in 16 bits.
const _: () = assert!(cfg::DROPS as usize * 4 <= u16::MAX as usize + 1);

/// What `precipitation.wgsl` is given: two `Vec4`s, a 32-byte block - a
/// multiple of 16, as WebGL2 requires.
#[derive(Debug, Clone, Copy, Default, PartialEq, ShaderType)]
pub struct PrecipitationUniforms {
    /// `x` how hard it rains and `y` how hard it snows (0 to 1); `z`, `w`
    /// the wind (m/s) along world x and z.
    pub fall: Vec4,
    /// The sky's light on what falls, linear RGB as the sky is drawn; `w`
    /// how stormy (#1684), for the gust fronts.
    pub light: Vec4,
}

/// The volume's material. Its vertex shader places every quad; its
/// fragment shader draws a streak or a flake.
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug, Default)]
pub struct PrecipitationMaterial {
    #[uniform(0)]
    pub uniforms: PrecipitationUniforms,
}

impl Material for PrecipitationMaterial {
    fn vertex_shader() -> ShaderRef {
        PRECIPITATION_SHADER_PATH.into()
    }

    fn fragment_shader() -> ShaderRef {
        PRECIPITATION_SHADER_PATH.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    /// The quads turn to the camera themselves, either side may face it;
    /// and what falls is seen through, writing no depth.
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

/// Marks the volume.
#[derive(Component)]
pub struct Precipitation;

/// Register the volume - in the game and in the render tool's world.
pub(crate) fn register(app: &mut App) {
    app.add_plugins(MaterialPlugin::<PrecipitationMaterial>::default())
        .add_systems(Startup, spawn_precipitation)
        .add_systems(
            Update,
            (follow_camera, weather_the_volume.after(DrawEnvironmentSet)),
        );
}

/// The uniforms for what falls in environment `env` under weather `now`.
pub(crate) fn uniforms(env: &Environment, now: &WeatherNow) -> PrecipitationUniforms {
    let [x, z] = env.cloud_wind_dir.0;
    let wind = Vec2::new(x, z).normalize_or_zero() * env.cloud_speed.0.max(0.0);
    let linear = |c: [f32; 3]| {
        let l = Color::srgb(c[0], c[1], c[2]).to_linear();
        Vec3::new(l.red, l.green, l.blue)
    };
    let fog = env.fog_color.0;
    let light = (linear(env.sky_color.0) + linear([fog[0], fog[1], fog[2]])) * 0.5;
    PrecipitationUniforms {
        fall: Vec4::new(now.rain, now.snow, wind.x, wind.y),
        light: light.extend(now.storm),
    }
}

/// The volume's mesh: `drops` quads, each corner carrying the quad's start
/// in the volume (0 to 1 across, up and along, in `POSITION`), the corner
/// (`UV_0`), and its rank, size, speed and phase (0 to 1, in `COLOR`).
/// Fixed draws, so every visitor's rain is laid out alike.
pub(crate) fn volume_mesh(drops: u32) -> Mesh {
    let mut state = 0x5EED_F411_u64;
    let mut next = || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
    };
    let corners = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
    let n = drops as usize;
    let mut starts = Vec::with_capacity(n * 4);
    let mut uvs = Vec::with_capacity(n * 4);
    let mut data = Vec::with_capacity(n * 4);
    let mut indices = Vec::with_capacity(n * 6);
    for drop in 0..drops {
        let start = [next(), next(), next()];
        // The rank evenly spread, so a share `a` of the quads is `a` of them.
        let rank = (drop as f32 + next()) / drops as f32;
        let look = [rank, next(), next(), next()];
        for corner in corners {
            starts.push(start);
            uvs.push(corner);
            data.push(look);
        }
        let first = (drop * 4) as u16;
        indices.extend([first, first + 1, first + 2, first + 2, first + 1, first + 3]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, starts)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, data)
    .with_inserted_indices(Indices::U16(indices))
}

/// Spawn the volume, hidden until something falls.
fn spawn_precipitation(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<PrecipitationMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(volume_mesh(cfg::DROPS))),
        MeshMaterial3d(materials.add(PrecipitationMaterial::default())),
        Transform::default(),
        Visibility::Hidden,
        // The vertex shader places every quad round the camera: the mesh's
        // own bounds say nothing of where they are drawn.
        NoFrustumCulling,
        NotShadowCaster,
        Precipitation,
    ));
}

/// Keep the volume's entity on the camera. The shader places the quads
/// itself; standing at the camera, the volume sorts in front of every other
/// see-through thing - the water, the clouds - and draws after them, as what
/// falls between them and the eye must.
fn follow_camera(
    camera: Query<&GlobalTransform, (crate::camera::IsWorldCamera, Without<Precipitation>)>,
    mut volume: Query<&mut Transform, With<Precipitation>>,
) {
    let Ok(camera) = camera.single() else {
        return;
    };
    let eye = camera.translation();
    for mut transform in volume.iter_mut() {
        if transform.translation != eye {
            transform.translation = eye;
        }
    }
}

/// Give the volume the drawn weather whenever it is drawn anew, and hide it
/// while nothing falls.
pub(crate) fn weather_the_volume(
    drawn: Res<DrawnEnvironment>,
    mut materials: ResMut<Assets<PrecipitationMaterial>>,
    mut volume: Query<
        (&MeshMaterial3d<PrecipitationMaterial>, &mut Visibility),
        With<Precipitation>,
    >,
) {
    if !drawn.is_changed() {
        return;
    }
    let wanted = uniforms(&drawn.env, &drawn.weather);
    let falling = drawn.weather.falls();
    for (material, mut visibility) in volume.iter_mut() {
        let shown = if falling {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != shown {
            *visibility = shown;
        }
        // Written only while something falls, and only when it differs: a
        // turning day redraws every minute of its day, and a material write
        // rebuilds its bind group and re-specialises what draws it (#1675).
        if falling
            && materials
                .get(&material.0)
                .is_some_and(|m| m.uniforms != wanted)
            && let Some(mut m) = materials.get_mut(&material.0)
        {
            m.uniforms = wanted;
        }
    }
}
