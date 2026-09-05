//! Transient toast notifications (#819).
//!
//! The single app-wide channel for "something just happened" feedback:
//! any system pushes a [`Toast`] into the [`Toasts`] resource and
//! [`toast_ui`] renders the queue as a stack of small framed rows
//! anchored to the BOTTOM-right of the screen, each expiring after
//! [`crate::config::ui::toast::DURATION_SECS`] or on its ✕ button. The
//! corner matters: the area is a real pointer area at
//! `Order::Foreground`, so wherever it sits it eats clicks — and the
//! top-right it used to occupy is where all five right-anchored windows
//! open (#1261 f43).
//!
//! Before this existed every surface hand-rolled its own transient
//! status (`Local<Option<(String, f64)>>` pairs in the Diagnostics
//! window) or — far more commonly — reported nothing at all: portal
//! failures, gift outcomes, and placement no-ops were silent. Those
//! flows migrate onto this channel issue by issue; the Diagnostics
//! landmark-copy and log-export statuses are the founding consumers.
//!
//! Severity is carried by THREE cues, not one (#1259 f236): the
//! painted dot, a glyph before the text ([`crate::ui::affordances`]'s
//! `CHECK` / `WARNING` / `CROSS`), and the wording of the message
//! itself. The dot used to be the whole of the chrome's signal, and it
//! read from the diagnostics severity RAMP — a gradient built to rank
//! severities in a debugging HUD, on which Warn and Error sat 1.47:1
//! apart and both plainly orange. It reads the semantic
//! `status.ok/warn/error/info` set now, which the palette's own
//! distinctness guard covers.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

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

impl ToastKind {
    /// Dot colour from the active theme's SEMANTIC status set (#1259
    /// f236), not the diagnostics severity ramp it used to borrow.
    ///
    /// The ramp is an ordered gradient for a debugging HUD, and it read
    /// as one: in Dark, Warn `(210,170,90)` and Error `(210,120,90)`
    /// were 1.47:1 apart and both plainly orange — which was the entire
    /// difference between "saved with a caution" and "the save failed"
    /// in the app's only success/failure channel. `status.ok/warn/
    /// error/info` are the four the palette's own distinctness guard
    /// has always covered.
    fn color(self, th: &crate::ui::theme::Theme) -> egui::Color32 {
        match self {
            ToastKind::Info => th.status.info,
            ToastKind::Success => th.status.ok,
            ToastKind::Warn => th.status.warn,
            ToastKind::Error => th.status.error,
        }
    }

    /// The severity token drawn before the text, so the chrome carries a
    /// SHAPE and not only a hue (WCAG 1.4.1) — under deuteranopia the
    /// warn and error dots desaturate toward each other.
    ///
    /// `Info` has none: it is the absence of a verdict, and the missing
    /// glyph is itself the signal. See [`crate::ui::affordances`] for
    /// why no fourth code point was invented.
    fn glyph(self) -> Option<&'static str> {
        match self {
            ToastKind::Info => None,
            ToastKind::Success => Some(crate::ui::affordances::CHECK),
            ToastKind::Warn => Some(crate::ui::affordances::WARNING),
            ToastKind::Error => Some(crate::ui::affordances::CROSS),
        }
    }
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
    id: u64,
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
    pub fn push(&mut self, kind: ToastKind, text: impl Into<String>, now: f64) {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.queue.push(Toast {
            kind,
            text: elide(&text.into(), cfg::MAX_TEXT_CHARS),
            expires_at: now + cfg::DURATION_SECS,
            id,
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

    fn prune(&mut self, now: f64) {
        self.queue.retain(|t| t.expires_at > now);
    }

    fn dismiss(&mut self, id: u64) {
        self.queue.retain(|t| t.id != id);
    }
}

/// Render the toast stack. Registered last in the egui chain and lifted
/// to the `Foreground` order so toasts paint above every floating
/// window; the anchored [`egui::Area`] is still a real pointer area, so
/// world-click consumers' existing `is_pointer_over_area()` checks keep
/// clicks on a toast from leaking into the 3D scene.
pub fn toast_ui(mut contexts: EguiContexts, mut toasts: ResMut<Toasts>, time: Res<Time>) {
    let now = time.elapsed_secs_f64();
    toasts.prune(now);
    if toasts.queue.is_empty() {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let mut dismissed: Option<u64> = None;
    egui::Area::new(egui::Id::new("overlands-toasts"))
        .anchor(egui::Align2::RIGHT_BOTTOM, cfg::ANCHOR_OFFSET)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_max_width(cfg::MAX_WIDTH);
            // Oldest first, so the NEWEST row sits against the corner
            // (#1261 f43). With a bottom anchor the stack grows upward,
            // so this keeps fresh feedback at a fixed spot and pushes
            // the older rows away from it — the other order would move
            // the newest toast every time one arrived.
            for toast in toasts.queue.iter() {
                egui::Frame::window(&ui.ctx().global_style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let colour = toast.kind.color(&crate::ui::theme::current(ui.ctx()));
                        crate::ui::affordances::status_dot(ui, colour);
                        // Dot AND glyph: the dot is the app's shared status
                        // idiom and reads fastest, the glyph is what
                        // survives a colour-blind reader (#1259 f236).
                        if let Some(glyph) = toast.kind.glyph() {
                            ui.colored_label(colour, glyph);
                        }
                        // Body, not `.small()`: this is the app's only
                        // "something just happened" channel and it was set
                        // in the smallest type on the screen (#1259 f243).
                        ui.add(egui::Label::new(&toast.text).wrap_mode(egui::TextWrapMode::Wrap));
                        if ui
                            .small_button(crate::ui::affordances::CROSS)
                            .on_hover_text("Dismiss")
                            .clicked()
                        {
                            dismissed = Some(toast.id);
                        }
                    });
                });
                ui.add_space(4.0);
            }
        });

    if let Some(id) = dismissed {
        toasts.dismiss(id);
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
