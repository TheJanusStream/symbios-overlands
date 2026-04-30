//! Per-`Generator` recursive spawn dispatch. The placement walker in
//! `compile_room_record` calls [`dispatch_top_level`] for each placement;
//! that resolves the named generator and routes the recursive walk into
//! [`spawn_generator`], which dispatches each [`GeneratorKind`] variant
//! into its sibling-module spawner (`prim`, `lsystem`, `shape`, `sign`,
//! `portal`, `particles`, `material::spawn_water_volume`).

use bevy::prelude::*;

use crate::config::terrain as tcfg;
use crate::pds::{Generator, GeneratorKind};

use super::super::lsystem::spawn_lsystem_entity;
use super::super::material::{spawn_procedural_material, spawn_water_volume};
use super::super::particles::{snapshot_from_record, spawn_particle_emitter_entity};
use super::super::portal::spawn_portal_entity;
use super::super::prim::{build_primitive_mesh, collider_for_primitive};
use super::super::shape::spawn_shape_entity;
use super::super::sign::spawn_sign_entity;
use super::super::{PrimMarker, RoomEntity, apply_traits, reset_traits};

use super::spawn_ctx::{SpawnCtx, budget_exceeded, transform_from_data};

/// Entry point called by the top-level `Placement` loop. Resolves the
/// generator by name, composes the placement-level cell transform with the
/// named generator's own root transform, and routes the recursive walk
/// into [`spawn_generator`] with an empty blueprint path. The returned
/// entity is the placement's root (caller adopts it as a child of the
/// placement anchor).
///
/// `cell_tf` is the per-cell transform contributed by the placement (the
/// per-grid-cell offset + yaw, the per-scatter-sample local position +
/// yaw, or `Transform::IDENTITY` for a single absolute placement). The
/// generator's authored `transform` is composed *inside* it: the final
/// root pose is `cell_tf * generator.transform`. So a `Placement::Absolute`
/// plants the generator at its authored pose, while a Grid or Scatter cell
/// shifts and rotates that pose by the cell's contribution.
///
/// Traits are applied here rather than inside `spawn_generator` because
/// only a top-level placement is keyed directly by `generator_ref` in the
/// record's `traits` table — children inside a tree share the named
/// generator's traits via the anchor and should not double-apply.
pub(crate) fn dispatch_top_level(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    generator_ref: &str,
    cell_tf: Transform,
) -> Option<Entity> {
    let Some(generator) = ctx.record.generators.get(generator_ref) else {
        warn!(
            "Placement references unknown generator `{}` — skipped",
            generator_ref
        );
        return None;
    };

    // Terrain is special: the heightmap mesh is already owned by the
    // terrain plugin (its config drives `FinishedHeightMap` upstream of
    // this pass). Apply the record's traits to those existing entities so
    // the heightfield collider lands on the live terrain mesh, then fall
    // through to the normal spawn path — `spawn_generator` will produce a
    // bare anchor entity for the Terrain root and walk its children. The
    // `traits` table thus targets the terrain mesh, while the children
    // (L-systems, props, water, …) ride along on the anchor.
    let is_terrain_root = matches!(&generator.kind, GeneratorKind::Terrain(_));
    if is_terrain_root {
        for terrain_entity in ctx.terrain_meshes.iter() {
            reset_traits(ctx.commands, terrain_entity);
            apply_traits(ctx.commands, terrain_entity, ctx.record, generator_ref);
        }
    }

    // Clone out the named generator so the recursive spawner doesn't have
    // to re-borrow `ctx.record.generators` at every depth. The clone is per
    // placement, not per scatter sample, so the cost is bounded.
    //
    // Water children of scattered/gridded blueprints used to be stripped here
    // because each cell would spawn a redundant world-extent plane. With
    // finite, transform-bounded surfaces tracked in `WaterSurfaces`, scattered
    // ponds are now legitimate — each cell's local transform produces a
    // distinct entry in the registry — so the strip step has been removed.
    let generator = generator.clone();
    let root_tf = cell_tf * transform_from_data(&generator.transform);
    let entity = spawn_generator(ctx, &generator, generator_ref, &[], root_tf);
    if let Some(entity) = entity
        && !is_terrain_root
    {
        // For non-terrain roots, traits attach to the spawned root entity.
        // Terrain refs already routed traits to the heightmap mesh above —
        // applying them again on the anchor would attach `Sensor` /
        // `collider_heightfield` to a transform-only node, which is wrong.
        apply_traits(ctx.commands, entity, ctx.record, generator_ref);
    }
    entity
}

/// Unified recursive spawner. Builds the entity tree for `generator`,
/// parented under a `base_ref`-qualified synthetic path so nested L-system
/// and procedural-texture caches stay collision-free across fractal
/// nestings.
///
/// * `base_ref` is the top-level generator's key in `RoomRecord::generators`.
/// * `path` records the child-index chain from the named generator's root
///   down to this node. It is `&[]` for the root of the named blueprint
///   itself, and grows by one index at each recursion into `children`.
///
/// The returned entity is the node's visible/physical root. Trait
/// application is the caller's responsibility — this function deliberately
/// does not apply traits so recursion into a generator's children doesn't
/// double-attach `Sensor` or `collider_heightfield` components.
pub fn spawn_generator(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    generator: &Generator,
    base_ref: &str,
    path: &[usize],
    transform: Transform,
) -> Option<Entity> {
    if budget_exceeded(*ctx.entities_spawned, ctx.budget_warned) {
        return None;
    }
    let cache_key = synthetic_cache_key(base_ref, path);
    let in_blueprint = !path.is_empty();

    let entity = match &generator.kind {
        // Terrain is root-only (sanitizer enforces). Its heightmap mesh is
        // owned by the terrain plugin — we don't spawn it here. We do
        // spawn a bare anchor entity so the Terrain root's children (the
        // region's water, L-systems, portals, props, …) have a per-instance
        // parent to attach to.
        GeneratorKind::Terrain(_) => {
            if in_blueprint {
                warn!("Terrain generator ignored as a child at `{cache_key}`");
                return None;
            }
            Some(
                ctx.commands
                    .spawn((transform, Visibility::default(), RoomEntity))
                    .id(),
            )
        }
        // Water is child-only (sanitizer enforces). Spawning at root would
        // place an unparented infinite cuboid at the world water level,
        // which is exactly the "stray top-level water" case the strict
        // rule forbids.
        GeneratorKind::Water {
            level_offset,
            surface,
        } => {
            if !in_blueprint {
                warn!("Water generator ignored at root at `{cache_key}`");
                return None;
            }
            let world_extent = ctx
                .heightmap
                .map(|hm| (hm.0.width() - 1) as f32 * hm.0.scale())
                .unwrap_or_else(|| (tcfg::GRID_SIZE - 1) as f32 * tcfg::CELL_SCALE);
            Some(spawn_water_volume(
                ctx.commands,
                level_offset.0,
                surface,
                &ctx.record.environment,
                transform,
                world_extent,
                ctx.meshes,
                ctx.water_materials,
                ctx.water_surfaces,
            ))
        }
        GeneratorKind::Shape { .. } => {
            // Synthetic cache key matches the L-system convention so a
            // Shape nested at `path=[2,0]` inside a Construct doesn't
            // collide with an unrelated Shape in another branch.
            spawn_shape_entity(ctx, &generator.kind, &cache_key, transform)
        }
        GeneratorKind::LSystem { .. } => {
            // Synthetic cache key keeps a nested L-system distinct from any
            // siblings (and from the outer named generator) so
            // `LSystemMeshCache` entries don't clobber each other.
            // Scattering 1000 generator trees each containing the same
            // L-system at path=[0] reuses the same "<base_ref>/0" cache
            // entry — 1 derivation, 999 handle clones.
            spawn_lsystem_entity(ctx, &generator.kind, &cache_key, transform)
        }
        GeneratorKind::Portal {
            target_did,
            target_pos,
        } => Some(spawn_portal_entity(ctx, target_did, target_pos, transform)),
        GeneratorKind::Cuboid { .. }
        | GeneratorKind::Sphere { .. }
        | GeneratorKind::Cylinder { .. }
        | GeneratorKind::Capsule { .. }
        | GeneratorKind::Cone { .. }
        | GeneratorKind::Torus { .. }
        | GeneratorKind::Plane { .. }
        | GeneratorKind::Tetrahedron { .. } => {
            Some(spawn_primitive_entity(ctx, &generator.kind, transform))
        }
        GeneratorKind::Sign {
            source,
            size,
            uv_repeat,
            uv_offset,
            material,
            double_sided,
            alpha_mode,
            unlit,
        } => Some(spawn_sign_entity(
            ctx,
            source,
            size,
            uv_repeat,
            uv_offset,
            material,
            *double_sided,
            alpha_mode,
            *unlit,
            transform,
        )),
        GeneratorKind::ParticleSystem {
            emitter_shape,
            rate_per_second,
            burst_count,
            max_particles,
            looping,
            duration,
            lifetime_min,
            lifetime_max,
            speed_min,
            speed_max,
            gravity_multiplier,
            acceleration,
            linear_drag,
            start_size,
            end_size,
            start_color,
            end_color,
            blend_mode,
            billboard,
            simulation_space,
            inherit_velocity,
            collide_terrain,
            collide_water,
            collide_colliders,
            bounce,
            friction,
            seed,
            texture,
            texture_atlas,
            frame_mode,
            texture_filter,
        } => {
            let snapshot = snapshot_from_record(
                emitter_shape,
                rate_per_second.0,
                *burst_count,
                *max_particles,
                *looping,
                duration.0,
                lifetime_min.0,
                lifetime_max.0,
                speed_min.0,
                speed_max.0,
                gravity_multiplier.0,
                acceleration,
                linear_drag.0,
                start_size.0,
                end_size.0,
                start_color,
                end_color,
                blend_mode,
                *billboard,
                simulation_space,
                inherit_velocity.0,
                *collide_terrain,
                *collide_water,
                *collide_colliders,
                bounce.0,
                friction.0,
                texture.clone(),
                texture_atlas.clone(),
                frame_mode.clone(),
                texture_filter.clone(),
            );
            Some(spawn_particle_emitter_entity(
                ctx, snapshot, *seed, transform,
            ))
        }
        GeneratorKind::Unknown => {
            warn!("Ignoring generator `{cache_key}` of unknown $type");
            None
        }
    };

    // Attach a PrimMarker to every node in the named generator's tree so
    // the editor gizmo can map a UI-selected node back to its live Bevy
    // entity by `(generator_ref, path)`. Top-level placements *also* get
    // PlacementMarker from the caller, but that lives on the outer anchor
    // — the generator entity itself always carries PrimMarker now so the
    // gizmo can target the root with `path=[]`.
    if let Some(e) = entity {
        // Charge the global budget here rather than at the spawn sites in
        // each variant arm: this is the one place that fires exactly once
        // per node we actually committed to the world, and the variants'
        // own internal entity counts (lsystem mesh buckets, portal top
        // face) are bounded constant multiples of this.
        *ctx.entities_spawned = ctx.entities_spawned.saturating_add(1);
        if !ctx.avatar_mode {
            // Room geometry: PrimMarker carries (generator_ref, path) so
            // the gizmo can find every live instance of a UI-selected
            // node by matching both keys.
            ctx.commands.entity(e).insert(PrimMarker {
                generator_ref: base_ref.to_string(),
                path: path.to_vec(),
            });
        } else if ctx.local_avatar_mode {
            // Local player's own avatar: tag with `AvatarVisualPrim` so
            // the gizmo can target a visuals node by `path`. Remote peers
            // skip this marker — their avatars replicate from the
            // network and aren't locally editable, so a query for
            // `&AvatarVisualPrim` is implicitly local-player-scoped.
            ctx.commands
                .entity(e)
                .insert(crate::world_builder::AvatarVisualPrim {
                    path: path.to_vec(),
                });
        }
        // Recurse into the children list, parenting each child entity to
        // this node's generated entity so the hierarchy mirrors the
        // blueprint shape.
        spawn_generator_children(ctx, generator, e, base_ref, path);
    }

    entity
}

/// Recursive walk of a generator's children. Each child is spawned as a
/// direct child of `parent_entity` (the generated entity for the parent
/// node, not its anchor), with its path extended by the child index.
fn spawn_generator_children(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    parent_node: &Generator,
    parent_entity: Entity,
    base_ref: &str,
    parent_path: &[usize],
) {
    for (i, child) in parent_node.children.iter().enumerate() {
        let mut child_path = parent_path.to_vec();
        child_path.push(i);
        let child_tf = transform_from_data(&child.transform);
        if let Some(child_entity) = spawn_generator(ctx, child, base_ref, &child_path, child_tf) {
            ctx.commands.entity(parent_entity).add_child(child_entity);
        }
    }
}

fn synthetic_cache_key(base_ref: &str, path: &[usize]) -> String {
    if path.is_empty() {
        base_ref.to_string()
    } else {
        let suffix = path
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("/");
        format!("{base_ref}/{suffix}")
    }
}

/// Spawn a parametric primitive entity: build its mesh (with vertex torture
/// when configured), pair it with a PBR material handle, and attach the
/// matching collider if the node is solid. Always carries `RoomEntity` so
/// the compile-pass cleanup sweeps it even when detached from the anchor
/// hierarchy by the gizmo.
fn spawn_primitive_entity(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    kind: &GeneratorKind,
    transform: Transform,
) -> Entity {
    let (solid, material_settings) = match kind {
        GeneratorKind::Cuboid {
            solid, material, ..
        }
        | GeneratorKind::Sphere {
            solid, material, ..
        }
        | GeneratorKind::Cylinder {
            solid, material, ..
        }
        | GeneratorKind::Capsule {
            solid, material, ..
        }
        | GeneratorKind::Cone {
            solid, material, ..
        }
        | GeneratorKind::Torus {
            solid, material, ..
        }
        | GeneratorKind::Plane {
            solid, material, ..
        }
        | GeneratorKind::Tetrahedron {
            solid, material, ..
        } => (*solid, material.clone()),
        _ => unreachable!("spawn_primitive_entity called on non-primitive kind"),
    };

    let raw_mesh = build_primitive_mesh(kind);
    // Avatar mode strips colliders unconditionally — the locomotion
    // preset's chassis collider is the only physics body on the avatar,
    // and per-prim colliders here would register as Static and conflict
    // with the chassis's dynamic body.
    let collider = if solid && !ctx.avatar_mode {
        collider_for_primitive(kind, &raw_mesh)
    } else {
        None
    };
    let mesh_handle = ctx.meshes.add(raw_mesh);
    let material_handle = spawn_procedural_material(ctx, &material_settings);

    let mut cmd = ctx.commands.spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle),
        transform,
    ));
    if !ctx.avatar_mode {
        cmd.insert(RoomEntity);
    }
    if let Some(collider) = collider {
        cmd.insert(collider);
    }
    cmd.id()
}
