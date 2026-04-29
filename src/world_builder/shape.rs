//! CGA Shape Grammar generator pipeline: geometry + material caches, stable
//! content hashes that invalidate them, the `build_shape_geometry` worker,
//! and the `spawn_shape_entity` dispatcher used by the room compiler.
//!
//! Shape grammars are the architecture-shaped sibling of the L-system
//! generator: instead of a stack-based turtle, the upstream
//! [`symbios_shape::Interpreter`] expands a queue of named rules into a flat
//! list of [`Terminal`] panels carrying a face-profiled cuboid scope. We bake
//! one unit-sized procedural mesh per `(profile, size)` pair and cache the
//! resulting per-terminal spawn list, so a `Placement::Scatter` with
//! `count = 100_000` re-uses the same baked terminals across every cell
//! instead of re-deriving the grammar 100 000 times on the main thread.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use bevy::prelude::*;
use bevy_symbios_shape::{mesh::build_profiled_mesh, transform::scope_to_transform};
use symbios_shape::grammar::parse_rule;
use symbios_shape::{FaceProfile, Interpreter, Quat as SQuat, Scope, Vec3 as SVec3};

use crate::pds::{Fp3, GeneratorKind, SovereignMaterialSettings};

use super::RoomEntity;
use super::compile::{SpawnCtx, budget_exceeded};
use super::material::spawn_procedural_material;

/// One cached shape-slot material: the content hash of the settings that
/// built it, plus the resulting PBR handle.
pub(super) struct CachedShapeMaterial {
    pub settings_hash: u64,
    pub handle: Handle<StandardMaterial>,
}

/// Persistent cross-compile cache for shape generator `StandardMaterial` handles.
///
/// Mirrors [`super::lsystem::LSystemMaterialCache`] — a `Placement::Scatter`
/// with `count=100` over a Shape generator would otherwise allocate 100 fresh
/// `StandardMaterial`s and enqueue 100 identical foliage texture tasks for
/// each `Mat("...")` slot. The cache keys on `(generator_ref, slot_name)` and
/// reuses the handle whenever the content hash of `SovereignMaterialSettings`
/// is identical.
///
/// Entries for `(generator_ref, slot)` pairs not touched during a compile
/// pass are dropped at the end of that pass so stale generators stop
/// pinning their handles in `Assets<StandardMaterial>`.
#[derive(Resource, Default)]
pub struct ShapeMaterialCache {
    pub(super) entries: HashMap<(String, String), CachedShapeMaterial>,
}

/// One pre-baked terminal: the world-relative transform produced by
/// `scope_to_transform`, the unit-sized procedural mesh handle (shared across
/// all terminals with the same `(profile, size)` triple), and the optional
/// material name emitted by `Mat("...")` in the grammar.
#[derive(Clone)]
pub(super) struct ShapeInstance {
    pub transform: Transform,
    pub mesh: Handle<Mesh>,
    pub material_id: Option<String>,
}

/// Cached geometry for a single shape generator: the fingerprint of the
/// geometry-affecting settings that produced it, and the per-terminal spawn
/// list. Materials are orthogonal — the per-instance `material_id` is
/// resolved against [`ShapeMaterialCache`] at spawn time.
pub(super) struct CachedShapeGeometry {
    pub geometry_hash: u64,
    pub instances: Vec<ShapeInstance>,
}

/// Persistent cross-compile cache for shape grammar geometry.
///
/// Without this, a scatter placement with `count = 1000` referencing a
/// shape generator would re-parse every grammar line, re-seed the
/// interpreter, re-walk the derivation queue, and re-upload one fresh
/// `Handle<Mesh>` per terminal per scatter point on the main thread.
/// Because every scattered instance of the same generator shares an
/// identical model (only the parent transform varies), we derive,
/// interpret, and bake meshes **once** per `(generator_ref, geometry_hash)`
/// pair and reuse the resulting per-terminal handles across every spawn.
///
/// Entries for `generator_ref`s not touched during a compile pass are dropped
/// at the end of that pass so stale meshes don't keep pinning `Assets<Mesh>`.
#[derive(Resource, Default)]
pub struct ShapeMeshCache {
    pub(super) entries: HashMap<String, CachedShapeGeometry>,
}

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
    const FP_SCALE: f32 = 10_000.0;
    let fp = |v: f32| (v * FP_SCALE).round() as i32;
    let mut h = DefaultHasher::new();
    grammar_source.hash(&mut h);
    root_rule.hash(&mut h);
    seed.hash(&mut h);
    fp(footprint.0[0]).hash(&mut h);
    fp(footprint.0[1]).hash(&mut h);
    fp(footprint.0[2]).hash(&mut h);
    h.finish()
}

/// Stable content hash of a `SovereignMaterialSettings` — bytes of its
/// canonical JSON serialisation. Identical to the L-system fingerprint
/// helper so the two caches can co-exist with the same eviction strategy.
fn material_fingerprint(settings: &SovereignMaterialSettings) -> u64 {
    let mut hasher = DefaultHasher::new();
    match serde_json::to_vec(settings) {
        Ok(bytes) => bytes.hash(&mut hasher),
        Err(_) => {
            0xDEAD_BEEF_u64.hash(&mut hasher);
            (settings as *const SovereignMaterialSettings as usize).hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Mesh-bucket key. The procedural mesh is unit-sized and centered at the
/// origin (the per-terminal `Transform::scale` from `scope_to_transform`
/// stretches it to the scope), but its UVs encode world-space tiling — so
/// two terminals with the same profile but different sizes need different
/// mesh assets. Profile and size triple together compose the cache key,
/// using bit-exact `u32::to_bits` of each `f32` coordinate (the grammar
/// always emits the same f32 values for the same input, so equality here
/// is correct).
#[derive(Clone, PartialEq, Eq, Hash)]
enum ProfileKey {
    Rectangle,
    Taper(u32),
    Triangle(u32),
    Trapezoid(u32, u32),
    Polygon(Vec<(u32, u32)>),
}

impl ProfileKey {
    fn from_profile(profile: &FaceProfile) -> Self {
        match profile {
            FaceProfile::Rectangle => Self::Rectangle,
            FaceProfile::Taper(t) => Self::Taper((*t as f32).to_bits()),
            FaceProfile::Triangle { peak_offset } => {
                Self::Triangle((*peak_offset as f32).to_bits())
            }
            FaceProfile::Trapezoid {
                top_width,
                offset_x,
            } => Self::Trapezoid((*top_width as f32).to_bits(), (*offset_x as f32).to_bits()),
            FaceProfile::Polygon(pts) => Self::Polygon(
                pts.iter()
                    .map(|p| ((p.x as f32).to_bits(), (p.y as f32).to_bits()))
                    .collect(),
            ),
        }
    }
}

/// Parse the multi-line grammar source line-by-line, populate the
/// interpreter, derive the model from the supplied footprint, and return
/// a flat list of [`ShapeInstance`]s ready to spawn. `None` on parse / derive
/// failure or empty output so the caller can skip the spawn.
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
) -> Option<Vec<ShapeInstance>> {
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
                    warn!(
                        "Shape `{}` rule `{}` rejected: {}",
                        generator_ref, rule.name, e
                    );
                    return None;
                }
                rule_count += 1;
            }
            Err(e) => {
                warn!("Shape `{}` line {}: {}", generator_ref, i + 1, e);
                return None;
            }
        }
    }

    if rule_count == 0 {
        return None;
    }
    if !interpreter.has_rule(root_rule) {
        warn!(
            "Shape `{}` root rule `{}` not defined in grammar",
            generator_ref, root_rule
        );
        return None;
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
            warn!("Shape `{}` derivation error: {}", generator_ref, e);
            return None;
        }
    };

    if model.terminals.is_empty() {
        return None;
    }

    // Dedupe meshes within this build pass: every terminal sharing the same
    // (profile, size) triple gets the same `Handle<Mesh>` so a 1000-window
    // facade allocates one window mesh, not 1000.
    let mut mesh_handles: HashMap<(ProfileKey, u32, u32, u32), Handle<Mesh>> = HashMap::new();
    let mut instances = Vec::with_capacity(model.terminals.len());
    for terminal in &model.terminals {
        let transform = scope_to_transform(&terminal.scope);
        let size = Vec3::new(
            terminal.scope.size.x as f32,
            terminal.scope.size.y as f32,
            terminal.scope.size.z as f32,
        );
        let key = (
            ProfileKey::from_profile(&terminal.face_profile),
            size.x.to_bits(),
            size.y.to_bits(),
            size.z.to_bits(),
        );
        let mesh = mesh_handles
            .entry(key)
            .or_insert_with(|| meshes.add(build_profiled_mesh(&terminal.face_profile, size, false)))
            .clone();
        instances.push(ShapeInstance {
            transform,
            mesh,
            material_id: terminal.material.clone(),
        });
    }

    Some(instances)
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
            let hash = material_fingerprint(settings);
            ctx.shape_material_touched.insert(key.clone());
            match ctx.shape_material_cache.entries.get(&key) {
                Some(cached) if cached.settings_hash == hash => cached.handle.clone(),
                _ => {
                    let handle = spawn_procedural_material(ctx, settings);
                    ctx.shape_material_cache.entries.insert(
                        key,
                        CachedShapeMaterial {
                            settings_hash: hash,
                            handle: handle.clone(),
                        },
                    );
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
            match ctx.shape_material_cache.entries.get(&key) {
                Some(cached) if cached.settings_hash == FALLBACK_SENTINEL_HASH => {
                    cached.handle.clone()
                }
                _ => {
                    let h = ctx.std_materials.add(StandardMaterial::default());
                    ctx.shape_material_cache.entries.insert(
                        key,
                        CachedShapeMaterial {
                            settings_hash: FALLBACK_SENTINEL_HASH,
                            handle: h.clone(),
                        },
                    );
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
    let cached = match ctx.shape_mesh_cache.entries.get(generator_ref) {
        Some(c) if c.geometry_hash == geometry_hash => Some(c.instances.clone()),
        _ => None,
    };

    let instances = match cached {
        Some(i) => i,
        None => {
            let Some(instances) = build_shape_geometry(
                grammar_source,
                root_rule,
                *footprint,
                *seed,
                generator_ref,
                ctx.meshes,
            ) else {
                // Grammar rejected, root rule missing, or empty model —
                // evict any stale entry so a later edit that fixes the
                // grammar triggers a rebuild instead of reusing a stale
                // success result.
                ctx.shape_mesh_cache.entries.remove(generator_ref);
                return None;
            };
            ctx.shape_mesh_cache.entries.insert(
                generator_ref.to_string(),
                CachedShapeGeometry {
                    geometry_hash,
                    instances: instances.clone(),
                },
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
            .spawn((transform, Visibility::default(), RoomEntity))
            .id()
    };

    // Each terminal is a real ECS entity, so it contributes to the
    // room-wide spawn budget. Without this, a record can put a high-count
    // `Scatter` over a Shape grammar that derives thousands of terminals
    // — the per-generator-node accounting in `spawn_generator` would only
    // charge one per scatter point regardless of terminal count, blowing
    // past `MAX_ROOM_ENTITIES` and OOMing the ECS.
    for instance in &instances {
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
