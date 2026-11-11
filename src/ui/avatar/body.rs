//! The Body tab: the rigged engine body in overlands chrome (#1059).
//!
//! Every axis widget here is the sibling crate's own - the public sections
//! `bevy_symbios_avatar::editor` exposes (upstream #231), hosted inside this
//! window's theme, undo ring and debounce instead of the viewer's
//! `SidePanel`. That is the one-source-of-truth contract: a new engine axis
//! learns to draw itself exactly once, upstream, and appears here on the
//! next dependency bump.
//!
//! The tab has three shapes:
//!
//!   - a **generator body**: one affordance - wear a rigged body. Wearing
//!     mints a fresh wardrobe rkey and resolves the DID-seeded engine
//!     default locally; nothing touches the PDS until Save publishes the
//!     bundle. The classic chassis stays reachable through the footer's
//!     Reset and the re-roll section, both of which build generator bodies.
//!   - a **rigged body, unresolved** (offline fetch, deleted reference):
//!     say so, and offer a fresh body.
//!   - a **rigged body, resolved**: the engine sections, the derived
//!     readout, and the wardrobe - every body this identity has published,
//!     wearable with a click, plus save-as-copy for branching a look.
//!
//! Archetype exposure: the engine identity section carries the body
//! choice - a person, a four-legged beast or a winged body (bird, bat,
//! glider or a cross) - and every one builds and animates on the same rig
//! path (#1057/#1058 run on the rig, not on an anatomy), so none is hidden
//! here. The viewer's sections draw for the body they belong to and nothing
//! on any other: since bevy_symbios_avatar 0.15 the tab hosts its `coat_axes`
//! and `fur_axes`, in the viewer's own order after the skin, which draw only
//! for a quadruped (#1801); since 0.25 its `body_axes` carry a winged body's
//! family, size and shape with a bird's plumage or a bat's and a glider's
//! coat (#1876). The face, hair and outfit hide on every body without a
//! person's head or trunk, and the identity row says which it hides.
//!
//! Switching between kinds - person, beast and winged, either way - rolls the
//! new body from the body's own seed with every lock lifted, and every edit
//! of a beast or a winged body refits its on-foot collider to it in the same
//! undo step (#1803, owner decisions D3 and D6; #1875, #1876) - see
//! [`draw_body_tab`]. A change of winged family is no switch: it is the
//! viewer's family picker, and refits as any edit does. The Locomotion tab
//! then reads the fitted collider of a beast or a winged body rather than
//! offering a person's capsule (`super::locomotion`).

use bevy::prelude::Commands;
use bevy_egui::egui;
use bevy_symbios_avatar::editor as sections;

use super::TabCtx;
use crate::pds::AvatarRecord;
use crate::pds::avatar::RiggedKind;
use crate::pds::avatar::wardrobe::{EngineAvatarRecord, wear_new_engine_body};

/// The Body tab, wired to the editor (#1161).
///
/// [`draw_body_tab`] below is the drawing half - it reports what the owner
/// did through a [`BodyTabOutcome`] and touches nothing else. This is the
/// half that acts on it: the undo label, the toast, the change flag and the
/// wardrobe refresh, which is a `Commands` spawn and so cannot live inside
/// a function that only borrows the record.
pub(super) fn draw_tab(
    ui: &mut egui::Ui,
    ctx: &mut TabCtx,
    height: f32,
    listing: &mut WardrobeListing,
    build_failed: bool,
    commands: &mut Commands,
) {
    ui.allocate_ui(egui::vec2(ui.available_width(), height), |ui| {
        let outcome = draw_body_tab(ui, ctx.record, listing, ctx.did, build_failed);
        *ctx.changed |= outcome.changed;
        if let Some(label) = outcome.label {
            ctx.labels.set_avatar(label);
        }
        if let Some(text) = outcome.toast {
            ctx.toasts.success(text, ctx.now);
        }
        if outcome.wants_wardrobe_refresh
            && let Some(did) = ctx.did
        {
            listing.fetching = true;
            listing.attempted = true;
            // Clear the last failure as the retry starts, so the error line
            // describes the attempt in flight and not the one before it
            // (#1141).
            listing.error = None;
            super::spawn_wardrobe_list_task(commands, did);
        }
    });
}

/// What the Body tab did this frame.
#[derive(Default)]
pub(super) struct BodyTabOutcome {
    /// The record changed in a way that changes the built body.
    pub changed: bool,
    /// A label for the undo entry, on discrete actions.
    pub label: Option<String>,
    /// The wardrobe "Refresh" button was clicked.
    pub wants_wardrobe_refresh: bool,
    /// A success to toast, for actions whose only other visible result is
    /// a highlight moving (#1201). The caller owns the toast stack.
    pub toast: Option<String>,
}

/// The standing "your body could not be built" banner (#1255).
///
/// Same frame idiom as the record-recovery banner one level up in
/// `avatar::mod` - a `danger_surface` fill with its paired text colour, so
/// the severity survives the high-contrast palette and does not depend on
/// hue alone. The engine returns a bare `None` with no reason value, so the
/// copy names the one documented cause rather than inventing a diagnosis it
/// cannot have.
fn build_failed_banner(ui: &mut egui::Ui) {
    let theme = crate::ui::theme::current(ui.ctx());
    egui::Frame::new()
        .fill(theme.danger_surface)
        .inner_margin(6.0)
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.colored_label(
                theme.danger_surface_text,
                "⚠ This body can't be built at these proportions.",
            );
            ui.label(
                egui::RichText::new(
                    "Limbs overlapping at a joint is the usual cause. Press Ctrl+Z, or move \
                     the shape sliders back - the body rebuilds as soon as the numbers work.",
                )
                .small(),
            );
        });
    ui.add_space(4.0);
}

/// The name a branched wardrobe entry gets (#1201): the parent's name with
/// " copy", numbered past the first, and never one the wardrobe already
/// lists - a wardrobe of identically-named rows is unusable, and an empty
/// name would publish as the "Wanderer" fallback beside its parent.
pub(super) fn copy_name(parent: &str, is_taken: impl Fn(&str) -> bool) -> String {
    let base = parent.trim();
    let base = if base.is_empty() { "Wanderer" } else { base };
    let first = format!("{base} copy");
    if !is_taken(&first) {
        return first;
    }
    for n in 2..1000 {
        let candidate = format!("{base} copy {n}");
        if !is_taken(&candidate) {
            return candidate;
        }
    }
    first
}

/// The fetched wardrobe listing, cached across frames on the editor state.
#[derive(Default)]
pub(super) struct WardrobeListing {
    /// `(rkey, record)` pairs in creation order, once a fetch has landed.
    pub entries: Option<Vec<(String, EngineAvatarRecord)>>,
    /// Whether a fetch is in flight, for the button's spinner text.
    pub fetching: bool,
    /// Why the last fetch failed, if it did (#1141). Without this a
    /// failed walk cleared the spinner and left `entries` at `None` -
    /// the same state as never having fetched - so a PDS or auth error
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
    /// A fetch is in flight - the Refresh button already says so.
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
            Self::Failed(reason) => Some(format!("Couldn't load your wardrobe - {reason}")),
            Self::Untried => Some(String::from(
                "Refresh to list the bodies saved to your wardrobe.",
            )),
            Self::Empty => Some(String::from(
                "No bodies in the wardrobe yet - saving stores this one.",
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
///
/// `build_failed` is the standing state of the owner's own body: the last
/// build the engine ran for this record produced nothing (#1255). It is
/// drawn here, above the sliders, because the sliders are what caused it and
/// are the only way out - the toast that fires at the moment of failure has
/// expired long before an owner works out which axis to move.
///
/// **Locomotion follows the body** (#1803, owner decision D3): a frame that
/// changes the worn body refits the on-foot preset to it before it returns -
/// a beast's collider, mass, jump and run on every edit, a person's capsule
/// stood back up when a beast becomes one, and nothing at all on a person's
/// own edits ([`crate::pds::avatar::locomotion::follow`]). It is the same
/// frame, so the same change tick and the same undo step: the editor
/// flushes the record once its debounce drains, and the undo ring snapshots
/// it whole at that tick. A tuned beast's run is carried from where its
/// owner left it ([`crate::pds::avatar::locomotion::RunAnchor`]), held in
/// the tab's egui memory across the frames of a drag, so a drag that comes
/// back returns the run exactly.
pub(super) fn draw_body_tab(
    ui: &mut egui::Ui,
    record: &mut AvatarRecord,
    listing: &mut WardrobeListing,
    did: Option<&str>,
    build_failed: bool,
) -> BodyTabOutcome {
    use crate::pds::avatar::locomotion::RunAnchor;
    // The body as it was, which the run of a tuned beast is carried across
    // an edit from. Only the engine record is copied, never the outfit.
    let before = worn_body(record).cloned();
    let outcome = draw_body_sections(ui, record, listing, did, build_failed, before.as_ref());
    if outcome.changed {
        let id = egui::Id::new("avatar_body_run_anchor");
        let mut anchor = ui.ctx().data(|d| d.get_temp::<RunAnchor>(id));
        crate::pds::avatar::locomotion::follow_body(record, before.as_ref(), &mut anchor);
        ui.ctx().data_mut(|d| match anchor {
            Some(anchor) => {
                d.insert_temp(id, anchor);
            }
            None => d.remove::<RunAnchor>(id),
        });
    }
    outcome
}

/// The engine body `record` wears, when it is resolved.
fn worn_body(record: &AvatarRecord) -> Option<&EngineAvatarRecord> {
    record
        .body
        .rigged_ref()
        .and_then(|rig| rig.resolved.as_ref())
        .map(|resolved| &resolved.body)
}

/// Roll the body a switch of kind gives, from the body's own seed (#1803,
/// owner decision D6), in both directions: the viewer's `identity` toggle
/// sets the same default dog, or the same default person, every time. The
/// engine's own re-roll, as the seed arrows draw it, but with EVERY lock
/// lifted for this one roll - a lock held a value of the body it was set
/// on, and across the switch that value is the toggle's default dog's or
/// default person's, so a held Stature would stand every beast at the
/// default dog's height and a held Colouring colour every one of them red.
/// The creator's lock set is put back after it, so the seed arrows from
/// here on keep the categories they locked, over the body this rolled.
fn roll_switched(body: &mut EngineAvatarRecord) {
    let locks = body.locks;
    body.locks = symbios_avatar::LockSet::NONE;
    body.reroll(body.seed);
    body.locks = locks;
}

/// What kind of body `body` is stored as, for telling a switch of kind
/// (#1803, #1875): a person, a four-legged beast or a winged body, whichever
/// family leads it - a change of winged family is no switch, it is the
/// viewer's family picker. A body this build cannot read counts as the
/// person it is drawn as, so picking a person on one rolls nothing, as
/// before.
fn body_kind(body: &EngineAvatarRecord) -> RiggedKind {
    match RiggedKind::of_record(body) {
        RiggedKind::Unknown => RiggedKind::Person,
        RiggedKind::Winged(_) => RiggedKind::Winged(symbios_avatar::WingedFamily::Bird),
        kind => kind,
    }
}

/// The Body tab's own drawing, before locomotion follows what it did.
///
/// `before` is the engine body as the frame began; the engine sections'
/// change is reported only where the body now differs from it (#1811).
fn draw_body_sections(
    ui: &mut egui::Ui,
    record: &mut AvatarRecord,
    listing: &mut WardrobeListing,
    did: Option<&str>,
    build_failed: bool,
    before: Option<&EngineAvatarRecord>,
) -> BodyTabOutcome {
    let mut outcome = BodyTabOutcome::default();
    if build_failed {
        build_failed_banner(ui);
    }

    let Some(rig) = record.body.rigged_ref() else {
        ui.label(
            "This avatar has a construction-kit body - the classic one, built \
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
                    "Rolled from your identity - re-roll, sculpt and dress it \
                     from here. Saving stores it in your wardrobe; the \
                     footer's Reset returns to a construction-kit body.",
                );
            }
            None => {
                ui.small("Log in to wear a rigged body - it lives in your wardrobe.");
            }
        }
        return outcome;
    };

    if rig.resolved.is_none() {
        ui.label(
            "This avatar wears a rigged body whose wardrobe record could not \
             be resolved - it may have been deleted, or the fetch failed.",
        );
        if let Some(did) = did {
            let worn = rig.attachments.len();
            if ui.button("Wear a fresh body instead").clicked() {
                let did = did.to_string();
                wear_new_engine_body(record, &did);
                outcome.changed = true;
                outcome.label = Some(String::from("wear rigged body"));
            }
            // Says what the swap keeps (#1201): the button used to read as
            // "start over", and it once did - dropping the reference list
            // and, at the next Save, every worn prop's record.
            ui.small(match worn {
                0 => String::from("A fresh DID-seeded body. Nothing is worn, so nothing is lost."),
                1 => String::from(
                    "A fresh DID-seeded body under your outfit - the 1 prop you wear \
                     stays referenced and returns when its record loads.",
                ),
                n => format!(
                    "A fresh DID-seeded body under your outfit - the {n} props you wear \
                     stay referenced and return when their records load."
                ),
            });
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

            // #1256 f107: the engine's `identity` section carries its own
            // `seed` DragValue with ◀/▶ arrows and its own `locked`
            // category row - stature/build/frame/… - while the window
            // hosting it has a "Whole-avatar seed" block eight rows above
            // with Chassis/Style/Ornateness/Wear locks. Same word, same
            // window, two scopes, and one of them replaces the entire
            // record. The host cannot rename an upstream widget, but it can
            // say which seed this is before drawing it.
            ui.label(egui::RichText::new("Body seed - sculpting only").strong());
            ui.small(
                "Rolls this body's proportions and colouring. The whole-avatar seed above \
                 replaces the entire avatar, including which kind of body it is.",
            );
            let was = body_kind(engine);
            let (rebuilt, noted) = sections::identity(ui, engine);
            // The viewer's body toggle reports a change and nothing about
            // what it was, so the switch is read off the archetype either
            // side of the draw (#1803): a kind, not a bool, since the toggle
            // offers a winged body too (#1875), and a beast made winged was
            // once named a switch to a person.
            let now = body_kind(engine);
            if now != was {
                roll_switched(engine);
                outcome.label = Some(String::from(match now {
                    RiggedKind::Beast => "switch to a four-legged body",
                    RiggedKind::Winged(_) => "switch to a winged body",
                    _ => "switch to a person",
                }));
            }
            let mut changed = rebuilt;
            ui.separator();
            changed |= sections::composite_axes(ui, engine);
            ui.separator();
            changed |= sections::body_axes(ui, engine);
            changed |= sections::skin_axes(ui, engine);
            // A beast's coat and long fur (bevy_symbios_avatar 0.15), in the
            // viewer's own order; each draws nothing on a person.
            changed |= sections::coat_axes(ui, engine);
            changed |= sections::fur_axes(ui, engine);
            changed |= sections::eye_axes(ui, engine);
            changed |= sections::face_axes(ui, engine);
            changed |= sections::hair_axes(ui, engine);
            changed |= sections::outfit_axes(ui, engine);
            ui.separator();
            sections::derived(ui, engine);
            if changed || noted {
                engine.sanitize();
            }
            // A section's report is a claim and the body is the fact
            // (#1811): the Fabric panel's colour pickers, held open with
            // nothing touched, report a change on every frame - egui's
            // linear colour button (the texture crate's) hands back its own
            // HSV round trip, and the garment writes back the surface it
            // parses from that, which lands on the very value it was. Taken
            // at its word, that re-armed the editor's debounce and named a
            // "body edit" every frame for a body nobody edited, so the report
            // stands only where the sanitized body differs from the one the
            // frame began with.
            outcome.changed |= (changed || noted) && before.is_none_or(|was| was != &*engine);

            // --- Wardrobe ---------------------------------------------------
            ui.separator();
            egui::CollapsingHeader::new("Wardrobe").show(ui, |ui| {
                let Some(did) = did else {
                    ui.small("Log in to browse your wardrobe.");
                    return;
                };
                // Fill the list the first time the section is opened
                // (#1141). This closure only runs while the header is
                // expanded, so nothing is fetched until someone looks -
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
                        .on_disabled_hover_text("Already refreshing your saved looks")
                        .clicked()
                    {
                        outcome.wants_wardrobe_refresh = true;
                    }
                    // Branch the current look: a NEW rkey, same body - the next
                    // save publishes it as its own wardrobe entry and the old
                    // record stays untouched for whatever else wears it.
                    if ui
                        .button("Save as copy")
                        .on_hover_text(
                            "Keep editing under a new wardrobe entry, named after this one.",
                        )
                        .clicked()
                    {
                        let fresh = crate::pds::tid::tid_now(crate::seeded_defaults::fnv1a_64(did));
                        rig.avatar = fresh;
                        // A copy gets its own name and says so (#1201): the
                        // only visible result used to be the worn row's
                        // highlight going out, and the copy published under
                        // its parent's name.
                        let taken: Vec<String> = listing
                            .entries
                            .as_deref()
                            .map(|entries| {
                                entries.iter().map(|(_, body)| body.name.clone()).collect()
                            })
                            .unwrap_or_default();
                        let name = rig
                            .resolved
                            .as_mut()
                            .map(|resolved| {
                                let name = copy_name(&resolved.body.name, |candidate| {
                                    taken.iter().any(|t| t == candidate)
                                });
                                resolved.body.name = name.clone();
                                name
                            })
                            .unwrap_or_default();
                        outcome.toast = Some(format!(
                            "Branched to a new wardrobe entry \"{name}\" - Save to store it."
                        ));
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
                                    .on_hover_text(if worn {
                                        String::from("You're wearing this one")
                                    } else {
                                        format!("Wear \"{name}\" instead")
                                    })
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

/// Swap the worn body in a resolved rig, keeping the worn attachments - an
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
    use crate::pds::avatar::LocomotionPreset;
    use crate::ui::headless::Headless as _;

    fn listing() -> WardrobeListing {
        WardrobeListing::default()
    }

    /// **A failed listing and a listing nobody asked for read
    /// differently** (#1141).
    ///
    /// Sequence from the finding: the PDS wardrobe walk fails during
    /// Loading (expired token, a 5xx, a DNS blip). Before this the Err
    /// arm was a `warn!` and nothing else - the spinner cleared,
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
            Some("Refresh to list the bodies saved to your wardrobe.")
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
            message.contains("the data server answered 500"),
            "and the reason, so a 500 and an expired token are not one hint: {message}"
        );
    }

    /// **A failed refresh over a loaded list keeps the rows and warns**
    /// (#1141).
    ///
    /// The rows are still the last thing the PDS confirmed, so throwing
    /// them away on a transient failure would lose more than it explains -
    /// but leaving them unannotated would present stale rows as
    /// current.
    #[test]
    fn a_failure_over_a_loaded_list_outranks_the_rows() {
        let mut stale = listing();
        stale.entries = Some(vec![(
            String::from("3jzfcijpj2z2a"),
            EngineAvatarRecord::default(),
        )]);
        stale.attempted = true;
        stale.error = Some(String::from("network - timed out"));
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
            Some("No bodies in the wardrobe yet - saving stores this one.")
        );
    }

    /// #1201 (finding 113): "Save as copy" published the branch under its
    /// parent's name (or "Wanderer" for an empty one), so the wardrobe
    /// filled with identically-named rows. A copy is named after its parent
    /// and never collides with a listed entry.
    #[test]
    fn a_wardrobe_copy_is_named_after_its_parent_and_never_collides() {
        let listed = ["Alder", "Alder copy", "Alder copy 2"];
        let taken = |name: &str| listed.contains(&name);
        assert_eq!(copy_name("Alder", taken), "Alder copy 3");
        assert_eq!(copy_name("Birch", taken), "Birch copy");
        assert_eq!(copy_name("   ", taken), "Wanderer copy");
    }

    /// The Body tab's section headers, drawn headless over `record` for
    /// two passes with every collapsing body open, and whether the tab
    /// reported a change it was never asked to make.
    fn hosted_sections(record: &mut AvatarRecord) -> (Vec<String>, bool) {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.memory_mut(|m| m.set_everything_is_visible(true));
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 4000.0));
        let mut listing = listing();
        let mut changed = false;
        let mut labels = Vec::new();
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            let output = ctx.run_headless(input, |ui| {
                changed |= draw_body_tab(ui, record, &mut listing, None, false).changed;
            });
            let update = output
                .platform_output
                .accesskit_update
                .as_ref()
                .expect("accesskit was enabled, so the pass has a tree");
            labels = update
                .nodes
                .iter()
                .filter(|(_, n)| {
                    n.role() == bevy_egui::egui::accesskit::Role::Button && n.toggled().is_none()
                })
                .filter_map(|(_, n)| n.label().map(str::to_string))
                .collect();
        }
        labels.sort();
        labels.dedup();
        (labels, changed)
    }

    fn wearing(body: EngineAvatarRecord) -> AvatarRecord {
        let mut record = AvatarRecord::wearing("3jzfcijpj2z2a");
        if let Some(rig) = record.body.rigged_mut() {
            rig.resolved = Some(crate::pds::avatar::ResolvedRig {
                body,
                attachments: Vec::new(),
            });
        }
        record
    }

    /// **A beast's coat and fur are on the Body tab** (#1801), hosted from
    /// `bevy_symbios_avatar::editor` in the viewer's own order, and a
    /// person's tab draws neither. The editor-current rule: a section the
    /// viewer gained is a schema change the host's editor must carry, and
    /// without `coat_axes` and `fur_axes` a quadruped's coat loci and fur
    /// lengths were stored on its record with no control anywhere in the
    /// app to change them. Drawing either tab touches nothing.
    #[test]
    fn a_beast_wears_its_coat_and_fur_on_the_body_tab_and_a_person_does_not() {
        let beast = EngineAvatarRecord::rolled(
            "Beast",
            symbios_avatar::Archetype::Quadruped(symbios_avatar::QuadrupedParams::default()),
            7,
        );
        let mut record = wearing(beast);
        let before = record.clone();
        let (labels, changed) = hosted_sections(&mut record);
        for section in ["coat", "fur"] {
            assert!(
                labels.iter().any(|l| l == section),
                "a quadruped's Body tab hosts the {section:?} section; drawn: {labels:?}"
            );
        }
        assert!(!changed, "drawing a beast's Body tab reported a change");
        assert_eq!(
            record, before,
            "drawing a beast's Body tab rewrote its record"
        );

        let mut person = wearing(crate::pds::avatar::wardrobe::engine_default_for_did(
            "did:plc:plan-test",
        ));
        let (labels, changed) = hosted_sections(&mut person);
        for section in ["coat", "fur"] {
            assert!(
                !labels.iter().any(|l| l == section),
                "a person's Body tab draws no {section:?} section; drawn: {labels:?}"
            );
        }
        assert!(!changed, "drawing a person's Body tab reported a change");
    }

    /// What one driven run of the Body tab did: whether it reported a
    /// change, and the undo label it named.
    struct Driven {
        changed: bool,
        label: Option<String>,
    }

    /// Draw the Body tab headless over `record` and click the control whose
    /// AccessKit label is `click` - through an AccessKit action, as a screen
    /// reader would - then draw it twice more against the result.
    fn drive(record: &mut AvatarRecord, click: &str) -> Driven {
        use bevy_egui::egui::accesskit::{Action, ActionRequest, Role, TreeId};
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 4000.0));
        let mut listing = listing();
        let mut driven = Driven {
            changed: false,
            label: None,
        };
        let mut events = Vec::new();
        let mut clicked = false;
        for _ in 0..4 {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events: std::mem::take(&mut events),
                ..Default::default()
            };
            let output = ctx.run_headless(input, |ui| {
                let outcome = draw_body_tab(ui, record, &mut listing, None, false);
                driven.changed |= outcome.changed;
                driven.label = outcome.label.or(driven.label.take());
            });
            if !clicked {
                let update = output
                    .platform_output
                    .accesskit_update
                    .as_ref()
                    .expect("accesskit was enabled, so the pass has a tree");
                let target = update
                    .nodes
                    .iter()
                    .find(|(_, n)| n.role() == Role::Button && n.label() == Some(click))
                    .map(|(id, _)| *id)
                    .unwrap_or_else(|| panic!("the Body tab draws no {click:?}"));
                events.push(egui::Event::AccessKitActionRequest(ActionRequest {
                    action: Action::Click,
                    target_tree: TreeId::ROOT,
                    target_node: target,
                    data: None,
                }));
                clicked = true;
            }
        }
        driven
    }

    /// The on-foot preset `record` carries.
    fn on_foot(record: &AvatarRecord) -> &crate::pds::HumanoidParams {
        match &record.locomotion {
            crate::pds::LocomotionConfig::Humanoid(p) => p,
            other => panic!("not on foot: {}", other.kind_tag()),
        }
    }

    /// A person on foot whose owner tuned every field the fit could touch,
    /// so a refit of any of them shows.
    fn tuned_person(did: &str) -> AvatarRecord {
        use crate::pds::types::Fp;
        let mut record = wearing(crate::pds::avatar::wardrobe::engine_default_for_did(did));
        record.locomotion = crate::pds::HumanoidParams {
            capsule_radius: Fp(0.31),
            capsule_length: Fp(1.07),
            mass: Fp(71.0),
            walk_speed: Fp(6.2),
            jump_impulse: Fp(520.0),
            acceleration: Fp(14.5),
            turn_rate: Fp(9.0),
            ..crate::pds::HumanoidParams::default()
        }
        .into_config();
        record
    }

    /// The avatar undo ring as the app runs it: `capture_avatar_history`
    /// over `LiveAvatarRecord`, which snapshots the whole record at each
    /// change tick. `before` is the baseline; `after` lands on ONE tick, as
    /// the editor's flush writes a frame's edits; then one undo.
    fn one_undo(before: &AvatarRecord, after: &AvatarRecord) -> (usize, AvatarRecord) {
        use bevy::prelude::*;
        let mut app = App::new();
        app.insert_resource(crate::state::LiveAvatarRecord(before.clone()));
        app.init_resource::<crate::ui::avatar::AvatarEditorState>();
        app.init_resource::<crate::ui::undo::AvatarUndoHistory>();
        app.init_resource::<crate::ui::undo::PendingUndoLabels>();
        app.add_systems(Update, crate::ui::undo::capture_avatar_history);
        app.update();
        {
            let mut live = app
                .world_mut()
                .resource_mut::<crate::state::LiveAvatarRecord>();
            live.bypass_change_detection().0 = after.clone();
            live.set_changed();
        }
        app.update();
        let mut history = app
            .world_mut()
            .resource_mut::<crate::ui::undo::AvatarUndoHistory>();
        let steps = history.len();
        let (entry, _) = history.undo().expect("the edit is undoable");
        (steps, entry.record.clone())
    }

    /// **Switching to a quadruped rolls a beast from the body's seed and
    /// stands it on a collider fitted to it, and one undo restores the
    /// person and their capsule** (#1803, owner decisions D3 and D6).
    ///
    /// The viewer's toggle alone sets `QuadrupedParams::default()` - the
    /// same dog for every seed - and leaves the person's upright capsule
    /// under it. Here the switch is the viewer's own `quadruped` toggle,
    /// clicked: the body is the beast the engine's re-roll draws from the
    /// person's seed, the preset lies along it as `fit_on_foot` sizes it,
    /// and both landed in the one draw that made the switch, so the
    /// record's one change tick carries both and one undo returns the
    /// tuned person to the bit.
    #[test]
    fn switching_to_a_beast_rolls_one_and_one_undo_restores_the_person() {
        let mut beasts = Vec::new();
        for did in ["did:plc:kind-switch", "did:plc:kind-switch-2"] {
            let person = tuned_person(did);
            let mut record = person.clone();
            let driven = drive(&mut record, "quadruped");
            assert!(driven.changed, "the switch reported no change");
            assert_eq!(
                driven.label.as_deref(),
                Some("switch to a four-legged body")
            );
            let body = worn_body(&record).expect("still resolved").clone();
            let mut want = worn_body(&person).expect("resolved").clone();
            want.archetype =
                symbios_avatar::Archetype::Quadruped(symbios_avatar::QuadrupedParams::default());
            roll_switched(&mut want);
            want.sanitize();
            assert_eq!(body, want, "{did}: not the beast the person's seed rolls");
            assert_ne!(
                body.archetype,
                symbios_avatar::Archetype::Quadruped(symbios_avatar::QuadrupedParams::default()),
                "{did}: the default dog, not a rolled beast"
            );
            let fitted = crate::pds::avatar::fit_on_foot(&body, on_foot(&person));
            assert!(fitted.is_lying(), "{did}: the premise, a beast that rigs");
            assert_eq!(
                on_foot(&record),
                &fitted,
                "{did}: the beast does not stand on its own collider"
            );
            let (steps, undone) = one_undo(&person, &record);
            assert_eq!(steps, 2, "the baseline and one step");
            assert_eq!(undone, person, "{did}: one undo is not the person back");
            assert_eq!(
                serde_json::to_string(&undone).expect("writes"),
                serde_json::to_string(&person).expect("writes"),
            );
            beasts.push(body.archetype);
        }
        assert_ne!(beasts[0], beasts[1], "two seeds rolled one beast");
    }

    /// **Every edit of a beast's body refits its collider in the same
    /// step** (#1803, D3): the seed arrow redraws the beast, and the
    /// collider, the mass, the jump and the run are the new body's - in
    /// the draw that made the edit, so one undo takes back both.
    #[test]
    fn editing_a_beasts_body_refits_it_in_one_step() {
        let mut beast = wearing(EngineAvatarRecord::rolled(
            "Beast",
            symbios_avatar::Archetype::Quadruped(symbios_avatar::QuadrupedParams::default()),
            11,
        ));
        let fitted = crate::pds::avatar::fit_on_foot(
            worn_body(&beast).expect("resolved"),
            &crate::pds::HumanoidParams::default(),
        );
        beast.locomotion = fitted.clone().into_config();
        let mut record = beast.clone();
        let driven = drive(&mut record, "▶");
        assert!(driven.changed, "the seed arrow reported no change");
        let body = worn_body(&record).expect("resolved");
        assert_ne!(body, worn_body(&beast).expect("resolved"), "the premise");
        let refit = crate::pds::avatar::fit_on_foot(body, &fitted);
        assert_ne!(refit, fitted, "the premise: the new body sizes differently");
        assert_eq!(on_foot(&record), &refit, "the collider did not follow");
        let (steps, undone) = one_undo(&beast, &record);
        assert_eq!(steps, 2, "the baseline and one step");
        assert_eq!(undone, beast, "one undo is not the beast before");
    }

    /// **A person's edits never refit them** (#1803, D3): the seed arrow
    /// redraws their body - a new stature among it - and their capsule,
    /// mass, jump and run stay whatever their owner set, so the record
    /// writes the bytes it wrote before the edit.
    #[test]
    fn a_persons_edits_never_refit_them() {
        for did in [
            "did:plc:kind-switch",
            "did:plc:plan-test",
            "did:plc:stop-test",
        ] {
            let person = tuned_person(did);
            let mut record = person.clone();
            let driven = drive(&mut record, "▶");
            assert!(driven.changed, "{did}: the seed arrow reported no change");
            let (before, after) = (worn_body(&person), worn_body(&record));
            let stature = |body: Option<&EngineAvatarRecord>| match body.map(|b| &b.archetype) {
                Some(symbios_avatar::Archetype::Humanoid(p)) => p.height,
                _ => panic!("{did}: not a person"),
            };
            assert_ne!(stature(before), stature(after), "{did}: the premise");
            assert_eq!(record.locomotion, person.locomotion, "{did}: refitted");
            assert_eq!(
                serde_json::to_string(&record).expect("writes"),
                serde_json::to_string(&person).expect("writes"),
                "{did}: a person's edit moved the record's bytes"
            );
        }
    }

    /// **A person who flies keeps their section and their tuning through an
    /// edit of their body** (#1875's critic, #1861): the seed arrow redraws a
    /// tuned person wearing the prototype's flight section, and nothing of
    /// their preset moves - an on-foot body of any kind may fly, and only a
    /// body that was winged has a winged body's section to lose.
    #[test]
    fn a_flying_persons_edits_never_refit_them() {
        for did in ["did:plc:kind-switch", "did:plc:plan-test"] {
            let mut person = tuned_person(did);
            if let crate::pds::LocomotionConfig::Humanoid(p) = &mut person.locomotion {
                p.flight = Some(crate::pds::avatar::locomotion::FlightParams::default());
            }
            let mut record = person.clone();
            let driven = drive(&mut record, "\u{25b6}");
            assert!(driven.changed, "{did}: the seed arrow reported no change");
            assert_ne!(worn_body(&person), worn_body(&record), "{did}: the premise");
            assert_eq!(record.locomotion, person.locomotion, "{did}: refitted");
        }
    }

    /// **A beast that becomes a person stands back up as one** (#1803): the
    /// stature rule's capsule, upright, and the default person's mass, jump
    /// and run - not the beast's, which the fit would otherwise keep - with
    /// every tuning the body does not size left as it was.
    #[test]
    fn a_beast_that_becomes_a_person_stands_up_as_one() {
        use crate::pds::types::Fp;
        let mut beast = wearing(EngineAvatarRecord::rolled(
            "Beast",
            symbios_avatar::Archetype::Quadruped(symbios_avatar::QuadrupedParams::default()),
            11,
        ));
        let mut fitted = crate::pds::avatar::fit_on_foot(
            worn_body(&beast).expect("resolved"),
            &crate::pds::HumanoidParams::default(),
        );
        fitted.acceleration = Fp(17.5);
        beast.locomotion = fitted.clone().into_config();
        let mut record = beast.clone();
        let driven = drive(&mut record, "humanoid");
        assert_eq!(driven.label.as_deref(), Some("switch to a person"));
        let p = on_foot(&record);
        let person = crate::pds::HumanoidParams::default();
        assert!(!p.is_lying(), "the person still lies along a capsule");
        assert_eq!(
            (p.mass, p.jump_impulse, p.walk_speed),
            (person.mass, person.jump_impulse, person.walk_speed),
            "the person kept the beast's weight, jump or run"
        );
        assert_eq!(
            p.acceleration,
            Fp(17.5),
            "a tuning the body does not size moved"
        );
        assert_eq!(
            p,
            &crate::pds::avatar::fit_on_foot(
                worn_body(&record).expect("resolved"),
                &crate::pds::HumanoidParams {
                    acceleration: Fp(17.5),
                    ..person
                }
            ),
            "not the stature rule"
        );
    }

    /// One egui context drawn frame after frame, as the editor draws the
    /// tab - what a gesture across many frames needs, where [`drive`]
    /// starts every run afresh.
    struct Tab {
        ctx: egui::Context,
        listing: WardrobeListing,
    }

    impl Tab {
        fn new() -> Self {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            ctx.memory_mut(|m| m.set_everything_is_visible(true));
            Self {
                ctx,
                listing: listing(),
            }
        }

        /// One frame over `record` with `events` as its input: what the tab
        /// reported, and the AccessKit node of every control it drew, by
        /// label.
        fn frame(
            &mut self,
            record: &mut AvatarRecord,
            events: Vec<egui::Event>,
        ) -> (BodyTabOutcome, Vec<(egui::accesskit::NodeId, String)>) {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 6000.0),
                )),
                events,
                ..Default::default()
            };
            let mut outcome = BodyTabOutcome::default();
            let listing = &mut self.listing;
            let output = self.ctx.run_headless(input, |ui| {
                let drawn = draw_body_tab(ui, record, listing, None, false);
                outcome.changed |= drawn.changed;
                outcome.label = drawn.label.or(outcome.label.take());
            });
            let update = output
                .platform_output
                .accesskit_update
                .expect("accesskit was enabled, so the pass has a tree");
            let mut controls: Vec<_> = update
                .nodes
                .iter()
                .filter_map(|(id, n)| n.label().map(|label| (*id, label.to_string())))
                .collect();
            controls.sort_by_key(|(id, _)| id.0);
            (outcome, controls)
        }
    }

    /// An AccessKit `action` on `target`, as a screen reader sends one.
    fn act(target: egui::accesskit::NodeId, action: egui::accesskit::Action) -> egui::Event {
        egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
            action,
            target_tree: egui::accesskit::TreeId::ROOT,
            target_node: target,
            data: None,
        })
    }

    /// The control `controls` labels `label`.
    fn control(
        controls: &[(egui::accesskit::NodeId, String)],
        label: &str,
    ) -> egui::accesskit::NodeId {
        controls
            .iter()
            .find(|(_, l)| l == label)
            .map(|(id, _)| *id)
            .unwrap_or_else(|| panic!("the Body tab draws no {label:?}"))
    }

    /// The seed-11 beast on its own fitted collider, its run tuned half as
    /// fast again when `tuned`.
    fn fitted_beast(tuned: bool) -> AvatarRecord {
        use crate::pds::types::Fp;
        let mut beast = wearing(EngineAvatarRecord::rolled(
            "Beast",
            symbios_avatar::Archetype::Quadruped(symbios_avatar::QuadrupedParams::default()),
            11,
        ));
        let mut fitted = crate::pds::avatar::fit_on_foot(
            worn_body(&beast).expect("resolved"),
            &crate::pds::HumanoidParams::default(),
        );
        if tuned {
            fitted.walk_speed = Fp(fitted.walk_speed.0 * 1.5).on_wire();
        }
        beast.locomotion = fitted.into_config();
        beast
    }

    /// **A switch of kind rolls the new body whole from the seed, in both
    /// directions, and the creator's locks survive it** (#1803, owner
    /// decision D6). The viewer's toggle sets the default dog or the
    /// default person, so a lock held across the switch held the default's
    /// values: a held Stature stood every beast at the default dog's 0.58 m,
    /// a held Head gave it no head size and no muzzle, a held Colouring made
    /// it red - and a beast becoming a person kept the composites the beast
    /// rolled. Here every lock set - one category, and all of them - gives
    /// the body the seed rolls with nothing locked, and the lock set comes
    /// back whole, so the seed arrow after it keeps a locked stature.
    #[test]
    fn a_switch_of_kind_rolls_the_new_body_whole_and_keeps_the_locks() {
        use symbios_avatar::{Archetype, Category, LockSet, QuadrupedParams};
        let mut every = LockSet::NONE;
        for category in Category::ALL {
            every = every.with(category);
        }
        let lock_sets = [
            LockSet::NONE.with(Category::Stature),
            LockSet::NONE.with(Category::Head),
            LockSet::NONE.with(Category::Colouring),
            every,
        ];
        // The body `from` becomes as a switch to `kind` rolls it: every lock
        // lifted for the roll, the creator's put back.
        let rolled = |from: &EngineAvatarRecord, kind: Archetype| {
            let mut want = from.clone();
            want.archetype = kind;
            want.locks = LockSet::NONE;
            want.reroll(want.seed);
            want.sanitize();
            want.locks = from.locks;
            want
        };
        let dog = QuadrupedParams::default();
        for did in ["did:plc:lock-a", "did:plc:lock-b", "did:plc:lock-c"] {
            for locks in lock_sets {
                // A person becoming a beast.
                let mut person = crate::pds::avatar::wardrobe::engine_default_for_did(did);
                person.locks = locks;
                let mut record = wearing(person.clone());
                let driven = drive(&mut record, "quadruped");
                assert!(driven.changed, "{did} {locks:?}: no change");
                let body = worn_body(&record).expect("resolved").clone();
                assert_eq!(body.locks, locks, "{did}: the lock set did not survive");
                assert_eq!(
                    body,
                    rolled(&person, Archetype::Quadruped(dog)),
                    "{did} {locks:?}: not the beast the seed rolls"
                );
                let Archetype::Quadruped(q) = &body.archetype else {
                    panic!("{did}: not a beast");
                };
                assert!(
                    q.height != dog.height && q.head_size != dog.head_size && q.coat != dog.coat,
                    "{did} {locks:?}: a locked category kept the default dog's"
                );

                // A beast becoming a person.
                let mut beast = body;
                beast.seed ^= 0x5eed;
                beast.reroll(beast.seed);
                beast.sanitize();
                let mut record = wearing(beast.clone());
                let driven = drive(&mut record, "humanoid");
                assert!(driven.changed, "{did} {locks:?}: no change back");
                let body = worn_body(&record).expect("resolved").clone();
                assert_eq!(
                    body.locks, locks,
                    "{did}: the lock set did not survive back"
                );
                let want = rolled(&beast, Archetype::Humanoid(Default::default()));
                assert_eq!(body, want, "{did} {locks:?}: not the person the seed rolls");
                assert_ne!(
                    body.archetype,
                    Archetype::Humanoid(Default::default()),
                    "{did} {locks:?}: the default person"
                );
            }
        }
        // The seed arrows after a switch keep the locks over the rolled body.
        let mut person = crate::pds::avatar::wardrobe::engine_default_for_did("did:plc:lock-a");
        person.locks = LockSet::NONE.with(Category::Stature);
        let mut record = wearing(person);
        drive(&mut record, "quadruped");
        let switched = worn_body(&record).expect("resolved").clone();
        drive(&mut record, "▶");
        let after = worn_body(&record).expect("resolved");
        let height = |body: &EngineAvatarRecord| match &body.archetype {
            Archetype::Quadruped(q) => q.height,
            _ => panic!("not a beast"),
        };
        assert_ne!(after.seed, switched.seed, "the premise: the arrow rolled");
        assert_eq!(height(after), height(&switched), "the stature lock let go");
    }

    /// **A tuned run comes back exactly from a drag that goes out and
    /// back** (#1803): thirty steps of the "height" slider up and thirty
    /// down on the tuned seed-11 beast, one frame each as a drag lands,
    /// return the record to the bit - the body, the collider, and the run
    /// half as fast again as the body's. Refitted from the frame before, the
    /// run's ratio and its rounding onto the wire's grid compounded, and it
    /// came back 8.486 m/s as 8.4859, so the record read as unsaved after a
    /// drag that changed nothing.
    #[test]
    fn a_tuned_run_comes_back_from_a_drag_out_and_back() {
        use egui::accesskit::Action;
        let base = fitted_beast(true);
        let mut record = base.clone();
        let mut tab = Tab::new();
        let (_, controls) = tab.frame(&mut record, Vec::new());
        let height = control(&controls, "height");
        for _ in 0..30 {
            tab.frame(&mut record, vec![act(height, Action::Increment)]);
        }
        assert_ne!(
            on_foot(&record).walk_speed,
            on_foot(&base).walk_speed,
            "the premise: the drag moved the run"
        );
        for _ in 0..30 {
            tab.frame(&mut record, vec![act(height, Action::Decrement)]);
        }
        tab.frame(&mut record, Vec::new());
        assert_eq!(
            worn_body(&record),
            worn_body(&base),
            "the premise: the body is back"
        );
        assert_eq!(
            on_foot(&record),
            on_foot(&base),
            "the run did not come back"
        );
        assert_eq!(
            serde_json::to_string(&record).expect("writes"),
            serde_json::to_string(&base).expect("writes"),
        );
    }

    /// **The refit lands in the frame of the edit, on its change** (#1803,
    /// D3): ONE frame - the one whose input clicks the seed arrow - reports
    /// the change, and the record that frame leaves already stands the
    /// beast on its new body's collider, so the tick the editor stamps for
    /// that change carries both and one undo takes back both. The tests
    /// above draw four frames after the click, which a refit deferred to
    /// the next frame - and not reported as a change of its own - also
    /// passed.
    #[test]
    fn the_refit_lands_in_the_frame_of_the_edit() {
        use egui::accesskit::Action;
        let beast = fitted_beast(false);
        let mut record = beast.clone();
        let mut tab = Tab::new();
        let (idle, controls) = tab.frame(&mut record, Vec::new());
        assert!(
            !idle.changed && record == beast,
            "the premise: drawing is inert"
        );
        let (outcome, _) = tab.frame(
            &mut record,
            vec![act(control(&controls, "▶"), Action::Click)],
        );
        assert!(outcome.changed, "the click's frame reported no change");
        let body = worn_body(&record).expect("resolved");
        assert_ne!(body, worn_body(&beast).expect("resolved"), "the premise");
        let refit = crate::pds::avatar::fit_on_foot(body, on_foot(&beast));
        assert_ne!(
            &refit,
            on_foot(&beast),
            "the premise: the new body sizes differently"
        );
        assert_eq!(
            on_foot(&record),
            &refit,
            "the click's frame left the old collider"
        );
        let (steps, undone) = one_undo(&beast, &record);
        assert_eq!(steps, 2, "the baseline and one step");
        assert_eq!(undone, beast, "one undo is not the beast before");
        let (next, _) = tab.frame(&mut record, Vec::new());
        assert!(
            !next.changed,
            "the frame after reported a change of its own"
        );
    }

    /// `family`'s default winged body - the owl, the fruit bat, the sugar
    /// glider - worn and on its fitted preset, as the editor leaves one.
    fn fitted_winged(family: symbios_avatar::WingedFamily) -> AvatarRecord {
        crate::pds::avatar::locomotion::fitted_record(
            crate::pds::avatar::locomotion::default_winged(family),
        )
    }

    /// **A winged body's Body tab draws its own panels and drawing them
    /// changes nothing** (#1876): the owl, the bat and the glider show the
    /// viewer's body section and their surface - a bird's plumage, a bat's
    /// and a glider's coat - and none of a person's face, hair or outfit, and
    /// two passes with every section open leave the record as it was and
    /// report no change.
    #[test]
    fn a_winged_body_draws_its_panels_and_drawing_them_changes_nothing() {
        use symbios_avatar::WingedFamily;
        for family in [WingedFamily::Bird, WingedFamily::Bat, WingedFamily::Glider] {
            let mut record = fitted_winged(family);
            let before = record.clone();
            let (labels, changed) = hosted_sections(&mut record);
            let surface = if family == WingedFamily::Bird {
                "plumage"
            } else {
                "coat"
            };
            for section in ["body", surface] {
                assert!(
                    labels.iter().any(|l| l == section),
                    "{family:?}'s Body tab hosts the {section:?} section; drawn: {labels:?}"
                );
            }
            for section in ["face", "hair", "outfit"] {
                assert!(
                    !labels.iter().any(|l| l == section),
                    "{family:?}'s Body tab draws a person's {section:?}; drawn: {labels:?}"
                );
            }
            assert!(!changed, "drawing {family:?}'s Body tab reported a change");
            assert_eq!(record, before, "drawing {family:?}'s Body tab rewrote it");
        }
    }

    /// **A switch to a winged body and back is one undo step each way**
    /// (#1876, owner decision 4 on #1862): the viewer's `winged` toggle,
    /// clicked under a tuned person, rolls the winged body the person's seed
    /// gives with every lock lifted - so it may land on a bat or a glider -
    /// names the step "switch to a winged body", and stands it on the preset
    /// fitted to it, flight and all, in the same draw; one undo is the person
    /// back to the bit. The way back - `humanoid` under the owl - names a
    /// switch to a person, stands the person up without the owl's flight,
    /// and one undo is the owl back; and the owl made a beast is a switch to
    /// a four-legged body that flies no more.
    #[test]
    fn a_switch_to_a_winged_body_and_back_is_one_undo_each_way() {
        use symbios_avatar::{Archetype, WingedParams};
        let mut rolled = Vec::new();
        for did in ["did:plc:kind-switch", "did:plc:kind-switch-2"] {
            let person = tuned_person(did);
            let mut record = person.clone();
            let driven = drive(&mut record, "winged");
            assert!(driven.changed, "{did}: the switch reported no change");
            assert_eq!(driven.label.as_deref(), Some("switch to a winged body"));
            let body = worn_body(&record).expect("still resolved").clone();
            let mut want = worn_body(&person).expect("resolved").clone();
            want.archetype = Archetype::Winged(WingedParams::default());
            roll_switched(&mut want);
            want.sanitize();
            assert_eq!(
                body, want,
                "{did}: not the winged body the person's seed rolls"
            );
            assert_ne!(
                body.archetype,
                Archetype::Winged(WingedParams::default()),
                "{did}: the default owl, not a rolled body"
            );
            let fitted = crate::pds::avatar::fit_on_foot(&body, on_foot(&person));
            assert!(
                fitted.flies(),
                "{did}: the premise, a winged body that rigs"
            );
            assert_eq!(on_foot(&record), &fitted, "{did}: not on its own preset");
            let (steps, undone) = one_undo(&person, &record);
            assert_eq!(steps, 2, "the baseline and one step");
            assert_eq!(undone, person, "{did}: one undo is not the person back");
            rolled.push(body.archetype);
        }
        assert_ne!(rolled[0], rolled[1], "two seeds rolled one body");

        let owl = fitted_winged(symbios_avatar::WingedFamily::Bird);
        let mut record = owl.clone();
        let driven = drive(&mut record, "humanoid");
        assert_eq!(driven.label.as_deref(), Some("switch to a person"));
        let p = on_foot(&record);
        assert!(
            !p.flies() && !p.is_lying(),
            "the person kept the owl's flight"
        );
        let (steps, undone) = one_undo(&owl, &record);
        assert_eq!(steps, 2, "the baseline and one step");
        assert_eq!(undone, owl, "one undo is not the owl back");

        let mut record = owl.clone();
        let driven = drive(&mut record, "quadruped");
        assert_eq!(
            driven.label.as_deref(),
            Some("switch to a four-legged body")
        );
        let p = on_foot(&record);
        assert!(
            p.is_lying() && !p.flies(),
            "the beast kept the owl's flight"
        );
        let (_, undone) = one_undo(&owl, &record);
        assert_eq!(undone, owl, "one undo is not the owl back");
    }

    /// **A switch to or from a winged body rolls the new body whole from the
    /// seed, and the creator's locks survive it** (#1876, mirroring
    /// `a_switch_of_kind_rolls_the_new_body_whole_and_keeps_the_locks` for
    /// the third kind): the viewer's toggle sets the default owl, so a lock
    /// held across the switch would hold the owl's values - a held Stature
    /// every winged body at the owl's size, a held Colouring every one in
    /// the owl's plumage. Every lock set gives the body the seed rolls with
    /// nothing locked, both ways, and the lock set comes back whole, so the
    /// seed arrow after it keeps a locked size.
    #[test]
    fn a_switch_to_or_from_a_winged_body_rolls_it_whole_and_keeps_the_locks() {
        use symbios_avatar::{Archetype, Category, LockSet, WingedParams};
        let mut every = LockSet::NONE;
        for category in Category::ALL {
            every = every.with(category);
        }
        let lock_sets = [
            LockSet::NONE.with(Category::Stature),
            LockSet::NONE.with(Category::Head),
            LockSet::NONE.with(Category::Colouring),
            every,
        ];
        let rolled = |from: &EngineAvatarRecord, kind: Archetype| {
            let mut want = from.clone();
            want.archetype = kind;
            want.locks = LockSet::NONE;
            want.reroll(want.seed);
            want.sanitize();
            want.locks = from.locks;
            want
        };
        let owl = WingedParams::default();
        for did in ["did:plc:lock-a", "did:plc:lock-b", "did:plc:lock-c"] {
            for locks in lock_sets {
                // A person becoming a winged body.
                let mut person = crate::pds::avatar::wardrobe::engine_default_for_did(did);
                person.locks = locks;
                let mut record = wearing(person.clone());
                let driven = drive(&mut record, "winged");
                assert!(driven.changed, "{did} {locks:?}: no change");
                let body = worn_body(&record).expect("resolved").clone();
                assert_eq!(body.locks, locks, "{did}: the lock set did not survive");
                assert_eq!(
                    body,
                    rolled(&person, Archetype::Winged(owl)),
                    "{did} {locks:?}: not the winged body the seed rolls"
                );
                let Archetype::Winged(w) = &body.archetype else {
                    panic!("{did}: not winged");
                };
                assert!(
                    w.size != owl.size
                        && w.head_size != owl.head_size
                        && (w.plumage != owl.plumage || w.coat != owl.coat),
                    "{did} {locks:?}: a locked category kept the default owl's"
                );

                // A winged body becoming a person.
                let mut winged = body;
                winged.seed ^= 0x5eed;
                winged.reroll(winged.seed);
                winged.sanitize();
                let mut record = wearing(winged.clone());
                let driven = drive(&mut record, "humanoid");
                assert!(driven.changed, "{did} {locks:?}: no change back");
                let body = worn_body(&record).expect("resolved").clone();
                assert_eq!(
                    body.locks, locks,
                    "{did}: the lock set did not survive back"
                );
                let want = rolled(&winged, Archetype::Humanoid(Default::default()));
                assert_eq!(body, want, "{did} {locks:?}: not the person the seed rolls");
                assert_ne!(
                    body.archetype,
                    Archetype::Humanoid(Default::default()),
                    "{did} {locks:?}: the default person"
                );
            }
        }
        // The seed arrows after a switch keep the locks over the rolled body.
        let mut person = crate::pds::avatar::wardrobe::engine_default_for_did("did:plc:lock-a");
        person.locks = LockSet::NONE.with(Category::Stature);
        let mut record = wearing(person);
        drive(&mut record, "winged");
        let switched = worn_body(&record).expect("resolved").clone();
        drive(&mut record, "\u{25b6}");
        let after = worn_body(&record).expect("resolved");
        let size = |body: &EngineAvatarRecord| match &body.archetype {
            Archetype::Winged(w) => w.size,
            _ => panic!("not winged"),
        };
        assert_ne!(after.seed, switched.seed, "the premise: the arrow rolled");
        assert_eq!(size(after), size(&switched), "the stature lock let go");
    }

    /// The persons, beasts and winged bodies the engine rolls from `seeds`.
    fn rolled_bodies(seeds: std::ops::Range<i64>) -> Vec<(String, EngineAvatarRecord)> {
        use symbios_avatar::{Archetype, QuadrupedParams};
        let kinds = [
            ("person", Archetype::Humanoid(Default::default())),
            ("beast", Archetype::Quadruped(QuadrupedParams::default())),
            ("winged", Archetype::Winged(Default::default())),
        ];
        kinds
            .iter()
            .flat_map(|(kind, archetype)| {
                seeds.clone().map(move |seed| {
                    (
                        format!("{kind} {seed}"),
                        EngineAvatarRecord::rolled("Rolled", archetype.clone(), seed),
                    )
                })
            })
            .collect()
    }

    /// **An idle Body tab reports no change, whatever body it draws**
    /// (#1811). Seed 17's person - and persons 0, 7, 8, 14 and 21 of the
    /// twenty-four drawn here (eleven of the first forty) - wears
    /// a Fabric garment, and the Fabric panel's colour pickers, drawn open,
    /// report a change on every frame with nothing touched: egui's
    /// `color_edit_button_rgb` hands back its HSV round trip, the garment
    /// writes the surface it parses from that back, and the surface lands on
    /// the very value it was. The record stayed bit-identical while the tab
    /// said it changed, frame after frame - which re-arms the editor's
    /// debounce, names a "body edit" for the undo ring and, once the picker
    /// shuts, republishes and rebuilds a body nobody edited.
    ///
    /// `set_everything_is_visible` opens every section and every popup at
    /// once; a person reaches the same state one picker at a time (the
    /// click-driven test below). Four frames each, for persons, beasts and
    /// winged bodies off twenty-four seeds: no report and no write.
    #[test]
    fn an_idle_body_tab_reports_no_change_for_any_rolled_body() {
        let mut reported = Vec::new();
        for (name, body) in rolled_bodies(0..24) {
            let mut record = wearing(body);
            let before = record.clone();
            let mut tab = Tab::new();
            for frame in 0..4 {
                let (outcome, _) = tab.frame(&mut record, Vec::new());
                if outcome.changed {
                    reported.push(format!("{name} frame {frame}"));
                }
            }
            assert_eq!(record, before, "{name}: drawing the Body tab rewrote it");
        }
        assert!(
            reported.is_empty(),
            "an idle Body tab reported a change: {reported:?}"
        );
    }

    /// **A colour picker held open on seed 17's trousers reports nothing**
    /// (#1811), the way a person reaches it: the outfit, the trousers and
    /// every section of their Fabric panel opened by a click on its header,
    /// then each colour well drawn clicked open in turn and held for twenty
    /// frames with no other input. Before the fix four of them - the Fabric
    /// panel's warp colour and its corrosion, crevice-dirt and streak
    /// colours - reported a change on every one of those frames.
    #[test]
    fn a_colour_picker_held_open_on_seed_17s_trousers_reports_nothing() {
        use egui::accesskit::{Action, Role};
        let person = EngineAvatarRecord::rolled(
            "Rolled",
            symbios_avatar::Archetype::Humanoid(Default::default()),
            17,
        );
        assert!(
            person.outfit.trousers.texture.is_some(),
            "the premise: seed 17's person wears textured trousers"
        );
        let mut record = wearing(person);
        let before = record.clone();
        let mut tab = Tab::new();
        tab.ctx.memory_mut(|m| m.set_everything_is_visible(false));
        let (_, mut controls) = tab.frame(&mut record, Vec::new());
        for header in [
            "outfit",
            "trousers",
            "Fabric Config",
            "Weathering",
            "Edge Wear",
            "Corrosion",
            "Crevice Dirt",
            "Streaks",
        ] {
            let mut events = vec![act(control(&controls, header), Action::Click)];
            for _ in 0..12 {
                let (outcome, drawn) = tab.frame(&mut record, std::mem::take(&mut events));
                assert!(!outcome.changed, "opening {header:?} reported a change");
                controls = drawn;
            }
        }
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 6000.0),
            )),
            ..Default::default()
        };
        let output = tab.ctx.run_headless(input, |ui| {
            let _ = draw_body_tab(ui, &mut record, &mut tab.listing, None, false);
        });
        let wells: Vec<_> = output
            .platform_output
            .accesskit_update
            .expect("accesskit was enabled, so the pass has a tree")
            .nodes
            .iter()
            .filter(|(_, n)| n.role() == Role::ColorWell)
            .map(|(id, _)| *id)
            .collect();
        assert!(
            wells.len() >= 7,
            "the premise: the trousers' colour and the Fabric panel's six; {} wells",
            wells.len()
        );
        let mut reported = Vec::new();
        for (index, well) in wells.into_iter().enumerate() {
            let mut events = vec![act(well, Action::Click)];
            let mut frames = 0;
            for _ in 0..20 {
                let (outcome, _) = tab.frame(&mut record, std::mem::take(&mut events));
                frames += usize::from(outcome.changed);
            }
            if frames > 0 {
                reported.push(format!("well {index}: {frames} of 20 frames"));
            }
            let escape = egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            };
            let _ = tab.frame(&mut record, vec![escape]);
            for _ in 0..3 {
                let _ = tab.frame(&mut record, Vec::new());
            }
        }
        assert!(
            reported.is_empty(),
            "a colour picker held open reported a change: {reported:?}"
        );
        assert_eq!(record, before, "holding a picker open rewrote the record");
    }
}
