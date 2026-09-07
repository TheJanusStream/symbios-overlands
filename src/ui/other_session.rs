//! The owner's other session (#1203): what happens when a `RoomStateUpdate`
//! arrives from a peer signed in as the SAME DID that owns this room.
//!
//! `is_owner` in `network::inbound` compares the sender's DID to the room's,
//! so a second tab or machine of the owner passes the gate like any owner
//! broadcast — and the arm used to replace `LiveRoomRecord` wholesale,
//! half an hour of unsaved edits included, while raising the foreign
//! observation that resets the undo ring. Nothing said so. The incoming
//! record then read as the dirty state, so the next Ctrl+S published the
//! clobbered copy.
//!
//! Three outcomes now, decided by [`classify_same_owner_update`]:
//!
//! - **Ignore** — the incoming record equals what this session already
//!   holds. This is the echo: both sessions rebroadcast on `is_changed`,
//!   so applying a record identical to ours would only reset the ring and
//!   send the same bytes straight back.
//! - **Apply** — this session is clean. The other session's copy is
//!   installed as before, with a toast saying where it came from.
//! - **Hold** — this session has unpublished edits. The incoming record is
//!   parked in [`OtherSessionRoom`] and [`other_session_room_ui`] asks
//!   which copy to keep; the live record is not touched until the owner
//!   answers. A newer update from the same session replaces the parked
//!   one (latest wins), so the choice is always between "mine" and
//!   "theirs, as of now".

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::pds::RoomRecord;
use crate::state::LiveRoomRecord;
use crate::state::RoomWriteSignals;

/// A room record from the owner's other session, held back because this
/// session has unpublished edits. Present only while the question is open;
/// session-scoped (torn down at logout, dropped on portal travel).
#[derive(Resource, Debug)]
pub struct OtherSessionRoom {
    pub record: RoomRecord,
}

/// What the inbound arm does with a same-owner `RoomStateUpdate`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SameOwnerUpdate {
    /// Identical to the local live record — the echo. Touch nothing.
    Ignore,
    /// Local is clean: install it, and say so.
    Apply,
    /// Local has unpublished edits: park it and ask.
    Hold,
}

/// The decision, pure so it can be tested without a socket.
pub fn classify_same_owner_update(
    local_dirty: bool,
    incoming_equals_live: bool,
) -> SameOwnerUpdate {
    if incoming_equals_live {
        SameOwnerUpdate::Ignore
    } else if local_dirty {
        SameOwnerUpdate::Hold
    } else {
        SameOwnerUpdate::Apply
    }
}

/// The owner's answer to the held record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    /// Drop the other session's copy; this session's edits stand.
    KeepMine,
    /// Replace the live record with the other session's copy.
    TakeTheirs,
}

/// Apply the owner's choice. Taking the other copy is a wholesale foreign
/// write, so the undo ring resets exactly as an accepted owner broadcast
/// does (#862) — the ring cannot offer undos across the other session's
/// history. Returns the toast text.
pub fn resolve(
    choice: Choice,
    held: OtherSessionRoom,
    live: &mut RoomRecord,
    signals: &mut RoomWriteSignals,
) -> &'static str {
    match choice {
        Choice::KeepMine => "Kept this session's edits — the other session's copy was not applied.",
        Choice::TakeTheirs => {
            *live = held.record;
            signals.foreign = true;
            "Took the other session's copy. Your unsaved edits here are gone; \
             the undo history starts over."
        }
    }
}

/// The keep-or-take modal. Runs while [`OtherSessionRoom`] exists (see the
/// registration in `crate::run`).
pub fn other_session_room_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    held: Res<OtherSessionRoom>,
    mut live: Option<ResMut<LiveRoomRecord>>,
    mut signals: ResMut<RoomWriteSignals>,
    mut toasts: ResMut<crate::notify::Toasts>,
    time: Res<Time>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let Some(live) = live.as_mut() else {
        commands.remove_resource::<OtherSessionRoom>();
        return;
    };
    crate::ui::confirm::note_modal_open(ctx);
    let mut choice: Option<Choice> = None;
    egui::Modal::new(egui::Id::new("other-session-room")).show(ctx, |ui| {
        ui.heading("Your world is open in another session");
        ui.add_space(4.0);
        ui.label(
            "Another session signed in as you changed this world while you have \
             unsaved edits here. Which copy do you want to keep?",
        );
        ui.label(
            egui::RichText::new(
                "Keeping yours leaves the other session's changes out until one of you \
                 saves; taking theirs discards your unsaved edits here.",
            )
            .small(),
        );
        // This modal deliberately does NOT take Esc (#1236 f53): both
        // answers are consequential and there is no third, non-destructive
        // one to make a dismissal mean. A silent refusal reads as a hang,
        // so the refusal is stated instead.
        ui.add_space(4.0);
        ui.small("Choose one to continue — this dialog has no dismiss.");
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Keep my edits").clicked() {
                choice = Some(Choice::KeepMine);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(crate::ui::confirm::danger_button(
                        "Take the other session's",
                        &crate::ui::theme::current(ui.ctx()),
                    ))
                    .clicked()
                {
                    choice = Some(Choice::TakeTheirs);
                }
            });
        });
    });
    if let Some(choice) = choice {
        // The held record moves out of the resource; the resource goes
        // with it. `Res` cannot be moved from, so clone the payload once —
        // a room record is a few KiB, and this happens on a click.
        let held = OtherSessionRoom {
            record: held.record.clone(),
        };
        let text = resolve(choice, held, &mut live.0, &mut signals);
        toasts.info(text, time.elapsed_secs_f64());
        commands.remove_resource::<OtherSessionRoom>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1203. Sequence: edit the world on the laptop for half an hour
    /// without saving; open the same world in a browser on another
    /// machine; the browser's broadcast arrives. The laptop must not
    /// replace its live record — it holds the copy and asks. Two more
    /// sequences ride on the same decision: the echo of our own record
    /// coming back must be ignored (or the two sessions ping-pong
    /// replacements and reset each other's undo rings), and a clean
    /// session simply takes the update.
    #[test]
    fn a_dirty_session_holds_a_clean_one_applies_and_an_echo_is_ignored() {
        assert_eq!(
            classify_same_owner_update(true, false),
            SameOwnerUpdate::Hold
        );
        assert_eq!(
            classify_same_owner_update(false, false),
            SameOwnerUpdate::Apply
        );
        assert_eq!(
            classify_same_owner_update(true, true),
            SameOwnerUpdate::Ignore,
            "an identical record is the echo of our own broadcast"
        );
        assert_eq!(
            classify_same_owner_update(false, true),
            SameOwnerUpdate::Ignore
        );
    }

    /// Keeping mine touches nothing; taking theirs installs the held copy
    /// and resets the ring, because the ring cannot walk back across the
    /// other session's history.
    #[test]
    fn keep_mine_leaves_live_alone_and_take_theirs_is_a_foreign_write() {
        let mine = RoomRecord::default_for_did("did:plc:mine");
        let mut theirs = mine.clone();
        theirs
            .traits
            .insert("theirs".into(), vec!["edited elsewhere".into()]);

        let mut live = mine.clone();
        let mut signals = RoomWriteSignals::default();
        resolve(
            Choice::KeepMine,
            OtherSessionRoom {
                record: theirs.clone(),
            },
            &mut live,
            &mut signals,
        );
        assert!(!crate::state::records_differ(&live, &mine));
        assert!(!signals.foreign, "nothing was written, so no reset");

        resolve(
            Choice::TakeTheirs,
            OtherSessionRoom {
                record: theirs.clone(),
            },
            &mut live,
            &mut signals,
        );
        assert!(!crate::state::records_differ(&live, &theirs));
        assert!(signals.foreign, "a wholesale foreign write resets the ring");
    }
}
