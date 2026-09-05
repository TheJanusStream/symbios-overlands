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
//!   inside a menu reads as a different widget class), prefixed with
//!   [`CROSS`].
//!
//!   **This reverses a recorded decision, deliberately (#1260 f247).**
//!   #815/#859 settled on "no `−` prefix — the colour is the signal",
//!   and colour alone is not a signal: under deuteranopia or protanopia
//!   the dark palette's `status.error` desaturates toward the same grey
//!   as the neutral rows beside it, so for roughly 8% of male users the
//!   one irreversible row in a menu looked exactly like the rest of it
//!   (WCAG 1.4.1). The reasoning behind the original decision survives
//!   intact and is still honoured — a menu row must not turn into a
//!   filled button — and [`remove_button`] had already paired its fill
//!   with a glyph, so the codebase owned the redundant-cue pattern and
//!   was declining to use it in the one place the actions are
//!   irreversible. `✖` and not `−`: the glyph says *delete*, and `−`
//!   is [`remove_button`]'s, which means *take out of this list*.
//! * **Done/valid** — [`CHECK`] in `status.ok`, via [`ok_label`] for
//!   the common glyph+text case; failures pair with [`CROSS`] and
//!   cautions with [`WARNING`]. All three are pinned to font-backed
//!   code points — see the constants' docs for the tofu story (#861).
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

/// THE caution glyph, completing the trio (#1259 f236). `⚠` (U+26A0)
/// was already proven drawable by the #1257 hosted-editor guard, and
/// every literal in `src/ui` goes through
/// `fonts::tests::every_ui_label_glyph_is_in_the_base_font_set`, so
/// this one is covered twice over.
///
/// There is deliberately NO info glyph. `ToastKind::Info` carries no
/// verdict, and inventing a fourth code point is how `✓`, `●` and `◈`
/// each shipped as tofu (#861, #1105).
pub const WARNING: &str = "⚠";

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

/// A destructive row inside a context/tree menu: error-red text and a
/// [`CROSS`], plain background. Menus keep their uniform row look — no
/// fill, matching the #838 confirm treatment that follows the click —
/// but the danger is carried by a shape as well as a hue, so it is
/// perceivable without colour vision. See the module docs for why this
/// reverses #815/#859.
///
/// One edit covers every call site: the in-world context menu's "Take
/// off", per-kind delete and "Delete placement", and the generator
/// tree's "Delete".
pub fn danger_menu_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let error = theme::current(ui.ctx()).status.error;
    ui.button(egui::RichText::new(danger_menu_label(label)).color(error))
}

/// The text [`danger_menu_button`] draws — pure, so the redundant cue is
/// a fact a test can hold rather than a line of render code nothing can
/// see (#1260 f247).
fn danger_menu_label(label: &str) -> String {
    format!("{CROSS} {label}")
}

/// A hover tooltip that a KEYBOARD user can also reach (#1260 f240).
///
/// egui's tooltip gate is purely pointer-driven — `should_show_tooltip`
/// reads hover position, movement, scroll and click timings and has no
/// `has_focus()` path anywhere in it — so every control whose meaning
/// lives only in `on_hover_text` is unlabelled to anyone tabbing
/// through. Use this instead of `on_hover_text` wherever the hover
/// carries meaning that exists nowhere else on screen.
///
/// It does not make hover-only text acceptable: a control whose PURPOSE
/// is only in a tooltip should get a visible label. This is for the
/// elaboration that follows one.
pub fn hint(response: egui::Response, text: &str) -> egui::Response {
    let response = response.on_hover_text(text);
    if response.has_focus() {
        response.show_tooltip_text(text);
    }
    response
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

/// The one way this app builds a text field (#1284).
///
/// Wrapping `ui.add` rather than a fresh constructor, so every builder
/// chain a call site already has — `desired_width`, `hint_text`,
/// `text_color`, `code_editor`, `font` — keeps working unchanged, and the
/// returned `egui::Response` is the field's own.
///
/// **What it does:** paints the focus ring from
/// [`crate::ui::theme::focus_ring`] instead of `selection_text`. egui
/// reads `visuals.selection.stroke` off the `Ui` the widget is added to,
/// so the override is set on this `Ui`, the widget is added, and the
/// previous value is put straight back — every selected chip in the app
/// keeps its label colour, which is the other role that same field
/// carries upstream. Deliberately NOT `ui.scope`: a child `Ui` derives a
/// different auto id, and these fields' focus and cursor state is keyed
/// on it.
///
/// Every text field goes through here, enforced by
/// `fonts::glyph_coverage_tests::the_only_text_fields_are_the_focusable_ones`
/// — the same shape `ui::num` uses for numeric widgets, and for the same
/// reason: a helper nobody is obliged to call fixes this once and loses
/// it at the next call site.
pub fn text_edit(ui: &mut egui::Ui, field: egui::TextEdit<'_>) -> egui::Response {
    text_edit_enabled(ui, true, field)
}

/// [`text_edit`] for a field that can be disabled — the gateway's
/// destination row greys itself out while a lookup is in flight.
///
/// A disabled field cannot take focus, so the ring override is inert
/// there; the variant exists so the source scan has nothing to make an
/// exception for.
pub fn text_edit_enabled(
    ui: &mut egui::Ui,
    enabled: bool,
    field: egui::TextEdit<'_>,
) -> egui::Response {
    let previous = ui.visuals().selection.stroke;
    let ring = theme::focus_ring(&theme::current(ui.ctx()));
    ui.visuals_mut().selection.stroke = ring;
    let response = ui.add_enabled(enabled, field);
    ui.visuals_mut().selection.stroke = previous;
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tofu glyphs must not sneak back in: U+2713/U+2717 exist in
    /// no font this app ships (#861) — the constants are the single
    /// source, pinned to the emoji-font-backed code points.
    /// #1260 f247: a destructive menu row must carry a SHAPE, not only a
    /// hue.
    ///
    /// This assertion is the reversal of a written decision (#815/#859
    /// chose colour alone), so it is worth stating what would put the
    /// old behaviour back: dropping the prefix here. Under deuteranopia
    /// the dark palette's `status.error` desaturates toward the grey of
    /// the neutral rows beside it, and the actions behind this control
    /// are the irreversible ones.
    #[test]
    fn a_destructive_menu_row_is_not_signalled_by_colour_alone() {
        let label = danger_menu_label("Delete placement");
        assert!(
            label.starts_with(CROSS),
            "the danger row lost its glyph: {label}"
        );
        assert!(label.ends_with("Delete placement"), "{label}");
        // NOT the remove_button minus: `−` means "take out of this list",
        // `✖` means delete. Two idioms, two glyphs.
        assert!(!label.contains('−'), "{label}");
    }

    #[test]
    fn glyph_constants_are_the_renderable_variants() {
        assert_eq!(CHECK, "\u{2714}");
        assert_eq!(CROSS, "\u{2716}");
        assert_eq!(WARNING, "\u{26A0}");
    }
}
