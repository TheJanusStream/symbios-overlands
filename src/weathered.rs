//! Weather on the world's parts (#1686): the material every part the world
//! builder spawns is drawn with - roads, buildings, placed items, plants'
//! trunks - a [`StandardMaterial`] that wets, gathers puddles and takes
//! settled snow as the ground under it does (`assets/shaders/weathered.wgsl`).
//!
//! # One block for every material
//!
//! A world draws a hundred or so distinct part materials, more on a Berlin
//! square, and a passing weather moves its amounts a hundred steps
//! (`config::lighting::weather::WEATHER_STEP`). A uniform of each material's
//! own would rebuild every one of them at every step. So the extension
//! carries no data of its own but whether it weathers at all: its bindings
//! are one uniform buffer and one texture that every weathered material
//! shares, kept in the render world ([`WorldWeatherGpu`]) and written in
//! place each frame the weather moves. A step costs one 48-byte write.
//!
//! What it holds is [`WorldWeather`], kept from the ground's own weather
//! (`terrain::GroundWeatherNow`, so a roof and the hill behind it whiten
//! together), from the ground cover's shelter map (#1685), which the
//! shader reads to keep what stands under a roof dry, and from how dark the
//! drawn world is (`DrawnEnvironment::lights`), which lights a glow shown
//! from dusk to dawn (#1782) - a lit room behind a window - and puts it out
//! by day.
//!
//! The shelter's texture is the one exception to "never per material": a
//! bind group holds the texture it was made with, so when the shelter's
//! texture is made anew - a world of another size, or the first shelter
//! of a session - every weathered material is rebuilt once. The ground
//! cover writes a shelter of the same size into the texture already bound
//! (`ground_cover::replace`), which needs nothing. The render world takes
//! the shelter's texture view once the frame's images are prepared and
//! before its materials are ([`resolve_shelter_view`]), or a rebuilt
//! material could bind the texture the resize just replaced, for good.
//!
//! One cost on native: a [`StandardMaterial`] alone draws bindless where
//! the GPU allows, and an extension without bindless slots - this one
//! binds shared resources, which bindless does not cover - draws every
//! part with a bind group of its own. The web builds never draw bindless.
//!
//! # Who weathers
//!
//! Every material the world builder makes is a [`WeatheredMaterial`], one
//! type through every spawn path and cache. Its `weathers` key says whether
//! the weather is drawn on it: on for the room's parts and instance sets,
//! off for avatars, what they wear and ride, and item previews, which draw
//! exactly as a plain `StandardMaterial` (the shader compiles without the
//! weather). The key is part of the content-addressed prim cache's key, so
//! an avatar's prop never shares a handle with a world's identical one.
//! Its `dusk_to_dawn` key, its settings' own
//! ([`SovereignMaterialSettings::dusk_to_dawn`](crate::pds::SovereignMaterialSettings::dusk_to_dawn)),
//! compiles the glow's darkness in, and only beside the weather.

use bevy::asset::AssetId;
use bevy::ecs::system::SystemParamItem;
use bevy::ecs::system::lifetimeless::SRes;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{
    ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline,
};
use bevy::prelude::*;
use bevy::render::erased_render_asset::prepare_erased_assets;
use bevy::render::render_asset::{RenderAssets, prepare_assets};
use bevy::render::render_resource::binding_types::{texture_2d, uniform_buffer};
use bevy::render::render_resource::{
    AsBindGroup, AsBindGroupError, BindGroupLayout, BindGroupLayoutEntry, BindingResources,
    OwnedBindingResource, RenderPipelineDescriptor, ShaderStages, ShaderType,
    SpecializedMeshPipelineError, TextureSampleType, TextureView, TextureViewDimension,
    UniformBuffer, UnpreparedBindGroup,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::texture::{FallbackImage, GpuImage};
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use bevy::shader::ShaderRef;

const SHADER_PATH: &str = "shaders/weathered.wgsl";

/// The material every part the world builder spawns is drawn with.
pub type WeatheredMaterial = ExtendedMaterial<StandardMaterial, WeatherExtension>;

/// `base` drawn under the weather when `weathers`, and exactly as it is when
/// not.
pub fn weathered(base: StandardMaterial, weathers: bool) -> WeatheredMaterial {
    weathered_glow(base, weathers, false)
}

/// `base` as [`weathered`] draws it, its glow shown from dusk to dawn only
/// when `dusk_to_dawn` (#1782). Only under the weather: a part drawn
/// without it - an avatar's, an item's preview - glows at every hour.
pub fn weathered_glow(
    base: StandardMaterial,
    weathers: bool,
    dusk_to_dawn: bool,
) -> WeatheredMaterial {
    ExtendedMaterial {
        base,
        extension: WeatherExtension {
            weathers,
            dusk_to_dawn: weathers && dusk_to_dawn,
        },
    }
}

/// The weather's half of a [`WeatheredMaterial`]: whether it weathers, and
/// whether its glow shows from dusk to dawn only. It is the material's
/// pipeline key whole; all else it binds is shared (see the module docs).
#[derive(Asset, TypePath, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WeatherExtension {
    pub weathers: bool,
    /// Never without `weathers` ([`weathered_glow`]): the shader reads how
    /// dark the world is from the weather's block.
    pub dusk_to_dawn: bool,
}

/// The block `weathered.wgsl` reads as `WorldWeather`, one for the world.
#[derive(ShaderType, Clone, Copy, Debug, Default, PartialEq)]
pub struct WorldWeatherUniforms {
    /// How wet, 0 to 1, as the ground (`terrain::GroundWeather`).
    pub wetness: f32,
    /// How much snow lies where it has settled, 0 to 1.
    pub snow: f32,
    /// The snow line (world Y) and its edge's half width (m).
    pub snow_line: f32,
    pub snow_feather: f32,
    /// The shelter map's low corner (world XZ), its texel (m) and its
    /// texels per side: 0 texels for none, which shelters nothing.
    pub shelter_origin: Vec2,
    pub shelter_texel: f32,
    pub shelter_size: f32,
    /// How far the lights that show from dusk to dawn are on, 0 by day to
    /// 1 in the dark (#1782, `DrawnEnvironment::lights`).
    pub lights: f32,
    /// Padding to a whole 16 bytes: WebGL2 lays a uniform block out std140,
    /// which rounds the block up so, and a buffer bound short of it fails
    /// the draw.
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
}

/// The weather every weathered material draws, in the main world: kept by
/// [`follow_world_weather`], taken to the render world each frame.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct WorldWeather {
    pub uniforms: WorldWeatherUniforms,
    /// The shelter map's texture (`ground_cover::GroundCover::shelter`), or
    /// none before the ground cover has one.
    pub shelter: Option<AssetId<Image>>,
}

/// The render world's half: the one buffer every weathered material binds,
/// and the shelter texture it binds beside it.
#[derive(Resource, Default)]
pub struct WorldWeatherGpu {
    uniform: UniformBuffer<WorldWeatherUniforms>,
    shelter: Option<AssetId<Image>>,
    /// The shelter's view this frame, its image prepared: `None` before the
    /// ground cover has one, and while it waits on its image.
    shelter_view: Option<TextureView>,
}

impl AsBindGroup for WeatherExtension {
    type Data = WeatherExtension;
    type Param = (SRes<WorldWeatherGpu>, SRes<FallbackImage>);

    fn label() -> &'static str {
        "weathered_material"
    }

    fn bind_group_data(&self) -> WeatherExtension {
        *self
    }

    fn unprepared_bind_group(
        &self,
        _layout: &BindGroupLayout,
        _render_device: &RenderDevice,
        (gpu, fallback): &mut SystemParamItem<'_, '_, Self::Param>,
        _force_no_bindless: bool,
    ) -> Result<UnpreparedBindGroup, AsBindGroupError> {
        let Some(buffer) = gpu.uniform.buffer() else {
            return Err(AsBindGroupError::RetryNextUpdate);
        };
        let shelter = match (gpu.shelter, &gpu.shelter_view) {
            (Some(_), Some(view)) => view.clone(),
            // Its image is not prepared yet.
            (Some(_), None) => return Err(AsBindGroupError::RetryNextUpdate),
            // No map yet: the uniform's size of 0 keeps the shader off it.
            (None, _) => fallback.d2.texture_view.clone(),
        };
        Ok(UnpreparedBindGroup {
            bindings: BindingResources(vec![
                (100, OwnedBindingResource::Buffer(buffer.clone())),
                (
                    101,
                    OwnedBindingResource::TextureView(TextureViewDimension::D2, shelter),
                ),
            ]),
        })
    }

    fn bind_group_layout_entries(
        _render_device: &RenderDevice,
        _force_no_bindless: bool,
    ) -> Vec<BindGroupLayoutEntry> {
        vec![
            uniform_buffer::<WorldWeatherUniforms>(false).build(100, ShaderStages::FRAGMENT),
            texture_2d(TextureSampleType::Float { filterable: false })
                .build(101, ShaderStages::FRAGMENT),
        ]
    }
}

impl MaterialExtension for WeatherExtension {
    fn fragment_shader() -> ShaderRef {
        SHADER_PATH.into()
    }

    /// The weather is compiled in only where it is drawn: a material that
    /// does not weather runs Bevy's own fragment, line for line. A glow
    /// shown from dusk to dawn is compiled in beside it.
    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let key = key.bind_group_data;
        if key.weathers
            && let Some(fragment) = descriptor.fragment.as_mut()
        {
            fragment.shader_defs.push("WEATHERED".into());
            if key.dusk_to_dawn {
                fragment.shader_defs.push("DUSK_TO_DAWN".into());
            }
        }
        Ok(())
    }
}

/// Keep [`WorldWeather`] with the ground's weather, its shelter map and how
/// dark the drawn world is, and rebuild every weathered material once when
/// the shelter's texture is made anew (see the module docs).
pub fn follow_world_weather(
    ground: Option<Res<crate::terrain::GroundWeatherNow>>,
    cover: Option<Res<crate::ground_cover::GroundCover>>,
    drawn: Option<Res<crate::drawn_environment::DrawnEnvironment>>,
    mut world: ResMut<WorldWeather>,
    mut materials: ResMut<Assets<WeatheredMaterial>>,
) {
    let wanted = wanted_weather(
        ground.map_or_else(Default::default, |g| g.0),
        cover.as_deref(),
        drawn.map_or(0.0, |drawn| drawn.lights()),
    );
    if *world == wanted {
        return;
    }
    let rebind = world.shelter != wanted.shelter
        || world.uniforms.shelter_size != wanted.uniforms.shelter_size;
    *world = wanted;
    if rebind {
        // `iter_mut` sends every material out as modified, which rebuilds
        // its bind group onto the new texture.
        materials.iter_mut().for_each(drop);
    }
}

/// What the world's parts draw under `ground`'s weather and `cover`'s
/// shelter, their lights from dusk to dawn on by `lights`.
fn wanted_weather(
    ground: crate::terrain::GroundWeather,
    cover: Option<&crate::ground_cover::GroundCover>,
    lights: f32,
) -> WorldWeather {
    let (shelter, origin, texel, size) = match cover {
        Some(cover) => (
            Some(cover.shelter.id()),
            cover.shelter_map.origin(),
            cover.shelter_map.texel(),
            cover.shelter_map.size() as f32,
        ),
        None => (None, Vec2::ZERO, 1.0, 0.0),
    };
    WorldWeather {
        uniforms: WorldWeatherUniforms {
            wetness: ground.wetness,
            snow: ground.snow,
            snow_line: ground.snow_line,
            snow_feather: ground.snow_feather,
            shelter_origin: origin,
            shelter_texel: texel,
            shelter_size: size,
            lights,
            ..Default::default()
        },
        shelter,
    }
}

/// Take [`WorldWeather`] to the render world: the shared buffer written in
/// place when it moved (made on the first frame), and the shelter's id for
/// the bind groups made after.
fn extract_world_weather(
    world: Extract<Res<WorldWeather>>,
    mut gpu: ResMut<WorldWeatherGpu>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    if gpu.uniform.buffer().is_none() || *gpu.uniform.get() != world.uniforms {
        gpu.uniform.set(world.uniforms);
        gpu.uniform.write_buffer(&device, &queue);
    }
    gpu.shelter = world.shelter;
}

/// Take the shelter's texture view from this frame's prepared images:
/// ordered after the images are prepared and before the weathered
/// materials are, so a material rebuilt for a resized shelter binds its new
/// texture and never the one it replaced (see the module docs).
fn resolve_shelter_view(mut gpu: ResMut<WorldWeatherGpu>, images: Res<RenderAssets<GpuImage>>) {
    let view = gpu
        .shelter
        .and_then(|id| images.get(id))
        .map(|image| image.texture_view.clone());
    gpu.shelter_view = view;
}

/// The weathered material, its texture patch (the texture crate's, for this
/// material: `bevy_symbios_texture` 0.13.1) and the weather it shares.
/// Added by the game and by
/// [`world_builder::register_headless_spawn`](crate::world_builder::register_headless_spawn),
/// so the render tool draws parts, and compiles their shader, as the game
/// does.
pub struct WeatheredMaterialPlugin;

impl Plugin for WeatheredMaterialPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<WeatheredMaterial>::default())
            .init_resource::<WorldWeather>()
            .add_systems(
                Update,
                bevy_symbios_texture::patch_material_textures::<WeatheredMaterial>,
            )
            // Before the frame's asset events go out, so a rebuild it asks
            // for reaches the render world with the shelter it is for.
            .add_systems(
                PostUpdate,
                follow_world_weather.before(bevy::asset::AssetEventSystems),
            );
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<WorldWeatherGpu>()
                .add_systems(ExtractSchedule, extract_world_weather)
                .add_systems(
                    Render,
                    resolve_shelter_view
                        .in_set(RenderSystems::PrepareAssets)
                        .after(prepare_assets::<GpuImage>)
                        .before(prepare_erased_assets::<MeshMaterial3d<WeatheredMaterial>>),
                );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ground_cover::{CoverMask, GroundCover, GroundFrame, ShelterMap};
    use crate::terrain::{GroundWeather, GroundWeatherNow};
    use bevy::asset::{AssetApp, AssetEvent, AssetPlugin};

    const SHADER: &str = include_str!("../assets/shaders/weathered.wgsl");

    /// The fields of WGSL struct `name` in `src`, name and type each.
    fn wgsl_fields(src: &str, name: &str) -> Vec<(String, String)> {
        let head = format!("struct {name} {{");
        let start = src.find(&head).expect("the struct is declared") + head.len();
        let body = &src[start..start + src[start..].find('}').expect("and closed")];
        body.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with("//") && l.contains(':'))
            .map(|l| {
                let (field, ty) = l.split_once(':').expect("filtered on ':'");
                (
                    field.trim().to_string(),
                    ty.trim().trim_end_matches(',').to_string(),
                )
            })
            .collect()
    }

    /// The block is written twice, in Rust and in WGSL, and nothing compiles
    /// the shader before a GPU loads it: a field added to one alone reads
    /// the weather at the wrong offsets, with no error anywhere.
    #[test]
    fn the_shader_s_weather_block_is_the_rust_one() {
        let expected = [
            ("wetness", "f32"),
            ("snow", "f32"),
            ("snow_line", "f32"),
            ("snow_feather", "f32"),
            ("shelter_origin", "vec2<f32>"),
            ("shelter_texel", "f32"),
            ("shelter_size", "f32"),
            ("lights", "f32"),
            ("_pad0", "f32"),
            ("_pad1", "f32"),
            ("_pad2", "f32"),
        ]
        .map(|(f, t)| (f.to_string(), t.to_string()));
        assert_eq!(wgsl_fields(SHADER, "WorldWeather"), expected);
        // A whole number of 16 bytes, as WebGL2's std140 block is.
        assert_eq!(WorldWeatherUniforms::min_size().get(), 48);
    }

    /// The parts stand dry as far under a shelter as the ground does.
    #[test]
    fn the_shader_s_clearance_is_the_ground_s() {
        let line = format!(
            "const SHELTER_CLEARANCE: f32 = {:?};",
            crate::config::terrain::ground_cover::SHELTER_CLEARANCE
        );
        assert!(SHADER.contains(&line), "weathered.wgsl lacks `{line}`");
    }

    /// Whether a material weathers, and whether its glow shows from dusk to
    /// dawn, are its whole key: the pipeline compiles each in for one and
    /// out for the other. A glow shown from dusk to dawn reads the weather's
    /// block, so it is never keyed without the weather.
    #[test]
    fn a_material_s_key_is_whether_it_weathers_and_glows_by_night() {
        let key = |weathers, dusk| {
            weathered_glow(StandardMaterial::default(), weathers, dusk)
                .extension
                .bind_group_data()
        };
        assert_eq!(
            weathered(StandardMaterial::default(), true)
                .extension
                .bind_group_data(),
            key(true, false)
        );
        assert!(key(true, false).weathers && !key(true, false).dusk_to_dawn);
        assert!(key(true, true).weathers && key(true, true).dusk_to_dawn);
        assert_eq!(key(false, true), key(false, false));
        assert!(!key(false, true).weathers && !key(false, true).dusk_to_dawn);
    }

    /// Modified events for weathered materials, counted.
    #[derive(Resource, Default)]
    struct Rebuilt(usize);

    fn count_rebuilds(
        mut events: MessageReader<AssetEvent<WeatheredMaterial>>,
        mut rebuilt: ResMut<Rebuilt>,
    ) {
        rebuilt.0 += events
            .read()
            .filter(|e| matches!(e, AssetEvent::Modified { .. }))
            .count();
    }

    fn cover_over(app: &mut App, vertices: usize) -> GroundCover {
        let hm = bevy_symbios_ground::HeightMap::new(vertices, vertices, 1.0);
        let frame = GroundFrame::of(&hm);
        let texel = frame.cover_texel();
        let origin = Vec2::splat(-frame.half);
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        let mut add = || images.add(Image::default());
        GroundCover {
            frame,
            height: add(),
            weights: None,
            maxima: None,
            cover: add(),
            mask: CoverMask::open(frame.cover_texels as usize, texel, origin),
            shelter: add(),
            shelter_map: ShelterMap::open(frame.cover_texels as usize, texel, origin),
            weight_layer: None,
            lakes: None,
            revision: 0,
        }
    }

    /// A passing weather moves the one shared block and rebuilds no
    /// material; a shelter texture made anew - the first, or one of another
    /// size - rebuilds each of them once, onto it.
    #[test]
    fn a_weather_step_rebuilds_nothing_and_a_new_shelter_everything() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<WeatheredMaterial>()
            .init_resource::<WorldWeather>()
            .init_resource::<GroundWeatherNow>()
            .init_resource::<Rebuilt>()
            .add_systems(Update, count_rebuilds)
            .add_systems(PostUpdate, follow_world_weather);
        let parts: Vec<_> = (0..3)
            .map(|_| {
                app.world_mut()
                    .resource_mut::<Assets<WeatheredMaterial>>()
                    .add(weathered(StandardMaterial::default(), true))
            })
            .collect();
        let settle = |app: &mut App| {
            for _ in 0..3 {
                app.update();
            }
            std::mem::take(&mut app.world_mut().resource_mut::<Rebuilt>().0)
        };
        assert_eq!(settle(&mut app), 0, "added, not modified");

        let rain = GroundWeather {
            wetness: 0.5,
            snow: 0.25,
            snow_line: 12.0,
            snow_feather: 3.0,
        };
        app.world_mut().resource_mut::<GroundWeatherNow>().0 = rain;
        assert_eq!(settle(&mut app), 0, "a weather step rebuilds nothing");
        let held = *app.world().resource::<WorldWeather>();
        assert_eq!(held.uniforms.wetness, 0.5);
        assert_eq!(held.uniforms.snow_line, 12.0);
        assert_eq!(held.uniforms.shelter_size, 0.0, "no map, no shelter");

        let cover = cover_over(&mut app, 65);
        let shelter = cover.shelter.id();
        app.insert_resource(cover);
        assert_eq!(settle(&mut app), parts.len(), "a first shelter: each once");
        let held = *app.world().resource::<WorldWeather>();
        assert_eq!(held.shelter, Some(shelter));
        assert_eq!(held.uniforms.shelter_size, 128.0);
        assert_eq!(held.uniforms.shelter_origin, Vec2::splat(-32.0));
        assert_eq!(held.uniforms.wetness, 0.5, "the weather kept");

        app.world_mut().resource_mut::<GroundWeatherNow>().0.wetness = 0.75;
        assert_eq!(settle(&mut app), 0, "the same shelter: nothing");

        // The ground cover keeps its handle and writes a ground of another
        // size into it: a new texture all the same.
        let mut cover = app.world_mut().remove_resource::<GroundCover>().unwrap();
        let bigger = cover_over(&mut app, 129);
        cover.frame = bigger.frame;
        cover.shelter_map = bigger.shelter_map;
        app.insert_resource(cover);
        assert_eq!(settle(&mut app), parts.len(), "another size: each once");
    }

    /// Night falling moves the shared block's lights and rebuilds no
    /// material (#1782): the drawn world's darkness, and none with no world
    /// drawn.
    #[test]
    fn the_lights_follow_the_drawn_world_and_rebuild_nothing() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<WeatheredMaterial>()
            .init_resource::<WorldWeather>()
            .init_resource::<Rebuilt>()
            .add_systems(Update, count_rebuilds)
            .add_systems(PostUpdate, follow_world_weather);
        app.world_mut()
            .resource_mut::<Assets<WeatheredMaterial>>()
            .add(weathered_glow(StandardMaterial::default(), true, true));
        let settle = |app: &mut App| {
            for _ in 0..3 {
                app.update();
            }
            std::mem::take(&mut app.world_mut().resource_mut::<Rebuilt>().0)
        };
        assert_eq!(settle(&mut app), 0, "added, not modified");
        let lights = |app: &App| app.world().resource::<WorldWeather>().uniforms.lights;
        assert_eq!(lights(&app), 0.0, "no world drawn");

        let env = crate::pds::RoomRecord::default().environment;
        let midnight = crate::time_of_day::environment_at(&env, 0.0);
        app.insert_resource(crate::drawn_environment::DrawnEnvironment {
            env: midnight.env,
            hour: Some(0.0),
            moon: Some(midnight.moon),
            weather: Default::default(),
        });
        assert_eq!(settle(&mut app), 0, "night falls and nothing is rebuilt");
        assert_eq!(lights(&app), 1.0);
    }
}
