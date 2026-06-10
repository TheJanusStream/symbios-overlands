//! Shared quad mesh + sprite-sheet atlas frame meshes for particles.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::pds::AnimationFrameMode;

/// Cached unit quad mesh — every untextured / single-frame particle
/// uses this handle. The quad is a 1×1 square in the local XY plane
/// facing local +Z; the tick system rotates it to face the camera
/// (billboard) or align with velocity each frame, and `Transform.scale`
/// applies the per-particle size.
#[derive(Resource)]
pub struct ParticleQuadMesh(pub Handle<Mesh>);

impl FromWorld for ParticleQuadMesh {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        ParticleQuadMesh(meshes.add(atlas_frame_mesh(1, 1, 0)))
    }
}

/// Lazily-built per-frame quad mesh cache for atlas-textured particles.
/// Keyed by `(rows, cols, frame_idx)`. Bounded at 256 unique entries
/// per `(rows, cols)` configuration by the sanitiser's
/// `MAX_PARTICLE_ATLAS_DIM = 16` cap, so cache memory stays well-defined.
#[derive(Resource, Default)]
pub struct ParticleAtlasMeshes {
    pub by_frame: std::collections::HashMap<(u32, u32, u32), Handle<Mesh>>,
}

impl ParticleAtlasMeshes {
    /// Look up or build the quad mesh for a given atlas frame. Cells
    /// are addressed in row-major order (`frame_idx = row * cols +
    /// col`) so cycling animations sweep across each row before
    /// dropping to the next.
    pub fn get_or_create(
        &mut self,
        meshes: &mut Assets<Mesh>,
        rows: u32,
        cols: u32,
        frame_idx: u32,
    ) -> Handle<Mesh> {
        let key = (rows, cols, frame_idx);
        if let Some(handle) = self.by_frame.get(&key) {
            return handle.clone();
        }
        let handle = meshes.add(atlas_frame_mesh(rows, cols, frame_idx));
        self.by_frame.insert(key, handle.clone());
        handle
    }
}

/// Build a 4-vertex quad whose UVs map to one cell of an
/// `rows × cols` sprite-sheet atlas. `(rows=1, cols=1, frame_idx=0)`
/// reproduces the full-image UVs that the v1 untextured quad used,
/// keeping the cache key uniform for the no-atlas case.
fn atlas_frame_mesh(rows: u32, cols: u32, frame_idx: u32) -> Mesh {
    let positions: Vec<[f32; 3]> = vec![
        [-0.5, -0.5, 0.0],
        [0.5, -0.5, 0.0],
        [0.5, 0.5, 0.0],
        [-0.5, 0.5, 0.0],
    ];
    let normals: Vec<[f32; 3]> = vec![[0.0, 0.0, 1.0]; 4];

    // Atlas frame UVs — assumes row-major order, top-left origin.
    let cols = cols.max(1);
    let rows = rows.max(1);
    let total = rows * cols;
    let frame = frame_idx.min(total.saturating_sub(1));
    let row = frame / cols;
    let col = frame % cols;
    let u_step = 1.0 / cols as f32;
    let v_step = 1.0 / rows as f32;
    let u0 = col as f32 * u_step;
    let v0 = row as f32 * v_step;
    let u1 = u0 + u_step;
    let v1 = v0 + v_step;
    // The quad winds bottom-left → bottom-right → top-right →
    // top-left in local XY. Map `v0` (top of cell) to local-top
    // verts and `v1` (bottom of cell) to local-bottom verts so atlas
    // frames render right-side-up.
    let uvs: Vec<[f32; 2]> = vec![[u0, v1], [u1, v1], [u1, v0], [u0, v0]];

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

/// Compute the active frame index for a particle given its frame
/// mode, age, and the atlas dimensions. `Still` always returns 0;
/// `RandomFrame` returns the spawn-baked index unchanged;
/// `OverLifetime` cycles through frames at the configured `fps`,
/// modulo the total cell count.
pub(super) fn current_frame_index(
    mode: &AnimationFrameMode,
    age: f32,
    spawn_index: u32,
    atlas_dim: Option<(u32, u32)>,
) -> u32 {
    let (rows, cols) = atlas_dim.unwrap_or((1, 1));
    let total = (rows.max(1) * cols.max(1)).max(1);
    match mode {
        AnimationFrameMode::Still => 0,
        AnimationFrameMode::RandomFrame => spawn_index % total,
        AnimationFrameMode::OverLifetime { fps } => {
            let idx = (age * fps.0).floor().max(0.0) as u32;
            idx % total
        }
        AnimationFrameMode::Unknown => 0,
    }
}
