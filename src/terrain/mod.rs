//! Deterministic procedural terrain plugin.
//!
//! A room's seed is the FNV-1a 64-bit hash of its owner's DID, so every
//! client visiting the same overland derives the identical landscape locally
//! — there is no authoritative server to replicate from.  Heightmap
//! generation (Voronoi terracing → hydraulic erosion → thermal erosion) runs
//! on `AsyncComputeTaskPool` while the four splat layer textures
//! (grass / dirt / rock / snow) are baked in parallel by
//! `bevy_symbios_texture`.  Once every task has finished, the layers are
//! concatenated into a 2D texture array and the `SplatExtension` material is
//! flipped from placeholder flat-colour mode to triplanar PBR splat blending.
//!
//! Water is spawned by the [`crate::world_builder`] module from the `Water`
//! generator in the active `RoomRecord` — this plugin only produces the
//! terrain mesh and heightfield collider.
//!
//! ## Sub-module map
//!
//! * [`heightmap`] — the async heightmap generator (FBM / DiamondSquare /
//!   Voronoi + erosion passes), its start/poll systems, and the
//!   mesh + heightfield-collider spawner.
//! * [`splat`] — the four-layer procedural texture tasks, the
//!   texture-array atlas build, the splat weight map, and the material
//!   flip from placeholder to triplanar splat blending (also publishes
//!   the CPU [`TerrainSurfaceQuery`](crate::interaction::TerrainSurfaceQuery)
//!   mirror for the contact classifier).
//! * [`referenced`] — the `SovereignTextureConfig::Referenced` splat-layer
//!   path: URL / ATProto-blob fetch, decode + resize, and the per-layer
//!   override of the procedural placeholder.
//! * [`lifecycle`] — logout cleanup and the in-place regenerate trigger
//!   that watches the live record's terrain config fingerprint.
//!
//! The shared task / state resources live here in `mod.rs` (private to
//! the terrain tree; child modules reach them via `super::`) because
//! every sub-module touches some subset of them.

mod heightmap;
mod lifecycle;
mod lots;
mod referenced;
mod roads;
mod splat;

use bevy::prelude::*;
use bevy_symbios_ground::HeightMap;
use bevy_symbios_texture::SymbiosTexturePlugin;

use crate::splat::SplatTerrainMaterial;
use crate::state::{AppState, LiveRoomRecord};

/// Marker inserted once the texture-layer spawn step has run, so the
/// Loading-phase scheduler doesn't kick the same four tasks twice while
/// waiting for the texture polls to drain.
#[derive(Resource)]
struct TextureTasksStarted;

#[derive(Component)]
pub struct TerrainMesh;

/// Marker inserted on the previous terrain entity during an in-place
/// regenerate. Kept alive (with its collider + textured mesh) until the new
/// heightmap task completes and `spawn_terrain_mesh` swaps in the fresh one —
/// otherwise the player would fall through the world for the ~frame(s) the
/// generator takes, and every peer would see a jarring flash to empty sky.
#[derive(Component)]
pub struct OutgoingTerrain;

/// Marker component for the water-level volume entity (translucent cuboid).
#[derive(Component)]
pub struct WaterVolume;

#[derive(Resource)]
pub struct FinishedHeightMap(pub HeightMap);

#[derive(Resource)]
pub struct TerrainTask(pub bevy::tasks::Task<HeightMap>);

/// Marker on the PendingTexture entity; value = layer index (0–3).
#[derive(Component)]
struct TextureLayerIndex(usize);

/// Accumulated handles from completed PendingTexture tasks.
#[derive(Resource, Default)]
struct TerrainSplatState {
    layer_albedo: [Option<Handle<Image>>; 4],
    layer_normal: [Option<Handle<Image>>; 4],
    applied: bool,
}

impl TerrainSplatState {
    fn all_ready(&self) -> bool {
        self.layer_albedo.iter().all(|h| h.is_some())
            && self.layer_normal.iter().all(|h| h.is_some())
    }
}

/// Handle to the terrain's `SplatTerrainMaterial`; stored so `apply_splat_textures`
/// can update it once all texture tasks finish.
#[derive(Resource)]
struct SplatMaterialHandle(Handle<SplatTerrainMaterial>);

/// Serialised fingerprint of the terrain config currently compiled into the
/// live heightmap. `maybe_regenerate_terrain` compares the active
/// `RoomRecord`'s terrain config against this and, on mismatch, triggers a
/// full heightmap/texture/mesh rebuild in place. Without this, a room owner
/// editing noise parameters would desync every guest: the live terrain would
/// stay frozen for everyone already in the room, while a new guest joining
/// afterwards would enter `Loading` and generate the *new* terrain — so
/// older peers and newcomers end up driving on fundamentally different
/// ground.
#[derive(Resource, Default)]
struct LastTerrainConfigJson(Option<String>);

/// Latest observed terrain target that has not yet been acted on.
///
/// `Res::is_changed()` ticks are per-system and consumed the moment this
/// system runs — so a change observed while a previous terrain task was
/// still in flight used to vanish: we'd return early, the tick would
/// clear, and subsequent frames would never re-fire because the record
/// didn't mutate again. Stashing the target here lets us survive any
/// number of in-flight frames and apply the edit as soon as the async
/// generator finishes.
///
/// The nesting is load-bearing: the outer `Option` is "is there a pending
/// change to apply", while the inner `Option<String>` is the *target* —
/// `Some(fingerprint)` for a terrain config, or `None` when the owner has
/// deleted the terrain generator and the live heightfield must be torn
/// down. Collapsing these would lose the deletion signal and leave the
/// old mesh as orphaned geometry.
#[derive(Resource, Default)]
struct PendingTerrainConfigJson(Option<Option<String>>);

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(SymbiosTexturePlugin::default())
            .add_plugins(MaterialPlugin::<SplatTerrainMaterial>::default())
            .init_resource::<TerrainSplatState>()
            .init_resource::<LastTerrainConfigJson>()
            .init_resource::<PendingTerrainConfigJson>()
            .init_resource::<roads::RoadFingerprint>()
            // Terrain + texture + mesh spawning runs as conditional Update
            // systems in both Loading and InGame so the same plumbing handles
            // the initial world build *and* in-place regeneration when the
            // room owner edits terrain parameters mid-session. Each step
            // guards itself against double-kicking with a resource/marker
            // check.
            .add_systems(
                Update,
                (
                    heightmap::start_terrain_generation.run_if(
                        resource_exists::<LiveRoomRecord>
                            .and(not(resource_exists::<TerrainTask>))
                            .and(not(resource_exists::<FinishedHeightMap>)),
                    ),
                    splat::start_texture_tasks.run_if(
                        resource_exists::<LiveRoomRecord>
                            .and(not(resource_exists::<TextureTasksStarted>)),
                    ),
                    heightmap::poll_terrain_task.run_if(resource_exists::<TerrainTask>),
                    heightmap::spawn_terrain_mesh.run_if(
                        resource_exists::<FinishedHeightMap>
                            .and(not(resource_exists::<SplatMaterialHandle>)),
                    ),
                    // Re-mesh roads from the existing heightmap whenever the
                    // road config or the heightmap changes — no terrain regen.
                    // Runs after the terrain task so a fresh heightmap is
                    // visible the same frame it lands.
                    roads::maybe_rebuild_roads
                        .run_if(resource_exists::<LiveRoomRecord>)
                        .after(heightmap::poll_terrain_task)
                        .after(heightmap::spawn_terrain_mesh),
                    // Once the heightmap exists, fill a road-growing room's
                    // lots with buildings (injected into the live record). Runs
                    // after the terrain task so it sees the finished surface.
                    lots::maybe_populate_lots
                        .run_if(resource_exists::<LiveRoomRecord>)
                        .after(heightmap::poll_terrain_task)
                        .after(heightmap::spawn_terrain_mesh),
                )
                    .run_if(not(in_state(AppState::Login))),
            )
            // `not(Login)` rather than `in_state(InGame)`: the splat
            // array assembly (~4 × full-mipchain layers concatenated
            // into one texture array) is a noticeable main-thread cost
            // on wasm, so it runs during `Loading` — behind the loading
            // screen — as soon as the four texture bakes land, instead
            // of in the first visible `InGame` frames.
            // `maybe_regenerate_terrain`'s first observation merely
            // records the config fingerprint, so running it earlier
            // does not change the regen semantics.
            .add_systems(
                Update,
                (
                    lifecycle::maybe_regenerate_terrain.run_if(resource_exists::<LiveRoomRecord>),
                    splat::collect_texture_results,
                    referenced::poll_splat_layer_fetches,
                    splat::apply_splat_textures,
                )
                    .chain()
                    .run_if(not(in_state(AppState::Login))),
            )
            .add_systems(OnExit(AppState::InGame), lifecycle::cleanup_terrain);
    }
}
