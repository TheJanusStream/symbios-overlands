//! Shared affordance idioms (#859): one add wording, one danger idiom,
//! one checkmark, one status dot — so the same intent always looks the
//! same.
//!
//! The #815 analysis found destructive actions rendered three ways
//! (red-fill "−", red text, plain menu rows), two checkmark glyphs, and
//! bespoke add wordings. The rules these helpers encode:
//!
//! * **Add** — labels start with `+ `; spell the verb (`+ Add point`)
//!   unless the noun is a type name (`+ Scatter`). No helper needed —
//!   the prefix is the idiom.
//! * **Remove (inline)** — a list row's remove control is
//!   [`remove_button`]: the small red-filled `−`. Big destructive
//!   actions (Discard, Reset, confirm dialogs) use
//!   [`crate::ui::confirm::danger_button`] — filled, white label.
//! * **Delete (menus)** — a context/tree menu's destructive row is
//!   [`danger_menu_button`]: error-red text, no fill (a filled button
//!   inside a menu reads as a different widget class), no `−` prefix —
//!   the colour is the signal.
//! * **Done/valid** — [`CHECK`] in `status.ok`, via [`ok_label`] for
//!   the common glyph+text case; failures pair with [`CROSS`]. Both are
//!   pinned to emoji-font-backed code points — see the constants' docs
//!   for the tofu story (#861).
//! * **Status dot** — [`status_dot`]: a *painted* circle, because the
//!   `●` glyph only exists in the monospace font (#861).

use bevy_egui::egui;

use crate::ui::theme;

/// THE checkmark. One glyph app-wide. `✔` (heavy check), NOT `✓`:
/// U+2713 exists in no font this app ships — not Noto Sans, not any of
/// egui's embedded faces — so every `✓` ever rendered was tofu (#861).
/// U+2714 lives in the embedded NotoEmoji/emoji-icon fallbacks.
pub const CHECK: &str = "✔";

/// THE cross/failure glyph, for the same reason: `✗`/`✕` exist in no
/// shipped font; `✖` (U+2716) renders via the emoji fallbacks.
pub const CROSS: &str = "✖";

/// A done/valid/saved label: `✔ text` in the theme's ok green.
pub fn ok_label(ui: &mut egui::Ui, text: impl std::fmt::Display) -> egui::Response {
    let ok = theme::current(ui.ctx()).status.ok;
    ui.colored_label(ok, format!("{CHECK} {text}"))
}

/// The inline remove control for list rows (material slots, sweep
/// points, placements, inventory items): a small red-filled `−` with a
/// white glyph. Pass the hover text naming what gets removed.
pub fn remove_button(ui: &mut egui::Ui, hover: &str) -> egui::Response {
    let th = theme::current(ui.ctx());
    ui.add(
        egui::Button::new(egui::RichText::new("−").color(egui::Color32::WHITE))
            .fill(th.danger_fill)
            .small(),
    )
    .on_hover_text(hover)
}

/// A destructive row inside a context/tree menu: error-red text, plain
/// background. Menus keep their uniform row look — colour alone marks
/// the danger, matching the #838 confirm treatment that follows the
/// click.
pub fn danger_menu_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let error = theme::current(ui.ctx()).status.error;
    ui.button(egui::RichText::new(label).color(error))
}

/// The status-colour dot that precedes badge/presence rows. PAINTED,
/// not a glyph: `●` (U+25CF) exists only in the embedded monospace
/// font, so as proportional text every dot in the app was tofu (#861).
pub fn status_dot(ui: &mut egui::Ui, color: egui::Color32) -> egui::Response {
    let size = ui.text_style_height(&egui::TextStyle::Body);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), size * 0.3, color);
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tofu glyphs must not sneak back in: U+2713/U+2717 exist in
    /// no font this app ships (#861) — the constants are the single
    /// source, pinned to the emoji-font-backed code points.
    #[test]
    fn glyph_constants_are_the_renderable_variants() {
        assert_eq!(CHECK, "\u{2714}");
        assert_eq!(CROSS, "\u{2716}");
    }
}
