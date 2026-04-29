//! Portal entity spawning. The top-face profile picture is delegated to
//! the shared [`BlobImageCache`](super::image_cache::BlobImageCache) via
//! a [`SignSource::DidPfp`] request, so portals and Sign generators
//! pointing at the same DID's profile picture coalesce onto a single
//! HTTPS round trip.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::pds::{Fp3, SignSource};

use super::compile::SpawnCtx;
use super::image_cache::request_blob_image;
use super::{PortalMarker, RoomEntity};

pub(super) fn spawn_portal_entity(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    target_did: &str,
    target_pos: &Fp3,
    transform: Transform,
) -> Entity {
    let is_local = ctx.current_room.map(|r| r.0 == target_did).unwrap_or(false);

    let cube_mat = ctx.std_materials.add(StandardMaterial {
        base_color: Color::srgba(0.2, 0.8, 1.0, 0.4),
        alpha_mode: AlphaMode::Blend,
        emissive: LinearRgba::rgb(0.5, 1.0, 2.0),
        double_sided: true,
        cull_mode: None,
        ..default()
    });

    let parent = ctx
        .commands
        .spawn((
            Mesh3d(ctx.meshes.add(Cuboid::new(1.5, 2.0, 1.5))),
            MeshMaterial3d(cube_mat),
            transform,
            Collider::cuboid(1.5, 2.0, 1.5),
            Sensor,
            PortalMarker {
                target_did: target_did.to_string(),
                target_pos: Vec3::from_array(target_pos.0),
            },
            RoomEntity,
        ))
        .id();

    // Top face — a thin plane pinned just above the cube's top so it renders
    // on top of the translucent volume without z-fighting. `unlit` keeps the
    // profile picture legible at any sun angle.
    let top_mat = ctx.std_materials.add(StandardMaterial {
        base_color: Color::WHITE,
        unlit: true,
        ..default()
    });

    let top_face = ctx
        .commands
        .spawn((
            Mesh3d(ctx.meshes.add(Plane3d::new(Vec3::Y, Vec2::new(0.75, 0.75)))),
            MeshMaterial3d(top_mat.clone()),
            Transform::from_xyz(0.0, 1.01, 0.0),
        ))
        .id();
    ctx.commands.entity(parent).add_child(top_face);

    // An intra-room portal points at the same DID we're rendering — its
    // top face would otherwise display the local user's own pfp, which is
    // visually redundant and confuses identity. Skip the fetch and leave
    // the panel white.
    if !is_local {
        request_blob_image(
            ctx.commands,
            ctx.blob_image_cache,
            ctx.std_materials,
            &top_mat,
            &SignSource::DidPfp {
                did: target_did.to_string(),
            },
        );
    }

    parent
}
