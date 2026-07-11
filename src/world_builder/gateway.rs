//! Gateway entity spawning (#747). The gateway is the walk-in zone of a
//! themed gate structure: a sensor volume carrying [`GatewayMarker`],
//! rendered as a faint emissive veil so even an unthemed zone reads as
//! interactive. The surrounding structure comes from the catalogue
//! entry's sibling prims, not from this node — and the destination list
//! is resolved at interaction time (the room owner's mutual follows),
//! so the entity carries no target of its own.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::pds::Fp3;

use super::compile::SpawnCtx;
use super::{GatewayMarker, RoomEntity};

pub(super) fn spawn_gateway_entity(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    size: &Fp3,
    transform: Transform,
) -> Entity {
    let [sx, sy, sz] = size.0;

    // A quiet cousin of the portal cube: translucent enough to walk into
    // without obscuring the themed frame around it, emissive enough to
    // read as "active" under any sky.
    let veil_mat = ctx.std_materials.add(StandardMaterial {
        base_color: Color::srgba(0.75, 0.9, 1.0, 0.12),
        alpha_mode: AlphaMode::Blend,
        emissive: LinearRgba::rgb(0.35, 0.6, 0.9),
        double_sided: true,
        cull_mode: None,
        ..default()
    });

    ctx.commands
        .spawn((
            Mesh3d(ctx.meshes.add(Cuboid::new(sx, sy, sz))),
            MeshMaterial3d(veil_mat),
            transform,
            Collider::cuboid(sx, sy, sz),
            Sensor,
            GatewayMarker,
            RoomEntity,
            super::PlacementUnit(ctx.placement_index),
        ))
        .id()
}
