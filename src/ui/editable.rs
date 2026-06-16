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
        let publish = egui::Button::new(egui::RichText::new("Save to PDS").color(if dirty {
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

/// Outcome of the manual re-roll [`seed_row`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SeedAction {
    /// Nothing actionable this frame.
    None,
    /// "Apply" clicked with a parseable seed — the caller re-rolls the
    /// whole record from it (`live = T::default_for_seed(seed, did)`).
    Reroll(u64),
}

/// Editor-owned state for the manual re-roll seed row. Embed one in each
/// editor's state resource and hand a `&mut` to [`seed_row`].
#[derive(Default)]
pub struct SeedRowState {
    /// The text the owner is editing. Empty until first synced.
    buf: String,
    /// DID-derived seed the buffer was last synced to. Re-syncs the
    /// buffer whenever the active DID (hence its seed) changes — e.g.
    /// after logging in as a different user — so the field never shows a
    /// stale owner's seed.
    synced_for: Option<u64>,
}

/// Render the "Random seed" re-roll row shared by the World and Avatar
/// editors.
///
/// The field shows `did_seed` — the master seed the DID-derived defaults
/// are built from — by default. The owner can type any `u64`, roll a
/// fresh one (🎲), or restore the DID seed (↺), then click **Apply** to
/// re-roll the whole record from that seed. This is exactly the existing
/// "Reset to default" with an owner-chosen seed instead of
/// `fnv1a_64(did)`. `now_secs` seeds the dice without a system clock
/// (wasm has none). Returns [`SeedAction::Reroll`] only on Apply with a
/// parseable seed.
pub fn seed_row(
    ui: &mut egui::Ui,
    state: &mut SeedRowState,
    did_seed: u64,
    now_secs: f64,
) -> SeedAction {
    // (Re)initialise the buffer to the DID seed on first use and whenever
    // the active DID's seed changes.
    if state.synced_for != Some(did_seed) {
        state.buf = did_seed.to_string();
        state.synced_for = Some(did_seed);
    }

    let mut action = SeedAction::None;
    ui.horizontal(|ui| {
        ui.label("Random seed:");

        // `parse` returns an owned `Result`, so this immutable borrow of
        // `buf` ends before the `&mut buf` the TextEdit takes below.
        let parsed = state.buf.trim().parse::<u64>();
        let mut field = egui::TextEdit::singleline(&mut state.buf).desired_width(190.0);
        if parsed.is_err() {
            field = field.text_color(egui::Color32::from_rgb(220, 90, 90));
        }
        ui.add(field).on_hover_text(
            "Master seed for the DID-derived defaults. Edit, then Apply to re-roll.",
        );

        if ui
            .button("🎲")
            .on_hover_text("Roll a fresh random seed")
            .clicked()
        {
            state.buf = dice_seed(now_secs, did_seed).to_string();
        }
        let apply_clicked = ui
            .add_enabled(parsed.is_ok(), egui::Button::new("Apply"))
            .on_hover_text("Re-roll the whole record from this seed")
            .clicked();
        if let (true, Ok(seed)) = (apply_clicked, parsed) {
            action = SeedAction::Reroll(seed);
        }
        if ui
            .button("↺")
            .on_hover_text("Restore the DID-derived seed")
            .clicked()
        {
            state.buf = did_seed.to_string();
        }
    });
    action
}

/// Diffuse a frame-time float + the DID seed into a fresh pseudo-random
/// `u64` for the 🎲 button. Not cryptographic — it only needs to look
/// random and differ frame-to-frame. `SystemTime` is unavailable on
/// wasm, so the entropy is the caller's elapsed-seconds clock.
fn dice_seed(now_secs: f64, salt: u64) -> u64 {
    // splitmix64 over the time bits combined with the DID seed.
    let mut z = now_secs
        .to_bits()
        .wrapping_add(salt)
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
