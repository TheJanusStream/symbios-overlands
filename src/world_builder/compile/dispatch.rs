//! Per-`Generator` recursive spawn dispatch. The placement walker in
//! `compile_room_record` calls [`dispatch_top_level`] for each placement;
//! that resolves the named generator and routes the recursive walk into
//! [`spawn_generator`], which dispatches each [`GeneratorKind`] variant
//! into its sibling-module spawner (`prim`, `lsystem`, `shape`, `sign`,
//! `portal`, `particles`, `material::spawn_water_volume`).

use std::sync::Arc;

use bevy::prelude::*;

use crate::config::terrain as tcfg;
use crate::pds::{Generator, GeneratorKind};

use super::super::generator_cache::settings_fingerprint;
use super::super::lsystem::spawn_lsystem_entity;
use super::super::material::{spawn_procedural_material, spawn_water_volume};
use super::super::particles::{snapshot_from_record, spawn_particle_emitter_entity};
use super::super::portal::spawn_portal_entity;
use super::super::prim::{
    build_primitive_groups, build_primitive_mesh, collider_for_primitive, group_material,
    plan_faces, prim_parts,
};
use super::super::prim_cache::{CachedGroup, bound_capacity, get_and_touch, prim_mesh_key};
use super::super::shape::spawn_shape_entity;
use super::super::sign::spawn_sign_entity;
use super::super::{
    PlacementUnit, PrimFaceGroup, PrimMarker, RoomEntity, apply_traits, reset_traits,
};

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
    // Copy the shared record reference out of `ctx` so the borrowed generator
    // is tied to the record's lifetime (`'a`), not to `ctx`. That lets the
    // recursive `spawn_generator` take `&mut ctx` below WITHOUT first deep-
    // cloning the whole subtree — which `dispatch_top_level` runs once per grid
    // cell / scatter sample (up to the ~500k entity cap), so the clone was the
    // pipeline's dominant per-sample allocation (#636). Same proven trick as
    // `start_unit` (mod.rs).
    let record = ctx.record;
    let Some(generator) = record.generators.get(generator_ref) else {
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

    // Water children of scattered/gridded blueprints used to be stripped here
    // because each cell would spawn a redundant world-extent plane. With
    // finite, transform-bounded surfaces tracked in `WaterSurfaces`, scattered
    // ponds are now legitimate — each cell's local transform produces a
    // distinct entry in the registry — so the strip step has been removed.
    let root_tf = cell_tf * transform_from_data(&generator.transform);
    let entity = spawn_generator(ctx, generator, generator_ref, &[], root_tf);
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
                    .spawn((
                        transform,
                        Visibility::default(),
                        RoomEntity,
                        PlacementUnit(ctx.placement_index),
                    ))
                    .id(),
            )
        }
        // Water is child-only (sanitizer enforces). Spawning at root would
        // place an unparented infinite cuboid at the world water level,
        // which is exactly the "stray top-level water" case the strict
        // rule forbids.
        GeneratorKind::Water { surface } => {
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
                surface,
                &ctx.record.environment,
                transform,
                world_extent,
                ctx.meshes,
                ctx.water_materials,
                ctx.water_surfaces,
                ctx.placement_index,
            ))
        }
        // The road network's mesh is built by the terrain plugin from its
        // config + the finished heightmap (same reason Terrain's own mesh isn't
        // spawned here — the heightmap is owned upstream). Inert in the compile
        // dispatch; a misplaced root instance simply produces no roads.
        GeneratorKind::RoadNetwork(_) => None,
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
        GeneratorKind::Gateway { size } => Some(
            crate::world_builder::gateway::spawn_gateway_entity(ctx, size, transform),
        ),
        crate::for_each_primitive!(pattern {}) => {
            Some(spawn_primitive_entity(ctx, &generator.kind, transform))
        }
        // The legacy `uv_repeat` / `uv_offset` are not read here: the
        // sanitizer folds them into `material` and resets them (#964).
        GeneratorKind::Sign {
            source,
            size,
            material,
            double_sided,
            alpha_mode,
            unlit,
            texture_filter,
            ..
        } => Some(spawn_sign_entity(
            ctx,
            source,
            size,
            material,
            *double_sided,
            alpha_mode,
            *unlit,
            texture_filter,
            transform,
        )),
        GeneratorKind::ParticleSystem(params) => {
            let snapshot = snapshot_from_record(params);
            Some(spawn_particle_emitter_entity(
                ctx,
                snapshot,
                params.seed,
                transform,
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
        } else if let Some(rkey) = ctx.attachment_rkey.as_ref() {
            // One of the local player's worn props (#1098): the part
            // marker keyed by its attachment record, so the parts editor
            // can select, pick and drag nodes of the worn copy.
            ctx.commands
                .entity(e)
                .insert(crate::world_builder::AttachmentPrim {
                    rkey: rkey.clone(),
                    path: path.to_vec(),
                });
        }
        // Recurse into the children list, parenting each child entity to
        // this node's generated entity so the hierarchy mirrors the
        // blueprint shape.
        spawn_generator_children(ctx, generator, e, base_ref, path);

        // Per-construct spatial audio (#301, expanded by #308 to
        // resolve Referenced sources). Mute-by-default — the
        // dispatcher no-ops on `SovereignAudioConfig::None` /
        // `Unknown`. For Referenced variants the audio resolver
        // coalesces fetches; for procedural variants a background
        // bake fires. Either path lands on the same
        // `poll_spatial_audio_tasks` /
        // `audio_resolver::poll_blob_audio_tasks` attach point.
        super::super::spatial_audio::dispatch_construct_audio(
            ctx.commands,
            ctx.blob_audio_cache,
            ctx.baked_audio_cache,
            e,
            &generator.audio,
        );
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
    // The one primitive destructure lives in `prim::shapes::prim_parts`
    // (#644); non-primitive kinds can't reach here — the router's variant
    // list gates the call.
    let parts = prim_parts(kind).expect("spawn_primitive_entity called on non-primitive kind");
    let solid = parts.solid;

    // How this prim's faces partition into materials (#959). A prim with no
    // per-face overrides plans as one whole group and takes exactly the
    // pre-#959 path below: one entity, one mesh, one material.
    let plan = plan_faces(kind);

    // Content-addressed dedup (#918): every instance of a scattered prop
    // hashes identically, so a scatter of N cards shares one mesh handle and
    // one material handle instead of allocating N of each.
    let mesh_key = prim_mesh_key(kind, &plan);

    // The collider is derived from the mesh data, which a cache hit does not
    // hand back — so build the meshes only when actually needed, and reuse
    // them for both the handles and the collider on a miss.
    //
    // `get_and_touch` also marks the key reachable for this pass, on hit and
    // miss alike, which is what the end-of-job GC retains against (#919).
    let cached = get_and_touch(ctx.prim_mesh_cache, ctx.prim_mesh_touched, mesh_key);
    let needs_collider = solid && !ctx.avatar_mode;
    let (groups, collider) = match cached {
        // Avatar mode strips colliders unconditionally — the locomotion
        // preset's chassis collider is the only physics body on the avatar,
        // and per-prim colliders here would register as Static and conflict
        // with the chassis's dynamic body.
        Some(groups) if !needs_collider => (groups, None),
        Some(groups) => {
            let whole = build_primitive_mesh(kind).mesh;
            (groups, collider_for_primitive(kind, &whole))
        }
        None => {
            let built = build_primitive_groups(kind, &plan);
            let collider = if needs_collider {
                // A whole prim's only mesh *is* the whole prim, so its hull
                // comes straight from it. A split one needs the unsplit
                // geometry — the hull must stand off the shape, not one
                // group of its faces.
                match (plan.is_whole(), built.first()) {
                    (true, Some(g)) => collider_for_primitive(kind, &g.mesh),
                    _ => collider_for_primitive(kind, &build_primitive_mesh(kind).mesh),
                }
            } else {
                None
            };
            let groups: Arc<[CachedGroup]> = built
                .into_iter()
                .map(|g| CachedGroup {
                    mesh: ctx.meshes.add(g.mesh),
                    faces: Arc::new(g.faces),
                    group: g.group,
                })
                .collect();
            bound_capacity(ctx.prim_mesh_cache);
            ctx.prim_mesh_cache
                .insert(mesh_key, mesh_key, groups.clone());
            (groups, collider)
        }
    };

    // Resolve each group's material handle through the shared cache. The
    // trailing flag is #916's: `true` when this group's material is foliage
    // and the entity drawing it should sway.
    let mut drawn: Vec<(CachedGroup, Handle<StandardMaterial>, bool)> =
        Vec::with_capacity(groups.len());
    for group in groups.iter() {
        let settings = plan
            .groups
            .get(group.group)
            .and_then(|g| group_material(kind, g))
            .unwrap_or(parts.material);
        let sways = crate::wind::sways(&settings.texture);
        let material_key = settings_fingerprint(settings);
        let handle = match get_and_touch(
            ctx.prim_material_cache,
            ctx.prim_material_touched,
            material_key,
        ) {
            Some(handle) => handle,
            None => {
                let handle = spawn_procedural_material(ctx, settings);
                bound_capacity(ctx.prim_material_cache);
                ctx.prim_material_cache
                    .insert(material_key, material_key, handle.clone());
                handle
            }
        };
        drawn.push((group.clone(), handle, sways));
    }

    // One group: the prim is a single mesh on a single entity, unchanged.
    // Several: a transform-only root carrying the prim's identity (markers,
    // collider, children, audio) with one render child per material — the
    // same shape the Shape grammar spawns its terminals in.
    let single = drawn.len() == 1;
    let mut cmd = ctx.commands.spawn(transform);
    if single {
        let (group, material, sways) = &drawn[0];
        cmd.insert((
            Mesh3d(group.mesh.clone()),
            MeshMaterial3d(material.clone()),
            PrimFaceGroup {
                faces: group.faces.clone(),
            },
        ));
        // Ground-cover cards (#916). A prim's origin is its own centre
        // rather than its base — a standing card is a `Plane` rotated
        // upright about its middle — which is what `WindSway::Card`'s
        // height bias accounts for.
        if *sways {
            cmd.insert(crate::wind::WindSway::Card);
        }
    } else {
        // `Mesh3d` is what normally brings `Visibility` in as a required
        // component. A split root has no mesh of its own, so it must carry
        // one explicitly or visibility would not propagate — to its render
        // children *or* to the generator's own child nodes hanging off it.
        cmd.insert(Visibility::default());
    }
    if !ctx.avatar_mode {
        // The unit marker (not just RoomEntity) is what lets the
        // incremental compiler reclaim this prim even after the gizmo
        // detaches it from its anchor hierarchy.
        cmd.insert((RoomEntity, PlacementUnit(ctx.placement_index)));
    }
    if let Some(collider) = collider {
        cmd.insert(collider);
    }
    let root = cmd.id();

    if !single {
        // NB: no `RoomEntity` / `PlacementUnit` on the render children — the
        // same rule the Shape and L-system spawners follow. The root carries
        // them, recursive despawn from it covers the children, and
        // double-marking makes the flat unit sweep try to despawn entities
        // the anchor sweep already took ("entity despawned" warnings on
        // every rebuild).
        //
        // Each child is a real ECS entity, so it is charged against the
        // room-wide budget: a scatter over a three-material prim costs three
        // entities per point, and the per-node accounting in
        // `spawn_generator` only knows about the root.
        let children: Vec<Entity> = drawn
            .iter()
            .map(|(group, material, sways)| {
                *ctx.entities_spawned = ctx.entities_spawned.saturating_add(1);
                let mut child = ctx.commands.spawn((
                    Mesh3d(group.mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::IDENTITY,
                    PrimFaceGroup {
                        faces: group.faces.clone(),
                    },
                ));
                // A split prim's foliage face sways on its own (#916) — the
                // render children are `Transform::IDENTITY` under the prim
                // root, so they share its origin and its profile.
                if *sways {
                    child.insert(crate::wind::WindSway::Card);
                }
                child.id()
            })
            .collect();
        ctx.commands.entity(root).add_children(&children);
    }
    root
}
