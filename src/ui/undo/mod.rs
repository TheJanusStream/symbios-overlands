//! Bounded undo/redo history for the world & avatar editors (#817).
//!
//! Design (#862): a ring of **whole-record clones**, not deltas — every
//! editor commit is path-addressed (`PlacementMarker` index,
//! `PrimMarker { generator_ref, path }`), so an index shift from a
//! structural edit would silently retarget a delta. A `Clone` of the
//! in-memory record is strictly cheaper than the `serde_json::to_value`
//! the open editor pays for its derived dirty check.
//!
//! # The depth is not the bound; the bytes are (#1270 f417)
//!
//! This paragraph used to argue that "typical records sit under the
//! 100 KiB publish soft budget, so a
//! [`crate::config::ui::editor::UNDO_DEPTH`]-entry ring is a few MiB
//! worst case". That reasoning was wrong, and
//! [`crate::pds::record_size`] spends thirty lines on the same mistake:
//! `SOFT_RECORD_BUDGET_BYTES` measures the largest SINGLE PUBLISHED
//! record after the manifest/child split (#697), not the assembled
//! in-memory room this ring stores. GothicHorror's seeded default is
//! 348.6 KiB assembled against 53.9 KiB largest published — a factor of
//! six, before the owner authors anything, and in memory it is larger
//! still (`String`s, `Vec`s, a `HashMap`, boxed enum payloads). A room
//! may hold 256 generators of up to 1024 nodes each, plus 16 KiB of
//! L-system and 16 KiB of shape source per generator.
//!
//! So the ring is bounded by BYTES as well as by depth
//! ([`crate::config::ui::editor::UNDO_RING_BUDGET_BYTES`]): every entry
//! is measured on push and the oldest are evicted until the total fits,
//! independently of `UNDO_DEPTH`. On an ordinary room nothing changes —
//! 33 entries sit well inside the budget. On a big one the history gets
//! SHORTER, which is the degradation this wanted: the failure mode it
//! replaces is a browser tab OOM, and a tab that dies takes with it
//! exactly the unsaved edits undo exists to protect. `depth_note` puts
//! the shortened depth on the Undo button, so a ring that has been
//! trimmed says so rather than just running out early.
//!
//! Capture rides the records' existing commit contract instead of
//! instrumenting every widget: the editors flush widget bursts into a
//! single debounced `set_changed()` (one tick per edit burst — a slider
//! scrub coalesces for free), and the discrete writers (gizmo drag
//! commit, scene context menu, viewport drag-drop, seed re-roll,
//! Load/Reset, raw-JSON parse) each produce exactly one tick. The
//! capture systems observe those ticks in `PostUpdate` and push one
//! entry per tick.
//!
//! Two kinds of non-edit writes share that change tick and must NOT
//! become entries:
//! - **Foreign wholesale replacements** — portal travel
//!   (`player::portal`) and the inbound owner `RoomStateUpdate`
//!   (`network::inbound`) swap the record's contents in place. Undoing
//!   across one would restore a different room (or fight a concurrent
//!   same-DID session), so they raise [`RoomWriteSignals::foreign`] and
//!   the history resets to a fresh baseline. Travel is additionally
//!   caught by the room-DID identity key, signal or not.
//! - **Derived side-effects** — terrain lot auto-population
//!   (`terrain::lots`) rewrites generators + placements as a
//!   *consequence* of an edit. It raises [`RoomWriteSignals::derived`]
//!   and the history folds the write into the current entry instead of
//!   minting a phantom "edit".
//!
//! Restores themselves (#863) also tick the record; [`UndoHistory::undo`]
//! / [`redo`](UndoHistory::redo) arm a one-shot suppression so the
//! capture system swallows that tick instead of re-recording it.
//!
//! Lifetime (decisions 2026-07-18): the room history clears on portal
//! travel, on a foreign overwrite, and on logout; the avatar history
//! survives travel (the avatar record does too) and clears on logout.
//! Nothing persists to disk.

use std::collections::VecDeque;

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::pds::{AvatarRecord, RoomRecord};
use crate::state::{CurrentRoomDid, LiveAvatarRecord, LiveRoomRecord};

use super::avatar::AvatarEditorState;
use super::room::{GenNodeId, RoomEditorState};

pub mod restore;
pub mod trigger;

pub use restore::{StepKind, step_avatar, step_room};
pub use trigger::{UndoShortcut, apply_undo_shortcut, undo_redo_buttons};

/// Fallback entry label until the mutation sites report richer ones
/// (#865). Toasts render it as "Undid: edit".
const GENERIC_LABEL: &str = "edit";

/// The World Editor selection state a restore must re-seed (#863):
/// placement index, generator key, prim path, and the tree-view widget's
/// own selection. All of it is index/path-addressed, so after a
/// wholesale record replace it either dangles or silently points at the
/// wrong node — each entry therefore carries the selection that was
/// live when the entry was captured.
#[derive(Clone, Default)]
pub struct RoomSelection {
    pub generator: Option<String>,
    pub placement: Option<usize>,
    pub prim_path: Option<Vec<usize>>,
    /// The `egui_ltreeview` widget's selected ids (kept alongside the
    /// field mirror above the same way `tree.rs::sync_selection_fields`
    /// keeps them in lockstep).
    pub tree: Vec<GenNodeId>,
}

/// Avatar-editor counterpart of [`RoomSelection`] — the visuals tree is
/// single-root and has no placements, so only the generator/path pair
/// and the tree-view selection exist.
#[derive(Clone, Default)]
pub struct AvatarSelection {
    pub generator: Option<String>,
    pub prim_path: Option<Vec<usize>>,
    pub tree: Vec<GenNodeId>,
}

/// One captured state: the whole record, the selection that accompanied
/// it, and a short human label for the edit that *produced* it
/// ("delete of oak_3 + 12 placements"). The baseline entry seeded on
/// load/travel carries no meaningful label — it is never undone *past*.
#[derive(Clone)]
pub struct UndoEntry<R, S> {
    pub record: R,
    pub selection: S,
    pub label: String,
    /// Serialized size of `record`, measured once on push (#1270 f417).
    /// A lower bound on what the entry costs in memory — the in-memory
    /// form carries pointers, capacity slack and a `HashMap`'s table on
    /// top — which is the right direction for a budget to be wrong in.
    /// `None` when the record does not serialize, which no record type
    /// can practically hit; such an entry counts as free rather than as
    /// infinite, so an unmeasurable record cannot empty the ring.
    pub bytes: Option<usize>,
}

/// What the capture system saw a record tick mean. Computed from the
/// write-signal resources; see the module docs for why each arm exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
    /// A user edit: push a new entry (dropping any redo branch).
    Edit(String),
    /// A wholesale replacement this user did not author (portal travel,
    /// remote owner broadcast): reset to a fresh baseline.
    Foreign,
    /// A derived side-effect of the previous edit (lot auto-population):
    /// fold into the current entry instead of minting a new one.
    Derived,
}

/// The bounded undo/redo ring for one record type. `entries[cursor]` is
/// always the record's current state once seeded; undo moves the cursor
/// back, redo forward, and a fresh edit truncates everything after the
/// cursor (the standard linear-history model).
#[derive(Resource)]
pub struct UndoHistory<R, S>
where
    R: Send + Sync + 'static,
    S: Send + Sync + 'static,
{
    entries: VecDeque<UndoEntry<R, S>>,
    cursor: usize,
    /// Identity of the content the ring describes — the room DID for the
    /// room history (`None` for the avatar's, whose identity is the
    /// session itself). A key mismatch on observation means the record
    /// was wholesale-swapped to different content (portal travel), so
    /// the ring resets even if no explicit signal fired.
    key: Option<String>,
    /// One-shot: the next observed tick is an internal restore write
    /// (#863) — consume it instead of recording it.
    suppress_capture: bool,
    /// Sum of every entry's `bytes` (#1270 f417). Maintained
    /// incrementally, so the budget check is an integer compare and never
    /// a walk of the ring.
    total_bytes: usize,
    /// How many entries the byte budget has evicted over this ring's
    /// life. Non-zero means the depth on offer is shorter than
    /// `UNDO_DEPTH`, which is what [`Self::depth_note`] tells the owner.
    evicted_for_size: usize,
}

impl<R, S> Default for UndoHistory<R, S>
where
    R: Send + Sync + 'static,
    S: Send + Sync + 'static,
{
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            cursor: 0,
            key: None,
            suppress_capture: false,
            total_bytes: 0,
            evicted_for_size: 0,
        }
    }
}

pub type RoomUndoHistory = UndoHistory<RoomRecord, RoomSelection>;
pub type AvatarUndoHistory = UndoHistory<AvatarRecord, AvatarSelection>;

impl<R, S> UndoHistory<R, S>
where
    R: serde::Serialize + Send + Sync + 'static,
    S: Send + Sync + 'static,
{
    /// Route one observed record tick into the ring. `record` /
    /// `selection` are closures so the (up to ~100 KiB) clone only
    /// happens on the arms that store it.
    pub fn observe(
        &mut self,
        key: Option<&str>,
        obs: Observation,
        record: impl FnOnce() -> R,
        selection: impl FnOnce() -> S,
    ) {
        // Identity change = different content entirely (portal travel
        // landed a new room). Reset regardless of what the signals say.
        if self.key.as_deref() != key || self.entries.is_empty() {
            self.reset(key, record(), selection());
            return;
        }
        match obs {
            Observation::Foreign => self.reset(key, record(), selection()),
            _ if std::mem::take(&mut self.suppress_capture) => {
                // Internal restore write (#863): the cursor already moved
                // in `undo()`/`redo()`; `entries[cursor]` IS this state.
            }
            Observation::Derived => {
                // Fold the side-effect into the state it derived from, so
                // undoing the triggering edit also undoes its fallout and
                // redo replays both as one step.
                if let Some(entry) = self.entries.get_mut(self.cursor) {
                    let record = record();
                    let bytes = crate::pds::record_size::serialized_record_bytes(&record);
                    // The entry is REPLACED, so its old size leaves the
                    // total with it (#1270 f417). Lot auto-population can
                    // add hundreds of generators, so a fold that only ever
                    // added would drift the total upward without bound.
                    self.total_bytes -= entry.bytes.unwrap_or(0);
                    self.total_bytes += bytes.unwrap_or(0);
                    entry.record = record;
                    entry.bytes = bytes;
                }
                self.evict();
            }
            Observation::Edit(label) => self.push(record(), selection(), label),
        }
    }

    /// Drop everything and seed a fresh baseline for `key`'s content.
    pub fn reset(&mut self, key: Option<&str>, record: R, selection: S) {
        self.entries.clear();
        self.total_bytes = 0;
        self.evicted_for_size = 0;
        let bytes = crate::pds::record_size::serialized_record_bytes(&record);
        self.total_bytes += bytes.unwrap_or(0);
        self.entries.push_back(UndoEntry {
            record,
            selection,
            label: GENERIC_LABEL.to_string(),
            bytes,
        });
        self.cursor = 0;
        self.key = key.map(str::to_owned);
        self.suppress_capture = false;
    }

    /// Forget everything (logout). The next observation re-seeds.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.cursor = 0;
        self.key = None;
        self.suppress_capture = false;
        self.total_bytes = 0;
        self.evicted_for_size = 0;
    }

    fn push(&mut self, record: R, selection: S, label: String) {
        // A fresh edit invalidates the redo branch (linear history).
        for dropped in self.entries.drain(self.cursor + 1..) {
            self.total_bytes -= dropped.bytes.unwrap_or(0);
        }
        // Measured here and nowhere else (#1270 f417): a push happens at
        // most once per debounced edit burst — a quarter of a second
        // apart at the very fastest — and it already deep-clones the
        // whole record, so a streamed byte count beside that clone is the
        // same order of cost. `serialized_record_bytes` counts into a
        // sink rather than building the JSON, so this allocates nothing.
        let bytes = crate::pds::record_size::serialized_record_bytes(&record);
        self.total_bytes += bytes.unwrap_or(0);
        self.entries.push_back(UndoEntry {
            record,
            selection,
            label,
            bytes,
        });
        self.cursor += 1;
        self.evict();
    }

    /// Trim the ring to both bounds: at most `UNDO_DEPTH` undoable steps
    /// beyond the baseline, and at most `UNDO_RING_BUDGET_BYTES` of
    /// serialized record (#1270 f417).
    ///
    /// The floor is what makes the byte bound safe. `entries[cursor]` is
    /// the record's CURRENT state and must never be evicted, and a ring
    /// offering zero undoable steps would be a silent removal of the
    /// feature — so a single record larger than the whole budget keeps
    /// `MIN_UNDO_DEPTH` steps and reports the shortfall through
    /// [`Self::depth_note`] rather than emptying itself.
    fn evict(&mut self) {
        use crate::config::ui::editor::{MIN_UNDO_DEPTH, UNDO_DEPTH, UNDO_RING_BUDGET_BYTES};
        // Both loops are gated on the CURSOR, not on the length. The
        // front entry is only droppable while something ahead of it is
        // the current state — after an undo the cursor sits behind a redo
        // branch, so `entries.len()` can be large while `entries[0]` IS
        // what the record currently holds. Evicting that would swap the
        // world out from under the owner.
        while self.cursor > 0 && self.entries.len() > UNDO_DEPTH + 1 {
            self.drop_oldest(false);
        }
        while self.cursor > MIN_UNDO_DEPTH && self.total_bytes > UNDO_RING_BUDGET_BYTES {
            self.drop_oldest(true);
        }
    }

    /// Pop the front entry, keeping `total_bytes` and `cursor` honest.
    /// Only ever called with `cursor > 0` — see [`Self::evict`].
    fn drop_oldest(&mut self, for_size: bool) {
        if let Some(dropped) = self.entries.pop_front() {
            self.total_bytes -= dropped.bytes.unwrap_or(0);
            self.cursor -= 1;
            if for_size {
                self.evicted_for_size += 1;
            }
        }
    }

    /// Serialized bytes the ring is currently holding (#1270 f417).
    /// Diagnostics and the guards; never rendered as a number.
    pub fn bytes(&self) -> usize {
        self.total_bytes
    }

    /// A sentence for the Undo button's hover when the byte budget has
    /// shortened the history, or `None` when it has not (#1270 f417).
    ///
    /// Silence is the common case and the right one: on an ordinary room
    /// the full depth is available and there is nothing to say. It speaks
    /// only once the ring has actually been trimmed, because "your undo
    /// history is shorter than usual" is the thing an owner needs to know
    /// BEFORE reaching the end of it.
    pub fn depth_note(&self) -> Option<String> {
        (self.evicted_for_size > 0).then(|| {
            format!(
                "This world is large, so the history is holding {} step{} instead of {}.",
                self.cursor,
                if self.cursor == 1 { "" } else { "s" },
                crate::config::ui::editor::UNDO_DEPTH,
            )
        })
    }

    /// Step back one entry. Returns `(entry to restore, label of the
    /// edit being undone)` — the label belongs to the entry we moved
    /// *off*, which is what "Undid: …" should name. Arms the one-shot
    /// capture suppression: the caller MUST write `entry.record` into
    /// the live resource (via the bypass + single-flush contract, #863)
    /// or the next real edit's tick gets swallowed.
    pub fn undo(&mut self) -> Option<(&UndoEntry<R, S>, &str)> {
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        self.suppress_capture = true;
        let label = self.entries[self.cursor + 1].label.as_str();
        Some((&self.entries[self.cursor], label))
    }

    /// Step forward one entry. Returns `(entry to restore, its label)` —
    /// a redo re-applies the entry it moves onto, so the toast names
    /// that entry. Same must-apply contract as [`undo`](Self::undo).
    pub fn redo(&mut self) -> Option<(&UndoEntry<R, S>, &str)> {
        if self.cursor + 1 >= self.entries.len() {
            return None;
        }
        self.cursor += 1;
        self.suppress_capture = true;
        let entry = &self.entries[self.cursor];
        Some((entry, entry.label.as_str()))
    }

    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_redo(&self) -> bool {
        self.cursor + 1 < self.entries.len()
    }

    /// Label of the edit an undo would revert (button tooltips, #864).
    pub fn undo_label(&self) -> Option<&str> {
        self.can_undo()
            .then(|| self.entries[self.cursor].label.as_str())
    }

    /// Label of the edit a redo would re-apply.
    pub fn redo_label(&self) -> Option<&str> {
        self.entries.get(self.cursor + 1).map(|e| e.label.as_str())
    }

    /// Number of stored entries, baseline included. Diagnostics only.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Raised by the non-editor writers of [`LiveRoomRecord`] in the same
/// system (and frame) that writes the record, consumed by
/// [`capture_room_history`] when it observes the corresponding tick.
/// The avatar record has no non-editor writers, so no counterpart
/// exists for it.
#[derive(Resource, Default)]
pub struct RoomWriteSignals {
    /// Portal travel / inbound owner broadcast — clear the history.
    pub foreign: bool,
    /// Lot auto-population — fold into the current entry.
    pub derived: bool,
}

/// Latest-wins label slots for the next captured entry, one per record
/// so simultaneous room + avatar commits cannot steal each other's
/// label. Mutation sites fill these (#865); capture takes them with a
/// generic fallback.
#[derive(Resource, Default)]
pub struct PendingUndoLabels {
    room: Option<String>,
    avatar: Option<String>,
}

impl PendingUndoLabels {
    pub fn set_room(&mut self, label: impl Into<String>) {
        self.room = Some(label.into());
    }

    pub fn set_avatar(&mut self, label: impl Into<String>) {
        self.avatar = Some(label.into());
    }

    /// True while a label is already parked for the room entry — the
    /// coarse tab-level fallback checks this so it never overwrites a
    /// specific site's label from the same edit burst.
    pub fn room_pending(&self) -> bool {
        self.room.is_some()
    }

    pub fn avatar_pending(&self) -> bool {
        self.avatar.is_some()
    }

    /// A handle pre-bound to one editor's slot, for shared UI helpers
    /// (the generator tree serves both editors) that shouldn't need to
    /// know which editor they're inside. Inventory has no undo stack —
    /// its slot swallows labels.
    pub fn slot(&mut self, kind: crate::ui::shortcuts::EditorKind) -> LabelSlot<'_> {
        LabelSlot { labels: self, kind }
    }

    /// The label parked for the next room entry, unconsumed — for tests
    /// that assert what a mutation site named its edit.
    pub fn peek_room(&self) -> Option<&str> {
        self.room.as_deref()
    }

    fn take_room(&mut self) -> String {
        self.room
            .take()
            .unwrap_or_else(|| GENERIC_LABEL.to_string())
    }

    fn take_avatar(&mut self) -> String {
        self.avatar
            .take()
            .unwrap_or_else(|| GENERIC_LABEL.to_string())
    }
}

/// See [`PendingUndoLabels::slot`].
pub struct LabelSlot<'a> {
    labels: &'a mut PendingUndoLabels,
    kind: crate::ui::shortcuts::EditorKind,
}

impl LabelSlot<'_> {
    /// Name the edit that is being committed this frame ("delete of
    /// oak_3 + 12 placements"). Latest-wins within a frame.
    pub fn set(&mut self, label: impl Into<String>) {
        match self.kind {
            crate::ui::shortcuts::EditorKind::World => self.labels.set_room(label),
            crate::ui::shortcuts::EditorKind::Avatar => self.labels.set_avatar(label),
            crate::ui::shortcuts::EditorKind::Inventory => {}
        }
    }
}

/// Observe `LiveRoomRecord` commits and record them. Runs in
/// `PostUpdate` so every writer's tick from this frame (editor debounce
/// flush, gizmo commit, context menu, drag-drop, lots, portal, inbound)
/// is visible together with the signal it raised.
pub fn capture_room_history(
    record: Option<Res<LiveRoomRecord>>,
    session: Option<Res<AtprotoSession>>,
    room_did: Option<Res<CurrentRoomDid>>,
    editor: Res<RoomEditorState>,
    mut history: ResMut<RoomUndoHistory>,
    mut signals: ResMut<RoomWriteSignals>,
    mut labels: ResMut<PendingUndoLabels>,
) {
    let (Some(record), Some(room_did)) = (record, room_did) else {
        history.clear();
        return;
    };
    // Guests can't edit (the World Editor is owner-gated), but they DO
    // receive wholesale owner broadcasts — tracking those as history
    // would burn a record clone per broadcast for a stack nobody can
    // use. Keep the ring empty until this user owns the room.
    if session.as_ref().is_none_or(|s| s.did != room_did.0) {
        history.clear();
        return;
    }
    if !record.is_changed() {
        return;
    }
    let foreign = std::mem::take(&mut signals.foreign);
    let derived = std::mem::take(&mut signals.derived);
    let obs = if foreign {
        Observation::Foreign
    } else if derived {
        Observation::Derived
    } else {
        Observation::Edit(labels.take_room())
    };
    history.observe(
        Some(room_did.0.as_str()),
        obs,
        || record.0.clone(),
        || editor.undo_selection(),
    );
}

/// Observe `LiveAvatarRecord` commits. The avatar record is strictly
/// single-writer (remote `AvatarStateUpdate`s land on `RemotePeer`
/// components, never here), so every tick is either a user edit or a
/// #863 restore — no signal plumbing needed.
pub fn capture_avatar_history(
    record: Option<Res<LiveAvatarRecord>>,
    editor: Res<AvatarEditorState>,
    mut history: ResMut<AvatarUndoHistory>,
    mut labels: ResMut<PendingUndoLabels>,
) {
    let Some(record) = record else {
        history.clear();
        return;
    };
    if !record.is_changed() {
        return;
    }
    let label = labels.take_avatar();
    history.observe(
        None,
        Observation::Edit(label),
        || record.0.clone(),
        || editor.undo_selection(),
    );
}

/// `OnExit(InGame)` teardown: undo does not survive logout (decision
/// 2026-07-18), and a stale ring from a previous login could otherwise
/// offer "undo" into another session's states on the next one. Kept out
/// of `logout::cleanup_on_logout`, which already sits at Bevy's
/// 16-param ceiling.
pub fn clear_history_on_logout(
    mut room: ResMut<RoomUndoHistory>,
    mut avatar: ResMut<AvatarUndoHistory>,
    mut signals: ResMut<RoomWriteSignals>,
    mut labels: ResMut<PendingUndoLabels>,
) {
    room.clear();
    avatar.clear();
    *signals = RoomWriteSignals::default();
    *labels = PendingUndoLabels::default();
}

#[cfg(test)]
mod tests {
    use super::*;

    type H = UndoHistory<String, u32>;

    fn seeded() -> H {
        let mut h = H::default();
        h.reset(Some("did:test:room"), "base".into(), 0);
        h
    }

    /// Push `state` as an edit labelled by `sel`, for the byte-budget
    /// tests where the state itself is the payload being measured.
    fn edit_with(h: &mut H, state: &str, sel: u32) {
        h.observe(
            Some("did:test:room"),
            Observation::Edit(format!("edit {sel}")),
            || state.to_string(),
            || sel,
        );
    }

    fn edit(h: &mut H, state: &str, sel: u32) {
        h.observe(
            Some("did:test:room"),
            Observation::Edit(format!("edit {state}")),
            || state.to_string(),
            || sel,
        );
    }

    /// The ring is bounded by BYTES, not only by depth (#1270 f417).
    ///
    /// The pairing is the two bounds on the same ring. With small
    /// records the depth bound bites first and nothing changes — 33
    /// entries, no eviction for size, no note on the button. With
    /// records big enough that thirty-three of them would be hundreds of
    /// megabytes, the byte bound bites first and the history gets
    /// shorter. The shape being replaced kept 33 in BOTH cases, which is
    /// the failure this separates: it is not that the ring was too deep,
    /// it is that depth was never a memory bound at all.
    #[test]
    fn the_ring_is_bounded_by_bytes_as_well_as_depth() {
        use crate::config::ui::editor::{MIN_UNDO_DEPTH, UNDO_DEPTH, UNDO_RING_BUDGET_BYTES};

        // Small records: the depth bound is the one that bites.
        let mut small = seeded();
        for i in 0..100 {
            edit(&mut small, &format!("state {i}"), i);
        }
        assert_eq!(
            small.len(),
            UNDO_DEPTH + 1,
            "baseline plus UNDO_DEPTH undoable steps, as before"
        );
        assert!(
            small.bytes() < UNDO_RING_BUDGET_BYTES,
            "and well inside the byte budget, so nothing else applies"
        );
        assert_eq!(
            small.depth_note(),
            None,
            "an untrimmed ring says nothing — silence is the common case"
        );

        // Records a quarter of the budget each. The old ring would hold
        // 33 of these; that is a little over 8x the budget.
        let big = "x".repeat(UNDO_RING_BUDGET_BYTES / 4);
        let mut heavy = H::default();
        heavy.reset(Some("did:test:room"), big.clone(), 0);
        for i in 0..20 {
            edit_with(&mut heavy, &big, i);
        }
        assert!(
            heavy.len() < UNDO_DEPTH + 1,
            "the byte budget shortened the ring below the depth bound — it held \
             {} entries",
            heavy.len()
        );
        assert!(
            heavy.bytes() <= UNDO_RING_BUDGET_BYTES,
            "and the total is inside the budget: {} bytes",
            heavy.bytes()
        );
        assert!(
            heavy.can_undo(),
            "a shortened ring is still an undo ring — the degradation is fewer \
             steps, never zero"
        );
        let note = heavy.depth_note().expect("a trimmed ring says so");
        assert!(
            note.contains(&heavy.cursor.to_string()) && note.contains("32"),
            "the note names the depth on offer and the depth expected: {note}"
        );

        // The floor: one record bigger than the whole budget. Every step
        // evicts, and the ring must still offer undo rather than
        // silently removing the feature in the world that most needs it.
        let enormous = "y".repeat(UNDO_RING_BUDGET_BYTES * 2);
        let mut giant = H::default();
        giant.reset(Some("did:test:room"), enormous.clone(), 0);
        for i in 0..5 {
            edit_with(&mut giant, &enormous, i);
        }
        assert_eq!(
            giant.len(),
            MIN_UNDO_DEPTH + 1,
            "the floor holds even when a single entry exceeds the whole budget"
        );
        assert!(giant.can_undo());
    }

    /// Eviction never drops the entry the record currently holds
    /// (#1270 f417).
    ///
    /// The trap: after an undo the cursor sits BEHIND a redo branch, so
    /// `entries.len()` can be large while `entries[0]` is the live state.
    /// A byte budget that trimmed on length alone would evict the world
    /// out from under the owner and leave the cursor pointing at a
    /// different one. Both eviction loops are gated on the CURSOR for
    /// exactly this, and the price is that a ring can sit over budget
    /// when there is nothing it is allowed to drop — which is the right
    /// trade: the next edit truncates the redo branch and the budget
    /// applies again.
    #[test]
    fn eviction_never_drops_the_current_state() {
        use crate::config::ui::editor::UNDO_RING_BUDGET_BYTES;

        // Small edits, so the ring is intact and the baseline survives.
        let mut h = seeded();
        for i in 0..4 {
            edit(&mut h, &format!("state {i}"), i);
        }
        // Walk all the way back, leaving a redo branch ahead of the
        // cursor.
        while h.can_undo() {
            h.undo();
        }
        h.suppress_capture = false;
        assert_eq!(h.cursor, 0);
        assert_eq!(h.len(), 5, "and four entries ahead of it");

        // Now a derived write folds a record bigger than the whole budget
        // into the current entry. Over budget, five entries present,
        // cursor at zero: there is nothing droppable.
        let big = "b".repeat(UNDO_RING_BUDGET_BYTES + 1);
        h.observe(
            Some("did:test:room"),
            Observation::Derived,
            || big.clone(),
            || 0,
        );

        assert_eq!(h.cursor, 0, "the cursor did not move");
        assert_eq!(h.len(), 5, "and nothing was evicted");
        assert_eq!(
            h.entries[h.cursor].record.len(),
            big.len(),
            "the fold replaced the current entry, which is what Derived means"
        );
        assert!(
            h.bytes() > UNDO_RING_BUDGET_BYTES,
            "so the ring sits over budget rather than evicting the live state"
        );
        assert!(h.can_redo(), "and the redo branch survived intact");

        // The next real edit truncates that branch, and the budget bites
        // again — the over-budget window is bounded by one edit.
        edit(&mut h, "a new branch", 42);
        assert!(
            h.bytes() <= UNDO_RING_BUDGET_BYTES || h.cursor <= 2,
            "held {} bytes at cursor {}",
            h.bytes(),
            h.cursor
        );
    }

    /// Eviction keeps the running total honest — the property every
    /// other assertion above rests on (#1270 f417).
    ///
    /// `total_bytes` is maintained incrementally so the budget check is
    /// an integer compare, and an incremental total is a total that can
    /// drift. Three paths change it: a push, a redo-branch truncation,
    /// and the `Derived` fold, which REPLACES an entry and so has to
    /// subtract the old size before adding the new one. A drift there
    /// would show as a ring that trims itself for no reason, or one that
    /// never trims at all.
    #[test]
    fn the_running_byte_total_matches_the_entries() {
        fn sum(h: &H) -> usize {
            h.entries.iter().filter_map(|e| e.bytes).sum()
        }

        let mut h = seeded();
        assert_eq!(h.bytes(), sum(&h));

        for i in 0..40 {
            edit(&mut h, &format!("state {i}"), i);
            assert_eq!(h.bytes(), sum(&h), "after push {i}");
        }

        // Undo twice, then edit: the redo branch is truncated.
        h.undo();
        h.undo();
        // `undo` armed the suppression twice; drain it so the edit below
        // is seen as one.
        h.suppress_capture = false;
        edit(&mut h, "a new branch", 99);
        assert_eq!(h.bytes(), sum(&h), "after the redo branch was dropped");

        // A derived fold replaces the current entry with a much larger
        // one — lot auto-population adding hundreds of generators.
        h.observe(
            Some("did:test:room"),
            Observation::Derived,
            || "z".repeat(50_000),
            || 0,
        );
        assert_eq!(h.bytes(), sum(&h), "after a derived fold grew the entry");

        // …and with a much smaller one.
        h.observe(
            Some("did:test:room"),
            Observation::Derived,
            || String::from("tiny"),
            || 0,
        );
        assert_eq!(h.bytes(), sum(&h), "after a derived fold shrank the entry");

        h.clear();
        assert_eq!(h.bytes(), 0, "and a cleared ring holds nothing");
    }

    #[test]
    fn push_undo_redo_round_trip() {
        let mut h = seeded();
        edit(&mut h, "a", 1);
        edit(&mut h, "b", 2);
        assert!(h.can_undo());
        assert!(!h.can_redo());
        assert_eq!(h.undo_label(), Some("edit b"));

        let (entry, undone) = h.undo().expect("undo available");
        assert_eq!(entry.record, "a");
        assert_eq!(entry.selection, 1);
        assert_eq!(undone, "edit b");
        assert!(h.can_redo());
        assert_eq!(h.redo_label(), Some("edit b"));

        let (entry, redone) = h.redo().expect("redo available");
        assert_eq!(entry.record, "b");
        assert_eq!(redone, "edit b");
        assert!(!h.can_redo());
    }

    #[test]
    fn undo_to_baseline_then_stops() {
        let mut h = seeded();
        edit(&mut h, "a", 1);
        assert_eq!(h.undo().expect("one step").0.record, "base");
        assert!(h.undo().is_none());
        assert!(h.can_redo());
    }

    #[test]
    fn new_edit_truncates_redo_branch() {
        let mut h = seeded();
        edit(&mut h, "a", 1);
        edit(&mut h, "b", 2);
        h.undo();
        // The suppression armed by undo() models the restore write; the
        // next tick after that is the real new edit.
        h.observe(
            Some("did:test:room"),
            Observation::Edit("restore".into()),
            || "a".to_string(),
            || 1,
        );
        edit(&mut h, "c", 3);
        assert!(!h.can_redo());
        assert_eq!(h.undo_label(), Some("edit c"));
        assert_eq!(h.undo().expect("undo").0.record, "a");
        assert!(h.undo().is_some()); // back to baseline
        assert!(h.undo().is_none());
    }

    #[test]
    fn ring_is_bounded_and_evicts_oldest() {
        let mut h = seeded();
        let depth = crate::config::ui::editor::UNDO_DEPTH;
        for i in 0..depth + 10 {
            edit(&mut h, &format!("s{i}"), i as u32);
        }
        assert_eq!(h.len(), depth + 1);
        // Walk all the way back: the oldest reachable state is s9 (the
        // first ten baselines were evicted), and exactly `depth` undos
        // are available.
        let mut steps = 0;
        let mut last = String::new();
        while let Some((e, _)) = h.undo() {
            last = e.record.clone();
            steps += 1;
            // Model the restore write so the suppression doesn't leak
            // into the next loop iteration's bookkeeping.
            h.observe(
                Some("did:test:room"),
                Observation::Edit("unused".into()),
                || last.clone(),
                || 0,
            );
        }
        assert_eq!(steps, depth);
        assert_eq!(last, "s9");
    }

    #[test]
    fn foreign_write_resets_to_fresh_baseline() {
        let mut h = seeded();
        edit(&mut h, "a", 1);
        edit(&mut h, "b", 2);
        h.observe(
            Some("did:test:room"),
            Observation::Foreign,
            || "remote".to_string(),
            || 0,
        );
        assert!(!h.can_undo());
        assert!(!h.can_redo());
        assert_eq!(h.len(), 1);
        // The baseline is the foreign state — a subsequent edit undoes
        // back to it, not past it.
        edit(&mut h, "c", 3);
        assert_eq!(h.undo().expect("undo").0.record, "remote");
    }

    #[test]
    fn key_change_resets_even_without_signal() {
        let mut h = seeded();
        edit(&mut h, "a", 1);
        // Portal travel: same resource, different room DID, no signal.
        h.observe(
            Some("did:test:other"),
            Observation::Edit("phantom".into()),
            || "other-room".to_string(),
            || 0,
        );
        assert!(!h.can_undo());
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn derived_write_folds_into_current_entry() {
        let mut h = seeded();
        edit(&mut h, "road-edit", 1);
        h.observe(
            Some("did:test:room"),
            Observation::Derived,
            || "road-edit+lots".to_string(),
            || 9,
        );
        // No new entry was minted...
        assert_eq!(h.len(), 2);
        assert_eq!(h.undo_label(), Some("edit road-edit"));
        // ...and undoing the triggering edit undoes its fallout too,
        // while redo replays the folded state (buildings included).
        assert_eq!(h.undo().expect("undo").0.record, "base");
        let (entry, _) = h.redo().expect("redo");
        assert_eq!(entry.record, "road-edit+lots");
    }

    #[test]
    fn derived_write_on_baseline_folds_into_baseline() {
        let mut h = seeded();
        h.observe(
            Some("did:test:room"),
            Observation::Derived,
            || "base+lots".to_string(),
            || 0,
        );
        assert_eq!(h.len(), 1);
        assert!(!h.can_undo());
    }

    #[test]
    fn restore_tick_is_suppressed_once() {
        let mut h = seeded();
        edit(&mut h, "a", 1);
        let restored = h.undo().expect("undo").0.record.clone();
        assert_eq!(restored, "base");
        // The restore write's tick arrives as a normal-looking edit
        // observation; the armed suppression swallows it.
        h.observe(
            Some("did:test:room"),
            Observation::Edit("should be swallowed".into()),
            || restored.clone(),
            || 0,
        );
        assert_eq!(h.len(), 2);
        assert_eq!(h.redo_label(), Some("edit a"));
        // The one AFTER is captured normally.
        edit(&mut h, "b", 2);
        assert_eq!(h.undo_label(), Some("edit b"));
        assert!(!h.can_redo());
    }

    #[test]
    fn clear_forgets_key_and_reseeds_on_next_observation() {
        let mut h = seeded();
        edit(&mut h, "a", 1);
        h.clear();
        assert!(h.is_empty());
        assert!(!h.can_undo());
        edit(&mut h, "fresh", 0);
        // First observation after clear() seeds a baseline rather than
        // recording an undoable edit.
        assert_eq!(h.len(), 1);
        assert!(!h.can_undo());
    }
}
