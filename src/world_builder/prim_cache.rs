//! Content-addressed primitive mesh / material dedup (#918).
//!
//! Unlike the L-system and Shape pipelines, primitives have no
//! *per-generator* cache: `spawn_primitive_entity` used to build a fresh
//! [`Mesh`] and a fresh [`StandardMaterial`] for every instance it spawned.
//! Only the generated *images* were deduped (by the upstream `TextureCache`),
//! so a rock scatter allocated one mesh and one material per boulder. That is
//! tolerable at a few dozen boulders and untenable for the ground-cover tier
//! (#911), which places cards by the hundred.
//!
//! These caches are keyed by **content hash** rather than by generator ref:
//! every instance of a scattered prop hashes identically, so the whole scatter
//! collapses onto one mesh handle and one material handle. That also dedups
//! *across* generators — two catalogue props that happen to use the same card
//! geometry share a mesh.
//!
//! # Keying
//!
//! * Mesh — [`prim_geometry_fingerprint`], the primitive's serialised form
//!   with the `material` field removed, so a colour change reuses the mesh.
//! * Material — the shared
//!   [`settings_fingerprint`](super::generator_cache::settings_fingerprint),
//!   the same hash the L-system and Shape material caches use.
//!
//! Because the key *is* the fingerprint, a lookup is self-validating: a
//! matching key can only have been built from identical content.
//!
//! # Eviction
//!
//! These caches are swept by the compile executor's end-of-job GC, exactly
//! like the per-generator caches: a full rebuild retains only the keys that
//! rebuild touched. The capacity ceiling ([`PRIM_CACHE_CAPACITY`]) remains as
//! a backstop against a pathological single record, and logout clears them
//! wholesale so a session's builds never outlive it (#625).
//!
//! The sweep is what #919 was missing. Shipped without it, these caches
//! survived every rebuild, so each region re-roll permanently added that
//! region's prim meshes and materials — and, through the materials, their
//! procedural images. Measured at ~90 image and ~100 mesh handles per
//! re-roll and roughly 70 MB of RSS, none of it ever released; the 4096-entry
//! ceiling would not have been reached for some 45 re-rolls.
//!
//! Eviction is safe because a cache entry is only ever a *second* owner of a
//! handle — every live instance holds its own — so dropping one frees the
//! asset if and only if nothing is using it, and costs a re-bake on the next
//! miss otherwise.

use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use bevy::prelude::*;

use crate::pds::GeneratorKind;

use super::generator_cache::GeneratorCache;
use super::prim::{FacePlan, FaceTable};

/// Entry ceiling per primitive cache. A room's distinct primitive geometries
/// number in the low hundreds even for a dense settlement; this is headroom
/// over that while still bounding a pathological record (or a long session of
/// edits) from pinning handles indefinitely.
pub const PRIM_CACHE_CAPACITY: usize = 4_096;

/// One built material group of a primitive (#959): the mesh carrying that
/// group's triangles, the face each of those triangles belongs to (for
/// click-picking), and which [`FacePlan`] group it draws with.
///
/// A prim with no per-face overrides has exactly one of these, and its mesh
/// is the whole prim — the pre-#959 shape of the cache.
#[derive(Clone)]
pub struct CachedGroup {
    pub mesh: Handle<Mesh>,
    /// Shared rather than cloned: every instance of a scattered prop points
    /// at the same table, and the spawner hands it to each child entity.
    pub faces: Arc<FaceTable>,
    /// Index into [`FacePlan::groups`].
    pub group: usize,
}

/// Content-addressed primitive mesh dedup — see the [module docs](self).
///
/// The value is the prim's material groups in spawn order. `Arc<[_]>` so a
/// cache hit costs a refcount rather than a `Vec` allocation per instance.
pub type PrimMeshCache = GeneratorCache<u64, Arc<[CachedGroup]>>;

/// Content-addressed primitive material dedup — see the [module docs](self).
pub type PrimMaterialCache = GeneratorCache<u64, Handle<StandardMaterial>>;

/// Mesh-cache key for a primitive: its geometry fingerprint, extended by the
/// *structure* of its face split when it has one.
///
/// A prim whose faces all share the base material keys on the plain
/// geometry fingerprint alone, so every entry cached before #959 — and every
/// prim in every existing room — stays valid and shared. Only a genuinely
/// split prim takes a distinct key, and because
/// [`FacePlan::signature`](super::prim::FacePlan::signature) hashes the
/// partition rather than the materials, recolouring one face reuses its
/// meshes instead of re-cutting them.
pub fn prim_mesh_key(kind: &GeneratorKind, plan: &FacePlan) -> u64 {
    let geometry = prim_geometry_fingerprint(kind);
    if plan.is_whole() {
        return geometry;
    }
    let mut hasher = DefaultHasher::new();
    geometry.hash(&mut hasher);
    plan.signature().hash(&mut hasher);
    hasher.finish()
}

/// Stable content hash of a primitive's **base geometry**, excluding
/// everything about its materials.
///
/// `build_primitive_mesh` ignores the material entirely, so folding it into
/// the key would split the cache on colour alone — every re-tinted copy of one
/// shape would rebuild an identical mesh. The `material` field is dropped from
/// the serialised form generically rather than by matching all sixteen
/// primitive variants, which would be a second place to update whenever a
/// primitive is added.
///
/// `faces` goes the same way, and for the same reason — its entries are
/// mostly *override materials*. What the per-face split does contribute to
/// geometry (which faces group together, and each group's projection) is
/// hashed separately by [`FacePlan::signature`], which
/// [`prim_mesh_key`] folds in only for a prim that actually splits. Dropping
/// the whole field here is what lets a prim whose overrides all resolve to
/// the base material share the plain prim's mesh instead of cutting an
/// identical copy under its own key.
pub fn prim_geometry_fingerprint(kind: &GeneratorKind) -> u64 {
    let mut hasher = DefaultHasher::new();
    match serde_json::to_value(kind) {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                obj.remove("material");
                obj.remove("faces");
            }
            // `to_string` on a `Value` is canonical for our shapes: object keys
            // come back in the order serde emitted them, which is the struct's
            // declaration order and therefore stable across passes.
            v.to_string().hash(&mut hasher);
        }
        // A primitive kind is a plain struct of scalars; serialisation cannot
        // fail in practice. Fall back to a per-call-unique sentinel so the
        // lookup misses (forcing a fresh build) instead of colliding every
        // failure onto one key.
        Err(_) => {
            0xF01D_BEEF_u64.hash(&mut hasher);
            (kind as *const GeneratorKind as usize).hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Drop every entry once the cache exceeds [`PRIM_CACHE_CAPACITY`].
///
/// Call before inserting. A backstop only — the executor's per-rebuild sweep
/// is the real eviction policy. Wholesale rather than LRU because, with the
/// sweep in place, reaching this ceiling means one record alone defined 4096
/// distinct primitives, and there is no useful subset to keep.
pub(super) fn bound_capacity<V: Clone>(cache: &mut GeneratorCache<u64, V>) {
    if cache.len() >= PRIM_CACHE_CAPACITY {
        cache.clear();
    }
}

/// Look `key` up **and** mark it reachable for this compile pass.
///
/// The two are paired in one call deliberately. The GC retains exactly the
/// keys a full rebuild touched, so a key served from cache must be marked
/// just as surely as one that was built — mark only the miss path and the
/// sweep evicts precisely the entries that were doing their job, on the very
/// next rebuild. Pairing them here means a caller cannot take the value
/// without leaving the mark (#919).
pub(super) fn get_and_touch<V: Clone>(
    cache: &GeneratorCache<u64, V>,
    touched: &mut HashSet<u64>,
    key: u64,
) -> Option<V> {
    touched.insert(key);
    cache.get_if(&key, key)
}

/// Retain only the entries whose keys `touched` contains.
///
/// Separate from the caller's `retain` call so the sweep and the marking it
/// depends on are defined together, and so the contract between them is
/// testable without standing up the compile world.
pub(super) fn retain_touched<V: Clone>(cache: &mut GeneratorCache<u64, V>, touched: &HashSet<u64>) {
    cache.entries.retain(|k, _| touched.contains(k));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{Fp, Fp2, Fp3, SovereignMaterialSettings, TortureParams};

    fn plane(size: [f32; 2], base_color: [f32; 3]) -> GeneratorKind {
        GeneratorKind::Plane {
            uv_mapping: crate::pds::generator::UvMapping::fit(),
            size: Fp2(size),
            subdivisions: 0,
            solid: false,
            faces: Vec::new(),
            material: SovereignMaterialSettings {
                base_color: Fp3(base_color),
                ..Default::default()
            },
            torture: TortureParams::default(),
        }
    }

    #[test]
    fn identical_geometry_hashes_identically() {
        assert_eq!(
            prim_geometry_fingerprint(&plane([1.0, 2.0], [1.0, 1.0, 1.0])),
            prim_geometry_fingerprint(&plane([1.0, 2.0], [1.0, 1.0, 1.0])),
        );
    }

    /// #959 cache continuity: a prim with no per-face overrides — which is
    /// every prim in every room authored before the feature existed — must
    /// key on the plain geometry fingerprint, so its already-cached mesh is
    /// still found and still shared.
    #[test]
    fn an_unsplit_prim_keeps_the_plain_geometry_key() {
        let kind = plane([1.0, 2.0], [1.0, 1.0, 1.0]);
        let plan = crate::world_builder::prim::plan_faces(&kind);
        assert!(plan.is_whole());
        assert_eq!(
            prim_mesh_key(&kind, &plan),
            prim_geometry_fingerprint(&kind),
            "an unsplit prim must not be re-keyed by #959"
        );
    }

    /// A split prim takes its own key (its meshes differ), but recolouring
    /// one of its faces must not — the split's *structure* is unchanged, so
    /// the cut meshes are still valid.
    #[test]
    fn a_split_prim_keys_on_structure_not_colour() {
        use crate::pds::generator::{FaceKey, FaceOverride};
        let painted = |color: [f32; 3]| {
            let mut kind = plane([1.0, 2.0], [1.0, 1.0, 1.0]);
            *kind.faces_mut().unwrap() = vec![FaceOverride {
                face: FaceKey::Surface,
                material: SovereignMaterialSettings {
                    base_color: Fp3(color),
                    ..Default::default()
                },
                uv_mapping: None,
            }];
            kind
        };
        let key_of = |kind: &GeneratorKind| {
            prim_mesh_key(kind, &crate::world_builder::prim::plan_faces(kind))
        };

        let red = painted([1.0, 0.0, 0.0]);
        let blue = painted([0.0, 0.0, 1.0]);
        let plain = plane([1.0, 2.0], [1.0, 1.0, 1.0]);
        assert_ne!(
            key_of(&red),
            key_of(&plain),
            "a split prim must not collide with the unsplit one"
        );
        assert_eq!(
            key_of(&red),
            key_of(&blue),
            "recolouring a face must reuse its cut meshes"
        );
    }

    /// The whole point of stripping `material`: a re-tinted copy of one shape
    /// must reuse the cached mesh rather than rebuild an identical one.
    #[test]
    fn material_does_not_affect_the_geometry_key() {
        assert_eq!(
            prim_geometry_fingerprint(&plane([1.0, 2.0], [1.0, 0.0, 0.0])),
            prim_geometry_fingerprint(&plane([1.0, 2.0], [0.0, 0.0, 1.0])),
        );
    }

    #[test]
    fn geometry_changes_do_affect_the_key() {
        assert_ne!(
            prim_geometry_fingerprint(&plane([1.0, 2.0], [1.0, 1.0, 1.0])),
            prim_geometry_fingerprint(&plane([1.0, 3.0], [1.0, 1.0, 1.0])),
        );
        // Solidity changes the collider, so it must not share a key either.
        let mut solid = plane([1.0, 2.0], [1.0, 1.0, 1.0]);
        if let GeneratorKind::Plane { solid: s, .. } = &mut solid {
            *s = true;
        }
        assert_ne!(
            prim_geometry_fingerprint(&plane([1.0, 2.0], [1.0, 1.0, 1.0])),
            prim_geometry_fingerprint(&solid),
        );
    }

    #[test]
    fn distinct_primitive_kinds_do_not_collide() {
        let cuboid = GeneratorKind::Cuboid {
            uv_mapping: crate::pds::generator::UvMapping::default(),
            size: Fp3([1.0, 1.0, 1.0]),
            solid: false,
            material: SovereignMaterialSettings::default(),
            faces: Vec::new(),
            torture: TortureParams::default(),
        };
        let sphere = GeneratorKind::Sphere {
            radius: Fp(1.0),
            resolution: 5,
            solid: false,
            uv_mapping: crate::pds::generator::UvMapping::fit(),
            material: SovereignMaterialSettings::default(),
            faces: Vec::new(),
            torture: TortureParams::default(),
        };
        assert_ne!(
            prim_geometry_fingerprint(&cuboid),
            prim_geometry_fingerprint(&sphere)
        );
    }

    /// The dedup contract dispatch relies on: two instances of one scattered
    /// prop hit the same entry, so a scatter of N shares one built value
    /// instead of allocating N. A differing geometry must still miss.
    #[test]
    fn identical_prims_share_one_cache_entry() {
        let mut cache: GeneratorCache<u64, u32> = GeneratorCache::default();
        let a = plane([0.55, 0.45], [1.0, 1.0, 1.0]);
        let key_a = prim_geometry_fingerprint(&a);
        assert!(
            cache.get_if(&key_a, key_a).is_none(),
            "first instance must miss"
        );
        cache.insert(key_a, key_a, 7);

        // A second, separately-constructed instance of the same prop.
        let b = plane([0.55, 0.45], [1.0, 1.0, 1.0]);
        let key_b = prim_geometry_fingerprint(&b);
        assert_eq!(cache.get_if(&key_b, key_b), Some(7), "second must hit");
        assert_eq!(cache.len(), 1, "one entry serves every instance");

        // A different card size is a different mesh.
        let c = plane([0.9, 0.45], [1.0, 1.0, 1.0]);
        let key_c = prim_geometry_fingerprint(&c);
        assert!(
            cache.get_if(&key_c, key_c).is_none(),
            "different geometry must miss"
        );
    }

    /// The #919 sweep: a rebuild retains what it touched and drops the rest.
    /// Without this the caches survived every rebuild, so each region
    /// re-roll permanently added its prims to them.
    #[test]
    fn sweep_drops_entries_the_rebuild_did_not_touch() {
        let mut cache: GeneratorCache<u64, u32> = GeneratorCache::default();
        // Two regions' worth of prims share the cache.
        for k in [1u64, 2, 3, 4] {
            cache.insert(k, k, k as u32);
        }
        // A rebuild that only reaches keys 3 and 4 — the new region.
        let touched: HashSet<u64> = [3u64, 4].into_iter().collect();
        retain_touched(&mut cache, &touched);

        assert_eq!(cache.len(), 2, "the old region's prims must not survive");
        assert!(cache.get_if(&3u64, 3).is_some() && cache.get_if(&4u64, 4).is_some());
        assert!(cache.get_if(&1u64, 1).is_none() && cache.get_if(&2u64, 2).is_none());
    }

    /// The subtle half of the sweep, and the way it would most plausibly be
    /// broken by a later edit: a key served *from cache* must be marked
    /// reachable too. Mark only on the build path and the very next rebuild
    /// evicts every entry that was working — the cache would then thrash
    /// instead of leak, which is harder to notice and worse for frame time.
    #[test]
    fn a_cache_hit_marks_its_key_so_the_sweep_keeps_it() {
        let mut cache: GeneratorCache<u64, u32> = GeneratorCache::default();
        cache.insert(7, 7, 70);

        // Pass one populated it; pass two only *reads* it.
        let mut touched = HashSet::new();
        assert_eq!(
            get_and_touch(&cache, &mut touched, 7),
            Some(70),
            "the entry is live and must be served"
        );
        retain_touched(&mut cache, &touched);
        assert_eq!(
            cache.get_if(&7u64, 7),
            Some(70),
            "a hit must survive the sweep that follows it"
        );
    }

    /// A miss marks its key as well, so the entry the caller is about to
    /// insert is not swept away by the same pass that built it.
    #[test]
    fn a_cache_miss_also_marks_its_key() {
        let cache: GeneratorCache<u64, u32> = GeneratorCache::default();
        let mut touched = HashSet::new();
        assert_eq!(get_and_touch(&cache, &mut touched, 9), None);
        assert!(touched.contains(&9), "a miss must still mark the key");
    }

    #[test]
    fn capacity_bound_clears_a_full_cache() {
        let mut cache: GeneratorCache<u64, u32> = GeneratorCache::default();
        for i in 0..PRIM_CACHE_CAPACITY as u64 {
            cache.insert(i, i, 0);
        }
        assert_eq!(cache.len(), PRIM_CACHE_CAPACITY);
        bound_capacity(&mut cache);
        assert_eq!(cache.len(), 0, "a full cache is dropped wholesale");
    }
}
