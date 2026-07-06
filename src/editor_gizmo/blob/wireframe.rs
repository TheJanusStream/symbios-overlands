//! Wireframe presentation of the blob under edit.
//!
//! WebGL2 (the wasm deploy target) has no line polygon mode, so Bevy's
//! `WireframePlugin` is off the table. Instead the blob entity's triangle
//! mesh is swapped for a real `LineList` mesh built from its unique
//! triangle edges, and its themed material for a flat unlit line colour.
//! The original handles are stashed in [`BlobWireframeSwap`] and restored
//! on deselect; a record-driven rebuild simply respawns the prim with its
//! solid mesh and the fresh entity gets a fresh swap next frame.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use std::collections::HashSet;

use super::BlobEditContext;
use super::proxy::BlobEditAssets;

/// Stashed identity of a wireframe-swapped blob entity. Presence of the
/// component *is* the "currently swapped" flag.
#[derive(Component)]
pub(crate) struct BlobWireframeSwap {
    original_mesh: Handle<Mesh>,
    original_material: Handle<StandardMaterial>,
    /// The live line mesh. The in-drag preview writes fresh edge geometry
    /// into this same handle so the swap never has to be re-applied.
    pub(crate) line_mesh: Handle<Mesh>,
}

/// Build a `LineList` mesh of `src`'s unique triangle edges. Shares the
/// source vertex buffer (positions + normals/uvs when present — the
/// normals are meaningless to the unlit line material but keep the vertex
/// layout conventional); only the index buffer is new. Returns `None` for
/// meshes without positions or indices (never the case for blob meshes).
pub(crate) fn edge_line_mesh(src: &Mesh) -> Option<Mesh> {
    let Some(VertexAttributeValues::Float32x3(positions)) = src.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return None;
    };
    let indices: Vec<u32> = match src.indices()? {
        Indices::U16(v) => v.iter().map(|&i| i as u32).collect(),
        Indices::U32(v) => v.clone(),
    };

    let mut edges: HashSet<(u32, u32)> = HashSet::with_capacity(indices.len());
    for tri in indices.chunks_exact(3) {
        for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            edges.insert((a.min(b), a.max(b)));
        }
    }
    let mut line_indices = Vec::with_capacity(edges.len() * 2);
    for (a, b) in edges {
        line_indices.push(a);
        line_indices.push(b);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::LineList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions.clone());
    if let Some(VertexAttributeValues::Float32x3(normals)) = src.attribute(Mesh::ATTRIBUTE_NORMAL) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals.clone());
    }
    if let Some(VertexAttributeValues::Float32x2(uvs)) = src.attribute(Mesh::ATTRIBUTE_UV_0) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs.clone());
    }
    mesh.insert_indices(Indices::U32(line_indices));
    Some(mesh)
}

/// Apply the wireframe swap to the blob under edit, restore any entity
/// that stopped being the edit target, and honour the context's
/// `wireframe_dirty` one-shot (drag ended without a commit — the in-drag
/// preview may have left speculative geometry in the line mesh, so
/// re-extract from the record-accurate original).
#[allow(clippy::type_complexity)]
pub(in crate::editor_gizmo) fn swap_blob_wireframe(
    mut commands: Commands,
    mut ctx: ResMut<BlobEditContext>,
    assets: Res<BlobEditAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut swapped: Query<(
        Entity,
        &mut Mesh3d,
        &mut MeshMaterial3d<StandardMaterial>,
        &BlobWireframeSwap,
    )>,
    mut fresh: Query<
        (&mut Mesh3d, &mut MeshMaterial3d<StandardMaterial>),
        Without<BlobWireframeSwap>,
    >,
) {
    let target = ctx.active.as_ref().map(|a| a.blob_entity);

    // Restore entities that are swapped but no longer the target (node
    // deselected, or the closest-instance rule re-homed the edit).
    for (entity, mut mesh, mut mat, swap) in swapped.iter_mut() {
        if target == Some(entity) {
            continue;
        }
        mesh.0 = swap.original_mesh.clone();
        mat.0 = swap.original_material.clone();
        commands
            .entity(entity)
            .remove::<(BlobWireframeSwap, bevy::light::NotShadowCaster)>();
    }

    let Some(blob_entity) = target else {
        ctx.wireframe_dirty = false;
        return;
    };

    if let Ok((_, _, _, swap)) = swapped.get(blob_entity) {
        // Already wireframed — re-extract only when flagged.
        if ctx.wireframe_dirty {
            if let Some(line) = meshes.get(&swap.original_mesh).and_then(edge_line_mesh) {
                // Strong handle held by the swap — id is always live.
                let _ = meshes.insert(&swap.line_mesh, line);
            }
            ctx.wireframe_dirty = false;
        }
        return;
    }

    // Fresh target: extract edges from its current (solid) mesh and swap.
    // A prim mid-respawn can miss a frame here (mesh asset not yet
    // queryable); the next frame's pass picks it up.
    if let Ok((mut mesh, mut mat)) = fresh.get_mut(blob_entity) {
        let Some(line) = meshes.get(&mesh.0).and_then(edge_line_mesh) else {
            return;
        };
        let line_mesh = meshes.add(line);
        let swap = BlobWireframeSwap {
            original_mesh: mesh.0.clone(),
            original_material: mat.0.clone(),
            line_mesh: line_mesh.clone(),
        };
        mesh.0 = line_mesh;
        mat.0 = assets.line_material.clone();
        commands
            .entity(blob_entity)
            .insert((swap, bevy::light::NotShadowCaster));
        ctx.wireframe_dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri_mesh(positions: Vec<[f32; 3]>, indices: Vec<u32>) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_indices(Indices::U32(indices));
        mesh
    }

    #[test]
    fn single_triangle_yields_three_edges() {
        let mesh = tri_mesh(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![0, 1, 2],
        );
        let line = edge_line_mesh(&mesh).expect("line mesh");
        let Some(Indices::U32(idx)) = line.indices() else {
            panic!("expected u32 indices");
        };
        assert_eq!(idx.len(), 6); // 3 unique edges × 2 endpoints
        assert_eq!(line.primitive_topology(), PrimitiveTopology::LineList);
    }

    #[test]
    fn shared_quad_edge_is_deduplicated() {
        // Two triangles sharing the 1–2 diagonal: 5 unique edges, not 6.
        let mesh = tri_mesh(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
            ],
            vec![0, 1, 2, 2, 1, 3],
        );
        let line = edge_line_mesh(&mesh).expect("line mesh");
        let Some(Indices::U32(idx)) = line.indices() else {
            panic!("expected u32 indices");
        };
        assert_eq!(idx.len(), 10);
    }

    #[test]
    fn positions_survive_unchanged() {
        let positions = vec![[0.5, 1.5, -2.0], [3.0, 0.0, 0.0], [0.0, 4.0, 1.0]];
        let mesh = tri_mesh(positions.clone(), vec![0, 1, 2]);
        let line = edge_line_mesh(&mesh).expect("line mesh");
        let Some(VertexAttributeValues::Float32x3(out)) = line.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions missing");
        };
        assert_eq!(*out, positions);
    }

    #[test]
    fn unindexed_mesh_is_rejected() {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD,
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        );
        assert!(edge_line_mesh(&mesh).is_none());
    }
}
