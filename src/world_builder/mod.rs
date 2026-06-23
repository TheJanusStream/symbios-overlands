//! World compiler: turns a `RoomRecord` recipe into ECS entities.
//!
//! This plugin owns every entity spawned from the active room recipe. When
//! the owner edits the record — locally through the world editor or
//! remotely via a `RoomStateUpdate` broadcast — the whole recipe is
//! replaced, the compiler despawns every previously-spawned `RoomEntity`,
//! and re-walks the placement graph. That strict rebuild is the only way
//! to avoid double-spawning colliders (Avian crashes if two heightfields
//! coexist at the origin) whenever a patch lands.
//!
//! Terrain heightmap generation stays in `terrain.rs` because the collider
//! must be solid before `AppState::InGame` starts; the recipe's
//! `Terrain` generator is recorded here as a no-op spawn but its `traits`
//! are still applied to the already-existing terrain mesh. Water, shapes
//! and l-systems are compiled fresh on every rebuild.
//!
//! **Determinism:** scatter placements use `ChaCha8Rng` seeded by the
//! placement's `local_seed` so every peer visiting the same DID sees the
//! same objects in the same locations. `thread_rng()` is explicitly
//! forbidden here — OS entropy would desynchronise the shared reality.
//!
//! ## Sub-module map
//!
//! * [`compile`] — the main `compile_room_record` system + its
//!   per-`GeneratorKind` dispatcher, its atmospheric sibling
//!   `apply_environment_state`, the shared `SpawnCtx`, and the scatter /
//!   biome math helpers.
//! * [`lsystem`] — L-system geometry + material caches and the spawn path.
//! * [`shape`] — CGA shape-grammar geometry + material caches and the
//!   per-terminal spawn path; the architectural sibling of [`lsystem`].
//! * [`prim`] — Primitive-generator spawners (Cuboid / Sphere / Cylinder /
//!   Capsule / Cone / Torus / Plane / Tetrahedron) and the parametric
//!   mesh/collider builders shared by their spawn arm.
//! * [`portal`] — portal cube spawning. The top-face profile picture is
//!   delegated to [`image_cache::BlobImageCache`] via a `SignSource::DidPfp`
//!   request so portals coalesce with Sign generators against the same
//!   source.
//! * [`image_cache`] — source-keyed coalescing cache for image fetches,
//!   shared by Sign generators, the Portal top face, and ParticleSystem
//!   textures. Three resolver paths (URL / atproto blob / DID-pfp) feed
//!   into the same Pending/Ready state machine so a room with many panels
//!   pointing at the same source issues exactly one HTTPS round trip; the
//!   cache key includes the sampler filter so Linear panels and Nearest
//!   pixel-art particles coexist as separate GPU images.
//! * [`blob_fetch`] — shared capped HTTPS / ATProto-blob byte fetcher
//!   used by [`image_cache`] and the [`crate::interaction::audio`] cue
//!   cache, so the wasm/native split and the OOM-guarding chunk loop
//!   live in exactly one place.
//! * [`sign`] — Sign generator spawner: textured plane with the full
//!   StandardMaterial toggles, image fetched asynchronously through
//!   [`image_cache`].
//! * [`particles`] — CPU + ECS particle emitter for `ParticleSystem`:
//!   per-frame spawn / motion / age systems, optional sprite-sheet atlas
//!   animation, and avian3d collisions.
//! * [`avatar_spawn`] — re-entry point that walks an avatar's `visuals`
//!   tree through the same dispatch arms with `SpawnCtx::avatar_mode = true`
//!   so room-only behaviours (RoomEntity tag, PrimMarker, per-prim
//!   colliders) are skipped.
//! * [`material`] — water volume spawn, procedural material bridge, and
//!   foliage texture task polling.

pub mod audio_resolver;
pub mod avatar_spawn;
pub(crate) mod blob_fetch;
pub(crate) mod compile;
pub mod image_cache;
mod lsystem;
mod material;
pub mod particles;
pub mod portal;
mod prim;
mod shape;
mod sign;
pub mod spatial_audio;

use std::collections::HashMap;

use crate::pds::{Placement, PropMeshType, RoomRecord, ScatterBounds};
use crate::state::{AppState, LiveRoomRecord};
use crate::terrain::FinishedHeightMap;
use crate::water::{WaterMaterial, WaterSurfaces};
use avian3d::prelude::Sensor;
use bevy::asset::RenderAssetUsages;
use bevy::math::Isometry3d;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

pub use lsystem::{LSystemMaterialCache, LSystemMeshCache};
pub use material::build_procedural_material;
pub use prim::build_primitive_mesh;
pub use shape::{ShapeMaterialCache, ShapeMeshCache};

/// Register the resources + plugins the generator spawn path
/// (`avatar_spawn::spawn_avatar_visuals_subtree` / `compile::spawn_generator`)
/// reads, *minus* the room-compile systems and `AppState` gating that
/// [`WorldBuilderPlugin`] adds. The headless render tool
/// ([`crate::render_tool`]) calls this to drive the real spawn path outside
/// the full game app, so its renders match what the game produces. Mirrors
/// the resource set in [`WorldBuilderPlugin::build`]; keep the two in sync.
pub fn register_headless_spawn(app: &mut App) {
    app.add_plugins(MaterialPlugin::<WaterMaterial>::default())
        .add_plugins(bevy_symbios_texture::SymbiosTexturePlugin::default())
        .insert_resource(bevy_symbios_texture::TextureCache::memory(64))
        .init_resource::<LSystemMaterialCache>()
        .init_resource::<LSystemMeshCache>()
        .init_resource::<ShapeMaterialCache>()
        .init_resource::<ShapeMeshCache>()
        .init_resource::<bevy_symbios_shape::cache::ShapeMeshCache>()
        .init_resource::<compile::CompiledWorld>()
        .init_resource::<compile::CompileJob>()
        .init_resource::<WaterSurfaces>()
        .init_resource::<image_cache::BlobImageCache>()
        .init_resource::<audio_resolver::BlobAudioCache>()
        .init_resource::<spatial_audio::BakedAudioCache>()
        .init_resource::<particles::ParticleQuadMesh>()
        .init_resource::<particles::ParticleAtlasMeshes>()
        .init_resource::<crate::state::DiagnosticsLog>();

    // Insert the shared L-system prop meshes imperatively (not via the
    // `setup_prop_assets` Startup system) so the resource is present before
    // the render tool's own Startup spawn system reads `AvatarSpawnDeps`.
    // Without this the headless path leaves `ctx.prop_assets` = None and
    // silently drops every L-system foliage card (`~`), so plant renders
    // showed only the bare woody skeleton.
    let prop_assets = {
        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        build_prop_mesh_assets(&mut meshes)
    };
    app.insert_resource(prop_assets);
}

/// Marks an in-scene portal cube and carries the destination coordinates the
/// interaction system reads when the local player's sensor-collision set
/// touches it.
#[derive(Component, Clone)]
pub struct PortalMarker {
    pub target_did: String,
    pub target_pos: Vec3,
}

/// Marker attached to every entity spawned from the active `RoomRecord`.
/// Despawning all of these is how the compiler applies a record update
/// without double-spawning anything.
#[derive(Component)]
pub struct RoomEntity;

/// Index of the `RoomRecord` placement whose compile pass spawned this
/// entity. The incremental compiler's unit teardown despawns by a flat
/// sweep over this marker — NOT by anchor-recursive despawn alone —
/// because entities can leave their anchor's hierarchy after spawning:
/// the 3D gizmo detaches a dragged prim from its parent
/// (`GizmoDetachedPrim`) and the detachment outlives the drag. Before
/// this marker existed, rebuilding a gizmo-edited placement left the
/// detached subtree behind as a duplicate (most visibly: a second
/// water plane after dragging the water layer's Y).
///
/// Avatar-mode spawns carry [`PlacementUnit::NONE`] — no placement owns
/// them, and only the full-pass `RoomEntity` sweep (which ignores this
/// marker) or their own lifecycle retires them.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacementUnit(pub usize);

impl PlacementUnit {
    /// Sentinel for spawns outside the placement system. Shares the
    /// value of [`WaterPlane::NO_OWNER`](crate::water::WaterPlane::NO_OWNER)
    /// — both mean "no placement index will ever match this".
    pub const NONE: usize = crate::water::WaterPlane::NO_OWNER;
}

/// Present once `compile_room_record` has completed at least one pass
/// for the active session. The loading gate
/// ([`crate::loading::check_loading_complete`]) waits on this, so the
/// all-green checklist can't hand over to `InGame` while the world is
/// still an empty heightfield — on wasm the first compile is the
/// longest single-frame stall of the whole boot, and it belongs behind
/// the loading screen. Removed by `logout::cleanup_on_logout` so the
/// next login waits again.
#[derive(Resource)]
pub struct WorldCompiled;

/// Tags the root entity of a `Placement::Absolute` with its index into the
/// live `RoomRecord::placements` vec. `editor_gizmo` reads this to map a
/// selected-in-UI placement to its 3D entity and to commit the gizmo's
/// final Transform back into the record when the user releases the mouse.
#[derive(Component)]
pub struct PlacementMarker(pub usize);

/// Tags every entity spawned from a node inside a named [`crate::pds::Generator`]
/// blueprint. Carries the generator's name plus the child-index chain from
/// the blueprint root so `editor_gizmo` can (a) find every live instance
/// matching a UI-selected node and (b) resolve the dragged Transform back
/// to its slot in the recipe. The path for the blueprint root is an empty
/// `Vec`; each descendant appends its child index at each depth.
#[derive(Component, Clone)]
pub struct PrimMarker {
    pub generator_ref: String,
    pub path: Vec<usize>,
}

/// Avatar-side counterpart to [`PrimMarker`], attached only to nodes
/// spawned for the **local** player's avatar `visuals` tree. Carries the
/// child-index chain from the visuals root so `editor_gizmo` can map an
/// avatar-editor row selection back to a live entity and write the
/// dragged Transform back into `LiveAvatarRecord.0.visuals`.
///
/// Remote peers' avatar visuals deliberately omit this marker — their
/// pose is replicated from the network and is not editable locally — so a
/// query for `&AvatarVisualPrim` is implicitly scoped to the local
/// avatar without a separate `LocalPlayer` filter.
#[derive(Component, Clone)]
pub struct AvatarVisualPrim {
    pub path: Vec<usize>,
}

/// Base meshes for each [`PropMeshType`] — built once at startup so every
/// L-system spawn can share the same handles. Foliage variants (Leaf, Twig)
/// are billboard cards whose UV layout matches the upstream
/// `bevy_symbios_texture` card convention (V=1 at the base).
#[derive(Resource)]
pub struct PropMeshAssets {
    pub meshes: HashMap<PropMeshType, Handle<Mesh>>,
}

pub struct WorldBuilderPlugin;

impl Plugin for WorldBuilderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<WaterMaterial>::default())
            // Content-fingerprinted procedural-texture dedup, consulted by
            // `build_procedural_material` before dispatching a bake and
            // populated by the upstream `patch_procedural_material_textures`
            // system (which takes it as an optional resource — inserting it
            // here is what switches caching on). Survives room changes by
            // design: keys are pure content hashes, so a revisited room
            // re-uses its textures. Capacity note: at the 512² bake size one
            // entry pins ~3 MiB of pixel data (albedo + normal + ORM), so
            // the upstream 256-entry default would allow ~768 MiB — too much
            // headroom for the wasm heap. 64 entries (~192 MiB worst case,
            // far less in practice) covers several rooms' worth of distinct
            // configs before FIFO eviction kicks in.
            .insert_resource(bevy_symbios_texture::TextureCache::memory(64))
            .init_resource::<LSystemMaterialCache>()
            .init_resource::<LSystemMeshCache>()
            .init_resource::<ShapeMaterialCache>()
            .init_resource::<ShapeMeshCache>()
            .init_resource::<bevy_symbios_shape::cache::ShapeMeshCache>()
            .init_resource::<compile::CompiledWorld>()
            .init_resource::<compile::CompileJob>()
            .init_resource::<WaterSurfaces>()
            .init_resource::<image_cache::BlobImageCache>()
            .init_resource::<audio_resolver::BlobAudioCache>()
            .init_resource::<spatial_audio::BakedAudioCache>()
            .init_resource::<particles::ParticleQuadMesh>()
            .init_resource::<particles::ParticleAtlasMeshes>()
            .add_systems(Startup, setup_prop_assets)
            // `not(Login)` rather than `in_state(InGame)`: the room
            // compile is by far the longest single-frame stall on the
            // wasm build (every entity + collider + L-system / shape
            // derivation lands synchronously on the main thread), so it
            // now runs during `Loading` — behind the loading screen —
            // and the gate waits on [`WorldCompiled`] before unveiling
            // the world. The compile additionally waits for a terrain
            // mesh entity: `dispatch_top_level` applies the record's
            // traits to it, and during Loading the mesh only spawns a
            // frame after `FinishedHeightMap` lands — compiling inside
            // that gap would silently skip the terrain traits.
            .add_systems(
                Update,
                (
                    compile::compile_room_record
                        .run_if(any_with_component::<crate::terrain::TerrainMesh>),
                    compile::apply_environment_state,
                    compile::apply_contact_recipes,
                    image_cache::poll_blob_image_tasks,
                    spatial_audio::poll_spatial_audio_tasks,
                    draw_placement_visualizers,
                )
                    .run_if(not(in_state(AppState::Login))),
            )
            // Audio-reference resolver poll runs in Loading too — the
            // loading gate's ambient-bake path dispatches Referenced
            // fetches before InGame is entered, and the gate would
            // hang if the resolver wasn't polling yet.
            .add_systems(Update, audio_resolver::poll_blob_audio_tasks)
            .add_systems(
                Update,
                (
                    particles::update_emitter_motion,
                    particles::tick_emitter_spawn,
                    particles::tick_particles,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn create_foliage_card(width: f32, height: f32) -> Mesh {
    let positions: Vec<[f32; 3]> = vec![
        [-width / 2.0, 0.0, 0.0],
        [width / 2.0, 0.0, 0.0],
        [width / 2.0, height, 0.0],
        [-width / 2.0, height, 0.0],
    ];
    let normals: Vec<[f32; 3]> = vec![[0.0, 0.0, 1.0]; 4];
    let uvs: Vec<[f32; 2]> = vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    let indices = Indices::U32(vec![0, 1, 2, 0, 2, 3]);

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(indices);
    let _ = mesh.generate_tangents();
    mesh
}

/// Build the shared prop-mesh set (one handle per `PropMeshType`). Shared by
/// the [`setup_prop_assets`] startup system and the headless render tool's
/// [`register_headless_spawn`] so both produce identical L-system foliage.
pub(crate) fn build_prop_mesh_assets(meshes: &mut Assets<Mesh>) -> PropMeshAssets {
    let mut prop_meshes = HashMap::new();
    prop_meshes.insert(
        PropMeshType::Leaf,
        meshes.add(create_foliage_card(0.5, 0.8)),
    );
    prop_meshes.insert(
        PropMeshType::Twig,
        meshes.add(create_foliage_card(0.7, 1.0)),
    );
    prop_meshes.insert(
        PropMeshType::Sphere,
        meshes.add(Sphere::new(0.2).mesh().ico(2).unwrap()),
    );
    prop_meshes.insert(
        PropMeshType::Cone,
        meshes.add(Cone::new(0.15, 0.4).mesh().resolution(8)),
    );
    prop_meshes.insert(
        PropMeshType::Cylinder,
        meshes.add(Cylinder::new(0.1, 0.5).mesh().resolution(8)),
    );
    prop_meshes.insert(PropMeshType::Cube, meshes.add(Cuboid::new(0.3, 0.3, 0.3)));

    PropMeshAssets {
        meshes: prop_meshes,
    }
}

/// Startup system that populates [`PropMeshAssets`] with the shared prop
/// meshes (one handle per `PropMeshType`).
fn setup_prop_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    let assets = build_prop_mesh_assets(&mut meshes);
    commands.insert_resource(assets);
}

/// Walks the active `RoomRecord` and produces ECS entities for every
/// placement. Re-runs whenever the record resource is marked changed *or*
/// `FinishedHeightMap` is inserted/modified. The first frame inside
/// `AppState::InGame` counts as a change because the resource was just
/// inserted during Loading, which performs the initial compilation for free.
///
fn apply_traits(commands: &mut Commands, entity: Entity, record: &RoomRecord, generator_ref: &str) {
    let Some(traits) = record.traits.get(generator_ref) else {
        return;
    };
    for t in traits {
        if t == "sensor" {
            commands.entity(entity).insert(Sensor);
        }
    }
}

/// Remove every component that `apply_traits` could have attached. Used on
/// long-lived entities (e.g. the terrain mesh) that survive a room rebuild
/// so a trait deletion actually lands on the live entity instead of
/// leaving the old component stuck in place.
fn reset_traits(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).remove::<Sensor>();
}

fn draw_placement_visualizers(
    mut gizmos: Gizmos,
    editor_state: Res<crate::ui::room::RoomEditorState>,
    record: Option<Res<LiveRoomRecord>>,
    heightmap: Option<Res<FinishedHeightMap>>,
) {
    let Some(record) = record else {
        return;
    };
    let record = &record.0;
    if editor_state.selected_tab != crate::ui::room::EditorTab::Placements {
        return;
    }
    let Some(idx) = editor_state.selected_placement else {
        return;
    };
    let Some(placement) = record.placements.get(idx) else {
        return;
    };

    let get_y = |x: f32, z: f32| -> f32 {
        if let Some(hm_res) = heightmap.as_deref() {
            let hm = &hm_res.0;
            let extent = (hm.width() - 1) as f32 * hm.scale();
            let half = extent * 0.5;
            let hm_x = (x + half).clamp(0.0, extent);
            let hm_z = (z + half).clamp(0.0, extent);
            hm.get_height_at(hm_x, hm_z)
        } else {
            0.0
        }
    };

    let color = Color::srgb(0.0, 1.0, 0.5);

    match placement {
        Placement::Absolute {
            transform,
            snap_to_terrain,
            ..
        } => {
            let mut pos = Vec3::from_array(transform.translation.0);
            if *snap_to_terrain {
                pos.y = get_y(pos.x, pos.z);
            }
            gizmos.sphere(pos, 1.0, color);
        }
        Placement::Scatter {
            bounds,
            snap_to_terrain,
            ..
        } => {
            match bounds {
                ScatterBounds::Circle { center, radius } => {
                    let mut pos = Vec3::new(center.0[0], 0.0, center.0[1]);
                    if *snap_to_terrain {
                        pos.y = get_y(pos.x, pos.z);
                    }
                    let iso =
                        Isometry3d::new(pos, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));
                    gizmos.circle(iso, radius.0, color);
                }
                ScatterBounds::Rect {
                    center,
                    extents,
                    rotation,
                } => {
                    let mut pos = Vec3::new(center.0[0], 0.0, center.0[1]);
                    if *snap_to_terrain {
                        pos.y = get_y(pos.x, pos.z);
                    }
                    // Align the rect to lie flat on the XZ plane
                    let rot = Quat::from_rotation_y(rotation.0)
                        * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
                    let size = Vec2::new(extents.0[0] * 2.0, extents.0[1] * 2.0);
                    gizmos.rect(Isometry3d::new(pos, rot), size, color);
                }
            }
        }
        Placement::Grid {
            transform,
            counts,
            gaps,
            snap_to_terrain,
            ..
        } => {
            let mut pos = Vec3::from_array(transform.translation.0);
            if *snap_to_terrain {
                pos.y = get_y(pos.x, pos.z);
            }
            let rot = Quat::from_array(transform.rotation.0);
            let w = ((counts[0] as f32) - 1.0).max(0.0) * gaps.0[0];
            let h = ((counts[1] as f32) - 1.0).max(0.0) * gaps.0[1];
            let d = ((counts[2] as f32) - 1.0).max(0.0) * gaps.0[2];

            // Draw 3 intersecting planes as an elegant bounding volume visualization
            let iso = Isometry3d::new(pos, rot);
            gizmos.rect(iso, Vec2::new(w + 1.0, d + 1.0), color);
            gizmos.rect(
                Isometry3d::new(
                    pos,
                    rot * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                ),
                Vec2::new(w + 1.0, h + 1.0),
                color,
            );
            gizmos.rect(
                Isometry3d::new(
                    pos,
                    rot * Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                ),
                Vec2::new(d + 1.0, h + 1.0),
                color,
            );
        }
        _ => {}
    }
}
