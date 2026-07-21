//! Per-pass [`SpawnCtx`], the bundled [`GeneratorCaches`] system param,
//! the spawn-budget cap, and the shared `Transform` builder.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_symbios::materials::MaterialPalette;
use bevy_symbios_shape::cache::ShapeMeshCache as UpstreamShapeMeshCache;
use bevy_symbios_texture::TextureCache;
use std::collections::HashSet;

use crate::pds::{RoomRecord, TransformData};
use crate::state::CurrentRoomDid;
use crate::terrain::{FinishedHeightMap, OutgoingTerrain, TerrainMesh};
use crate::water::{WaterMaterial, WaterSurfaces};

use super::super::audio_resolver::BlobAudioCache;
use super::super::image_cache::BlobImageCache;
use super::super::lsystem::{LSystemMaterialCache, LSystemMeshCache};
use super::super::shape::{ShapeMaterialCache, ShapeMeshCache};

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
    /// Content-addressed primitive dedup (#918). Primitives have no
    /// per-generator cache, so without these a scatter allocates one
    /// `Mesh` and one `StandardMaterial` per instance.
    pub(crate) prim_mesh: ResMut<'w, super::super::prim_cache::PrimMeshCache>,
    pub(crate) prim_material: ResMut<'w, super::super::prim_cache::PrimMaterialCache>,
    /// Mesh-asset dedup shared with every other consumer of
    /// `bevy_symbios_shape` (e.g. avatar visualisers): a generator's
    /// `(profile, size)` mesh handle is reused across compile passes and
    /// across generators with the same terminal geometry.
    pub(crate) upstream_shape_mesh: ResMut<'w, UpstreamShapeMeshCache>,
    /// Content-keyed baked-audio buffers shared across constructs and
    /// compile passes — see
    /// [`BakedAudioCache`](super::super::spatial_audio::BakedAudioCache).
    /// Bundled here (rather than as its own system param) to stay under
    /// Bevy's 16-parameter `IntoSystem` ceiling on
    /// `compile_room_record`; same for the fields below.
    pub(crate) baked_audio: ResMut<'w, super::super::spatial_audio::BakedAudioCache>,
    /// Content-fingerprinted procedural-texture dedup shared with the
    /// upstream `bevy_symbios_texture` patch system. Consulted by
    /// `build_procedural_material_async` *before* it dispatches a bake,
    /// so identical configs (every boulder of a rock scatter, a rebuilt
    /// unchanged prim) clone three `Handle<Image>`s instead of queueing
    /// a fresh 512² generation task — which on wasm would run
    /// monolithically on the main thread.
    pub(crate) texture: ResMut<'w, TextureCache>,
    /// Per-placement compiled state (fingerprints + anchors) the diff
    /// planner reads and the executor commits into — see
    /// [`CompiledWorld`](super::job::CompiledWorld).
    pub(crate) world: ResMut<'w, super::job::CompiledWorld>,
    /// The in-flight sliced compile job, if any — see
    /// [`CompileJob`](super::job::CompileJob).
    pub(crate) job: ResMut<'w, super::job::CompileJob>,
    /// Clock for the executor's per-slice frame budget and the
    /// telemetry timestamp.
    pub(crate) time: Res<'w, Time>,
    /// Session log for the per-job compile telemetry event.
    pub(crate) session_log: ResMut<'w, crate::diagnostics::SessionLog>,
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
    /// Image asset store. Threaded through so every procedural material
    /// helper can hand it to
    /// [`bevy_symbios_texture::build_procedural_material_async`] for the
    /// fast cache-hit path (the helper writes generated `Handle<Image>`s
    /// directly into the just-built material when the cache fires).
    pub(crate) images: &'a mut Assets<Image>,
    pub(crate) palette: Option<&'a MaterialPalette>,
    pub(crate) heightmap: Option<&'a FinishedHeightMap>,
    pub(crate) terrain_meshes:
        &'a Query<'wq, 'sq, Entity, (With<TerrainMesh>, Without<OutgoingTerrain>)>,
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
    pub(crate) lsystem_cache_touched: &'a mut HashSet<(String, u16)>,
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
    /// Content-addressed primitive mesh / material dedup — see
    /// [`prim_cache`](super::super::prim_cache).
    pub(crate) prim_mesh_cache: &'a mut super::super::prim_cache::PrimMeshCache,
    pub(crate) prim_material_cache: &'a mut super::super::prim_cache::PrimMaterialCache,
    /// Upstream mesh-asset dedup. Whereas `shape_mesh_cache` caches the full
    /// post-derivation instance list per generator, this cache dedupes
    /// individual `Handle<Mesh>` by `(profile, size)` across every generator
    /// in the world.
    pub(crate) upstream_shape_mesh_cache: &'a mut UpstreamShapeMeshCache,
    /// `generator_ref` keys touched this compile pass so the caller can GC
    /// shape meshes belonging to generators removed from the record.
    pub(crate) shape_mesh_touched: &'a mut HashSet<String>,
    /// Cross-generator procedural-texture dedup — see
    /// [`GeneratorCaches::texture`]. Unlike the per-generator material
    /// caches above it also covers primitives, which have no
    /// generator-level cache of their own: without it every boulder of
    /// a rebuilt rock scatter re-bakes an identical texture set.
    pub(crate) texture_cache: &'a mut TextureCache,
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
    /// Source-keyed coalescing cache for Referenced-audio fetches —
    /// the sister of [`blob_image_cache`](Self::blob_image_cache) for ambient + per-construct
    /// audio that comes from an URL or ATProto blob. Constructs with
    /// a `SovereignAudioConfig::Referenced` audio field flow through
    /// this cache on every spawn so a room scattering many constructs
    /// pointing at the same source pays one round-trip.
    pub(crate) blob_audio_cache: &'a mut BlobAudioCache,
    /// Content-keyed baked-audio buffers (see
    /// [`BakedAudioCache`](super::super::spatial_audio::BakedAudioCache)):
    /// procedural construct audio resolves through this so identical
    /// configs — within a pass and across recompiles — share one bake.
    pub(crate) baked_audio_cache: &'a mut super::super::spatial_audio::BakedAudioCache,
    /// Runtime water-surface registry. Cleared at the top of each compile
    /// pass and pushed to from `spawn_water_volume`. Read by the scatter
    /// biome filter (this pass) and rover buoyancy (every fixed step).
    pub(crate) water_surfaces: &'a mut WaterSurfaces,
    /// Index of the `RoomRecord` placement currently being compiled —
    /// stamped onto the water planes this unit spawns so the
    /// incremental compiler can retire exactly them on a rebuild.
    /// [`WaterPlane::NO_OWNER`](crate::water::WaterPlane::NO_OWNER) in
    /// avatar mode (no placement owns avatar visuals).
    pub(crate) placement_index: usize,
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

impl SpawnCtx<'_, '_, '_, '_, '_> {
    /// Record a grammar compile outcome into
    /// [`crate::world_builder::grammar_diag::GrammarDiagnostics`] (#829),
    /// keyed so the editors can look it up: room compiles use the
    /// generator's record key; the LOCAL avatar uses the avatar editor's
    /// fixed root key (the synthetic `avatar/<id>` cache namespace would
    /// never match a UI selection); REMOTE peers' grammars are not
    /// recorded — a neighbour's broken tree is not the local editor's
    /// business. Queued as a command (the spawn paths have no resource
    /// access — same zero-signature-ripple idiom as the texture-cache
    /// counters), applied when this compile's command buffer drains.
    pub(crate) fn record_grammar_status(&mut self, generator_ref: &str, error: Option<String>) {
        let key = if !self.avatar_mode {
            generator_ref.to_string()
        } else if self.local_avatar_mode {
            crate::ui::room::generators::AvatarVisualsTreeSource::ROOT_NAME.to_string()
        } else {
            return;
        };
        let status = match error {
            None => crate::world_builder::grammar_diag::GrammarStatus::Ok,
            Some(message) => crate::world_builder::grammar_diag::GrammarStatus::Error { message },
        };
        self.commands.queue(move |world: &mut World| {
            if let Some(mut diag) =
                world.get_resource_mut::<crate::world_builder::grammar_diag::GrammarDiagnostics>()
            {
                diag.by_generator.insert(key, status);
            }
        });
    }
}
