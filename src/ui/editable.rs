//! Shared Save / Load / Reset UI for every PDS-backed editable record.
//!
//! Before this module the Room, Avatar and Inventory editors each
//! hand-rolled their own commit row and status line, and they had
//! drifted apart: Room/Inventory showed a live "(Ns ago)" timer while
//! Avatar showed a static "Published ✓"; Room cleared dirty
//! optimistically (so a failed publish could not be retried); Inventory
//! had no Load/Reset at all; and Room+Avatar shared one
//! `PublishFeedback` resource so publishing one stamped the other's
//! status line.
//!
//! Every editor now renders the **same** button row
//! ([`save_load_reset_row`]) and the **same** status line
//! ([`publish_status_line`]) over a per-record
//! [`PublishFeedback`](crate::state::PublishFeedback). The helper only
//! owns the look + uniform enable rules and reports a [`RecordAction`];
//! the caller still performs the record-specific work (clone + spawn
//! the publish task, copy stored→live / default→live, refresh any raw
//! JSON mirror, clear selections) because those side effects genuinely
//! differ per record.

use bevy_egui::egui;

use crate::state::PublishStatus;

/// Which Save/Load/Reset button the owner clicked this frame. The
/// caller maps each arm to the record-specific effect.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RecordAction {
    /// Nothing clicked this frame.
    None,
    /// "Publish to PDS" — push `live` to the PDS; on success the poll
    /// system pins `stored = live`.
    Publish,
    /// "Load from PDS" — discard uncommitted edits (`live = stored`).
    Load,
    /// "Reset to default" — `live = default_for_did(did)`.
    Reset,
}

/// Render the uniform Publish / Load / Reset row.
///
/// Enable rules, identical for all three records:
/// * **Publish** — `dirty && can_publish` (a session + refresh context
///   must exist to write to the PDS). Tinted green while dirty, grey
///   when clean, so "there is something to save" is glanceable. Never
///   cleared optimistically: the derived `dirty` only drops once the
///   poll system pins `stored = live` on a *successful* round-trip, so
///   a failed publish stays dirty and retryable.
/// * **Load from PDS** — `dirty` (nothing to revert when clean).
/// * **Reset to default** — `can_reset` (the live record already
///   differs from the canonical default).
pub fn save_load_reset_row(
    ui: &mut egui::Ui,
    dirty: bool,
    can_publish: bool,
    can_reset: bool,
) -> RecordAction {
    let mut action = RecordAction::None;
    ui.horizontal(|ui| {
        let publish = egui::Button::new(egui::RichText::new("Publish to PDS").color(if dirty {
            egui::Color32::LIGHT_GREEN
        } else {
            egui::Color32::GRAY
        }));
        if ui.add_enabled(dirty && can_publish, publish).clicked() {
            action = RecordAction::Publish;
        }
        if ui
            .add_enabled(dirty, egui::Button::new("Load from PDS"))
            .clicked()
        {
            action = RecordAction::Load;
        }
        if ui
            .add_enabled(can_reset, egui::Button::new("Reset to default"))
            .clicked()
        {
            action = RecordAction::Reset;
        }
    });
    action
}

/// Render the uniform publish status line. `Idle` draws nothing; every
/// other state is a single coloured line, and **both** Success and
/// Failed carry the same live `(Ns ago)` counter (Avatar used to drop
/// it). Wording is identical across editors — the editor window's own
/// title already says *which* record, so the line stays terse.
pub fn publish_status_line(ui: &mut egui::Ui, status: &PublishStatus, now_secs: f64) {
    let ago = |at: f64| (now_secs - at).max(0.0);
    match status {
        PublishStatus::Idle => {}
        PublishStatus::Publishing => {
            ui.colored_label(egui::Color32::from_rgb(220, 200, 80), "⟳ Saving to PDS…");
        }
        PublishStatus::Success { at_secs } => {
            ui.colored_label(
                egui::Color32::from_rgb(80, 200, 120),
                format!("✓ Saved ({:.0}s ago)", ago(*at_secs)),
            );
        }
        PublishStatus::Failed { at_secs, message } => {
            ui.colored_label(
                egui::Color32::from_rgb(220, 90, 90),
                format!("✗ Save failed ({:.0}s ago): {message}", ago(*at_secs)),
            );
        }
    }
}
