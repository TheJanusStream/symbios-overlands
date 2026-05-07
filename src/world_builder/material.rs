//! Water volume spawning and procedural material building. Texture
//! generation is dispatched through
//! [`bevy_symbios_texture::build_procedural_material_async`], which spawns a
//! [`bevy_symbios_texture::PatchMaterialTextures`] task entity and lets the
//! upstream `patch_procedural_material_textures` system (registered by
//! `SymbiosTexturePlugin`) write generated images into the material as soon
//! as they're ready — no Overlands-side polling resource required.

use bevy::prelude::*;
use bevy_symbios_texture::build_procedural_material_async;

use crate::config::terrain as tcfg;
use crate::pds::{Environment, SovereignMaterialSettings, WaterSurface};
use crate::terrain::WaterVolume;
use crate::water::{WaterExtension, WaterMaterial, WaterPlane, WaterSurfaces, WaterUniforms};

use super::RoomEntity;
use super::compile::SpawnCtx;

/// Translate a [`WaterSurface`] + [`Environment`] pair into the uniform block
/// the water shader reads. Every value that the shader depends on flows
/// through this function so the egui widgets, raw JSON edits, and peer
/// broadcasts all converge on the same GPU state.
fn build_water_uniforms(surface: &WaterSurface, env: &Environment) -> WaterUniforms {
    WaterUniforms {
        shallow_color: Vec4::from_array(surface.shallow_color.0),
        deep_color: Vec4::from_array(surface.deep_color.0),
        scatter_color: Vec4::new(
            env.water_scatter_color.0[0],
            env.water_scatter_color.0[1],
            env.water_scatter_color.0[2],
            0.0,
        ),
        wave_direction: Vec2::from_array(surface.wave_direction.0),
        wave_scale: surface.wave_scale.0,
        wave_speed: surface.wave_speed.0,
        wave_choppiness: surface.wave_choppiness.0,
        roughness: surface.roughness.0,
        metallic: surface.metallic.0,
        reflectance: surface.reflectance.0,
        foam_amount: surface.foam_amount.0,
        normal_scale_near: env.water_normal_scale_near.0,
        normal_scale_far: env.water_normal_scale_far.0,
        refraction_strength: env.water_refraction_strength.0,
        sun_glitter: env.water_sun_glitter.0,
        shore_foam_width: env.water_shore_foam_width.0,
        flow_amount: surface.flow_amount.0,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_water_volume(
    commands: &mut Commands,
    level_offset: f32,
    surface: &WaterSurface,
    env: &Environment,
    placement_tf: Transform,
    world_extent: f32,
    meshes: &mut Assets<Mesh>,
    water_materials: &mut Assets<WaterMaterial>,
    water_surfaces: &mut WaterSurfaces,
) -> Entity {
    let base_wl = tcfg::water::LEVEL_FACTOR * tcfg::HEIGHT_SCALE;
    let wl = (base_wl + level_offset).max(0.001);

    // Straight-down view colour seeds the StandardMaterial base colour for
    // any non-shader-overridden path (shadow-caster fallback, editor outline,
    // etc.). The shader re-derives the view-dependent blend itself.
    let base = surface.shallow_color.0;
    let water_mat = water_materials.add(WaterMaterial {
        base: StandardMaterial {
            base_color: Color::srgba(base[0], base[1], base[2], base[3]),
            perceptual_roughness: surface.roughness.0,
            metallic: surface.metallic.0,
            alpha_mode: AlphaMode::Blend,
            // Back-face cull the plane: viewed from underwater the surface
            // contributes nothing, matching the previous Cuboid+discard
            // behaviour without the wasted side-face rasterisation.
            cull_mode: Some(bevy::render::render_resource::Face::Back),
            ..default()
        },
        extension: WaterExtension {
            uniforms: build_water_uniforms(surface, env),
        },
    });

    // Flat `Plane3d` at the water-surface altitude. The previous iteration
    // spawned a 1×1×1 `Cuboid` scaled to `(world_extent, wl, world_extent)`
    // and then discarded five out of six faces in the fragment shader — a
    // lot of rasterisation work for zero visible fragments, and
    // `fwidth`-after-`discard` is only well-defined under uniform quad
    // control flow. The plane eliminates both.
    let mut tf = placement_tf;
    tf.translation.y += wl;

    let half_extent = world_extent / 2.0;

    // Register this surface in the runtime lookup so per-frame physics
    // (rover buoyancy, scatter biome filter) can find it without re-walking
    // the record. The mesh half-extent is recorded BEFORE the transform's
    // scale is applied — `WaterSurfaces::surface_at` re-applies the scale
    // via the inverse transform when testing containment.
    water_surfaces.planes.push(WaterPlane {
        world_from_local: tf,
        local_half_extents: Vec2::splat(half_extent),
        flow_strength: surface.flow_strength.0,
    });

    commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(half_extent)))),
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
    build_procedural_material(ctx.commands, ctx.std_materials, ctx.images, settings)
}

/// Free-function core of [`spawn_procedural_material`] — takes the three
/// resources upstream's
/// [`bevy_symbios_texture::build_procedural_material_async`] needs instead
/// of the full [`SpawnCtx`], so avatar builders can reuse it without
/// constructing a world-builder context.
///
/// Returns a [`StandardMaterial`] handle whose texture slots are populated
/// asynchronously once the texture-generator task finishes. The actual
/// patching is performed by `patch_procedural_material_textures`,
/// auto-registered by [`bevy_symbios_texture::SymbiosTexturePlugin`].
pub fn build_procedural_material(
    commands: &mut Commands,
    std_materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    settings: &SovereignMaterialSettings,
) -> Handle<StandardMaterial> {
    let native = settings.to_native();
    // 512×512 matches the size Overlands has historically generated; the
    // upstream helper hands every variant the same dimensions so foliage
    // cards and tiling surfaces share the cache layout. No `TextureCache`
    // is supplied — Overlands' generator-level caches already amortise the
    // common case; cross-generator dedup at the texture level is filed as
    // a follow-up.
    build_procedural_material_async(commands, std_materials, images, None, &native, 512, 512)
}
