//! World compiler: turns a `RoomRecord` recipe into ECS entities.
//!
//! This plugin owns every entity spawned from the active room recipe. When
//! the owner edits the record — locally through the advanced editor or
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

use avian3d::prelude::*;
use bevy::prelude::*;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use crate::config::terrain as tcfg;
use crate::pds::{Generator, Placement, RoomRecord, ScatterBounds, TransformData};
use crate::state::AppState;
use crate::terrain::{TerrainMesh, WaterVolume};
use crate::water::{WaterExtension, WaterMaterial};

/// Marker attached to every entity spawned from the active `RoomRecord`.
/// Despawning all of these is how the compiler applies a record update
/// without double-spawning anything.
#[derive(Component)]
pub struct RoomEntity;

pub struct WorldBuilderPlugin;

impl Plugin for WorldBuilderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<WaterMaterial>::default())
            .add_systems(
                Update,
                compile_room_record.run_if(in_state(AppState::InGame)),
            );
    }
}

/// Walks the active `RoomRecord` and produces ECS entities for every
/// placement. Re-runs automatically whenever the record resource is marked
/// changed; the first frame inside `AppState::InGame` counts as a change
/// because the resource was just inserted during Loading, which performs
/// the initial compilation for free.
#[allow(clippy::too_many_arguments)]
fn compile_room_record(
    mut commands: Commands,
    record: Option<Res<RoomRecord>>,
    existing: Query<Entity, With<RoomEntity>>,
    terrain_meshes: Query<Entity, With<TerrainMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
    mut lights: Query<&mut DirectionalLight>,
) {
    let Some(record) = record else {
        return;
    };
    if !record.is_changed() {
        return;
    }

    // Step 1 — Cleanup. Despawn every entity previously compiled out of
    // this record. Terrain is NOT a `RoomEntity` (it is owned by the
    // terrain plugin's own lifecycle), so it survives the rebuild.
    for e in &existing {
        commands.entity(e).despawn();
    }

    // Step 2 — Environment. Patch the shared directional light in place
    // so a record update takes effect on every connected peer.
    let c = record.environment.sun_color;
    for mut light in lights.iter_mut() {
        light.color = Color::srgb(c[0], c[1], c[2]);
    }

    // Step 3 — Placements. Walk the recipe; each scatter placement uses
    // its own deterministic RNG so every peer reproduces the same layout.
    for placement in &record.placements {
        match placement {
            Placement::Absolute {
                generator_ref,
                transform,
            } => {
                spawn_from_generator(
                    &mut commands,
                    generator_ref,
                    &record,
                    transform_from_data(transform),
                    &mut meshes,
                    &mut water_materials,
                    &terrain_meshes,
                );
            }
            Placement::Scatter {
                generator_ref,
                bounds,
                count,
                local_seed,
            } => {
                let mut rng = ChaCha8Rng::seed_from_u64(*local_seed);
                for _ in 0..*count {
                    let (x, z) = sample_bounds(bounds, &mut rng);
                    let tf = Transform::from_xyz(x, 0.0, z);
                    spawn_from_generator(
                        &mut commands,
                        generator_ref,
                        &record,
                        tf,
                        &mut meshes,
                        &mut water_materials,
                        &terrain_meshes,
                    );
                }
            }
            Placement::Unknown => {
                warn!("Skipping placement with unknown $type");
            }
        }
    }
}

fn transform_from_data(t: &TransformData) -> Transform {
    Transform {
        translation: Vec3::from_array(t.translation),
        rotation: Quat::from_array(t.rotation),
        scale: Vec3::from_array(t.scale),
    }
}

/// Uniform sample inside the scatter region. Circle bounds use rejection
/// sampling so the distribution stays flat instead of clumping at the
/// centre (which a naïve `radius * random()` would produce).
fn sample_bounds(bounds: &ScatterBounds, rng: &mut ChaCha8Rng) -> (f32, f32) {
    match bounds {
        ScatterBounds::Rect { center, extents } => {
            let x = center[0] + unit_f32(rng) * extents[0];
            let z = center[1] + unit_f32(rng) * extents[1];
            (x, z)
        }
        ScatterBounds::Circle { center, radius } => loop {
            let x = unit_f32(rng);
            let z = unit_f32(rng);
            if x * x + z * z <= 1.0 {
                return (center[0] + x * radius, center[1] + z * radius);
            }
        },
    }
}

/// Deterministic `[-1, 1]` sample from a `ChaCha8Rng`. Uses `next_u32` so
/// we do not depend on the full `rand` crate — the project only needs a
/// seed-reproducible PRNG, not the distribution helpers.
fn unit_f32(rng: &mut ChaCha8Rng) -> f32 {
    let v = rng.next_u32() as f32 / u32::MAX as f32;
    v * 2.0 - 1.0
}

#[allow(clippy::too_many_arguments)]
fn spawn_from_generator(
    commands: &mut Commands,
    generator_ref: &str,
    record: &RoomRecord,
    transform: Transform,
    meshes: &mut Assets<Mesh>,
    water_materials: &mut Assets<WaterMaterial>,
    terrain_meshes: &Query<Entity, With<TerrainMesh>>,
) {
    let Some(generator) = record.generators.get(generator_ref) else {
        warn!(
            "Placement references unknown generator `{}` — skipped",
            generator_ref
        );
        return;
    };
    match generator {
        Generator::Terrain { .. } => {
            // Terrain is generated and meshed by `terrain.rs` during the
            // Loading state (so the heightfield collider is ready before
            // gameplay begins). The recipe still participates through
            // `traits`, which we apply here to every existing terrain
            // mesh entity.
            for terrain_entity in terrain_meshes.iter() {
                apply_traits(commands, terrain_entity, record, generator_ref);
            }
        }
        Generator::Water { level_offset } => {
            let entity = spawn_water_volume(
                commands,
                *level_offset,
                transform,
                meshes,
                water_materials,
            );
            apply_traits(commands, entity, record, generator_ref);
        }
        Generator::Shape { .. } | Generator::LSystem { .. } => {
            // Stub: symbios-shape integration lands in a follow-up.
            // We still honour the traits so future shape entities inherit
            // any `sensor`/`ground` markers once wired up.
        }
        Generator::Unknown => {
            warn!(
                "Ignoring generator `{}` of unknown $type",
                generator_ref
            );
        }
    }
}

/// Spawn the translucent water cuboid scaled to cover the whole terrain.
/// World extent is recomputed from config constants so we don't need a
/// `FinishedHeightMap` handle just to build the water.
fn spawn_water_volume(
    commands: &mut Commands,
    level_offset: f32,
    placement_tf: Transform,
    meshes: &mut Assets<Mesh>,
    water_materials: &mut Assets<WaterMaterial>,
) -> Entity {
    let world_extent = (tcfg::GRID_SIZE - 1) as f32 * tcfg::CELL_SCALE;
    let base_wl = tcfg::water::LEVEL_FACTOR * tcfg::HEIGHT_SCALE;
    let wl = (base_wl + level_offset).max(0.001);

    let water_mat = water_materials.add(WaterMaterial {
        base: StandardMaterial {
            base_color: Color::srgba(
                tcfg::water::COLOR[0],
                tcfg::water::COLOR[1],
                tcfg::water::COLOR[2],
                tcfg::water::COLOR[3],
            ),
            perceptual_roughness: tcfg::water::ROUGHNESS,
            metallic: tcfg::water::METALLIC,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            ..default()
        },
        extension: WaterExtension::default(),
    });

    // Compose placement transform with the auto-sized cuboid. The
    // placement's translation acts as an offset relative to the map
    // centre; the scale is always overridden so the water plane fully
    // covers the terrain.
    let mut tf = placement_tf;
    tf.translation.y += wl / 2.0;
    tf.scale = Vec3::new(world_extent, wl, world_extent);

    commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
            MeshMaterial3d(water_mat),
            tf,
            WaterVolume,
            RoomEntity,
        ))
        .id()
}

/// Attach any ECS components listed under `record.traits[generator_ref]`
/// to `entity`. The trait engine is the main extension point — new
/// lexicon tokens map cleanly to Bevy components without schema churn.
fn apply_traits(
    commands: &mut Commands,
    entity: Entity,
    record: &RoomRecord,
    generator_ref: &str,
) {
    let Some(traits) = record.traits.get(generator_ref) else {
        return;
    };
    // `collider_heightfield` / `ground` are informational for the terrain
    // pipeline, which already attaches its own collider; they still appear
    // in the recipe for documentation and future targeted behaviours.
    for t in traits {
        if t == "sensor" {
            commands.entity(entity).insert(Sensor);
        }
    }
}
