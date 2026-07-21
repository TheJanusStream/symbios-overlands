//! The shared per-generator cache family and content-fingerprint helpers
//! (#646). The L-system and Shape pipelines each keep a material cache and
//! a geometry cache with identical semantics — keyed per generator (or per
//! `(generator, slot)`), invalidated by a content hash of the settings that
//! built the entry, GC'd against the compile job's touch-sets, and cleared
//! at logout. [`GeneratorCache`] is that family expressed once; the
//! concrete caches are type aliases in `lsystem.rs` / `shape.rs`.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use bevy::ecs::resource::Resource;

use crate::pds::SovereignMaterialSettings;

/// One cached build output: the content hash of the settings that built it,
/// plus the built value.
pub(super) struct CachedBuild<V> {
    pub fingerprint: u64,
    pub value: V,
}

/// Persistent cross-compile cache for per-generator build outputs.
///
/// Without one of these, every `RoomRecord` change rebuilds every
/// generator's output — re-deriving grammars and re-baking textures for
/// configs that haven't moved, once per scatter sample. Lookups compare a
/// content fingerprint so a record edit that touches *only* (say) the
/// scatter count reuses last pass's build instead of redoing it.
///
/// Entries for keys not touched during a full compile pass are dropped at
/// the end of that pass (the executor retains against the job's
/// touch-sets) so stale generators stop pinning their handles in the asset
/// registries.
pub struct GeneratorCache<K, V> {
    pub(super) entries: HashMap<K, CachedBuild<V>>,
}

impl<K, V> Default for GeneratorCache<K, V> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

impl<K, V> Resource for GeneratorCache<K, V>
where
    K: Send + Sync + 'static,
    V: Send + Sync + 'static,
{
}

impl<K: Eq + Hash, V: Clone> GeneratorCache<K, V> {
    /// The cached value for `key`, provided it was built from settings with
    /// this exact `fingerprint` — a hash mismatch is a miss, not an error.
    pub(super) fn get_if<Q>(&self, key: &Q, fingerprint: u64) -> Option<V>
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        match self.entries.get(key) {
            Some(c) if c.fingerprint == fingerprint => Some(c.value.clone()),
            _ => None,
        }
    }

    /// Store `value` as the build output for `key` at `fingerprint`,
    /// replacing any stale entry.
    pub(super) fn insert(&mut self, key: K, fingerprint: u64, value: V) {
        self.entries.insert(key, CachedBuild { fingerprint, value });
    }

    /// Evict `key` — used when a rebuild fails, so a later edit that fixes
    /// the config triggers a fresh build instead of reusing stale output.
    pub(super) fn remove<Q>(&mut self, key: &Q)
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.entries.remove(key);
    }

    /// Drop every cached entry (and the handles it pins). Called on logout
    /// so one session's builds don't outlive it (#625).
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Number of cached entries. The content-addressed primitive caches
    /// (`prim_cache`) bound themselves on this, having no generator ref to
    /// GC against.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` when nothing is cached.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Stable content hash of a `SovereignMaterialSettings` — bytes of its
/// canonical JSON serialisation. Shared by the L-system and Shape material
/// caches so the two co-exist with the same eviction strategy.
pub(super) fn settings_fingerprint(settings: &SovereignMaterialSettings) -> u64 {
    let mut hasher = DefaultHasher::new();
    match serde_json::to_vec(settings) {
        Ok(bytes) => bytes.hash(&mut hasher),
        // Serialisation of a plain struct of scalars cannot fail in
        // practice; if it somehow does, fall back to a distinct sentinel
        // so the lookup treats it as a miss (forcing a rebuild) rather
        // than collapsing all failures onto the same key.
        Err(_) => {
            0xDEAD_BEEF_u64.hash(&mut hasher);
            (settings as *const SovereignMaterialSettings as usize).hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Quantizing content hasher for geometry fingerprints. Float fields are
/// hashed through [`Self::fp`]'s fixed-point form so NaN / denormal floats
/// can't destabilise the key across compile passes; discrete fields hash
/// verbatim via [`Self::field`]. Shared by the L-system and Shape geometry
/// fingerprints, which differ only in their field sets.
pub(super) struct GeometryHasher(DefaultHasher);

impl GeometryHasher {
    /// Fixed-point scale: 1/10_000 resolution, matching the historical
    /// per-pipeline fingerprint helpers so cache keys keep their semantics.
    const FP_SCALE: f32 = 10_000.0;

    pub(super) fn new() -> Self {
        Self(DefaultHasher::new())
    }

    /// Hash a float via its fixed-point form.
    pub(super) fn fp(&mut self, v: f32) {
        (((v * Self::FP_SCALE).round()) as i32).hash(&mut self.0);
    }

    /// Hash a discrete (already hash-stable) field verbatim.
    pub(super) fn field(&mut self, v: impl Hash) {
        v.hash(&mut self.0);
    }

    pub(super) fn finish(self) -> u64 {
        self.0.finish()
    }
}
