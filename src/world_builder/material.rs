//! Water volume spawning, procedural material building, and foliage texture
//! task polling.

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, block_on, futures_lite::future};
use bevy_symbios_texture::ashlar::AshlarGenerator;
use bevy_symbios_texture::asphalt::AsphaltGenerator;
use bevy_symbios_texture::bark::BarkGenerator;
use bevy_symbios_texture::brick::BrickGenerator;
use bevy_symbios_texture::cobblestone::CobblestoneGenerator;
use bevy_symbios_texture::concrete::ConcreteGenerator;
use bevy_symbios_texture::corrugated::CorrugatedGenerator;
use bevy_symbios_texture::encaustic::EncausticGenerator;
use bevy_symbios_texture::generator::{
    TextureError as SymTextureError, TextureGenerator, TextureMap,
};
use bevy_symbios_texture::ground::GroundGenerator;
use bevy_symbios_texture::iron_grille::IronGrilleGenerator;
use bevy_symbios_texture::leaf::LeafGenerator;
use bevy_symbios_texture::marble::MarbleGenerator;
use bevy_symbios_texture::metal::MetalGenerator;
use bevy_symbios_texture::pavers::PaversGenerator;
use bevy_symbios_texture::plank::PlankGenerator;
use bevy_symbios_texture::rock::RockGenerator;
use bevy_symbios_texture::shingle::ShingleGenerator;
use bevy_symbios_texture::stained_glass::StainedGlassGenerator;
use bevy_symbios_texture::stucco::StuccoGenerator;
use bevy_symbios_texture::thatch::ThatchGenerator;
use bevy_symbios_texture::twig::TwigGenerator;
use bevy_symbios_texture::wainscoting::WainscotingGenerator;
use bevy_symbios_texture::window::WindowGenerator;
use bevy_symbios_texture::{map_to_images, map_to_images_card};

use crate::config::terrain as tcfg;
use crate::pds::{SovereignMaterialSettings, SovereignTextureConfig};
use crate::terrain::WaterVolume;
use crate::water::{WaterExtension, WaterMaterial};

use super::compile::SpawnCtx;
use super::{OverlandsFoliageTasks, RoomEntity};

pub(super) fn spawn_water_volume(
    commands: &mut Commands,
    level_offset: f32,
    placement_tf: Transform,
    world_extent: f32,
    meshes: &mut Assets<Mesh>,
    water_materials: &mut Assets<WaterMaterial>,
) -> Entity {
    let base_wl = tcfg::water::LEVEL_FACTOR * tcfg::HEIGHT_SCALE;
    let wl = (base_wl + level_offset).max(0.001);

    let water_mat = water_materials.add(WaterMaterial {
        base: StandardMaterial {
            base_color: Color::srgba(
                tcfg::water::COLOR[0],
                tcfg::water::COLOR[1],
                tcfg::water::COLOR[2],
                tcfg::water::COLOR[3],
            ),
            perceptual_roughness: tcfg::water::ROUGHNESS,
            metallic: tcfg::water::METALLIC,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            ..default()
        },
        extension: WaterExtension::default(),
    });

    let mut tf = placement_tf;
    tf.translation.y += wl / 2.0;
    tf.scale = Vec3::new(world_extent, wl, world_extent);

    commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
            MeshMaterial3d(water_mat),
            tf,
            WaterVolume,
            RoomEntity,
        ))
        .id()
}

/// Thin `SpawnCtx` wrapper around [`build_procedural_material`] for the
/// world-builder hot path, which already holds a [`SpawnCtx`] and doesn't
/// need to unpack its individual `&mut` resources at every call site.
pub(super) fn spawn_procedural_material(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    settings: &SovereignMaterialSettings,
) -> Handle<StandardMaterial> {
    build_procedural_material(ctx.std_materials, ctx.foliage_tasks, settings)
}

/// Free-function core of [`spawn_procedural_material`] — takes the two
/// resources it actually needs instead of the full [`SpawnCtx`], so avatar
/// builders can reuse it without constructing a world-builder context.
/// Returns a [`StandardMaterial`] handle whose texture slots are populated
/// asynchronously once the texture-generator task finishes.
pub fn build_procedural_material(
    std_materials: &mut Assets<StandardMaterial>,
    foliage_tasks: &mut OverlandsFoliageTasks,
    settings: &SovereignMaterialSettings,
) -> Handle<StandardMaterial> {
    let emissive = Color::srgb_from_array(settings.emission_color.0).to_linear()
        * settings.emission_strength.0;

    let (alpha_mode, double_sided, cull_mode, is_card) = settings.texture.render_properties();

    let handle = std_materials.add(StandardMaterial {
        base_color: Color::srgb_from_array(settings.base_color.0),
        perceptual_roughness: settings.roughness.0,
        metallic: settings.metallic.0,
        emissive,
        alpha_mode,
        double_sided,
        cull_mode,
        ..default()
    });

    let pool = AsyncComputeTaskPool::get();
    macro_rules! spawn_gen {
        ($gen:ty, $cfg:expr) => {{
            let config = $cfg;
            let task = pool.spawn(async move { <$gen>::new(config).generate(512, 512) });
            foliage_tasks.tasks.push((task, handle.clone(), is_card));
        }};
    }

    match &settings.texture {
        SovereignTextureConfig::None | SovereignTextureConfig::Unknown => {}
        SovereignTextureConfig::Leaf(c) => spawn_gen!(LeafGenerator, c.to_native()),
        SovereignTextureConfig::Twig(c) => spawn_gen!(TwigGenerator, c.to_native()),
        SovereignTextureConfig::Bark(c) => spawn_gen!(BarkGenerator, c.to_native()),
        SovereignTextureConfig::Window(c) => spawn_gen!(WindowGenerator, c.to_native()),
        SovereignTextureConfig::StainedGlass(c) => {
            spawn_gen!(StainedGlassGenerator, c.to_native())
        }
        SovereignTextureConfig::IronGrille(c) => spawn_gen!(IronGrilleGenerator, c.to_native()),
        SovereignTextureConfig::Ground(c) => spawn_gen!(GroundGenerator, c.to_native()),
        SovereignTextureConfig::Rock(c) => spawn_gen!(RockGenerator, c.to_native()),
        SovereignTextureConfig::Brick(c) => spawn_gen!(BrickGenerator, c.to_native()),
        SovereignTextureConfig::Plank(c) => spawn_gen!(PlankGenerator, c.to_native()),
        SovereignTextureConfig::Shingle(c) => spawn_gen!(ShingleGenerator, c.to_native()),
        SovereignTextureConfig::Stucco(c) => spawn_gen!(StuccoGenerator, c.to_native()),
        SovereignTextureConfig::Concrete(c) => spawn_gen!(ConcreteGenerator, c.to_native()),
        SovereignTextureConfig::Metal(c) => spawn_gen!(MetalGenerator, c.to_native()),
        SovereignTextureConfig::Pavers(c) => spawn_gen!(PaversGenerator, c.to_native()),
        SovereignTextureConfig::Ashlar(c) => spawn_gen!(AshlarGenerator, c.to_native()),
        SovereignTextureConfig::Cobblestone(c) => {
            spawn_gen!(CobblestoneGenerator, c.to_native())
        }
        SovereignTextureConfig::Thatch(c) => spawn_gen!(ThatchGenerator, c.to_native()),
        SovereignTextureConfig::Marble(c) => spawn_gen!(MarbleGenerator, c.to_native()),
        SovereignTextureConfig::Corrugated(c) => spawn_gen!(CorrugatedGenerator, c.to_native()),
        SovereignTextureConfig::Asphalt(c) => spawn_gen!(AsphaltGenerator, c.to_native()),
        SovereignTextureConfig::Wainscoting(c) => {
            spawn_gen!(WainscotingGenerator, c.to_native())
        }
        SovereignTextureConfig::Encaustic(c) => spawn_gen!(EncausticGenerator, c.to_native()),
    }

    handle
}

/// Drains completed foliage texture tasks and copies the generated images
/// onto their target `StandardMaterial` handles. Runs every frame.
pub fn poll_overlands_foliage_tasks(
    mut foliage_tasks: ResMut<OverlandsFoliageTasks>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut finished: Vec<(
        Handle<StandardMaterial>,
        Result<TextureMap, SymTextureError>,
        bool,
    )> = Vec::new();

    foliage_tasks.tasks.retain_mut(|(task, handle, is_card)| {
        if let Some(result) = block_on(future::poll_once(task)) {
            finished.push((handle.clone(), result, *is_card));
            false
        } else {
            true
        }
    });

    for (handle, result, is_card) in finished {
        let map = match result {
            Ok(m) => m,
            Err(e) => {
                error!("Foliage texture generation failed: {e}");
                continue;
            }
        };

        let handles = if is_card {
            map_to_images_card(map, &mut images)
        } else {
            map_to_images(map, &mut images)
        };

        if let Some(mat) = materials.get_mut(&handle) {
            mat.base_color_texture = Some(handles.albedo);
            mat.normal_map_texture = Some(handles.normal);
            mat.metallic_roughness_texture = Some(handles.roughness);
        }
    }
}
