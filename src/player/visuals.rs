//! Avatar-side generator-tree spawner. Walks an
//! [`crate::pds::AvatarRecord`]'s `visuals: Generator` tree and spawns
//! one Bevy entity per node,
//! parented into the chassis hierarchy. Routes through
//! [`crate::world_builder::avatar_spawn::spawn_avatar_visuals_subtree`]
//! so the avatar dispatch arms reuse the same primitive / LSystem /
//! Shape machinery as room generators, with the room-only behaviours
//! (RoomEntity tag, PrimMarker, per-prim colliders) suppressed by
//! `SpawnCtx::avatar_mode = true`.
//!
//! Hot-swap discipline: callers despawn the previous chassis children
//! before re-running this spawner. The sub-tree spawner does not tag
//! children with `RoomEntity`, so the room compiler's cleanup query
//! cannot reach them — only the chassis's `with_children` despawn
//! does.

use bevy::prelude::*;
use bevy_symbios::materials::MaterialPalette;

use crate::pds::Generator;
use crate::state::CurrentRoomDid;
use crate::terrain::{FinishedHeightMap, OutgoingTerrain, TerrainMesh};
use crate::water::{WaterMaterial, WaterSurfaces};
use crate::world_builder::PropMeshAssets;
use crate::world_builder::avatar_spawn::spawn_avatar_visuals_subtree;
use crate::world_builder::compile::GeneratorCaches;
use crate::world_builder::image_cache::BlobImageCache;

/// Bundle of every shared `world_builder` resource the avatar spawn
/// path needs to reach. Bundled because Bevy 0.18 caps `IntoSystem`
/// at 16 parameters and the host systems already carry a handful of
/// `ResMut`s of their own — folding the world-builder fan-out into one
/// `SystemParam` keeps callers under the budget.
#[derive(bevy::ecs::system::SystemParam)]
pub struct AvatarSpawnDeps<'w, 's> {
    pub water_materials: ResMut<'w, Assets<WaterMaterial>>,
    pub palette: Option<Res<'w, MaterialPalette>>,
    pub heightmap: Option<Res<'w, FinishedHeightMap>>,
    pub terrain_meshes: Query<'w, 's, Entity, (With<TerrainMesh>, Without<OutgoingTerrain>)>,
    pub prop_assets: Option<Res<'w, PropMeshAssets>>,
    pub caches: GeneratorCaches<'w>,
    pub blob_image_cache: ResMut<'w, BlobImageCache>,
    pub blob_audio_cache: ResMut<'w, crate::world_builder::audio_resolver::BlobAudioCache>,
    pub water_surfaces: ResMut<'w, WaterSurfaces>,
    pub current_room: Option<Res<'w, CurrentRoomDid>>,
}

/// Walk `visuals` and spawn one entity per node, parented under
/// `chassis`. The visuals root's transform composes with each node's
/// local transform; the chassis (a parent rigid body) provides the
/// world-space anchor.
///
/// `existing_children` is the chassis's current child list, despawned
/// before the new tree spawns to avoid double-instantiation on hot-swap.
/// Pass `None` on first spawn (no prior visuals to clear).
#[allow(clippy::too_many_arguments)]
pub fn spawn_avatar_visuals(
    commands: &mut Commands,
    chassis: Entity,
    visuals: &Generator,
    existing_children: Option<&Children>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    deps: &mut AvatarSpawnDeps,
    is_local: bool,
) {
    if let Some(children) = existing_children {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }

    // The avatar spawner's `record` parameter is unused on every reachable
    // dispatch arm — the sanitiser strips Terrain / Water / Portal upstream, and
    // that Water arm is the only `ctx.record` reader — so a single shared
    // default is a safe read-only sentinel. `RoomRecord::default` runs the whole
    // seeded-defaults pipeline (terrain shape, palette, scatters, a generator
    // tree), and this path is NOT rare — `rebuild_local_visuals` fires every
    // frame while the avatar editor mutates the record, and `detect_remote_change`
    // once per remote peer per avatar update — so build it ONCE and lend the same
    // instance to every spawn (#638).
    static SENTINEL: std::sync::OnceLock<crate::pds::RoomRecord> = std::sync::OnceLock::new();
    let empty_record = SENTINEL.get_or_init(crate::pds::RoomRecord::default);

    spawn_avatar_visuals_subtree(
        commands,
        chassis,
        visuals,
        meshes,
        materials,
        deps.water_materials.as_mut(),
        images,
        deps.palette.as_deref(),
        deps.heightmap.as_deref(),
        &deps.terrain_meshes,
        deps.prop_assets.as_deref(),
        &mut deps.caches,
        deps.blob_image_cache.as_mut(),
        deps.blob_audio_cache.as_mut(),
        deps.water_surfaces.as_mut(),
        empty_record,
        deps.current_room.as_deref(),
        is_local,
    );
}
