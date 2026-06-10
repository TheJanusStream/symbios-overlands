//! Terrain lifecycle: logout cleanup and the in-place regenerate
//! trigger driven by terrain-config edits in the live room record.

use bevy::prelude::*;
use bevy_symbios_texture::async_gen::PendingTexture;

use crate::interaction::TerrainSurfaceQuery;
use crate::state::LiveRoomRecord;

use super::referenced::PendingSplatLayerFetch;
use super::{
    FinishedHeightMap, LastTerrainConfigJson, OutgoingTerrain, PendingTerrainConfigJson,
    SplatMaterialHandle, TerrainMesh, TerrainSplatState, TerrainTask, TextureLayerIndex,
    TextureTasksStarted, WaterVolume,
};

/// Despawn terrain + water entities and reset terrain-specific resources so
/// the next login cycle restarts heightmap generation and splat texture
/// uploads from scratch.
#[allow(clippy::too_many_arguments)]
pub(super) fn cleanup_terrain(
    mut commands: Commands,
    terrain: Query<Entity, With<TerrainMesh>>,
    outgoing: Query<Entity, With<OutgoingTerrain>>,
    water: Query<Entity, With<WaterVolume>>,
    pending_textures: Query<Entity, With<TextureLayerIndex>>,
    pending_raw: Query<Entity, With<PendingTexture>>,
    pending_splat_refs: Query<Entity, With<PendingSplatLayerFetch>>,
    mut splat_state: ResMut<TerrainSplatState>,
    mut last_cfg: ResMut<LastTerrainConfigJson>,
    mut pending_cfg: ResMut<PendingTerrainConfigJson>,
) {
    for e in &terrain {
        commands.entity(e).despawn();
    }
    for e in &outgoing {
        commands.entity(e).try_despawn();
    }
    for e in &water {
        commands.entity(e).despawn();
    }
    // In-flight splat texture tasks would otherwise survive into the next
    // login cycle, land their `TextureReady` components on orphaned entities,
    // and leak until process exit. Drain both the marker-tagged and any
    // `PendingTexture` entities missing the marker (recovery path) here.
    for e in &pending_textures {
        commands.entity(e).despawn();
    }
    for e in &pending_raw {
        commands.entity(e).despawn();
    }
    // In-flight Referenced-texture fetches likewise need to be drained
    // so a fast logout/login cycle doesn't leak network tasks past the
    // room they were dispatched for.
    for e in &pending_splat_refs {
        commands.entity(e).despawn();
    }
    *splat_state = TerrainSplatState::default();
    last_cfg.0 = None;
    pending_cfg.0 = None;
    commands.remove_resource::<FinishedHeightMap>();
    commands.remove_resource::<SplatMaterialHandle>();
    commands.remove_resource::<TextureTasksStarted>();
    commands.remove_resource::<TerrainTask>();
    // Drop the interaction CPU terrain mirror with its terrain so the
    // classifier doesn't probe a stale heightmap after logout.
    commands.remove_resource::<TerrainSurfaceQuery>();
}

/// Watch the active room record for terrain-config changes. When the owner
/// edits grid size, noise params, erosion, splat rules, or any other
/// terrain-affecting field, despawn the existing heightfield, drop the
/// cached heightmap / splat resources, and let the generic `Update`
/// pipeline re-kick terrain + texture tasks from scratch. The first
/// observation of a config simply records the fingerprint — Loading handled
/// the initial build — so this only fires on *changes* after the player is
/// already InGame.
#[allow(clippy::too_many_arguments)]
pub(super) fn maybe_regenerate_terrain(
    mut commands: Commands,
    record: Res<LiveRoomRecord>,
    mut last_cfg: ResMut<LastTerrainConfigJson>,
    mut pending_cfg: ResMut<PendingTerrainConfigJson>,
    terrain_q: Query<Entity, (With<TerrainMesh>, Without<OutgoingTerrain>)>,
    pending_textures: Query<Entity, With<TextureLayerIndex>>,
    pending_splat_refs: Query<Entity, With<PendingSplatLayerFetch>>,
    mut splat_state: ResMut<TerrainSplatState>,
    terrain_task: Option<Res<TerrainTask>>,
) {
    // Capture the fingerprint of any observed change *before* we decide
    // whether to act on it. `Res::is_changed` is a per-system tick that
    // fires exactly once; if we let a frame with an in-flight terrain
    // task consume the tick via an early return, the edit is silently
    // lost — `record.is_changed()` won't re-fire unless the owner edits
    // again. Serde-serialising the full `SovereignTerrainConfig` is
    // non-trivial (deeply nested record), so we still gate it on
    // `is_changed` rather than rebuilding every frame.
    if record.is_changed()
        && let Some(cfg) = crate::pds::find_terrain_config(&record.0)
        && let Ok(fp) = serde_json::to_string(cfg)
    {
        pending_cfg.0 = Some(fp);
    }

    // Refuse to tear down in-flight generation — the previous async
    // task's output would still land in `FinishedHeightMap` and the new
    // pipeline couldn't start. The pending fingerprint stays queued and
    // will be applied on a later frame once the task completes.
    if terrain_task.is_some() {
        return;
    }

    let Some(fp) = pending_cfg.0.take() else {
        return;
    };
    let should_regen = match &last_cfg.0 {
        Some(prev) if prev == &fp => false,
        Some(_) => true,
        None => false, // first observation — initial Loading pipeline built it
    };
    last_cfg.0 = Some(fp);
    if !should_regen {
        return;
    }

    // Mark the current terrain as outgoing instead of despawning it
    // immediately: the player sits on its heightfield collider, so removing
    // it before the new heightmap task completes would drop them through the
    // world for the ~frame(s) generation takes, and every peer would see an
    // abrupt flash to empty sky. `spawn_terrain_mesh` despawns outgoing
    // entries atomically when the fresh mesh spawns. Water is a
    // `RoomEntity`, so `compile_room_record` despawns and rebuilds it in
    // response to the same record change — touching it here would race and
    // double-despawn.
    for e in &terrain_q {
        commands.entity(e).insert(OutgoingTerrain);
    }
    for e in &pending_textures {
        commands.entity(e).despawn();
    }
    // Drop in-flight Referenced-layer fetches from the previous config
    // too — they'd otherwise land on the new splat state with stale
    // textures from the old config's layers.
    for e in &pending_splat_refs {
        commands.entity(e).despawn();
    }
    commands.remove_resource::<FinishedHeightMap>();
    commands.remove_resource::<SplatMaterialHandle>();
    commands.remove_resource::<TextureTasksStarted>();
    commands.remove_resource::<TerrainTask>();
    *splat_state = TerrainSplatState::default();
    info!("Terrain config changed — regenerating heightmap + splat textures");
}
