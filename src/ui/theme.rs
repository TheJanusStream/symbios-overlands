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

/// The full semantic palette. Fields are `pub` — consumers read roles
/// directly (`theme.status.ok`, `theme.accent`) rather than through
/// getters, keeping call sites as short as the literals they replace.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub status: StatusPalette,
    /// Identity accent (teal): hyperlinks, selection, wordmark, focus.
    pub accent: egui::Color32,
    /// Filled call-to-action background (white text on top) — the login
    /// "Enter the Overlands" button. Darker than `accent` so large
    /// fills don't glow.
    pub accent_fill: egui::Color32,
    /// Destructive filled-button background (white text) — the shared
    /// danger idiom (`ui::confirm::danger_button`).
    pub danger_fill: egui::Color32,
    /// Floating-window background.
    pub window_fill: egui::Color32,
    /// Top/side panel background (toolbar).
    pub panel_fill: egui::Color32,
    /// Chart/plot background fill (diagnostics histograms).
    pub chart_fill: egui::Color32,
    /// Deeper chart fill for nested/inset plot areas.
    pub chart_fill_deep: egui::Color32,
    /// Primary readable text.
    pub text_strong: egui::Color32,
    /// De-emphasised text (hints, timestamps, reasons).
    pub text_weak: egui::Color32,
    /// Window and separator strokes.
    pub border: egui::Color32,
    /// Which egui base visuals this palette is built over — pinned into
    /// `ThemePreference` by [`apply_theme`] so an OS-theme-forwarding
    /// bevy_egui upgrade can never flip the mode out from under the
    /// palette.
    pub egui_base: egui::Theme,
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
            accent_fill: egui::Color32::from_rgb(16, 125, 134),
            danger_fill: egui::Color32::from_rgb(160, 40, 40),
            window_fill: egui::Color32::from_gray(27),
            panel_fill: egui::Color32::from_gray(27),
            chart_fill: egui::Color32::from_gray(28),
            chart_fill_deep: egui::Color32::from_gray(24),
            text_strong: egui::Color32::from_gray(220),
            text_weak: egui::Color32::from_gray(140),
            border: egui::Color32::from_gray(60),
            egui_base: egui::Theme::Dark,
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

/// Push a [`Theme`] into an egui context: pin the theme preference (see
/// [`Theme::egui_base`]), then overlay our palette onto the matching
/// stock visuals. Deliberately a light touch on widget internals — the
/// dark look users know IS mostly stock egui; the palette owns identity
/// (accent) and surfaces, not every bevel.
pub fn apply_theme(ctx: &egui::Context, theme: &Theme) {
    ctx.options_mut(|o| {
        o.theme_preference = match theme.egui_base {
            egui::Theme::Dark => egui::ThemePreference::Dark,
            egui::Theme::Light => egui::ThemePreference::Light,
        };
    });
    let mut visuals = match theme.egui_base {
        egui::Theme::Dark => egui::Visuals::dark(),
        egui::Theme::Light => egui::Visuals::light(),
    };
    visuals.hyperlink_color = theme.accent;
    visuals.selection.bg_fill = theme.accent_fill;
    visuals.selection.stroke = egui::Stroke::new(1.0, theme.accent);
    visuals.window_fill = theme.window_fill;
    visuals.panel_fill = theme.panel_fill;
    visuals.window_stroke = egui::Stroke::new(1.0, theme.border);
    visuals.warn_fg_color = theme.status.warn;
    visuals.error_fg_color = theme.status.error;
    ctx.set_visuals(visuals);
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

    /// The 2026-07-17 decision this palette encodes: the identity accent
    /// is teal, NOT the ok-green — status and brand must never collide.
    #[test]
    fn accent_is_distinct_from_every_status_colour() {
        let t = Theme::dark();
        for (name, c) in [
            ("ok", t.status.ok),
            ("warn", t.status.warn),
            ("error", t.status.error),
            ("info", t.status.info),
        ] {
            assert!(
                dist(t.accent, c) > 90,
                "accent {:?} too close to status.{name} {c:?}",
                t.accent
            );
        }
        // Teal shape: blue ≈ green (both high), red clearly lowest.
        assert!(t.accent.r() < t.accent.g() && t.accent.r() < t.accent.b());
        let gb_gap = (t.accent.g() as i32 - t.accent.b() as i32).abs();
        assert!(gb_gap < 40, "accent should be teal, not green or blue");
    }

    /// The four general-purpose status colours must be mutually distinct
    /// — the whole point of collapsing 4 ambers / 5 reds / 7 greens.
    #[test]
    fn status_colours_are_mutually_distinct() {
        let s = Theme::dark().status;
        let all = [
            ("ok", s.ok),
            ("warn", s.warn),
            ("error", s.error),
            ("info", s.info),
        ];
        for (i, (an, a)) in all.iter().enumerate() {
            for (bn, b) in all.iter().skip(i + 1) {
                assert!(dist(*a, *b) > 90, "status.{an} and status.{bn} too close");
            }
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

    /// Dark-surface sanity: text roles must actually read on the fills.
    #[test]
    fn text_reads_on_dark_surfaces() {
        let t = Theme::dark();
        assert!(dist(t.text_strong, t.window_fill) > 400);
        assert!(dist(t.text_weak, t.window_fill) > 200);
        assert!(
            dist(t.chart_fill, t.window_fill) < 30,
            "chart fills sit near the window tone"
        );
    }
}
