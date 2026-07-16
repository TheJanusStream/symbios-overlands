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

use crate::diagnostics::event::{EventPayload, RecordKind};
use crate::diagnostics::{MetricsRegistry, SessionLog, names};
use crate::pds::record_size::{
    self, HARD_RECORD_CEILING_BYTES, SOFT_RECORD_BUDGET_BYTES, SizeClass, human_bytes,
};
use crate::state::PublishStatus;

/// Which Save/Load/Reset button the owner clicked this frame. The
/// caller maps each arm to the record-specific effect.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RecordAction {
    /// Nothing clicked this frame.
    None,
    /// "Save to PDS" — push `live` to the PDS; on success the poll
    /// system pins `stored = live`.
    Publish,
    /// "Revert to saved" — discard uncommitted edits (`live = stored`,
    /// the session-cached copy; no network fetch happens — the old
    /// "Load from PDS" label promised one, #830).
    Load,
    /// "Reset to default" — `live = default_for_did(did)`.
    Reset,
}

/// Render the uniform Publish / Load / Reset row.
///
/// Enable rules, identical for all three records:
/// * **Publish** — `dirty && can_publish` (a session + refresh context
///   must exist to write to the PDS), and the live record must be under
///   the hard size ceiling (`record_bytes`, see below). Tinted green
///   while dirty, grey when clean, so "there is something to save" is
///   glanceable. Never cleared optimistically: the derived `dirty` only
///   drops once the poll system pins `stored = live` on a *successful*
///   round-trip, so a failed publish stays dirty and retryable.
/// * **Revert to saved** — `dirty` (nothing to revert when clean).
/// * **Reset to default** — `can_reset` (the live record already
///   differs from the canonical default).
///
/// `record_bytes` is the live record's serialized size (the throttled
/// cache in [`crate::state::PublishFeedback`], `None` while never
/// measured). The row appends a size readout — neutral under the
/// [`SOFT_RECORD_BUDGET_BYTES`] soft budget, amber past it, red past the
/// [`HARD_RECORD_CEILING_BYTES`] hard ceiling — and past the ceiling the
/// Publish button is disabled outright, mirroring the pre-flight guard
/// in `crate::pds::record_size::preflight` (#694).
pub fn save_load_reset_row(
    ui: &mut egui::Ui,
    dirty: bool,
    can_publish: bool,
    can_reset: bool,
    record_bytes: Option<usize>,
    publish_shortcut: bool,
) -> RecordAction {
    let size_class = record_bytes.map(record_size::classify);
    let over_hard = size_class == Some(SizeClass::OverHardCeiling);
    let mut action = RecordAction::None;
    ui.horizontal(|ui| {
        let publish = egui::Button::new(egui::RichText::new("Save to PDS").color(if dirty {
            egui::Color32::LIGHT_GREEN
        } else {
            egui::Color32::GRAY
        }));
        let enabled = dirty && can_publish && !over_hard;
        if ui
            .add_enabled(enabled, publish)
            .on_hover_text("Save your edits to your PDS (Ctrl+S)")
            .clicked()
        {
            action = RecordAction::Publish;
        }
        // Ctrl+S (#836) — behind the SAME gate as the button, so the
        // shortcut can never publish what a click could not.
        if publish_shortcut && enabled {
            action = RecordAction::Publish;
        }
        if ui
            .add_enabled(dirty, egui::Button::new("Revert to saved"))
            .on_hover_text(
                "Discard unsaved edits and restore the last state saved to \
                 your PDS this session",
            )
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
        if let (Some(bytes), Some(class)) = (record_bytes, size_class) {
            let (text, color) = match class {
                SizeClass::WithinBudget => (human_bytes(bytes), egui::Color32::GRAY),
                SizeClass::OverSoftBudget => (
                    format!("⚠ {}", human_bytes(bytes)),
                    egui::Color32::from_rgb(220, 200, 80),
                ),
                SizeClass::OverHardCeiling => (
                    format!("✗ {} — too large to save", human_bytes(bytes)),
                    egui::Color32::from_rgb(220, 90, 90),
                ),
            };
            ui.label(egui::RichText::new(text).color(color).small())
                .on_hover_text(format!(
                    "Serialized size of the largest record this editor publishes \
                     (the whole record for Room/Avatar; the biggest single item \
                     for Inventory). Soft budget {} (warns), hard ceiling {} \
                     (blocks saving — an ATProto record is a single ~1 MiB-max repo \
                     block). Remove or shrink content to fit.",
                    human_bytes(SOFT_RECORD_BUDGET_BYTES),
                    human_bytes(HARD_RECORD_CEILING_BYTES),
                ));
        }
    });
    action
}

/// Throttled refresh of the live record's serialized-size cache in
/// [`PublishFeedback`](crate::state::PublishFeedback), returning the current
/// reading for [`save_load_reset_row`]. Serializing the full record every
/// frame would be wasted work, so the cache refreshes at
/// [`SIZE_READOUT_REFRESH_SECS`](crate::config::ui::editor::SIZE_READOUT_REFRESH_SECS)
/// cadence — at worst the readout (and its publish hard-block) lags an edit
/// by half a second, and the pre-flight guard in
/// `crate::pds::record_size::preflight` backstops that window.
pub fn refresh_size_readout<R: Send + Sync + 'static, T: serde::Serialize>(
    feedback: &mut crate::state::PublishFeedback<R>,
    live: &T,
    now: f64,
) -> Option<usize> {
    if feedback
        .live_bytes_at
        .is_none_or(|at| now - at >= crate::config::ui::editor::SIZE_READOUT_REFRESH_SECS)
    {
        feedback.live_bytes = record_size::serialized_record_bytes(live);
        feedback.live_bytes_at = Some(now);
    }
    feedback.live_bytes
}

/// Record a publish attempt's serialized size into the metrics registry and
/// session log (#694). Shared by the three publish-poll systems so the
/// gauge and event emission stays identical per record kind. Severity
/// encodes the budget classification (info / warn / error past the hard
/// ceiling — where the pre-flight guard refused the write). `bytes` is
/// `None` only when the record failed to serialize, which the guard
/// reports separately.
pub fn log_record_size(
    session_log: &mut SessionLog,
    metrics: &mut MetricsRegistry,
    now: f64,
    record: RecordKind,
    bytes: Option<usize>,
) {
    let Some(bytes) = bytes else { return };
    let gauge = match record {
        RecordKind::Room => names::RECORD_SIZE_ROOM_BYTES,
        RecordKind::Avatar => names::RECORD_SIZE_AVATAR_BYTES,
        RecordKind::Inventory => names::RECORD_SIZE_INVENTORY_BYTES,
    };
    metrics.observe_gauge(gauge, bytes as f64);
    let payload = EventPayload::RecordSizeMeasured {
        record,
        bytes: bytes as u64,
        soft_budget_bytes: SOFT_RECORD_BUDGET_BYTES as u64,
        hard_ceiling_bytes: HARD_RECORD_CEILING_BYTES as u64,
    };
    match record_size::classify(bytes) {
        SizeClass::WithinBudget => session_log.info(now, payload),
        SizeClass::OverSoftBudget => session_log.warn(now, payload),
        SizeClass::OverHardCeiling => session_log.error(now, payload),
    };
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
