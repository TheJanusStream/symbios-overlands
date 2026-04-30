//! Per-pass [`SpawnCtx`], the bundled [`GeneratorCaches`] system param,
//! the spawn-budget cap, and the shared `Transform` builder.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_symbios::materials::MaterialPalette;
use std::collections::HashSet;

use crate::pds::{RoomRecord, TransformData};
use crate::state::CurrentRoomDid;
use crate::terrain::{FinishedHeightMap, OutgoingTerrain, TerrainMesh};
use crate::water::{WaterMaterial, WaterSurfaces};

use super::super::image_cache::BlobImageCache;
use super::super::lsystem::{LSystemMaterialCache, LSystemMeshCache};
use super::super::shape::{ShapeMaterialCache, ShapeMeshCache};
use super::super::{OverlandsFoliageTasks, PropMeshAssets};

/// Bundled per-generator caches for the compile pass. Bevy 0.18 imposes a
/// 16-parameter ceiling on `IntoSystem`, and `compile_room_record` already
/// hugged that bound; collapsing the four geometry / material caches into
/// one `SystemParam` struct keeps the signature inside the budget when
/// future generators need their own caches alongside L-system and Shape.
#[derive(SystemParam)]
pub struct GeneratorCaches<'w> {
    pub(crate) lsystem_material: ResMut<'w, LSystemMaterialCache>,
    pub(crate) lsystem_mesh: ResMut<'w, LSystemMeshCache>,
    pub(crate) shape_material: ResMut<'w, ShapeMaterialCache>,
    pub(crate) shape_mesh: ResMut<'w, ShapeMeshCache>,
}

/// Hard ceiling on the number of `spawn_generator` calls a single
/// `compile_room_record` pass is allowed to make. The per-axis sanitiser
/// caps are *additive* (1024 placements × 100k scatter × 1024 nodes/tree)
/// and their product is many orders of magnitude past anything a real room
/// produces — this is the multiplicative bound.
///
/// 500_000 was chosen so a single legitimate scatter at
/// `MAX_SCATTER_COUNT = 100_000` over a 1–5-node generator tree fits with
/// headroom for a handful of additional placements in the same room. A
/// scatter dense enough to exhaust this on its own is already past the
/// authoring envelope and the cap fail-stops the compile so the rest of
/// the frame stays interactive.
pub(crate) const MAX_ROOM_ENTITIES: u32 = 500_000;

/// Returns `true` once the running spawn count has reached the budget.
/// The first overshoot logs a warning; subsequent calls in the same pass
/// stay quiet so a runaway record doesn't flood the log.
pub fn budget_exceeded(spawned: u32, warned: &mut bool) -> bool {
    if spawned >= MAX_ROOM_ENTITIES {
        if !*warned {
            warn!(
                "Room entity budget {} exceeded; remaining placements skipped",
                MAX_ROOM_ENTITIES
            );
            *warned = true;
        }
        true
    } else {
        false
    }
}

pub(crate) fn transform_from_data(t: &TransformData) -> Transform {
    Transform {
        translation: Vec3::from_array(t.translation.0),
        rotation: Quat::from_array(t.rotation.0),
        scale: Vec3::from_array(t.scale.0),
    }
}

/// Parameter bundle for recursive generator spawning — a plain struct
/// keeps the call sites readable while avoiding a 12-argument signature.
/// Commands and Query carry separate `('w, 's)` lifetimes from the
/// SystemParam pair; we can't unify them here without making the borrow
/// checker invariance rules break at the call site, so they get independent
/// parameters.
pub struct SpawnCtx<'a, 'wc, 'sc, 'wq, 'sq> {
    pub(crate) commands: &'a mut Commands<'wc, 'sc>,
    pub(crate) record: &'a RoomRecord,
    pub(crate) meshes: &'a mut Assets<Mesh>,
    pub(crate) std_materials: &'a mut Assets<StandardMaterial>,
    pub(crate) water_materials: &'a mut Assets<WaterMaterial>,
    pub(crate) palette: Option<&'a MaterialPalette>,
    pub(crate) heightmap: Option<&'a FinishedHeightMap>,
    pub(crate) terrain_meshes:
        &'a Query<'wq, 'sq, Entity, (With<TerrainMesh>, Without<OutgoingTerrain>)>,
    pub(crate) prop_assets: Option<&'a PropMeshAssets>,
    pub(crate) foliage_tasks: &'a mut OverlandsFoliageTasks,
    /// Persistent, hash-invalidated material cache. A single scatter
    /// placement with count=100 would otherwise allocate 100 fresh
    /// `StandardMaterial`s *and* enqueue 100 identical foliage texture
    /// tasks for the same slot — and across compile passes an unchanged
    /// slot would re-bake every time the record is patched. The cache
    /// keys on `(generator_ref, slot)` and reuses the handle whenever the
    /// content hash of `SovereignMaterialSettings` is identical.
    pub(crate) lsystem_material_cache: &'a mut LSystemMaterialCache,
    /// `(generator_ref, slot)` keys touched this compile pass. Populated
    /// as we resolve material handles so the caller can GC stale entries.
    pub(crate) lsystem_cache_touched: &'a mut HashSet<(String, u8)>,
    /// Persistent mesh cache. A single scatter placement with `count=100_000`
    /// would otherwise re-derive / re-interpret / re-mesh the L-system on
    /// every spawn, pegging the main thread for minutes and allocating
    /// 100_000 unique `Handle<Mesh>` entries. The cache keys on
    /// `generator_ref` and reuses the baked `Handle<Mesh>` bucket across
    /// every scatter point whenever the geometry fingerprint matches.
    pub(crate) lsystem_mesh_cache: &'a mut LSystemMeshCache,
    /// `generator_ref` keys touched this compile pass so the caller can GC
    /// meshes belonging to generators removed from the record.
    pub(crate) lsystem_mesh_touched: &'a mut HashSet<String>,
    /// Shape grammar material cache — sister of `lsystem_material_cache`,
    /// keyed by `(generator_ref, slot_name)` because the upstream
    /// interpreter emits string slot names from `Mat("...")` rather than
    /// the L-system's u8 slot ids.
    pub(crate) shape_material_cache: &'a mut ShapeMaterialCache,
    /// `(generator_ref, slot_name)` keys touched this compile pass so the
    /// caller can GC stale shape material handles.
    pub(crate) shape_material_touched: &'a mut HashSet<(String, String)>,
    /// Shape grammar geometry cache — derives once per
    /// `(generator_ref, geometry_hash)` pair and shares the per-terminal
    /// `Handle<Mesh>` list across every scatter/grid spawn.
    pub(crate) shape_mesh_cache: &'a mut ShapeMeshCache,
    /// `generator_ref` keys touched this compile pass so the caller can GC
    /// shape meshes belonging to generators removed from the record.
    pub(crate) shape_mesh_touched: &'a mut HashSet<String>,
    /// DID of the room we're currently compiling. Portal generators skip the
    /// ATProto profile-picture fetch when `target_did` equals this (an
    /// intra-room portal has no remote identity to paint onto its top face).
    pub(crate) current_room: Option<&'a CurrentRoomDid>,
    /// Running count of entities spawned this compile pass. Compared
    /// against [`MAX_ROOM_ENTITIES`] to fail-stop pathological records
    /// whose per-axis sanitiser caps still multiply into a frame-killing
    /// total.
    pub(crate) entities_spawned: &'a mut u32,
    /// Latch that flips on the first budget overshoot so the warning
    /// fires once per pass instead of once per skipped spawn.
    pub(crate) budget_warned: &'a mut bool,
    /// Source-keyed coalescing cache for image fetches used by both
    /// [`Sign`](crate::pds::GeneratorKind::Sign) generators and Portal
    /// top-face profile pictures. The first requester for a given source
    /// (URL / atproto blob / DID-pfp) registers a pending task here;
    /// every subsequent requester sharing that source enqueues its
    /// material handle on the existing pending list instead of issuing a
    /// redundant HTTPS round trip.
    pub(crate) blob_image_cache: &'a mut BlobImageCache,
    /// Runtime water-surface registry. Cleared at the top of each compile
    /// pass and pushed to from `spawn_water_volume`. Read by the scatter
    /// biome filter (this pass) and rover buoyancy (every fixed step).
    pub(crate) water_surfaces: &'a mut WaterSurfaces,
    /// `true` when the spawner is producing avatar visuals rather than
    /// room geometry. Avatar mode skips three room-specific behaviours
    /// in every spawn arm: (1) `RoomEntity` insertion (avatars manage
    /// their own cleanup via the chassis's child despawn), (2)
    /// `PrimMarker` insertion (room-only gizmo addressing — but see
    /// `local_avatar_mode` for the avatar's own gizmo marker), and (3)
    /// collider attachment in `spawn_primitive_entity` (the locomotion
    /// preset's chassis collider is the only physics body on an
    /// avatar).
    pub(crate) avatar_mode: bool,
    /// `true` only when the avatar being spawned is the **local** player's
    /// own avatar — implies `avatar_mode` is also `true`. Drives
    /// `AvatarVisualPrim` insertion so the editor gizmo can target the
    /// local player's visuals tree without also picking up remote peers'
    /// avatars (whose visuals are not locally editable).
    pub(crate) local_avatar_mode: bool,
}
