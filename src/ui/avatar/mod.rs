//! Avatar editor — tabbed split view.
//!
//! The avatar window has four tabs:
//!
//!   * **Body** (#1059) — the rigged engine body: the sibling crate's own
//!     axis sections hosted in this window's chrome, plus the cross-app
//!     wardrobe. See [`body`].
//!   * **Attachments** (#1059) — props worn at rig sockets, copied out of
//!     the inventory. See [`attachments`].
//!   * **Visuals** — embeds the same tree-view + detail-panel widget that
//!     drives the room editor's Generators tab, fed by an
//!     [`AvatarVisualsTreeSource`] adapter so a *generator* body's tree is
//!     editable through the unified vocabulary. A rigged body has no such
//!     tree and the tab says so.
//!   * **Locomotion** — picker for the [`crate::pds::LocomotionConfig`]
//!     preset (HoverBoat / Humanoid / Airplane / Helicopter / Car) plus a
//!     per-preset slider panel for collider dimensions and physics
//!     tuning. Each preset's panel lives in `locomotion`.
//!
//! Live UX is preserved: every widget mutates [`LiveAvatarRecord`] in
//! place, the player module rebuilds visuals or swaps locomotion the same
//! frame the resource changes, and `network::broadcast_avatar_state`
//! pushes a preview update to peers so they see the edit before the
//! author commits. Three explicit buttons drive persistence and discard
//! flows:
//!
//!   * **Save to PDS** writes the current `LiveAvatarRecord` to the
//!     owner's PDS via `com.atproto.repo.putRecord` and then syncs the
//!     value into [`StoredAvatarRecord`] on success.
//!   * **Revert to saved** drops all in-flight edits by copying
//!     [`StoredAvatarRecord`] back into `LiveAvatarRecord`.
//!   * **Reset to default** replaces `LiveAvatarRecord` with the canonical
//!     [`AvatarRecord::default_for_did`] seed.

mod attachments;
pub(crate) use attachments::{
    attach_record, is_worn_from, record_for_inventory_item, save_worn_to_inventory, take_off_rkey,
    take_off_source, worn_rkeys_from,
};
mod body;
mod locomotion;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::diagnostics::SessionLog;
use crate::diagnostics::event::{EventPayload, RecordKind};
use crate::pds::{self, AvatarRecord};
use crate::state::{
    LiveAvatarRecord, LiveInventoryRecord, PublishFeedback, PublishStatus, StoredAvatarRecord,
};
use crate::ui::editable::{
    RecordAction, SeedAction, pin_axis_row, publish_status_line, save_load_reset_row, seed_row,
};
use crate::ui::room::RoomEditorState;
use crate::ui::room::generators::{
    AttachmentTreeSource, AvatarVisualsTreeSource, GenNodeId, draw_generators_tab,
};

use locomotion::draw_locomotion_tab;

/// Async task for publishing the avatar record to the owner's PDS. Carries the
/// target `did` + dispatch time so [`poll_publish_avatar_tasks`] can emit a typed
/// `RecordWrite*` session event (with the write's duration) when it resolves.
#[derive(Component)]
pub struct PublishAvatarTask {
    pub task: bevy::tasks::Task<Result<(), String>>,
    pub did: String,
    pub spawned_at: f64,
    /// Serialized size of the record being written, measured at dispatch so
    /// the poll system can gauge + log it (#694).
    pub record_bytes: Option<usize>,
    /// The exact avatar this task handed to the PDS. On success `stored` is
    /// pinned to THIS, never to whatever `live` holds when the task lands
    /// (#1116). Load-bearing twice over here: `stored` is what the dirty
    /// flag diffs against, AND since #1110 it is the baseline the
    /// attachment delete set is derived from (`stored` refs − `live`
    /// refs). A `stored` that claims an attachment was published when it
    /// was not makes the NEXT save compute a delete for a record the PDS
    /// never had, and `applyWrites` rejects the whole batch.
    pub published: AvatarRecord,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
enum AvatarTab {
    #[default]
    Body,
    Attachments,
    Visuals,
    Locomotion,
}

/// Persistent avatar-editor state across frames. Promoted to a `Resource`
/// (alongside `RoomEditorState`) so the 3D gizmo controller in
/// `editor_gizmo` and the locomotion-freeze gate in `player::mod` can
/// observe which visuals node the owner has selected without reading
/// through the egui closure.
#[derive(Resource, Default)]
pub struct AvatarEditorState {
    selected_tab: AvatarTab,
    /// Tree-view selection mirrors the room editor's RoomEditorState.
    /// `selected_generator` is always `Some(AvatarVisualsTreeSource::ROOT_NAME)`
    /// once a node has been picked; `selected_prim_path` is the child
    /// chain into the visuals tree.
    pub selected_generator: Option<String>,
    pub selected_prim_path: Option<Vec<usize>>,
    tree_view_state: egui_ltreeview::TreeViewState<GenNodeId>,
    /// Unused for the avatar (single-root sources have no rename) but
    /// required by [`draw_generators_tab`]'s signature. Holding an owned
    /// `Option` lets us hand a `&mut` to the callee without
    /// conditionally constructing a stack reference each frame.
    renaming_unused: Option<(String, String)>,
    /// Seconds remaining before a pending widget change is flushed into
    /// `LiveAvatarRecord`'s change tick. The downstream player rebuild
    /// and `network::broadcast_avatar_state` peer broadcast fire once
    /// when the timer drains rather than every frame.
    pending_flush_secs: f32,
    /// Pop-out audio editor state for the per-construct audio slot on
    /// avatar visuals generators. Shares the same widget as the room
    /// editor; see [`crate::ui::room::audio::AudioEditorState`].
    pub(crate) audio_editor: crate::ui::room::audio::AudioEditorState,
    /// Buffer for the manual re-roll "Random seed" row — defaults to the
    /// owner's DID seed, editable to re-roll the whole avatar. See
    /// [`crate::ui::editable::seed_row`].
    seed_row_state: crate::ui::editable::SeedRowState,
    /// Per-axis locks for the pinned re-roll (#1005): axes held (or
    /// explicitly picked) across re-roll "Apply" clicks via a
    /// deterministic seed hunt. Transient editor state — never stored in
    /// the record. See [`crate::seeded_defaults::AvatarPins`].
    avatar_pins: crate::seeded_defaults::AvatarPins,
    /// Memoized hunt result for the axis readout (#1005): re-hunts only
    /// when the seed text or the pins change, so the preview can derive
    /// from the seed "Apply" will actually use without paying the
    /// hunt every frame.
    pin_hunt: crate::ui::editable::PinHuntCache<crate::seeded_defaults::AvatarPins>,
    /// Pending publish-after-unrecoverable-fetch confirmation (#840):
    /// while [`crate::state::AvatarRecordRecovery`] is present the
    /// editor holds the default, and saving would overwrite the real
    /// stored record — the first publish asks first.
    publish_guard: crate::ui::confirm::ConfirmState<()>,
    /// Pending destructive tree-operation confirmations (#838): root
    /// delete (no-op for the single-root avatar tree, but the kind-change
    /// half is live) shared with the room editor's Generators machinery.
    tree_confirms: crate::ui::room::generators::TreeConfirms,
    /// Cached seeded-default record, keyed by the DID it was built for (#637).
    /// `AvatarRecord::default_for_did` runs the full part-composition pipeline,
    /// so build it once per session rather than every frame the editor is open;
    /// invalidated when the session DID changes.
    default_cache: Option<(String, AvatarRecord)>,
    /// Mirror of this frame's "Avatar window is open and un-collapsed"
    /// state, written by [`avatar_ui`] so non-UI systems can read it
    /// without reaching into egui. Since #1103 the freeze gates no longer
    /// key on it — only on an aimed gizmo — but it still decides when the
    /// selections are released ([`Self::release_hidden_selections`]):
    /// collapsing the window counts as closed.
    window_visible: bool,
    /// Set for one frame when an in-world pick (#823) selects a visuals
    /// node. On the next Visuals-tab draw the tree grabs keyboard focus
    /// so the picked row highlights like a direct click — the same
    /// one-shot mechanism as `RoomEditorState::pending_tree_focus`.
    pending_tree_focus: bool,
    /// The fetched cross-app wardrobe listing (#1059), refreshed on demand
    /// from the Body tab rather than on open: it is a `listRecords` walk of
    /// someone's whole avatar collection.
    wardrobe: body::WardrobeListing,
    /// Attachment picker state (item + socket) across frames.
    attachments: attachments::AttachmentsTabState,
    /// Which worn prop the in-world offset gizmo is aimed at (#1062), by
    /// attachment record rkey. An attachment is addressed by its `(rkey,
    /// socket)` record pair rather than by a visuals-tree path — a prop is
    /// not a node in any tree — so this is deliberately *not* folded into
    /// `selected_prim_path`. Mutually exclusive with the visuals selection:
    /// leaving either tab clears the other's.
    selected_attachment: Option<String>,
    /// Set for one frame when an in-world pick (#1062) selects a worn prop,
    /// so the next Attachments draw force-opens that row and scrolls to it.
    /// The attachment-tab twin of [`Self::pending_tree_focus`].
    pending_attachment_focus: bool,
    /// Which worn prop's PARTS editor is open in the Attachments tab
    /// (#1098), by record key: the tab then shows that item's generator
    /// tree — the region-asset editor over the worn copy — instead of the
    /// worn list. `None` = the list.
    editing_parts: Option<String>,
    /// The selected PART of a worn item (#1098): `(rkey, path into the
    /// item tree)`. Its own selection, distinct from `selected_attachment`
    /// (the whole prop's offset gizmo) and the visuals selection — one
    /// gizmo target at a time, so setting any one of the three clears the
    /// other two.
    attachment_part: Option<(String, Vec<usize>)>,
    /// Tree-view state for the parts editor — separate from the visuals
    /// tree's so a body's expanded rows survive editing a prop.
    part_tree_state: egui_ltreeview::TreeViewState<GenNodeId>,
    /// Destructive-op confirms for the parts editor (#838 machinery).
    part_tree_confirms: crate::ui::room::generators::TreeConfirms,
    /// One-shot focus request for the parts tree after a scene pick.
    pending_part_focus: bool,
    /// The parts editor's selected root/path mirror, in the shape
    /// [`draw_generators_tab`] wants (`selected_generator` is the rkey).
    part_selected_generator: Option<String>,
    part_selected_path: Option<Vec<usize>>,
}

impl AvatarEditorState {
    /// True when a visuals row is currently selected. The locomotion
    /// freeze gate and the gizmo dispatch read this.
    pub fn has_visuals_selection(&self) -> bool {
        self.selected_prim_path.is_some()
    }

    /// The worn prop the offset gizmo is aimed at (#1062), by record rkey.
    pub fn selected_attachment(&self) -> Option<&str> {
        self.selected_attachment.as_deref()
    }

    /// True while a worn prop is selected for in-world offset editing.
    pub fn has_attachment_selection(&self) -> bool {
        self.selected_attachment.is_some()
    }

    /// Drop the attachment selection — tab switch, window collapse, or the
    /// mutex against the visuals selection.
    pub fn clear_attachment_selection(&mut self) {
        self.selected_attachment = None;
    }

    /// Drop the gizmo selection if it named a prop that just came off
    /// (#1096): the Inventory window's Take off and the scene menu detach
    /// outside this editor, and a gizmo aimed at a prop that is no longer
    /// worn has nothing to move.
    ///
    /// Retiring the *records* is not tracked here. The next save derives its
    /// delete set from the published record's reference list (#1110), which
    /// take-off has already shortened, so there is no session queue to keep
    /// in step — and nothing left to go stale across an undo or a logout.
    pub(crate) fn forget_attachments(&mut self, rkeys: impl IntoIterator<Item = String>) {
        for rkey in rkeys {
            if self.selected_attachment.as_deref() == Some(rkey.as_str()) {
                self.selected_attachment = None;
            }
        }
    }

    /// True while the local rigged body must be pinned to its **bind pose**
    /// (#1062): an attachment offset lives in the carrying joint's rest
    /// frame, so a gizmo aimed at a **whole worn prop** is placed in that
    /// frame and the body has to be in that pose for the two to coincide.
    /// Read by [`crate::player`]'s rigged motion driver.
    ///
    /// Exactly the whole-prop gizmo selection and nothing wider (#1103,
    /// owner direction): the tab being open is not a hold — the numeric
    /// rows work against an animating body, because their arithmetic never
    /// reads the pose. A **part** selection is deliberately NOT here
    /// (#1106): a part's transform is relative to its item root, which
    /// rides the joint wherever it is, so snapping the body to rest under
    /// a part that was just detached at its animated pose moved the parent
    /// out from under it — selecting a part visibly shifted it. Parts hold
    /// the pose as it stands instead: [`Self::holds_rig_pose`].
    pub fn holds_rig_at_rest(&self) -> bool {
        self.has_attachment_selection()
    }

    /// True while the local rigged body must be held **exactly where it is**
    /// (#1106): a gizmo is aimed at a part of a worn item. The part is
    /// detached to world space at its current pose and committed back
    /// against its parent's pose, so the parent must not move — but it may
    /// stand in any pose at all. Selecting must never change a transform,
    /// so this is a pause, not a re-pose: the driver skips the body and the
    /// last pose stays applied. Read by [`crate::player`]'s rigged motion
    /// driver, after [`Self::holds_rig_at_rest`] (a whole-prop selection
    /// and a part selection are mutually exclusive, so the order is moot).
    pub fn holds_rig_pose(&self) -> bool {
        self.has_part_selection()
    }

    /// Select a worn prop from an in-world scene pick (#1062), the
    /// attachment-tab counterpart of [`Self::select_from_scene_pick`]: the
    /// tab that can show it comes forward, the row is selected, and a
    /// one-shot focus request is armed so the next draw opens and scrolls to
    /// it. The visuals selection goes, because only one gizmo target exists.
    pub fn select_attachment_from_scene_pick(&mut self, rkey: String) {
        self.selected_tab = AvatarTab::Attachments;
        // The whole-prop selection lives on the worn LIST; a parts editor
        // that happens to be open steps aside.
        self.editing_parts = None;
        self.clear_part_selection();
        self.selected_attachment = Some(rkey);
        self.pending_attachment_focus = true;
        if self.has_visuals_selection() {
            self.clear_visuals_selection();
        }
    }

    /// Land the editor on the Body tab (#1097) — the scene menu's "Edit
    /// avatar" on one's own rigged body, which has no visuals node to
    /// select. Drops every selection so no gizmo is left aimed.
    pub fn open_body_tab(&mut self) {
        self.selected_tab = AvatarTab::Body;
        self.selected_attachment = None;
        self.clear_part_selection();
        if self.has_visuals_selection() {
            self.clear_visuals_selection();
        }
    }

    /// The worn prop whose parts editor is open (#1098).
    pub fn editing_parts(&self) -> Option<&str> {
        self.editing_parts.as_deref()
    }

    /// Open the parts editor on a worn prop (#1098): the Attachments tab
    /// comes forward showing that item's tree, with the item ROOT selected
    /// so a gizmo is aimed immediately. The whole-prop offset selection
    /// goes — one gizmo target at a time.
    pub fn open_parts_editor(&mut self, rkey: String) {
        self.selected_tab = AvatarTab::Attachments;
        self.editing_parts = Some(rkey.clone());
        self.select_attachment_part(rkey, Vec::new());
    }

    /// Back from the parts editor to the worn list. Drops the part
    /// selection with it.
    pub fn close_parts_editor(&mut self) {
        self.editing_parts = None;
        self.clear_part_selection();
    }

    /// The selected part of a worn item, `(rkey, path)`.
    pub fn attachment_part(&self) -> Option<(&str, &[usize])> {
        self.attachment_part
            .as_ref()
            .map(|(rkey, path)| (rkey.as_str(), path.as_slice()))
    }

    pub fn has_part_selection(&self) -> bool {
        self.attachment_part.is_some()
    }

    /// Select a part of a worn item (#1098) — from the parts tree or a
    /// scene pick. Clears the other two gizmo selections and mirrors the
    /// choice into the tree-view state so the row highlights.
    pub fn select_attachment_part(&mut self, rkey: String, path: Vec<usize>) {
        self.selected_attachment = None;
        if self.has_visuals_selection() {
            self.clear_visuals_selection();
        }
        for depth in 0..path.len() {
            self.part_tree_state
                .set_openness(GenNodeId::child(rkey.clone(), path[..depth].to_vec()), true);
        }
        self.part_tree_state
            .set_selected(vec![GenNodeId::child(rkey.clone(), path.clone())]);
        self.part_selected_generator = Some(rkey.clone());
        self.part_selected_path = Some(path.clone());
        self.attachment_part = Some((rkey, path));
    }

    /// A scene pick on a part of a worn item (#1098): opens that prop's
    /// parts editor if it is not already open, selects the part, and arms
    /// the one-shot tree focus.
    pub fn select_attachment_part_from_scene_pick(&mut self, rkey: String, path: Vec<usize>) {
        self.selected_tab = AvatarTab::Attachments;
        self.editing_parts = Some(rkey.clone());
        self.select_attachment_part(rkey, path);
        self.pending_part_focus = true;
    }

    pub fn clear_part_selection(&mut self) {
        self.attachment_part = None;
        self.part_selected_generator = None;
        self.part_selected_path = None;
        self.part_tree_state.set_selected(Vec::new());
    }

    /// True while the Avatar window is open with its body visible (as of
    /// the last [`avatar_ui`] run). The local gait pause reads this.
    pub fn window_visible(&self) -> bool {
        self.window_visible
    }

    /// True whenever the local avatar should be held perfectly still for
    /// editing: **a gizmo is aimed at it or at something it wears** — a
    /// visuals row, a worn prop, or a part of one. This is the gate the
    /// cosmetic gait/sway hold (`player::gait::animate_avatar_gait`) *and*
    /// the full-body chassis freeze (`player::freeze_local_avatar_while_editing`,
    /// which also stops falling-physics and the passive movers) key on.
    ///
    /// Selection-scoped by owner direction (#1103), reversing #814's
    /// window-wide hold: with the editor open and nothing aimed the body
    /// walks, sways and falls live — the World editor's contract, where a
    /// region asset is only pinned while its gizmo is up. The close-frame
    /// gap is covered from the other side: every path that hides the panel
    /// releases the selections ([`Self::release_hidden_selections`]), so a
    /// closed window never holds. A lingering selection still holds until
    /// that release runs, so a drag released as the window goes cannot
    /// land against a moving chassis.
    pub fn holds_avatar_still(&self) -> bool {
        self.has_gizmo_selection()
    }

    /// True while any of the three avatar-side gizmo selections is live
    /// (visuals row, worn prop, worn-prop part) — the one question the
    /// freeze gates and the release paths ask.
    pub fn has_gizmo_selection(&self) -> bool {
        self.has_visuals_selection() || self.has_attachment_selection() || self.has_part_selection()
    }

    /// Drop every gizmo selection at once. The parts editor stays open
    /// (like a World-editor tab, it remembers where it was); only the aim
    /// goes, and with it the freeze and the bind-pose hold.
    pub fn clear_gizmo_selections(&mut self) {
        if self.has_visuals_selection() {
            self.clear_visuals_selection();
        }
        self.clear_attachment_selection();
        self.clear_part_selection();
    }

    /// The end-of-frame release rules (#1103), applied by [`avatar_ui`]
    /// after the window has drawn (or not): a selection only persists
    /// while the panel showing it is visible.
    ///
    /// * Window hidden or collapsed → every gizmo selection goes, so the
    ///   gizmo detaches and the chassis / bind-pose holds release — the
    ///   World editor's close contract. Before this, the part selection
    ///   (#1098) survived the close and kept both holds engaged.
    /// * Off the Visuals tab → the visuals row goes (the editor never
    ///   gizmo-edits Locomotion or Body).
    /// * Off the Attachments tab → the worn-prop and part selections go
    ///   with it (#1062).
    pub fn release_hidden_selections(&mut self, window_visible: bool) {
        if !window_visible {
            self.clear_gizmo_selections();
        }
        if self.selected_tab != AvatarTab::Visuals && self.has_visuals_selection() {
            self.clear_visuals_selection();
        }
        if self.selected_tab != AvatarTab::Attachments {
            self.clear_attachment_selection();
            self.clear_part_selection();
        }
    }

    /// A left-click into the scene that hit nothing of the local avatar's
    /// (#1103, the World editor's contract): the worn-prop and part gizmos
    /// always let go; the visuals row lets go unless face picking is
    /// armed, because an armed pick is aiming at *something* and a miss
    /// must not close the panel it is aimed from. Face picking never aims
    /// at a prop, so it does not gate those.
    pub fn release_on_scene_miss(&mut self, face_pick_armed: bool) {
        if self.has_visuals_selection() && !face_pick_armed {
            self.clear_visuals_selection();
        }
        self.clear_attachment_selection();
        self.clear_part_selection();
    }

    /// Drop the visuals selection — used when switching tabs, collapsing
    /// the editor window, or losing the mutex to the room editor.
    pub fn clear_visuals_selection(&mut self) {
        self.selected_generator = None;
        self.selected_prim_path = None;
        self.tree_view_state.set_selected(Vec::new());
    }

    /// Snapshot the selection state an undo entry carries (#862) so a
    /// restore (#863) can re-seed it instead of dumping the user to a
    /// full deselect.
    pub(crate) fn undo_selection(&self) -> crate::ui::undo::AvatarSelection {
        crate::ui::undo::AvatarSelection {
            generator: self.selected_generator.clone(),
            prim_path: self.selected_prim_path.clone(),
            tree: self.tree_view_state.selected().clone(),
        }
    }

    /// Post-restore fixup (#863): re-seed the visuals selection from the
    /// undo entry, validated against the restored record; drop parked
    /// confirm payloads and the pending widget debounce. Mirrors
    /// [`RoomEditorState::restore_from_undo`](crate::ui::room::RoomEditorState).
    pub(crate) fn restore_from_undo(
        &mut self,
        record: &AvatarRecord,
        sel: &crate::ui::undo::AvatarSelection,
    ) {
        self.publish_guard.cancel();
        self.tree_confirms.delete.cancel();
        self.tree_confirms.kind.cancel();
        // A pending burst was aimed at pre-restore state; draining it
        // would double-fire `set_changed` and mint a phantom entry.
        self.pending_flush_secs = 0.0;
        // A worn prop the restored record no longer wears cannot host a
        // gizmo (#1062); one it still wears keeps its selection.
        if let Some(rkey) = self.selected_attachment.as_deref() {
            let still_worn = record
                .body
                .rigged_ref()
                .and_then(|rig| rig.resolved.as_ref())
                .is_some_and(|resolved| resolved.attachments.iter().any(|a| a.rkey == rkey));
            if !still_worn {
                self.selected_attachment = None;
            }
        }
        match &sel.prim_path {
            // `select_from_scene_pick` is exactly the fixup contract:
            // fields set, ancestors expanded, row selected + focused.
            Some(path)
                if record
                    .body
                    .visuals()
                    .is_some_and(|v| crate::ui::undo::restore::node_path_valid(v, path)) =>
            {
                self.select_from_scene_pick(path.clone());
            }
            // Root row selected without a node path: keep it — the
            // single visuals root always exists.
            None if sel.generator.is_some() => {
                let root = AvatarVisualsTreeSource::ROOT_NAME.to_string();
                self.selected_generator = Some(root.clone());
                self.selected_prim_path = None;
                self.tree_view_state
                    .set_selected(vec![GenNodeId::root(root)]);
            }
            _ => self.clear_visuals_selection(),
        }
    }

    /// Select a visuals node from an in-world scene pick (#823), exactly
    /// as if its tree row had been clicked: selection set, every
    /// ancestor expanded (the tree collapses by default, so the picked
    /// row must be revealed), the row marked selected in the tree
    /// widget, and a one-shot focus request armed so the row gets the
    /// bright focused highlight on the next draw. Mirrors the room
    /// editor's pick path in `editor_gizmo::pick_on_scene_click`.
    pub fn select_from_scene_pick(&mut self, path: Vec<usize>) {
        // Only one gizmo target at a time — the visuals/attachment mutex is
        // the intra-editor twin of the room/avatar one.
        self.selected_attachment = None;
        self.clear_part_selection();
        let root = AvatarVisualsTreeSource::ROOT_NAME.to_string();
        self.selected_generator = Some(root.clone());
        self.selected_prim_path = Some(path.clone());
        for depth in 0..path.len() {
            self.tree_view_state
                .set_openness(GenNodeId::child(root.clone(), path[..depth].to_vec()), true);
        }
        self.tree_view_state
            .set_selected(vec![GenNodeId::child(root, path)]);
        self.pending_tree_focus = true;
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn avatar_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<crate::ui::toolbar::UiPanels>,
    mut commands: Commands,
    mut live: ResMut<LiveAvatarRecord>,
    stored: Option<Res<StoredAvatarRecord>>,
    session: Option<Res<AtprotoSession>>,
    refresh_ctx: Option<Res<crate::oauth::OauthRefreshCtx>>,
    mut feedback: ResMut<PublishFeedback<AvatarRecord>>,
    mut inventory: Option<ResMut<LiveInventoryRecord>>,
    mut editor: ResMut<AvatarEditorState>,
    mut room_editor: Option<ResMut<RoomEditorState>>,
    mut gizmo_frame_pref: ResMut<crate::editor_gizmo::GizmoFramePref>,
    mut chrome: crate::ui::layout::WindowChrome,
    mut publish_shortcut: ResMut<crate::ui::shortcuts::PublishShortcut>,
    // Grouped into one tuple param so `session_log` fits under Bevy's 16-param
    // `IntoSystem` ceiling (needed to record an avatar re-seed, #627).
    (
        audio_monitor,
        mut audio_requests,
        time,
        mut session_log,
        mut blob_ctx,
        grammar_diag,
        recovery,
        mut toasts,
        undo_history,
        mut undo_shortcut,
        mut undo_labels,
        mut face_pick,
    ): (
        Res<bevy_symbios_audio::ui::AudioMonitor>,
        MessageWriter<bevy_symbios_audio::ui::MonitorRequest>,
        Res<Time>,
        ResMut<SessionLog>,
        ResMut<crate::editor_gizmo::BlobEditContext>,
        Res<crate::world_builder::grammar_diag::GrammarDiagnostics>,
        Option<Res<crate::state::AvatarRecordRecovery>>,
        ResMut<crate::ui::toast::Toasts>,
        Res<crate::ui::undo::AvatarUndoHistory>,
        ResMut<crate::ui::undo::UndoShortcut>,
        ResMut<crate::ui::undo::PendingUndoLabels>,
        ResMut<crate::editor_gizmo::FacePick>,
    ),
) {
    // `ResMut::deref_mut` unconditionally flips the change tick, so
    // mutating `live.0` inside the egui closure would otherwise mark the
    // resource changed every frame the editor is visible — and
    // `network::broadcast_avatar_state` turns that into a peer broadcast
    // storm. Route UI access through `bypass_change_detection` and call
    // `live.set_changed()` explicitly below, only after the debounce
    // timer drains.
    let mut widget_changed = false;
    // Snapshot pre-frame selection state so we can detect (a) "selection
    // just appeared" — the rising edge that clears the room editor's
    // selection per the cross-editor mutex contract, and (b) tab change —
    // switching off the Visuals tab drops the gizmo target the same way
    // the room editor's tab bar already does.
    let prev_visuals_selected = editor.has_visuals_selection() || editor.has_attachment_selection();

    // `.open()` only hides the window *body* — without this gate the
    // whole-record `before` clone below (and the egui Window bookkeeping)
    // ran every in-game frame with the panel closed (#674). The tail logic
    // after this block still runs: collapse-deselect sees `false` here, and
    // a debounce flush pending from just before the panel closed still
    // drains and broadcasts.
    let window_visible_with_body = if !panels.avatar {
        false
    } else {
        let live_mut = live.bypass_change_detection();
        let before = live_mut.0.clone();

        let ctx = contexts.ctx_mut().unwrap();
        // Width only from the layout slot — the Avatar window auto-heights
        // to its content, and forcing the persisted height back on it
        // would pad the shorter Locomotion tab with dead space.
        let (pos, size) = chrome.place(crate::ui::layout::UiWindow::Avatar, ctx);
        // Guarded-dirty (#879): `.open(&mut panels.avatar)` through the
        // `ResMut` would mark UiPanels changed every frame, starving the
        // prefs save debounce — local copy in, write back only on close.
        let mut open = panels.avatar;
        let response = egui::Window::new("Avatar")
            .open(&mut open)
            .default_pos(pos)
            .default_width(size.x)
            .constrain_to(chrome.available_rect(ctx))
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                // --- Tab bar ----------------------------------------------
                ui.horizontal(|ui| {
                    let tabs = [
                        (AvatarTab::Body, "Body"),
                        (AvatarTab::Attachments, "Attachments"),
                        (AvatarTab::Visuals, "Visuals"),
                        (AvatarTab::Locomotion, "Locomotion"),
                    ];
                    for (tab, label) in tabs {
                        if ui
                            .selectable_label(editor.selected_tab == tab, label)
                            .clicked()
                        {
                            editor.selected_tab = tab;
                        }
                    }
                    ui.separator();
                    // Bypassed borrow + explicit tick (#871): the pref is
                    // persisted on change, and a raw ResMut deref here would
                    // re-arm the save debounce every frame the tab bar draws.
                    if crate::editor_gizmo::draw_gizmo_frame_toggle(
                        ui,
                        gizmo_frame_pref.bypass_change_detection(),
                        blob_ctx.selected_element.is_some(),
                    ) {
                        gizmo_frame_pref.set_changed();
                    }
                    ui.separator();
                    crate::ui::undo::undo_redo_buttons(
                        ui,
                        &undo_history,
                        crate::ui::shortcuts::EditorKind::Avatar,
                        &mut undo_shortcut,
                    );
                });
                ui.separator();

                let AvatarEditorState {
                    selected_tab,
                    selected_generator,
                    selected_prim_path,
                    tree_view_state,
                    renaming_unused,
                    audio_editor,
                    seed_row_state,
                    avatar_pins,
                    pin_hunt,
                    publish_guard,
                    tree_confirms,
                    default_cache,
                    pending_tree_focus,
                    wardrobe,
                    attachments: attachments_state,
                    selected_attachment,
                    pending_attachment_focus,
                    editing_parts,
                    attachment_part,
                    part_tree_state,
                    part_tree_confirms,
                    pending_part_focus,
                    part_selected_generator,
                    part_selected_path,
                    ..
                } = &mut *editor;

                // Recovery banner (#840) — the stored record could not be
                // loaded and this editor holds the DID default. Same idiom
                // as the World editor's banner; the deliberate-overwrite
                // affordance here is the publish confirm, not a reset
                // button (publishing IS the reset).
                if let Some(rec) = recovery.as_deref() {
                    egui::Frame::new()
                        .fill(crate::ui::theme::current(ui.ctx()).danger_surface)
                        .inner_margin(6.0)
                        .corner_radius(4.0)
                        .show(ui, |ui| {
                            ui.colored_label(
                                crate::ui::theme::current(ui.ctx()).danger_surface_text,
                                "⚠ Your stored avatar could not be loaded — this is the default.",
                            );
                            ui.label(
                                egui::RichText::new(format!("Reason: {}", rec.reason)).small(),
                            );
                            ui.label(
                                egui::RichText::new(
                                    "Saving will overwrite the stored copy (you'll be asked \
                                     first). Logging out and back in retries the load.",
                                )
                                .small(),
                            );
                        });
                    ui.add_space(4.0);
                }

                // --- Manual re-roll ---------------------------------------
                // The same DID-seeded engine as the defaults, with an
                // owner-chosen master seed. Replaces the whole working avatar
                // like "Reset to default" (which is this with
                // seed = fnv1a_64(did)). The pfp banner tracks the DID, not
                // the seed, so it survives a re-roll.
                //
                // Laid out in the window's normal flow rather than inside the
                // footer panel (#1048). A `TopBottomPanel` reserves the height
                // it measured LAST frame, so on the frame the collapsible
                // section below opens, the taller content overflowed that
                // reserve and egui grew the window to contain it — and a
                // `Window`'s desired size never shrinks again, so collapsing
                // handed the freed height to the greedy tab body instead of
                // giving it back. Toggling therefore ratcheted the window
                // taller every cycle. Here the tab body measures what is left
                // AFTER this block is laid out, so the body absorbs the change
                // in the same frame and the window height never moves.
                if let Some(s) = session.as_ref() {
                    let did_seed = crate::seeded_defaults::fnv1a_64(&s.did);
                    // Collapsible (#1047): the seed field plus four pin rows
                    // is the tallest fixed furniture in this window, and an
                    // owner who has settled on an avatar rarely re-rolls it
                    // again. Collapsed, the whole block folds to one header
                    // row and the tab body takes back the space.
                    let (reroll, start, effective) =
                        crate::ui::editable::reroll_section(ui, "avatar_reroll", |ui| {
                            let reroll = seed_row(
                                ui,
                                seed_row_state,
                                did_seed,
                                time.elapsed_secs_f64(),
                                "avatar",
                            );

                            // Pinned re-roll readout (#1005): what "Apply"
                            // will roll for each top-level avatar axis, each
                            // lockable. The preview derives from the hunted
                            // seed — the one a click will actually build from
                            // — not the typed one, so 🎲 previews exactly what
                            // Apply then delivers. Memoized: the hunt only
                            // reruns when the seed text or the pins change.
                            let start = seed_row_state.current_seed().unwrap_or(did_seed);
                            let effective = pin_hunt
                                .effective_seed(start, *avatar_pins, |s| avatar_pins.find_seed(s));
                            use crate::seeded_defaults::{
                                AvatarCharacter, ChassisFamily, OrnatenessTier, ThemeArchetype,
                                WearTier,
                            };
                            let rolled = AvatarCharacter::for_seed(effective.unwrap_or(start));
                            egui::Grid::new("avatar_pin_axes")
                                .num_columns(3)
                                .show(ui, |ui| {
                                    pin_axis_row(
                                        ui,
                                        "Chassis",
                                        &ChassisFamily::ALL,
                                        ChassisFamily::label,
                                        &mut avatar_pins.chassis,
                                        rolled.chassis,
                                    );
                                    pin_axis_row(
                                        ui,
                                        "Style",
                                        &ThemeArchetype::ALL,
                                        ThemeArchetype::label,
                                        &mut avatar_pins.style,
                                        rolled.style,
                                    );
                                    pin_axis_row(
                                        ui,
                                        "Ornateness",
                                        &OrnatenessTier::ALL,
                                        OrnatenessTier::label,
                                        &mut avatar_pins.ornateness,
                                        rolled.ornateness_tier(),
                                    );
                                    pin_axis_row(
                                        ui,
                                        "Wear",
                                        &WearTier::ALL,
                                        WearTier::label,
                                        &mut avatar_pins.wear,
                                        rolled.wear_tier(),
                                    );
                                });
                            (reroll, start, effective)
                        })
                        // Collapsed: no Apply button was drawn, so there is
                        // nothing to act on this frame.
                        .unwrap_or((SeedAction::None, did_seed, None));

                    if let SeedAction::Reroll(_) = reroll {
                        // Build from the same hunted seed the readout
                        // previewed — never the raw typed one.
                        if let Some(seed) = effective {
                            seed_row_state.set_seed(seed);
                            live_mut.0 = AvatarRecord::default_for_seed(seed);
                            undo_labels.set_avatar(format!("seed re-roll ({seed})"));
                            session_log.info(
                                time.elapsed_secs_f64(),
                                EventPayload::AvatarReseeded { seed },
                            );
                        } else {
                            // Unreachable in practice (the cap misses a legal
                            // pin-set with probability ~e⁻²⁴¹⁵); keep the
                            // record untouched rather than violate the locks.
                            bevy::log::warn!(
                                "pinned re-roll found no seed matching {avatar_pins:?} from {start}"
                            );
                        }
                    }
                    ui.separator();
                }

                // --- Footer as a real bottom panel (#830) -----------------
                // Declared BEFORE the tab body (egui's panels-before-content
                // rule) but rendered pinned to the window's bottom edge, so
                // it can never be clipped off a short window — the old
                // fixed FOOTER_RESERVE guessed the footer height and lost
                // whenever the guess was wrong. The tab body then fills
                // exactly the space that remains. Everything in here is
                // fixed-height, which is what keeps the panel's reserve
                // honest (see the re-roll block above).
                egui::Panel::bottom("avatar_footer")
                    .resizable(false)
                    .show(ui, |ui| {
                        // The "Smooth remote peers" toggle moved to the
                        // Settings window (#857) — it's a client network
                        // preference, not part of the avatar record this
                        // editor publishes.

                        // --- Publish / Revert / Reset -------------------------
                        // Same shared row + status line as the World and
                        // Inventory editors (`ui::editable`). Dirty is derived
                        // through `records_differ` — the *same* canonical
                        // equality the other two use — instead of
                        // `AvatarRecord`'s `PartialEq`, so all three editors
                        // behave identically.

                        let dirty = stored
                            .as_ref()
                            .is_some_and(|s| live_mut.0.publishes_differently_from(&s.0));
                        let can_publish = session.is_some() && refresh_ctx.is_some();
                        // Rebuild the seeded default only when the session DID
                        // changes, not every frame (#637) — full
                        // part-composition build.
                        match session.as_ref() {
                            Some(s) if default_cache.as_ref().is_none_or(|(d, _)| d != &s.did) => {
                                *default_cache =
                                    Some((s.did.clone(), AvatarRecord::default_for_did(&s.did)));
                            }
                            None => *default_cache = None,
                            _ => {}
                        }
                        let default_record = default_cache.as_ref().map(|(_, r)| r);
                        let can_reset = default_record
                            .is_some_and(|d| live_mut.0.publishes_differently_from(d));

                        let record_bytes = crate::ui::editable::refresh_size_readout(
                            &mut *feedback,
                            &live_mut.0,
                            time.elapsed_secs_f64(),
                        );
                        let ctrl_s =
                            publish_shortcut.take(crate::ui::shortcuts::EditorKind::Avatar);
                        let mut do_publish = false;
                        match save_load_reset_row(
                            ui,
                            dirty,
                            can_publish,
                            can_reset,
                            record_bytes,
                            ctrl_s,
                            matches!(feedback.status, PublishStatus::Publishing),
                            // Undo covers Revert/Reset here (#866).
                            None,
                        ) {
                            RecordAction::None => {}
                            RecordAction::Publish => {
                                // Clobber protection (#840): after an
                                // unrecoverable fetch the editor holds the
                                // default while the real record may still
                                // sit on the PDS — the first publish asks.
                                if let Some(rec) = recovery.as_deref() {
                                    publish_guard.request(
                                        "Overwrite your stored avatar?",
                                        format!(
                                            "Your avatar loaded as the default because \
                                             the stored copy could not be read ({}). \
                                             Saving now replaces whatever is stored on \
                                             your PDS with what you see here.",
                                            rec.reason
                                        ),
                                        "Save anyway",
                                        (),
                                    );
                                } else {
                                    do_publish = true;
                                }
                            }
                            RecordAction::Load => {
                                if let Some(stored) = &stored {
                                    live_mut.0 = stored.0.clone();
                                    undo_labels.set_avatar("load from PDS");
                                }
                            }
                            RecordAction::Reset => {
                                if let Some(default_record) = default_record {
                                    live_mut.0 = default_record.clone();
                                    undo_labels.set_avatar("reset to default");
                                }
                            }
                        }
                        if publish_guard
                            .show(ui.ctx(), "avatar-recovery-publish")
                            .is_some()
                        {
                            // Acknowledged: the overwrite is deliberate now,
                            // so the marker (and its banner) retires.
                            commands.remove_resource::<crate::state::AvatarRecordRecovery>();
                            do_publish = true;
                        }
                        if do_publish
                            && let (Some(session), Some(refresh)) =
                                (session.as_ref(), refresh_ctx.as_ref())
                        {
                            feedback.status = PublishStatus::Publishing;
                            spawn_publish_avatar_task(
                                &mut commands,
                                session,
                                refresh,
                                live_mut.0.clone(),
                                stored.as_ref().map_or_else(Vec::new, |s| {
                                    pds::avatar::wardrobe::attachment_rkeys(&s.0)
                                }),
                                time.elapsed_secs_f64(),
                            );
                        }

                        publish_status_line(ui, &feedback.status, time.elapsed_secs_f64());
                    });

                // The tab body fills exactly what the footer left over.
                let body_height = ui.available_height();

                match *selected_tab {
                    AvatarTab::Body => {
                        ui.allocate_ui(egui::vec2(ui.available_width(), body_height), |ui| {
                            let outcome = body::draw_body_tab(
                                ui,
                                &mut live_mut.0,
                                wardrobe,
                                session.as_ref().map(|s| s.did.as_str()),
                            );
                            widget_changed |= outcome.changed;
                            if let Some(label) = outcome.label {
                                undo_labels.set_avatar(label);
                            }
                            if outcome.wants_wardrobe_refresh
                                && let Some(s) = session.as_ref()
                            {
                                wardrobe.fetching = true;
                                spawn_wardrobe_list_task(&mut commands, &s.did);
                            }
                        });
                    }
                    AvatarTab::Attachments => {
                        ui.allocate_ui(egui::vec2(ui.available_width(), body_height), |ui| {
                            // The parts editor (#1098): the region-asset
                            // tree editor over one worn item's copy. Shown
                            // in place of the worn list while a prop is
                            // opened for parts; a prop taken off meanwhile
                            // drops the editor back to the list.
                            if let Some(rkey) = editing_parts.clone() {
                                let worn_item = live_mut
                                    .0
                                    .body
                                    .rigged_mut()
                                    .and_then(|rig| rig.resolved.as_mut())
                                    .and_then(|resolved| {
                                        resolved.attachments.iter_mut().find(|a| a.rkey == rkey)
                                    });
                                let Some(worn) = worn_item else {
                                    *editing_parts = None;
                                    *attachment_part = None;
                                    *part_selected_generator = None;
                                    *part_selected_path = None;
                                    return;
                                };
                                ui.horizontal(|ui| {
                                    if ui.button("⬅ Worn items").clicked() {
                                        *editing_parts = None;
                                        *attachment_part = None;
                                        *part_selected_generator = None;
                                        *part_selected_path = None;
                                        part_tree_state.set_selected(Vec::new());
                                    }
                                    let what = worn
                                        .record
                                        .source
                                        .clone()
                                        .unwrap_or_else(|| format!("prop {}", worn.rkey));
                                    ui.label(
                                        egui::RichText::new(format!("Parts of {what}")).strong(),
                                    );
                                });
                                if editing_parts.is_none() {
                                    return;
                                }
                                let mut source =
                                    AttachmentTreeSource::new(&rkey, &mut worn.record.item);
                                let request_focus = std::mem::take(pending_part_focus);
                                draw_generators_tab(
                                    ui,
                                    &mut source,
                                    part_selected_generator,
                                    part_selected_path,
                                    part_tree_state,
                                    request_focus,
                                    renaming_unused,
                                    inventory.as_deref_mut(),
                                    audio_editor,
                                    &grammar_diag,
                                    &mut widget_changed,
                                    &mut blob_ctx.selected_element,
                                    part_tree_confirms,
                                    &mut toasts,
                                    time.elapsed_secs_f64(),
                                    &mut undo_labels.slot(crate::ui::shortcuts::EditorKind::Avatar),
                                    None,
                                    &mut face_pick,
                                );
                                // The tree's selection IS the gizmo target:
                                // mirror it (a tree click picks a part; a
                                // cleared tree drops the gizmo).
                                *attachment_part = match (
                                    part_selected_generator.as_ref(),
                                    part_selected_path.as_ref(),
                                ) {
                                    (Some(root), Some(path)) if *root == rkey => {
                                        Some((rkey.clone(), path.clone()))
                                    }
                                    _ => None,
                                };
                                if attachment_part.is_some() {
                                    *selected_attachment = None;
                                }
                                return;
                            }
                            let outcome = attachments::draw_attachments_tab(
                                ui,
                                &mut live_mut.0,
                                inventory.as_deref_mut(),
                                attachments_state,
                                session.as_ref().map(|s| s.did.as_str()),
                                selected_attachment,
                                std::mem::take(pending_attachment_focus),
                                &mut toasts,
                                time.elapsed_secs_f64(),
                            );
                            widget_changed |= outcome.changed;
                            if let Some(label) = outcome.label {
                                undo_labels.set_avatar(label);
                            }
                            if let Some(rkey) = outcome.open_parts {
                                // Open on the item ROOT so a gizmo is aimed
                                // at once; the whole-prop selection yields.
                                *selected_attachment = None;
                                part_tree_state.set_selected(vec![GenNodeId::root(rkey.clone())]);
                                *part_selected_generator = Some(rkey.clone());
                                *part_selected_path = Some(Vec::new());
                                *attachment_part = Some((rkey.clone(), Vec::new()));
                                *editing_parts = Some(rkey);
                            }
                        });
                    }
                    AvatarTab::Visuals => {
                        ui.allocate_ui(egui::vec2(ui.available_width(), body_height), |ui| {
                            // The tree edits a generator body's tree; a
                            // rigged body has no tree to draw and gets its
                            // own editor sections with #1059.
                            let Some(visuals) = live_mut.0.body.visuals_mut() else {
                                ui.label(
                                    egui::RichText::new(
                                        "This avatar wears a rigged body — its editor \
                                         arrives with the wardrobe work (#1059). Re-roll \
                                         to switch back to a generator chassis.",
                                    )
                                    .small()
                                    .weak(),
                                );
                                return;
                            };
                            let mut source = AvatarVisualsTreeSource::new(visuals);
                            // One-shot focus request from an in-world pick
                            // (#823) — same consume-on-draw contract as the
                            // room editor's tree.
                            let request_focus = std::mem::take(pending_tree_focus);
                            draw_generators_tab(
                                ui,
                                &mut source,
                                selected_generator,
                                selected_prim_path,
                                tree_view_state,
                                request_focus,
                                renaming_unused,
                                inventory.as_deref_mut(),
                                audio_editor,
                                &grammar_diag,
                                &mut widget_changed,
                                &mut blob_ctx.selected_element,
                                tree_confirms,
                                &mut toasts,
                                time.elapsed_secs_f64(),
                                &mut undo_labels.slot(crate::ui::shortcuts::EditorKind::Avatar),
                                // Avatars can't grow roads — no stats readout.
                                None,
                                &mut face_pick,
                            );
                        });
                    }
                    AvatarTab::Locomotion => {
                        egui::ScrollArea::vertical()
                            .auto_shrink([true, false])
                            .max_height(body_height)
                            .show(ui, |ui| {
                                // The full-body edit freeze (#814) makes
                                // tuning feel like editing a statue; the
                                // sanctioned preview path existed only in
                                // code comments until #830.
                                ui.label(
                                    egui::RichText::new(
                                        "⏵ Collapse this window (double-click its title \
                                         bar) to test-drive — physics resumes while it's \
                                         collapsed, reopen to keep tuning.",
                                    )
                                    .small()
                                    .weak(),
                                );
                                ui.add_space(4.0);
                                // Master seed for the Idle-motion section's
                                // baseline + ⟲ re-derive: the seed row's
                                // current value when it parses (the footer
                                // synced it to the DID seed on first draw),
                                // else the DID derivation every peer falls
                                // back to for a record without a gait
                                // section.
                                let fallback_seed = seed_row_state
                                    .current_seed()
                                    .or_else(|| {
                                        session
                                            .as_ref()
                                            .map(|s| crate::seeded_defaults::fnv1a_64(&s.did))
                                    })
                                    .unwrap_or_default();
                                let record = &mut live_mut.0;
                                draw_locomotion_tab(
                                    ui,
                                    &mut record.locomotion,
                                    &mut record.gait,
                                    fallback_seed,
                                    &mut widget_changed,
                                    &mut undo_labels.slot(crate::ui::shortcuts::EditorKind::Avatar),
                                );
                            });
                    }
                }
            });

        if live_mut.0 != before {
            widget_changed = true;
        }

        if let Some(response) = response.as_ref() {
            chrome.remember(crate::ui::layout::UiWindow::Avatar, response.response.rect);
        }
        if panels.avatar && !open {
            panels.avatar = false;
        }

        // `Window::show` returns `Some(InnerResponse { inner: None, .. })`
        // when the window is rendered but collapsed (the closure does not
        // fire). `Some(InnerResponse { inner: Some(_), .. })` means the
        // body ran. `None` means the window is closed entirely. Treat
        // collapsed *and* closed identically: the user can no longer see
        // the selection in the panel, so the gizmo should detach and the
        // mutex against the room editor should release.
        response.as_ref().is_some_and(|r| r.inner.is_some())
    };
    // Publish the window state for non-UI readers (the gait pause, #741)
    // every frame this system runs — including the `!panels.avatar` arm,
    // so closing the window un-pauses without a stale frame.
    editor.window_visible = window_visible_with_body;

    // Pop-out audio editor for the per-construct slot on avatar visuals
    // generators — a top-level Window sibling to the Avatar window.
    // Rendered after the Avatar window's borrow of the egui context is
    // released. Slot-agnostic: it stages committed edits in
    // `audio_editor.committed`, which the construct's bridge in the
    // Visuals tab picks up next frame and writes into the live record.
    crate::ui::room::audio::draw_audio_editor_window(
        contexts.ctx_mut().unwrap(),
        &mut editor.audio_editor,
        &audio_monitor,
        &mut audio_requests,
        &mut chrome,
    );

    // Collapse-deselect + tab-switch clear (#1103): a selection only
    // persists while the panel showing it is visible, so the gizmo can
    // detach and the freeze / bind-pose holds release. One helper for all
    // three selections — the part selection (#1098) used to be missing
    // here, which left its gizmo aimed and the body held after the window
    // closed.
    editor.release_hidden_selections(window_visible_with_body);

    // Cross-editor mutex: when this frame's avatar selection rose from
    // None → Some, drop the room editor's selection so only one gizmo is
    // attached at a time. The reverse direction is enforced by the
    // analogous block in `room::room_admin_ui`.
    let now_visuals_selected = editor.has_visuals_selection() || editor.has_attachment_selection();
    if now_visuals_selected
        && !prev_visuals_selected
        && let Some(room) = room_editor.as_deref_mut()
    {
        room.selected_placement = None;
        room.selected_generator = None;
        room.selected_prim_path = None;
        room.tree_view_state.set_selected(Vec::new());
    }

    if widget_changed {
        editor.pending_flush_secs = crate::config::ui::editor::MENU_DEBOUNCE_SECS;
        // Coarse per-tab undo label (#865) when no site named the edit.
        if !undo_labels.avatar_pending() {
            undo_labels.set_avatar(match editor.selected_tab {
                AvatarTab::Body => "body edit",
                AvatarTab::Attachments => "attachment edit",
                AvatarTab::Visuals => "visuals edit",
                AvatarTab::Locomotion => "locomotion edit",
            });
        }
    }
    if editor.pending_flush_secs > 0.0 {
        editor.pending_flush_secs = (editor.pending_flush_secs - time.delta_secs()).max(0.0);
        if editor.pending_flush_secs <= 0.0 {
            // Debounce drained — clamp the accumulated edit through the
            // same bounds the network-ingress path enforces, then publish
            // it to player (visual rebuild) and `broadcast_avatar_state`
            // (peer preview) in a single change tick. The clamp matters:
            // egui's DragValue parses typed `NaN`/`inf` and its range
            // clamp passes NaN through, so an unsanitized flush could
            // hand NaN half-extents straight to the collider builders.
            live.bypass_change_detection().0.sanitize();
            live.set_changed();
        }
    }
}

/// An in-flight wardrobe listing (#1059), landed by
/// [`poll_wardrobe_list_tasks`] onto the editor's cached listing.
#[derive(Component)]
pub struct WardrobeListTask {
    task:
        bevy::tasks::Task<Result<Vec<(String, pds::avatar::EngineAvatarRecord)>, pds::FetchError>>,
}

/// Walk the identity's wardrobe collection for the Body tab's list.
fn spawn_wardrobe_list_task(commands: &mut Commands, did: &str) {
    let did = did.to_string();
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            let client = crate::config::http::default_client();
            pds::avatar::wardrobe::list_wardrobe(&client, &did).await
        };
        #[cfg(target_arch = "wasm32")]
        {
            fut.await
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            crate::config::http::block_on(fut)
        }
    });
    commands.spawn(WardrobeListTask { task });
}

/// Land finished wardrobe listings. A failed walk clears the spinner and
/// leaves whatever list was there — the button is the retry.
pub fn poll_wardrobe_list_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut WardrobeListTask)>,
    mut editor: ResMut<AvatarEditorState>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        commands.entity(entity).despawn();
        let editor = editor.bypass_change_detection();
        editor.wardrobe.fetching = false;
        match result {
            Ok(entries) => editor.wardrobe.entries = Some(entries),
            Err(err) => warn!("wardrobe listing failed: {err:?}"),
        }
    }
}

/// Spawn the async avatar publish. `pub(crate)` because the unsaved-edits
/// guard ([`crate::ui::unsaved_guard`]) drives the same pipeline for its
/// "Publish & log out" path — the shared [`poll_publish_avatar_tasks`]
/// system lands the result either way.
///
/// A rigged record is a **bundle** (#1059): the wardrobe body and every
/// worn attachment land before the profile and the avatar record that
/// reference them, and detached records are deleted last, so no reader ever
/// resolves a dangling reference. A generator record is exactly the classic
/// single-record save it always was — [`pds::avatar::wardrobe::plan_avatar_publish`]
/// decides which by looking at the body.
///
/// `stored_attachments` is the attachment rkey list of the record the PDS
/// currently holds — [`StoredAvatarRecord`], or empty when nothing has been
/// fetched. The plan retires exactly what that set has and `record` no
/// longer references (#1110), so every caller must pass it: handing over an
/// empty list where a stored record exists silently orphans whatever this
/// save takes off.
pub(crate) fn spawn_publish_avatar_task(
    commands: &mut Commands,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    record: AvatarRecord,
    stored_attachments: Vec<String>,
    now: f64,
) {
    // The avatar record is always the local user's own, saved to their PDS, so
    // the write DID is the session DID (unlike a room save, whose DID is the
    // room owner's `CurrentRoomDid`).
    let did = session.did.clone();
    let session_clone = session.clone();
    let refresh_clone = refresh.clone();
    let record_bytes = pds::record_size::serialized_record_bytes(&record);
    let published = record.clone();
    // The engine crate is clock-free by design (std::time panics on wasm),
    // so the ISO timestamp the wardrobe lexicon requires is stamped here —
    // chrono is already the app's wasm-safe clock (#846).
    let now_iso = chrono::Utc::now().to_rfc3339();
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            let client = crate::config::http::default_client();
            let plan =
                pds::avatar::wardrobe::plan_avatar_publish(&record, &stored_attachments, &now_iso);
            pds::avatar::wardrobe::publish_avatar_bundle(
                &client,
                &session_clone,
                &refresh_clone,
                &plan,
            )
            .await
        };
        #[cfg(target_arch = "wasm32")]
        {
            fut.await
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            crate::config::http::block_on(fut)
        }
    });
    commands.spawn(PublishAvatarTask {
        task,
        did,
        spawned_at: now,
        record_bytes,
        published,
    });
}

/// Poll outstanding avatar publish tasks. On success, sync the record the
/// task actually published into `StoredAvatarRecord` so the "Load from PDS"
/// button is disabled until the next edit.
///
/// Deliberately does not read `LiveAvatarRecord` (#1116) — see
/// [`PublishAvatarTask::published`].
#[allow(clippy::too_many_arguments)]
pub fn poll_publish_avatar_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PublishAvatarTask)>,
    mut stored: Option<ResMut<StoredAvatarRecord>>,
    mut feedback: ResMut<PublishFeedback<AvatarRecord>>,
    mut session_log: ResMut<SessionLog>,
    mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
    time: Res<Time>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.task))
        else {
            continue;
        };
        commands.entity(entity).despawn();
        let now = time.elapsed_secs_f64();
        let did = task.did.clone();
        let duration_secs = now - task.spawned_at;
        crate::ui::editable::log_record_size(
            &mut session_log,
            &mut metrics,
            now,
            RecordKind::Avatar,
            task.record_bytes,
        );
        match result {
            Ok(()) => {
                info!("Avatar record saved to PDS");
                if let Some(stored) = stored.as_mut() {
                    stored.0 = task.published.clone();
                }
                feedback.status = PublishStatus::Success { at_secs: now };
                session_log.info(
                    now,
                    EventPayload::RecordWriteCompleted {
                        record: RecordKind::Avatar,
                        did,
                        duration_secs,
                    },
                );
            }
            Err(e) => {
                warn!("Failed to save avatar record: {}", e);
                session_log.error(
                    now,
                    EventPayload::RecordWriteFailed {
                        record: RecordKind::Avatar,
                        did,
                        reason: e.clone(),
                    },
                );
                feedback.status = PublishStatus::Failed {
                    at_secs: now,
                    message: e,
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1103 (owner direction, reversing #814): the chassis freeze and the
    /// gait/sway hold engage only while a gizmo is aimed at the avatar or
    /// at something it wears — never merely because the window is open.
    /// Each of the three selections holds; an open window with nothing
    /// aimed does not.
    #[test]
    fn the_freeze_holds_only_while_a_gizmo_is_aimed() {
        let mut state = AvatarEditorState::default();
        assert!(!state.holds_avatar_still());

        state.window_visible = true;
        assert!(
            !state.holds_avatar_still(),
            "an open window with nothing aimed leaves the body live"
        );

        state.select_from_scene_pick(vec![0]);
        assert!(state.holds_avatar_still(), "a visuals row holds");
        state.select_attachment_from_scene_pick(String::from("3jzfcijpj2z2a"));
        assert!(state.holds_avatar_still(), "a worn prop holds");
        state.select_attachment_part_from_scene_pick(String::from("3jzfcijpj2z2a"), vec![1]);
        assert!(state.holds_avatar_still(), "a worn-prop part holds");

        // A selection lingering into the close-frame gap still holds, so a
        // drag released as the window goes lands against a still chassis.
        state.window_visible = false;
        assert!(state.holds_avatar_still());
        state.release_hidden_selections(false);
        assert!(!state.holds_avatar_still());
    }

    /// #1103 bug 1, reproduced: closing (or collapsing) the Avatar window
    /// while a worn item's PART is selected left the gizmo aimed and the
    /// bind pose held, because the collapse-deselect only knew about the
    /// visuals and whole-prop selections. The release must leave no gizmo
    /// target and no hold behind, for every selection kind.
    #[test]
    fn closing_the_window_releases_every_gizmo_selection() {
        let room = crate::ui::room::RoomEditorState::default();
        let rkey = String::from("3jzfcijpj2z2a");
        let selections: [fn(&mut AvatarEditorState, &str); 3] = [
            |s, _| s.select_from_scene_pick(vec![0]),
            |s, k| s.select_attachment_from_scene_pick(k.to_string()),
            |s, k| s.select_attachment_part_from_scene_pick(k.to_string(), vec![0]),
        ];
        for select in selections {
            let mut state = AvatarEditorState {
                window_visible: true,
                ..Default::default()
            };
            select(&mut state, &rkey);
            assert_ne!(
                crate::editor_gizmo::determine_active_target(&room, &state),
                crate::editor_gizmo::ActiveTarget::None,
                "the selection aims a gizmo"
            );

            state.window_visible = false;
            state.release_hidden_selections(false);
            assert_eq!(
                crate::editor_gizmo::determine_active_target(&room, &state),
                crate::editor_gizmo::ActiveTarget::None,
                "closing the window detaches the gizmo"
            );
            assert!(!state.holds_avatar_still(), "the chassis freeze released");
            assert!(!state.holds_rig_at_rest(), "the bind pose released");
            assert!(!state.holds_rig_pose(), "the pose hold released");
        }
    }

    /// #1103 bug 3, reproduced: a left-click into empty scene while a
    /// worn item's part was selected kept the part gizmo up — the miss
    /// path cleared the other two selections only. Also pins the
    /// face-pick exemption, which protects the visuals row alone.
    #[test]
    fn a_scene_miss_releases_the_prop_and_part_gizmos() {
        let rkey = String::from("3jzfcijpj2z2a");
        let mut state = AvatarEditorState::default();
        state.select_attachment_part_from_scene_pick(rkey.clone(), vec![0]);
        state.release_on_scene_miss(false);
        assert!(!state.has_part_selection(), "the part gizmo let go");
        assert!(!state.holds_rig_at_rest());
        assert_eq!(
            state.editing_parts(),
            Some(rkey.as_str()),
            "the parts editor stays open, like a World-editor tab"
        );

        state.select_attachment_from_scene_pick(rkey.clone());
        state.release_on_scene_miss(true);
        assert!(
            !state.has_attachment_selection(),
            "face picking never aims at a prop"
        );

        state.select_from_scene_pick(vec![0]);
        state.release_on_scene_miss(true);
        assert!(
            state.has_visuals_selection(),
            "an armed face pick keeps its row"
        );
        state.release_on_scene_miss(false);
        assert!(!state.has_visuals_selection());
    }

    /// Leaving the Attachments tab drops the part selection with the
    /// whole-prop one (#1103): a gizmo belongs to the tab that shows it.
    #[test]
    fn leaving_the_attachments_tab_drops_the_part_gizmo() {
        let mut state = AvatarEditorState {
            window_visible: true,
            ..Default::default()
        };
        state.select_attachment_part_from_scene_pick(String::from("3jzfcijpj2z2a"), vec![0]);
        state.selected_tab = AvatarTab::Body;
        state.release_hidden_selections(true);
        assert!(!state.has_part_selection());
        assert!(!state.holds_rig_at_rest());
    }

    /// #1062: the two avatar-side gizmo targets are mutually exclusive, and
    /// both scene-pick entry points enforce it. Without this the gizmo
    /// dispatch has two live selections to choose between and the loser's
    /// row stays highlighted over a gizmo it does not own.
    #[test]
    fn a_prop_pick_and_a_visuals_pick_take_the_gizmo_from_each_other() {
        let mut state = AvatarEditorState::default();

        state.select_from_scene_pick(vec![0, 1]);
        state.select_attachment_from_scene_pick(String::from("3jzfcijpj2z2a"));
        assert_eq!(state.selected_attachment(), Some("3jzfcijpj2z2a"));
        assert!(!state.has_visuals_selection(), "the visuals row let go");
        assert_eq!(
            state.selected_tab,
            AvatarTab::Attachments,
            "the pick brings forward the tab that can show it"
        );
        assert!(state.pending_attachment_focus, "focus request armed");

        state.select_from_scene_pick(vec![2]);
        assert!(!state.has_attachment_selection(), "the prop let go");
        assert!(state.has_visuals_selection());
    }

    /// #1062 → #1103 → #1106: an attachment offset is stored in its
    /// carrying joint's rest frame, so a gizmo aimed at a WHOLE worn prop
    /// pins the body to its bind pose — and only then. A PART gizmo holds
    /// the pose as it stands instead (selecting must not move anything);
    /// the tab being open is not a hold (owner direction), and a
    /// visuals-row gizmo is neither.
    #[test]
    fn the_bind_pose_hold_follows_the_prop_gizmo_and_the_pose_hold_the_part_gizmo() {
        let mut state = AvatarEditorState {
            window_visible: true,
            ..Default::default()
        };
        state.selected_tab = AvatarTab::Attachments;
        assert!(!state.holds_rig_at_rest(), "an open tab is not a hold");
        assert!(!state.holds_rig_pose());

        state.select_attachment_from_scene_pick(String::from("3jzfcijpj2z2a"));
        assert!(
            state.holds_rig_at_rest(),
            "a whole-prop gizmo pins the bind pose"
        );
        assert!(!state.holds_rig_pose());

        state.select_attachment_part_from_scene_pick(String::from("3jzfcijpj2z2a"), vec![0]);
        assert!(
            !state.holds_rig_at_rest(),
            "a part gizmo must NOT re-pose the body (#1106)"
        );
        assert!(
            state.holds_rig_pose(),
            "a part gizmo holds the pose as it stands"
        );

        state.select_from_scene_pick(vec![0]);
        assert!(
            !state.holds_rig_at_rest(),
            "a visuals gizmo is not a bind-pose hold"
        );
        assert!(!state.holds_rig_pose());
    }

    /// #823: a scene pick must land the full row-click state — selection
    /// set to the picked path under the fixed "visuals" root, the row
    /// selected in the tree widget, every ancestor expanded, and the
    /// one-shot focus request armed (then consumed by the next draw).
    #[test]
    fn scene_pick_selects_expands_and_arms_focus() {
        let mut state = AvatarEditorState::default();
        state.select_from_scene_pick(vec![1, 0, 2]);

        assert_eq!(
            state.selected_generator.as_deref(),
            Some(AvatarVisualsTreeSource::ROOT_NAME)
        );
        assert_eq!(state.selected_prim_path, Some(vec![1, 0, 2]));
        assert!(state.has_visuals_selection());
        assert!(state.pending_tree_focus, "focus request armed");

        // The tree widget mirrors the selection...
        let selected_id = GenNodeId::child(
            AvatarVisualsTreeSource::ROOT_NAME.to_string(),
            vec![1, 0, 2],
        );
        assert_eq!(state.tree_view_state.selected(), &vec![selected_id]);
        // ...and every ancestor (root, [1], [1,0]) is explicitly opened
        // so the picked row is actually visible.
        for depth in 0..3 {
            let ancestor = GenNodeId::child(
                AvatarVisualsTreeSource::ROOT_NAME.to_string(),
                vec![1, 0, 2][..depth].to_vec(),
            );
            assert_eq!(
                state.tree_view_state.is_open(&ancestor),
                Some(true),
                "ancestor at depth {depth} expanded"
            );
        }
    }
}
