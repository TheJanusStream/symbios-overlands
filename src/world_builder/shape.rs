//! CGA Shape Grammar generator pipeline: geometry + material caches, stable
//! content hashes that invalidate them, the `build_shape_geometry` worker,
//! and the `spawn_shape_entity` dispatcher used by the room compiler.
//!
//! Shape grammars are the architecture-shaped sibling of the L-system
//! generator: instead of a stack-based turtle, the upstream
//! [`symbios_shape::Interpreter`] expands a queue of named rules into a flat
//! list of [`Terminal`](symbios_shape::Terminal) panels carrying a face-profiled cuboid scope. We bake
//! one unit-sized procedural mesh per `(profile, size)` pair and cache the
//! resulting per-terminal spawn list, so a `Placement::Scatter` with
//! `count = 100_000` re-uses the same baked terminals across every cell
//! instead of re-deriving the grammar 100 000 times on the main thread.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use bevy_symbios_shape::cache::{
    MeshCacheKey, ProfileKey, ShapeMeshCache as UpstreamShapeMeshCache,
};
use bevy_symbios_shape::{mesh::build_profiled_mesh, transform::scope_to_transform};
use symbios_shape::grammar::parse_rule;
use symbios_shape::{Interpreter, Quat as SQuat, Scope, Vec3 as SVec3};

use crate::pds::{Fp3, GeneratorKind, SovereignMaterialSettings};

use super::RoomEntity;
use super::compile::{SpawnCtx, budget_exceeded};
use super::generator_cache::{GeneratorCache, GeometryHasher, settings_fingerprint};
use super::material::spawn_procedural_material;

/// Persistent cross-compile cache for shape generator `StandardMaterial` handles.
///
/// Mirrors [`super::lsystem::LSystemMaterialCache`] — a `Placement::Scatter`
/// with `count=100` over a Shape generator would otherwise allocate 100 fresh
/// `StandardMaterial`s and enqueue 100 identical foliage texture tasks for
/// each `Mat("...")` slot. The cache keys on `(generator_ref, slot_name)` and
/// reuses the handle whenever [`settings_fingerprint`] of the
/// `SovereignMaterialSettings` is identical. GC + logout semantics come with
/// [`GeneratorCache`].
pub type ShapeMaterialCache = GeneratorCache<(String, String), Handle<StandardMaterial>>;

/// One pre-baked terminal: the world-relative transform produced by
/// `scope_to_transform`, the unit-sized procedural mesh handle (shared across
/// all terminals with the same `(profile, size)` triple), and the optional
/// material name emitted by `Mat("...")` in the grammar.
#[derive(Clone, Debug)]
pub struct ShapeInstance {
    pub transform: Transform,
    pub mesh: Handle<Mesh>,
    pub material_id: Option<String>,
}

/// Persistent cross-compile cache for shape grammar geometry — the
/// per-terminal spawn list. Materials are orthogonal: the per-instance
/// `material_id` is resolved against [`ShapeMaterialCache`] at spawn time.
///
/// Without this, a scatter placement with `count = 1000` referencing a
/// shape generator would re-parse every grammar line, re-seed the
/// interpreter, re-walk the derivation queue, and re-upload one fresh
/// `Handle<Mesh>` per terminal per scatter point on the main thread.
/// Because every scattered instance of the same generator shares an
/// identical model (only the parent transform varies), we derive,
/// interpret, and bake meshes **once** per `(generator_ref, geometry_hash)`
/// pair and reuse the resulting per-terminal handles across every spawn.
/// The list is an `Arc` so a cache HIT hands out an O(1) refcount bump
/// instead of deep-cloning the `Vec` (+ its per-instance `material_id`
/// Strings) on every scatter sample / grid cell (#636).
pub type ShapeMeshCache = GeneratorCache<String, Arc<[ShapeInstance]>>;

/// Stable content hash of the geometry-affecting fields of a
/// `GeneratorKind::Shape`. Material settings are deliberately excluded
/// because those are applied per-spawn on top of a shared mesh list (see
/// [`ShapeMaterialCache`]). Each `Fp3` axis is hashed via its fixed-point
/// wire form so NaN/denormal floats can't destabilise the key across
/// compile passes.
fn shape_geometry_fingerprint(
    grammar_source: &str,
    root_rule: &str,
    footprint: Fp3,
    seed: u64,
) -> u64 {
    let mut h = GeometryHasher::new();
    h.field(grammar_source);
    h.field(root_rule);
    h.field(seed);
    h.fp(footprint.0[0]);
    h.fp(footprint.0[1]);
    h.fp(footprint.0[2]);
    h.finish()
}

// Mesh dedup keys (`MeshCacheKey`, `ProfileKey`) and the cross-spawn
// [`UpstreamShapeMeshCache`] resource live in `bevy_symbios_shape::cache` —
// importing them at the top of this module keeps mesh handle reuse
// consistent across every consumer of the shape grammar.

/// Parse the multi-line grammar source line-by-line, populate the
/// interpreter, derive the model from the supplied footprint, and return
/// a flat list of [`ShapeInstance`]s ready to spawn. `Err` (the grammar
/// error, line-numbered where the parser knows one) on parse / derive
/// failure or empty output so the caller can skip the spawn and surface
/// the message in the editor (#829); every error is also `warn!`-logged.
///
/// Mirrors the line-based authoring convention used by sibling editors
/// (`symbios-ground-lab`): one rule per line, blank lines and `// …` lines
/// ignored. Per-line parse errors are logged and the whole rebuild aborts
/// — partial rule tables produce confusing terminal layouts that look like
/// silent bugs in the grammar.
fn build_shape_geometry(
    grammar_source: &str,
    root_rule: &str,
    footprint: Fp3,
    seed: u64,
    generator_ref: &str,
    meshes: &mut Assets<Mesh>,
    upstream_cache: &mut UpstreamShapeMeshCache,
) -> Result<Vec<ShapeInstance>, String> {
    let mut interpreter = Interpreter::new();
    interpreter.seed = seed;

    let mut rule_count: u32 = 0;
    for (i, raw) in grammar_source.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        match parse_rule(line) {
            Ok(rule) => {
                if let Err(e) = interpreter.add_weighted_rules(&rule.name, rule.variants) {
                    let msg = format!("rule `{}` rejected: {}", rule.name, e);
                    warn!("Shape `{}` {}", generator_ref, msg);
                    return Err(msg);
                }
                rule_count += 1;
            }
            Err(e) => {
                let msg = format!("line {}: {}", i + 1, e);
                warn!("Shape `{}` {}", generator_ref, msg);
                return Err(msg);
            }
        }
    }

    if rule_count == 0 {
        return Err("grammar has no rules".to_string());
    }
    if !interpreter.has_rule(root_rule) {
        let msg = format!("root rule `{}` not defined in grammar", root_rule);
        warn!("Shape `{}` {}", generator_ref, msg);
        return Err(msg);
    }

    let root_scope = Scope::new(
        SVec3::ZERO,
        SQuat::IDENTITY,
        SVec3::new(
            footprint.0[0] as f64,
            footprint.0[1] as f64,
            footprint.0[2] as f64,
        ),
    );

    let model = match interpreter.derive(root_scope, root_rule) {
        Ok(m) => m,
        Err(e) => {
            let msg = format!("derivation error: {}", e);
            warn!("Shape `{}` {}", generator_ref, msg);
            return Err(msg);
        }
    };

    if model.terminals.is_empty() {
        return Err("grammar produced no geometry (no terminal shapes)".to_string());
    }

    // Dedupe meshes through the upstream `ShapeMeshCache` resource so two
    // different generators that produce terminals with the same
    // `(profile, size)` triple share the same `Handle<Mesh>` — a 1000-window
    // facade allocates one window mesh, not 1000, AND a second building
    // generator with the same window pattern reuses the existing handle
    // instead of uploading a duplicate.
    let mut instances = Vec::with_capacity(model.terminals.len());
    for terminal in &model.terminals {
        let transform = scope_to_transform(&terminal.scope);
        let size = Vec3::new(
            terminal.scope.size.x as f32,
            terminal.scope.size.y as f32,
            terminal.scope.size.z as f32,
        );
        let key = MeshCacheKey {
            profile: ProfileKey::from_profile(&terminal.face_profile),
            size_x_bits: size.x.to_bits(),
            size_y_bits: size.y.to_bits(),
            size_z_bits: size.z.to_bits(),
            stretch_uvs: false,
        };
        let mesh = upstream_cache.get_or_insert_with(key, || {
            meshes.add(build_profiled_mesh(&terminal.face_profile, size, false))
        });
        instances.push(ShapeInstance {
            transform,
            mesh,
            material_id: terminal.material.as_ref().map(|m| m.id.clone()),
        });
    }

    Ok(instances)
}

/// Resolve (and cache) a [`StandardMaterial`] handle for a given material
/// slot name. A `None` slot or a slot that has no entry in the generator's
/// `materials` map both fall through to a shared default handle keyed by
/// the sentinel `""` slot name, so 1000 unmapped terminals share one
/// fallback material instead of allocating 1000.
fn resolve_material_handle(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    generator_ref: &str,
    materials: &HashMap<String, SovereignMaterialSettings>,
    slot_name: Option<&str>,
) -> Handle<StandardMaterial> {
    const FALLBACK_SENTINEL_HASH: u64 = u64::MAX;
    let lookup = slot_name.and_then(|n| materials.get(n).map(|s| (n.to_string(), s)));
    match lookup {
        Some((name, settings)) => {
            let key = (generator_ref.to_string(), name);
            let hash = settings_fingerprint(settings);
            ctx.shape_material_touched.insert(key.clone());
            match ctx.shape_material_cache.get_if(&key, hash) {
                Some(handle) => handle,
                None => {
                    let handle = spawn_procedural_material(ctx, settings);
                    ctx.shape_material_cache.insert(key, hash, handle.clone());
                    handle
                }
            }
        }
        None => {
            // Use the empty slot name as a stable cache key for the shared
            // fallback. Without this, every unmapped terminal in a 100k
            // scatter allocates its own `StandardMaterial::default()`.
            let key = (generator_ref.to_string(), String::new());
            ctx.shape_material_touched.insert(key.clone());
            match ctx
                .shape_material_cache
                .get_if(&key, FALLBACK_SENTINEL_HASH)
            {
                Some(handle) => handle,
                None => {
                    let h = ctx.std_materials.add(StandardMaterial::default());
                    ctx.shape_material_cache
                        .insert(key, FALLBACK_SENTINEL_HASH, h.clone());
                    h
                }
            }
        }
    }
}

pub(super) fn spawn_shape_entity(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    kind: &GeneratorKind,
    generator_ref: &str,
    transform: Transform,
) -> Option<Entity> {
    let GeneratorKind::Shape {
        grammar_source,
        root_rule,
        footprint,
        seed,
        materials,
    } = kind
    else {
        return None;
    };

    // Reuse cached geometry when the geometry-affecting settings are
    // unchanged. A scatter placement with count=1000 would otherwise
    // re-derive the grammar and re-bake every terminal mesh on every spawn.
    ctx.shape_mesh_touched.insert(generator_ref.to_string());
    let geometry_hash = shape_geometry_fingerprint(grammar_source, root_rule, *footprint, *seed);
    let cached = ctx.shape_mesh_cache.get_if(generator_ref, geometry_hash);

    let instances = match cached {
        // Cache hit = this exact grammar compiled cleanly earlier in the
        // session — still record Ok so a fixed-then-unchanged grammar
        // doesn't leave a stale error in the editor (#829).
        Some(i) => {
            ctx.record_grammar_status(generator_ref, None);
            i
        }
        None => {
            let built = match build_shape_geometry(
                grammar_source,
                root_rule,
                *footprint,
                *seed,
                generator_ref,
                ctx.meshes,
                ctx.upstream_shape_mesh_cache,
            ) {
                Ok(built) => built,
                Err(message) => {
                    // Grammar rejected, root rule missing, or empty model —
                    // evict any stale entry so a later edit that fixes the
                    // grammar triggers a rebuild instead of reusing a stale
                    // success result, and surface the error in the editor's
                    // grammar forge (#829).
                    ctx.shape_mesh_cache.remove(generator_ref);
                    ctx.record_grammar_status(generator_ref, Some(message));
                    return None;
                }
            };
            ctx.record_grammar_status(generator_ref, None);
            let instances: Arc<[ShapeInstance]> = built.into();
            ctx.shape_mesh_cache.insert(
                generator_ref.to_string(),
                geometry_hash,
                instances.clone(),
            );
            instances
        }
    };

    // Parent every terminal under a single transform so the placement's
    // rotation/position anchors the whole building as a unit. Avatar
    // mode skips the `RoomEntity` tag for the same reason as the
    // lsystem spawner — see `world_builder::lsystem::spawn_lsystem_entity`.
    let parent = if ctx.avatar_mode {
        ctx.commands.spawn((transform, Visibility::default())).id()
    } else {
        ctx.commands
            .spawn((
                transform,
                Visibility::default(),
                RoomEntity,
                super::PlacementUnit(ctx.placement_index),
            ))
            .id()
    };

    // Each terminal is a real ECS entity, so it contributes to the
    // room-wide spawn budget. Without this, a record can put a high-count
    // `Scatter` over a Shape grammar that derives thousands of terminals
    // — the per-generator-node accounting in `spawn_generator` would only
    // charge one per scatter point regardless of terminal count, blowing
    // past `MAX_ROOM_ENTITIES` and OOMing the ECS.
    for instance in instances.iter() {
        if budget_exceeded(*ctx.entities_spawned, ctx.budget_warned) {
            break;
        }
        let material = resolve_material_handle(
            ctx,
            generator_ref,
            materials,
            instance.material_id.as_deref(),
        );
        // NB: no `RoomEntity` marker on child meshes — see the lsystem
        // spawner for the rationale (recursive despawn from the parent
        // covers them; double-marking cascades into "entity despawned"
        // warnings during room rebuilds).
        let child = ctx
            .commands
            .spawn((
                Mesh3d(instance.mesh.clone()),
                MeshMaterial3d(material),
                instance.transform,
            ))
            .id();
        ctx.commands.entity(parent).add_child(child);
        *ctx.entities_spawned = ctx.entities_spawned.saturating_add(1);
    }

    Some(parent)
}

#[cfg(test)]
mod grammar_error_tests {
    use super::*;

    /// #829: shape-grammar failures surface as `Err` with the message the
    /// editor forge renders — line-numbered parse errors, a named missing
    /// root rule, and the no-rules case.
    #[test]
    fn shape_grammar_errors_surface_as_results() {
        // A bare asset store suffices — the error paths never reach the
        // mesh-baking stage that would populate it.
        let mut meshes = Assets::<Mesh>::default();
        let mut cache = UpstreamShapeMeshCache::default();

        let err = build_shape_geometry(
            "",
            "Root",
            Fp3([8.0, 8.0, 8.0]),
            1,
            "test_gen",
            &mut meshes,
            &mut cache,
        )
        .expect_err("empty grammar must be rejected");
        assert!(err.contains("no rules"), "{err}");

        let err = build_shape_geometry(
            "House --> Extrude(10) Body",
            "Root",
            Fp3([8.0, 8.0, 8.0]),
            1,
            "test_gen",
            &mut meshes,
            &mut cache,
        )
        .expect_err("missing root rule must be rejected");
        assert!(err.contains("root rule `Root`"), "{err}");

        let err = build_shape_geometry(
            "%%% not a rule at all",
            "Root",
            Fp3([8.0, 8.0, 8.0]),
            1,
            "test_gen",
            &mut meshes,
            &mut cache,
        )
        .expect_err("parse failure must be rejected");
        assert!(err.contains("line 1"), "{err}");
    }
}
