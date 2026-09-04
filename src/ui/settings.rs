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

use crate::camera::CameraGroundAvoidance;
use crate::state::LocalSettings;
use crate::ui::theme::UserTheme;
use crate::ui::toolbar::UiPanels;

/// Render the Settings window while its toolbar toggle is on.
#[allow(clippy::too_many_arguments)]
pub fn settings_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<UiPanels>,
    mut settings: ResMut<LocalSettings>,
    mut chrome: crate::ui::layout::WindowChrome,
    mut muted_dids: ResMut<crate::state::MutedDids>,
    profile_cache: Res<crate::avatar::BskyProfileCache>,
    clipboard: Res<crate::boot_params::ClipboardQueue>,
    mut session_log: ResMut<crate::diagnostics::SessionLog>,
    time: Res<Time>,
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
        .constrain_to(chrome.available_rect(ctx))
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
            ui.strong("Camera");
            ui.label("Ground avoidance:");
            ui.horizontal(|ui| {
                for mode in [
                    CameraGroundAvoidance::Off,
                    CameraGroundAvoidance::CameraOnly,
                    CameraGroundAvoidance::FullRay,
                ] {
                    dirty |= ui
                        .selectable_value(&mut s.camera_ground_avoidance, mode, mode.label())
                        .on_hover_text(match mode {
                            CameraGroundAvoidance::Off => {
                                "Never pull the camera in — it may dip under \
                                 terrain when orbiting low."
                            }
                            CameraGroundAvoidance::CameraOnly => {
                                "Keep the camera itself above the ground. \
                                 Terrain between you and the camera may block \
                                 the view but never zooms it in."
                            }
                            CameraGroundAvoidance::FullRay => {
                                "Also zoom in whenever terrain would block the \
                                 view of your avatar (the old behavior — \
                                 aggressive at low angles)."
                            }
                        })
                        .changed();
                }
            });
            if s.camera_ground_avoidance != CameraGroundAvoidance::Off {
                ui.horizontal(|ui| {
                    ui.label("Clearance:");
                    dirty |= ui
                        .add(
                            egui::Slider::new(&mut s.camera_ground_clearance_m, 0.2..=5.0)
                                .suffix(" m"),
                        )
                        .on_hover_text("Headroom kept between the camera and the terrain")
                        .changed();
                });
            }

            ui.add_space(8.0);
            ui.separator();
            ui.strong("Network");
            // Outcome-first, like the Camera options above it (#1224
            // f334). Three of the four content words in the old text —
            // spline, packet, jitter — were transport implementation, and
            // it described the mechanism rather than what happens to the
            // people you are looking at. Both states have a real
            // user-visible shape and neither was named.
            dirty |= ui
                .checkbox(&mut s.smooth_kinematics, "Smooth remote peers")
                .on_hover_text(if s.smooth_kinematics {
                    "On: other people move smoothly, shown a fraction of a second \
                     behind where they really are."
                } else {
                    "Off: other people jump straight to their last known position — \
                     choppier, but with no delay."
                })
                .changed();

            ui.add_space(8.0);
            ui.separator();
            ui.strong("Effects");
            ui.label("Contact effects from the room you're in:");
            ui.horizontal(|ui| {
                for level in [
                    crate::state::EffectsIntensity::Full,
                    crate::state::EffectsIntensity::Reduced,
                    crate::state::EffectsIntensity::Off,
                ] {
                    dirty |= ui
                        .selectable_value(&mut s.effects_intensity, level, level.label())
                        .on_hover_text(match level {
                            crate::state::EffectsIntensity::Full => {
                                "Play the splashes, dust, scorch marks and footstep \
                                 sounds the room's owner authored."
                            }
                            crate::state::EffectsIntensity::Reduced => {
                                "Keep them, smaller and quieter, and never more than \
                                 one of each per second."
                            }
                            crate::state::EffectsIntensity::Off => {
                                "None at all. Rooms you visit are authored by other \
                                 people, and this is the only control over what they \
                                 can put on your screen."
                            }
                        })
                        .changed();
                }
            });
            ui.small(
                "Also the setting to reach for if flashing or motion is a problem \
                 for you.",
            );

            ui.add_space(8.0);
            ui.separator();
            ui.strong("Login screen");
            dirty |= ui
                .checkbox(&mut s.login_world_backdrop, "Live world backdrop")
                .on_hover_text(
                    "Build and slowly orbit a random seeded world behind the \
                     login screen. Costs a few seconds of world generation; \
                     applies the next time you see the login screen.",
                )
                .changed();
            ui.small("(this device only — not saved to your PDS)");

            ui.add_space(8.0);
            ui.separator();
            muted_people_section(
                ui,
                &mut muted_dids,
                &profile_cache,
                &clipboard,
                &mut session_log,
                time.elapsed_secs_f64(),
            );

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

/// The mute list, and the only way to leave it (#1223 f292).
///
/// `MutedDids` is durable and was reachable from exactly two places, both of
/// which required the muted person to be standing in the room with you: the
/// roster checkbox and the offer dialog. An accidental tick was therefore
/// effectively permanent — the only way back was to hope they wandered into
/// a room you happened to be in.
///
/// Split out of [`settings_ui`] so the section can be read on its own; it
/// takes the pieces rather than the whole `ResMut` set so the guarded-dirty
/// discipline above is not accidentally inherited (a mute write SHOULD dirty
/// its resource — that is what persists it).
fn muted_people_section(
    ui: &mut egui::Ui,
    muted_dids: &mut crate::state::MutedDids,
    profile_cache: &crate::avatar::BskyProfileCache,
    clipboard: &crate::boot_params::ClipboardQueue,
    session_log: &mut crate::diagnostics::SessionLog,
    now: f64,
) {
    ui.strong("Muted people");
    if muted_dids.0.is_empty() {
        ui.colored_label(
            crate::ui::theme::current(ui.ctx()).text_weak,
            "You haven't muted anyone.",
        );
        return;
    }
    ui.small("Hidden and silenced for you, on every visit, until you unmute them.");
    // Sorted: a `HashSet` iterates arbitrarily, and rows that reshuffle
    // between frames are rows you cannot click a button in.
    let mut unmute: Option<String> = None;
    egui::ScrollArea::vertical()
        .id_salt("muted_people")
        .max_height(140.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for did in muted_dids.sorted() {
                ui.horizontal(|ui| {
                    // The cached handle when we have ever resolved this DID
                    // in this session; the identifier otherwise. A list of
                    // bare DIDs is a list nobody can act on.
                    match profile_cache.get(did).and_then(|p| p.handle.as_deref()) {
                        Some(handle) => {
                            ui.monospace(format!("@{handle}"));
                        }
                        None => {
                            ui.monospace(
                                egui::RichText::new(did)
                                    .small()
                                    .color(crate::ui::theme::current(ui.ctx()).text_weak),
                            );
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Unmute").clicked() {
                            unmute = Some(did.to_owned());
                        }
                        if ui.small_button("Copy id").on_hover_text(did).clicked() {
                            clipboard.copy(did, &format!("Copied: {did}"));
                        }
                    });
                });
            }
        });
    if let Some(did) = unmute {
        // Through the same funnel as every other mute write (#1219) so the
        // durable list and the session log cannot disagree about what
        // happened. No live peer to pass: the person whose row this is may
        // be nowhere near, which is exactly why this surface exists — but
        // `sync_mute_visibility` picks them up within a frame if they are.
        crate::network::presence::set_peer_mute(
            None,
            Some(did.as_str()),
            false,
            muted_dids,
            session_log,
            None,
            now,
        );
    }
}
