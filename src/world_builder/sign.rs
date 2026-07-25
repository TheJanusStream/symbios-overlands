//! Spawner for [`GeneratorKind::Sign`](crate::pds::GeneratorKind::Sign): a flat panel textured with an
//! image fetched from a [`SignSource`]. The mesh is a `Plane3d` sized by
//! the variant's `size`, spanning the image exactly once; how that image
//! sits on the panel — scale, offset, rotation — rides on the material's UV
//! transform, the same one every other surface goes through (#964).
//!
//! The image fetch is decoupled from the spawn: the material starts with
//! its tint colour and `base_color_texture = None`, then
//! [`request_blob_image_filtered`] either paints synchronously (cache hit) or
//! enqueues onto the in-flight task list (cache miss / pending). The
//! poll system in `image_cache` drains completions and patches the
//! material asset directly.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::pds::{AlphaModeKind, Fp2, SignSource, SovereignMaterialSettings};

use super::compile::SpawnCtx;
use super::image_cache::{SamplerFilter, request_blob_image_filtered};
use super::material::sovereign_uv_transform;

/// Spawn a Sign entity: a textured plane with the StandardMaterial
/// toggles surfaced by the [`GeneratorKind::Sign`](crate::pds::GeneratorKind::Sign) variant. Returns the
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
    material_settings: &SovereignMaterialSettings,
    double_sided: bool,
    alpha_mode: &AlphaModeKind,
    unlit: bool,
    texture_filter: &crate::pds::TextureFilter,
    transform: Transform,
) -> Entity {
    let mesh = build_sign_mesh(size);
    let mesh_handle = ctx.meshes.add(mesh);

    let material = build_sign_material(material_settings, double_sided, alpha_mode, unlit);
    let material_handle = ctx.std_materials.add(material);

    // Kick off (or reuse) the texture fetch with the record's sampler
    // filter (#663 — pixel-art signage stays crisp under Nearest). The
    // request is a no-op for `SignSource::Unknown` and for sources with
    // empty required fields, so a freshly-defaulted Sign with no URL set
    // yet simply renders flat-coloured until the user fills in a source.
    request_blob_image_filtered(
        ctx.commands,
        ctx.blob_image_cache,
        ctx.std_materials,
        &material_handle,
        source,
        SamplerFilter::from_record(texture_filter),
    );

    let mut cmd = ctx.commands.spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle),
        transform,
    ));
    if !ctx.avatar_mode {
        cmd.insert((super::RoomEntity, super::PlacementUnit(ctx.placement_index)));
    }
    cmd.id()
}

/// Build the textured-plane mesh for a Sign: a 4-vertex quad lying in the
/// local XZ plane (Y-up normal) whose UVs span `0..1` exactly once.
///
/// Geometry only — the UV window used to be baked into these four vertices
/// (#964 moved it onto the material), which meant re-meshing to pan an image
/// and a second UV vocabulary to learn.
fn build_sign_mesh(size: &Fp2) -> Mesh {
    let half_x = size.0[0] * 0.5;
    let half_z = size.0[1] * 0.5;
    let positions: Vec<[f32; 3]> = vec![
        [-half_x, 0.0, -half_z],
        [half_x, 0.0, -half_z],
        [half_x, 0.0, half_z],
        [-half_x, 0.0, half_z],
    ];
    let normals: Vec<[f32; 3]> = vec![[0.0, 1.0, 0.0]; 4];

    // U runs along local **−X**, V along local +Z, spanning the image once —
    // the `Fit` convention every alpha card wants, and the identity the
    // material's UV transform then scales / offsets / rotates.
    //
    // The `−X` is the fix for #976, and it is forced by this quad's winding.
    // The index order below makes the **−Y** side the front face (the
    // right-hand normal of `0,1,2` is `−Y`, whatever the `+Y` normal
    // attribute says — and with `unlit` signs, which is what a profile
    // picture wants, only the winding is observable). Stand such a quad up to
    // face a viewer and you need a transform taking `−Y → −Z`, `V+ → down`
    // and `U+ → the viewer's right`; with `U+` on `+X` those three demand a
    // determinant of −1, which no rotation has. The panel could be turned to
    // face front, or drawn the right way up, or drawn unmirrored — never all
    // three. Flipping `U` here makes the set satisfiable, by
    // `quat_x(FRAC_PI_2)`.
    let uvs: Vec<[f32; 2]> = vec![[1.0, 0.0], [0.0, 0.0], [0.0, 1.0], [1.0, 1.0]];

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
/// by [`request_blob_image_filtered`] into `base_color_texture`.
///
/// The UV transform is the shared [`sovereign_uv_transform`] (#964), so
/// `uv_scale` / `uv_offset` / `uv_rotation` mean here what they mean
/// everywhere else. Sign images upload **clamp-to-edge**, so a scale above
/// `1.0` shrinks the image into the panel's near corner and stretches its
/// border across the rest — a crop/zoom control, never a tiling one.
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
        uv_transform: sovereign_uv_transform(settings),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::types::{Fp, Fp2 as PdsFp2};
    use bevy::math::Vec2;
    use bevy::mesh::VertexAttributeValues;

    /// #964: the panel's UVs span the image exactly once. Everything about
    /// *where* the image sits moved onto the material, so this mesh is pure
    /// geometry — panning a sign no longer re-meshes it.
    ///
    /// #976: and `U` runs along local **−X**, which is not cosmetic. The
    /// vertex order makes `−Y` the wound front face; with `U` on `+X` a
    /// stood-up panel cannot be face-front, upright *and* unmirrored at the
    /// same time, because the transform that would do it has determinant −1.
    #[test]
    fn the_panel_mesh_spans_the_image_once() {
        let mesh = build_sign_mesh(&PdsFp2([4.0, 2.0]));
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("no UVs");
        };
        assert_eq!(uvs, &[[1.0, 0.0], [0.0, 0.0], [0.0, 1.0], [1.0, 1.0]]);
    }

    /// #976: the quad's wound front face is `−Y`, and the standing rotation
    /// the catalogue uses puts it on `−Z` — facing whoever the prop faces.
    ///
    /// The normal *attribute* says `+Y` and disagrees; that is harmless for
    /// the unlit panels a profile picture wants, and this test exists so the
    /// discrepancy is recorded rather than rediscovered. Fixing the attribute
    /// instead of the UVs would flip which side of every existing sign is
    /// visible, so the winding is the thing held still here.
    #[test]
    fn the_wound_front_face_is_negative_y() {
        let mesh = build_sign_mesh(&PdsFp2([2.0, 2.0]));
        let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("no positions")
        };
        let Some(Indices::U32(idx)) = mesh.indices() else {
            panic!("no indices")
        };
        let v = |i: u32| Vec3::from_array(pos[i as usize]);
        let n = (v(idx[1]) - v(idx[0]))
            .cross(v(idx[2]) - v(idx[0]))
            .normalize();
        assert!(
            (n - Vec3::NEG_Y).length() < 1e-5,
            "wound front is {n:?}, not −Y — the catalogue's standing rotation \
             assumes −Y and would turn every owner panel away from the viewer"
        );
    }

    /// The migrated window reproduces the old vertex-baked sampling. A
    /// legacy `repeat = 2, offset = 0.5` became `uv_scale = 2,
    /// uv_offset = 0.25` (see `sanitize_sign`); sampling the quad's corners
    /// through the material transform must land on the same UVs the old
    /// mesh baked in: `offset + repeat · t`.
    #[test]
    fn the_material_transform_reproduces_the_legacy_uv_window() {
        let settings = SovereignMaterialSettings {
            uv_scale: Fp(2.0),
            uv_offset: PdsFp2([0.25, 0.25]),
            ..Default::default()
        };
        let material =
            build_sign_material(&settings, false, &crate::pds::AlphaModeKind::Opaque, true);
        for (corner, legacy) in [
            (Vec2::new(0.0, 0.0), Vec2::new(0.5, 0.5)),
            (Vec2::new(1.0, 0.0), Vec2::new(2.5, 0.5)),
            (Vec2::new(1.0, 1.0), Vec2::new(2.5, 2.5)),
        ] {
            let got = material.uv_transform.transform_point2(corner);
            assert!(
                (got - legacy).length() < 1e-5,
                "corner {corner:?} sampled {got:?}, legacy baked {legacy:?}"
            );
        }
    }
}
