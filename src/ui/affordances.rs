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
//! * **Done/valid** — [`CHECK`] (`✓`) in `status.ok`, via [`ok_label`]
//!   for the common glyph+text case. (`✔` is retired.)
//! * **Status dot** — [`status_dot`] for the `●`-in-a-status-colour
//!   pattern (anomaly badges, presence, toasts).

use bevy_egui::egui;

use crate::ui::theme;

/// THE checkmark. One glyph app-wide — the loading screen used `✔`
/// while everything else used `✓`.
pub const CHECK: &str = "✓";

/// A done/valid/saved label: `✓ text` in the theme's ok green.
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

/// The `●`-in-a-status-colour dot that precedes badge/presence rows.
pub fn status_dot(ui: &mut egui::Ui, color: egui::Color32) -> egui::Response {
    ui.colored_label(color, "●")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The retired glyph must not sneak back in: the constant is the
    /// single source for the app's checkmark.
    #[test]
    fn check_glyph_is_the_plain_checkmark() {
        assert_eq!(CHECK, "\u{2713}");
    }
}
