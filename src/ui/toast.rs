//! Rendering for the app-wide toast channel (#819): where the stack sits,
//! what a row looks like, and how a severity reads.
//!
//! The queue itself — [`crate::notify::Toasts`], its coalescing, bounding
//! and expiry — is NOT here (#1158). Feedback is cross-cutting: `network`,
//! `player`, `loading` and `terrain` all raise toasts and none of them
//! should have to depend on egui to do it. This module is only the half
//! that draws.
//!
//! **Where the stack sits has moved twice and both moves were about
//! attention.** The area is a real pointer area at `Order::Foreground`, so
//! wherever it sits it eats clicks: the top-RIGHT it started in is where
//! all five right-anchored windows open, and a stack of rows covered their
//! title bars for its full life (#1261 f43). The bottom-right it moved to
//! has no such neighbour but is easy to miss on a large display — the eye
//! is in the middle of the screen and the feedback was in a far corner
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
use crate::notify::{ToastKind, Toasts};

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
    if toasts.is_empty() {
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
            // `newest_first` carries the order AND the reason it is a
            // rule — see [`crate::notify::Toasts::newest_first`].
            for toast in toasts.newest_first() {
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
                        // The count a coalesced repeat carries (#1277
                        // f23). Only above 1, so an ordinary toast is
                        // unchanged, and weak-coloured: the badge says
                        // "again", it is not a second message.
                        if toast.repeats > 1 {
                            ui.label(
                                egui::RichText::new(format!("×{}", toast.repeats))
                                    .small()
                                    .color(crate::ui::theme::current(ui.ctx()).text_weak),
                            )
                            .on_hover_text("This message has repeated.");
                        }
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
}
