//! Portal entity spawning + async avatar-picture fetch polling.

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::tasks::IoTaskPool;

use crate::pds::Fp3;

use super::compile::SpawnCtx;
use super::{PortalAvatarTask, PortalMarker, RoomEntity};

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

    if !is_local {
        // `IoTaskPool` — not `AsyncComputeTaskPool` — is the right home for
        // a blocking ATProto HTTP fetch. The compute pool is sized to the
        // physical CPU-core count and is shared with terrain generation
        // and procedural texture baking; a room with enough portals
        // scheduled for fetch would pin every compute worker on a socket
        // read and hang the client's `Loading` screen indefinitely.
        let pool = IoTaskPool::get();
        let did_clone = target_did.to_string();
        let task = pool.spawn(async move {
            let fut = crate::avatar::fetch_avatar_bytes(did_clone);
            #[cfg(target_arch = "wasm32")]
            {
                fut.await
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(fut)
            }
        });
        ctx.commands.spawn(PortalAvatarTask {
            task,
            material: top_mat,
        });
    }

    parent
}

/// Drain finished portal-avatar fetches and paint the resulting texture onto
/// the portal top face's material. Failed fetches leave the material at the
/// fallback white so the portal is still visible and interactable.
pub(super) fn poll_portal_avatar_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PortalAvatarTask)>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        commands.entity(entity).despawn();
        let Some(bytes) = result.bytes else {
            continue;
        };
        let Ok(dyn_img) = image::load_from_memory(&bytes) else {
            continue;
        };
        let img = Image::from_dynamic(
            dyn_img,
            true,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        if let Some(mat) = materials.get_mut(&task.material) {
            mat.base_color_texture = Some(images.add(img));
        }
    }
}
