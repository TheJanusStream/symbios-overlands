//! The Attachments tab: dress the rigged body from the inventory (#1059).
//!
//! An attachment is an owned copy (epic #1054 decision): picking an
//! inventory item clones its `Generator` into a fresh attachment record at
//! a minted TID, so later edits or deletion of the stash item never mutate
//! an outfit already dressed. Detaching drops the reference and stops
//! there; the orphaned record is retired by the next save, whose delete set
//! is derived from the published record's reference list (#1110). There is
//! no session-held delete queue — an earlier design had one, and this
//! paragraph still described it long after it was gone.
//!
//! Offsets are edited numerically here as a **full transform** (#1095):
//! translation in the joint's rest-pose frame, yaw / pitch / roll, and
//! per-axis scale — the same rows a region placement gets, through the
//! same shared rotation widget. Every drag quantises through the record's
//! own `Fp` grid on sanitize, so what is shown is what is stored.
//!
//! These rows are the numeric twin of the in-world offset gizmo (#1062).
//! Selecting a row here aims the gizmo at that prop; an in-world pick
//! focuses the row back. Both write the same quaternion, and the rotation
//! row re-derives its angles from whatever the quaternion currently is, so
//! a gizmo tilt and a typed yaw compose instead of fighting.

use bevy_egui::egui;

use crate::pds::avatar::wardrobe::AttachmentRecord;
use crate::pds::avatar::{MAX_AVATAR_ATTACHMENTS, ResolvedAttachment};
use crate::pds::{AvatarRecord, InventoryRecord};
use crate::state::LiveInventoryRecord;

/// What the Attachments tab did this frame.
#[derive(Default)]
pub(super) struct AttachmentsTabOutcome {
    pub changed: bool,
    pub label: Option<String>,
    /// The worn prop whose parts editor the owner asked to open (#1098),
    /// by record key — applied by the caller, which owns that state.
    pub open_parts: Option<String>,
}

/// Cross-frame picker state.
#[derive(Default)]
pub(super) struct AttachmentsTabState {
    /// The inventory item name picked in the wear row.
    pick_item: Option<String>,
}

/// Draw the tab. `inventory` is mutable for the one write this tab makes to
/// it — **Save to inventory** on a worn prop (#1096); the guarded-dirty rule
/// holds because the stash's dirty state is derived live-vs-stored, never
/// from a change tick. Taking a prop off drops its reference and stops
/// there; the record it leaves behind is retired by the next save (#1110).
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_attachments_tab(
    ui: &mut egui::Ui,
    record: &mut AvatarRecord,
    inventory: Option<&mut LiveInventoryRecord>,
    state: &mut AttachmentsTabState,
    did: Option<&str>,
    selected: &mut Option<String>,
    focus_selected: bool,
    toasts: &mut crate::ui::toast::Toasts,
    now: f64,
) -> AttachmentsTabOutcome {
    let mut outcome = AttachmentsTabOutcome::default();
    let mut inventory = inventory;

    // A gizmo aim is only meaningful while some worn record still carries
    // that rkey (#1140). Every known path that takes a prop off already
    // calls `forget_attachments`, but a selection that outlives its prop
    // is the shape that freezes the chassis with nothing on screen to
    // release — so the tab that owns the selection re-checks it rather
    // than trusting every producer to have remembered.
    if let Some(rkey) = selected.clone() {
        let still_worn = record
            .body
            .rigged_ref()
            .and_then(|rig| rig.resolved.as_ref())
            .is_some_and(|resolved| resolved.attachments.iter().any(|a| a.rkey == rkey));
        if !still_worn {
            *selected = None;
        }
    }

    let Some(rig) = record.body.rigged_mut() else {
        ui.label("Attachments dress a rigged body — switch on the Body tab first.");
        return outcome;
    };
    if rig.resolved.is_none() {
        ui.label("This body's wardrobe record has not resolved; nothing to dress yet.");
        return outcome;
    }

    egui::ScrollArea::vertical()
        .id_salt("attachments_tab_scroll")
        .show(ui, |ui| {
            // --- Worn props ---------------------------------------------
            // The resolved borrow is scoped to this block: detaching below
            // needs `rig` itself, because the reference list lives there and
            // both halves have to move together.
            let mut detach: Option<usize> = None;
            let mut save_back: Option<usize> = None;
            let mut newly_selected: Option<Option<String>> = None;
            {
                let Some(resolved) = rig.resolved.as_mut() else {
                    return;
                };
                for (index, attachment) in resolved.attachments.iter_mut().enumerate() {
                    let is_selected = selected.as_deref() == Some(attachment.rkey.as_str());
                    // Named by its inventory provenance when it has one
                    // (#1096); a prop attached from a bare generator falls
                    // back to socket + record key.
                    let socket_label = match attachment.record.socket() {
                        Some(socket) => socket.name().to_string(),
                        None => format!("{} (unknown socket)", attachment.record.socket),
                    };
                    let title = match attachment.record.source.as_deref() {
                        Some(source) => format!("{source} — {socket_label}"),
                        None => format!("{socket_label} — {}", attachment.rkey),
                    };
                    // An in-world pick (#1062) opens the row it landed on and
                    // scrolls it into view; every other frame the header keeps
                    // whatever openness the owner left it at.
                    let force_open = (focus_selected && is_selected).then_some(true);
                    let header = egui::CollapsingHeader::new(title)
                        .id_salt(("attachment", index))
                        .open(force_open)
                        .show(ui, |ui| {
                            // Gizmo aim (#1062): the row IS the target, the
                            // same contract a visuals tree row has. Clicking
                            // the armed row again drops the gizmo.
                            if ui
                                .selectable_label(is_selected, "⌖ Drag in world")
                                .on_hover_text(
                                    "aim the in-world gizmo at this prop — the body holds \
                                     its bind pose while the gizmo is up, because the \
                                     offset is stored in the joint's rest frame",
                                )
                                .clicked()
                            {
                                newly_selected =
                                    Some((!is_selected).then(|| attachment.rkey.clone()));
                            }
                            // Socket picker: re-seating a prop keeps its offset —
                            // usually wrong for the new socket, but predictable;
                            // zeroing it re-seats via the engine on next spawn.
                            ui.horizontal_wrapped(|ui| {
                                ui.label("socket");
                                for socket in symbios_avatar::Socket::ALL {
                                    let picked = attachment.record.socket == socket.name();
                                    if ui.selectable_label(picked, socket.name()).clicked()
                                        && !picked
                                    {
                                        attachment.record.socket = socket.name().to_string();
                                        outcome.changed = true;
                                        outcome.label =
                                            Some(format!("move prop to {}", socket.name()));
                                    }
                                }
                            });
                            outcome.changed |= offset_rows(ui, &mut attachment.record);
                            // Fit is item metadata (#1089), shown so the
                            // schema has a face in the editor — never edited
                            // here: the declaration belongs to the catalogue
                            // entry, and the computed scale to the body.
                            if attachment.record.fit_band_mm != 0 {
                                ui.small(format!(
                                    "fitted: {} mm band, scaled to your head while the \
                                     offset stays engine-seated",
                                    attachment.record.fit_band_mm
                                ));
                            }
                            ui.horizontal(|ui| {
                                if ui
                                    .button("Re-seat")
                                    .on_hover_text(
                                        "zero the offset — the engine seats the prop just \
                                     outside the body at its socket",
                                    )
                                    .clicked()
                                {
                                    attachment.record.offset = crate::pds::TransformData::default();
                                    outcome.changed = true;
                                }
                                if ui
                                    .button("Edit parts")
                                    .on_hover_text(
                                        "open this item's part tree — the region-asset editor, \
                                         on your worn copy",
                                    )
                                    .clicked()
                                {
                                    outcome.open_parts = Some(attachment.rkey.clone());
                                }
                                if ui.button("Take off").clicked() {
                                    detach = Some(index);
                                }
                                // Save-back (#1096): the worn item — geometry,
                                // socket, fit and offset — written to the
                                // stash under its source name, or as a new
                                // item when it has none.
                                let save_label = match attachment.record.source.as_deref() {
                                    Some(source) => format!("Save to inventory as \"{source}\""),
                                    None => String::from("Save to inventory"),
                                };
                                if ui
                                    .add_enabled(inventory.is_some(), egui::Button::new(save_label))
                                    .on_hover_text(
                                        "write this worn item, with its socket and offset, \
                                         back to your inventory so wearing it again looks \
                                         exactly like this",
                                    )
                                    .on_disabled_hover_text(
                                        "Open your inventory once to save into it.",
                                    )
                                    .clicked()
                                {
                                    save_back = Some(index);
                                }
                            });
                        });
                    if force_open.is_some() {
                        header
                            .header_response
                            .scroll_to_me(Some(egui::Align::Center));
                    }
                }
            }
            if let Some(pick) = newly_selected {
                *selected = pick;
            }
            let worn_count = rig
                .resolved
                .as_ref()
                .map_or(0, |resolved| resolved.attachments.len());
            if let Some(index) = save_back
                && let Some(inv) = inventory.as_deref_mut()
                && let Some(worn) = rig
                    .resolved
                    .as_ref()
                    .and_then(|resolved| resolved.attachments.get(index))
            {
                match save_worn_to_inventory(&worn.record, &mut inv.0) {
                    Ok(name) => toasts.success(
                        format!("Saved as \"{name}\" — wear it again from your inventory."),
                        now,
                    ),
                    Err(reason) => toasts.warn(reason, now),
                }
            }
            if let Some(index) = detach {
                let gone = rig
                    .resolved
                    .as_ref()
                    .and_then(|r| r.attachments.get(index))
                    .map(|a| a.rkey.clone());
                detach_at(rig, index);
                if gone.is_some() && *selected == gone {
                    *selected = None;
                }
                outcome.changed = true;
                outcome.label = Some(String::from("take off prop"));
            }
            if worn_count == 0 {
                ui.small("Nothing worn yet.");
            }

            // --- Wear row -------------------------------------------------
            // The inventory is the wear surface (#1096): only items that
            // carry wear metadata are offered, at the socket and offset they
            // were last saved with. Anything else in the stash is decor.
            ui.separator();
            let Some(did) = did else {
                ui.small("Log in to wear items — worn props live in your repo.");
                return;
            };
            let Some(inventory) = inventory.as_deref() else {
                ui.small("Open your inventory once to wear items from it.");
                return;
            };
            if worn_count >= MAX_AVATAR_ATTACHMENTS {
                ui.small(format!(
                    "Wearing {MAX_AVATAR_ATTACHMENTS} props — the fan-out cap; take one off to \
                     wear another."
                ));
                return;
            }
            let mut names: Vec<&String> = inventory.0.wear.keys().collect();
            names.sort_by_key(|name| name.to_lowercase());
            if names.is_empty() {
                ui.small(
                    "Nothing wearable in your inventory — copy a wearable from the Catalogue \
                     first.",
                );
                return;
            }
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("wear_item")
                    .selected_text(state.pick_item.as_deref().unwrap_or("wearable…"))
                    .show_ui(ui, |ui| {
                        for name in &names {
                            let socket = inventory
                                .0
                                .wear
                                .get(*name)
                                .map_or("?", |meta| meta.socket.as_str());
                            ui.selectable_value(
                                &mut state.pick_item,
                                Some((*name).clone()),
                                format!("{name} ({socket})"),
                            );
                        }
                    });
                let ready = state.pick_item.is_some();
                if ui.add_enabled(ready, egui::Button::new("Wear")).clicked()
                    && let Some(name) = state.pick_item.clone()
                    && let Some(record) = record_for_inventory_item(&inventory.0, &name)
                {
                    attach_record(rig, record, did);
                    outcome.changed = true;
                    outcome.label = Some(format!("wear {name}"));
                }
            });
            ui.small(
                "A worn item is a copy of the inventory item: edit it here, then Save to \
                 inventory to keep the changes.",
            );
        });

    outcome
}

/// The attachment record wearing the named inventory item (#1096): its
/// generator, its wear metadata, and its name as provenance. `None` when
/// the item is missing or is not wearable.
pub(crate) fn record_for_inventory_item(
    inventory: &InventoryRecord,
    name: &str,
) -> Option<AttachmentRecord> {
    let generator = inventory.generators.get(name)?;
    let meta = inventory.wear.get(name)?;
    Some(AttachmentRecord::from_inventory(
        name,
        generator.clone(),
        meta,
    ))
}

/// Why the live avatar cannot take another attachment right now, phrased
/// for the person about to click — or `None` when it can.
///
/// One function rather than a ladder per surface. Three surfaces offer to
/// wear something (the Inventory row, the catalogue's "Copy to inventory
/// & wear", the scene context menu); all three had their own copy of the
/// reasons, all three agreed on three of them, and all three were missing
/// the same fourth (#1141).
///
/// That fourth is a **rigged body whose wardrobe record did not resolve**
/// — the state a failed wardrobe fetch or a deleted wardrobe record leaves
/// behind, and one the Body tab already describes and offers a way out of.
/// It reports zero worn attachments, so the cap check passed and the
/// button enabled; [`attach_record`] then returned `None` and the click
/// did nothing at all. The catalogue went on to toast that the item was
/// being worn. A surface that cannot succeed has to say so before the
/// click, not stay silent after it.
///
/// Ordered so the most fundamental obstacle wins: an unresolved rig has no
/// attachment list to be full, so asking about the cap first would name
/// the wrong reason.
pub(crate) fn wear_blocked_reason(avatar: Option<&AvatarRecord>) -> Option<String> {
    let Some(avatar) = avatar else {
        return Some(String::from("No avatar loaded yet."));
    };
    let Some(rig) = avatar.body.rigged_ref() else {
        return Some(String::from(
            "Vehicles carry no attachments — pilot a body to wear this.",
        ));
    };
    let Some(resolved) = rig.resolved.as_ref() else {
        return Some(String::from(
            "This body's wardrobe record could not be resolved — the Avatar \
             window's Body tab can wear a fresh one.",
        ));
    };
    if resolved.attachments.len() >= MAX_AVATAR_ATTACHMENTS {
        return Some(format!(
            "All {MAX_AVATAR_ATTACHMENTS} attachment slots are taken — take \
             something off first."
        ));
    }
    None
}

/// Put a built attachment record on the body: sanitised, at a minted TID,
/// pushed onto both the resolved outfit and the record's reference list —
/// the two halves that must move together, exactly as [`detach_at`] takes
/// them off together. Returns the minted rkey, or `None` for a body that
/// cannot take it: no resolved rig to dress, or already at
/// [`MAX_AVATAR_ATTACHMENTS`].
///
/// The cap is checked *here*, not only at the three surfaces that offer a
/// wear, so this function's precondition is exactly the one
/// [`wear_blocked_reason`] renders — a property the tests assert in both
/// directions (#1141). Without it a caller that forgot the cap would push
/// a seventeenth prop and `RiggedBody::sanitize` would truncate it back
/// out on the way to the wire: the same silently-ineffective click this
/// issue is about, one layer down.
pub(crate) fn attach_record(
    rig: &mut crate::pds::avatar::RiggedBody,
    mut record: AttachmentRecord,
    did: &str,
) -> Option<String> {
    let resolved = rig.resolved.as_mut()?;
    if resolved.attachments.len() >= MAX_AVATAR_ATTACHMENTS {
        return None;
    }
    record.sanitize();
    let rkey = crate::pds::tid::tid_now(crate::seeded_defaults::fnv1a_64(did));
    resolved.attachments.push(ResolvedAttachment {
        rkey: rkey.clone(),
        record,
    });
    rig.attachments.push(rkey.clone());
    Some(rkey)
}

/// Re-point the provenance of every worn prop that came from `old` at
/// `new`. Returns how many moved.
///
/// The inventory is the wear surface (#1096) and the *only* link it has
/// to a worn prop is this string: the row's worn state is
/// [`is_worn_from`], its Take off is [`take_off_source`], and
/// [`save_worn_to_inventory`] writes back under it. Renaming the stash
/// item moved the generator and its wear metadata to the new key and left
/// every worn record pointing at a name that no longer existed (#1141) —
/// so the renamed row offered Wear on a prop already on the body, the
/// original could no longer be taken off from the Inventory window at
/// all, and Save to inventory minted a second item under the old name.
///
/// Deliberately not done for *deletion*, which strands the same string:
/// there the old name is free, so a later Save to inventory recreating it
/// restores the item rather than duplicating it. Rename is the case where
/// the name is still taken and the write lands beside the thing it came
/// from.
pub(crate) fn rename_worn_source(
    rig: &mut crate::pds::avatar::RiggedBody,
    old: &str,
    new: &str,
) -> usize {
    let Some(resolved) = rig.resolved.as_mut() else {
        return 0;
    };
    let mut moved = 0;
    for worn in &mut resolved.attachments {
        if worn.record.source.as_deref() == Some(old) {
            worn.record.source = Some(new.to_string());
            moved += 1;
        }
    }
    moved
}

/// Take off every worn prop that came from the named inventory item
/// (#1096) — the Inventory window's Take off. Returns how many came off;
/// their records are retired by the next save, which derives the delete set
/// from the reference list this drops them from (#1110).
pub(crate) fn take_off_source(rig: &mut crate::pds::avatar::RiggedBody, source: &str) -> usize {
    let mut taken = 0;
    loop {
        let Some(index) = rig.resolved.as_ref().and_then(|resolved| {
            resolved
                .attachments
                .iter()
                .position(|worn| worn.record.source.as_deref() == Some(source))
        }) else {
            return taken;
        };
        detach_at(rig, index);
        taken += 1;
    }
}

/// Take off the worn prop with this record key (#1097) — the scene menu's
/// Take off. `false` when nothing worn has that key.
pub(crate) fn take_off_rkey(rig: &mut crate::pds::avatar::RiggedBody, rkey: &str) -> bool {
    let Some(index) = rig.resolved.as_ref().and_then(|resolved| {
        resolved
            .attachments
            .iter()
            .position(|worn| worn.rkey == rkey)
    }) else {
        return false;
    };
    detach_at(rig, index);
    true
}

/// The record keys of every worn prop that came from the named inventory
/// item — what [`take_off_source`] is about to drop, read before it does so
/// the caller can retire a gizmo aimed at one of them.
pub(crate) fn worn_rkeys_from(rig: &crate::pds::avatar::RiggedBody, source: &str) -> Vec<String> {
    rig.resolved
        .as_ref()
        .map(|resolved| {
            resolved
                .attachments
                .iter()
                .filter(|worn| worn.record.source.as_deref() == Some(source))
                .map(|worn| worn.rkey.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// Whether any worn prop came from the named inventory item.
pub(crate) fn is_worn_from(rig: &crate::pds::avatar::RiggedBody, source: &str) -> bool {
    rig.resolved.as_ref().is_some_and(|resolved| {
        resolved
            .attachments
            .iter()
            .any(|worn| worn.record.source.as_deref() == Some(source))
    })
}

/// Write a worn prop back to the inventory (#1096): its generator and its
/// wear metadata (socket, fit, offset) under its source name — replacing
/// that item — or, for a prop with no provenance, as a new item named
/// after its socket. The record's `source` is left as it was: the caller
/// that wants the fresh name to become provenance reads it from the
/// return. Refuses only when a NEW item would breach the stash cap.
pub(crate) fn save_worn_to_inventory(
    record: &AttachmentRecord,
    inventory: &mut InventoryRecord,
) -> Result<String, String> {
    let name = match record.source.as_deref() {
        Some(source) => source.to_string(),
        None => crate::ui::room::widgets::unique_key(&inventory.generators, &record.socket),
    };
    let cap = crate::config::state::MAX_INVENTORY_ITEMS;
    if !inventory.generators.contains_key(&name) && inventory.generators.len() >= cap {
        return Err(format!("Inventory full ({cap}/{cap}) — item not saved."));
    }
    inventory.put_item(name.clone(), record.item.clone(), Some(record.wear_meta()));
    Ok(name)
}

/// Take prop `index` off the body: drop it from the resolved outfit AND
/// from the record's reference list.
///
/// Both, or neither — a reference left behind is a fetch every peer pays for
/// a prop nobody wears. Retiring the *record* is not this function's job and
/// never was a session queue's (#1110): the next save derives what to delete
/// from the published record's reference list, so dropping the reference here
/// is the whole of taking something off.
fn detach_at(rig: &mut crate::pds::avatar::RiggedBody, index: usize) {
    let Some(resolved) = rig.resolved.as_mut() else {
        return;
    };
    if index >= resolved.attachments.len() {
        return;
    }
    let gone = resolved.attachments.remove(index);
    rig.attachments.retain(|rkey| rkey != &gone.rkey);
}

/// The full transform rows for one worn prop (#1095): translation, yaw /
/// pitch / roll, per-axis scale — a region placement's editor, tuned for
/// body scale (centimetre drag steps, a few metres of range). Values land
/// on the wire's grid when the record sanitises on flush; the ranges are
/// the generous "keep it near the body" kind, not authorship limits.
fn offset_rows(ui: &mut egui::Ui, record: &mut AttachmentRecord) -> bool {
    let mut changed = false;
    let translation = &mut record.offset.translation.0;
    ui.horizontal(|ui| {
        ui.label("offset");
        for (axis, value) in ["x", "y", "z"].into_iter().zip(translation.iter_mut()) {
            changed |= ui
                .add(
                    egui::DragValue::new(value)
                        .speed(0.005)
                        .range(-3.0..=3.0)
                        .prefix(format!("{axis} "))
                        .suffix(" m"),
                )
                .changed();
        }
    });
    // The shared rotation row (`ui::room::widgets`): decomposes the stored
    // quaternion into degrees and recomposes on edit, so a tilt the gizmo
    // put there survives a typed yaw.
    crate::ui::room::widgets::euler_rotation_row(
        ui,
        "rotation",
        &mut record.offset.rotation,
        &mut changed,
    );
    ui.horizontal(|ui| {
        ui.label("scale");
        for (axis, value) in ["x", "y", "z"]
            .into_iter()
            .zip(record.offset.scale.0.iter_mut())
        {
            changed |= ui
                .add(
                    egui::DragValue::new(value)
                        .speed(0.01)
                        .range(0.05..=10.0)
                        .prefix(format!("{axis} ")),
                )
                .changed();
        }
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::Generator;
    use crate::pds::avatar::{ResolvedRig, RiggedBody};

    fn dressed(count: usize) -> RiggedBody {
        let worn: Vec<ResolvedAttachment> = (0..count)
            .map(|i| ResolvedAttachment {
                rkey: format!("rkey{i}"),
                record: AttachmentRecord::new(Generator::default(), symbios_avatar::Socket::Crown),
            })
            .collect();
        RiggedBody {
            avatar: String::from("3jzfcijpj2z2a"),
            attachments: worn.iter().map(|w| w.rkey.clone()).collect(),
            resolved: Some(ResolvedRig {
                body: crate::pds::avatar::wardrobe::engine_default_for_did("did:plc:detach"),
                attachments: worn,
            }),
        }
    }

    /// A rigged body whose wardrobe record never resolved — what a failed
    /// wardrobe fetch, or a deleted wardrobe record, leaves behind. The
    /// Body tab has always described this state and offered a way out;
    /// the wear surfaces did not.
    fn unresolved() -> crate::pds::AvatarRecord {
        let mut record = crate::pds::AvatarRecord::default_for_did("did:plc:tester");
        record.body = crate::pds::avatar::AvatarBody::Rigged(Box::new(RiggedBody {
            avatar: String::from("3jzfcijpj2z2a"),
            attachments: Vec::new(),
            resolved: None,
        }));
        record
    }

    /// A generator chassis — a vehicle. Carries no sockets, so it can
    /// never take an attachment.
    fn vehicle() -> crate::pds::AvatarRecord {
        let mut record = crate::pds::AvatarRecord::default_for_did("did:plc:tester");
        record.body = crate::pds::avatar::AvatarBody::generator(Generator::default());
        record
    }

    fn wearing(count: usize) -> crate::pds::AvatarRecord {
        let mut record = crate::pds::AvatarRecord::default_for_did("did:plc:tester");
        record.body = crate::pds::avatar::AvatarBody::Rigged(Box::new(dressed(count)));
        record
    }

    /// **The reason shown is exactly the condition the wear would fail
    /// on** (#1141).
    ///
    /// The defect was a gap between the two: every wear surface computed
    /// its cap from `resolved.map_or(0, …)`, so an unresolved rig reported
    /// zero worn attachments, passed the cap check, and enabled its
    /// button — and then [`attach_record`] returned `None` and nothing
    /// happened. Asserting the two agree, in both directions and across
    /// every body state, is what closes the gap rather than patching the
    /// one surface that also lied about it.
    #[test]
    fn a_wear_is_offered_exactly_when_it_can_land() {
        let cases: Vec<(&str, Option<crate::pds::AvatarRecord>)> = vec![
            ("no avatar at all", None),
            ("a generator chassis", Some(vehicle())),
            ("a rigged body that never resolved", Some(unresolved())),
            ("a resolved body with room", Some(wearing(0))),
            (
                "a resolved body one short of the cap",
                Some(wearing(MAX_AVATAR_ATTACHMENTS - 1)),
            ),
            (
                "a resolved body at the cap",
                Some(wearing(MAX_AVATAR_ATTACHMENTS)),
            ),
        ];
        for (what, record) in cases {
            let offered = wear_blocked_reason(record.as_ref()).is_none();
            let landed = record.clone().is_some_and(|mut record| {
                record.body.rigged_mut().is_some_and(|rig| {
                    attach_record(
                        rig,
                        AttachmentRecord::new(Generator::default(), symbios_avatar::Socket::Crown),
                        "did:plc:tester",
                    )
                    .is_some()
                })
            });
            assert_eq!(
                offered, landed,
                "{what}: the surface offers the wear = {offered}, but it lands = {landed}"
            );
        }
    }

    /// **The unresolved body's reason points at the tab that can fix it**
    /// (#1141).
    ///
    /// The words matter, not just the refusal: this is the one blocked
    /// state the user can do something about, and the Body tab is where.
    #[test]
    fn the_unresolved_body_is_refused_in_words_that_name_the_way_out() {
        let reason = wear_blocked_reason(Some(&unresolved())).expect("refused");
        assert!(
            reason.contains("could not be resolved"),
            "names the state: {reason}"
        );
        assert!(reason.contains("Body tab"), "names the way out: {reason}");
        // The control: the same body, resolved, is not refused at all.
        assert!(wear_blocked_reason(Some(&wearing(0))).is_none());
    }

    /// **A rename carries the worn props' provenance with it** (#1141).
    ///
    /// Sequence from the finding: wear "circlet", rename the stash item
    /// to "gold circlet". The inventory is the wear surface (#1096) and
    /// its only link to a worn prop is that string, so before this the
    /// renamed row offered Wear on something already on the body — a
    /// second copy one click away — and the original could no longer be
    /// taken off from the Inventory window at all.
    #[test]
    fn renaming_an_item_moves_the_provenance_of_what_it_put_on() {
        let mut rig = dressed(2);
        let resolved = rig.resolved.as_mut().expect("resolved");
        resolved.attachments[0].record.source = Some(String::from("circlet"));
        resolved.attachments[1].record.source = Some(String::from("pauldron"));

        assert!(is_worn_from(&rig, "circlet"));
        assert_eq!(rename_worn_source(&mut rig, "circlet", "gold circlet"), 1);

        assert!(
            !is_worn_from(&rig, "circlet"),
            "the old name must stop matching, or Take off aims at nothing"
        );
        assert!(
            is_worn_from(&rig, "gold circlet"),
            "the renamed row must show the prop as worn, not offer Wear again"
        );
        assert!(
            is_worn_from(&rig, "pauldron"),
            "an unrelated prop's provenance is untouched"
        );
        assert_eq!(
            rename_worn_source(&mut rig, "circlet", "gold circlet"),
            0,
            "renaming a name nothing wears moves nothing"
        );
    }

    #[test]
    fn detaching_drops_the_prop_from_both_halves() {
        // Both move together or the outfit is wrong: a reference left
        // behind is a fetch every peer pays for a prop nobody wears.
        // Retiring the RECORD is the next save's job, derived from this
        // shortened reference list (#1110) — not a queue kept here.
        let mut rig = dressed(3);
        detach_at(&mut rig, 1);

        assert_eq!(rig.attachments, vec!["rkey0".to_string(), "rkey2".into()]);
        let resolved = rig.resolved.as_ref().expect("still resolved");
        assert_eq!(resolved.attachments.len(), 2);
        assert!(resolved.attachments.iter().all(|w| w.rkey != "rkey1"));
    }

    #[test]
    fn attaching_copies_the_item_and_references_it_from_both_halves() {
        // A copy, not a reference (epic #1054): editing or deleting the
        // stash item afterwards must not reach into a dressed outfit.
        let mut rig = dressed(0);
        let rkey = attach_record(
            &mut rig,
            AttachmentRecord::new(Generator::default(), symbios_avatar::Socket::LeftHand),
            "did:plc:attach-test",
        )
        .expect("a resolved rig dresses");
        let resolved = rig.resolved.as_ref().expect("resolved");
        assert_eq!(resolved.attachments.len(), 1);
        let worn = &resolved.attachments[0];
        assert_eq!(worn.rkey, rkey);
        assert_eq!(worn.rkey.len(), 13, "a freshly minted TID rkey");
        assert_eq!(rig.attachments, vec![worn.rkey.clone()]);
        assert_eq!(worn.record.socket(), Some(symbios_avatar::Socket::LeftHand));
    }

    /// The inventory round trip (#1096): wear a stash item, take it off
    /// by name, save a worn prop back — provenance drives all three.
    #[test]
    fn wearing_from_the_inventory_round_trips_through_provenance() {
        use crate::pds::inventory::WearMeta;
        let mut inventory = InventoryRecord::default();
        inventory.put_item(
            String::from("Gilded Circlet"),
            Generator::default(),
            Some(WearMeta::for_entry(
                symbios_avatar::Socket::Crown,
                Some(crate::catalogue::WearFit::HeadBand {
                    inner_diameter: 0.178,
                }),
            )),
        );
        inventory.put_item(String::from("plain box"), Generator::default(), None);
        assert!(
            record_for_inventory_item(&inventory, "plain box").is_none(),
            "decor is not wearable"
        );
        let record = record_for_inventory_item(&inventory, "Gilded Circlet").expect("wearable");
        assert_eq!(record.socket(), Some(symbios_avatar::Socket::Crown));
        assert_eq!(record.fit_band_mm, 178);
        assert_eq!(record.source.as_deref(), Some("Gilded Circlet"));

        let mut rig = dressed(0);
        attach_record(&mut rig, record, "did:plc:wear-test");
        assert!(is_worn_from(&rig, "Gilded Circlet"));
        assert!(!is_worn_from(&rig, "plain box"));

        // Edit while worn, save back: the stash item now carries the offset.
        let worn = &mut rig.resolved.as_mut().expect("resolved").attachments[0];
        worn.record.offset.translation = crate::pds::types::Fp3([0.0, 0.03, 0.0]);
        let saved = save_worn_to_inventory(&worn.record, &mut inventory).expect("saves");
        assert_eq!(saved, "Gilded Circlet");
        assert_eq!(
            inventory.wear["Gilded Circlet"].offset.translation.0,
            [0.0, 0.03, 0.0]
        );

        // A prop with no provenance saves as a NEW item named by socket.
        let orphan = AttachmentRecord::new(Generator::default(), symbios_avatar::Socket::Back);
        let name = save_worn_to_inventory(&orphan, &mut inventory).expect("saves");
        assert_eq!(name, "back");
        assert!(inventory.is_wearable("back"));

        // Take off by name: both halves move. `worn_rkeys_from` names what
        // is about to come off, which is what the caller retires a gizmo on.
        assert_eq!(worn_rkeys_from(&rig, "Gilded Circlet").len(), 1);
        assert_eq!(take_off_source(&mut rig, "Gilded Circlet"), 1);
        assert!(worn_rkeys_from(&rig, "Gilded Circlet").is_empty());
        assert!(rig.attachments.is_empty());
        assert!(!is_worn_from(&rig, "Gilded Circlet"));

        // And by record key (the scene menu's path).
        let rkey = attach_record(
            &mut rig,
            AttachmentRecord::new(Generator::default(), symbios_avatar::Socket::Back),
            "did:plc:wear-test",
        )
        .expect("dresses");
        assert!(!take_off_rkey(&mut rig, "not-a-key"));
        assert!(take_off_rkey(&mut rig, &rkey));
        assert!(rig.attachments.is_empty());
    }

    #[test]
    fn detaching_a_prop_that_is_not_there_changes_nothing() {
        let mut rig = dressed(1);
        detach_at(&mut rig, 7);
        assert_eq!(rig.attachments.len(), 1);
    }
}
