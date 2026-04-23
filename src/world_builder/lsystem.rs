//! L-system generator pipeline: geometry + material caches, stable content
//! hashes that invalidate them, the `build_lsystem_geometry` worker, and the
//! `spawn_lsystem_entity` dispatcher used by the room compiler.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use bevy::prelude::*;
use bevy_symbios::LSystemMeshBuilder;
use symbios::System;
use symbios_turtle_3d::{SkeletonProp, TurtleConfig, TurtleInterpreter};

use crate::pds::{Fp, Fp3, Generator, PropMeshType, SovereignMaterialSettings};

use super::compile::SpawnCtx;
use super::material::spawn_procedural_material;
use super::{RoomEntity, apply_traits};

/// One cached L-system slot material: the content hash of the settings that
/// built it, plus the resulting PBR handle.
pub(super) struct CachedLSystemMaterial {
    pub settings_hash: u64,
    pub handle: Handle<StandardMaterial>,
}

/// Persistent cross-compile cache for L-system `StandardMaterial` handles.
///
/// Without this, every `RoomRecord` change rebuilds every generator's
/// material — enqueuing fresh foliage texture tasks for configs that haven't
/// moved. Keyed by `(generator_ref, slot_id)` and invalidated by hashing the
/// canonical (fixed-point) serialisation of `SovereignMaterialSettings`, so
/// a record edit that touches *only* (say) the scatter count re-uses last
/// pass's baked textures instead of re-baking them.
///
/// Entries for `(generator_ref, slot)` pairs not touched during a compile
/// pass are dropped at the end of that pass so stale generators stop
/// pinning their handles in `Assets<StandardMaterial>`.
#[derive(Resource, Default)]
pub struct LSystemMaterialCache {
    pub(super) entries: HashMap<(String, u8), CachedLSystemMaterial>,
}

/// Cached geometry for a single L-system generator: the fingerprint of the
/// geometry-affecting settings that produced it, the shared per-material mesh
/// handles, and the skeleton's prop list. Props are stored raw because the
/// prop→mesh mapping and prop scale are resolved per-spawn against the
/// current generator settings.
pub(super) struct CachedLSystemGeometry {
    pub geometry_hash: u64,
    pub mesh_buckets: Vec<(u8, Handle<Mesh>)>,
    pub props: Vec<SkeletonProp>,
}

/// Persistent cross-compile cache for L-system mesh geometry.
///
/// A `Placement::Scatter` with `count = 100_000` referencing an LSystem
/// generator would otherwise re-parse the grammar, re-derive the state,
/// re-interpret the turtle and re-upload a fresh `Handle<Mesh>` per scatter
/// point on the main thread. Because all scattered instances of the same
/// generator share identical geometry (only the parent transform varies),
/// we derive, interpret and mesh **once** per `(generator_ref, geometry_hash)`
/// pair and reuse the resulting `Handle<Mesh>` across every spawn.
///
/// Keyed by `generator_ref` and invalidated by hashing the geometry-relevant
/// fields (source, finalization, iterations, seed, angle/step/width/
/// elasticity, tropism, mesh resolution) in their fixed-point wire form.
/// Material settings are orthogonal — those live in `LSystemMaterialCache`
/// so a pure colour edit re-uses the cached mesh handles as-is.
///
/// Entries for `generator_ref`s not touched during a compile pass are dropped
/// at the end of that pass so stale meshes don't keep pinning `Assets<Mesh>`.
#[derive(Resource, Default)]
pub struct LSystemMeshCache {
    pub(super) entries: HashMap<String, CachedLSystemGeometry>,
}

pub(super) fn settings_fingerprint(settings: &SovereignMaterialSettings) -> u64 {
    let mut hasher = DefaultHasher::new();
    match serde_json::to_vec(settings) {
        Ok(bytes) => bytes.hash(&mut hasher),
        // Serialisation of a plain struct of scalars cannot fail in
        // practice; if it somehow does, fall back to a distinct sentinel
        // so the match arm below treats every lookup as a miss (forcing a
        // rebuild) rather than collapsing all failures onto the same key.
        Err(_) => {
            0xDEAD_BEEF_u64.hash(&mut hasher);
            (settings as *const SovereignMaterialSettings as usize).hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Stable content hash of the geometry-affecting fields of a `Generator::LSystem`.
/// Material / prop-mapping settings are deliberately excluded because those
/// are applied per-spawn on top of a shared mesh (see `LSystemMeshCache`).
/// Each `Fp` field is hashed via its fixed-point wire form so NaN/denormal
/// floats can't destabilise the key across compile passes.
#[allow(clippy::too_many_arguments)]
pub(super) fn lsystem_geometry_fingerprint(
    source_code: &str,
    finalization_code: &str,
    iterations: u32,
    seed: u64,
    angle: Fp,
    step: Fp,
    width: Fp,
    elasticity: Fp,
    tropism: Option<Fp3>,
    mesh_resolution: u32,
) -> u64 {
    const FP_SCALE: f32 = 10_000.0;
    let fp = |v: f32| (v * FP_SCALE).round() as i32;
    let mut h = DefaultHasher::new();
    source_code.hash(&mut h);
    finalization_code.hash(&mut h);
    iterations.hash(&mut h);
    seed.hash(&mut h);
    fp(angle.0).hash(&mut h);
    fp(step.0).hash(&mut h);
    fp(width.0).hash(&mut h);
    fp(elasticity.0).hash(&mut h);
    match tropism {
        Some(t) => {
            1u8.hash(&mut h);
            fp(t.0[0]).hash(&mut h);
            fp(t.0[1]).hash(&mut h);
            fp(t.0[2]).hash(&mut h);
        }
        None => 0u8.hash(&mut h),
    }
    mesh_resolution.hash(&mut h);
    h.finish()
}

/// Pair of raw mesh buckets (keyed by material id) and the skeleton's prop
/// list — the cacheable output of an L-system build pass.
type LSystemGeometryBuild = (Vec<(u8, Mesh)>, Vec<SkeletonProp>);

/// Parse, derive, interpret and mesh an L-system generator. Returns the raw
/// mesh buckets keyed by material id, plus the skeleton's prop list. `None`
/// on grammar errors or empty state so the caller can skip the spawn.
///
/// Split out of `spawn_lsystem_entity` so `LSystemMeshCache` can invoke the
/// expensive pipeline at most once per `(generator_ref, geometry_hash)` pair.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_lsystem_geometry(
    source_code: &str,
    finalization_code: &str,
    iterations: u32,
    seed: u64,
    angle: Fp,
    step: Fp,
    width: Fp,
    elasticity: Fp,
    tropism: Option<Fp3>,
    mesh_resolution: u32,
    generator_ref: &str,
) -> Option<LSystemGeometryBuild> {
    let mut sys = System::new();
    sys.set_seed(seed);

    for (i, line) in source_code.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if trimmed.starts_with('#') {
            if let Err(e) = sys.add_directive(trimmed) {
                warn!("L-system `{}` line {}: {}", generator_ref, i + 1, e);
                return None;
            }
            continue;
        }
        if let Some(axiom) = trimmed.strip_prefix("omega:") {
            if let Err(e) = sys.set_axiom(axiom.trim()) {
                warn!("L-system `{}` axiom error: {}", generator_ref, e);
                return None;
            }
            continue;
        }
        if let Err(e) = sys.add_rule(trimmed) {
            warn!("L-system `{}` rule error: {}", generator_ref, e);
            return None;
        }
    }

    // Cap the derived state length so a malicious record can't weaponise a
    // productive grammar (e.g. an axiom expanding >10× per step) into a
    // multi-gigabyte symbol buffer that locks the main thread inside the
    // turtle interpreter. 2^20 symbols is well past the largest legitimate
    // L-system our shipping presets produce.
    const MAX_LSYSTEM_STATE_LEN: usize = 1 << 20;
    // Force the hard cap into symbios's own back-buffer so the derivation
    // engine returns `CapacityOverflow` before the single-step expansion
    // can allocate past our budget. Without this, a rule like
    // `A -> [16 KB of junk]` applied to a 1M-symbol state could try to
    // allocate tens of billions of symbols inside a single `derive(1)`
    // call — the post-derive length check fires too late to prevent the
    // OOM that allocation triggers.
    sys.max_capacity = MAX_LSYSTEM_STATE_LEN;
    for _ in 0..iterations {
        if let Err(e) = sys.derive(1) {
            warn!("L-system `{}` derivation error: {}", generator_ref, e);
            return None;
        }
        if sys.state.len() > MAX_LSYSTEM_STATE_LEN {
            warn!(
                "L-system `{}` state exceeded {} symbols — aborting derivation",
                generator_ref, MAX_LSYSTEM_STATE_LEN
            );
            return None;
        }
    }

    if !finalization_code.trim().is_empty() {
        sys.rules.clear();
        sys.ignored_symbols.clear();
        for (i, line) in finalization_code.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("omega:") {
                continue;
            }
            if trimmed.starts_with('#') {
                if let Err(e) = sys.add_directive(trimmed) {
                    warn!(
                        "L-system `{}` finalization line {}: {}",
                        generator_ref,
                        i + 1,
                        e
                    );
                    return None;
                }
                continue;
            }
            if let Err(e) = sys.add_rule(trimmed) {
                warn!(
                    "L-system `{}` finalization rule error: {}",
                    generator_ref, e
                );
                return None;
            }
        }
        if let Err(e) = sys.derive(1) {
            warn!(
                "L-system `{}` finalization derivation error: {}",
                generator_ref, e
            );
            return None;
        }
        if sys.state.len() > MAX_LSYSTEM_STATE_LEN {
            warn!(
                "L-system `{}` finalization exceeded {} symbols — aborting",
                generator_ref, MAX_LSYSTEM_STATE_LEN
            );
            return None;
        }
    }

    if sys.state.is_empty() {
        return None;
    }

    let turtle_config = TurtleConfig {
        default_step: step.0.max(0.001),
        default_angle: angle.0.to_radians(),
        initial_width: width.0.max(0.001),
        tropism: tropism.as_ref().map(|t| Vec3::from_array(t.0)),
        elasticity: elasticity.0,
        max_stack_depth: 1024,
    };
    let mut interpreter = TurtleInterpreter::new(turtle_config);
    interpreter.populate_standard_symbols(&sys.interner);
    let skeleton = interpreter.build_skeleton(&sys.state);

    // Each material ID produces a separate mesh bucket.
    let mesh_buckets: Vec<(u8, Mesh)> = LSystemMeshBuilder::new()
        .with_resolution(mesh_resolution.max(3))
        .build(&skeleton)
        .into_iter()
        .collect();

    Some((mesh_buckets, skeleton.props))
}

pub(super) fn spawn_lsystem_entity(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    generator: &Generator,
    generator_ref: &str,
    transform: Transform,
) -> Option<Entity> {
    let Generator::LSystem {
        source_code,
        finalization_code,
        iterations,
        seed,
        angle,
        step,
        width,
        elasticity,
        tropism,
        materials: lsys_materials,
        prop_mappings,
        prop_scale,
        mesh_resolution,
        ..
    } = generator
    else {
        return None;
    };

    // Reuse cached geometry when the geometry-affecting settings are
    // unchanged. A scatter placement with count=100_000 would otherwise
    // re-derive the grammar, re-walk the turtle and re-upload 100_000
    // `Handle<Mesh>` entries per scatter point on the main thread.
    ctx.lsystem_mesh_touched.insert(generator_ref.to_string());
    let geometry_hash = lsystem_geometry_fingerprint(
        source_code,
        finalization_code,
        *iterations,
        *seed,
        *angle,
        *step,
        *width,
        *elasticity,
        *tropism,
        *mesh_resolution,
    );
    let geometry = match ctx.lsystem_mesh_cache.entries.get(generator_ref) {
        Some(c) if c.geometry_hash == geometry_hash => {
            Some((c.mesh_buckets.clone(), c.props.clone()))
        }
        _ => None,
    };

    let (mesh_bucket_handles, props) = match geometry {
        Some(g) => g,
        None => {
            let Some((mesh_buckets_raw, skeleton_props)) = build_lsystem_geometry(
                source_code,
                finalization_code,
                *iterations,
                *seed,
                *angle,
                *step,
                *width,
                *elasticity,
                *tropism,
                *mesh_resolution,
                generator_ref,
            ) else {
                // Grammar rejected or empty state — evict any stale entry
                // so a later edit that fixes the grammar triggers a rebuild
                // instead of reusing invalid geometry.
                ctx.lsystem_mesh_cache.entries.remove(generator_ref);
                return None;
            };
            let bucket_handles: Vec<(u8, Handle<Mesh>)> = mesh_buckets_raw
                .into_iter()
                .map(|(mat_id, mesh)| (mat_id, ctx.meshes.add(mesh)))
                .collect();
            ctx.lsystem_mesh_cache.entries.insert(
                generator_ref.to_string(),
                CachedLSystemGeometry {
                    geometry_hash,
                    mesh_buckets: bucket_handles.clone(),
                    props: skeleton_props.clone(),
                },
            );
            (bucket_handles, skeleton_props)
        }
    };

    // Parent every mesh under a single transform so the placement's
    // rotation/position anchors the whole plant/shape as a unit.
    let parent = ctx
        .commands
        .spawn((transform, Visibility::default(), RoomEntity))
        .id();

    // Build material handles per slot. For foliage slots (Leaf/Twig/Bark)
    // we *also* spawn a texture-generation task so the handle receives its
    // procedural albedo/normal/ORM maps on a later frame. The palette path
    // still wins when `bevy_symbios::materials::sync_*` has already
    // resolved a shared palette slot for us — in that case we skip the
    // task, because the palette owns texture sync.
    let mut slot_handles: HashMap<u8, Handle<StandardMaterial>> = HashMap::new();
    for (&slot, settings) in lsys_materials.iter() {
        let handle = if let Some(palette) = ctx.palette
            && let Some(h) = palette.materials.get(&slot)
        {
            h.clone()
        } else {
            let key = (generator_ref.to_string(), slot);
            let hash = settings_fingerprint(settings);
            ctx.lsystem_cache_touched.insert(key.clone());
            match ctx.lsystem_material_cache.entries.get(&key) {
                Some(cached) if cached.settings_hash == hash => cached.handle.clone(),
                _ => {
                    let handle = spawn_procedural_material(ctx, settings);
                    ctx.lsystem_material_cache.entries.insert(
                        key,
                        CachedLSystemMaterial {
                            settings_hash: hash,
                            handle: handle.clone(),
                        },
                    );
                    handle
                }
            }
        };
        slot_handles.insert(slot, handle);
    }

    for (material_id, mesh_handle) in &mesh_bucket_handles {
        let material = slot_handles
            .get(material_id)
            .cloned()
            .unwrap_or_else(|| ctx.std_materials.add(StandardMaterial::default()));

        // NB: no `RoomEntity` marker on child meshes. The parent below
        // carries it, and Bevy 0.18's recursive `despawn` tears down
        // children automatically. Marking children with `RoomEntity` too
        // causes the logout / room-rebuild cleanup queries to yield both
        // parent and child, and whichever lands first cascades the
        // despawn, leaving the other as an "entity despawned" warning.
        let child = ctx
            .commands
            .spawn((
                Mesh3d(mesh_handle.clone()),
                MeshMaterial3d(material),
                Transform::IDENTITY,
            ))
            .id();
        ctx.commands.entity(parent).add_child(child);
    }

    // Spawn prop billboards/primitives. Each prop inherits its material
    // from `slot_handles`, so foliage props share the same handle as the
    // branch meshes — when the async texture task finishes, the prop picks
    // up the albedo automatically. A prop whose `prop_id` has no mapping
    // falls back to `PropMeshType::Leaf`.
    if let Some(prop_assets) = ctx.prop_assets {
        let ps = prop_scale.0.max(0.0);
        for prop in &props {
            let mesh_type = prop_mappings
                .get(&prop.prop_id)
                .copied()
                .unwrap_or(PropMeshType::Leaf);
            let Some(mesh_handle) = prop_assets.meshes.get(&mesh_type) else {
                continue;
            };
            let material = slot_handles
                .get(&prop.material_id)
                .cloned()
                .unwrap_or_else(|| ctx.std_materials.add(StandardMaterial::default()));

            let child = ctx
                .commands
                .spawn((
                    Mesh3d(mesh_handle.clone()),
                    MeshMaterial3d(material),
                    Transform {
                        translation: prop.position,
                        rotation: prop.rotation,
                        scale: prop.scale * ps,
                    },
                ))
                .id();
            ctx.commands.entity(parent).add_child(child);
        }
    }

    apply_traits(ctx.commands, parent, ctx.record, generator_ref);
    // Silence unused-binding warnings when the heightmap is unused here.
    let _ = ctx.heightmap;
    Some(parent)
}
