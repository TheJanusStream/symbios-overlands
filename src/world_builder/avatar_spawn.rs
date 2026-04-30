//! Public avatar-side wrapper around the room compiler's recursive
//! [`super::compile::spawn_generator`] machinery. Routes a
//! [`Generator`] tree through the same dispatch arms (primitives,
//! LSystem, Shape) that rooms use, but with `avatar_mode = true` on
//! [`super::compile::SpawnCtx`] so each spawn arm:
//!
//!   1. Skips the `RoomEntity` cleanup tag (avatar children live under
//!      the chassis hierarchy and despawn through it).
//!   2. Skips the `PrimMarker` editor-gizmo tag (avatars have no
//!      gizmo in v1).
//!   3. Skips per-primitive collider attachment (the locomotion
//!      preset's chassis collider is the only physics body on an
//!      avatar).
//!
//! Terrain / Water / Portal kinds are stripped upstream by
//! [`crate::pds::sanitize_avatar_visuals`], so the unreachable arms
//! never fire here. The avatar caller still provides the same
//! mutable references the room compiler needs (caches, foliage tasks,
//! water surface registry, …) — most of those are unused for avatars
//! but the borrow shapes have to match the existing `SpawnCtx`.

use std::collections::HashSet;

use bevy::prelude::*;
use bevy_symbios::materials::MaterialPalette;

use super::compile::{GeneratorCaches, SpawnCtx, spawn_generator};
use super::image_cache::BlobImageCache;
use super::{OverlandsFoliageTasks, PropMeshAssets};
use crate::pds::{Generator, RoomRecord};
use crate::state::CurrentRoomDid;
use crate::terrain::{FinishedHeightMap, OutgoingTerrain, TerrainMesh};
use crate::water::{WaterMaterial, WaterSurfaces};

/// Compile an avatar's `visuals` Generator tree into Bevy entities,
/// parented under `chassis`. The visuals root's transform composes
/// with each node's local transform; the chassis (a parent rigid
/// body) provides the world-space anchor.
///
/// Existing chassis children must be despawned by the caller before
/// invoking — the avatar mode does not tag entities with `RoomEntity`,
/// so the compile pass's cleanup query won't reach them.
///
/// Mutable references mirror the room compiler's
/// [`super::compile::SpawnCtx`]; most are unused for avatars (water,
/// portal, terrain, room-record) but the borrow shapes must match the
/// existing struct. Pass an `&RoomRecord::default()` for `record` and
/// any matching `Query` for `terrain_meshes`. The caches **must** be
/// the same persistent-resource handles the room compiler reads from
/// — sharing keeps a humanoid avatar with an LSystem cape from
/// double-baking textures already cached for an identical room asset.
#[allow(clippy::too_many_arguments)]
pub fn spawn_avatar_visuals_subtree(
    commands: &mut Commands,
    chassis: Entity,
    visuals: &Generator,
    meshes: &mut Assets<Mesh>,
    std_materials: &mut Assets<StandardMaterial>,
    water_materials: &mut Assets<WaterMaterial>,
    palette: Option<&MaterialPalette>,
    heightmap: Option<&FinishedHeightMap>,
    terrain_meshes: &Query<Entity, (With<TerrainMesh>, Without<OutgoingTerrain>)>,
    prop_assets: Option<&PropMeshAssets>,
    foliage_tasks: &mut OverlandsFoliageTasks,
    caches: &mut GeneratorCaches,
    blob_image_cache: &mut BlobImageCache,
    water_surfaces: &mut WaterSurfaces,
    record: &RoomRecord,
    current_room: Option<&CurrentRoomDid>,
    is_local: bool,
) {
    // Touch-sets are scratch state for the room compiler's GC pass at
    // the end of compile_room_record. Avatar spawning doesn't run
    // through that GC, but `spawn_lsystem_entity` and
    // `spawn_shape_entity` write to them unconditionally, so we
    // provide local sinks. The cache entries the avatar fills in stay
    // pinned across frames — their next room rebuild will sweep them
    // through the regular GC because the keys carry the avatar's
    // generator-ref string. Pinning is fine: a cached LSystem mesh is
    // cheap to keep until the avatar editor drops the kind.
    let mut lsystem_cache_touched: HashSet<(String, u8)> = HashSet::new();
    let mut lsystem_mesh_touched: HashSet<String> = HashSet::new();
    let mut shape_material_touched: HashSet<(String, String)> = HashSet::new();
    let mut shape_mesh_touched: HashSet<String> = HashSet::new();

    let mut entities_spawned: u32 = 0;
    let mut budget_warned = false;

    // Synthetic generator-ref namespace so the persistent caches don't
    // collide between simultaneous avatar spawns and room generators.
    // Every avatar shares the prefix; per-instance distinction comes
    // from chassis-entity ID embedded in the cache key.
    let cache_key = format!("avatar/{}", chassis.index());

    let mut ctx = SpawnCtx {
        commands,
        record,
        meshes,
        std_materials,
        water_materials,
        palette,
        heightmap,
        terrain_meshes,
        prop_assets,
        foliage_tasks,
        lsystem_material_cache: &mut caches.lsystem_material,
        lsystem_cache_touched: &mut lsystem_cache_touched,
        lsystem_mesh_cache: &mut caches.lsystem_mesh,
        lsystem_mesh_touched: &mut lsystem_mesh_touched,
        shape_material_cache: &mut caches.shape_material,
        shape_material_touched: &mut shape_material_touched,
        shape_mesh_cache: &mut caches.shape_mesh,
        shape_mesh_touched: &mut shape_mesh_touched,
        current_room,
        entities_spawned: &mut entities_spawned,
        budget_warned: &mut budget_warned,
        blob_image_cache,
        water_surfaces,
        avatar_mode: true,
        local_avatar_mode: is_local,
    };

    // The visuals root carries its own transform — which the spawner
    // applies to the entity it creates. Parent that entity to the
    // chassis so the chassis's world transform anchors the whole tree.
    let local_tf = transform_from_data(&visuals.transform);
    if let Some(root) = spawn_generator(&mut ctx, visuals, &cache_key, &[], local_tf) {
        ctx.commands.entity(chassis).add_child(root);
    }
}

/// PDS `TransformData` → Bevy `Transform`. Local copy so this
/// submodule doesn't reach into `compile`'s `pub(super)` helper.
fn transform_from_data(t: &crate::pds::TransformData) -> Transform {
    Transform {
        translation: Vec3::from_array(t.translation.0),
        rotation: Quat::from_array(t.rotation.0),
        scale: Vec3::from_array(t.scale.0),
    }
}
