//! L-system generator pipeline: geometry + material caches, stable content
//! hashes that invalidate them, the `build_lsystem_geometry` worker, and the
//! `spawn_lsystem_entity` dispatcher used by the room compiler.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use bevy_symbios::LSystemMeshBuilder;
use symbios::System;
use symbios_turtle_3d::{Skeleton, TurtleConfig, TurtleInterpreter};

use crate::pds::{Fp, Fp3, GeneratorKind, PropMeshType};

use super::RoomEntity;
use super::compile::{SpawnCtx, budget_exceeded};
use super::generator_cache::{GeneratorCache, GeometryHasher, settings_fingerprint};
use super::material::spawn_procedural_material;

/// Persistent cross-compile cache for L-system `StandardMaterial` handles.
///
/// Without this, every `RoomRecord` change rebuilds every generator's
/// material — enqueuing fresh foliage texture tasks for configs that haven't
/// moved. Keyed by `(generator_ref, slot_id)` and invalidated by
/// [`settings_fingerprint`], so a record edit that touches *only* (say) the
/// scatter count re-uses last pass's baked textures instead of re-baking
/// them. GC + logout semantics come with [`GeneratorCache`].
pub type LSystemMaterialCache = GeneratorCache<(String, u16), Handle<StandardMaterial>>;

/// Cached geometry build for a single L-system generator: the shared
/// per-material mesh handles. Every prop (leaf / fruit / …) is baked into the
/// mesh bucket for its material id at build time (#812), so a spawned tree is
/// just its parent entity plus one child per bucket — no per-prop entities.
/// The handle slice is an `Arc` so a cache HIT is an O(1) refcount bump per
/// scatter sample / grid cell instead of deep-cloning the Vec (#636).
#[derive(Clone)]
pub struct LSystemGeometry {
    pub mesh_buckets: Arc<[(u16, Handle<Mesh>)]>,
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
/// Keyed by `generator_ref` and invalidated by
/// [`lsystem_geometry_fingerprint`] over the geometry-relevant fields.
/// Material settings are orthogonal — those live in `LSystemMaterialCache`
/// so a pure colour edit re-uses the cached mesh handles as-is.
pub type LSystemMeshCache = GeneratorCache<String, LSystemGeometry>;

/// Stable content hash of the geometry-affecting fields of a `GeneratorKind::LSystem`.
/// Material settings are deliberately excluded because those are applied
/// per-spawn on top of a shared mesh (see `LSystemMaterialCache`). Prop mapping
/// and prop scale, by contrast, *are* included: since #812 props are baked into
/// the mesh buckets at build time, so a change to either alters the cached
/// geometry. Each `Fp` field is hashed via its fixed-point wire form so
/// NaN/denormal floats can't destabilise the key across compile passes.
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
    prop_mappings: &HashMap<u16, PropMeshType>,
    prop_scale: Fp,
) -> u64 {
    let mut h = GeometryHasher::new();
    h.field(source_code);
    h.field(finalization_code);
    h.field(iterations);
    h.field(seed);
    h.fp(angle.0);
    h.fp(step.0);
    h.fp(width.0);
    h.fp(elasticity.0);
    match tropism {
        Some(t) => {
            h.field(1u8);
            h.fp(t.0[0]);
            h.fp(t.0[1]);
            h.fp(t.0[2]);
        }
        None => h.field(0u8),
    }
    h.field(mesh_resolution);
    // Prop mappings feed the baked geometry now. `HashMap` iteration order is
    // unstable, so hash the entries sorted by key — otherwise two identical
    // generators could produce different keys across compile passes.
    let mut mappings: Vec<(u16, PropMeshType)> =
        prop_mappings.iter().map(|(&k, &v)| (k, v)).collect();
    mappings.sort_unstable_by_key(|(k, _)| *k);
    h.field(&mappings);
    h.fp(prop_scale.0);
    h.finish()
}

/// Raw mesh buckets keyed by material id — the cacheable output of an L-system
/// build pass. Since #812 the skeleton's props are baked straight into these
/// buckets, so there is no separate prop list to carry.
type LSystemGeometryBuild = Vec<(u16, Mesh)>;

/// Fold one prop's primitive mesh (its [`PropMeshType`], posed by `transform`)
/// into `bucket`. The prop mesh is given a neutral white vertex colour so it
/// carries the `ATTRIBUTE_COLOR` the turtle mesher writes on every branch
/// bucket — [`Mesh::merge`] requires the source to cover every attribute the
/// destination has, and drops attributes the destination lacks. White is a
/// no-op multiply, so the prop renders with its material's albedo exactly as
/// the old per-prop entity did. `bucket` must already have had its tangents
/// stripped (some prop primitives carry none); the caller regenerates them
/// once after every prop is folded in.
fn bake_prop_into_bucket(bucket: &mut Mesh, mesh_type: PropMeshType, transform: Transform) {
    let mut prop = super::prop_mesh_geometry(mesh_type);
    let vertex_count = prop.count_vertices();
    prop.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        vec![[1.0f32, 1.0, 1.0, 1.0]; vertex_count],
    );
    prop.transform_by(transform);
    // Both are triangle lists sharing POSITION/NORMAL/COLOR/UV_0. An error
    // would mean a mesh-attribute mismatch bug — assert in debug, but in
    // release a rejected prop is just a missing decoration, never a crash.
    if let Err(e) = bucket.merge(&prop) {
        debug_assert!(false, "prop incompatible with L-system mesh bucket: {e:?}");
    }
}

/// Parse, derive and turtle-walk an L-system generator to its raw
/// [`Skeleton`] — the pure expansion (no ECS, no meshes) shared by the
/// mesher below and the seeded-room per-tree entity clamp
/// ([`lsystem_entity_estimate`], consumed by `pds::room`'s tree-scatter
/// derivation, #810). `None` on grammar errors or empty state so callers
/// can skip the generator.
#[allow(clippy::too_many_arguments)]
pub(crate) fn expand_lsystem_skeleton(
    source_code: &str,
    finalization_code: &str,
    iterations: u32,
    seed: u64,
    angle: Fp,
    step: Fp,
    width: Fp,
    elasticity: Fp,
    tropism: Option<Fp3>,
    generator_ref: &str,
) -> Option<Skeleton> {
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
    Some(interpreter.build_skeleton(&sys.state))
}

/// Parse, derive, interpret and mesh an L-system generator, baking every prop
/// into the mesh bucket for its material id. Returns the raw mesh buckets keyed
/// by material id. `None` on grammar errors or empty state so the caller can
/// skip the spawn.
///
/// Split out of `spawn_lsystem_entity` so `LSystemMeshCache` can invoke the
/// expensive pipeline at most once per `(generator_ref, geometry_hash)` pair.
/// Props (leaves / fruit — the term that exploded entity counts, #810) become
/// merged triangles rather than one entity each, killing the per-frame
/// `BinnedRenderPhase` churn that ratcheted wasm memory to the 4 GiB wall
/// (#811).
#[allow(clippy::too_many_arguments)]
// `pub(crate)` (not `pub(super)`): the render tool's `--room-census` (#810)
// expands seeded rooms' L-systems analytically to count the entities a
// compile would spawn — this builder is pure (no ECS), so it doubles as that
// counter's ground truth.
pub(crate) fn build_lsystem_geometry(
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
    prop_mappings: &HashMap<u16, PropMeshType>,
    prop_scale: Fp,
    generator_ref: &str,
) -> Option<LSystemGeometryBuild> {
    let skeleton = expand_lsystem_skeleton(
        source_code,
        finalization_code,
        iterations,
        seed,
        angle,
        step,
        width,
        elasticity,
        tropism,
        generator_ref,
    )?;

    // Each material ID produces a separate mesh bucket.
    let mut mesh_buckets: Vec<(u16, Mesh)> = LSystemMeshBuilder::new()
        .with_resolution(mesh_resolution.max(3))
        .build(&skeleton)
        .into_iter()
        .collect();

    // Strip branch-bucket tangents up front: some prop primitives carry no
    // tangent attribute, and `Mesh::merge` needs the source to cover every
    // destination attribute. Tangents are regenerated once below, over the
    // combined branch + prop geometry (matching the turtle mesher, which
    // tangents every bucket).
    for (_, mesh) in mesh_buckets.iter_mut() {
        mesh.remove_attribute(Mesh::ATTRIBUTE_TANGENT);
    }

    // Bake each prop into the bucket for its material id, creating a
    // prop-only bucket when the material has no branch geometry. A prop
    // whose `prop_id` has no mapping falls back to `PropMeshType::Leaf`
    // (mirrors the old per-prop spawn path). `prop_scale <= 0` collapses
    // every prop, so skip the fold entirely — a zero scale would also trip
    // `transform_by`'s non-degenerate-scale assertion.
    let ps = prop_scale.0.max(0.0);
    if ps > 0.0 {
        for prop in &skeleton.props {
            let mesh_type = prop_mappings
                .get(&prop.prop_id)
                .copied()
                .unwrap_or(PropMeshType::Leaf);
            let transform = Transform {
                translation: prop.position,
                rotation: prop.rotation,
                scale: prop.scale * ps,
            };
            let key = prop.material_id as u16;
            let idx = match mesh_buckets.iter().position(|(id, _)| *id == key) {
                Some(i) => i,
                None => {
                    mesh_buckets.push((key, super::empty_bucket_mesh()));
                    mesh_buckets.len() - 1
                }
            };
            bake_prop_into_bucket(&mut mesh_buckets[idx].1, mesh_type, transform);
        }
    }

    // Regenerate tangents now that props are folded in. Ignore failures the
    // same way the turtle mesher does — a bucket missing UVs simply goes
    // untangented rather than aborting the build.
    for (_, mesh) in mesh_buckets.iter_mut() {
        let _ = mesh.generate_tangents();
    }

    Some(mesh_buckets)
}

/// Entity count one spawned instance of this L-system produces: `1` root +
/// one mesh-bucket entity per distinct material id. Since #812 props (leaves /
/// flowers) are baked into the bucket for their material id rather than spawned
/// as one entity each, so a prop only adds to the count when its material id
/// has no branch geometry (a fresh prop-only bucket). The distinct-material
/// union below is therefore exactly the bucket set `build_lsystem_geometry`
/// produces — the parity `entity_estimate_matches_built_geometry` guards. The
/// seeded-room deriver steps a tree's `iterations` down until this fits the
/// per-tree budget, and scales scatter counts against the room budget. `None`
/// mirrors [`expand_lsystem_skeleton`]'s grammar-error case (the spawn path
/// skips those generators, so they cost nothing).
#[allow(clippy::too_many_arguments)]
pub(crate) fn lsystem_entity_estimate(
    source_code: &str,
    finalization_code: &str,
    iterations: u32,
    seed: u64,
    angle: Fp,
    step: Fp,
    width: Fp,
    elasticity: Fp,
    tropism: Option<Fp3>,
    generator_ref: &str,
) -> Option<u64> {
    let skeleton = expand_lsystem_skeleton(
        source_code,
        finalization_code,
        iterations,
        seed,
        angle,
        step,
        width,
        elasticity,
        tropism,
        generator_ref,
    )?;
    let mut materials: Vec<u8> = skeleton
        .strands
        .iter()
        .flatten()
        .map(|p| p.material_id)
        .collect();
    // Props merge into the bucket for their material id, adding a bucket only
    // when that material had no branch geometry — union the two id sets.
    materials.extend(skeleton.props.iter().map(|p| p.material_id));
    materials.sort_unstable();
    materials.dedup();
    Some(1 + materials.len() as u64)
}

pub(super) fn spawn_lsystem_entity(
    ctx: &mut SpawnCtx<'_, '_, '_, '_, '_>,
    kind: &GeneratorKind,
    generator_ref: &str,
    transform: Transform,
) -> Option<Entity> {
    let GeneratorKind::LSystem {
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
    } = kind
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
        prop_mappings,
        *prop_scale,
    );
    let geometry = ctx.lsystem_mesh_cache.get_if(generator_ref, geometry_hash);

    let LSystemGeometry {
        mesh_buckets: mesh_bucket_handles,
    } = match geometry {
        Some(g) => g,
        None => {
            let Some(mesh_buckets_raw) = build_lsystem_geometry(
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
                prop_mappings,
                *prop_scale,
                generator_ref,
            ) else {
                // Grammar rejected or empty state — evict any stale entry
                // so a later edit that fixes the grammar triggers a rebuild
                // instead of reusing invalid geometry.
                ctx.lsystem_mesh_cache.remove(generator_ref);
                return None;
            };
            let built = LSystemGeometry {
                mesh_buckets: mesh_buckets_raw
                    .into_iter()
                    .map(|(mat_id, mesh)| (mat_id, ctx.meshes.add(mesh)))
                    .collect(),
            };
            ctx.lsystem_mesh_cache
                .insert(generator_ref.to_string(), geometry_hash, built.clone());
            built
        }
    };

    // Parent every mesh under a single transform so the placement's
    // rotation/position anchors the whole plant/shape as a unit. Avatar
    // mode skips the `RoomEntity` tag — the chassis owns the parent
    // entity and despawns it directly through the Bevy hierarchy.
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

    // Build material handles per slot. For foliage slots (Leaf/Twig/Bark)
    // we *also* spawn a texture-generation task so the handle receives its
    // procedural albedo/normal/ORM maps on a later frame. The palette path
    // still wins when `bevy_symbios::materials::sync_*` has already
    // resolved a shared palette slot for us — in that case we skip the
    // task, because the palette owns texture sync.
    let mut slot_handles: HashMap<u16, Handle<StandardMaterial>> = HashMap::new();
    for (&slot, settings) in lsys_materials.iter() {
        let handle = if let Some(palette) = ctx.palette
            && let Some(h) = palette.materials.get(&slot)
        {
            h.clone()
        } else {
            let key = (generator_ref.to_string(), slot);
            let hash = settings_fingerprint(settings);
            ctx.lsystem_cache_touched.insert(key.clone());
            match ctx.lsystem_material_cache.get_if(&key, hash) {
                Some(handle) => handle,
                None => {
                    let handle = spawn_procedural_material(ctx, settings);
                    ctx.lsystem_material_cache.insert(key, hash, handle.clone());
                    handle
                }
            }
        };
        slot_handles.insert(slot, handle);
    }

    // Backfill a shared fallback for any material id produced by the geometry
    // (branch buckets or baked-in prop buckets) that the generator's
    // `materials` map doesn't define. Without this, the per-use
    // `std_materials.add(..)` fallback below would allocate a fresh
    // `StandardMaterial` for every scatter instance — attacker-crafted
    // geometry with unmapped ids + a scatter of 100k would otherwise push
    // millions of unique materials into the asset registry in a single frame.
    // We route through `lsystem_material_cache` so every scatter instance of
    // this generator shares one handle. `FALLBACK_SENTINEL_HASH` is
    // intentionally a value `settings_fingerprint` cannot return, so a later
    // record edit that *adds* a real `SovereignMaterialSettings` for the slot
    // triggers a rebuild instead of reusing the bare default.
    const FALLBACK_SENTINEL_HASH: u64 = u64::MAX;
    let referenced_ids: std::collections::HashSet<u16> =
        mesh_bucket_handles.iter().map(|(id, _)| *id).collect();
    for id in referenced_ids {
        if slot_handles.contains_key(&id) {
            continue;
        }
        let key = (generator_ref.to_string(), id);
        ctx.lsystem_cache_touched.insert(key.clone());
        let handle = match ctx
            .lsystem_material_cache
            .get_if(&key, FALLBACK_SENTINEL_HASH)
        {
            Some(handle) => handle,
            None => {
                let h = ctx.std_materials.add(StandardMaterial::default());
                ctx.lsystem_material_cache
                    .insert(key, FALLBACK_SENTINEL_HASH, h.clone());
                h
            }
        };
        slot_handles.insert(id, handle);
    }

    // Each mesh bucket is a real ECS entity, so it contributes to the
    // room-wide spawn budget. Props are baked into these buckets at build
    // time (#812), so the count is now just `1 + bucket count` per tree —
    // the term that used to explode (one entity per leaf) is gone, and with
    // it the per-frame `BinnedRenderPhase` churn (#811). The budget guard
    // stays as belt-and-braces against a pathological material count.
    for (material_id, mesh_handle) in mesh_bucket_handles.iter() {
        if budget_exceeded(*ctx.entities_spawned, ctx.budget_warned) {
            break;
        }
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
        *ctx.entities_spawned = ctx.entities_spawned.saturating_add(1);
    }

    // Trait application is now owned by `dispatch_top_level` in compile.rs
    // so a Construct containing an L-system doesn't accidentally double-
    // attach traits (they belong to the Construct, not its internal nodes).
    let _ = ctx.heightmap;
    Some(parent)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real seeded tree species' L-system fields, exactly as the room
    /// deriver builds them — grammar fields plus the prop mapping and prop
    /// scale that now feed the baked geometry (#812).
    #[allow(clippy::type_complexity)]
    fn ternary_props(
        iterations_override: Option<u32>,
    ) -> (
        String,
        String,
        u32,
        u64,
        Fp,
        Fp,
        Fp,
        Fp,
        Option<Fp3>,
        HashMap<u16, PropMeshType>,
        Fp,
    ) {
        let entry =
            crate::catalogue::by_slug("lsys_ternary_props").expect("seeded species in catalogue");
        let generator = entry.build("did:test:estimate");
        let GeneratorKind::LSystem {
            source_code,
            finalization_code,
            iterations,
            seed,
            angle,
            step,
            width,
            elasticity,
            tropism,
            prop_mappings,
            prop_scale,
            ..
        } = generator.kind
        else {
            panic!("lsys_ternary_props is an L-system");
        };
        (
            source_code,
            finalization_code,
            iterations_override.unwrap_or(iterations),
            seed,
            angle,
            step,
            width,
            elasticity,
            tropism,
            prop_mappings,
            prop_scale,
        )
    }

    /// The estimator must count exactly what the geometry builder produces:
    /// one root + one bucket entity per distinct material mesh (props are
    /// baked into those buckets, #812). This is the parity that lets the
    /// seeded-room clamp (#810) and the `--room-census` agree with the real
    /// compile.
    #[test]
    fn entity_estimate_matches_built_geometry() {
        let (src, fin, iters, seed, angle, step, width, elasticity, tropism, mappings, scale) =
            ternary_props(None);
        let buckets = build_lsystem_geometry(
            &src, &fin, iters, seed, angle, step, width, elasticity, tropism, 4, &mappings, scale,
            "test",
        )
        .expect("species grammar builds");
        let estimate = lsystem_entity_estimate(
            &src, &fin, iters, seed, angle, step, width, elasticity, tropism, "test",
        )
        .expect("species grammar estimates");
        assert_eq!(estimate, 1 + buckets.len() as u64);
    }

    /// Props are merged into their material bucket's mesh rather than spawned
    /// as one entity each (#812): folding them in must add vertices, and no
    /// prop escapes the bucket set.
    #[test]
    fn props_are_baked_into_material_buckets() {
        let (src, fin, iters, seed, angle, step, width, elasticity, tropism, mappings, scale) =
            ternary_props(None);
        let verts = |b: &[(u16, Mesh)]| b.iter().map(|(_, m)| m.count_vertices()).sum::<usize>();

        // `prop_scale = 0` collapses every prop, so this is the branch-only
        // geometry; the real scale bakes the props in on top of it.
        let trunk_only = build_lsystem_geometry(
            &src,
            &fin,
            iters,
            seed,
            angle,
            step,
            width,
            elasticity,
            tropism,
            4,
            &mappings,
            Fp(0.0),
            "test",
        )
        .expect("trunk geometry builds");
        let merged = build_lsystem_geometry(
            &src, &fin, iters, seed, angle, step, width, elasticity, tropism, 4, &mappings, scale,
            "test",
        )
        .expect("merged geometry builds");

        assert!(
            verts(&merged) > verts(&trunk_only),
            "baking props should add vertices (trunk={}, merged={})",
            verts(&trunk_only),
            verts(&merged),
        );
        // Every bucket the trunk produced is still present; props may add
        // more (a prop material with no branch geometry), never fewer.
        assert!(merged.len() >= trunk_only.len());
    }

    /// Stepping iterations down must never *increase* the estimate — the
    /// monotonicity the per-tree budget loop in `pds::room` relies on to make
    /// progress. Before #812 props dominated the count and it shrank sharply
    /// with iterations; now props bake into buckets, so the count is bound by
    /// the (near iteration-invariant) distinct-material set and the budget
    /// rarely binds — but the loop still needs "fewer iterations ⇒ no more
    /// entities" to hold.
    #[test]
    fn entity_estimate_does_not_grow_when_iterations_drop() {
        let (src, fin, iters, seed, angle, step, width, elasticity, tropism, _mappings, _scale) =
            ternary_props(None);
        let hi = lsystem_entity_estimate(
            &src, &fin, iters, seed, angle, step, width, elasticity, tropism, "test",
        )
        .expect("estimates at shipped iterations");
        let lo = lsystem_entity_estimate(
            &src,
            &fin,
            iters.saturating_sub(2).max(2),
            seed,
            angle,
            step,
            width,
            elasticity,
            tropism,
            "test",
        )
        .expect("estimates at reduced iterations");
        assert!(
            lo <= hi,
            "reducing iterations must not increase the estimate (lo={lo}, hi={hi})"
        );
    }
}
