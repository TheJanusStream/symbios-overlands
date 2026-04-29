//! Spawner for [`GeneratorKind::Sign`]: a flat panel textured with an
//! image fetched from a [`SignSource`]. The mesh is a `Plane3d` sized by
//! the variant's `size`, with UV coordinates pre-baked to honour
//! `uv_repeat` and `uv_offset` so the user can tile / pan the image
//! without resizing the panel itself.
//!
//! The image fetch is decoupled from the spawn: the material starts with
//! its tint colour and `base_color_texture = None`, then
//! [`request_blob_image`] either paints synchronously (cache hit) or
//! enqueues onto the in-flight task list (cache miss / pending). The
//! poll system in `image_cache` drains completions and patches the
//! material asset directly.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::pds::{AlphaModeKind, Fp2, SignSource, SovereignMaterialSettings};

use super::compile::SpawnCtx;
use super::image_cache::request_blob_image;

/// Spawn a Sign entity: a textured plane with the StandardMaterial
/// toggles surfaced by the [`GeneratorKind::Sign`] variant. Returns the
/// spawned entity so the caller can parent it under the placement
/// anchor and the recursive walker can attach children.
///
/// Avatar mode (`ctx.avatar_mode`) skips the `RoomEntity` cleanup tag
/// so the panel rides on its chassis's child despawn — matching the
/// existing primitive spawner's avatar-mode behaviour.
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_sign_entity(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    source: &SignSource,
    size: &Fp2,
    uv_repeat: &Fp2,
    uv_offset: &Fp2,
    material_settings: &SovereignMaterialSettings,
    double_sided: bool,
    alpha_mode: &AlphaModeKind,
    unlit: bool,
    transform: Transform,
) -> Entity {
    let mesh = build_sign_mesh(size, uv_repeat, uv_offset);
    let mesh_handle = ctx.meshes.add(mesh);

    let material = build_sign_material(material_settings, double_sided, alpha_mode, unlit);
    let material_handle = ctx.std_materials.add(material);

    // Kick off (or reuse) the texture fetch. `request_blob_image` is a
    // no-op for `SignSource::Unknown` and for sources with empty
    // required fields, so a freshly-defaulted Sign with no URL set yet
    // simply renders flat-coloured until the user fills in a source.
    request_blob_image(
        ctx.commands,
        ctx.blob_image_cache,
        ctx.std_materials,
        &material_handle,
        source,
    );

    let mut cmd = ctx.commands.spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle),
        transform,
    ));
    if !ctx.avatar_mode {
        cmd.insert(super::RoomEntity);
    }
    cmd.id()
}

/// Build the textured-plane mesh for a Sign. Bevy's `Plane3d::mesh()` does
/// not expose UV repeat/offset, so we hand-roll a 4-vertex quad lying in
/// the local XZ plane (Y-up normal) with UVs computed from the variant's
/// `uv_repeat` and `uv_offset`.
fn build_sign_mesh(size: &Fp2, uv_repeat: &Fp2, uv_offset: &Fp2) -> Mesh {
    let half_x = size.0[0] * 0.5;
    let half_z = size.0[1] * 0.5;
    let positions: Vec<[f32; 3]> = vec![
        [-half_x, 0.0, -half_z],
        [half_x, 0.0, -half_z],
        [half_x, 0.0, half_z],
        [-half_x, 0.0, half_z],
    ];
    let normals: Vec<[f32; 3]> = vec![[0.0, 1.0, 0.0]; 4];

    // UV layout: U runs along local +X, V runs along local +Z. The
    // canonical [0,1] → [0,1] grid is multiplied by the repeat factor
    // and shifted by the offset; the StandardMaterial sampler wraps,
    // so an offset of 0.5 cleanly recentres a tiled texture.
    let ru = uv_repeat.0[0];
    let rv = uv_repeat.0[1];
    let ou = uv_offset.0[0];
    let ov = uv_offset.0[1];
    let uvs: Vec<[f32; 2]> = vec![[ou, ov], [ou + ru, ov], [ou + ru, ov + rv], [ou, ov + rv]];

    let indices = Indices::U32(vec![0, 1, 2, 0, 2, 3]);

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(indices);
    let _ = mesh.generate_tangents();
    mesh
}

/// Build the Sign's `StandardMaterial` from the variant's PBR settings
/// and the panel-specific toggles. The base colour is the tint applied
/// over the texture; emission and roughness/metallic carry through; the
/// procedural texture slot (`material.texture`) is intentionally
/// ignored — the Sign's image is the texture, painted asynchronously
/// by [`request_blob_image`] into `base_color_texture`.
fn build_sign_material(
    settings: &SovereignMaterialSettings,
    double_sided: bool,
    alpha_mode: &AlphaModeKind,
    unlit: bool,
) -> StandardMaterial {
    let base_color = Color::srgb(
        settings.base_color.0[0],
        settings.base_color.0[1],
        settings.base_color.0[2],
    );
    let emissive = LinearRgba::rgb(
        settings.emission_color.0[0] * settings.emission_strength.0,
        settings.emission_color.0[1] * settings.emission_strength.0,
        settings.emission_color.0[2] * settings.emission_strength.0,
    );
    let bevy_alpha = match alpha_mode {
        AlphaModeKind::Opaque => AlphaMode::Opaque,
        AlphaModeKind::Mask { cutoff } => AlphaMode::Mask(cutoff.0),
        AlphaModeKind::Blend => AlphaMode::Blend,
        // Forward-compat: an unknown alpha mode from a future engine
        // version falls back to Opaque so the panel still renders
        // instead of silently disappearing.
        AlphaModeKind::Unknown => AlphaMode::Opaque,
    };

    StandardMaterial {
        base_color,
        emissive,
        perceptual_roughness: settings.roughness.0,
        metallic: settings.metallic.0,
        alpha_mode: bevy_alpha,
        double_sided,
        cull_mode: if double_sided {
            None
        } else {
            Some(bevy::render::render_resource::Face::Back)
        },
        unlit,
        ..default()
    }
}
