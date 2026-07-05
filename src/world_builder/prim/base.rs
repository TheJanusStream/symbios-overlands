//! Shared raw-buffer mesh assembly for the hand-built cut / prism meshers:
//! winding reconciliation against supplied normals and the attribute-buffer
//! → `Mesh` tail every builder ends in.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// Re-wind every triangle so its face winding agrees with the supplied vertex
/// normals (front faces stay visible under back-face culling). Lets the
/// hand-built [`build_tube_mesh`](super::prisms::build_tube_mesh) /
/// [`build_bevel_mesh`](super::prisms::build_bevel_mesh) emit correct normals
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

/// Split every triangle into four (edge midpoints), `levels` times, lerping
/// positions / normals / UVs. Purely flat refinement — face shapes and
/// shading are unchanged — used to give the low-poly prims (Wedge /
/// Tetrahedron) the interior vertices the nonlinear vertex deforms (twist /
/// bend / bulge) need to show at all. Vertices are duplicated per triangle;
/// at the 1-2k-triangle scale these prims reach, sharing isn't worth the
/// bookkeeping.
pub(super) fn subdivide_flat(mesh: &mut Mesh, levels: u32) {
    use bevy::mesh::{Indices, VertexAttributeValues};
    for _ in 0..levels {
        let (Some(VertexAttributeValues::Float32x3(pos)), Some(idx)) =
            (mesh.attribute(Mesh::ATTRIBUTE_POSITION), mesh.indices())
        else {
            return;
        };
        let pos = pos.clone();
        let idx: Vec<u32> = idx.iter().map(|i| i as u32).collect();
        let nor = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
            Some(VertexAttributeValues::Float32x3(n)) => n.clone(),
            _ => return,
        };
        let uv = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
            Some(VertexAttributeValues::Float32x2(u)) => u.clone(),
            _ => return,
        };
        let mid3 = |a: [f32; 3], b: [f32; 3]| {
            [
                (a[0] + b[0]) * 0.5,
                (a[1] + b[1]) * 0.5,
                (a[2] + b[2]) * 0.5,
            ]
        };
        let mid2 = |a: [f32; 2], b: [f32; 2]| [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];

        let mut np: Vec<[f32; 3]> = Vec::with_capacity(idx.len() * 2);
        let mut nn: Vec<[f32; 3]> = Vec::with_capacity(idx.len() * 2);
        let mut nu: Vec<[f32; 2]> = Vec::with_capacity(idx.len() * 2);
        let mut ni: Vec<u32> = Vec::with_capacity(idx.len() * 4);
        for tri in idx.chunks_exact(3) {
            let (a, b, c) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
            let base = np.len() as u32;
            // Corner verts 0,1,2 then edge midpoints ab=3, bc=4, ca=5.
            np.extend_from_slice(&[
                pos[a],
                pos[b],
                pos[c],
                mid3(pos[a], pos[b]),
                mid3(pos[b], pos[c]),
                mid3(pos[c], pos[a]),
            ]);
            nn.extend_from_slice(&[
                nor[a],
                nor[b],
                nor[c],
                mid3(nor[a], nor[b]),
                mid3(nor[b], nor[c]),
                mid3(nor[c], nor[a]),
            ]);
            nu.extend_from_slice(&[
                uv[a],
                uv[b],
                uv[c],
                mid2(uv[a], uv[b]),
                mid2(uv[b], uv[c]),
                mid2(uv[c], uv[a]),
            ]);
            ni.extend_from_slice(&[
                base,
                base + 3,
                base + 5,
                base + 3,
                base + 1,
                base + 4,
                base + 5,
                base + 4,
                base + 2,
                base + 3,
                base + 4,
                base + 5,
            ]);
        }
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, np);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nn);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, nu);
        mesh.insert_indices(Indices::U32(ni));
    }
}
