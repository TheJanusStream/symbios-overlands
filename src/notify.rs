//! The app-wide "something just happened" channel: the toast QUEUE (#819).
//!
//! Any system pushes a [`Toast`] into the [`Toasts`] resource with the
//! current `Time::elapsed_secs_f64`; `ui::toast::toast_ui` renders the
//! queue and prunes expired entries each frame. This module is the queue
//! and its semantics — coalescing, bounding, eviction, expiry — and knows
//! nothing about egui, themes or where on the screen a toast lands. Those
//! are [`crate::ui::toast`]'s.
//!
//! **Why the split** (#1158). Feedback is not a UI concern that gameplay
//! happens to touch; it is a cross-cutting one that the UI happens to
//! draw. `Toasts` was the single most-imported `crate::ui::` type in the
//! tree — 37 references across `network`, `player`, `loading`, `terrain`
//! and `oauth` — so every one of those modules depended on the egui layer
//! to say a sentence to the user, and their unit tests had to drag egui
//! state into scope to assert that a code path reached a human. Moving the
//! queue here is what lets those modules keep the channel and drop the
//! dependency.
//!
//! Before this channel existed every surface hand-rolled its own transient
//! status (`Local<Option<(String, f64)>>` pairs in the Diagnostics window)
//! or — far more commonly — reported nothing at all: portal failures, gift
//! outcomes, and placement no-ops were silent.

use bevy::prelude::*;

use crate::config::ui::toast as cfg;

/// What flavour of feedback a toast carries; drives only its accent
/// colour. Deliberately smaller than the diagnostics [`Severity`]
/// ladder — toasts are user-facing, so Trace/Critical have no place.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToastKind {
    Info,
    Success,
    Warn,
    Error,
}

/// The app-wide toast queue. Push from any system with the current
/// `Time::elapsed_secs_f64`; the render system prunes expired entries
/// each frame. Bounded to [`crate::config::ui::toast::MAX_VISIBLE`]
/// entries — a burst of notifications drops the oldest rather than
/// growing a scrollback (toasts are glanceable feedback, not a log; the
/// diagnostics event log is the durable record).
#[derive(Resource, Default)]
pub struct Toasts {
    queue: Vec<Toast>,
    next_id: u64,
}

/// One queued notification.
#[derive(Clone, Debug)]
pub struct Toast {
    pub kind: ToastKind,
    pub text: String,
    /// Session-relative second (`Time::elapsed_secs_f64`) past which the
    /// toast is pruned.
    expires_at: f64,
    /// Queue-unique id so the ✕ button can dismiss exactly this entry
    /// even while neighbours expire out from under the loop.
    pub(crate) id: u64,
    /// How many times this exact `(kind, text)` has been pushed while it
    /// was the newest entry (#1277 f23). `1` for an ordinary toast; the
    /// row renders a `×N` badge above that.
    pub(crate) repeats: u32,
}

/// Cut `text` to at most `max_chars` characters plus an ellipsis — on a
/// char boundary by construction, since it counts scalars, never bytes.
/// Shared by the toast queue and the loading rows (#1205): both quote
/// strings someone else wrote, and neither may grow without bound.
pub(crate) fn elide(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push('…');
    out
}

impl Toasts {
    /// Queue a toast. `now` is `Time::elapsed_secs_f64` — passed in
    /// rather than read here so the queue logic stays unit-testable.
    ///
    /// **Repeats coalesce** (#1277 f23). A repeating event used to spend
    /// the whole channel: `respawn_if_fallen` runs in `FixedUpdate` and
    /// pushes on every respawn, so a fall loop filled all
    /// [`cfg::MAX_VISIBLE`] slots with the same sentence within a second
    /// and evicted the publish failure, gift offer or arrival message
    /// raised in those seconds — exactly when the session was in trouble
    /// and other feedback mattered most.
    ///
    /// The comparison is against the **newest queued entry only**, never a
    /// scan of the queue. Merging with an older entry would fold two
    /// unrelated bursts together whenever they happened to interleave, and
    /// would resurrect a message the user has already read past; adjacency
    /// is what makes "this is still happening" true.
    pub fn push(&mut self, kind: ToastKind, text: impl Into<String>, now: f64) {
        let text = elide(&text.into(), cfg::MAX_TEXT_CHARS);
        if let Some(last) = self.queue.last_mut()
            && last.kind == kind
            && last.text == text
        {
            last.repeats = last.repeats.saturating_add(1);
            // Refreshed, not extended: the card lives `DURATION_SECS` from
            // the LAST occurrence, so a loop that stops stops being shown.
            last.expires_at = now + cfg::DURATION_SECS;
            return;
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.queue.push(Toast {
            kind,
            text,
            expires_at: now + cfg::DURATION_SECS,
            id,
            repeats: 1,
        });
        // Oldest-first eviction keeps the newest feedback visible.
        while self.queue.len() > cfg::MAX_VISIBLE {
            self.queue.remove(0);
        }
    }

    pub fn info(&mut self, text: impl Into<String>, now: f64) {
        self.push(ToastKind::Info, text, now);
    }
    pub fn success(&mut self, text: impl Into<String>, now: f64) {
        self.push(ToastKind::Success, text, now);
    }
    pub fn warn(&mut self, text: impl Into<String>, now: f64) {
        self.push(ToastKind::Warn, text, now);
    }
    pub fn error(&mut self, text: impl Into<String>, now: f64) {
        self.push(ToastKind::Error, text, now);
    }

    /// Drop everything — logout cleanup calls this so a toast from one
    /// session can never linger into the next login's first frames.
    pub fn clear(&mut self) {
        self.queue.clear();
    }

    /// What the user was actually told, oldest first. For tests in other
    /// modules that assert a code path reached the human — the queue itself
    /// stays private so nothing outside can reorder or mutate it.
    #[cfg(test)]
    pub(crate) fn shown(&self) -> Vec<(ToastKind, &str)> {
        self.queue
            .iter()
            .map(|t| (t.kind, t.text.as_str()))
            .collect()
    }

    /// How many times the newest entry has repeated (#1277 f23). `None`
    /// on an empty queue.
    #[cfg(test)]
    pub(crate) fn newest_repeats(&self) -> Option<u32> {
        self.queue.last().map(|t| t.repeats)
    }

    pub(crate) fn prune(&mut self, now: f64) {
        self.queue.retain(|t| t.expires_at > now);
    }

    pub(crate) fn dismiss(&mut self, id: u64) {
        self.queue.retain(|t| t.id != id);
    }

    /// Nothing queued, so the renderer can return before touching egui.
    pub(crate) fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// The queue in RENDER order — newest first.
    ///
    /// The stack grows DOWNWARD from a fixed top edge (#1286), so the
    /// first row drawn is the one that never moves and fresh feedback
    /// stays at one spot while older rows are pushed away from it. Under
    /// the old bottom anchor the stack grew upward and this order was
    /// exactly reversed (#1261 f43); the rule is the same one, which is
    /// why it is written as a rule: the newest toast goes against the
    /// anchored edge.
    ///
    /// An iterator rather than a field the renderer indexes: the order IS
    /// the rule, so it belongs beside the queue rather than at the call
    /// site, where the last two placement changes each had to rediscover
    /// it.
    pub(crate) fn newest_first(&self) -> impl Iterator<Item = &Toast> {
        self.queue.iter().rev()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1205: a toast quoting a record-supplied name is bounded at push,
    /// so nothing written into a record can cover the screen from the
    /// Foreground layer. The cut is in chars, so a CJK name is safe.
    #[test]
    fn push_elides_unbounded_text_on_a_char_boundary() {
        let mut t = Toasts::default();
        let long = "あ".repeat(cfg::MAX_TEXT_CHARS + 100);
        t.info(long, 0.0);
        let shown = t.shown();
        let text = shown[0].1;
        assert_eq!(text.chars().count(), cfg::MAX_TEXT_CHARS + 1);
        assert!(text.ends_with('…'));
        t.info("short", 0.0);
        assert_eq!(t.shown()[1].1, "short");
    }

    #[test]
    fn push_assigns_ttl_and_keeps_arrival_order() {
        let mut toasts = Toasts::default();
        toasts.info("first", 10.0);
        toasts.success("second", 11.0);
        assert_eq!(toasts.queue.len(), 2);
        assert_eq!(toasts.queue[0].text, "first");
        assert_eq!(toasts.queue[1].text, "second");
        assert_eq!(toasts.queue[0].expires_at, 10.0 + cfg::DURATION_SECS);
    }

    #[test]
    fn prune_drops_only_expired_entries() {
        let mut toasts = Toasts::default();
        toasts.info("old", 0.0);
        toasts.info("fresh", 5.0);
        toasts.prune(cfg::DURATION_SECS + 1.0);
        assert_eq!(toasts.queue.len(), 1);
        assert_eq!(toasts.queue[0].text, "fresh");
        // At exactly the expiry instant the toast is gone (`>` retain).
        toasts.prune(5.0 + cfg::DURATION_SECS);
        assert!(toasts.queue.is_empty());
    }

    #[test]
    fn queue_caps_at_max_visible_dropping_the_oldest() {
        let mut toasts = Toasts::default();
        for i in 0..(cfg::MAX_VISIBLE + 3) {
            toasts.info(format!("t{i}"), 0.0);
        }
        assert_eq!(toasts.queue.len(), cfg::MAX_VISIBLE);
        assert_eq!(toasts.queue[0].text, "t3");
        assert_eq!(
            toasts.queue.last().unwrap().text,
            format!("t{}", cfg::MAX_VISIBLE + 2)
        );
    }

    /// A repeating event no longer spends the whole channel (#1277 f23).
    ///
    /// This is the pairing the finding describes, and both halves are
    /// asserted over the SAME script, because the defect was never the
    /// duplicate cards — it was the unrelated message they evicted while
    /// nobody was reading them. `respawn_if_fallen` runs in `FixedUpdate`,
    /// so a fall loop reaches `MAX_VISIBLE` in well under a second.
    #[test]
    fn a_repeating_message_coalesces_instead_of_evicting_the_others() {
        // The shape that shipped: something the user needs, then a loop.
        let mut old_way = Toasts::default();
        old_way.error("Saving your world failed — the server refused it.", 0.0);
        for i in 0..cfg::MAX_VISIBLE + 4 {
            // Distinct text stands in for the un-coalesced behaviour: the
            // point is what a queue of MAX_VISIBLE arrivals does to the
            // entry underneath it.
            old_way.warn(format!("Returned to spawn — you fell out. {i}"), 0.1);
        }
        assert!(
            !old_way
                .shown()
                .iter()
                .any(|(_, t)| t.starts_with("Saving your world failed")),
            "the control: an uncoalesced burst evicts the message that mattered"
        );

        // The shape that ships now.
        let mut toasts = Toasts::default();
        toasts.error("Saving your world failed — the server refused it.", 0.0);
        for _ in 0..cfg::MAX_VISIBLE + 4 {
            toasts.warn("Returned to spawn — you fell out of the world.", 0.1);
        }
        assert_eq!(toasts.queue.len(), 2, "the loop occupies exactly one slot");
        assert!(
            toasts.shown()[0].1.starts_with("Saving your world failed"),
            "and the message that mattered is still on screen"
        );
        assert_eq!(toasts.newest_repeats(), Some(cfg::MAX_VISIBLE as u32 + 4));
    }

    /// Coalescing compares the NEWEST entry only, so two interleaved
    /// bursts stay two messages (#1277 f23).
    ///
    /// Scanning the whole queue would merge them — and would resurrect a
    /// card the user had already read past, by refreshing an expiry
    /// several seconds old. Adjacency is what makes "this is still
    /// happening" a true statement.
    #[test]
    fn only_the_newest_entry_coalesces() {
        let mut toasts = Toasts::default();
        toasts.warn("fell", 0.0);
        toasts.info("a friend arrived", 1.0);
        toasts.warn("fell", 2.0);
        assert_eq!(toasts.queue.len(), 3);
        assert_eq!(toasts.newest_repeats(), Some(1));

        // Kind is part of the identity: the same words at a different
        // severity are a different statement.
        let mut kinds = Toasts::default();
        kinds.warn("same words", 0.0);
        kinds.error("same words", 0.0);
        assert_eq!(kinds.queue.len(), 2);
    }

    /// A coalesced card lives `DURATION_SECS` from the LAST occurrence,
    /// not from the first — so a loop that stops, stops being shown
    /// (#1277 f23).
    #[test]
    fn a_repeat_refreshes_the_expiry_rather_than_extending_it() {
        let mut toasts = Toasts::default();
        toasts.warn("fell", 0.0);
        toasts.warn("fell", 4.0);
        assert_eq!(toasts.queue[0].expires_at, 4.0 + cfg::DURATION_SECS);
        // Still bounded: the card is gone one duration after the loop
        // ends, however long the loop ran.
        toasts.prune(4.0 + cfg::DURATION_SECS + 0.1);
        assert!(toasts.queue.is_empty());
    }

    /// The newest toast sits against the anchored edge, so a fixed spot
    /// carries the fresh message and older rows are pushed away from it
    /// (#1286).
    ///
    /// The rule survived a move but its DIRECTION did not: under the old
    /// bottom anchor the stack grew upward and the render iterated
    /// oldest-first to put the newest at the bottom; from a fixed top
    /// edge it grows downward and the same rule needs `.rev()`. Getting
    /// this wrong is not a crash — it is the newest toast jumping down
    /// the screen every time another arrives, which is precisely what
    /// makes a stack unreadable during a burst.
    #[test]
    fn the_newest_toast_is_drawn_first_so_it_never_moves() {
        let mut toasts = Toasts::default();
        for i in 0..4 {
            toasts.info(format!("t{i}"), 0.0);
        }
        // The queue is oldest-first; the RENDER order is what is asserted.
        let drawn: Vec<&str> = toasts.newest_first().map(|t| t.text.as_str()).collect();
        assert_eq!(drawn, ["t3", "t2", "t1", "t0"]);

        // And it holds as the queue changes: a new arrival takes the
        // first slot rather than displacing everything above it.
        toasts.info("t4", 0.0);
        let first = toasts.newest_first().map(|t| t.text.as_str()).next();
        assert_eq!(first, Some("t4"));
    }

    #[test]
    fn dismiss_removes_exactly_the_requested_toast() {
        let mut toasts = Toasts::default();
        toasts.info("keep-a", 0.0);
        toasts.warn("drop-me", 0.0);
        toasts.error("keep-b", 0.0);
        let id = toasts.queue[1].id;
        toasts.dismiss(id);
        let texts: Vec<&str> = toasts.queue.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, ["keep-a", "keep-b"]);
    }

    #[test]
    fn clear_empties_the_queue_for_logout() {
        let mut toasts = Toasts::default();
        toasts.info("anything", 0.0);
        toasts.clear();
        assert!(toasts.queue.is_empty());
    }

    #[test]
    fn helper_constructors_tag_the_matching_kind() {
        let mut toasts = Toasts::default();
        toasts.info("i", 0.0);
        toasts.success("s", 0.0);
        toasts.warn("w", 0.0);
        toasts.error("e", 0.0);
        let kinds: Vec<ToastKind> = toasts.queue.iter().map(|t| t.kind).collect();
        assert_eq!(
            kinds,
            [
                ToastKind::Info,
                ToastKind::Success,
                ToastKind::Warn,
                ToastKind::Error
            ]
        );
    }
}
