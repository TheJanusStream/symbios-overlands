//! Portal entity spawning + async avatar-picture fetch polling.

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use std::collections::HashMap;

use crate::pds::Fp3;

use super::compile::SpawnCtx;
use super::{PortalAvatarTask, PortalMarker, RoomEntity};

/// DID-keyed coalescing cache for portal-avatar fetches. A room scattering
/// portals to the same DID would otherwise issue one HTTPS round trip and
/// one image decode per portal entity; here, the first portal records a
/// `Pending` task and every later portal to the same DID enqueues its
/// top-face material on that pending list. When the task finishes, the
/// poll system paints the resulting texture into every queued material at
/// once and promotes the entry to `Ready` so any *future* portal to the
/// same DID paints synchronously without a fetch.
#[derive(Resource, Default)]
pub struct PortalAvatarCache {
    pub by_did: HashMap<String, PortalAvatarCacheEntry>,
}

pub enum PortalAvatarCacheEntry {
    /// HTTPS fetch is in flight. Each subsequent portal to this DID pushes
    /// its top-face material handle onto this list; the poll system drains
    /// the whole list on completion.
    Pending(Vec<Handle<StandardMaterial>>),
    /// Image is already GPU-resident. Subsequent portals paint
    /// synchronously by cloning the handle into their own material.
    Ready(Handle<Image>),
}

impl PortalAvatarCache {
    pub fn clear(&mut self) {
        self.by_did.clear();
    }
}

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
        match ctx.portal_avatar_cache.by_did.get_mut(target_did) {
            // The texture is already loaded — paint synchronously, no
            // network task. This is the fast path for repeated visits to
            // the same DID across compile passes (or for the second + Nth
            // portal to a DID whose first portal already finished
            // fetching).
            Some(PortalAvatarCacheEntry::Ready(img_handle)) => {
                let img_handle = img_handle.clone();
                if let Some(mat) = ctx.std_materials.get_mut(&top_mat) {
                    mat.base_color_texture = Some(img_handle);
                }
            }
            // A task for this DID is already in flight. Enqueue our
            // material so the existing task's poll completion paints us
            // without spawning a redundant HTTPS fetch.
            Some(PortalAvatarCacheEntry::Pending(list)) => {
                list.push(top_mat.clone());
            }
            // First portal to this DID this session — register a pending
            // entry and start exactly one fetch task. `IoTaskPool` (not
            // `AsyncComputeTaskPool`) is the right home for a blocking
            // ATProto HTTP fetch; the compute pool is sized to physical
            // cores and pinning every worker on a socket read would hang
            // procedural texture / terrain generation.
            None => {
                ctx.portal_avatar_cache.by_did.insert(
                    target_did.to_string(),
                    PortalAvatarCacheEntry::Pending(vec![top_mat.clone()]),
                );
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
                    did: target_did.to_string(),
                });
            }
        }
    }

    parent
}

/// Drain finished portal-avatar fetches and paint the resulting texture onto
/// every portal top-face material that was waiting on this DID. Failed
/// fetches leave each pending material at the fallback white so the portal
/// is still visible and interactable, and promote the entry to a
/// `Pending(empty)` -free state by removing it.
pub(super) fn poll_portal_avatar_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PortalAvatarTask)>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: ResMut<PortalAvatarCache>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        commands.entity(entity).despawn();

        // Take ownership of the pending list. If the entry has already
        // been promoted to `Ready` (only possible if a duplicate task
        // somehow raced) or removed by a logout, drop this result.
        let pending = match cache.by_did.remove(&task.did) {
            Some(PortalAvatarCacheEntry::Pending(list)) => list,
            Some(other) => {
                cache.by_did.insert(task.did.clone(), other);
                continue;
            }
            None => continue,
        };

        let Some(bytes) = result.bytes else {
            // Fetch failed. Drop the pending entry so a future portal to
            // the same DID gets a fresh attempt instead of permanently
            // displaying white.
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
        let img_handle = images.add(img);
        for mat_handle in pending {
            if let Some(mat) = materials.get_mut(&mat_handle) {
                mat.base_color_texture = Some(img_handle.clone());
            }
        }
        cache
            .by_did
            .insert(task.did.clone(), PortalAvatarCacheEntry::Ready(img_handle));
    }
}
