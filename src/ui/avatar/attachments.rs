//! The Attachments tab: dress the rigged body from the inventory (#1059).
//!
//! An attachment is an owned copy (epic #1054 decision): picking an
//! inventory item clones its `Generator` into a fresh attachment record at
//! a minted TID, so later edits or deletion of the stash item never mutate
//! an outfit already dressed. Detaching queues the record's rkey for
//! deletion — executed by the publish bundle *after* the avatar record
//! stops referencing it, and only cleared once that save lands.
//!
//! Offsets are edited numerically here: translation in the joint's
//! rest-pose frame, a yaw turn, and one uniform scale — the same three the
//! record's sanitiser guarantees (a non-uniform scale under an animated
//! joint shears every nested sub-assembly). Every drag quantises through
//! the record's own `Fp` grid on sanitize, so what is shown is what is
//! stored. The in-world drag gizmo on attachment roots is follow-up work,
//! filed on the epic.

use bevy_egui::egui;

use crate::pds::avatar::wardrobe::AttachmentRecord;
use crate::pds::avatar::{MAX_AVATAR_ATTACHMENTS, ResolvedAttachment};
use crate::pds::{AvatarRecord, InventoryRecord};

/// What the Attachments tab did this frame.
#[derive(Default)]
pub(super) struct AttachmentsTabOutcome {
    pub changed: bool,
    pub label: Option<String>,
}

/// Cross-frame picker state.
#[derive(Default)]
pub(super) struct AttachmentsTabState {
    /// The inventory item name picked in the attach row.
    pick_item: Option<String>,
    /// The socket picked in the attach row.
    pick_socket: Option<symbios_avatar::Socket>,
}

/// Draw the tab. `pending_deletes` is the session's queue of detached
/// record rkeys, owned by the publish flow.
pub(super) fn draw_attachments_tab(
    ui: &mut egui::Ui,
    record: &mut AvatarRecord,
    inventory: Option<&InventoryRecord>,
    state: &mut AttachmentsTabState,
    pending_deletes: &mut Vec<String>,
    did: Option<&str>,
) -> AttachmentsTabOutcome {
    let mut outcome = AttachmentsTabOutcome::default();

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
            {
                let Some(resolved) = rig.resolved.as_mut() else {
                    return;
                };
                for (index, attachment) in resolved.attachments.iter_mut().enumerate() {
                    let title = match attachment.record.socket() {
                        Some(socket) => format!("{} — {}", socket.name(), attachment.rkey),
                        None => format!(
                            "{} (unknown socket) — {}",
                            attachment.record.socket, attachment.rkey
                        ),
                    };
                    egui::CollapsingHeader::new(title)
                        .id_salt(("attachment", index))
                        .show(ui, |ui| {
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
                                if ui.button("Detach").clicked() {
                                    detach = Some(index);
                                }
                            });
                        });
                }
            }
            let worn_count = rig
                .resolved
                .as_ref()
                .map_or(0, |resolved| resolved.attachments.len());
            if let Some(index) = detach {
                detach_at(rig, index, pending_deletes);
                outcome.changed = true;
                outcome.label = Some(String::from("detach prop"));
            }
            if worn_count == 0 {
                ui.small("Nothing worn yet.");
            }

            // --- Attach row ---------------------------------------------
            ui.separator();
            let Some(did) = did else {
                ui.small("Log in to attach items — worn props live in your repo.");
                return;
            };
            let Some(inventory) = inventory else {
                ui.small("Open your inventory once to attach items from it.");
                return;
            };
            if worn_count >= MAX_AVATAR_ATTACHMENTS {
                ui.small(format!(
                    "Wearing {MAX_AVATAR_ATTACHMENTS} props — the fan-out cap; detach one to \
                     attach another."
                ));
                return;
            }
            let mut names: Vec<&String> = inventory.generators.keys().collect();
            names.sort();
            if names.is_empty() {
                ui.small("Your inventory is empty — save an item there first.");
                return;
            }
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("attach_item")
                    .selected_text(state.pick_item.as_deref().unwrap_or("item…"))
                    .show_ui(ui, |ui| {
                        for name in &names {
                            ui.selectable_value(&mut state.pick_item, Some((*name).clone()), *name);
                        }
                    });
                let socket_text = state
                    .pick_socket
                    .map_or("socket…", symbios_avatar::Socket::name);
                egui::ComboBox::from_id_salt("attach_socket")
                    .selected_text(socket_text)
                    .show_ui(ui, |ui| {
                        for socket in symbios_avatar::Socket::ALL {
                            ui.selectable_value(
                                &mut state.pick_socket,
                                Some(socket),
                                socket.name(),
                            );
                        }
                    });
                let ready = state.pick_item.is_some() && state.pick_socket.is_some();
                if ui.add_enabled(ready, egui::Button::new("Attach")).clicked()
                    && let (Some(name), Some(socket)) = (state.pick_item.clone(), state.pick_socket)
                    && let Some(item) = inventory.generators.get(&name)
                {
                    attach_to(rig, item.clone(), socket, did);
                    outcome.changed = true;
                    outcome.label = Some(format!("attach {name} at {}", socket.name()));
                }
            });
            ui.small(
                "An attachment is a copy: later edits to the inventory item leave \
                 this outfit alone. Identity offsets are engine-seated just outside \
                 the body.",
            );
        });

    outcome
}

/// Put `item` on the body at `socket`: an owned COPY into a fresh
/// attachment record at a minted TID, pushed onto both the resolved outfit
/// and the record's reference list — the two halves that must move
/// together, exactly as [`detach_at`] takes them off together.
fn attach_to(
    rig: &mut crate::pds::avatar::RiggedBody,
    item: crate::pds::Generator,
    socket: symbios_avatar::Socket,
    did: &str,
) {
    let Some(resolved) = rig.resolved.as_mut() else {
        return;
    };
    let mut worn = AttachmentRecord::new(item, socket);
    worn.sanitize();
    let rkey = crate::pds::tid::tid_now(crate::seeded_defaults::fnv1a_64(did));
    resolved.attachments.push(ResolvedAttachment {
        rkey: rkey.clone(),
        record: worn,
    });
    rig.attachments.push(rkey);
}

/// Take prop `index` off the body: drop it from the resolved outfit AND
/// from the record's reference list, and queue its record for deletion.
///
/// All three, or none — a reference left behind is a fetch every peer pays
/// for a prop nobody wears, and a queue entry left behind orphans a record
/// in the repo forever.
fn detach_at(
    rig: &mut crate::pds::avatar::RiggedBody,
    index: usize,
    pending_deletes: &mut Vec<String>,
) {
    let Some(resolved) = rig.resolved.as_mut() else {
        return;
    };
    if index >= resolved.attachments.len() {
        return;
    }
    let gone = resolved.attachments.remove(index);
    rig.attachments.retain(|rkey| rkey != &gone.rkey);
    pending_deletes.push(gone.rkey);
}

/// Translation / yaw / uniform-scale rows for one worn prop. Values land on
/// the wire's grid when the record sanitises on flush; the ranges are the
/// generous "keep it near the body" kind, not authorship limits.
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
    ui.horizontal(|ui| {
        // One yaw turn covers the common "face the prop outward" need; the
        // full free rotation waits for the attachment gizmo follow-up.
        let quat = bevy::math::Quat::from_array(record.offset.rotation.0);
        let (mut yaw_degrees, ..) = quat.to_euler(bevy::math::EulerRot::YXZ);
        yaw_degrees = yaw_degrees.to_degrees();
        ui.label("yaw");
        if ui
            .add(
                egui::DragValue::new(&mut yaw_degrees)
                    .speed(1.0)
                    .range(-180.0..=180.0)
                    .suffix("°"),
            )
            .changed()
        {
            record.offset.rotation.0 =
                bevy::math::Quat::from_rotation_y(yaw_degrees.to_radians()).to_array();
            changed = true;
        }
        ui.label("scale");
        let mut scale = record.offset.scale.0[0];
        if ui
            .add(
                egui::DragValue::new(&mut scale)
                    .speed(0.01)
                    .range(0.05..=10.0),
            )
            .changed()
        {
            record.offset.scale.0 = [scale, scale, scale];
            changed = true;
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

    #[test]
    fn detaching_drops_the_prop_the_reference_and_queues_the_delete() {
        // Three things move together or the outfit is wrong: peers would
        // keep fetching a reference for a prop nobody wears, or the record
        // would be orphaned in the repo forever.
        let mut rig = dressed(3);
        let mut deletes = Vec::new();
        detach_at(&mut rig, 1, &mut deletes);

        assert_eq!(deletes, vec![String::from("rkey1")]);
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
        attach_to(
            &mut rig,
            Generator::default(),
            symbios_avatar::Socket::LeftHand,
            "did:plc:attach-test",
        );
        let resolved = rig.resolved.as_ref().expect("resolved");
        assert_eq!(resolved.attachments.len(), 1);
        let worn = &resolved.attachments[0];
        assert_eq!(worn.rkey.len(), 13, "a freshly minted TID rkey");
        assert_eq!(rig.attachments, vec![worn.rkey.clone()]);
        assert_eq!(worn.record.socket(), Some(symbios_avatar::Socket::LeftHand));
    }

    #[test]
    fn detaching_a_prop_that_is_not_there_changes_nothing() {
        let mut rig = dressed(1);
        let mut deletes = Vec::new();
        detach_at(&mut rig, 7, &mut deletes);
        assert!(deletes.is_empty());
        assert_eq!(rig.attachments.len(), 1);
    }
}
