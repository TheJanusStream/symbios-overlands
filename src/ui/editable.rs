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
#[allow(clippy::too_many_arguments)]
pub fn save_load_reset_row(
    ui: &mut egui::Ui,
    dirty: bool,
    can_publish: bool,
    can_reset: bool,
    record_bytes: Option<usize>,
    publish_shortcut: bool,
    publishing: bool,
    // `Some` routes Revert/Reset through the confirm modal — required
    // for the Inventory editor, which has no undo stack (#866). Room
    // and Avatar pass `None`: both replacements are one Ctrl+Z away,
    // so the guard would only double-charge a now-recoverable click.
    mut confirm: Option<&mut crate::ui::confirm::ConfirmState<RecordAction>>,
) -> RecordAction {
    let size_class = record_bytes.map(record_size::classify);
    let over_hard = size_class == Some(SizeClass::OverHardCeiling);
    let mut action = RecordAction::None;
    ui.horizontal(|ui| {
        // While a publish is in flight the button reads "Saving…" and is
        // disabled — a second click used to race a second task against
        // the first (#838).
        let publish_label = if publishing {
            "Saving…"
        } else {
            "Save to PDS"
        };
        let publish = egui::Button::new(egui::RichText::new(publish_label).color(
            if dirty && !publishing {
                crate::ui::theme::current(ui.ctx()).status.ok
            } else {
                crate::ui::theme::current(ui.ctx()).text_weak
            },
        ));
        let enabled = dirty && can_publish && !over_hard && !publishing;
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
        // Revert / Reset are whole-record replacements. With an undo
        // stack behind the editor (`confirm: None`) they fire directly —
        // Ctrl+Z restores the pre-click state. Without one (Inventory)
        // they still route through the confirm modal (#838 → #866).
        let revert_hover = if confirm.is_none() {
            "Discard unsaved edits and restore the last state saved to \
             your PDS this session. Undo (Ctrl+Z) restores them."
        } else {
            "Discard unsaved edits and restore the last state saved to \
             your PDS this session"
        };
        if ui
            .add_enabled(dirty, egui::Button::new("Revert to saved"))
            .on_hover_text(revert_hover)
            .clicked()
        {
            match confirm.as_deref_mut() {
                None => action = RecordAction::Load,
                Some(confirm) => confirm.request(
                    "Revert to saved?",
                    "Discards every unsaved edit and restores the last state saved \
                     to your PDS this session. This cannot be undone.",
                    "Discard edits",
                    RecordAction::Load,
                ),
            }
        }
        if ui
            .add_enabled(can_reset, egui::Button::new("Reset to default"))
            .clicked()
        {
            match confirm.as_deref_mut() {
                None => action = RecordAction::Reset,
                Some(confirm) => confirm.request(
                    "Reset to default?",
                    "Replaces the whole record with its generated default. Unsaved \
                     edits are lost immediately; the copy on your PDS is untouched \
                     until you save.",
                    "Reset",
                    RecordAction::Reset,
                ),
            }
        }
        if let (Some(bytes), Some(class)) = (record_bytes, size_class) {
            let (text, color) = match class {
                SizeClass::WithinBudget => (
                    human_bytes(bytes),
                    crate::ui::theme::current(ui.ctx()).text_weak,
                ),
                SizeClass::OverSoftBudget => (
                    format!("⚠ {}", human_bytes(bytes)),
                    crate::ui::theme::current(ui.ctx()).status.warn,
                ),
                SizeClass::OverHardCeiling => (
                    format!(
                        "{} {} — too large to save",
                        crate::ui::affordances::CROSS,
                        human_bytes(bytes)
                    ),
                    crate::ui::theme::current(ui.ctx()).status.error,
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
    // A confirmed Revert/Reset surfaces as this frame's action, exactly
    // as if the (guarded) button had fired directly.
    if let Some(confirm) = confirm
        && let Some(confirmed) = confirm.show(ui.ctx(), "save-row")
    {
        action = confirmed;
    }
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
            ui.colored_label(
                crate::ui::theme::current(ui.ctx()).status.warn,
                "⟳ Saving to PDS…",
            );
        }
        PublishStatus::Success { at_secs } => {
            crate::ui::affordances::ok_label(ui, format!("Saved ({:.0}s ago)", ago(*at_secs)));
        }
        PublishStatus::Failed { at_secs, message } => {
            ui.colored_label(
                crate::ui::theme::current(ui.ctx()).status.error,
                format!(
                    "{} Save failed ({:.0}s ago): {message}",
                    crate::ui::affordances::CROSS,
                    ago(*at_secs)
                ),
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

impl SeedRowState {
    /// The seed the row currently shows, when it parses. `None` both for
    /// an un-parseable edit in progress and before the first
    /// [`seed_row`] draw synced the buffer — callers fall back to the
    /// DID seed, matching what that first sync will show.
    pub fn current_seed(&self) -> Option<u64> {
        self.buf.trim().parse().ok()
    }

    /// Overwrite the buffer with the seed a pinned re-roll actually used
    /// (#1005): the hunt may land past the typed start, and the row must
    /// always show the seed the record was really built from.
    pub fn set_seed(&mut self, seed: u64) {
        self.buf = seed.to_string();
    }
}

/// Memoized pinned-re-roll seed hunt (#1005). The axis readout must
/// preview the seed "Re-roll" will *actually* build from — with locks
/// engaged the hunt may walk past the typed seed, and previewing the
/// typed seed showed unlocked values a click would then not deliver —
/// but a full hunt costs milliseconds, far too much to rerun every
/// frame. The result is keyed on `(start, pins)` and recomputed only
/// when either changes (a keystroke in the seed field, a 🎲 roll, a lock
/// toggle, a combo pick). Embed one per editor next to its
/// [`SeedRowState`].
pub struct PinHuntCache<P> {
    key: Option<(u64, P)>,
    found: Option<u64>,
}

// Manual impl: a derived `Default` would needlessly bound `P: Default`.
impl<P> Default for PinHuntCache<P> {
    fn default() -> Self {
        Self {
            key: None,
            found: None,
        }
    }
}

impl<P: Copy + PartialEq> PinHuntCache<P> {
    /// The seed a re-roll from `start` under `pins` will build from —
    /// `ScenePins::find_seed` / `AvatarPins::find_seed` passed as
    /// `hunt` — or `None` if the hunt capped out (practically
    /// unreachable for a legal pin-set). Both the readout and the
    /// "Re-roll" handler read this, so the preview and the applied
    /// record can never disagree.
    pub fn effective_seed(
        &mut self,
        start: u64,
        pins: P,
        hunt: impl FnOnce(u64) -> Option<u64>,
    ) -> Option<u64> {
        if self.key != Some((start, pins)) {
            self.key = Some((start, pins));
            self.found = hunt(start);
        }
        self.found
    }
}

/// Wrap an editor's re-roll block — a [`seed_row`] plus its
/// [`pin_axis_row`] readout — in a collapsible section (#1047).
///
/// The block is the tallest thing in either editor's footer (a seed
/// field over five or six pin rows), and an owner who has settled on a
/// world or an avatar rarely re-rolls it again; collapsed it costs one
/// header row and hands the rest back to the tab body above. Open by
/// default — the pinned readout is only discoverable if it starts
/// expanded — and the open/closed state lives in egui memory under
/// `id_salt`, so it survives closing and reopening the editor window.
///
/// Returns the closure's value, or `None` while the section is
/// collapsed and the body did not run. Callers fold that to their
/// "nothing happened" case: a collapsed section shows no "Apply"
/// button, so it can never report an action.
pub fn reroll_section<R>(
    ui: &mut egui::Ui,
    id_salt: &str,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<R> {
    egui::CollapsingHeader::new("Seed & re-roll")
        .id_salt(id_salt)
        .default_open(true)
        .show(ui, body)
        .body_returned
}

/// Render the "Random seed" re-roll row shared by the World and Avatar
/// editors.
///
/// The field shows `did_seed` — the master seed the DID-derived defaults
/// are built from — by default. The owner can type any `u64`, roll a
/// fresh one (🎲), or restore the DID seed (↺), then click "Apply". That
/// it replaces the ENTIRE record — not just the axes shown — is carried
/// by the hover text, which names `subject`. This is exactly the
/// existing "Reset to default" with an owner-chosen seed instead of
/// `fnv1a_64(did)`. `now_secs` seeds the dice without a system clock
/// (wasm has none). Fires on the click itself (#866): the confirm that
/// guarded it pre-undo would only double-charge a one-Ctrl+Z-away
/// replacement, and the undo toast names the seed it replaced.
pub fn seed_row(
    ui: &mut egui::Ui,
    state: &mut SeedRowState,
    did_seed: u64,
    now_secs: f64,
    subject: &str,
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
            field = field.text_color(crate::ui::theme::current(ui.ctx()).status.error);
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
            .on_hover_text(format!(
                "Replace the whole {subject} with a fresh roll from this seed"
            ))
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

/// One row of the pinned re-roll readout under [`seed_row`] (#1005),
/// shared by the World and Avatar editors: what the seed in the row rolls
/// for one category axis, with a lock toggle. Locking captures the shown
/// value into `pin`; a locked axis renders as a combo box so an explicit
/// value can be picked. Pins apply on the next "Apply" click — the
/// caller hunts a seed satisfying them (`ScenePins::find_seed` /
/// `AvatarPins::find_seed`) — matching the seed field's own
/// edit-then-apply contract.
///
/// Draws three cells (axis label, lock, value) and ends the row; call
/// inside an `egui::Grid` so the columns align across axes. The lock
/// glyphs are `🔒`/`🔓` (U+1F512/U+1F513) — both present in egui's
/// embedded NotoEmoji fallback, verified against its cmap (#861 tofu
/// discipline).
pub fn pin_axis_row<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    axis: &str,
    options: &[T],
    label_of: impl Fn(T) -> &'static str,
    pin: &mut Option<T>,
    rolled: T,
) {
    ui.label(format!("{axis}:"));

    let locked = pin.is_some();
    let glyph = if locked { "🔒" } else { "🔓" };
    let hover = if locked {
        format!("{axis} is locked: re-rolls hold it. Click to let it roll freely.")
    } else {
        format!("Lock {axis}: re-rolls will hold the shown value.")
    };
    if ui
        .selectable_label(locked, glyph)
        .on_hover_text(hover)
        .clicked()
    {
        *pin = if locked { None } else { Some(rolled) };
    }

    match pin {
        Some(v) => {
            egui::ComboBox::from_id_salt(("pin_axis", axis))
                .selected_text(label_of(*v))
                .show_ui(ui, |ui| {
                    for opt in options {
                        ui.selectable_value(v, *opt, label_of(*opt));
                    }
                });
        }
        None => {
            ui.weak(label_of(rolled))
                .on_hover_text("Rolled by this seed. Lock to hold it across re-rolls.");
        }
    }
    ui.end_row();
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

/// Poll a publish task, or declare it dead if it has outlived
/// [`crate::config::http::PUBLISH_TASK_DEADLINE`] (#1129).
///
/// `Some(Ok(_))`/`Some(Err(_))` is the task's own result; `Some(Err(_))`
/// from an expiry is synthesised here. `None` means keep waiting.
///
/// Why an outside deadline when the request already has one: the in-task
/// bound races a timer against the fetch, which covers a request that
/// never settles. It does not cover a task that never gets polled to
/// completion for some other reason, and the cost of being wrong is not a
/// slow save — it is an editor pinned on `Publishing` forever, where Save
/// is disabled and the unsaved-edits guard offers no way out. Freeing the
/// editor on a stale task turns that trap back into an ordinary failure
/// the owner can retry.
///
/// The task is dropped when this returns an expiry, which on wasm aborts
/// the underlying fetch.
pub fn poll_or_expire(
    task: &mut bevy::tasks::Task<Result<(), String>>,
    spawned_at: f64,
    now: f64,
    label: &str,
) -> Option<Result<(), String>> {
    if let Some(result) = futures_lite::future::block_on(futures_lite::future::poll_once(task)) {
        return Some(result);
    }
    if now - spawned_at > crate::config::http::PUBLISH_TASK_DEADLINE.as_secs_f64() {
        return Some(Err(crate::config::http::timed_out(label)));
    }
    None
}

#[cfg(test)]
mod publish_deadline_tests {
    use super::*;

    /// A task that never resolves, standing in for a browser fetch that
    /// connected and then went silent.
    fn never_lands() -> bevy::tasks::Task<Result<(), String>> {
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default)
            .spawn(std::future::pending())
    }

    fn lands_ok() -> bevy::tasks::Task<Result<(), String>> {
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::default).spawn(async { Ok(()) })
    }

    /// The trap this closes (#1129): a stalled publish left the editor on
    /// `PublishStatus::Publishing` forever — Save disabled, and the
    /// unsaved-edits guard auto-entering a phase whose only button was
    /// "Continue in background". The sequence was one request that
    /// connected and never settled; on wasm nothing bounded it, and unlike
    /// native there was no client timeout to end it.
    #[test]
    fn a_task_past_the_deadline_is_declared_failed() {
        let mut task = never_lands();
        let deadline = crate::config::http::PUBLISH_TASK_DEADLINE.as_secs_f64();

        assert!(
            poll_or_expire(&mut task, 0.0, deadline, "test").is_none(),
            "at the deadline it is still waiting — the bound is exclusive"
        );
        let expired = poll_or_expire(&mut task, 0.0, deadline + 0.001, "test")
            .expect("past the deadline the editor must be freed");
        let message = expired.expect_err("expiry is a failure, not a success");
        assert!(
            message.contains("timed out"),
            "the owner is told why, not just that Save came back: {message}"
        );
    }

    /// The control, and the thing that would break if the deadline were
    /// mistaken for a timeout on the request itself: a task that lands
    /// normally reports its own result, whatever the clock says.
    #[test]
    fn a_landed_task_reports_its_own_result_however_old_it_is() {
        let mut task = lands_ok();
        // Wait for the pool WITHOUT going through `poll_or_expire` — asking
        // it would race the deadline against the scheduler and make this
        // test's own result depend on thread timing.
        //
        // `yield_now`, not a spin: the whole suite runs many processes at
        // once, and a busy-wait that never gives up its slice starved the
        // very worker it was waiting for. Bounded by the wall clock rather
        // than an iteration count for the same reason — an iteration count
        // means something different on a loaded machine.
        let give_up_at = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !task.is_finished() && std::time::Instant::now() < give_up_at {
            std::thread::yield_now();
        }
        assert!(task.is_finished(), "task never landed");

        // A save that SUCCEEDED must not be rewritten into a failure just
        // because the clock ran on: the deadline frees a stuck editor, it
        // does not overrule a result that exists.
        let far_future = crate::config::http::PUBLISH_TASK_DEADLINE.as_secs_f64() * 10.0;
        let result = poll_or_expire(&mut task, 0.0, far_future, "test")
            .expect("a finished task always yields its result");
        assert!(result.is_ok(), "the task's own Ok must survive");
    }
}
