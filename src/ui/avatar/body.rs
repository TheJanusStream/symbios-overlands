//! The Body tab: the rigged engine body in overlands chrome (#1059).
//!
//! Every axis widget here is the sibling crate's own — the public sections
//! `bevy_symbios_avatar::editor` exposes (upstream #231), hosted inside this
//! window's theme, undo ring and debounce instead of the viewer's
//! `SidePanel`. That is the one-source-of-truth contract: a new engine axis
//! learns to draw itself exactly once, upstream, and appears here on the
//! next dependency bump.
//!
//! The tab has three shapes:
//!
//!   - a **generator body**: one affordance — wear a rigged body. Wearing
//!     mints a fresh wardrobe rkey and resolves the DID-seeded engine
//!     default locally; nothing touches the PDS until Save publishes the
//!     bundle. The classic chassis stays reachable through the footer's
//!     Reset and the re-roll section, both of which build generator bodies.
//!   - a **rigged body, unresolved** (offline fetch, deleted reference):
//!     say so, and offer a fresh body.
//!   - a **rigged body, resolved**: the engine sections, the derived
//!     readout, and the wardrobe — every body this identity has published,
//!     wearable with a click, plus save-as-copy for branching a look.
//!
//! Archetype exposure: the engine identity section carries the
//! humanoid/quadruped toggle, and both build, animate and dress end to end
//! (#1057/#1058 run on the rig, not on an anatomy) — so quadrupeds are
//! deliberately NOT hidden here.

use bevy_egui::egui;
use bevy_symbios_avatar::editor as sections;

use crate::pds::AvatarRecord;
use crate::pds::avatar::wardrobe::{EngineAvatarRecord, wear_new_engine_body};

/// What the Body tab did this frame.
#[derive(Default)]
pub(super) struct BodyTabOutcome {
    /// The record changed in a way that changes the built body.
    pub changed: bool,
    /// A label for the undo entry, on discrete actions.
    pub label: Option<String>,
    /// The wardrobe "Refresh" button was clicked.
    pub wants_wardrobe_refresh: bool,
}

/// The fetched wardrobe listing, cached across frames on the editor state.
#[derive(Default)]
pub(super) struct WardrobeListing {
    /// `(rkey, record)` pairs in creation order, once a fetch has landed.
    pub entries: Option<Vec<(String, EngineAvatarRecord)>>,
    /// Whether a fetch is in flight, for the button's spinner text.
    pub fetching: bool,
}

/// Draw the tab. `did` is the session identity; without one (not logged in)
/// wearing and the wardrobe are disabled with a hint, since both need a
/// repo to point into.
pub(super) fn draw_body_tab(
    ui: &mut egui::Ui,
    record: &mut AvatarRecord,
    listing: &mut WardrobeListing,
    did: Option<&str>,
) -> BodyTabOutcome {
    let mut outcome = BodyTabOutcome::default();

    let Some(rig) = record.body.rigged_ref() else {
        ui.label(
            "This avatar is a generator chassis — the classic construction-kit \
             body. A rigged body is the parametric skinned kind: sculpted by \
             sliders, animated procedurally, dressable at sockets.",
        );
        ui.add_space(4.0);
        match did {
            Some(did) => {
                if ui.button("Wear a rigged body").clicked() {
                    let did = did.to_string();
                    wear_new_engine_body(record, &did);
                    outcome.changed = true;
                    outcome.label = Some(String::from("wear rigged body"));
                }
                ui.small(
                    "Rolled from your identity — re-roll, sculpt and dress it \
                     from here. Saving publishes it to your wardrobe; the \
                     footer's Reset returns to a generator chassis.",
                );
            }
            None => {
                ui.small("Log in to wear a rigged body — it lives in your wardrobe.");
            }
        }
        return outcome;
    };

    if rig.resolved.is_none() {
        ui.label(
            "This avatar wears a rigged body whose wardrobe record could not \
             be resolved — it may have been deleted, or the fetch failed.",
        );
        if let Some(did) = did
            && ui.button("Wear a fresh body instead").clicked()
        {
            let did = did.to_string();
            wear_new_engine_body(record, &did);
            outcome.changed = true;
            outcome.label = Some(String::from("wear rigged body"));
        }
        return outcome;
    }

    // --- The engine sections --------------------------------------------
    // Sections write the engine record and report whether the BODY changed;
    // quantising to the wire grid is the host's half of the contract, paid
    // once below rather than inside every widget.
    egui::ScrollArea::vertical()
        .id_salt("body_tab_scroll")
        .show(ui, |ui| {
            let worn_rkey = record.body.rigged_ref().map(|r| r.avatar.clone());
            let Some(rig) = record.body.rigged_mut() else {
                return;
            };
            let Some(resolved) = rig.resolved.as_mut() else {
                return;
            };
            let engine = &mut resolved.body;

            let (rebuilt, noted) = sections::identity(ui, engine);
            let mut changed = rebuilt;
            ui.separator();
            changed |= sections::composite_axes(ui, engine);
            ui.separator();
            changed |= sections::body_axes(ui, &mut engine.archetype);
            changed |= sections::skin_axes(ui, engine);
            changed |= sections::eye_axes(ui, engine);
            changed |= sections::face_axes(ui, engine);
            changed |= sections::hair_axes(ui, engine);
            changed |= sections::outfit_axes(ui, engine);
            ui.separator();
            sections::derived(ui, engine);
            if changed || noted {
                engine.sanitize();
            }
            outcome.changed |= changed || noted;

            // --- Wardrobe ---------------------------------------------------
            ui.separator();
            egui::CollapsingHeader::new("wardrobe").show(ui, |ui| {
                let Some(did) = did else {
                    ui.small("Log in to browse your wardrobe.");
                    return;
                };
                ui.horizontal(|ui| {
                    let label = if listing.fetching {
                        "Refreshing…"
                    } else {
                        "Refresh"
                    };
                    if ui
                        .add_enabled(!listing.fetching, egui::Button::new(label))
                        .clicked()
                    {
                        outcome.wants_wardrobe_refresh = true;
                    }
                    // Branch the current look: a NEW rkey, same body — the next
                    // save publishes it as its own wardrobe entry and the old
                    // record stays untouched for whatever else wears it.
                    if ui
                        .button("Save as copy")
                        .on_hover_text("keep editing under a new wardrobe entry")
                        .clicked()
                    {
                        let fresh = crate::pds::tid::tid_now(crate::seeded_defaults::fnv1a_64(did));
                        rig.avatar = fresh;
                        outcome.changed = true;
                        outcome.label = Some(String::from("save body as copy"));
                    }
                });
                match listing.entries.as_ref() {
                    None if !listing.fetching => {
                        ui.small("Refresh to list the bodies published to your wardrobe.");
                    }
                    None => {}
                    Some(entries) if entries.is_empty() => {
                        ui.small("No bodies in the wardrobe yet — saving publishes this one.");
                    }
                    Some(entries) => {
                        for (rkey, body) in entries {
                            ui.horizontal(|ui| {
                                let worn = worn_rkey.as_deref() == Some(rkey.as_str());
                                let name = if body.name.trim().is_empty() {
                                    "(unnamed)"
                                } else {
                                    body.name.as_str()
                                };
                                if ui
                                    .selectable_label(worn, name)
                                    .on_hover_text(format!("wardrobe/{rkey}"))
                                    .clicked()
                                    && !worn
                                {
                                    rig.avatar = rkey.clone();
                                    resolved_replace(rig, body.clone());
                                    outcome.changed = true;
                                    outcome.label = Some(format!("wear {name}"));
                                }
                            });
                        }
                    }
                }
            });
        });

    outcome
}

/// Swap the worn body in a resolved rig, keeping the worn attachments — an
/// outfit belongs to the avatar record, not to the body it happens to dress.
fn resolved_replace(rig: &mut crate::pds::avatar::RiggedBody, body: EngineAvatarRecord) {
    match rig.resolved.as_mut() {
        Some(resolved) => resolved.body = body,
        None => {
            rig.resolved = Some(crate::pds::avatar::ResolvedRig {
                body,
                attachments: Vec::new(),
            });
        }
    }
}
