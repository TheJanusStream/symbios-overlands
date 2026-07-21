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
//! There is no generator ref to GC against, so these caches bound themselves
//! by capacity ([`PRIM_CACHE_CAPACITY`]) and are cleared wholesale on logout
//! alongside the per-generator caches (#625 — a session's builds must not
//! outlive it). Clearing wholesale rather than evicting one entry keeps the
//! policy trivial and, since the next compile pass re-populates only what it
//! actually spawns, costs at most one pass of rebuilds.

use std::hash::{DefaultHasher, Hash, Hasher};

use bevy::prelude::*;

use crate::pds::GeneratorKind;

use super::generator_cache::GeneratorCache;

/// Entry ceiling per primitive cache. A room's distinct primitive geometries
/// number in the low hundreds even for a dense settlement; this is headroom
/// over that while still bounding a pathological record (or a long session of
/// edits) from pinning handles indefinitely.
pub const PRIM_CACHE_CAPACITY: usize = 4_096;

/// Content-addressed primitive mesh dedup — see the [module docs](self).
pub type PrimMeshCache = GeneratorCache<u64, Handle<Mesh>>;

/// Content-addressed primitive material dedup — see the [module docs](self).
pub type PrimMaterialCache = GeneratorCache<u64, Handle<StandardMaterial>>;

/// Stable content hash of a primitive's **geometry**, excluding its material.
///
/// `build_primitive_mesh` ignores the material entirely, so folding it into
/// the key would split the cache on colour alone — every re-tinted copy of one
/// shape would rebuild an identical mesh. The `material` field is dropped from
/// the serialised form generically rather than by matching all sixteen
/// primitive variants, which would be a second place to update whenever a
/// primitive is added.
pub fn prim_geometry_fingerprint(kind: &GeneratorKind) -> u64 {
    let mut hasher = DefaultHasher::new();
    match serde_json::to_value(kind) {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                obj.remove("material");
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
/// Call before inserting. See the eviction note in the [module docs](self) for
/// why this is wholesale rather than LRU.
pub(super) fn bound_capacity<V: Clone>(cache: &mut GeneratorCache<u64, V>) {
    if cache.len() >= PRIM_CACHE_CAPACITY {
        cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{Fp, Fp2, Fp3, SovereignMaterialSettings, TortureParams};

    fn plane(size: [f32; 2], base_color: [f32; 3]) -> GeneratorKind {
        GeneratorKind::Plane {
            size: Fp2(size),
            subdivisions: 0,
            solid: false,
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
            size: Fp3([1.0, 1.0, 1.0]),
            solid: false,
            material: SovereignMaterialSettings::default(),
            torture: TortureParams::default(),
        };
        let sphere = GeneratorKind::Sphere {
            radius: Fp(1.0),
            resolution: 5,
            solid: false,
            material: SovereignMaterialSettings::default(),
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
