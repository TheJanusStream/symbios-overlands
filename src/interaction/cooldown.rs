//! Shared per-`(avatar, recipe)` cooldown throttle used by the contact
//! consumer channels (audio cues, particle bursts, decal stamps). Each
//! channel wraps a [`CooldownTable`] in its own `Resource` so Bevy still
//! sees three independent states; the insert/check/prune mechanics live
//! here once (#652).

use std::collections::HashMap;

use bevy::prelude::*;

/// Per-`(avatar, recipe index)` time of last emission, for the cooldown
/// throttle on continuous (`Dwell`) recipes. Keyed by the recipe's *index*
/// in its registry list, so renaming a recipe in the editor never resets a
/// live cooldown.
///
/// **The index is only stable while the list is** (#1254 f322). This doc
/// used to claim "a room recompile rebuilds both", and it rebuilt the
/// registry and not the tables — so deleting a recipe shifted every later
/// index down by one and transplanted a live throttle onto whichever recipe
/// inherited the slot, and the sanitiser's name-sort at the 64-recipe cap
/// could permute them wholesale. It self-healed on the TTL, which is what
/// made it a transient oddity in the middle of the exact workflow (delete a
/// recipe, immediately test the survivors) where it is least attributable.
/// [`clear`](Self::clear) is called from `apply_contact_recipes` now, so the
/// claim is true.
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

    /// Forget every throttle. Called when the registry the indices refer to
    /// is rebuilt (#1254 f322) — losing a live cooldown for one frame is
    /// nothing; applying it to a different recipe is a bug.
    pub fn clear(&mut self) {
        self.last.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1254 f322, as the sequence: the owner deletes a recipe and the one
    /// below it goes quiet for a while, then starts working again on its
    /// own. The index the throttle is keyed by belongs to a list that no
    /// longer exists.
    #[test]
    fn clearing_the_table_is_what_a_registry_rebuild_owes_it() {
        let mut t = CooldownTable::new(30.0);
        let victim = (Entity::PLACEHOLDER, 1);
        t.mark(victim, 10.0);
        assert!(t.active(victim, 10.1, 5.0), "the throttle is live");
        // The recipe at index 0 is deleted: index 1's recipe is now index
        // 0, and index 1 is somebody else's — carrying this mark.
        t.clear();
        assert!(
            !t.active(victim, 10.1, 5.0),
            "a rebuilt registry must not inherit the old list's throttles"
        );
    }

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
