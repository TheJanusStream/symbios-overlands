//! Construct / Prim node spawners plus parametric mesh and collider helpers.

use std::collections::HashMap;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::pds::{PrimNode, PrimShape};

use super::compile::{SpawnCtx, transform_from_data};
use super::lsystem::settings_fingerprint;
use super::material::spawn_procedural_material;
use super::{PrimMarker, RoomEntity, apply_traits};

pub(super) fn spawn_construct_entity(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    root: &PrimNode,
    generator_ref: &str,
    placement_tf: Transform,
) -> Entity {
    let mut material_cache: HashMap<u64, Handle<StandardMaterial>> = HashMap::new();

    // World-space anchor. Owns the rigid body and the `RoomEntity` marker so
    // the whole subtree despawns together on the next compile pass. No mesh
    // or material — it only exists to isolate world space from blueprint
    // space.
    let anchor = ctx
        .commands
        .spawn((
            placement_tf,
            Visibility::default(),
            RigidBody::Static,
            RoomEntity,
        ))
        .id();

    // Blueprint root, spawned in its own local transform. Attaching it as a
    // child of the anchor composes the placement's world transform with the
    // blueprint root's local transform via Bevy's hierarchy, so visually the
    // whole tree lands exactly where the `Placement::Absolute` asked.
    let mut path: Vec<usize> = Vec::new();
    let root_child = spawn_prim_tree(
        ctx,
        root,
        transform_from_data(&root.transform),
        &mut material_cache,
        generator_ref,
        &mut path,
    );
    ctx.commands.entity(anchor).add_child(root_child);

    apply_traits(ctx.commands, anchor, ctx.record, generator_ref);
    anchor
}

fn spawn_prim_tree(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    node: &PrimNode,
    tf: Transform,
    material_cache: &mut HashMap<u64, Handle<StandardMaterial>>,
    generator_ref: &str,
    path: &mut Vec<usize>,
) -> Entity {
    let mesh = mesh_for_prim_shape(ctx.meshes, &node.shape);

    let hash = settings_fingerprint(&node.material);
    let material = if let Some(h) = material_cache.get(&hash) {
        h.clone()
    } else {
        let h = spawn_procedural_material(ctx, &node.material);
        material_cache.insert(hash, h.clone());
        h
    };

    let mut cmd = ctx.commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(material),
        tf,
        PrimMarker {
            generator_ref: generator_ref.to_string(),
            path: path.clone(),
        },
        // Per-prim `RoomEntity` so the compile-pass cleanup finds every
        // prim directly, not just through the anchor's recursive despawn.
        // A gizmo-detached prim has no `ChildOf` link back to the anchor
        // and would otherwise survive the rebuild as a dangling ghost.
        RoomEntity,
    ));
    if node.solid
        && let Some(collider) = collider_for_prim_shape(&node.shape)
    {
        cmd.insert(collider);
    }
    let entity = cmd.id();

    for (i, child_node) in node.children.iter().enumerate() {
        path.push(i);
        let child_tf = transform_from_data(&child_node.transform);
        let child = spawn_prim_tree(
            ctx,
            child_node,
            child_tf,
            material_cache,
            generator_ref,
            path,
        );
        ctx.commands.entity(entity).add_child(child);
        path.pop();
    }
    entity
}

/// Build the parametric mesh for a [`PrimShape`]. The node's
/// [`TransformData::scale`] is applied via Bevy's transform hierarchy on
/// top of the shape's intrinsic dimensions.
fn mesh_for_prim_shape(meshes: &mut Assets<Mesh>, shape: &PrimShape) -> Handle<Mesh> {
    let mut mesh = match shape {
        PrimShape::Cuboid { size } => Cuboid::new(size.0[0], size.0[1], size.0[2]).mesh().build(),
        PrimShape::Sphere { radius, resolution } => Sphere::new(radius.0)
            .mesh()
            .ico(*resolution)
            .unwrap_or_else(|_| Sphere::new(radius.0).mesh().build()),
        PrimShape::Cylinder {
            radius,
            height,
            resolution,
        } => Cylinder::new(radius.0, height.0)
            .mesh()
            .resolution(*resolution)
            .build(),
        PrimShape::Capsule {
            radius,
            length,
            latitudes,
            longitudes,
        } => Capsule3d::new(radius.0, length.0)
            .mesh()
            .latitudes(*latitudes)
            .longitudes(*longitudes)
            .build(),
        PrimShape::Cone {
            radius,
            height,
            resolution,
        } => Cone::new(radius.0, height.0)
            .mesh()
            .resolution(*resolution)
            .build(),
        PrimShape::Torus {
            minor_radius,
            major_radius,
            minor_resolution,
            major_resolution,
        } => Torus {
            minor_radius: minor_radius.0,
            major_radius: major_radius.0,
        }
        .mesh()
        .minor_resolution(*minor_resolution as usize)
        .major_resolution(*major_resolution as usize)
        .build(),
        PrimShape::Plane { size, subdivisions } => {
            Plane3d::new(Vec3::Y, Vec2::new(size.0[0] / 2.0, size.0[1] / 2.0))
                .mesh()
                .subdivisions(*subdivisions)
                .build()
        }
        PrimShape::Tetrahedron { size } => {
            let s = size.0;
            let p0 = Vec3::new(0.0, 1.0, 0.0) * s;
            let p1 = Vec3::new(-1.0, -1.0, 1.0).normalize() * s;
            let p2 = Vec3::new(1.0, -1.0, 1.0).normalize() * s;
            let p3 = Vec3::new(0.0, -1.0, -1.0).normalize() * s;
            Tetrahedron::new(p0, p1, p2, p3).mesh().build()
        }
    };
    let _ = mesh.generate_tangents();
    meshes.add(mesh)
}

/// Build the Avian collider matching a [`PrimShape`]'s mesh. `Torus` and
/// `Plane` fall back to bounding cuboids because Avian 0.6 has no native
/// primitives for them; `Tetrahedron` uses a convex hull.
fn collider_for_prim_shape(shape: &PrimShape) -> Option<Collider> {
    Some(match shape {
        PrimShape::Cuboid { size } => Collider::cuboid(size.0[0], size.0[1], size.0[2]),
        PrimShape::Sphere { radius, .. } => Collider::sphere(radius.0),
        PrimShape::Cylinder { radius, height, .. } => Collider::cylinder(radius.0, height.0),
        PrimShape::Capsule { radius, length, .. } => Collider::capsule(radius.0, length.0),
        PrimShape::Cone { radius, height, .. } => Collider::cone(radius.0, height.0),
        PrimShape::Torus {
            minor_radius,
            major_radius,
            ..
        } => Collider::cuboid(
            major_radius.0 + minor_radius.0,
            minor_radius.0 * 2.0,
            major_radius.0 + minor_radius.0,
        ),
        PrimShape::Plane { size, .. } => Collider::cuboid(size.0[0], 0.01, size.0[1]),
        PrimShape::Tetrahedron { size } => {
            let s = size.0;
            let p0 = Vec3::new(0.0, 1.0, 0.0) * s;
            let p1 = Vec3::new(-1.0, -1.0, 1.0).normalize() * s;
            let p2 = Vec3::new(1.0, -1.0, 1.0).normalize() * s;
            let p3 = Vec3::new(0.0, -1.0, -1.0).normalize() * s;
            Collider::convex_hull(vec![p0, p1, p2, p3]).unwrap_or_else(|| Collider::sphere(s))
        }
    })
}
