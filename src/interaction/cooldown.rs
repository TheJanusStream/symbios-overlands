//! Shared per-`(avatar, recipe)` cooldown throttle used by the contact
//! consumer channels (audio cues, particle bursts, decal stamps). Each
//! channel wraps a [`CooldownTable`] in its own `Resource` so Bevy still
//! sees three independent states; the insert/check/prune mechanics live
//! here once (#652).

use std::collections::HashMap;

use bevy::prelude::*;

/// Per-`(avatar, recipe index)` time of last emission, for the cooldown
/// throttle on continuous (`Dwell`) recipes. Keyed by the recipe's *index*
/// in its registry list (stable for the registry's lifetime; a room
/// recompile rebuilds both, and stale entries are TTL-pruned anyway), so
/// renaming a recipe in the editor never resets a live cooldown.
pub struct CooldownTable {
    /// Prune horizon (s) — far longer than any sane recipe cooldown, so
    /// pruning never resets a live throttle.
    ttl: f32,
    last: HashMap<(Entity, usize), f32>,
}

impl CooldownTable {
    pub fn new(ttl: f32) -> Self {
        Self {
            ttl,
            last: HashMap::new(),
        }
    }

    /// True while `key` is still within `cooldown` seconds of its last
    /// [`mark`](Self::mark) — the caller should skip this emission.
    pub fn active(&self, key: (Entity, usize), now: f32, cooldown: f32) -> bool {
        self.last.get(&key).is_some_and(|&t| now - t < cooldown)
    }

    /// Record an emission for `key` at `now`.
    pub fn mark(&mut self, key: (Entity, usize), now: f32) {
        self.last.insert(key, now);
    }

    /// Drop entries older than the table's TTL (despawned avatars,
    /// long-idle throttles).
    pub fn prune(&mut self, now: f32) {
        let ttl = self.ttl;
        self.last.retain(|_, &mut t| now - t < ttl);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooldown_gates_then_releases() {
        let mut t = CooldownTable::new(30.0);
        let key = (Entity::PLACEHOLDER, 3);
        assert!(!t.active(key, 10.0, 0.5), "no mark yet — never active");
        t.mark(key, 10.0);
        assert!(t.active(key, 10.4, 0.5), "inside the window");
        assert!(!t.active(key, 10.6, 0.5), "window elapsed");
    }

    #[test]
    fn prune_drops_only_stale_entries() {
        let mut t = CooldownTable::new(30.0);
        let fresh = (Entity::PLACEHOLDER, 0);
        let stale = (Entity::PLACEHOLDER, 1);
        t.mark(stale, 0.0);
        t.mark(fresh, 40.0);
        t.prune(45.0);
        assert!(!t.active(stale, 45.0, f32::MAX), "stale entry pruned");
        assert!(t.active(fresh, 45.0, 10.0), "fresh entry survives");
    }
}
