//! Transient toast notifications (#819).
//!
//! The single app-wide channel for "something just happened" feedback:
//! any system pushes a [`Toast`] into the [`Toasts`] resource and
//! [`toast_ui`] renders the queue as a stack of small framed rows
//! anchored to the top-right of the screen (below the toolbar), each
//! expiring after [`crate::config::ui::toast::DURATION_SECS`] or on its
//! ✕ button.
//!
//! Before this existed every surface hand-rolled its own transient
//! status (`Local<Option<(String, f64)>>` pairs in the Diagnostics
//! window) or — far more commonly — reported nothing at all: portal
//! failures, gift outcomes, and placement no-ops were silent. Those
//! flows migrate onto this channel issue by issue; the Diagnostics
//! landmark-copy and log-export statuses are the founding consumers.
//!
//! Severity colours reuse the diagnostics map
//! ([`crate::ui::diagnostics::severity_color`]) so a warning reads the
//! same amber here as in the event log and anomaly badges; `Success`
//! keeps the exact green the migrated Diagnostics toasts used. No new
//! palette is invented — consolidation belongs to the theming epic
//! (#816).

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::config::ui::toast as cfg;
use crate::diagnostics::event::Severity;

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
    /// Dot colour from the active theme (#856): the Success green is the
    /// semantic ok (was its own config green), the rest ride the same
    /// severity ramp as the diagnostics HUD.
    fn color(self, th: &crate::ui::theme::Theme) -> egui::Color32 {
        match self {
            ToastKind::Info => th.status.severity(Severity::Info),
            ToastKind::Success => th.status.ok,
            ToastKind::Warn => th.status.severity(Severity::Warn),
            ToastKind::Error => th.status.severity(Severity::Error),
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

impl Toasts {
    /// Queue a toast. `now` is `Time::elapsed_secs_f64` — passed in
    /// rather than read here so the queue logic stays unit-testable.
    pub fn push(&mut self, kind: ToastKind, text: impl Into<String>, now: f64) {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.queue.push(Toast {
            kind,
            text: text.into(),
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
        .anchor(egui::Align2::RIGHT_TOP, cfg::ANCHOR_OFFSET)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_max_width(cfg::MAX_WIDTH);
            // Newest first, so fresh feedback lands where the eye
            // already is (directly under the toolbar).
            for toast in toasts.queue.iter().rev() {
                egui::Frame::window(&ui.ctx().style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        crate::ui::affordances::status_dot(
                            ui,
                            toast.kind.color(&crate::ui::theme::current(ui.ctx())),
                        );
                        ui.add(
                            egui::Label::new(egui::RichText::new(&toast.text).small())
                                .wrap_mode(egui::TextWrapMode::Wrap),
                        );
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
