//! The Settings window (#857): client-side, this-machine-only
//! preferences — the theme picker and the remote-peer smoothing toggle
//! (absorbed from its odd first home in the Avatar editor's footer).
//!
//! Everything here edits [`LocalSettings`], which `crate::prefs`
//! persists (#820); the theme pick reaches egui via
//! `theme::sync_theme_from_settings` → `theme::apply_theme_on_change`,
//! so a click recolors the whole UI the same frame. Writes go through
//! `bypass_change_detection` with an explicit `set_changed` on real
//! interaction, so merely having the window open doesn't ping the prefs
//! save debounce every frame.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::state::LocalSettings;
use crate::ui::theme::UserTheme;
use crate::ui::toolbar::UiPanels;

/// Render the Settings window while its toolbar toggle is on.
pub fn settings_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<UiPanels>,
    mut settings: ResMut<LocalSettings>,
    mut chrome: crate::ui::layout::WindowChrome,
) {
    if !panels.settings {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let mut open = panels.settings;
    let (pos, size) = chrome.place(crate::ui::layout::UiWindow::Settings, ctx);
    let response = egui::Window::new("Settings")
        .open(&mut open)
        .default_pos(pos)
        .default_size(size)
        .constrain_to(ctx.available_rect())
        .resizable(false)
        .collapsible(true)
        .show(ctx, |ui| {
            // Guarded-dirty pattern: `&mut` field access through the
            // `ResMut` would mark the resource changed every frame the
            // window is open, and the prefs debounce would re-save
            // identical data forever. Only a real interaction dirties.
            let s = settings.bypass_change_detection();
            let mut dirty = false;

            ui.strong("Theme");
            ui.horizontal(|ui| {
                for pref in [UserTheme::Dark, UserTheme::Light, UserTheme::HighContrast] {
                    dirty |= ui
                        .selectable_value(&mut s.theme, pref, pref.label())
                        .changed();
                }
            });
            ui.small("Applies immediately; remembered on this machine.");

            ui.add_space(8.0);
            ui.separator();
            ui.strong("Network");
            dirty |= ui
                .checkbox(&mut s.smooth_kinematics, "Smooth remote peers")
                .on_hover_text(
                    "Hermite spline + 100 ms buffer. Uncheck to snap to the \
                     latest packet and expose raw jitter.",
                )
                .changed();
            ui.small("(this device only — not saved to your PDS)");

            if dirty {
                settings.set_changed();
            }
        });
    if let Some(response) = response.as_ref() {
        chrome.remember(
            crate::ui::layout::UiWindow::Settings,
            response.response.rect,
        );
    }
    if !open {
        panels.settings = false;
    }
}
