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
    // Whether "Reset window layout" has been pressed while this window
    // has been open (#1261 f45). A `Local` and not a frame-local, because
    // the confirmation has to outlive the click that produced it — the
    // windows it affects re-tidy on their NEXT open, which may be minutes
    // away, and a one-frame flash is indistinguishable from nothing
    // having happened.
    mut layout_reset: Local<bool>,
) {
    if !panels.settings {
        // Closing the window ends the statement it was making.
        *layout_reset = false;
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
            // The other half of the accessibility surface (#1259 f239).
            // The palette picker used to be all of it: there was no
            // text-size control anywhere, and egui's own Ctrl+plus was
            // undocumented and forgotten at every launch. The slider and
            // the shortcut are one setting — `theme::sync_ui_scale` reads
            // the keyboard zoom back out — so this number is always what
            // is on screen, however the user got there.
            ui.strong("Interface size");
            ui.horizontal(|ui| {
                dirty |= ui
                    .add(
                        crate::ui::num::slider(
                            &mut s.ui_scale,
                            crate::config::ui::UI_SCALE_MIN..=crate::config::ui::UI_SCALE_MAX,
                        )
                        .fixed_decimals(2)
                        .suffix("x"),
                    )
                    .on_hover_text(
                        "Scales all text and controls. Ctrl+plus and Ctrl+minus \
                         do the same thing from anywhere.",
                    )
                    .changed();
                if ui
                    .button("Reset")
                    .on_hover_text("Back to the default size.")
                    .clicked()
                {
                    s.ui_scale = 1.0;
                    dirty = true;
                }
            });

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
                            crate::ui::num::slider(&mut s.camera_ground_clearance_m, 0.2..=5.0)
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
            // The only in-world identity the product has (#1226 f325). Off
            // is a real preference — a busy room is a wall of text — so it
            // is a setting and not a constant, but it defaults on, because
            // with it off nothing on screen connects a People row to a body
            // and Mute has to be aimed by trial and error.
            dirty |= ui
                .checkbox(&mut s.show_peer_nametags, "Show names over people")
                .on_hover_text(if s.show_peer_nametags {
                    "On: each person's name hangs over their body, and hovering \
                     a row in People outlines the body it belongs to."
                } else {
                    "Off: bodies carry no name. You can still tell who is who \
                     from the People window."
                })
                .changed();

            ui.add_space(8.0);
            ui.separator();
            ui.strong("Privacy");
            // #1248 f298 asked for a policy, not a patch, and this is the
            // policy: state the exposure, and give the person standing in
            // somebody else's world a way out of it. A world reached
            // through a portal or a gateway is a stranger's, its record can
            // name any web address, and being in the room is what makes
            // your client fetch from it. #1127 removed the crude half of
            // that (http, loopback, private ranges); the rest is inherent
            // to following an address somebody else chose.
            //
            // Defaults ON: most authored imagery in the product is a URL,
            // and defaulting off would make every visited world worse
            // without the visitor knowing why. Being able to see and change
            // it is the fix; hiding it was the defect.
            dirty |= ui
                .checkbox(
                    &mut s.load_external_assets,
                    "Load images and sounds from outside Bluesky",
                )
                .on_hover_text(if s.load_external_assets {
                    "On: worlds can show pictures and play sounds stored anywhere \
                     on the web. Whoever built the world chooses the address, and \
                     loading it tells that address you are here."
                } else {
                    "Off: only pictures and sounds stored in Bluesky are loaded. \
                     Anything a world points at elsewhere is left blank, and the \
                     editor says so."
                })
                .changed();

            ui.add_space(8.0);
            ui.separator();
            ui.strong("Effects");
            ui.label("Contact effects from the world you're in:");
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
                                 sounds this world's owner authored."
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
            ui.small("(this device only — not saved to your account)");

            ui.add_space(8.0);
            ui.separator();
            ui.strong("Windows");
            // #1261 f45: the #833 non-overlap guarantee held only until
            // each window had been shown once — `remember` persists a
            // rect on the very first frame and `place` returns it
            // thereafter, so the staggering machinery was dead from then
            // on and a machine inherited whatever geometry its first
            // session produced. `place` re-tidies a rect that no longer
            // fits the screen now, but a merely MESSY arrangement is the
            // user's own and only they can say when they are done with
            // it. This is that button.
            ui.horizontal(|ui| {
                if ui
                    .button("Reset window layout")
                    .on_hover_text(
                        "Forget where every window was left, so they lay themselves \
                         out again next time you open them. Nothing else changes.",
                    )
                    .clicked()
                {
                    *layout_reset = chrome.reset_layout();
                }
                if *layout_reset {
                    crate::ui::affordances::ok_label(ui, "Windows will re-tidy when reopened");
                }
            });

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
