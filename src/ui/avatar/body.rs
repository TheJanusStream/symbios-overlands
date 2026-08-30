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
    /// Why the last fetch failed, if it did (#1141). Without this a
    /// failed walk cleared the spinner and left `entries` at `None` —
    /// the same state as never having fetched — so a PDS or auth error
    /// rendered as the neutral "Refresh to list…" hint and looked to the
    /// owner like they had simply not clicked yet.
    pub error: Option<String>,
    /// Whether a fetch has ever been asked for this session, so opening
    /// the wardrobe section fills it once instead of showing an empty
    /// list until someone finds the Refresh button.
    pub attempted: bool,
}

/// What the wardrobe section says about the list itself, above the rows.
///
/// A named state rather than a chain of match arms on `Option`s because
/// the bug was two states rendering identically: a failed fetch and a
/// fetch that never ran both left `entries` at `None`, so a PDS or auth
/// error read as "you forgot to click Refresh" (#1141). Separating them
/// here also makes the line the owner actually reads assertable, which a
/// branch inside the draw closure was not.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum WardrobeStatus {
    /// A fetch is in flight — the Refresh button already says so.
    Fetching,
    /// The last fetch failed, with the reason.
    Failed(String),
    /// Nothing has been asked for yet.
    Untried,
    /// Fetched, and the wardrobe is empty.
    Empty,
    /// Rows follow; they speak for themselves.
    Listed,
}

impl WardrobeStatus {
    /// The line rendered above the rows, verbatim, or `None` when the
    /// rows (or the button) already say it.
    pub(super) fn message(&self) -> Option<String> {
        match self {
            Self::Fetching | Self::Listed => None,
            Self::Failed(reason) => Some(format!("Couldn't load your wardrobe — {reason}")),
            Self::Untried => Some(String::from(
                "Refresh to list the bodies published to your wardrobe.",
            )),
            Self::Empty => Some(String::from(
                "No bodies in the wardrobe yet — saving publishes this one.",
            )),
        }
    }
}

impl WardrobeListing {
    /// Which of the five states this listing is in.
    ///
    /// A standing failure outranks a stale list: rows from an earlier
    /// fetch are still worth showing, but the owner has to be told they
    /// are not what the PDS holds now.
    pub(super) fn status(&self) -> WardrobeStatus {
        if let Some(reason) = self.error.as_deref() {
            return WardrobeStatus::Failed(reason.to_string());
        }
        match self.entries.as_deref() {
            Some([]) => WardrobeStatus::Empty,
            Some(_) => WardrobeStatus::Listed,
            None if self.fetching => WardrobeStatus::Fetching,
            None => WardrobeStatus::Untried,
        }
    }
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
                // Fill the list the first time the section is opened
                // (#1141). This closure only runs while the header is
                // expanded, so nothing is fetched until someone looks —
                // and `attempted` latches, so a fetch that fails is not
                // retried in a loop; the Refresh button is the retry.
                if !listing.attempted && !listing.fetching && listing.entries.is_none() {
                    outcome.wants_wardrobe_refresh = true;
                }
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
                // A failed walk says so, in the colour and shape the
                // mutuals picker uses for the same class of failure
                // (#1141). Shown above the rows rather than instead of
                // them: a refresh that fails over an already-loaded list
                // means those rows are stale, which is worth saying too.
                let status = listing.status();
                if let Some(message) = status.message() {
                    match status {
                        WardrobeStatus::Failed(_) => {
                            ui.colored_label(
                                crate::ui::theme::current(ui.ctx()).status.error,
                                message,
                            );
                            ui.small("Refresh to try again.");
                        }
                        _ => {
                            ui.small(message);
                        }
                    }
                }
                match listing.entries.as_ref() {
                    None => {}
                    Some(entries) if entries.is_empty() => {}
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

#[cfg(test)]
mod tests {
    use super::*;

    fn listing() -> WardrobeListing {
        WardrobeListing::default()
    }

    /// **A failed listing and a listing nobody asked for read
    /// differently** (#1141).
    ///
    /// Sequence from the finding: the PDS wardrobe walk fails during
    /// Loading (expired token, a 5xx, a DNS blip). Before this the Err
    /// arm was a `warn!` and nothing else — the spinner cleared,
    /// `entries` stayed `None`, and the tab re-showed "Refresh to list…",
    /// which is exactly what a pristine session shows. Clicking Refresh
    /// again produced the same silent nothing, so the owner had no way to
    /// tell a broken PDS from their own forgetfulness.
    ///
    /// Asserted on the rendered line rather than on the flags, because
    /// the two states differing internally was never the problem: they
    /// differed internally before too (`fetching` had been cleared). What
    /// they did not do was *say* anything different.
    #[test]
    fn a_failed_wardrobe_walk_does_not_read_as_a_pristine_one() {
        let pristine = listing().status();
        assert_eq!(pristine, WardrobeStatus::Untried);
        assert_eq!(
            pristine.message().as_deref(),
            Some("Refresh to list the bodies published to your wardrobe.")
        );

        let mut failed = listing();
        failed.attempted = true;
        failed.error = Some(crate::pds::FetchError::PdsError(500).to_string());
        let failed = failed.status();
        assert_ne!(
            failed.message(),
            pristine.message(),
            "the two states must not render the same words"
        );
        let message = failed.message().expect("a failure says something");
        assert!(
            message.starts_with("Couldn't load your wardrobe"),
            "names the failure: {message}"
        );
        assert!(
            message.contains("the PDS answered 500"),
            "and the reason, so a 500 and an expired token are not one hint: {message}"
        );
    }

    /// **A failed refresh over a loaded list keeps the rows and warns**
    /// (#1141).
    ///
    /// The rows are still the last thing the PDS confirmed, so throwing
    /// them away on a transient failure would lose more than it explains
    /// — but leaving them unannotated would present stale rows as
    /// current.
    #[test]
    fn a_failure_over_a_loaded_list_outranks_the_rows() {
        let mut stale = listing();
        stale.entries = Some(vec![(
            String::from("3jzfcijpj2z2a"),
            EngineAvatarRecord::default(),
        )]);
        stale.attempted = true;
        stale.error = Some(String::from("network — timed out"));
        assert!(matches!(stale.status(), WardrobeStatus::Failed(_)));
        assert!(
            stale.entries.is_some(),
            "the rows survive the failure that could not replace them"
        );
    }

    /// **The in-flight and empty states keep their own wording** (#1141).
    ///
    /// The control for the test above: separating "failed" out must not
    /// have collapsed the three states that were already distinct.
    #[test]
    fn the_other_listing_states_are_unchanged() {
        let mut fetching = listing();
        fetching.fetching = true;
        assert_eq!(fetching.status(), WardrobeStatus::Fetching);
        assert_eq!(
            fetching.status().message(),
            None,
            "the Refresh button already says 'Refreshing…'"
        );

        let mut empty = listing();
        empty.entries = Some(Vec::new());
        assert_eq!(empty.status(), WardrobeStatus::Empty);
        assert_eq!(
            empty.status().message().as_deref(),
            Some("No bodies in the wardrobe yet — saving publishes this one.")
        );
    }
}
