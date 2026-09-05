//! Semantic theme foundation (#855, epic #816).
//!
//! Before this module, the app had ZERO theming: `EguiPlugin::default()`
//! ran bare (stock egui dark), and ~120 scattered `Color32` literals
//! carried the UI's entire visual identity — the #815 analysis counted
//! four different ambers, five reds and seven greens standing in for the
//! same semantics. This module is the single source those call sites
//! migrate onto (the sweep is #856):
//!
//! * [`Theme`] — the semantic palette. Two colour groups are kept
//!   deliberately distinct: **status** ([`StatusPalette`]:
//!   ok/warn/error/info plus the diagnostics severity ramp) and
//!   **identity** (`accent*`: teal by decision 2026-07-17 — NOT the
//!   ok-green, so brand and status can never collide), plus surface
//!   tones (window/panel fills, chart fills, text roles, borders).
//! * [`CurrentTheme`] — the Bevy resource consumers read each frame.
//!   Swapping it re-applies everything ([`apply_theme_on_change`]),
//!   which is what makes #857's picker a one-line resource write.
//! * [`apply_theme`] — pushes the palette into egui `Visuals`/`Style`
//!   *and pins* `ThemePreference`: bevy_egui 0.39 never forwards the OS
//!   theme, but a future upgrade might — an explicit pin means an
//!   upgrade can't silently flip users into an un-designed mode.
//!
//! The dark palette codifies today's de-facto look: egui's stock dark
//! chrome, the most-used literal of each semantic family (amber
//! `210,170,90`, chrome red `220,90,90`, the `130,190,130` green
//! family), the diagnostics severity ramp from `config::ui::diagnostics`
//! (still the source of truth until #856 flips `severity_color()`), and
//! the `from_gray(24/28)` chart fills. Light and high-contrast palettes
//! land with the picker in #857.
//!
//! # Two rules #1258 added, both of them about *where* a colour is read
//!
//! **Measure the `Visuals`, not the `Theme`.** [`visuals_for`] is what
//! the renderer is handed; a [`Theme`] field is only a colour the
//! palette *offers*. The high-contrast palette shipped a `text_strong`
//! of `from_gray(255)` that an ordinary `ui.label()` could not reach —
//! the label rendered egui's inherited `from_gray(140)` — and its guard
//! passed, because the guard compared the field. Every colour test in
//! this module now goes through `visuals_for`.
//!
//! **`dist` is distinctness; [`contrast_ratio`] is legibility.** The
//! channel-sum `dist` in the tests answers "could these two hues be
//! confused" and is the right tool for accent-versus-status. It cannot
//! answer "can this be read": white on the light palette's selection
//! band scored 395 on `dist` and 3.46:1 on the screen.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::diagnostics::event::Severity;

/// Status colours — outcome semantics only. Never use these for brand
/// or chrome accents (that's [`Theme::accent`]); the separation is what
/// keeps "selected" and "healthy" readable as different things.
#[derive(Clone, Debug, PartialEq)]
pub struct StatusPalette {
    /// Success / saved / valid / present.
    pub ok: egui::Color32,
    /// Caution / fallback-in-use / slower-than-usual.
    pub warn: egui::Color32,
    /// Failure / invalid / destructive-outcome.
    pub error: egui::Color32,
    /// Neutral information (chat authors, counts, hints with no verdict).
    pub info: egui::Color32,
    /// Diagnostics severity ramp — [`Severity::Trace`] → quietest.
    pub trace: egui::Color32,
    /// [`Severity::Info`] tier of the ramp (brighter than `trace`,
    /// distinct from the general-purpose `info` blue).
    pub info_tier: egui::Color32,
    /// [`Severity::Warn`] tier — same hue family as `warn`.
    pub warn_tier: egui::Color32,
    /// [`Severity::Error`] tier.
    pub error_tier: egui::Color32,
    /// [`Severity::Critical`] tier — the loudest colour in the app.
    pub critical_tier: egui::Color32,
}

impl StatusPalette {
    /// Colour for a diagnostics [`Severity`] — the ramp
    /// `ui::diagnostics::severity_color` migrates onto in #856.
    pub fn severity(&self, sev: Severity) -> egui::Color32 {
        match sev {
            Severity::Trace => self.trace,
            Severity::Info => self.info_tier,
            Severity::Warn => self.warn_tier,
            Severity::Error => self.error_tier,
            Severity::Critical => self.critical_tier,
        }
    }
}

/// WCAG 2.1 relative luminance of an opaque sRGB colour.
///
/// The palette is authored in gamma-encoded sRGB (`Color32` bytes) and
/// every contrast threshold in the accessibility literature is defined
/// on the *linear* luminance underneath it. That gap is why the
/// channel-sum `dist` the guards used before #1258 could pass a
/// comparison the screen failed: `dist` is a fine test for "are these
/// two colours visibly different", and no test at all for "can this be
/// read".
fn relative_luminance(c: egui::Color32) -> f32 {
    let lin = |v: u8| {
        let s = v as f32 / 255.0;
        if s <= 0.040_45 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}

/// WCAG 2.1 contrast ratio between two opaque colours: 1.0 for two
/// identical colours, 21.0 for black on white.
///
/// This is the quantity every legibility threshold in this module is
/// written against — 4.5 for normal text (AA), 3.0 for large text and
/// for the boundary of a UI component (1.4.11), 7.0 for AAA. Both
/// arguments must be **opaque**: `Color32` is premultiplied, so a
/// translucent colour has to be composited over its background before
/// it can be measured (see the login hero's frame).
pub fn contrast_ratio(a: egui::Color32, b: egui::Color32) -> f32 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Composite a translucent (premultiplied) colour over an opaque one —
/// what the screen shows, and therefore the only thing
/// [`contrast_ratio`] may be handed.
///
/// The login hero's frame is `window_fill.gamma_multiply(0.85)`, so the
/// text on it reads against neither the palette's window fill nor the
/// backdrop behind it but against this blend (#1258 f237).
pub fn composite_over(src: egui::Color32, bg: egui::Color32) -> egui::Color32 {
    let inv = 1.0 - src.a() as f32 / 255.0;
    let mix = |s: u8, b: u8| (s as f32 + b as f32 * inv).round().clamp(0.0, 255.0) as u8;
    egui::Color32::from_rgb(
        mix(src.r(), bg.r()),
        mix(src.g(), bg.g()),
        mix(src.b(), bg.b()),
    )
}

/// Label colour for each of egui's five widget tiers (#1258).
///
/// egui derives *every* text emphasis from these and from nothing else:
/// a plain `ui.label()` paints [`Self::noninteractive`]
/// (`Visuals::text_color`), a button at rest paints [`Self::inactive`],
/// and `ui.strong()` paints [`Self::active`]
/// (`Visuals::strong_text_color`) — there is no bold face in the
/// bundled font, so colour is the whole of the emphasis.
///
/// Before #1258 [`apply_theme`] rewrote these only when the base was
/// `egui::Theme::Light`, so the high-contrast palette — a *dark*-based
/// one — shipped egui's stock `from_gray(140)` body text on its
/// near-black window: 5.89:1, against 5.12:1 for the same label in the
/// dark palette. The palette that exists for low-vision use improved
/// the text that makes up most of the UI by 0.77, and its guard passed
/// because it compared [`Theme::text_strong`], which an ordinary label
/// never consults. Every palette now states all five tiers outright.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WidgetText {
    /// Ordinary body text — `ui.label()`, and the fill-free
    /// "noninteractive" tier egui paints most static text with.
    pub noninteractive: egui::Color32,
    /// A button, checkbox or text field at rest.
    pub inactive: egui::Color32,
    /// The same widget under the pointer.
    pub hovered: egui::Color32,
    /// The same widget while pressed — **and** `ui.strong()` everywhere,
    /// which is why this must stay distinct from
    /// [`Self::noninteractive`] or section headings lose their emphasis.
    pub active: egui::Color32,
    /// An open combo box / menu root.
    pub open: egui::Color32,
}

/// The full semantic palette. Fields are `pub` — consumers read roles
/// directly (`theme.status.ok`, `theme.accent`) rather than through
/// getters, keeping call sites as short as the literals they replace.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub status: StatusPalette,
    /// Identity accent (teal): hyperlinks, selection, wordmark, focus.
    pub accent: egui::Color32,
    /// Filled call-to-action background — the login "Enter the
    /// Overlands" button. Bright teal on the dark bases, deep teal on
    /// light; [`Self::accent_fill_text`] carries the matching inverted
    /// label colour.
    pub accent_fill: egui::Color32,
    /// Label colour on every accent-teal fill (#857 follow-up): the CTA
    /// button AND selected chips (toolbar toggles, tabs, gizmo
    /// World/Local — egui paints selected-widget text with
    /// `selection.stroke`). Dark bases run bright fills with near-black
    /// text, the light base deeper fills with white text.
    pub accent_fill_text: egui::Color32,
    /// Selected-chip / text-selection background. Mid-toned in every
    /// palette: bright enough that [`Self::selection_text`] reads on a
    /// selected chip, dim enough that body text survives over a
    /// text-selection band (one egui knob covers both).
    pub selection_fill: egui::Color32,
    /// Label colour on [`Self::selection_fill`] — egui paints a selected
    /// widget's text with `selection.stroke`, so this is what every
    /// selected toolbar toggle, tab and gizmo chip reads as.
    ///
    /// Split from [`Self::accent_fill_text`] by #1258: the CTA button's
    /// fill and the selection band are different colours, and forcing
    /// one label colour onto both left the light palette's white chip
    /// text at 3.46:1. Dark bases still invert to near-black; light
    /// takes the palette's own strong text.
    pub selection_text: egui::Color32,
    /// Destructive filled-button background (white text) — the shared
    /// danger idiom (`ui::confirm::danger_button`).
    pub danger_fill: egui::Color32,
    /// Danger *banner* background (critical-anomaly strip, record-recovery
    /// banners) — darker than `danger_fill` so a persistent surface
    /// doesn't scream like a button.
    pub danger_surface: egui::Color32,
    /// Readable pale text on [`Self::danger_surface`].
    pub danger_surface_text: egui::Color32,
    /// Floating-window background.
    pub window_fill: egui::Color32,
    /// Top/side panel background (toolbar).
    pub panel_fill: egui::Color32,
    /// Text-field interior (egui's `extreme_bg_color`). #1258: nothing
    /// wrote this before, so a `TextEdit` inherited the base's stock
    /// value — `from_gray(10)` on the high-contrast palette's
    /// `from_gray(10)` window, a field at 1.00:1 with `Stroke::NONE`
    /// around it. The edge comes from [`Self::border`], which
    /// [`apply_theme`] now installs as the resting widget stroke.
    pub field_fill: egui::Color32,
    /// Chart/plot background fill (diagnostics histograms).
    pub chart_fill: egui::Color32,
    /// Deeper chart fill for nested/inset plot areas.
    pub chart_fill_deep: egui::Color32,
    /// Primary readable text.
    pub text_strong: egui::Color32,
    /// De-emphasised text (hints, timestamps, reasons).
    pub text_weak: egui::Color32,
    /// Quietest text tier (fire-count markers, dividers-with-words) —
    /// present but deliberately easy to skip over. Still a *readable*
    /// tier: #1258 holds it to WCAG's 3:1 non-text floor, because both
    /// its consumers (the muted-peer dot, the anomaly fire count) carry
    /// state rather than decoration.
    pub text_faint: egui::Color32,
    /// Label colour per egui widget tier — see [`WidgetText`]. The
    /// single most load-bearing role in the palette: it is what an
    /// ordinary `ui.label()` renders as.
    pub widget_text: WidgetText,
    /// Window and separator strokes.
    pub border: egui::Color32,
    /// The edge a CONTROL draws around itself — a resting text field, a
    /// button, a combo box (#1283).
    ///
    /// Separate from [`Self::border`], which is window chrome, because
    /// the two draw on different grounds and one value cannot serve both.
    /// In Dark they were the same value by accident and that accident is
    /// the whole history of this role: `border` is `from_gray(60)`, which
    /// is *exactly* the stock dark button fill, so the edge #1258 f233
    /// gave every control was invisible on every button in the default
    /// palette while being perfectly visible on the text field beside it.
    ///
    /// Held to **3:1 against the surface it encloses** —
    /// `every_palette_gives_a_resting_control_a_visible_edge` — which is
    /// WCAG 1.4.11's floor for a non-text boundary.
    pub control_border: egui::Color32,
    /// Login-screen backdrop gradient, top edge (zenith). Painted as a
    /// full-screen vertical gradient behind the login cards so the
    /// pre-world screen reads as a sky, not a flat clear-colour void.
    pub backdrop_top: egui::Color32,
    /// Login-screen backdrop gradient, bottom edge (horizon).
    pub backdrop_bottom: egui::Color32,
    /// Which egui base visuals this palette is built over — pinned into
    /// `ThemePreference` by [`apply_theme`] so an OS-theme-forwarding
    /// bevy_egui upgrade can never flip the mode out from under the
    /// palette.
    pub egui_base: egui::Theme,
    /// Window/separator stroke width — the high-contrast palette widens
    /// it so window edges register without hunting.
    pub border_stroke_width: f32,
}

/// Which shipped palette the user picked (#857). Persisted in
/// [`crate::state::LocalSettings`] → prefs; `theme::sync_theme_from_settings`
/// keeps [`CurrentTheme`] following it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum UserTheme {
    #[default]
    Dark,
    Light,
    HighContrast,
}

impl UserTheme {
    /// Picker label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::HighContrast => "High contrast",
        }
    }

    /// Build the palette this preference names.
    pub fn theme(self) -> Theme {
        match self {
            Self::Dark => Theme::dark(),
            Self::Light => Theme::light(),
            Self::HighContrast => Theme::high_contrast(),
        }
    }
}

/// Keep [`CurrentTheme`] following the persisted preference. Guarded
/// write, so the resource change (and the egui re-apply it triggers)
/// fires only when the pick actually differs — including the startup
/// frame where the prefs load swaps `LocalSettings` in.
pub fn sync_theme_from_settings(
    settings: Res<crate::state::LocalSettings>,
    mut current: ResMut<CurrentTheme>,
) {
    let want = settings.theme.theme();
    if current.0 != want {
        current.0 = want;
    }
}

impl Theme {
    /// The shipped dark palette — codifies the app's de-facto look (see
    /// module docs for the provenance of each value).
    pub fn dark() -> Self {
        use crate::config::ui::diagnostics as sev_cfg;
        let ramp = |rgb: [u8; 3]| egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        Self {
            status: StatusPalette {
                ok: egui::Color32::from_rgb(130, 200, 130),
                warn: egui::Color32::from_rgb(210, 170, 90),
                // A touch redder than the historical (220,90,90) so the
                // error/warn pair clears the distinctness guard.
                error: egui::Color32::from_rgb(225, 85, 85),
                // Pushed toward true blue (away from cyan) so the info
                // tint can never be mistaken for the teal accent.
                info: egui::Color32::from_rgb(100, 160, 240),
                trace: ramp(sev_cfg::SEVERITY_TRACE_RGB),
                info_tier: ramp(sev_cfg::SEVERITY_INFO_RGB),
                warn_tier: ramp(sev_cfg::SEVERITY_WARN_RGB),
                error_tier: ramp(sev_cfg::SEVERITY_ERROR_RGB),
                critical_tier: ramp(sev_cfg::SEVERITY_CRITICAL_RGB),
            },
            accent: egui::Color32::from_rgb(72, 199, 208),
            // Bright fill + near-black label (#857 follow-up): the old
            // dark-teal fill with white text read muddy.
            accent_fill: egui::Color32::from_rgb(64, 190, 200),
            accent_fill_text: egui::Color32::from_gray(8),
            selection_fill: egui::Color32::from_rgb(40, 165, 175),
            selection_text: egui::Color32::from_gray(8),
            danger_fill: egui::Color32::from_rgb(160, 40, 40),
            // Unifies the (90,20,20) critical-anomaly strip and the
            // (90,30,30) recovery banners onto one surface.
            danger_surface: egui::Color32::from_rgb(90, 25, 25),
            danger_surface_text: egui::Color32::from_rgb(255, 210, 210),
            window_fill: egui::Color32::from_gray(27),
            panel_fill: egui::Color32::from_gray(27),
            field_fill: egui::Color32::from_gray(10),
            chart_fill: egui::Color32::from_gray(28),
            chart_fill_deep: egui::Color32::from_gray(24),
            text_strong: egui::Color32::from_gray(220),
            text_weak: egui::Color32::from_gray(140),
            // Raised from 96 by #1258 f242: 96 measured 2.74:1 against
            // the window, under WCAG's 3:1 floor for a non-text cue,
            // and both consumers (the muted-peer dot, the anomaly fire
            // count) report state. 112 is 3.48:1 — the one value in
            // this palette #1258 moved.
            text_faint: egui::Color32::from_gray(112),
            // Exactly egui's stock dark tiers: the dark look #857
            // validated IS this tiering, and stating it here rather
            // than inheriting it is the whole of the #1258 fix — the
            // palette, not the base, now decides.
            widget_text: WidgetText {
                noninteractive: egui::Color32::from_gray(140),
                inactive: egui::Color32::from_gray(180),
                hovered: egui::Color32::from_gray(240),
                active: egui::Color32::WHITE,
                open: egui::Color32::from_gray(210),
            },
            border: egui::Color32::from_gray(60),
            // 105 and not `border`'s 60: 60 IS the stock dark button
            // fill, so it draws nothing at all on a button and 1.80:1 on
            // a field. 105 clears 3:1 on both grounds a control has —
            // 3.61:1 on the field's `from_gray(10)` interior and 3.14:1
            // on the `from_gray(27)` window — while staying quiet enough
            // that the #857-validated dark look is not turned into an
            // outlined-control theme (#1283). The first attempt here was
            // 95, which cleared the field at 3.30:1 and missed the window
            // at 2.70:1: the interior is the darker of the two grounds,
            // so checking only the field is checking the easy one.
            control_border: egui::Color32::from_gray(105),
            // Night-sky slate falling toward a teal-tinged horizon — the
            // horizon hue is a desaturated cousin of the accent so the
            // backdrop and the CTA read as one family.
            backdrop_top: egui::Color32::from_rgb(14, 22, 33),
            backdrop_bottom: egui::Color32::from_rgb(56, 86, 102),
            egui_base: egui::Theme::Dark,
            border_stroke_width: 1.0,
        }
    }

    /// The light palette (#857): every dark-assuming role re-derived for
    /// pale surfaces — status hues deepened so they hold AA-ish contrast
    /// on near-white, accent deepened per the 2026-07-17 decision.
    pub fn light() -> Self {
        Self {
            status: StatusPalette {
                ok: egui::Color32::from_rgb(30, 130, 50),
                // Deepened from (165,115,10) by #1258 f241: 3.84:1 on
                // `window_fill`, and it carries the "Saving to PDS…"
                // publish line and the peer build-mismatch chip.
                warn: egui::Color32::from_rgb(145, 98, 0),
                error: egui::Color32::from_rgb(185, 35, 35),
                info: egui::Color32::from_rgb(25, 95, 200),
                // Ramp widened by #1259 f236, same rule as the dark one:
                // the three alarm tiers clear the distinctness bar and
                // fall monotonically in luminance (0.166 → 0.095 →
                // 0.040 — on a pale ground the heaviest tier is the
                // darkest), so severity survives greyscale. `trace` also
                // rises off gray-150 (2.4:1) to clear the 3:1 floor.
                trace: egui::Color32::from_gray(118),
                info_tier: egui::Color32::from_gray(70),
                warn_tier: egui::Color32::from_rgb(150, 105, 0),
                error_tier: egui::Color32::from_rgb(170, 30, 0),
                critical_tier: egui::Color32::from_rgb(120, 0, 20),
            },
            // Both deepened by #1258 f241 to clear AA on `window_fill`:
            // the old (0,130,140) hyperlink/wordmark measured 4.25:1 and
            // its fill 5.38:1 under white.
            accent: egui::Color32::from_rgb(0, 115, 125),
            accent_fill: egui::Color32::from_rgb(0, 105, 115),
            accent_fill_text: egui::Color32::WHITE,
            // Mid-deep, and read with DARK text (`selection_text`): the
            // white chip label #857 shipped measured 3.46:1 here, and
            // deepening the band far enough to carry white would have
            // cost the body text drawn over a text-selection run.
            selection_fill: egui::Color32::from_rgb(60, 150, 160),
            selection_text: egui::Color32::from_gray(18),
            danger_fill: egui::Color32::from_rgb(175, 45, 45),
            danger_surface: egui::Color32::from_rgb(250, 218, 218),
            danger_surface_text: egui::Color32::from_rgb(120, 20, 20),
            window_fill: egui::Color32::from_gray(246),
            panel_fill: egui::Color32::from_gray(246),
            field_fill: egui::Color32::from_gray(255),
            chart_fill: egui::Color32::from_gray(235),
            chart_fill_deep: egui::Color32::from_gray(225),
            text_strong: egui::Color32::from_gray(18),
            text_weak: egui::Color32::from_gray(90),
            text_faint: egui::Color32::from_gray(118),
            // #1258 f238: #857's follow-up pulled ALL five tiers onto
            // `text_strong`, which made `ui.strong()` byte-identical to
            // `ui.label()` — egui reads emphasis off `active` alone, so
            // every section heading in the light palette lost it. Body
            // text is a deliberate 50 rather than 18: still far darker
            // than egui's stock 80 (the paleness #857 was fixing) and
            // far enough from black that the strong tier reads as
            // emphasis, 11.86:1 against 19.43:1.
            widget_text: WidgetText {
                noninteractive: egui::Color32::from_gray(50),
                inactive: egui::Color32::from_gray(40),
                hovered: egui::Color32::from_gray(20),
                active: egui::Color32::BLACK,
                open: egui::Color32::from_gray(20),
            },
            // Deepened from 190 (#1258 f233/f237): this is the resting
            // edge of every text field and button as well as the login
            // card's stroke, and at 190 it measured 1.72:1 against the
            // window. 140 is the lightest grey that clears WCAG
            // 1.4.11's 3:1 boundary against BOTH the window (3.11:1)
            // and a field's white interior (3.36:1).
            border: egui::Color32::from_gray(140),
            // The same value serves here: 3.36:1 on the white field and
            // 3.08:1 against the window behind a button (#1283).
            control_border: egui::Color32::from_gray(140),
            // Daylight sky falling to a pale near-white horizon.
            backdrop_top: egui::Color32::from_rgb(128, 168, 198),
            backdrop_bottom: egui::Color32::from_rgb(233, 240, 244),
            egui_base: egui::Theme::Light,
            border_stroke_width: 1.0,
        }
    }

    /// The high-contrast palette (#857): a dark base pushed to the
    /// extremes — near-black surfaces, near-white primary text, brighter
    /// status hues and a wider window stroke, for low-vision use and
    /// harsh ambient light.
    pub fn high_contrast() -> Self {
        Self {
            status: StatusPalette {
                ok: egui::Color32::from_rgb(90, 255, 120),
                warn: egui::Color32::from_rgb(255, 200, 60),
                error: egui::Color32::from_rgb(255, 90, 80),
                info: egui::Color32::from_rgb(130, 190, 255),
                trace: egui::Color32::from_gray(170),
                info_tier: egui::Color32::from_gray(255),
                // #1259 f236: (255,200,60) / (255,145,90) / (255,80,80)
                // were 85 and 75 apart in channel-sum — three shades of
                // the same alarm on the palette that exists for people
                // who cannot resolve small colour differences.
                warn_tier: egui::Color32::from_rgb(255, 215, 70),
                error_tier: egui::Color32::from_rgb(255, 140, 40),
                critical_tier: egui::Color32::from_rgb(255, 70, 90),
            },
            accent: egui::Color32::from_rgb(90, 240, 250),
            accent_fill: egui::Color32::from_rgb(90, 240, 250),
            accent_fill_text: egui::Color32::BLACK,
            selection_fill: egui::Color32::from_rgb(30, 170, 185),
            selection_text: egui::Color32::BLACK,
            danger_fill: egui::Color32::from_rgb(205, 40, 40),
            danger_surface: egui::Color32::from_rgb(120, 15, 15),
            danger_surface_text: egui::Color32::from_rgb(255, 230, 230),
            window_fill: egui::Color32::from_gray(10),
            panel_fill: egui::Color32::from_gray(10),
            // Lifted off `window_fill` so a field is at least a surface
            // and not a void. Only 1.16:1, and deliberately not more:
            // a fill light enough to clear 3:1 against a `from_gray(10)`
            // window is a mid grey, and egui draws hint text at
            // `text_color().gamma_multiply(0.6)` — around gray-135 here
            // — which would land at about 1.4:1 on it. The contrast a
            // field needs has to come from its edge (#1283), not from
            // making its interior pale enough to swallow the hint.
            field_fill: egui::Color32::from_gray(28),
            chart_fill: egui::Color32::from_gray(20),
            chart_fill_deep: egui::Color32::from_gray(14),
            text_strong: egui::Color32::from_gray(255),
            text_weak: egui::Color32::from_gray(200),
            text_faint: egui::Color32::from_gray(160),
            // The point of #1258 f232. Body text moves from egui's
            // inherited gray-140 (5.89:1 — 0.77 better than dark, on
            // the palette written for low-vision use) to 15.14:1, and
            // the strong tier keeps a real step above it at 19.80:1.
            // Not a flat 255 across all five: with no bold face,
            // spending the last stop of luminance on `active` is the
            // only way a heading stays a heading.
            widget_text: WidgetText {
                noninteractive: egui::Color32::from_gray(225),
                inactive: egui::Color32::from_gray(225),
                hovered: egui::Color32::WHITE,
                active: egui::Color32::WHITE,
                open: egui::Color32::from_gray(240),
            },
            border: egui::Color32::from_gray(170),
            // 7.32:1 on the field and 8.52:1 against the window — this is
            // the palette where a control that cannot be found is the
            // whole problem (#1283).
            control_border: egui::Color32::from_gray(170),
            // Near-flat and near-black: a decorative gradient would cost
            // contrast, which is this palette's whole reason to exist.
            backdrop_top: egui::Color32::from_gray(0),
            backdrop_bottom: egui::Color32::from_gray(16),
            egui_base: egui::Theme::Dark,
            border_stroke_width: 1.5,
        }
    }
}

/// The active theme. Consumers read it; the #857 picker writes it —
/// [`apply_theme_on_change`] re-pushes egui state whenever it changes.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct CurrentTheme(pub Theme);

impl Default for CurrentTheme {
    fn default() -> Self {
        Self(Theme::dark())
    }
}

/// Interface scale: push [`crate::state::LocalSettings::ui_scale`] into
/// the egui context, and read egui's own keyboard zoom back out (#1259
/// f239).
///
/// **Both directions, because there are two controls for one setting.**
/// egui's Ctrl+plus / Ctrl+minus has always worked here
/// (`Options::zoom_with_keyboard` defaults on) but was documented
/// nowhere and reset at every launch, because nothing in this app or in
/// bevy_egui serialises egui's `Options`. Adopting the context's value
/// whenever this system did not set it makes the keyboard shortcut
/// persist through the same prefs file as the slider, instead of the two
/// fighting each other.
///
/// The `Local` is the arbitration: it holds the last value **we** wrote,
/// so a difference between it and `ctx.zoom_factor()` can only have come
/// from the keyboard. The write back to `LocalSettings` is guarded-dirty
/// (#879) — an unguarded `ResMut` deref here would re-arm the prefs save
/// debounce on every frame of the session.
///
/// Not `run_if(resource_changed)`, for [`apply_theme_on_change`]'s
/// reason: the egui context may not exist on the frame the prefs load
/// swaps `LocalSettings` in, and a `run_if` would eat that one-shot edge.
/// What [`sync_ui_scale`] should do with the interface scale this frame.
///
/// Pure, because the arbitration is the whole of the logic and the rest
/// is two writes: the value is owned by two controls at once (the
/// Settings slider and egui's Ctrl+plus), and getting the precedence
/// wrong makes them fight — a push every frame would swallow every
/// keystroke, an adopt every frame would swallow every drag.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ScaleAction {
    /// The setting moved (slider, prefs load, first frame): write it to
    /// the context. The payload is already clamped, and differs from the
    /// stored value exactly when the stored value needs normalising.
    Push(f32),
    /// The context moved on its own, so the keyboard did it: take that
    /// value into the setting. Clamped, so a held Ctrl+plus cannot walk
    /// the slider that undoes it off the screen.
    Adopt(f32),
    /// Both agree.
    Nothing,
}

/// Scale steps are ~0.1 apart; anything under this is float noise rather
/// than a user gesture.
const SCALE_MOVED: f32 = 0.001;

/// Decide between the two writers of [`crate::state::LocalSettings::ui_scale`].
///
/// `applied` is the last value **this app wrote** to the context, which
/// is what makes the question answerable: a `live` that differs from it
/// can only have come from egui's keyboard zoom. The setting wins ties,
/// so a deliberate slider drag is never mistaken for a keystroke.
pub fn scale_action(setting: f32, applied: Option<f32>, live: f32) -> ScaleAction {
    use crate::config::ui::{UI_SCALE_MAX, UI_SCALE_MIN};
    let want = setting.clamp(UI_SCALE_MIN, UI_SCALE_MAX);
    match applied {
        // First frame: nothing has been written, so nothing can be adopted.
        None => ScaleAction::Push(want),
        Some(prev) if (want - prev).abs() >= SCALE_MOVED => ScaleAction::Push(want),
        Some(prev) if (live - prev).abs() >= SCALE_MOVED => {
            ScaleAction::Adopt(live.clamp(UI_SCALE_MIN, UI_SCALE_MAX))
        }
        Some(_) => ScaleAction::Nothing,
    }
}

/// Interface scale: push [`crate::state::LocalSettings::ui_scale`] into
/// the egui context, and read egui's own keyboard zoom back out (#1259
/// f239).
///
/// **Both directions, because there are two controls for one setting.**
/// egui's Ctrl+plus / Ctrl+minus has always worked here
/// (`Options::zoom_with_keyboard` defaults on) but was documented
/// nowhere and reset at every launch, because nothing in this app or in
/// bevy_egui serialises egui's `Options`. Adopting the context's value
/// whenever this system did not set it makes the keyboard shortcut
/// persist through the same prefs file as the slider, instead of the two
/// fighting each other.
///
/// The `Local` is the arbitration — see [`scale_action`], which holds
/// all of the logic and none of the writes. The write back to
/// `LocalSettings` is guarded-dirty (#879): an unguarded `ResMut` deref
/// would re-arm the prefs save debounce on every frame of the session.
///
/// Not `run_if(resource_changed)`, for [`apply_theme_on_change`]'s
/// reason: the egui context may not exist on the frame the prefs load
/// swaps `LocalSettings` in, and a `run_if` would eat that one-shot edge.
pub fn sync_ui_scale(
    mut contexts: EguiContexts,
    mut settings: ResMut<crate::state::LocalSettings>,
    mut applied: Local<Option<f32>>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    match scale_action(settings.ui_scale, *applied, ctx.zoom_factor()) {
        ScaleAction::Push(want) => {
            ctx.set_zoom_factor(want);
            *applied = Some(want);
            // A prefs file naming something outside the bounds is
            // normalised once, here, rather than left to disagree with
            // the screen for the rest of the session.
            if (settings.ui_scale - want).abs() >= SCALE_MOVED {
                settings.ui_scale = want;
            }
        }
        ScaleAction::Adopt(scale) => {
            // Ctrl+plus does not know about our bounds; hold it to them.
            ctx.set_zoom_factor(scale);
            *applied = Some(scale);
            settings.bypass_change_detection().ui_scale = scale;
            settings.set_changed();
        }
        ScaleAction::Nothing => {}
    }
}

/// Push a [`Theme`] into an egui context: pin the theme preference (see
/// [`Theme::egui_base`]), then overlay our palette onto the matching
/// stock visuals. Deliberately a light touch on widget internals — the
/// dark look users know IS mostly stock egui; the palette owns identity
/// (accent) and surfaces, not every bevel.
/// egui-context carrier for the active palette — see [`current`].
#[derive(Clone)]
struct ThemeInCtx(std::sync::Arc<Theme>);

/// Read the active theme from inside any egui render code — no system
/// param needed. [`apply_theme`] stashes an `Arc` of the palette in the
/// context's data map, so deeply nested render helpers (and systems
/// already at Bevy's 16-param `IntoSystem` ceiling, like `room_admin_ui`
/// and `avatar_ui`) reach it through the `Ui`/`Context` they hold:
/// `let th = theme::current(ui.ctx());`. Falls back to the dark palette
/// if read before the first apply — in practice impossible, since the
/// installer runs in `Update` ahead of every egui pass.
pub fn current(ctx: &egui::Context) -> std::sync::Arc<Theme> {
    ctx.data(|d| d.get_temp::<ThemeInCtx>(egui::Id::NULL))
        .map(|t| t.0)
        .unwrap_or_else(|| std::sync::Arc::new(Theme::dark()))
}

/// Build the `Visuals` a palette installs — **the** answer to "what
/// does this label actually render as".
///
/// Split out of [`apply_theme`] by #1258 so the guards can measure the
/// thing the renderer reads. Every colour test in this module before
/// #1258 compared [`Theme`] struct fields, which is how the
/// high-contrast palette shipped with its own `text_strong` unreachable
/// by an ordinary `ui.label()` and a guard passing at 19.8 vs 12.6
/// while the screen moved 5.12 → 5.89.
pub fn visuals_for(theme: &Theme) -> egui::Visuals {
    let mut visuals = match theme.egui_base {
        egui::Theme::Dark => egui::Visuals::dark(),
        egui::Theme::Light => egui::Visuals::light(),
    };
    visuals.hyperlink_color = theme.accent;
    visuals.selection.bg_fill = theme.selection_fill;
    // `selection.stroke` is BOTH the selected-widget label colour and the
    // focused-TextEdit outline in egui 0.35. The label wins (#857
    // follow-up: chips invert their text against the teal fill); the
    // focus cue moves to a thicker accent text cursor below, so a
    // focused field stays findable even though its ring goes dark.
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, theme.selection_text);
    visuals.text_cursor.stroke = egui::Stroke::new(2.0_f32, theme.accent);
    visuals.window_fill = theme.window_fill;
    visuals.panel_fill = theme.panel_fill;
    visuals.window_stroke = egui::Stroke::new(theme.border_stroke_width, theme.border);
    visuals.warn_fg_color = theme.status.warn;
    visuals.error_fg_color = theme.status.error;
    // A text field's interior is a surface the palette owns (#1258
    // f233): egui never wrote `extreme_bg_color`, so a `TextEdit`
    // inherited the base's `from_gray(10)` under high contrast's
    // `from_gray(10)` window — a field at 1.00:1, invisible on the
    // palette written for people who cannot see faint ones.
    //
    // **And it gets a BORDER, on the second attempt (#1283).** #1258 f233
    // gave the resting tier a stroke and #1281 took it straight back out,
    // because `Style::button_style` computes
    //
    //   inner_margin = button_padding + expansion - bg_stroke.width
    //   outer_margin = -expansion
    //
    // so that a framed widget's total size is `2*button_padding +
    // content` whatever the stroke — while `Button::show` throws the
    // frame away for an unselected `toggle_value`/`selectable_label`,
    // keeping the shrunken inner margin and losing both the stroke and
    // the negative outer margin that paid for it. The unframed size is
    // then `2*(button_padding + expansion - stroke_width) + content`, so
    // every unselected toggle in the app sat 2 pt narrower at rest than
    // under the pointer and hovering one shoved its neighbours.
    //
    // The two expressions agree exactly when **expansion == stroke
    // width**, which is not a trick: it is the relationship egui's own
    // `hovered` tier already ships (expansion 1.0, stroke width 1.0), and
    // the resting tier was the odd one out at 0 and 0. Setting both to
    // 1.0 restores stock geometry to the pixel — `2*button_padding +
    // content`, framed or not — and
    // `hovering_a_widget_does_not_move_the_one_after_it` is what proves
    // it rather than this paragraph.
    //
    // `hovered`/`active` go to 2.0 so the *painted* rect still grows
    // under the pointer. Without that the resting and hovered rects
    // coincide and hover would be signalled by colour alone, which this
    // tranche has already refused once (#1259 f247).
    //
    // The colour is `control_border`, not `border`: window chrome and a
    // control's edge draw on different grounds, and in Dark they were the
    // same value only by accident — see the field's doc.
    visuals.extreme_bg_color = theme.field_fill;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, theme.border);
    visuals.widgets.inactive.bg_stroke =
        egui::Stroke::new(CONTROL_EDGE_WIDTH, theme.control_border);
    visuals.widgets.inactive.expansion = CONTROL_EDGE_WIDTH;
    visuals.widgets.hovered.expansion = CONTROL_EDGE_WIDTH + 1.0;
    visuals.widgets.active.expansion = CONTROL_EDGE_WIDTH + 1.0;
    // Widget label colours, from the PALETTE and for every base (#1258
    // f232/f238). This used to run only under `egui::Theme::Light`,
    // pulling all five tiers onto `text_strong` — which left the
    // dark-based high-contrast palette on egui's stock gray-140 body
    // text, and flattened `ui.strong()` into `ui.label()` in light,
    // since egui reads emphasis off the `active` tier alone.
    for (w, colour) in [
        (
            &mut visuals.widgets.noninteractive,
            theme.widget_text.noninteractive,
        ),
        (&mut visuals.widgets.inactive, theme.widget_text.inactive),
        (&mut visuals.widgets.hovered, theme.widget_text.hovered),
        (&mut visuals.widgets.active, theme.widget_text.active),
        (&mut visuals.widgets.open, theme.widget_text.open),
    ] {
        w.fg_stroke.color = colour;
    }
    visuals
}

/// Width of a resting control's own edge, in points (#1283).
///
/// A whole number on purpose. `button_style` and `TextEdit`'s frame both
/// fold this into an `i8` `Margin` — `(expansion - stroke.width).round()`
/// in the field's case — so a fractional width rounds in one place and
/// not the other, and the geometry that
/// `hovering_a_widget_does_not_move_the_one_after_it` holds stops being
/// exact. The high-contrast palette widens its WINDOW chrome
/// (`border_stroke_width` 1.5) and deliberately does not widen this: a
/// window edge is one line per window, a control edge is one per control.
const CONTROL_EDGE_WIDTH: f32 = 1.0;

pub fn apply_theme(ctx: &egui::Context, theme: &Theme) {
    ctx.data_mut(|d| {
        d.insert_temp(
            egui::Id::NULL,
            ThemeInCtx(std::sync::Arc::new(theme.clone())),
        )
    });
    ctx.options_mut(|o| {
        o.theme_preference = match theme.egui_base {
            egui::Theme::Dark => egui::ThemePreference::Dark,
            egui::Theme::Light => egui::ThemePreference::Light,
        };
    });
    ctx.set_visuals(visuals_for(theme));
}

/// Apply [`CurrentTheme`] to the primary egui context on startup and on
/// every later change (the #857 picker path).
///
/// Not `run_if(resource_changed)`: the change tick for the initial
/// insertion can fire on a frame where the egui context doesn't exist
/// yet (bevy_egui creates it with the window), and a `run_if` would
/// consume that one-shot edge — the app would boot unthemed. The
/// `Local` latch retries every frame until the first successful apply,
/// then only reacts to real changes.
pub fn apply_theme_on_change(
    mut contexts: EguiContexts,
    theme: Res<CurrentTheme>,
    mut applied_once: Local<bool>,
) {
    if *applied_once && !theme.is_changed() {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    apply_theme(ctx, &theme.0);
    *applied_once = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Channel-space distance — crude but enough to assert "these are
    /// visibly different colours".
    fn dist(a: egui::Color32, b: egui::Color32) -> u32 {
        let d = |x: u8, y: u8| (x as i32 - y as i32).unsigned_abs();
        d(a.r(), b.r()) + d(a.g(), b.g()) + d(a.b(), b.b())
    }

    fn all_palettes() -> [(&'static str, Theme); 3] {
        [
            ("dark", Theme::dark()),
            ("light", Theme::light()),
            ("high_contrast", Theme::high_contrast()),
        ]
    }

    /// The 2026-07-17 decision every palette encodes: the identity accent
    /// is teal, NOT the ok-green — status and brand must never collide.
    #[test]
    fn accent_is_distinct_from_every_status_colour() {
        for (palette, t) in all_palettes() {
            for (name, c) in [
                ("ok", t.status.ok),
                ("warn", t.status.warn),
                ("error", t.status.error),
                ("info", t.status.info),
            ] {
                assert!(
                    dist(t.accent, c) > 90,
                    "{palette}: accent {:?} too close to status.{name} {c:?}",
                    t.accent
                );
            }
            // Teal shape: blue ≈ green (both high), red clearly lowest.
            assert!(t.accent.r() < t.accent.g() && t.accent.r() < t.accent.b());
            let gb_gap = (t.accent.g() as i32 - t.accent.b() as i32).abs();
            assert!(
                gb_gap < 40,
                "{palette}: accent should be teal, not green or blue"
            );
        }
    }

    /// The four general-purpose status colours must be mutually distinct
    /// — the whole point of collapsing 4 ambers / 5 reds / 7 greens.
    #[test]
    fn status_colours_are_mutually_distinct() {
        for (palette, t) in all_palettes() {
            let s = t.status;
            let all = [
                ("ok", s.ok),
                ("warn", s.warn),
                ("error", s.error),
                ("info", s.info),
            ];
            for (i, (an, a)) in all.iter().enumerate() {
                for (bn, b) in all.iter().skip(i + 1) {
                    assert!(
                        dist(*a, *b) > 90,
                        "{palette}: status.{an} and status.{bn} too close"
                    );
                }
            }
        }
    }

    /// The diagnostics severity ramp's three ALARM tiers must be
    /// distinguishable too (#1259 f236) — the guard above never covered
    /// them, and they were three shades of orange: in Dark, Warn
    /// `(210,170,90)` and Error `(210,120,90)` matched in R and B, 50
    /// apart in G, 1.47:1 in luminance.
    ///
    /// Two claims, because hue on its own is not a signal (WCAG 1.4.1):
    /// the tiers must be distinct as colours, AND they must RANK in
    /// luminance, so the ramp still reads as a ramp in greyscale and
    /// under the dichromacies. `trace`/`info` are excluded from the
    /// distinctness half — they are the deliberately-quiet end and are
    /// neutral greys by design — but `trace` still owes the 3:1 floor,
    /// since it tints whole event-log lines.
    #[test]
    fn the_severity_ramp_ranks_without_relying_on_hue() {
        for (palette, t) in all_palettes() {
            let s = &t.status;
            let alarm = [
                ("warn_tier", s.warn_tier),
                ("error_tier", s.error_tier),
                ("critical_tier", s.critical_tier),
            ];
            for (i, (an, a)) in alarm.iter().enumerate() {
                for (bn, b) in alarm.iter().skip(i + 1) {
                    assert!(
                        dist(*a, *b) > 90,
                        "{palette}: {an} {a:?} and {bn} {b:?} are the same alarm"
                    );
                }
            }
            // Strictly falling luminance across the three: the ramp is an
            // ORDER, and an order a greyscale reader can still see. Both
            // directions are the same rule — on a dark ground the loudest
            // tier is the darkest of the three, and on a pale one it is
            // darker still.
            for (a, b) in alarm.windows(2).map(|w| (w[0], w[1])) {
                let (la, lb) = (relative_luminance(a.1), relative_luminance(b.1));
                assert!(
                    la > lb * 1.35,
                    "{palette}: {} ({la:.4}) does not out-rank {} ({lb:.4}) in luminance",
                    a.0,
                    b.0
                );
            }
            let trace = contrast_ratio(s.trace, t.window_fill);
            assert!(
                trace >= AA_LARGE,
                "{palette}: the trace tier is {trace:.2}:1 on window_fill"
            );
        }
    }

    /// The severity ramp stays sourced from `config::ui::diagnostics`
    /// until #856 flips `severity_color()` onto the theme — the two must
    /// agree in the meantime.
    #[test]
    fn severity_ramp_matches_the_config_source_of_truth() {
        use crate::config::ui::diagnostics as cfg;
        let s = Theme::dark().status;
        for (sev, rgb) in [
            (Severity::Trace, cfg::SEVERITY_TRACE_RGB),
            (Severity::Info, cfg::SEVERITY_INFO_RGB),
            (Severity::Warn, cfg::SEVERITY_WARN_RGB),
            (Severity::Error, cfg::SEVERITY_ERROR_RGB),
            (Severity::Critical, cfg::SEVERITY_CRITICAL_RGB),
        ] {
            assert_eq!(
                s.severity(sev),
                egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
                "{sev:?} ramp drifted from config"
            );
        }
    }

    /// WCAG thresholds, named once so the asserts below read as the
    /// rules they are (#1258).
    const AA_TEXT: f32 = 4.5;
    /// AA for large text (>= 18.7 pt, or 14 pt bold) and the boundary of
    /// a UI component (WCAG 1.4.11). Also the floor for a non-text cue
    /// that carries state.
    const AA_LARGE: f32 = 3.0;
    /// AAA for normal text — what a palette named "high contrast" is
    /// promising.
    const AAA_TEXT: f32 = 7.0;

    /// Surface sanity for every palette, measured on the [`egui::Visuals`]
    /// the renderer is actually handed — not on [`Theme`] struct fields,
    /// which is what let #1258 f232 ship.
    ///
    /// `dist` survives above for *distinctness* claims ("these two hues
    /// must not be confused"); every claim about whether something can be
    /// READ is a [`contrast_ratio`].
    #[test]
    fn text_reads_on_every_palette_surface() {
        for (palette, t) in all_palettes() {
            let v = visuals_for(&t);
            // The load-bearing one: what a plain `ui.label()` renders as,
            // over the window it renders on.
            let body = contrast_ratio(v.text_color(), t.window_fill);
            assert!(
                body >= AA_TEXT,
                "{palette}: body text is {body:.2}:1 on window_fill, under AA {AA_TEXT}"
            );
            // A button's label on the button, not on the window.
            let button = contrast_ratio(
                v.widgets.inactive.fg_stroke.color,
                v.widgets.inactive.weak_bg_fill,
            );
            assert!(
                button >= AA_TEXT,
                "{palette}: button text is {button:.2}:1 on its fill"
            );
            for (role, c) in [("text_strong", t.text_strong), ("text_weak", t.text_weak)] {
                let r = contrast_ratio(c, t.window_fill);
                assert!(r >= AA_TEXT, "{palette}: {role} is {r:.2}:1 on window_fill");
            }
            // #1258 f242: the quiet tier is a state cue (the muted-peer
            // dot, the anomaly fire count), so it owes the 3:1 non-text
            // floor even though it is deliberately the quietest thing
            // on screen.
            let faint = contrast_ratio(t.text_faint, t.window_fill);
            assert!(
                faint >= AA_LARGE,
                "{palette}: text_faint is {faint:.2}:1 on window_fill"
            );
            // Hyperlinks and the wordmark.
            let accent = contrast_ratio(t.accent, t.window_fill);
            assert!(
                accent >= AA_TEXT,
                "{palette}: accent is {accent:.2}:1 on window_fill"
            );
            for (name, c) in [
                ("ok", t.status.ok),
                ("warn", t.status.warn),
                ("error", t.status.error),
                ("info", t.status.info),
            ] {
                let r = contrast_ratio(c, t.window_fill);
                assert!(
                    r >= AA_TEXT,
                    "{palette}: status.{name} is {r:.2}:1 on window_fill"
                );
            }
            assert!(
                dist(t.chart_fill, t.window_fill) < 40,
                "{palette}: chart fills should sit near the window tone"
            );
            let danger = contrast_ratio(t.danger_surface_text, t.danger_surface);
            assert!(
                danger >= AA_TEXT,
                "{palette}: danger banner text is {danger:.2}:1 on its surface"
            );
        }
    }

    /// #1258 f238: `ui.strong()` must not render as `ui.label()`.
    ///
    /// egui derives emphasis from colour alone — `RichText::strong()`
    /// resolves to `Visuals::strong_text_color()`, which IS
    /// `widgets.active.fg_stroke.color` — and the bundled font ships no
    /// bold face, so if those two colours agree, every section heading
    /// in the app is drawn exactly like the body text beneath it.
    #[test]
    fn strong_text_is_distinguishable_from_body_text_in_every_palette() {
        for (palette, t) in all_palettes() {
            let v = visuals_for(&t);
            let (body, strong) = (v.text_color(), v.strong_text_color());
            assert_ne!(body, strong, "{palette}: ui.strong() == ui.label()");
            let (rb, rs) = (
                contrast_ratio(body, t.window_fill),
                contrast_ratio(strong, t.window_fill),
            );
            assert!(
                rs > rb * 1.2,
                "{palette}: strong text ({rs:.2}:1) barely outreaches body ({rb:.2}:1)"
            );
        }
    }

    /// #1258 f233 / #1281 / #1283: the palette owns a text field's
    /// interior, and a resting control has an edge that costs no
    /// geometry.
    ///
    /// Three attempts, and the arithmetic is why. egui never wrote
    /// `extreme_bg_color`, so a `TextEdit` inherited the base's
    /// `from_gray(10)` under high contrast's `from_gray(10)` window —
    /// 1.00:1. That half has stood since #1258.
    ///
    /// The border took two goes. #1258 gave the resting tier a stroke
    /// and #1281 reverted it, because `button_style`'s
    /// `inner_margin = button_padding + expansion - bg_stroke.width`
    /// is only paid back by the stroke and the negative outer margin a
    /// FRAMED widget draws, and `Button::show` discards the frame for an
    /// unselected toggle. The two agree exactly when `expansion ==
    /// stroke.width`, which is what the `hovered` tier has always
    /// shipped; the invariant is asserted here and the geometry it buys
    /// is measured by
    /// [`hovering_a_widget_does_not_move_the_one_after_it`].
    #[test]
    fn the_palette_owns_the_field_fill_and_the_resting_edge_is_free() {
        for (palette, t) in all_palettes() {
            let v = visuals_for(&t);
            assert_eq!(
                v.text_edit_bg_color(),
                t.field_fill,
                "{palette}: the palette does not own the field's fill"
            );
            assert_eq!(
                v.widgets.inactive.bg_stroke.color, t.control_border,
                "{palette}: a resting control's edge is the palette's to choose"
            );
            assert!(
                v.widgets.inactive.bg_stroke.width > 0.0,
                "{palette}: a resting field and button have no edge at all (#1283)"
            );
            // THE invariant. Break it and every unselected toggle in the
            // app changes width under the pointer (#1281) — from a
            // palette edit, which is not a thing that looks like a layout
            // change to anyone reading the diff.
            assert_eq!(
                v.widgets.inactive.expansion, v.widgets.inactive.bg_stroke.width,
                "{palette}: a resting stroke must be paid for by an equal expansion"
            );
            // And hover still costs something a user can see in the
            // geometry, not only in the colour (#1259 f247).
            assert!(
                v.widgets.hovered.expansion > v.widgets.inactive.expansion,
                "{palette}: hover has stopped growing the widget"
            );
        }
    }

    /// #1283: a resting control is distinguishable from what it sits on.
    ///
    /// WCAG 1.4.11's floor for a non-text boundary is 3:1, and the edge
    /// has two neighbours that matter: the interior it encloses (a text
    /// field's fill) and the ground it sits on (the window behind a
    /// button). Measured with `contrast_ratio`, not `dist` — this is a
    /// legibility question, and the module's rule is that `dist` answers
    /// "could these be confused" and nothing else.
    ///
    /// Dark's button interior is the one pair NOT asserted, and the
    /// omission is deliberate: stock dark's button fill is
    /// `from_gray(60)` on a `from_gray(27)` window, already 1.56:1 before
    /// any edge, so an edge that cleared 3:1 against BOTH would have to
    /// be around `from_gray(133)` and would turn the #857-validated dark
    /// palette into an outlined-control theme. #1283 named the
    /// high-contrast button, and high contrast is where it is held.
    #[test]
    fn every_palette_gives_a_resting_control_a_visible_edge() {
        for (palette, t) in all_palettes() {
            let v = visuals_for(&t);
            let edge = v.widgets.inactive.bg_stroke.color;

            let on_field = contrast_ratio(edge, t.field_fill);
            assert!(
                on_field >= AA_LARGE,
                "{palette}: a resting text field's edge is {on_field:.2}:1 on its own fill"
            );

            let on_window = contrast_ratio(edge, t.window_fill);
            assert!(
                on_window >= AA_LARGE,
                "{palette}: a resting button's edge is {on_window:.2}:1 on the window"
            );
        }

        // The control: this must be able to fail. `border`'s dark value
        // is the button fill it would have drawn on, which is the
        // coincidence that let #1258 f233 ship without retuning dark.
        let dark = Theme::dark();
        assert!(
            contrast_ratio(dark.border, dark.field_fill) < AA_LARGE,
            "if the old window-chrome colour now passes, this guard proves nothing"
        );
    }

    /// #1283: the edge is really PAINTED, not merely present in a struct.
    ///
    /// The whole tranche started from a palette whose guards read `Theme`
    /// fields the renderer never consulted (#1258), so the strongest
    /// available check for a palette claim is the shapes egui actually
    /// emits. This runs a real `TextEdit` and a real `Button` in a real
    /// context and reads the stroke off the rectangles they paint.
    ///
    /// Drawn on the root `Ui` rather than inside an `Area`: an `Area`
    /// fades in over its first frames, and every colour in those passes
    /// arrives premultiplied by the fade, which would make an exact
    /// comparison a comparison against an animation.
    #[test]
    fn a_resting_control_paints_the_palettes_edge() {
        fn rect_strokes(shape: &egui::Shape, out: &mut Vec<egui::epaint::Stroke>) {
            match shape {
                egui::Shape::Rect(r) => out.push(r.stroke),
                egui::Shape::Vec(v) => v.iter().for_each(|s| rect_strokes(s, out)),
                _ => {}
            }
        }

        for (palette, theme) in all_palettes() {
            let ctx = egui::Context::default();
            let mut text = String::new();
            let mut strokes = Vec::new();
            // Three passes: a button reads LAST frame's response to pick
            // its state, so the first pass paints nothing settled.
            for _ in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 200.0),
                    )),
                    ..Default::default()
                };
                strokes.clear();
                let out = ctx.run_ui(input, |ui| {
                    ui.ctx().set_visuals(visuals_for(&theme));
                    // Both responses dropped on purpose: this test reads
                    // the PAINT, not the interaction. Leaving the field
                    // unfocused is the point — a focused one takes its
                    // stroke from `selection.stroke` instead (#1284).
                    let _ = ui.text_edit_singleline(&mut text);
                    let _ = ui.button("Example");
                });
                for clipped in &out.shapes {
                    rect_strokes(&clipped.shape, &mut strokes);
                }
            }

            let edges = strokes
                .iter()
                .filter(|s| s.color == theme.control_border && s.width == CONTROL_EDGE_WIDTH)
                .count();
            assert!(
                edges >= 2,
                "{palette}: expected the field and the button to paint the palette's \
                 edge, found {edges} of them in {strokes:?}"
            );
        }
    }

    /// #1281: hovering a widget must not move the widget after it.
    ///
    /// The regression this catches came from a PALETTE edit, and nothing
    /// about a palette edit looks like a layout change — so the guard
    /// measures what a user actually sees: where the next widget in a
    /// row lands, at rest and under the pointer, in a real egui context.
    ///
    /// `toggle_value` and `selectable_label` are here because they are
    /// what the toolbar and every editor tab bar are made of, and
    /// because they are the pair that discards its frame at rest. The
    /// plain `Button` cases stayed stable throughout and would not have
    /// caught it.
    #[test]
    fn hovering_a_widget_does_not_move_the_one_after_it() {
        /// Where the label after `kind` lands, and whether the hover
        /// actually registered — a probe that never hovers would pass
        /// this test while proving nothing.
        fn next_widget_x(theme: &Theme, kind: &str, hover: bool) -> (f32, bool) {
            let ctx = egui::Context::default();
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 200.0));
            let mut next_x = 0.0_f32;
            let mut hovered = false;
            let mut target =
                egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(5.0, 5.0));
            let mut toggled = false;
            // Five passes: `Button::show` reads LAST frame's response to
            // pick its state, so a size that depends on state needs two
            // to settle — and an oscillation needs more to show itself.
            for pass in 0..5 {
                let mut input = egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                };
                if hover && pass > 0 {
                    input
                        .events
                        .push(egui::Event::PointerMoved(target.center()));
                }
                let _ = ctx.run_ui(input, |ui| {
                    ui.ctx().set_visuals(visuals_for(theme));
                    egui::Area::new("probe".into())
                        .fixed_pos(egui::pos2(10.0, 10.0))
                        .show(ui.ctx(), |ui| {
                            ui.horizontal(|ui| {
                                let r = match kind {
                                    "button" => ui.button("Example"),
                                    "toggle_value" => ui.toggle_value(&mut toggled, "Example"),
                                    "selectable_label" => ui.selectable_label(false, "Example"),
                                    "small_button" => ui.small_button("Example"),
                                    "menu_button" => ui.menu_button("Example", |_| {}).response,
                                    _ => unreachable!("unknown widget kind {kind}"),
                                };
                                target = r.rect;
                                hovered = r.hovered();
                                next_x = ui.label("after").rect.left();
                            });
                        });
                });
            }
            (next_x, hovered)
        }

        for (palette, theme) in all_palettes() {
            for kind in [
                "button",
                "toggle_value",
                "selectable_label",
                "small_button",
                "menu_button",
            ] {
                let (at_rest, _) = next_widget_x(&theme, kind, false);
                let (under_pointer, hovered) = next_widget_x(&theme, kind, true);
                assert!(
                    hovered,
                    "{palette}/{kind}: the probe never hovered, so it proves nothing"
                );
                assert!(
                    (at_rest - under_pointer).abs() < 0.01,
                    "{palette}/{kind}: hovering moves the next widget by {:.1} pt",
                    under_pointer - at_rest
                );
            }
        }
    }

    /// #857 follow-up, re-measured for #1258 f241: the CTA label must
    /// read on its fill in every palette — dark bases run
    /// bright-fill/dark-text, light the inverse — and so must the label
    /// on a selected chip, which egui paints with `selection.stroke`
    /// over `selection.bg_fill` on every toolbar toggle, tab and gizmo
    /// World/Local pair.
    ///
    /// The old form asserted `dist(..) > 300`, which the light
    /// palette's white-on-(60,150,160) chip passed at a measured
    /// 3.46:1.
    #[test]
    fn accent_fill_labels_and_selection_stay_readable() {
        for (palette, t) in all_palettes() {
            let v = visuals_for(&t);
            let cta = contrast_ratio(t.accent_fill_text, t.accent_fill);
            assert!(
                cta >= AA_TEXT,
                "{palette}: CTA label is {cta:.2}:1 on accent_fill"
            );
            let chip = contrast_ratio(v.selection.stroke.color, v.selection.bg_fill);
            assert!(
                chip >= AA_TEXT,
                "{palette}: selected-chip label is {chip:.2}:1 on the selection band"
            );
            // Body text drawn OVER a text-selection run is the other
            // half of egui's single `selection` knob. A distinctness
            // claim, not a legibility one: the band is transient, it
            // never carries the only copy of the text, and holding it
            // to AA would force the chip label the other way.
            assert!(
                dist(t.text_strong, t.selection_fill) > 200,
                "{palette}: the selection band is the tone of the body text"
            );
        }
    }

    /// #1259 f239: the slider and Ctrl+plus are two controls for one
    /// setting, and the arbitration is the whole feature.
    ///
    /// THE SEQUENCE that motivates every arm: a user presses Ctrl+plus
    /// three times, quits, and comes back. Before this, egui's zoom was
    /// live but nothing serialised it, so they came back to 1.0 and had
    /// to rediscover a shortcut nothing documents.
    #[test]
    fn the_slider_and_the_keyboard_zoom_do_not_fight() {
        use crate::config::ui::{UI_SCALE_MAX, UI_SCALE_MIN};

        // First frame: nothing has been written, so nothing can be
        // adopted — the persisted setting wins.
        assert_eq!(scale_action(1.3, None, 1.0), ScaleAction::Push(1.3));

        // Steady state: both agree, and this system must write nothing
        // at all, or the prefs debounce never gets to fire.
        assert_eq!(scale_action(1.3, Some(1.3), 1.3), ScaleAction::Nothing);

        // The slider moved. The setting wins even though the context
        // still holds the old value — otherwise a drag reads as a
        // keystroke and gets undone.
        assert_eq!(scale_action(1.6, Some(1.3), 1.3), ScaleAction::Push(1.6));

        // Ctrl+plus moved the context and nothing else did: adopt it, so
        // the shortcut persists through the same prefs file as the
        // slider.
        assert_eq!(scale_action(1.3, Some(1.3), 1.4), ScaleAction::Adopt(1.4));

        // Bounds, in both directions. A held Ctrl+plus must not be able
        // to walk the Settings slider that undoes it off the screen, and
        // a prefs file (or a hand-edited one) naming 0.05 must not leave
        // the app unreadable.
        assert_eq!(
            scale_action(1.0, Some(1.0), 9.0),
            ScaleAction::Adopt(UI_SCALE_MAX)
        );
        assert_eq!(
            scale_action(0.05, None, 1.0),
            ScaleAction::Push(UI_SCALE_MIN)
        );
        // ... and the clamped Push differs from the stored value, which
        // is exactly the condition the system normalises on.
        assert!((UI_SCALE_MIN - 0.05).abs() >= SCALE_MOVED);
    }

    /// High-contrast must earn its name **on the text a user reads**,
    /// which is the tier the renderer takes from `Visuals`, not the
    /// `text_strong` field a handful of call sites ask for by name
    /// (#1258 f232).
    ///
    /// The old form compared `dist` on the struct fields and passed at
    /// 19.8 vs 12.6 while the screen moved 5.12 → 5.89, because
    /// `apply_theme` rewrote the widget tiers only under a Light base
    /// and this palette is Dark-based. Every assert here goes through
    /// [`visuals_for`] for that reason.
    #[test]
    fn high_contrast_is_actually_higher_contrast() {
        let dark = Theme::dark();
        let hc = Theme::high_contrast();
        let (dv, hv) = (visuals_for(&dark), visuals_for(&hc));

        let dark_body = contrast_ratio(dv.text_color(), dark.window_fill);
        let hc_body = contrast_ratio(hv.text_color(), hc.window_fill);
        assert!(
            hc_body >= AAA_TEXT,
            "high contrast body text is {hc_body:.2}:1, under AAA {AAA_TEXT}"
        );
        assert!(
            hc_body > dark_body * 2.0,
            "high contrast body text ({hc_body:.2}:1) barely beats dark ({dark_body:.2}:1)"
        );

        for (name, dc, hc_c) in [
            ("text_strong", dark.text_strong, hc.text_strong),
            ("text_weak", dark.text_weak, hc.text_weak),
            ("text_faint", dark.text_faint, hc.text_faint),
        ] {
            let d = contrast_ratio(dc, dark.window_fill);
            let h = contrast_ratio(hc_c, hc.window_fill);
            assert!(
                h > d,
                "high contrast {name} is {h:.2}:1 against dark's {d:.2}:1"
            );
        }

        // A button's label on the button — one of the two surfaces f232
        // found still wearing the stock dark chrome.
        let btn = contrast_ratio(
            hv.widgets.inactive.fg_stroke.color,
            hv.widgets.inactive.weak_bg_fill,
        );
        assert!(btn >= AAA_TEXT, "high contrast button text is {btn:.2}:1");
        // The other surface — the button's own BOUNDARY — was left
        // unasserted here through #1258 and #1281 and is now held
        // (#1283). The button's FILL against the window is still
        // `from_gray(60)` on `from_gray(10)`, 1.79:1, and that has not
        // changed: no fill can clear 3:1 against a near-black window and
        // still carry AAA text, because the first needs luminance above
        // 0.109 and the second needs it below 0.10. So the boundary is
        // the EDGE's job, which is why it could never have been fixed by
        // lifting `weak_bg_fill` as #1258 f232 originally proposed.
        let fill_alone = contrast_ratio(hv.widgets.inactive.weak_bg_fill, hc.window_fill);
        assert!(
            fill_alone < AA_LARGE,
            "if the fill alone now clears {AA_LARGE}:1 the reasoning above is stale"
        );
        let boundary = contrast_ratio(hv.widgets.inactive.bg_stroke.color, hc.window_fill);
        assert!(
            boundary >= AA_LARGE,
            "high contrast button boundary is {boundary:.2}:1, under WCAG 1.4.11"
        );

        assert!(hc.border_stroke_width > dark.border_stroke_width);
    }
}
