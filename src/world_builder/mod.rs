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
//! * [`compile`] — the main `compile_room_record` system, its atmospheric
//!   sibling `apply_environment_state`, the shared `SpawnCtx`, and scatter
//!   math helpers.
//! * [`lsystem`] — L-system geometry + material caches and the spawn path.
//! * [`prim`] — Construct/Prim spawners and parametric mesh/collider
//!   builders.
//! * [`portal`] — portal cube spawning + avatar picture polling.
//! * [`material`] — water volume spawn, procedural material bridge, and
//!   foliage texture task polling.

mod compile;
mod lsystem;
mod material;
pub mod portal;
mod prim;
mod shape;

use std::collections::HashMap;

use avian3d::prelude::Sensor;
use bevy::asset::RenderAssetUsages;
use bevy::math::Isometry3d;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::tasks::Task;
use bevy_symbios_texture::generator::{TextureError, TextureMap};

use crate::pds::{Placement, PropMeshType, RoomRecord, ScatterBounds};
use crate::state::AppState;
use crate::terrain::FinishedHeightMap;
use crate::water::{WaterMaterial, WaterSurfaces};

pub use lsystem::{LSystemMaterialCache, LSystemMeshCache};
pub use material::build_procedural_material;
pub use shape::{ShapeMaterialCache, ShapeMeshCache};

/// Marks an in-scene portal cube and carries the destination coordinates the
/// interaction system reads when the local player's sensor-collision set
/// touches it.
#[derive(Component, Clone)]
pub struct PortalMarker {
    pub target_did: String,
    pub target_pos: Vec3,
}

/// In-flight ATProto profile-picture fetch for the top face of a portal cube.
/// Drained by `poll_portal_avatar_tasks`; the task lives on its own entity so
/// the portal itself can be despawned by a room rebuild without having to
/// cancel the future explicitly.
///
/// Keyed by `did` rather than a single material handle: the
/// [`portal::PortalAvatarCache`] resource holds the list of every portal
/// material waiting on this DID's image, so one task fans out to N portals.
#[derive(Component)]
pub struct PortalAvatarTask {
    pub(crate) task: bevy::tasks::Task<crate::avatar::AvatarFetchResult>,
    pub(crate) did: String,
}

/// Marker attached to every entity spawned from the active `RoomRecord`.
/// Despawning all of these is how the compiler applies a record update
/// without double-spawning anything.
#[derive(Component)]
pub struct RoomEntity;

/// Tags the root entity of a `Placement::Absolute` with its index into the
/// live `RoomRecord::placements` vec. `editor_gizmo` reads this to map a
/// selected-in-UI placement to its 3D entity and to commit the gizmo's
/// final Transform back into the record when the user releases the mouse.
#[derive(Component)]
pub struct PlacementMarker(pub usize);

/// Tags every entity spawned from a node in a `Generator::Construct`
/// blueprint. Carries the generator's name plus the child-index chain from
/// the blueprint root so `editor_gizmo` can (a) find every live instance
/// matching a UI-selected prim and (b) resolve the dragged Transform back
/// to its slot in the recipe. The path for the blueprint root is an empty
/// `Vec`; each descendant appends its child index at each depth.
#[derive(Component, Clone)]
pub struct PrimMarker {
    pub generator_ref: String,
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

/// A single in-flight foliage texture task: the async generator future, the
/// material handle whose textures should be populated when the result
/// arrives, and a `is_card` flag selecting between `map_to_images` (tileable)
/// and `map_to_images_card` (clamp-to-edge) upload paths.
pub type FoliageTask = (
    Task<Result<TextureMap, TextureError>>,
    Handle<StandardMaterial>,
    bool,
);

/// In-flight foliage texture tasks, drained by `poll_overlands_foliage_tasks`.
#[derive(Resource, Default)]
pub struct OverlandsFoliageTasks {
    pub tasks: Vec<FoliageTask>,
}

pub struct WorldBuilderPlugin;

impl Plugin for WorldBuilderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<WaterMaterial>::default())
            .init_resource::<OverlandsFoliageTasks>()
            .init_resource::<LSystemMaterialCache>()
            .init_resource::<LSystemMeshCache>()
            .init_resource::<ShapeMaterialCache>()
            .init_resource::<ShapeMeshCache>()
            .init_resource::<WaterSurfaces>()
            .init_resource::<portal::PortalAvatarCache>()
            .add_systems(Startup, setup_prop_assets)
            .add_systems(
                Update,
                (
                    compile::compile_room_record,
                    compile::apply_environment_state,
                    material::poll_overlands_foliage_tasks,
                    portal::poll_portal_avatar_tasks,
                    draw_placement_visualizers,
                )
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

/// Startup system that populates [`PropMeshAssets`] with the shared prop
/// meshes (one handle per `PropMeshType`).
fn setup_prop_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
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

    commands.insert_resource(PropMeshAssets {
        meshes: prop_meshes,
    });
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
    record: Option<Res<RoomRecord>>,
    heightmap: Option<Res<FinishedHeightMap>>,
) {
    let Some(record) = record else {
        return;
    };
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
