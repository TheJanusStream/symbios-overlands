//! Convex-hull collider fallback for tortured / cut meshes whose analytical
//! shape would diverge from the visible geometry. (The per-variant analytical
//! colliders live on each [`super::shapes::PrimitiveShape`] impl.)

use avian3d::prelude::*;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;

pub(super) fn convex_hull_from_mesh(mesh: &Mesh) -> Option<Collider> {
    let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION)?;
    let VertexAttributeValues::Float32x3(verts) = positions else {
        return None;
    };
    if verts.len() < 4 {
        return None;
    }
    let points: Vec<Vec3> = verts.iter().map(|p| Vec3::from_array(*p)).collect();
    Collider::convex_hull(points)
}
