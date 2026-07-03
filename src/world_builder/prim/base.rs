//! Shared raw-buffer mesh assembly for the hand-built cut / prism meshers:
//! winding reconciliation against supplied normals and the attribute-buffer
//! → `Mesh` tail every builder ends in.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// Re-wind every triangle so its face winding agrees with the supplied vertex
/// normals (front faces stay visible under back-face culling). Lets the
/// hand-built [`build_tube_mesh`] / [`build_bevel_mesh`] emit correct normals
/// without also hand-proving every triangle's index order.
pub(super) fn orient_to_normals(pos: &[[f32; 3]], nor: &[[f32; 3]], idx: &mut [u32]) {
    for tri in idx.chunks_exact_mut(3) {
        let (a, b, c) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let p0 = Vec3::from_array(pos[a]);
        let face = (Vec3::from_array(pos[b]) - p0).cross(Vec3::from_array(pos[c]) - p0);
        let vn = Vec3::from_array(nor[a]) + Vec3::from_array(nor[b]) + Vec3::from_array(nor[c]);
        if face.dot(vn) < 0.0 {
            tri.swap(1, 2);
        }
    }
}

/// Assemble a CPU mesh from raw attribute buffers, fixing winding against the
/// normals and generating tangents — the shared tail of the hand-built tube /
/// bevel builders.
pub(super) fn mesh_from_parts(
    pos: Vec<[f32; 3]>,
    nor: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    mut idx: Vec<u32>,
) -> Mesh {
    orient_to_normals(&pos, &nor, &mut idx);
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_indices(Indices::U32(idx));
    let _ = mesh.generate_tangents();
    mesh
}
