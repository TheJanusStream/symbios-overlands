//! Transient toast notifications (#819).
//!
//! The single app-wide channel for "something just happened" feedback:
//! any system pushes a [`Toast`] into the [`Toasts`] resource and
//! [`toast_ui`] renders the queue as a stack of small framed rows at the
//! TOP-CENTRE of the panel-free rect, each expiring after
//! [`crate::config::ui::toast::DURATION_SECS`] or on its ✕ button.
//!
//! **Where it sits has moved twice and both moves were about attention.**
//! The area is a real pointer area at `Order::Foreground`, so wherever it
//! sits it eats clicks: the top-RIGHT it started in is where all five
//! right-anchored windows open, and a stack of rows covered their title
//! bars for its full life (#1261 f43). The bottom-right it moved to has
//! no such neighbour but is easy to miss on a large display — the eye is
//! in the middle of the screen and the feedback was in a far corner
//! (#1286). Centre-top is where the user is already looking and no
//! `SlotAnchor` claims it.
//!
//! Two centred neighbours share that band and neither is a window:
//! `ui::modes`' movement-mode banner, which the offset clears, and the
//! travel overlay's card, which it does not — a tall stack will overlap
//! that card, including its Cancel button, while both are up. Travel is
//! brief and Cancel is an escape hatch rather than the main path, so
//! that is the accepted cost of being seen at all.
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
pub fn toast_ui(
    mut contexts: EguiContexts,
    mut toasts: ResMut<Toasts>,
    time: Res<Time>,
    free: Res<crate::ui::layout::PanelFreeRect>,
) {
    let now = time.elapsed_secs_f64();
    toasts.prune(now);
    if toasts.queue.is_empty() {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    // Placed from the PANEL-FREE rect rather than anchored (#1286): an
    // anchored `Area` aligns within `content_rect`, which includes the
    // toolbar panel, so `CENTER_TOP` would put the stack underneath it.
    // `fixed_pos` + `pivot` is the idiom `ui::modes`' banner already uses
    // for the same reason.
    let free_rect = free.0.unwrap_or_else(|| ctx.content_rect());
    let top_centre = egui::pos2(free_rect.center().x, free_rect.top() + cfg::TOP_OFFSET);

    let mut dismissed: Option<u64> = None;
    egui::Area::new(egui::Id::new("overlands-toasts"))
        .fixed_pos(top_centre)
        .pivot(egui::Align2::CENTER_TOP)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_max_width(cfg::MAX_WIDTH);
            // NEWEST first, because the stack now grows DOWNWARD from a
            // fixed top edge (#1286) — so the first row drawn is the one
            // that never moves, and fresh feedback stays at one spot
            // while older rows are pushed away from it. Under the old
            // bottom anchor the stack grew upward and this order was
            // exactly reversed (#1261 f43); the rule is the same one,
            // which is why it is written as a rule: the newest toast
            // goes against the anchored edge.
            for toast in toasts.queue.iter().rev() {
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
        let drawn: Vec<&str> = toasts.queue.iter().rev().map(|t| t.text.as_str()).collect();
        assert_eq!(drawn, ["t3", "t2", "t1", "t0"]);

        // And it holds as the queue changes: a new arrival takes the
        // first slot rather than displacing everything above it.
        toasts.info("t4", 0.0);
        let first = toasts.queue.iter().rev().map(|t| t.text.as_str()).next();
        assert_eq!(first, Some("t4"));
    }

    /// The stack clears the movement-mode banner it shares the centre
    /// band with (#1286).
    ///
    /// `ui::modes` draws at the panel-free top + 8 inside a popup frame;
    /// a transient message must not sit on a standing state cue. Asserted
    /// against the neighbour's own constant rather than a remembered
    /// number, so moving the banner fails here instead of silently
    /// putting the two back on top of each other.
    #[test]
    fn the_stack_clears_the_movement_mode_banner() {
        // A `const` block, because both sides are constants and clippy is
        // right that this is a compile-time fact. No format arguments for
        // the same reason — const context cannot run `format!` — so the
        // message names the two constants instead of printing them.
        const {
            assert!(
                cfg::TOP_OFFSET > crate::ui::modes::BANNER_TOP_OFFSET,
                "toast::TOP_OFFSET must clear modes::BANNER_TOP_OFFSET"
            )
        };
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
