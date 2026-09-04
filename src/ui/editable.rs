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
    HARD_RECORD_CEILING_BYTES, SOFT_RECORD_BUDGET_BYTES, SizeClass, SizeReadout, human_bytes,
};
use crate::state::PublishStatus;

/// Which Save/Load/Reset button the owner clicked this frame. The
/// caller maps each arm to the record-specific effect.
#[derive(Clone, PartialEq, Eq, Debug)]
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
    /// Ctrl+S arrived and the row's own gate refused it (#1208): the
    /// reason, in the words the disabled Save button's hover uses. The
    /// caller toasts it through [`ctrl_s_refused`] — the button can be
    /// hovered to learn why, a keypress cannot, so the shortcut has to be
    /// told out loud.
    Refused(String),
}

/// Why "Save to PDS" is disabled right now, in ONE place: the button's
/// disabled hover and the Ctrl+S refusal toast both read it, so the click
/// and the chord can never explain the same gate two ways. `None` means
/// the button is enabled. Order is by immediacy: a save in flight is the
/// fact of the moment; a session that cannot write at all is the fact of
/// the session; the size ceiling and "nothing to save" come after.
///
/// The expired-session arm (#1214) is what turns the retry loop into a
/// dead stop. Every other refusal here describes something the owner can
/// change; this one describes something they cannot, so it says what they
/// have to do instead. Its input is the record's own last outcome —
/// `report_publish_failure` marks a failure terminal when the OAuth
/// refresh token is gone (see [`crate::oauth::refresh_is_terminal`]) — so
/// the row cannot claim the session expired on a save that never failed.
fn save_refusal(
    dirty: bool,
    can_publish: bool,
    size: &SizeReadout,
    publishing: bool,
    status: &PublishStatus,
) -> Option<String> {
    if publishing {
        Some(String::from("a save is already in flight"))
    } else if matches!(status, PublishStatus::Failed { terminal: true, .. }) {
        Some(String::from(
            "your session has expired — sign in again before saving",
        ))
    } else if let Some(reason) = &size.unserializable {
        Some(reason.clone())
    } else if size.class() == Some(SizeClass::OverHardCeiling) {
        Some(match &size.largest {
            Some(largest) => format!(
                "{largest} is {} — past the {} ceiling; remove or shrink it",
                human_bytes(size.bytes.unwrap_or_default()),
                human_bytes(HARD_RECORD_CEILING_BYTES)
            ),
            None => String::from("the record is too large to save"),
        })
    } else if !dirty {
        Some(String::from("nothing to save — no unsaved edits"))
    } else if !can_publish {
        Some(String::from("saving is not possible right now"))
    } else {
        None
    }
}

/// Why "Revert to saved" is disabled (#1206). Revert is a whole-record
/// replacement with `stored`, and during a save `stored` is about to
/// change: a Revert clicked while "Saving…" restored the PRE-save snapshot,
/// the landing publish then pinned `stored` to what it wrote, and the row
/// went dirty again holding the very edits the owner had just discarded.
fn revert_refusal(dirty: bool, publishing: bool) -> Option<&'static str> {
    if publishing {
        Some("Wait for the save to finish")
    } else if !dirty {
        Some("Nothing to revert — no unsaved edits")
    } else {
        None
    }
}

/// Why "Reset to default" is disabled — the same in-flight rule as
/// Revert, and otherwise "already the default" (#1209).
fn reset_refusal(can_reset: bool, publishing: bool) -> Option<&'static str> {
    if publishing {
        Some("Wait for the save to finish")
    } else if !can_reset {
        Some("Already the default")
    } else {
        None
    }
}

/// The toast for a Ctrl+S the row refused, so all three editors word it
/// identically.
pub fn ctrl_s_refused(reason: &str) -> String {
    format!("Ctrl+S did not save: {reason}")
}

/// What the size readout's number measures, per record (#1207). One
/// sentence used to cover all three and was wrong for two of them: it
/// said "the whole record" while the Room measured its biggest child and
/// the Avatar measured a reference-only record.
fn size_measures(kind: RecordKind) -> &'static str {
    match kind {
        RecordKind::Room => {
            "the largest single record a save writes — the room manifest (environment, \
             placements, traits, effects) or the biggest generator"
        }
        RecordKind::Avatar => {
            "the largest record in the avatar bundle — the avatar record, the worn body, \
             a worn prop, or the profile"
        }
        RecordKind::Inventory => "the largest single item in the stash",
    }
}

/// Inputs to [`save_load_reset_row`]. A struct rather than ten positional
/// parameters, so the three call sites name what they pass.
pub struct SaveRow<'a> {
    /// Which record this row saves — words the size hover per record.
    pub kind: RecordKind,
    /// The live record differs from its stored mirror.
    pub dirty: bool,
    /// A session and refresh context exist to write with (and, for the
    /// stash, the item cap is not exceeded).
    pub can_publish: bool,
    /// The live record differs from the canonical default.
    pub can_reset: bool,
    /// The throttled measurement of what a save would write.
    pub size: &'a SizeReadout,
    /// A Ctrl+S request landed on this row this frame.
    pub publish_shortcut: bool,
    /// The record's publish status; read for the in-flight gate, reset to
    /// `Idle` by a Revert or Reset so an outcome from before the
    /// replacement is never quoted after it (#1206).
    pub status: &'a mut PublishStatus,
    /// `Some` routes Revert/Reset through the confirm modal — required
    /// for the Inventory editor, which has no undo stack (#866). Room
    /// and Avatar pass `None`: both replacements are one Ctrl+Z away,
    /// so the guard would only double-charge a now-recoverable click.
    pub confirm: Option<&'a mut crate::ui::confirm::ConfirmState<RecordAction>>,
    pub reset: ResetWording,
}

/// What "Reset to default" does to this editor's record, for the button's
/// hover and the confirm copy (#1200). The room and avatar have a DID-seeded
/// default the reset rebuilds; the inventory has none — its "default" is an
/// empty stash, and the shared wording ("replaces the whole record with its
/// generated default") never said that every item goes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResetWording {
    /// A generated default replaces the record.
    Record,
    /// The stash is emptied; `items` is how many go.
    EmptyStash { items: usize },
}

impl ResetWording {
    /// The button's hover. `undoable` (no confirm: an undo stack is behind
    /// the editor) adds the recovery sentence the neighbouring Revert
    /// carries — the most destructive control in the row used to be the
    /// only one that did not say Ctrl+Z restores it (#1209).
    fn hover(self, undoable: bool) -> String {
        match self {
            Self::Record if undoable => String::from(
                "Replace the whole record with its generated default. The copy on \
                 your PDS is untouched until you save. Undo (Ctrl+Z) restores your edits.",
            ),
            Self::Record => String::from(
                "Replace the whole record with its generated default. The copy on \
                 your PDS is untouched until you save.",
            ),
            Self::EmptyStash { items } => format!(
                "Empty your inventory — deletes all {items} item{}. The inventory has \
                 no undo; your PDS copy is untouched until you save.",
                if items == 1 { "" } else { "s" }
            ),
        }
    }

    fn confirm(self) -> (&'static str, String, &'static str) {
        match self {
            Self::Record => (
                "Reset to default?",
                String::from(
                    "Replaces the whole record with its generated default. Unsaved \
                     edits are lost immediately; the copy on your PDS is untouched \
                     until you save.",
                ),
                "Reset",
            ),
            Self::EmptyStash { items } => (
                "Empty your inventory?",
                format!(
                    "Deletes all {items} item{} from your stash. The inventory has no \
                     undo. Your PDS copy is untouched until you save.",
                    if items == 1 { "" } else { "s" }
                ),
                "Empty inventory",
            ),
        }
    }
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
/// `size` is the throttled measurement of what a save would write (the
/// cache in [`crate::state::PublishFeedback`]). The row appends a size
/// readout — neutral under the [`SOFT_RECORD_BUDGET_BYTES`] soft budget,
/// amber past it, red past the [`HARD_RECORD_CEILING_BYTES`] hard ceiling,
/// and red "can't be saved" for a record this build cannot serialize —
/// and in the last two states the Publish button is disabled outright
/// with the reason on its hover, mirroring the pre-flight guard in
/// `crate::pds::record_size::preflight` (#694, #1207).
pub fn save_load_reset_row(ui: &mut egui::Ui, row: SaveRow<'_>) -> RecordAction {
    let SaveRow {
        kind,
        dirty,
        can_publish,
        can_reset,
        size,
        publish_shortcut,
        status,
        mut confirm,
        reset,
    } = row;
    let publishing = status.is_publishing();
    // Resolved before the closure borrows `status` mutably; the row reads
    // the record's last outcome to know whether a retry can work at all.
    let refusal = save_refusal(dirty, can_publish, size, publishing, status);
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
        let enabled = refusal.is_none();
        if ui
            .add_enabled(enabled, publish)
            .on_hover_text("Save your edits to your PDS (Ctrl+S)")
            .on_disabled_hover_text(format!(
                "Can't save: {}",
                refusal.as_deref().unwrap_or_default()
            ))
            .clicked()
        {
            action = RecordAction::Publish;
        }
        // Ctrl+S (#836) — behind the SAME gate as the button, so the
        // shortcut can never publish what a click could not. A refused
        // chord reports the gate's reason instead of vanishing (#1208).
        if publish_shortcut {
            action = match refusal {
                None => RecordAction::Publish,
                Some(reason) => RecordAction::Refused(reason),
            };
        }
        // Revert / Reset are whole-record replacements. With an undo
        // stack behind the editor (`confirm: None`) they fire directly —
        // Ctrl+Z restores the pre-click state. Without one (Inventory)
        // they still route through the confirm modal (#838 → #866).
        // Both stand down while a save is in flight (#1206).
        let revert_hover = if confirm.is_none() {
            "Discard unsaved edits and restore the last state saved to \
             your PDS this session. Undo (Ctrl+Z) restores them."
        } else {
            "Discard unsaved edits and restore the last state saved to \
             your PDS this session"
        };
        let revert_refused = revert_refusal(dirty, publishing);
        if ui
            .add_enabled(
                revert_refused.is_none(),
                egui::Button::new("Revert to saved"),
            )
            .on_hover_text(revert_hover)
            .on_disabled_hover_text(revert_refused.unwrap_or_default())
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
        let reset_refused = reset_refusal(can_reset, publishing);
        if ui
            .add_enabled(
                reset_refused.is_none(),
                egui::Button::new("Reset to default"),
            )
            .on_hover_text(reset.hover(confirm.is_none()))
            .on_disabled_hover_text(reset_refused.unwrap_or_default())
            .clicked()
        {
            match confirm.as_deref_mut() {
                None => action = RecordAction::Reset,
                Some(confirm) => {
                    let (title, body, button) = reset.confirm();
                    confirm.request(title, body, button, RecordAction::Reset);
                }
            }
        }
        size_readout(ui, kind, size);
    });
    // A confirmed Revert/Reset surfaces as this frame's action, exactly
    // as if the (guarded) button had fired directly.
    if let Some(confirm) = confirm
        && let Some(confirmed) = confirm.show(ui.ctx(), "save-row")
    {
        action = confirmed;
    }
    // A Revert or Reset replaces the record the last outcome was about;
    // quoting that outcome afterwards — "✔ Saved" over a reverted record,
    // or a "✖ Save failed" the unsaved guard later reads as THIS attempt's
    // reason — is what #1206 found. Nothing is in flight here: both
    // buttons stand down while publishing.
    if matches!(action, RecordAction::Load | RecordAction::Reset) {
        *status = PublishStatus::Idle;
    }
    action
}

/// The size readout at the end of the row: a number classed against the
/// budgets, or "can't be saved" for a record this build cannot write, with
/// a hover that says what the number measures for THIS record and which
/// part holds it (#1207).
fn size_readout(ui: &mut egui::Ui, kind: RecordKind, size: &SizeReadout) {
    let theme = crate::ui::theme::current(ui.ctx());
    if let Some(reason) = &size.unserializable {
        ui.label(
            egui::RichText::new(format!("{} can't be saved", crate::ui::affordances::CROSS))
                .color(theme.status.error)
                .small(),
        )
        .on_hover_text(reason);
        return;
    }
    let (Some(bytes), Some(class)) = (size.bytes, size.class()) else {
        return;
    };
    let (text, color) = match class {
        SizeClass::WithinBudget => (human_bytes(bytes), theme.text_weak),
        SizeClass::OverSoftBudget => (format!("⚠ {}", human_bytes(bytes)), theme.status.warn),
        SizeClass::OverHardCeiling => (
            format!(
                "{} {} — too large to save",
                crate::ui::affordances::CROSS,
                human_bytes(bytes)
            ),
            theme.status.error,
        ),
    };
    let largest = size
        .largest
        .as_deref()
        .map(|largest| format!("Largest: {largest} at {}. ", human_bytes(bytes)))
        .unwrap_or_default();
    ui.label(egui::RichText::new(text).color(color).small())
        .on_hover_text(format!(
            "Serialized size of {}. {largest}Soft budget {} (warns), hard ceiling {} \
             (blocks saving — an ATProto record is a single ~1 MiB-max repo block). \
             Remove or shrink content to fit.",
            size_measures(kind),
            human_bytes(SOFT_RECORD_BUDGET_BYTES),
            human_bytes(HARD_RECORD_CEILING_BYTES),
        ));
}

/// Throttled refresh of the live record's size cache in
/// [`PublishFeedback`](crate::state::PublishFeedback), returning the current
/// reading for [`save_load_reset_row`]. `measure` is the record's own
/// `measure_publish` — what its save actually writes (#1207). Serializing
/// the full record every frame would be wasted work, so the cache refreshes at
/// [`SIZE_READOUT_REFRESH_SECS`](crate::config::ui::editor::SIZE_READOUT_REFRESH_SECS)
/// cadence — at worst the readout (and its publish hard-block) lags an edit
/// by half a second, and the pre-flight guard in
/// `crate::pds::record_size::preflight` backstops that window.
pub fn refresh_size_readout<R: Send + Sync + 'static, T>(
    feedback: &mut crate::state::PublishFeedback<R>,
    live: &T,
    now: f64,
    measure: impl FnOnce(&T) -> SizeReadout,
) -> bool {
    if feedback
        .live_bytes_at
        .is_none_or(|at| now - at >= crate::config::ui::editor::SIZE_READOUT_REFRESH_SECS)
    {
        feedback.live_size = measure(live);
        feedback.live_bytes_at = Some(now);
        true
    } else {
        false
    }
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
    match crate::pds::record_size::classify(bytes) {
        SizeClass::WithinBudget => session_log.info(now, payload),
        SizeClass::OverSoftBudget => session_log.warn(now, payload),
        SizeClass::OverHardCeiling => session_log.error(now, payload),
    };
}

/// Which PDS write failed — the two verbs the poll systems dispatch. Only
/// the room's "Reset to default" takes the delete-then-put path, but a
/// failed reset reads as "couldn't reset", not "couldn't save", and the
/// user-facing wording has to say which.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WriteOp {
    Save,
    Reset,
}

/// Where a failed PDS write is reported: the log it is recorded in, the
/// editor's own status line, the toast stack, and the panel set that decides
/// which window is on screen. Bundled so [`report_publish_failure`] states
/// one destination rather than four parameters.
pub struct FailureSinks<'a, R: 'static + Send + Sync> {
    pub session_log: &'a mut SessionLog,
    pub feedback: &'a mut crate::state::PublishFeedback<R>,
    pub toasts: &'a mut crate::ui::toast::Toasts,
    pub panels: &'a mut crate::ui::toolbar::UiPanels,
}

/// Everything a failed PDS write owes the user, in one place (#1137).
///
/// A failed write is the one outcome a thin client must make loud: the
/// record IS the world, so a save that did not land means the next session
/// starts from older state. Three everyday flows leave the editor's own
/// footer unread when the failure arrives — Ctrl+S then Esc-closing the
/// window (the request TTL lets the save proceed with the window shut), the
/// unsaved guard's "Stay here (save continues)", and a publish fired just
/// before a portal hop. So the report is: log it, record the typed session
/// event, toast it, and re-open the window that carries the Retry.
///
/// Inventory got exactly this treatment in #843(e) and Room and Avatar did
/// not, which left the three editors — written against one shared row
/// precisely so their behaviour could not diverge — surfacing the same
/// failure three different ways. One function now, so the next one cannot
/// drift either.
pub fn report_publish_failure<R: 'static + Send + Sync>(
    record: RecordKind,
    op: WriteOp,
    did: String,
    error: String,
    now: f64,
    sinks: FailureSinks<'_, R>,
) {
    let FailureSinks {
        session_log,
        feedback,
        toasts,
        panels,
    } = sinks;
    let noun = match record {
        RecordKind::Room => "world",
        RecordKind::Avatar => "avatar",
        RecordKind::Inventory => "inventory",
    };
    let verb = match op {
        WriteOp::Save => "save",
        WriteOp::Reset => "reset",
    };
    bevy::log::warn!("Failed to {verb} {noun} record: {error}");
    // The raw chain goes in the durable record regardless — the friendly
    // sentence below is for the human, and a post-mortem needs the shape
    // the PDS actually returned.
    session_log.error(
        now,
        EventPayload::RecordWriteFailed {
            record,
            did,
            reason: error.clone(),
        },
    );
    // Terminal means retrying cannot work (#1214): the OAuth refresh token
    // is expired or revoked, so this exact failure follows every click. The
    // classification is derived here rather than passed in, so no caller can
    // forget it and no two doors can disagree.
    let terminal = crate::oauth::refresh_is_terminal(&error);
    if terminal {
        // `friendly_login_error` already owns the sentence for this state —
        // it was simply unreachable from in-game, which is why the owner got
        // a raw `refresh: …` Rust error chain as their primary feedback.
        let (friendly, _raw) = crate::ui::login::friendly_login_error(&error);
        toasts.error(format!("Couldn't {verb} your {noun}. {friendly}"), now);
        // NOT re-opened. The window's only offered action is the Save that
        // cannot succeed, and forcing it back into view on every attempt is
        // what turned a failure into a loop — the auto-open is good for a
        // 5xx or a timeout and actively worse here.
        feedback.status = PublishStatus::Failed {
            at_secs: now,
            message: friendly,
            terminal: true,
        };
        return;
    }
    toasts.error(format!("Couldn't {verb} your {noun} — {error}"), now);
    // The window is where the status line and the Save button that retries
    // live, so the toast has somewhere to point.
    match record {
        RecordKind::Room => panels.world_editor = true,
        RecordKind::Avatar => panels.avatar = true,
        RecordKind::Inventory => panels.inventory = true,
    }
    feedback.status = PublishStatus::Failed {
        at_secs: now,
        message: error,
        terminal: false,
    };
}

/// The three record-recovery markers, read together (#1199).
///
/// A marker means the loader installed a synthesised default as BOTH live
/// and stored while the owner's real record still sits on the PDS, so
/// "dirty" reads clean and any publish of that record overwrites a copy
/// this client never read. The markers used to be consulted by the three
/// editors' own Save rows only; the unsaved guard's "Publish & …" and the
/// gift auto-publish walked straight past them. Every door onto a publish
/// now asks this one type, so the doors cannot drift apart again.
///
/// A marker retires where success is known — in the poll systems, on the
/// `Ok` arm — never at the click that asked for the overwrite. Retiring on
/// the click left a failed recovery with no banner and no retry.
#[derive(bevy::ecs::system::SystemParam)]
pub struct RecoveryMarkers<'w> {
    room: Option<bevy::prelude::Res<'w, crate::state::RoomRecordRecovery>>,
    avatar: Option<bevy::prelude::Res<'w, crate::state::AvatarRecordRecovery>>,
    inventory: Option<bevy::prelude::Res<'w, crate::state::InventoryRecordRecovery>>,
}

impl RecoveryMarkers<'_> {
    /// Why publishing `record` right now would overwrite an unread stored
    /// copy, or `None` when its fetch landed cleanly.
    pub fn reason(&self, record: RecordKind) -> Option<&str> {
        match record {
            RecordKind::Room => self.room.as_deref().map(|r| r.reason.as_str()),
            RecordKind::Avatar => self.avatar.as_deref().map(|r| r.reason.as_str()),
            RecordKind::Inventory => self.inventory.as_deref().map(|r| r.reason.as_str()),
        }
    }

    /// The reasons in `[room, avatar, inventory]` order, for the pure
    /// decision functions that must stay testable without an ECS.
    pub fn reasons(&self) -> [Option<&str>; 3] {
        [
            self.reason(RecordKind::Room),
            self.reason(RecordKind::Avatar),
            self.reason(RecordKind::Inventory),
        ]
    }
}

/// The user-facing noun for a record kind in recovery copy.
fn recovery_noun(record: RecordKind) -> &'static str {
    match record {
        RecordKind::Room => "world",
        RecordKind::Avatar => "avatar",
        RecordKind::Inventory => "inventory",
    }
}

/// The sentence every recovery-gated publish shows before it proceeds:
/// what loaded instead, why, and what saving now does to the stored copy.
pub fn overwrite_warning(record: RecordKind, reason: &str) -> String {
    let loaded_as = match record {
        RecordKind::Inventory => "an empty default",
        RecordKind::Room | RecordKind::Avatar => "the default",
    };
    format!(
        "Your {} loaded as {loaded_as} because the stored copy could not be read ({reason}). \
         Saving now replaces whatever is stored on your PDS with what you see here.",
        recovery_noun(record)
    )
}

/// Ask before a recovery-gated publish, in the words every editor uses.
/// The caller publishes when the confirm's `show` yields `Some` — and
/// leaves the marker alone: the poll system retires it on success.
pub fn request_overwrite_confirm(
    confirm: &mut crate::ui::confirm::ConfirmState<()>,
    record: RecordKind,
    reason: &str,
) {
    confirm.request(
        format!("Overwrite your stored {}?", recovery_noun(record)),
        overwrite_warning(record, reason),
        "Save anyway",
        (),
    );
}

/// Hover text for a publish control that is disabled because it would
/// write over unread stored copies — one line per blocked record, then
/// where the confirmed overwrite lives.
pub fn publish_blocked_hover(blocked: &[(RecordKind, &str)]) -> String {
    let mut lines: Vec<String> = blocked
        .iter()
        .map(|(record, reason)| {
            format!(
                "Your {} could not be loaded ({reason}); saving it from here would overwrite \
                 the stored copy unread.",
                recovery_noun(*record)
            )
        })
        .collect();
    lines.push(String::from(
        "Open its editor to save deliberately — that Save asks first.",
    ));
    lines.join("\n")
}

/// How the status line is coloured — a tone, so the wording can be decided
/// (and tested) without an egui context.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatusTone {
    Ok,
    Warn,
    Error,
    Weak,
}

/// Seconds a save may be in flight before the status line stops saying
/// merely "Saving…" and starts saying how long, and when it gives up. The
/// login gate (#849) uses the same threshold for the same class of PDS
/// round trip.
const SAVE_SLOW_SECS: f64 = 15.0;

/// The status line's words and tone (#1206). `dirty` qualifies a
/// success: an edit made while the save was in flight leaves the row dirty
/// (`stored` is pinned to what was WRITTEN, #1116), and "✔ Saved" beside a
/// green Save button said yes to "did my work land?" when the answer was
/// "partly". A save in flight counts up, and past [`SAVE_SLOW_SECS`] names
/// the deadline it will give up at — a bare "Saving…" that could sit for a
/// minute taught the owner that quiet means hung.
pub fn status_line_text(
    status: &PublishStatus,
    now_secs: f64,
    dirty: bool,
) -> Option<(StatusTone, String)> {
    let ago = |at: f64| (now_secs - at).max(0.0);
    match status {
        PublishStatus::Idle => None,
        PublishStatus::Publishing { since_secs } => {
            let elapsed = ago(*since_secs);
            if elapsed >= SAVE_SLOW_SECS {
                Some((
                    StatusTone::Error,
                    format!(
                        "⟳ Saving to PDS… ({elapsed:.0}s) — still trying; gives up at {}s",
                        crate::config::http::PUBLISH_TASK_DEADLINE.as_secs()
                    ),
                ))
            } else {
                Some((
                    StatusTone::Warn,
                    format!("⟳ Saving to PDS… ({elapsed:.0}s)"),
                ))
            }
        }
        PublishStatus::Success { at_secs } if dirty => Some((
            StatusTone::Weak,
            format!(
                "{} Saved ({:.0}s ago) — edited since",
                crate::ui::affordances::CHECK,
                ago(*at_secs)
            ),
        )),
        PublishStatus::Success { at_secs } => Some((
            StatusTone::Ok,
            format!(
                "{} Saved ({:.0}s ago)",
                crate::ui::affordances::CHECK,
                ago(*at_secs)
            ),
        )),
        PublishStatus::Failed {
            at_secs, message, ..
        } => Some((
            StatusTone::Error,
            format!(
                "{} Save failed ({:.0}s ago): {message}",
                crate::ui::affordances::CROSS,
                ago(*at_secs)
            ),
        )),
    }
}

/// Render the uniform publish status line. `Idle` draws nothing; every
/// other state is a single coloured line, and **both** Success and
/// Failed carry the same live `(Ns ago)` counter (Avatar used to drop
/// it). Wording is identical across editors — the editor window's own
/// title already says *which* record, so the line stays terse. `dirty` is
/// the row's own derived flag, see [`status_line_text`].
pub fn publish_status_line(ui: &mut egui::Ui, status: &PublishStatus, now_secs: f64, dirty: bool) {
    let Some((tone, text)) = status_line_text(status, now_secs, dirty) else {
        return;
    };
    let theme = crate::ui::theme::current(ui.ctx());
    let color = match tone {
        StatusTone::Ok => theme.status.ok,
        StatusTone::Warn => theme.status.warn,
        StatusTone::Error => theme.status.error,
        StatusTone::Weak => theme.text_weak,
    };
    ui.colored_label(color, text);
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
        // Its OWN bound (#1206): borrowing `timed_out` reported the inner
        // 30 s a full minute after the click.
        return Some(Err(crate::config::http::timed_out_after(
            label,
            crate::config::http::PUBLISH_TASK_DEADLINE,
        )));
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
        // #1206, finding 209: the outer deadline reports ITS number. It
        // borrowed `timed_out` and said "after 30s" a full minute in.
        assert!(
            message.contains(&format!(
                "after {}s",
                crate::config::http::PUBLISH_TASK_DEADLINE.as_secs()
            )),
            "{message}"
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{AvatarRecord, RoomRecord};
    use crate::state::PublishFeedback;
    use crate::ui::toast::{ToastKind, Toasts};
    use crate::ui::toolbar::UiPanels;

    /// #1200 (finding 128): "Reset to default" on the inventory empties the
    /// stash, and the shared copy talked about "the whole record" and "its
    /// generated default" — words written for the room. The inventory's
    /// wording must say what goes, and how many.
    #[test]
    fn the_inventory_reset_says_it_empties_the_stash() {
        let (title, body, button) = ResetWording::EmptyStash { items: 40 }.confirm();
        assert_eq!(title, "Empty your inventory?");
        assert!(body.contains("all 40 items"));
        assert!(body.contains("no undo"));
        assert_eq!(button, "Empty inventory");
        assert!(
            ResetWording::EmptyStash { items: 1 }
                .hover(false)
                .contains("all 1 item.")
        );
        // #1209, finding 202: with an undo stack behind it the hover says
        // so, like the Revert beside it; behind a confirm it does not
        // promise an undo that does not exist.
        assert!(ResetWording::Record.hover(true).contains("Undo (Ctrl+Z)"));
        assert!(!ResetWording::Record.hover(false).contains("Undo"));
        let (title, body, _) = ResetWording::Record.confirm();
        assert_eq!(title, "Reset to default?");
        assert!(body.contains("generated default"));
    }

    /// #1208, finding 72 (the hard-ceiling half). Sequence: the record
    /// grows past the ceiling, the owner presses Ctrl+S. The row took the
    /// request and its `enabled` gate discarded it — the same silence as a
    /// collapsed window. The gate now names its reason, and the reason is
    /// the one the disabled button's hover shows.
    #[test]
    fn a_refused_ctrl_s_names_the_gate_that_refused_it() {
        let fine = SizeReadout {
            bytes: Some(1_000),
            largest: Some(String::from("room manifest")),
            unserializable: None,
        };
        let over = SizeReadout {
            bytes: Some(HARD_RECORD_CEILING_BYTES + 1),
            largest: Some(String::from("room generator \"oak_grove\"")),
            unserializable: None,
        };
        assert_eq!(
            save_refusal(true, true, &fine, false, &PublishStatus::Idle),
            None
        );
        let too_big = save_refusal(true, true, &over, false, &PublishStatus::Idle)
            .expect("over the ceiling refuses");
        assert!(too_big.contains("past the"));
        assert!(
            too_big.contains("oak_grove"),
            "the refusal names the offender (#1207): {too_big}"
        );
        let busy =
            save_refusal(true, true, &fine, true, &PublishStatus::Idle).expect("in flight refuses");
        assert!(busy.contains("already"));
        // Priority: an in-flight save is the more immediate fact.
        assert_eq!(
            save_refusal(true, true, &over, true, &PublishStatus::Idle),
            Some(busy)
        );
        assert!(
            save_refusal(false, true, &fine, false, &PublishStatus::Idle)
                .expect("clean refuses")
                .contains("nothing to save")
        );
        assert!(ctrl_s_refused(&too_big).starts_with("Ctrl+S did not save: "));
    }

    /// #1207, findings 122 and 204. Sequence: a gift from a newer build
    /// lands in the stash (or a world holds a generator this build cannot
    /// decode); the row showed either no readout and an enabled Save that
    /// failed on the click, or — with the item in both live and stored —
    /// a Save that never lit up, with no reason anywhere. The readout now
    /// says "can't be saved", the Save button carries the sentence, and a
    /// Ctrl+S is refused with the same sentence, before any I/O.
    #[test]
    fn an_unserializable_record_refuses_save_with_the_newer_build_sentence() {
        let mut size = SizeReadout::default();
        size.consider(&serde_json::json!({"ok": true}), "inventory item \"lamp\"");
        // `serde_json::to_vec` of a map with a non-string key is the one
        // failure serde_json produces on its own; the real case is the
        // `skip_serializing` Unknown arm, whose error text contains
        // "cannot be serialized" — model that text directly.
        size.refuse(crate::pds::record_size::unserializable_reason(
            "inventory item \"gift\"",
            "unknown variant cannot be serialized",
        ));
        // Clean AND cannot serialize: the refusal is the session fact, not
        // "nothing to save".
        let reason =
            save_refusal(false, true, &size, false, &PublishStatus::Idle).expect("refused");
        assert!(reason.contains("newer version of Overlands"), "{reason}");
        assert!(reason.contains("inventory item \"gift\""), "{reason}");
    }

    /// #1206, finding 201. Sequence: press Save, change your mind, click
    /// "Revert to saved" while the button reads "Saving…". Revert was
    /// gated on `dirty` alone, restored the PRE-save snapshot, and the
    /// landing publish then pinned `stored` to what it wrote — the row lit
    /// up dirty again holding the edits just discarded, and the PDS held
    /// them. Both replacements stand down while a save is in flight.
    #[test]
    fn revert_and_reset_stand_down_while_a_save_is_in_flight() {
        assert_eq!(revert_refusal(true, false), None);
        assert!(
            revert_refusal(true, true)
                .expect("in flight refuses")
                .contains("Wait for the save")
        );
        assert!(revert_refusal(false, false).is_some());
        assert_eq!(reset_refusal(true, false), None);
        assert!(reset_refusal(true, true).is_some());
        assert_eq!(reset_refusal(false, false), Some("Already the default"));
    }

    /// #1206, findings 205 and 277. Sequence: keep editing while "Saving…"
    /// is up; the save lands. The line said "✔ Saved (0s ago)" in green
    /// beside a green, dirty Save button — an unqualified yes to "did my
    /// work land?" when the answer was "partly". And while in flight it
    /// showed no elapsed time against a 60 s deadline.
    #[test]
    fn the_status_line_qualifies_a_save_the_owner_edited_past_and_counts_a_slow_one() {
        let landed = PublishStatus::Success { at_secs: 100.0 };
        let (tone, text) = status_line_text(&landed, 103.0, true).expect("drawn");
        assert_eq!(tone, StatusTone::Weak);
        assert!(text.contains("Saved (3s ago) — edited since"), "{text}");
        let (tone, text) = status_line_text(&landed, 103.0, false).expect("drawn");
        assert_eq!(tone, StatusTone::Ok);
        assert!(!text.contains("edited since"));

        let in_flight = PublishStatus::Publishing { since_secs: 100.0 };
        let (tone, text) = status_line_text(&in_flight, 104.0, true).expect("drawn");
        assert_eq!(tone, StatusTone::Warn);
        assert!(text.contains("(4s)"), "{text}");
        let (tone, text) = status_line_text(&in_flight, 120.0, true).expect("drawn");
        assert_eq!(tone, StatusTone::Error);
        assert!(text.contains("(20s)"), "{text}");
        assert!(
            text.contains(&format!(
                "gives up at {}s",
                crate::config::http::PUBLISH_TASK_DEADLINE.as_secs()
            )),
            "{text}"
        );
        assert!(status_line_text(&PublishStatus::Idle, 0.0, true).is_none());
    }

    /// #1207, finding 203. One sentence used to say "the whole record for
    /// Room/Avatar" — false for both. Each record says what its number
    /// measures.
    #[test]
    fn the_size_hover_says_what_each_record_measures() {
        assert!(size_measures(RecordKind::Room).contains("manifest"));
        assert!(size_measures(RecordKind::Room).contains("generator"));
        assert!(size_measures(RecordKind::Avatar).contains("worn"));
        assert!(size_measures(RecordKind::Inventory).contains("item"));
    }

    /// #1199: the three editors used to word the same warning three ways,
    /// and the room had none. One sentence, naming the record, the reason
    /// and the consequence.
    #[test]
    fn the_overwrite_warning_names_record_reason_and_consequence() {
        let text = overwrite_warning(RecordKind::Room, "decode error");
        assert!(text.contains("world"));
        assert!(text.contains("decode error"));
        assert!(text.contains("replaces whatever is stored"));
        assert!(overwrite_warning(RecordKind::Inventory, "x").contains("empty default"));
        let hover = publish_blocked_hover(&[(RecordKind::Avatar, "timed out")]);
        assert!(hover.contains("avatar"));
        assert!(hover.contains("timed out"));
        assert!(hover.contains("asks first"));
    }

    /// #1137. Sequence: press Ctrl+S in the Avatar editor, then Esc to
    /// close the window — the request TTL lets the save proceed with the
    /// window shut, and the 30 s request timeout means the answer can be
    /// half a minute away. (Same shape via the unsaved guard's "Continue in
    /// background", and via a publish fired just before a portal hop.) The
    /// failure used to land as `warn!` + a `PublishStatus::Failed` that only
    /// `publish_status_line` renders — i.e. in the footer of a window
    /// nobody has open. The user believes the save landed; the next guard
    /// prompt is the first hint, by which time the dirty diff is large.
    #[test]
    fn a_failed_avatar_save_reaches_a_user_who_closed_the_window() {
        let mut log = SessionLog::default();
        let mut feedback = PublishFeedback::<AvatarRecord>::default();
        let mut toasts = Toasts::default();
        // Default is every window shut, which is the state the sequence
        // above leaves behind — the failure has to reach the user anyway.
        let mut panels = UiPanels::default();

        report_publish_failure(
            RecordKind::Avatar,
            WriteOp::Save,
            String::from("did:plc:alice"),
            String::from("502 Bad Gateway"),
            12.0,
            FailureSinks {
                session_log: &mut log,
                feedback: &mut feedback,
                toasts: &mut toasts,
                panels: &mut panels,
            },
        );

        assert!(
            panels.avatar,
            "the window carrying the status line and the retry has to be on screen"
        );
        let shown = toasts.shown();
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].0, ToastKind::Error);
        assert_eq!(shown[0].1, "Couldn't save your avatar — 502 Bad Gateway");
        assert!(matches!(feedback.status, PublishStatus::Failed { .. },));
    }

    /// THE SEQUENCE (#1214 f407): the refresh token dies mid-session, the
    /// owner presses Ctrl+S, and the world editor pops back open with a red
    /// "Save failed" line and a Save button — so they press it again, and
    /// again. Every failure looked retryable, because the only shape the UI
    /// had to branch on was the raw string `refresh: …`. A terminal failure
    /// must not re-open the window onto the button that cannot work, and
    /// must name the one action that can.
    #[test]
    fn an_expired_session_does_not_point_the_owner_back_at_the_retry() {
        let mut log = SessionLog::default();
        let mut feedback = PublishFeedback::<RoomRecord>::default();
        let mut toasts = Toasts::default();
        let mut panels = UiPanels::default();

        report_publish_failure(
            RecordKind::Room,
            WriteOp::Save,
            String::from("did:plc:alice"),
            // The exact shape `oauth_post_with_refresh` propagates when the
            // PDS rejects the refresh token itself.
            String::from("refresh: OAuth server error: invalid_grant - refresh token revoked"),
            12.0,
            FailureSinks {
                session_log: &mut log,
                feedback: &mut feedback,
                toasts: &mut toasts,
                panels: &mut panels,
            },
        );

        assert!(
            !panels.world_editor,
            "the auto-open is good for a 5xx and worse than useless here — \
             its only offered action is the save that cannot succeed"
        );
        let shown = toasts.shown();
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].0, ToastKind::Error);
        assert!(
            shown[0].1.contains("sign in again"),
            "the toast has to name the only thing that works: {}",
            shown[0].1
        );
        assert!(
            !shown[0].1.contains("invalid_grant"),
            "the owner read a raw Rust error chain as their primary feedback: {}",
            shown[0].1
        );
        // The raw chain still reaches the durable record — the analyzer and
        // a bug report both need the shape the PDS actually returned.
        assert!(
            log.iter().any(|e| matches!(
                &e.payload,
                EventPayload::RecordWriteFailed { reason, .. } if reason.contains("invalid_grant")
            )),
            "the session log keeps the raw chain"
        );

        let PublishStatus::Failed {
            terminal, message, ..
        } = &feedback.status
        else {
            panic!("a failure is recorded");
        };
        assert!(*terminal);
        assert!(message.contains("sign in again"), "{message}");

        // …and the Save button the owner would press next is refused, with
        // the same fact in the same words the toast used.
        let refusal = save_refusal(true, true, &SizeReadout::default(), false, &feedback.status)
            .expect("an expired session refuses the save");
        assert!(refusal.contains("sign in again"), "{refusal}");
    }

    /// The other direction, and the one that matters more: a transient
    /// failure MUST stay retryable. Calling a timeout or a 5xx terminal
    /// would disable Save on an owner whose very next click would have
    /// worked — a worse bug than the one being fixed.
    #[test]
    fn a_transient_failure_stays_retryable() {
        for error in [
            "refresh: fetch error: operation timed out",
            "502 Bad Gateway",
            "applyWrites failed: 500 Internal Server Error",
        ] {
            let mut log = SessionLog::default();
            let mut feedback = PublishFeedback::<RoomRecord>::default();
            let mut toasts = Toasts::default();
            let mut panels = UiPanels::default();
            report_publish_failure(
                RecordKind::Room,
                WriteOp::Save,
                String::from("did:plc:alice"),
                String::from(error),
                1.0,
                FailureSinks {
                    session_log: &mut log,
                    feedback: &mut feedback,
                    toasts: &mut toasts,
                    panels: &mut panels,
                },
            );
            assert!(panels.world_editor, "{error} must still offer the retry");
            assert!(
                matches!(
                    feedback.status,
                    PublishStatus::Failed {
                        terminal: false,
                        ..
                    }
                ),
                "{error} is not a dead session"
            );
            assert_eq!(
                save_refusal(true, true, &SizeReadout::default(), false, &feedback.status),
                None,
                "{error} must leave Save enabled"
            );
        }
    }

    /// The room's Reset-to-default is a delete-then-put, and a failure
    /// there is not a failed save. Saying "couldn't save" over a reset
    /// would send the user looking for edits they never made.
    #[test]
    fn a_failed_room_reset_says_reset() {
        let mut log = SessionLog::default();
        let mut feedback = PublishFeedback::<RoomRecord>::default();
        let mut toasts = Toasts::default();
        let mut panels = UiPanels::default();

        report_publish_failure(
            RecordKind::Room,
            WriteOp::Reset,
            String::from("did:plc:alice"),
            String::from("timed out"),
            3.0,
            FailureSinks {
                session_log: &mut log,
                feedback: &mut feedback,
                toasts: &mut toasts,
                panels: &mut panels,
            },
        );

        assert!(panels.world_editor);
        assert_eq!(toasts.shown()[0].1, "Couldn't reset your world — timed out");
    }

    /// The three editors report identically — that is the whole point of
    /// the shared helper. Inventory had the toast + auto-open since
    /// #843(e); Room and Avatar reached #1137 without it, so the same
    /// failure surfaced three different ways from one shared row.
    #[test]
    fn every_record_kind_toasts_and_opens_its_own_window() {
        type PanelProbe = fn(&UiPanels) -> bool;
        let opens: [(RecordKind, &str, PanelProbe); 3] = [
            (RecordKind::Room, "Couldn't save your world — nope", |p| {
                p.world_editor
            }),
            (
                RecordKind::Avatar,
                "Couldn't save your avatar — nope",
                |p| p.avatar,
            ),
            (
                RecordKind::Inventory,
                "Couldn't save your inventory — nope",
                |p| p.inventory,
            ),
        ];
        for (record, expected, opened) in opens {
            let mut log = SessionLog::default();
            let mut feedback = PublishFeedback::<RoomRecord>::default();
            let mut toasts = Toasts::default();
            let mut panels = UiPanels::default();
            report_publish_failure(
                record,
                WriteOp::Save,
                String::from("did:plc:alice"),
                String::from("nope"),
                1.0,
                FailureSinks {
                    session_log: &mut log,
                    feedback: &mut feedback,
                    toasts: &mut toasts,
                    panels: &mut panels,
                },
            );
            assert_eq!(toasts.shown()[0].1, expected);
            assert!(opened(&panels), "{record:?} must open its own window");
            assert_eq!(log.len(), 1, "and the failure is in the session log");
        }
    }
}
