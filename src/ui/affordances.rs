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

/// THE "this leaves the app" glyph (#1291). Appended to the label of
/// every control that hands the user off to a web browser, so a click
/// that backgrounds the app — or, on the web build, opens a second tab —
/// is never a surprise.
///
/// The login feed's "Open on Bluesky" card button had been spelling this
/// arrow inline since #896 and is the reason the code point was already
/// known good; it reads the constant now, so there is one definition
/// rather than a convention.
///
/// `↗` (U+2197) rather than a box-and-arrow: it is in the Arrows block
/// Noto Sans covers, and
/// `fonts::glyph_coverage_tests::every_ui_label_glyph_is_in_the_base_font_set`
/// walks every literal in `src/ui`, so a code point that would tofu fails
/// the gate rather than shipping invisible (#861's lesson, three times
/// over).
pub const EXTERNAL: &str = "↗";

/// Open `url` in the user's browser.
///
/// Lived in `ui::login::posts` until #1291, where it served the "Create a
/// free Bluesky account" link alone. It is a cross-surface concern now —
/// the Feedback affordance is on the login screen AND in the account menu
/// — and a private helper in a feed module is not a home for it.
///
/// **The outcome is deliberately not reported.** On the web build this is
/// `window.open(_, "_blank")`, which a popup blocker may refuse silently;
/// on native `webbrowser::open` can fail with no user-visible sign. This
/// is the review's "reports a success it cannot know" shape (#1274 f186),
/// and the answer there was the same: do not claim it worked. Callers
/// that need a fallback should offer the URL through `ClipboardQueue`,
/// the way "Copy login URL" does (#1234 f8).
pub fn open_url_in_browser(url: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = webbrowser::open(url);
    }
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            let _ = window.open_with_url_and_target(url, "_blank");
        }
    }
}

/// THE control that leaves the app for a web page: `label ↗`, which opens
/// `url` on a click (#1291).
///
/// One definition, so the glyph, the hover and the "opens in your browser"
/// promise cannot drift between the surfaces that use it — the same reason
/// [`CHECK`] and [`CROSS`] are constants. `hover` says what is on the far
/// end; the helper appends where it opens, because that half is the same
/// sentence everywhere and a caller should not have to remember it.
///
/// Returns the `egui::Response` rather than a `bool` so a caller can chain
/// its own decoration or close a menu — the lesson `fp_slider` and
/// `color_picker` each learned separately (#1233 f264, #1268).
pub fn external_link_button(
    ui: &mut egui::Ui,
    label: &str,
    url: &str,
    hover: &str,
) -> egui::Response {
    // `Extend`, never wrap (#1290): these sit in anchored auto-sized
    // areas and in menus, both of which offer a width derived from last
    // pass's measurement, so a wrapping label can ratchet itself narrow
    // and never recover.
    let response = ui.add(
        egui::Button::new(format!("{label} {EXTERNAL}")).wrap_mode(egui::TextWrapMode::Extend),
    );
    if response.clicked() {
        open_url_in_browser(url);
    }
    response.on_hover_text(format!("{hover}\nOpens in your browser."))
}

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
        assert_eq!(EXTERNAL, "\u{2197}");
    }

    /// The Feedback board is addressed by DID and rkey, never by handle
    /// (#1291).
    ///
    /// `userinput.app` is an ATProto app: the space is an
    /// `app.userinput.space` record, so its canonical address is the
    /// owner's DID plus the record key. A handle-shaped URL would look
    /// tidier and would break the day the owner changes handle — which is
    /// exactly the substitution this project's own naming ladder exists to
    /// prevent (`PeerLabel`, #1218 f299).
    #[test]
    fn the_feedback_link_is_addressed_by_did_not_handle() {
        let url = crate::config::ui::FEEDBACK_URL;
        assert!(url.starts_with("https://"), "{url}");
        assert!(
            url.contains("/s/did:plc:"),
            "the space is DID-addressed: {url}"
        );
        assert!(
            !url.contains('@') && !url.contains(".bsky.social"),
            "a handle in the address is the thing that rots: {url}"
        );
        // A record key follows the DID, so the address names one space.
        let rkey = url.rsplit('/').next().unwrap_or_default();
        assert!(!rkey.is_empty() && !rkey.starts_with("did:"), "{url}");
    }

    /// The Feedback affordance is on BOTH surfaces the owner asked for,
    /// and every one of them goes through [`external_link_button`]
    /// (#1291).
    ///
    /// The requirement was "on the login screen as well as when logged
    /// in", and neither half is derivable from the other — a refactor can
    /// drop one and leave a codebase that still compiles, still passes
    /// every other test, and quietly offers feedback from one place. So
    /// the pair is asserted, by file.
    ///
    /// Routing is asserted too, because the value of a shared idiom is
    /// that the glyph, the hover's "Opens in your browser" promise and the
    /// wrap mode cannot drift between call sites. A third site that spells
    /// its own `Button` + `open_url_in_browser` would look right and be a
    /// fourth spelling.
    #[test]
    fn the_feedback_affordance_is_on_both_surfaces_through_the_one_idiom() {
        let surfaces = [
            ("the login screen", "src/ui/login/mod.rs"),
            ("the account menu", "src/ui/toolbar.rs"),
        ];
        for (name, path) in surfaces {
            let source = std::fs::read_to_string(path).expect("source is readable");
            let source = crate::ui::fonts::glyph_coverage_tests::non_test_source(&source);
            let at = source
                .find("FEEDBACK_URL")
                .unwrap_or_else(|| panic!("{name} ({path}) no longer offers Feedback"));
            // The call is `external_link_button(ui, label, URL, hover)`,
            // so the helper's name precedes the constant in the same
            // expression. Look back a short way rather than at the whole
            // file, which would pass on any unrelated use of the helper.
            let lead = &source[at.saturating_sub(200)..at];
            assert!(
                lead.contains("external_link_button"),
                "{name} ({path}) opens the feedback URL without going through \
                 `external_link_button`"
            );
        }
    }
}
