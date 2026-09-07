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
//! place, and the player module rebuilds visuals or swaps locomotion the
//! same frame the resource changes.
//!
//! **What peers see, and when (#1122).** `network::broadcast_avatar_state`
//! pushes the record to the room on every edit, and for a **generator**
//! body that record IS the payload — peers do see the edit before the
//! author commits. A **rigged** body is different and the contract here is
//! deliberate: its payload lives in separate wardrobe and attachment
//! records, and the broadcast can only carry their rkeys (`resolved` is
//! `serde(skip)`), so a peer resolves those names against the owner's PDS
//! and renders the owner's last SAVED body. A sculpt or an offset nudge
//! therefore reaches the room when it is saved, not while it is being
//! dragged; [`poll_publish_avatar_tasks`] broadcasts
//! `AvatarRecordsPublished` at that moment so peers re-resolve instead of
//! sitting on the pre-save body indefinitely.
//!
//! This module used to claim the unqualified "peers see the edit before
//! the author commits", which was false for every rigged body in the app.
//! The alternative — inlining the resolved records in the broadcast — was
//! weighed and declined: it would put an unsigned, peer-authored body on
//! screen where every other avatar comes from its owner's repo, re-send a
//! multi-hundred-KiB payload per preview burst on the ordered reliable
//! channel, and can exceed the 900 KiB wire ceiling, past which the send is
//! refused in console silence (#1123, open). It remains available if the
//! preview fidelity is ever worth those three.
//!
//! Three explicit buttons drive persistence and discard flows:
//!
//!   * **Save** writes the current `LiveAvatarRecord` to the
//!     owner's PDS via `com.atproto.repo.putRecord` and then syncs the
//!     value into [`StoredAvatarRecord`] on success.
//!   * **Revert to saved** drops all in-flight edits by copying
//!     [`StoredAvatarRecord`] back into `LiveAvatarRecord`.
//!   * **Reset to default** replaces `LiveAvatarRecord` with the canonical
//!     [`AvatarRecord::default_for_did`] seed.

mod attachments;
pub(crate) use attachments::{
    attach_record, is_worn_from, record_for_inventory_item, rename_worn_source,
    save_worn_to_inventory, take_off_rkey, take_off_source, wear_blocked_reason, worn_rkeys_from,
};
mod body;
mod locomotion;
mod target;
pub use target::GizmoTarget;

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
    /// The avatar editor's one-node clipboard (#1244 f422) — the same
    /// Copy / Paste-as-child gesture the room's tree gained, since both
    /// draw the identical tree panel.
    node_clipboard: Option<crate::pds::Generator>,
    /// The one thing the 3-D gizmo is aimed at (#1161) — a visuals node,
    /// a whole worn prop, or one part of one. Three parallel `Option`
    /// fields lived here until the exclusivity between them became a
    /// property of the type; see [`GizmoTarget`], and [`Self::aim`] for
    /// the single place it is written.
    ///
    /// The tree widgets do not read it directly: they speak the
    /// `(root, path)` pair [`draw_generators_tab`] takes, which
    /// [`avatar_ui`] seeds from the aim before the draw and folds back
    /// after it. Only the tree that is actually on screen may speak for
    /// the aim.
    gizmo: GizmoTarget,
    /// The Visuals tab's generator tree (#1161) — the same
    /// [`TreePanelState`](crate::ui::room::generators::TreePanelState) the
    /// room editor and the parts editor each own one of. Its `selection` is
    /// the widget's I/O, not the truth: it is seeded from [`Self::gizmo`]
    /// before each draw and folded back after.
    ///
    /// This replaced four loose fields, one of which — `renaming_unused` —
    /// existed only because [`draw_generators_tab`] demanded a `&mut` for a
    /// rename that a single-root source can never offer.
    visuals_tree: crate::ui::room::generators::TreePanelState,
    /// Seconds remaining before a pending widget change is flushed into
    /// `LiveAvatarRecord`'s change tick. The downstream player rebuild
    /// and `network::broadcast_avatar_state` peer broadcast fire once
    /// when the timer drains rather than every frame.
    pending_flush_secs: f32,
    /// Pop-out audio editor state for the per-construct audio slot on
    /// avatar visuals generators. Shares the same widget as the room
    /// editor; see [`crate::ui::room::audio::AudioEditorState`].
    pub(crate) audio_editor: crate::ui::room::audio::AudioEditorState,
    /// The manual re-roll block (#1005): the seed row's buffer, the pinned
    /// axes and the memoized hunt over the two — the room editor's
    /// identical trio, which is why it is one generic type. See
    /// [`crate::ui::editable::ReRollState`].
    reroll: crate::ui::editable::ReRollState<crate::seeded_defaults::AvatarPins>,
    /// Pending publish-after-unrecoverable-fetch confirmation (#840):
    /// while [`crate::state::AvatarRecordRecovery`] is present the
    /// editor holds the default, and saving would overwrite the real
    /// stored record — the first publish asks first.
    publish_guard: crate::ui::confirm::ConfirmState<()>,
    /// Cached seeded-default record, keyed by the DID it was built for (#637).
    /// `AvatarRecord::default_for_did` runs the full part-composition pipeline,
    /// so build it once per session rather than every frame the editor is open;
    /// invalidated when the session DID changes.
    /// The third element is the record's serialized form, pre-baked for
    /// the per-frame `can_reset` comparison — the room editor's #674
    /// idiom, which #1135's doc comment claimed had reached this editor
    /// and which #1270 f273 found had not.
    default_cache: Option<(String, AvatarRecord, Option<serde_json::Value>)>,
    /// Serialized form of the stored record for the per-frame dirty check
    /// (#1270 f273), recomputed only when the stored resource changes.
    /// Keyed by `last_changed()` rather than `is_changed()` for the same
    /// reason as the room's: the change flag is consumed on frames where
    /// this system early-returns (a closed panel, a body-less session),
    /// which would otherwise leave a stale baseline behind.
    stored_baseline: Option<(bevy::ecs::change_detection::Tick, Option<serde_json::Value>)>,
    /// Serialized form of the LIVE record, rebuilt only when the record
    /// could have changed (#1270 f273). Before this the editor asked
    /// `avatar_is_dirty` three times a frame, each of which serialised
    /// BOTH sides — six whole-record `Value` trees per frame on the
    /// surface where the owner spends the longest continuous stretch of
    /// fine-grained interaction, and the one editor #674's caching never
    /// reached.
    live_baseline: crate::ui::perf::LiveValueCache,
    /// [`Self::live_baseline`]'s rebuild count as of the last size
    /// measurement, so the 0.5 s readout can skip a record that has not
    /// changed since it last looked (#1270 f418's gate).
    size_readout_generation: Option<u64>,
    /// Mirror of this frame's "Avatar window is open and un-collapsed"
    /// state, written by [`avatar_ui`] so non-UI systems can read it
    /// without reaching into egui. Since #1103 the freeze gates no longer
    /// key on it — only on an aimed gizmo — but it still decides when the
    /// selections are released ([`Self::release_hidden_selections`]):
    /// collapsing the window counts as closed.
    window_visible: bool,
    /// The fetched cross-app wardrobe listing (#1059), refreshed on demand
    /// from the Body tab rather than on open: it is a `listRecords` walk of
    /// someone's whole avatar collection.
    wardrobe: body::WardrobeListing,
    /// Attachment picker state (item + socket) across frames.
    attachments: attachments::AttachmentsTabState,
    /// Set for one frame when an in-world pick (#1062) selects a worn prop,
    /// so the next Attachments draw force-opens that row and scrolls to it.
    /// The attachment-tab twin of the trees' own `pending_focus`.
    pending_attachment_focus: bool,
    /// Which worn prop's PARTS editor is open in the Attachments tab
    /// (#1098), by record key: the tab then shows that item's generator
    /// tree — the region-asset editor over the worn copy — instead of the
    /// worn list. `None` = the list.
    editing_parts: Option<String>,
    /// The parts editor's own tree (#1098) — separate from the visuals
    /// tree's so a body's expanded rows survive editing a prop.
    parts_tree: crate::ui::room::generators::TreePanelState,
}

/// [`AvatarEditorState::aim`]'s body, over field references rather than
/// `&mut self`.
///
/// [`avatar_ui`] destructures the resource into per-field `&mut`s for the
/// whole draw — that is how the tab arms edit unrelated fields at once —
/// so it cannot call a method on the struct. Rather than let the tab arms
/// assign the aim raw, they call this: **one body, two entry points**, so
/// the tree-row release cannot be forgotten on the side that does most of
/// the aiming.
fn aim_in_place(
    gizmo: &mut GizmoTarget,
    visuals_tree: &mut egui_ltreeview::TreeViewState<GenNodeId>,
    part_tree: &mut egui_ltreeview::TreeViewState<GenNodeId>,
    target: GizmoTarget,
) {
    if gizmo.visuals_path().is_some() && target.visuals_path().is_none() {
        visuals_tree.set_selected(Vec::new());
    }
    if gizmo.worn_part().is_some() && target.worn_part().is_none() {
        part_tree.set_selected(Vec::new());
    }
    *gizmo = target;
}

impl AvatarEditorState {
    /// What the gizmo is aimed at (#1161). The `editor_gizmo` dispatch,
    /// the sync pass and the highlight pass all match on this rather than
    /// asking three separate questions and hoping at most one says yes.
    pub fn gizmo(&self) -> &GizmoTarget {
        &self.gizmo
    }

    /// **The** enforcement point for "one gizmo target at a time".
    ///
    /// The aim is a single field, so the assignment *is* the mutex — the
    /// three parallel `Option`s each `select_*` used to clear by hand are
    /// gone, and with them the class of bug where a new selection kind was
    /// added to some of the clearing paths and not others (#1103 bugs 1
    /// and 3). What this still has to do is drop the **tree-row
    /// highlight** of the target being left behind: that is widget state
    /// living beside the aim rather than in it, and a row left highlighted
    /// over a gizmo it no longer owns is exactly the #1062 symptom.
    fn aim(&mut self, target: GizmoTarget) {
        aim_in_place(
            &mut self.gizmo,
            &mut self.visuals_tree.view,
            &mut self.parts_tree.view,
            target,
        );
    }

    /// True when a visuals row is currently selected. The locomotion
    /// freeze gate and the gizmo dispatch read this.
    ///
    /// Note the asymmetry it is named for: this is **one of three** gizmo
    /// selections, so it is the wrong question for anything that means
    /// "is a gizmo aimed at the avatar" — see [`Self::has_gizmo_selection`],
    /// and #1236 f139 for the Esc ladder that asked this one and skipped
    /// its rung on a worn prop.
    pub fn has_visuals_selection(&self) -> bool {
        self.gizmo.visuals_path().is_some()
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
        let Some(aimed) = self.gizmo.worn_prop().map(str::to_owned) else {
            return;
        };
        if rkeys.into_iter().any(|rkey| rkey == aimed) {
            self.aim(GizmoTarget::None);
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
        self.gizmo.worn_prop().is_some()
    }

    /// True while the local rigged body must be held **exactly where it is**
    /// (#1106): a gizmo is aimed at a part of a worn item. The part is
    /// detached to world space at its current pose and committed back
    /// against its parent's pose, so the parent must not move — but it may
    /// stand in any pose at all. Selecting must never change a transform,
    /// so this is a pause, not a re-pose: the driver skips the body and the
    /// last pose stays applied. Read by [`crate::player`]'s rigged motion
    /// driver, after [`Self::holds_rig_at_rest`] — which the enum now makes
    /// provably exclusive with this rather than merely conventionally so.
    pub fn holds_rig_pose(&self) -> bool {
        self.gizmo.worn_part().is_some()
    }

    /// Select a worn prop from an in-world scene pick (#1062), the
    /// attachment-tab counterpart of [`Self::select_from_scene_pick`]: the
    /// tab that can show it comes forward, the row is selected, and a
    /// one-shot focus request is armed so the next draw opens and scrolls to
    /// it. Any other aim goes, because only one gizmo target exists.
    pub fn select_attachment_from_scene_pick(&mut self, rkey: String) {
        self.selected_tab = AvatarTab::Attachments;
        // The whole-prop selection lives on the worn LIST; a parts editor
        // that happens to be open steps aside.
        self.editing_parts = None;
        self.aim(GizmoTarget::WornProp { rkey });
        self.pending_attachment_focus = true;
    }

    /// Land the editor on the Body tab (#1097) — the scene menu's "Edit
    /// avatar" on one's own rigged body, which has no visuals node to
    /// select. Drops the aim so no gizmo is left up.
    pub fn open_body_tab(&mut self) {
        self.selected_tab = AvatarTab::Body;
        self.aim(GizmoTarget::None);
    }

    /// The worn prop whose parts editor is open (#1098).
    pub fn editing_parts(&self) -> Option<&str> {
        self.editing_parts.as_deref()
    }

    /// Open the parts editor on a worn prop (#1098): the Attachments tab
    /// comes forward showing that item's tree, with the item ROOT selected
    /// so a gizmo is aimed immediately. The whole-prop offset selection
    /// goes with it — one gizmo target at a time.
    pub fn open_parts_editor(&mut self, rkey: String) {
        self.selected_tab = AvatarTab::Attachments;
        self.editing_parts = Some(rkey.clone());
        self.select_attachment_part(rkey, Vec::new());
    }

    /// Back from the parts editor to the worn list. Drops the part
    /// selection with it — but leaves a differently-aimed gizmo alone,
    /// since closing this panel says nothing about one.
    pub fn close_parts_editor(&mut self) {
        self.editing_parts = None;
        if self.gizmo.worn_part().is_some() {
            self.aim(GizmoTarget::None);
        }
    }

    /// Select a part of a worn item (#1098) — from the parts tree or a
    /// scene pick. Takes the aim from whatever held it and mirrors the
    /// choice into the tree-view state so the row highlights.
    pub fn select_attachment_part(&mut self, rkey: String, path: Vec<usize>) {
        self.aim(GizmoTarget::WornPart {
            rkey: rkey.clone(),
            path: path.clone(),
        });
        for depth in 0..path.len() {
            self.parts_tree
                .view
                .set_openness(GenNodeId::child(rkey.clone(), path[..depth].to_vec()), true);
        }
        self.parts_tree
            .view
            .set_selected(vec![GenNodeId::child(rkey, path)]);
    }

    /// A scene pick on a part of a worn item (#1098): opens that prop's
    /// parts editor if it is not already open, selects the part, and arms
    /// the one-shot tree focus.
    pub fn select_attachment_part_from_scene_pick(&mut self, rkey: String, path: Vec<usize>) {
        self.selected_tab = AvatarTab::Attachments;
        self.editing_parts = Some(rkey.clone());
        self.select_attachment_part(rkey, path);
        self.parts_tree.pending_focus = true;
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
    /// releases the aim ([`Self::release_hidden_selections`]), so a closed
    /// window never holds. A lingering selection still holds until that
    /// release runs, so a drag released as the window goes cannot land
    /// against a moving chassis.
    pub fn holds_avatar_still(&self) -> bool {
        self.has_gizmo_selection()
    }

    /// True while any of the three avatar-side gizmo selections is live
    /// (visuals row, worn prop, worn-prop part) — the one question the
    /// freeze gates and the release paths ask.
    pub fn has_gizmo_selection(&self) -> bool {
        self.gizmo.is_aimed()
    }

    /// Drop the gizmo selection. The parts editor stays open (like a
    /// World-editor tab, it remembers where it was); only the aim goes,
    /// and with it the freeze and the bind-pose hold.
    pub fn clear_gizmo_selections(&mut self) {
        self.aim(GizmoTarget::None);
    }

    /// The end-of-frame release rule (#1103), applied by [`avatar_ui`]
    /// after the window has drawn (or not): **an aim only persists while
    /// the panel showing it is visible.**
    ///
    /// * Window hidden or collapsed → the aim goes, so the gizmo detaches
    ///   and the chassis / bind-pose holds release — the World editor's
    ///   close contract. Before #1103 the part selection survived the
    ///   close and kept both holds engaged.
    /// * Otherwise the aim survives only on the tab that can show it: the
    ///   visuals row on Visuals, the worn prop and the part on Attachments
    ///   (#1062). The editor never gizmo-edits Locomotion or Body.
    ///
    /// This used to be three conditionals over three fields, and each new
    /// selection kind had to be added to the right one. It is now one
    /// question asked of the aim itself, which is why a fourth variant
    /// cannot silently skip it.
    pub fn release_hidden_selections(&mut self, window_visible: bool) {
        let tab_can_show_it = match self.gizmo {
            GizmoTarget::None => true,
            GizmoTarget::VisualsNode { .. } => self.selected_tab == AvatarTab::Visuals,
            GizmoTarget::WornProp { .. } | GizmoTarget::WornPart { .. } => {
                self.selected_tab == AvatarTab::Attachments
            }
        };
        if !window_visible || !tab_can_show_it {
            self.aim(GizmoTarget::None);
        }
    }

    /// A left-click into the scene that hit nothing of the local avatar's
    /// (#1103, the World editor's contract): the aim lets go. The one
    /// exemption is a visuals row while face picking is armed, because an
    /// armed pick is aiming at *something* and a miss must not close the
    /// panel it is aimed from. Face picking never aims at a prop, so it
    /// does not exempt those.
    pub fn release_on_scene_miss(&mut self, face_pick_armed: bool) {
        if face_pick_armed && self.gizmo.visuals_path().is_some() {
            return;
        }
        self.aim(GizmoTarget::None);
    }

    /// Snapshot the selection state an undo entry carries (#862) so a
    /// restore (#863) can re-seed it instead of dumping the user to a
    /// full deselect.
    ///
    /// Only the visuals aim is carried, which is what the avatar's undo
    /// history covers: the tree row it names is the one an undone edit can
    /// invalidate. A worn-prop aim is validated separately in
    /// [`Self::restore_from_undo`], against the restored record's own
    /// attachment list.
    pub(crate) fn undo_selection(&self) -> crate::ui::undo::AvatarSelection {
        crate::ui::undo::AvatarSelection {
            generator: self
                .gizmo
                .visuals_path()
                .map(|_| AvatarVisualsTreeSource::ROOT_NAME.to_string()),
            prim_path: self.gizmo.visuals_path().map(<[usize]>::to_vec),
            tree: self.visuals_tree.view.selected().clone(),
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
        self.visuals_tree.confirms.delete.cancel();
        self.visuals_tree.confirms.kind.cancel();
        // A pending burst was aimed at pre-restore state; draining it
        // would double-fire `set_changed` and mint a phantom entry.
        self.pending_flush_secs = 0.0;
        // A worn prop the restored record no longer wears cannot host a
        // gizmo (#1062); one it still wears keeps its aim.
        if let Some(rkey) = self.gizmo.worn_prop() {
            let still_worn = record
                .body
                .rigged_ref()
                .and_then(|rig| rig.resolved.as_ref())
                .is_some_and(|resolved| resolved.attachments.iter().any(|a| a.rkey == rkey));
            if !still_worn {
                self.aim(GizmoTarget::None);
            }
        }
        match &sel.prim_path {
            // `select_from_scene_pick` is exactly the fixup contract:
            // aim set, ancestors expanded, row selected + focused.
            Some(path)
                if record
                    .body
                    .visuals()
                    .is_some_and(|v| crate::ui::undo::restore::node_path_valid(v, path)) =>
            {
                self.select_from_scene_pick(path.clone());
            }
            // Root ROW selected without a node path: keep the row — the
            // single visuals root always exists — but it aims no gizmo,
            // which is what a `None` path meant before the aim was a type.
            None if sel.generator.is_some() => {
                self.release_visuals_aim();
                self.visuals_tree.view.set_selected(vec![GenNodeId::root(
                    AvatarVisualsTreeSource::ROOT_NAME.to_string(),
                )]);
            }
            _ => self.release_visuals_aim(),
        }
    }

    /// Let go of the aim if — and only if — it is on a visuals node.
    ///
    /// Two callers, both meaning "this says nothing about a worn prop's
    /// gizmo, so do not take one down with it": the undo restore's two
    /// tree re-seed paths, and the room editor's half of the cross-editor
    /// mutex (`room::room_admin_ui`), which takes the gizmo from an avatar
    /// visuals row but has never claimed a worn prop's.
    pub(crate) fn release_visuals_aim(&mut self) {
        if self.gizmo.visuals_path().is_some() {
            self.aim(GizmoTarget::None);
        }
    }

    /// Select a visuals node from an in-world scene pick (#823), exactly
    /// as if its tree row had been clicked: aim set, every ancestor
    /// expanded (the tree collapses by default, so the picked row must be
    /// revealed), the row marked selected in the tree widget, and a
    /// one-shot focus request armed so the row gets the bright focused
    /// highlight on the next draw. Mirrors the room editor's pick path in
    /// `editor_gizmo::pick_on_scene_click`.
    pub fn select_from_scene_pick(&mut self, path: Vec<usize>) {
        self.aim(GizmoTarget::VisualsNode { path: path.clone() });
        let root = AvatarVisualsTreeSource::ROOT_NAME.to_string();
        for depth in 0..path.len() {
            self.visuals_tree
                .view
                .set_openness(GenNodeId::child(root.clone(), path[..depth].to_vec()), true);
        }
        self.visuals_tree
            .view
            .set_selected(vec![GenNodeId::child(root, path)]);
        self.visuals_tree.pending_focus = true;
    }
}

/// Publish [`crate::player::RigHold`] from this frame's editor state
/// (#1158) — the ONE writer of that resource.
///
/// The player systems used to read `AvatarEditorState` themselves, which
/// pointed the dependency arrow from the physics and animation drivers
/// into the egui layer. They read four booleans out of it, so four
/// booleans is what crosses now; the predicates and their reasoning
/// (#1103, #1106) stay here, beside the selection state that answers them.
///
/// Runs unconditionally rather than inside `avatar_ui`: the panel draws
/// only while it is open, and a hold that stopped being republished the
/// moment the window closed would latch at its last value. With no editor
/// state at all — before login, and in the headless render tool — every
/// field stays `false`, which is what the old `Option<Res<…>>` degraded
/// to.
pub fn mirror_rig_hold(
    editor: Option<Res<AvatarEditorState>>,
    mut hold: ResMut<crate::player::RigHold>,
) {
    let next = editor
        .as_deref()
        .map_or(crate::player::RigHold::default(), |e| {
            crate::player::RigHold {
                at_rest: e.holds_rig_at_rest(),
                pose: e.holds_rig_pose(),
                still: e.holds_avatar_still(),
                visuals_row: e.has_visuals_selection(),
            }
        });
    // Guarded write (#879): an unconditional `*hold = next` would mark the
    // resource changed every frame.
    if *hold != next {
        *hold = next;
    }
}

/// Why this tab is a dead end on this body kind, or `None` (#1256 f100).
///
/// Exactly one tab is a dead end at any time — never two, and never the Body
/// tab, which on a generator body is the feature's ENTRY POINT (three
/// sentences of explanation and a working "Wear a rigged body" button)
/// rather than a no-op. Body and Visuals are two exclusive body KINDS, and
/// the tab bar used to hide that model behind a click: four undifferentiated
/// `selectable_label`s, one of which answered "that's for the other kind of
/// body".
fn tab_disabled_reason(tab: AvatarTab, rigged: bool) -> Option<&'static str> {
    match tab {
        AvatarTab::Visuals if rigged => Some(
            "Visuals edits a construction-kit body; you're wearing a rigged one. \
             Sculpt it on the Body tab.",
        ),
        AvatarTab::Attachments if !rigged => Some(
            "Wearables dress a rigged body; you're wearing a construction-kit one. \
             Switch on the Body tab first.",
        ),
        _ => None,
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
        movement,
        mut asset_caches,
        local_body,
    ): (
        Res<bevy_symbios_audio::ui::AudioMonitor>,
        MessageWriter<bevy_symbios_audio::ui::MonitorRequest>,
        Res<Time>,
        ResMut<SessionLog>,
        ResMut<crate::editor_gizmo::BlobEditContext>,
        Res<crate::world_builder::grammar_diag::GrammarDiagnostics>,
        Option<Res<crate::state::AvatarRecordRecovery>>,
        ResMut<crate::notify::Toasts>,
        Res<crate::ui::undo::AvatarUndoHistory>,
        ResMut<crate::ui::undo::UndoShortcut>,
        ResMut<crate::ui::undo::PendingUndoLabels>,
        ResMut<crate::editor_gizmo::FacePick>,
        Res<crate::player::LocalMovement>,
        // The asset caches (#1246): a worn item's Sign faces are fetched
        // through the same cache the room's are, so the Parts editor gets
        // the same status lines rather than a second, silent copy of the
        // tree.
        crate::ui::room::assets::AssetCaches,
        // What the body standing in the world knows (#1255, #1256): whether
        // its last build failed, where each worn prop really sits, and which
        // sockets this rig has. Queries rather than editor state, because
        // every one of those facts belongs to the player module that owns
        // it — the editor is a reader, not the owner.
        attachments::LocalBody,
    ),
    // Who is here (#1269 f293). A construction-kit body is broadcast as it
    // is sculpted; a rigged one is not — see `audience_notice` below,
    // which is the only place either fact is stated. A free parameter, not
    // a member of the tuple above: that tuple is at Bevy's 16-param
    // `IntoSystem` ceiling and a seventeenth member fails to compile with
    // an error that names `Curve`, not this.
    peers: Query<(), With<crate::state::RemotePeer>>,
) {
    // `ResMut::deref_mut` unconditionally flips the change tick, so
    // mutating `live.0` inside the egui closure would otherwise mark the
    // resource changed every frame the editor is visible — and
    // `network::broadcast_avatar_state` turns that into a peer broadcast
    // storm. Route UI access through `bypass_change_detection` and call
    // `live.set_changed()` explicitly below, only after the debounce
    // timer drains.
    let mut widget_changed = false;
    // The owner every catalogue stamp is personalised for (#1239 f78) —
    // the avatar's trees belong to the signed-in user by construction.
    let owner_did: String = session
        .as_deref()
        .map_or_else(String::new, |s| s.did.clone());
    // Snapshot pre-frame selection state so we can detect (a) "selection
    // just appeared" — the rising edge that clears the room editor's
    // selection per the cross-editor mutex contract, and (b) tab change —
    // switching off the Visuals tab drops the gizmo target the same way
    // the room editor's tab bar already does.
    // The cross-editor mutex asks about the two aims that attach a gizmo to
    // something the ROOM editor could also be aiming at; a worn part is
    // inside a prop that is already covered by the prop's own aim.
    let aims_at_the_avatar = |e: &AvatarEditorState| {
        e.gizmo().visuals_path().is_some() || e.gizmo().worn_prop().is_some()
    };
    let prev_visuals_selected = aims_at_the_avatar(&editor);

    // One borrowed view of the asset caches for the frame (#1246); see
    // `room::assets::AssetPanel`.
    let mut asset_panel = asset_caches.panel(time.elapsed_secs_f64());

    // …and one of the live body (#1256). Built here rather than inside the
    // tab because it is ECS queries and the tab is a plain drawing function
    // over the record.
    let worn_body = local_body.worn();

    // `.open()` only hides the window *body* — without this gate the
    // whole-record `before` clone below (and the egui Window bookkeeping)
    // ran every in-game frame with the panel closed (#674). The tail logic
    // after this block still runs: collapse-deselect sees `false` here, and
    // a debounce flush pending from just before the panel closed still
    // drains and broadcasts.
    let window_visible_with_body = if !panels.avatar {
        false
    } else {
        // Taken before the bypassing reborrow: the tick of the last real
        // `set_changed()`, i.e. every edit that reached the record from
        // outside this editor (#1270 f273).
        let live_tick = live.last_changed();
        let live_mut = live.bypass_change_detection();

        let ctx = contexts.ctx_mut().unwrap();
        // Width only from the layout slot — the Avatar window auto-heights
        // to its content, and forcing the persisted height back on it
        // would pad the shorter Locomotion tab with dead space.
        let (pos, size) = chrome.place(crate::ui::layout::UiWindow::Avatar, ctx);
        // Guarded-dirty (#879): `.open(&mut panels.avatar)` through the
        // `ResMut` would mark UiPanels changed every frame, starving the
        // prefs save debounce — local copy in, write back only on close.
        let mut open = panels.avatar;
        // #1230 f33: set by the recovery banner's re-read button, acted on
        // after the closure (the spawn is a `Commands` write, and the
        // closure's own return value already carries the collapsed/closed
        // distinction below).
        let mut reload_avatar = false;
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
                        (AvatarTab::Attachments, "Wearables"),
                        (AvatarTab::Visuals, "Visuals"),
                        (AvatarTab::Locomotion, "Locomotion"),
                    ];
                    // #1256 f100: exactly one tab is a dead end at any
                    // time — never two, and never the Body tab, which on a
                    // generator body is the feature's entry point rather
                    // than a no-op. Body and Visuals are two EXCLUSIVE body
                    // kinds, and the tab bar used to hide that model behind
                    // a click: four undifferentiated labels, one of which
                    // answered "that's for the other kind of body".
                    let rigged = live_mut.0.body.rigged_ref().is_some();
                    for (tab, label) in tabs {
                        let disabled_reason = tab_disabled_reason(tab, rigged);
                        // A currently-selected tab that has just become a
                        // dead end (the owner switched body kind under it)
                        // falls back rather than sitting there disabled and
                        // selected, which reads as broken.
                        if disabled_reason.is_some() && editor.selected_tab == tab {
                            editor.selected_tab = AvatarTab::Body;
                        }
                        let mut response = ui.add_enabled(
                            disabled_reason.is_none(),
                            egui::Button::selectable(editor.selected_tab == tab, label),
                        );
                        // egui gives no tooltip on a disabled widget without
                        // this — the reason has to be asked for explicitly.
                        if let Some(reason) = disabled_reason {
                            response = response.on_disabled_hover_text(reason);
                        }
                        if response.clicked() {
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
                    gizmo,
                    visuals_tree,
                    audio_editor,
                    reroll,
                    publish_guard,
                    default_cache,
                    stored_baseline,
                    live_baseline,
                    size_readout_generation,
                    wardrobe,
                    attachments: attachments_state,
                    pending_attachment_focus,
                    editing_parts,
                    parts_tree,
                    node_clipboard,
                    ..
                } = &mut *editor;

                // --- The three comparison baselines (#1270 f273) --------
                // This editor asks "is the avatar dirty?" three times a
                // frame — the Save row, "would Reset change anything?",
                // and the recovery banner's reload button — and every one
                // of them used to serialise BOTH sides. Six whole-record
                // `Value` trees per frame, on the surface where the owner
                // spends the longest continuous stretch of fine-grained
                // interaction. #674 fixed this for the room editor and
                // #1135's doc comment claimed it had reached here too; it
                // had not.
                //
                // Each of the three sides is now cached on the thing that
                // makes it stale, and they are maintained HERE rather than
                // in the footer so the banner above reads the same
                // baselines the row below does.
                //
                // The seeded default: rebuilt only when the session DID
                // changes (#637 — it is a full part-composition build),
                // with its serialized form riding along.
                match session.as_ref() {
                    Some(s) if default_cache.as_ref().is_none_or(|(d, _, _)| d != &s.did) => {
                        let record = AvatarRecord::default_for_did(&s.did);
                        let value = serde_json::to_value(&record).ok();
                        *default_cache = Some((s.did.clone(), record, value));
                    }
                    None => *default_cache = None,
                    _ => {}
                }
                let default_record = default_cache.as_ref().map(|(_, r, _)| r);
                // The stored side: re-serialised only when the resource
                // changes. Keyed on `last_changed()` and not
                // `is_changed()`, because the flag is consumed on frames
                // where this system early-returns.
                match stored.as_ref() {
                    Some(s)
                        if stored_baseline
                            .as_ref()
                            .is_none_or(|(tick, _)| *tick != s.last_changed()) =>
                    {
                        *stored_baseline =
                            Some((s.last_changed(), serde_json::to_value(&s.0).ok()));
                    }
                    None => *stored_baseline = None,
                    _ => {}
                }

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
                                     first).",
                                )
                                .small(),
                            );
                            // The non-destructive direction (#1230 f33). The
                            // failure is almost always transient and has
                            // usually healed by the time the user reads
                            // this; the only route back to the real body
                            // used to be a full logout.
                            if crate::ui::editable::recovery_reload_button(
                                ui,
                                crate::diagnostics::event::RecordKind::Avatar,
                                match (stored.as_ref(), stored_baseline.as_ref()) {
                                    (Some(s), Some((_, baseline))) => {
                                        pds::avatar::avatar_dirty_against(
                                            &live_mut.0,
                                            live_baseline.value(live_tick, &live_mut.0),
                                            &s.0,
                                            baseline,
                                        )
                                    }
                                    _ => false,
                                },
                            )
                            .clicked()
                            {
                                reload_avatar = true;
                            }
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
                    let (action, start, effective) = crate::ui::editable::reroll_section(
                        ui,
                        "avatar_reroll",
                        "Whole-avatar seed & re-roll",
                        |ui| {
                            let action = seed_row(
                                ui,
                                &mut reroll.seed_row,
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
                            let start = reroll.start_seed(did_seed);
                            let effective = reroll.effective_seed(start);
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
                                        &mut reroll.pins.chassis,
                                        rolled.chassis,
                                    );
                                    pin_axis_row(
                                        ui,
                                        "Style",
                                        &ThemeArchetype::ALL,
                                        ThemeArchetype::label,
                                        &mut reroll.pins.style,
                                        rolled.style,
                                    );
                                    pin_axis_row(
                                        ui,
                                        "Ornateness",
                                        &OrnatenessTier::ALL,
                                        OrnatenessTier::label,
                                        &mut reroll.pins.ornateness,
                                        rolled.ornateness_tier(),
                                    );
                                    pin_axis_row(
                                        ui,
                                        "Wear",
                                        &WearTier::ALL,
                                        WearTier::label,
                                        &mut reroll.pins.wear,
                                        rolled.wear_tier(),
                                    );
                                });
                            crate::ui::editable::hunt_disclosure_line(ui, start, effective);
                            (action, start, effective)
                        },
                    )
                    // Collapsed: no Re-roll button was drawn, so there is
                    // nothing to act on this frame.
                    .unwrap_or((SeedAction::None, did_seed, None));

                    if let SeedAction::Reroll(_) = action {
                        // Build from the same hunted seed the readout
                        // previewed — never the raw typed one.
                        if let Some(seed) = effective {
                            reroll.seed_row.set_seed(seed);
                            live_mut.0 = AvatarRecord::default_for_seed(seed);
                            widget_changed = true;
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
                                "pinned re-roll found no seed matching {:?} from {start}",
                                reroll.pins
                            );
                            // Said out loud, not only logged (#1268 f69).
                            toasts.error(
                                "No seed matches these locks — unlock an axis and try again.",
                                time.elapsed_secs_f64(),
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
                        // Inventory editors (`ui::editable`). Dirty is NOT
                        // `records_differ` here, unlike the other two: a
                        // rigged body's payload rides on the serde-skipped
                        // `resolved`, so the wire compare calls a sculpted
                        // body clean (#1059). `avatar_is_dirty` is the single
                        // derivation this row, Ctrl+S and the unsaved-edits
                        // guard all ask (#1138).

                        let can_publish = session.is_some() && refresh_ctx.is_some();
                        // Both questions below read the baselines cached
                        // above and ONE live serialisation between them,
                        // rebuilt only on a frame after an edit — instead
                        // of six whole-record `Value` trees per frame
                        // (#1270 f273).
                        let generation = live_baseline.recomputes();
                        let (dirty, can_reset) = {
                            let live_value = live_baseline.value(live_tick, &live_mut.0);
                            let dirty = match (stored.as_ref(), stored_baseline.as_ref()) {
                                (Some(s), Some((_, baseline))) => {
                                    pds::avatar::avatar_dirty_against(
                                        &live_mut.0,
                                        live_value,
                                        &s.0,
                                        baseline,
                                    )
                                }
                                _ => false,
                            };
                            // Same question, different baseline: "would
                            // Reset change anything?" is live-vs-default.
                            let can_reset = match (default_record, default_cache.as_ref()) {
                                (Some(d), Some((_, _, value))) => {
                                    pds::avatar::avatar_dirty_against(
                                        &live_mut.0,
                                        live_value,
                                        d,
                                        value,
                                    )
                                }
                                _ => false,
                            };
                            (dirty, can_reset)
                        };

                        // The bundle a save writes, not the reference-only
                        // record (#1207). Skipped entirely while the record
                        // has not changed since it was last measured
                        // (#1270 f418's gate, on the same cache).
                        if crate::ui::editable::refresh_size_readout(
                            &mut *feedback,
                            &live_mut.0,
                            time.elapsed_secs_f64(),
                            *size_readout_generation != Some(generation),
                            pds::avatar::wardrobe::measure_publish,
                        ) {
                            *size_readout_generation = Some(generation);
                        }
                        let size = feedback.live_size.clone();
                        let ctrl_s =
                            publish_shortcut.take(crate::ui::shortcuts::EditorKind::Avatar);
                        let mut do_publish = false;
                        match save_load_reset_row(
                            ui,
                            crate::ui::editable::SaveRow {
                                kind: RecordKind::Avatar,
                                dirty,
                                can_publish,
                                can_reset,
                                size: &size,
                                publish_shortcut: ctrl_s,
                                status: &mut feedback.status,
                                // Undo covers Revert/Reset here (#866).
                                confirm: None,
                                reset: crate::ui::editable::ResetWording::Record,
                            },
                        ) {
                            RecordAction::None => {}
                            RecordAction::Refused(reason) => {
                                toasts.info(
                                    crate::ui::editable::ctrl_s_refused(&reason),
                                    time.elapsed_secs_f64(),
                                );
                            }
                            RecordAction::Publish => {
                                // Clobber protection (#840): after an
                                // unrecoverable fetch the editor holds the
                                // default while the real record may still
                                // sit on the PDS — the first publish asks.
                                match recovery.as_deref() {
                                    Some(rec) => crate::ui::editable::request_overwrite_confirm(
                                        publish_guard,
                                        RecordKind::Avatar,
                                        &rec.reason,
                                    ),
                                    None => do_publish = true,
                                }
                            }
                            RecordAction::Load => {
                                if let Some(stored) = &stored {
                                    live_mut.0 = stored.0.clone();
                                    widget_changed = true;
                                    undo_labels.set_avatar("revert to saved");
                                }
                            }
                            RecordAction::Reset => {
                                if let Some(default_record) = default_record {
                                    live_mut.0 = default_record.clone();
                                    widget_changed = true;
                                    undo_labels.set_avatar("reset to default");
                                }
                            }
                        }
                        if publish_guard
                            .show(ui.ctx(), "avatar-recovery-publish")
                            .is_some()
                        {
                            // Acknowledged. The marker stays until the poll
                            // system sees the write land (#1199) — retiring
                            // it here left a failed overwrite with no banner.
                            do_publish = true;
                        }
                        if do_publish
                            && let (Some(session), Some(refresh)) =
                                (session.as_ref(), refresh_ctx.as_ref())
                        {
                            feedback.status = PublishStatus::Publishing {
                                since_secs: time.elapsed_secs_f64(),
                            };
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

                        // #1269 f111 + f293. The two body kinds have
                        // OPPOSITE live-preview semantics in this one
                        // window and neither was stated anywhere but the
                        // module source: a construction-kit body's record
                        // IS the broadcast payload, while a rigged body's
                        // rides a `serde(skip)` field, so peers keep
                        // rendering the owner's last SAVED body until a
                        // publish lands. An owner sculpting a face for ten
                        // minutes in a room full of visitors was
                        // performing for nobody.
                        crate::ui::editable::audience_notice(
                            ui,
                            if live_mut.0.body.rigged_ref().is_some() {
                                crate::ui::editable::EditVisibility::SavedOnly
                            } else {
                                crate::ui::editable::EditVisibility::Live
                            },
                            peers.iter().count(),
                            "avatar",
                        );
                        publish_status_line(ui, &feedback.status, time.elapsed_secs_f64(), dirty);
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
                                local_body.build_failed(),
                            );
                            widget_changed |= outcome.changed;
                            if let Some(label) = outcome.label {
                                undo_labels.set_avatar(label);
                            }
                            if let Some(text) = outcome.toast {
                                toasts.success(text, time.elapsed_secs_f64());
                            }
                            if outcome.wants_wardrobe_refresh
                                && let Some(s) = session.as_ref()
                            {
                                wardrobe.fetching = true;
                                wardrobe.attempted = true;
                                // Clear the last failure as the retry
                                // starts, so the error line describes the
                                // attempt in flight and not the one before
                                // it (#1141).
                                wardrobe.error = None;
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
                                    aim_in_place(
                                        gizmo,
                                        &mut visuals_tree.view,
                                        &mut parts_tree.view,
                                        GizmoTarget::None,
                                    );
                                    return;
                                };
                                ui.horizontal(|ui| {
                                    if ui.button("⬅ Worn items").clicked() {
                                        *editing_parts = None;
                                        aim_in_place(
                                            gizmo,
                                            &mut visuals_tree.view,
                                            &mut parts_tree.view,
                                            GizmoTarget::None,
                                        );
                                        parts_tree.view.set_selected(Vec::new());
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
                                // The panel's selection is the widget's
                                // I/O, not the truth: seed it from the aim,
                                // fold it back after (#1161).
                                parts_tree.selection = match gizmo.worn_part() {
                                    Some((root, path)) => {
                                        crate::ui::room::generators::TreeSelection {
                                            root: Some(root.to_owned()),
                                            path: Some(path.to_vec()),
                                        }
                                    }
                                    None => Default::default(),
                                };
                                draw_generators_tab(
                                    ui,
                                    &mut source,
                                    parts_tree,
                                    inventory.as_deref_mut(),
                                    audio_editor,
                                    &grammar_diag,
                                    &mut widget_changed,
                                    &mut blob_ctx.selected_element,
                                    &mut toasts,
                                    time.elapsed_secs_f64(),
                                    &mut undo_labels.slot(crate::ui::shortcuts::EditorKind::Avatar),
                                    None,
                                    &mut face_pick,
                                    // #1239 f78: the same owner every other
                                    // catalogue path stamps with.
                                    &owner_did,
                                    // No placement layer on an avatar's
                                    // trees — its roots ARE instanced.
                                    &mut None,
                                    // Single-root: no filter box is drawn.
                                    &mut String::new(),
                                    node_clipboard,
                                    &mut asset_panel,
                                );
                                // The tree's selection IS the gizmo target:
                                // fold it back (a tree click picks a part;
                                // a cleared tree drops the aim). Nothing
                                // has to clear the whole-prop selection
                                // here any more — the aim is one field, so
                                // naming a part *is* releasing the prop.
                                //
                                // Only a part-shaped answer, or an outgoing
                                // part aim, may write: a tree that is on
                                // screen speaks for its own kind of target
                                // and no other. (A scene pick can leave a
                                // visuals node aimed while this tab is
                                // still the open one, for the one frame
                                // before `release_hidden_selections` runs.)
                                let picked = match (
                                    parts_tree.selection.root.as_ref(),
                                    parts_tree.selection.path.clone(),
                                ) {
                                    (Some(root), Some(path)) if *root == rkey => {
                                        Some(GizmoTarget::WornPart {
                                            rkey: rkey.clone(),
                                            path,
                                        })
                                    }
                                    _ => None,
                                };
                                if picked.is_some() || gizmo.worn_part().is_some() {
                                    aim_in_place(
                                        gizmo,
                                        &mut visuals_tree.view,
                                        &mut parts_tree.view,
                                        picked.unwrap_or(GizmoTarget::None),
                                    );
                                }
                                return;
                            }
                            // Same two-way channel as the trees': the list
                            // reads and writes an `Option<rkey>`, seeded
                            // from the aim and folded back only if it
                            // moved — see the parts tree's note on why a
                            // panel may only speak for its own kind.
                            let mut listed = gizmo.worn_prop().map(str::to_owned);
                            let outcome = attachments::draw_attachments_tab(
                                ui,
                                &mut live_mut.0,
                                inventory.as_deref_mut(),
                                attachments_state,
                                session.as_ref().map(|s| s.did.as_str()),
                                &mut listed,
                                std::mem::take(pending_attachment_focus),
                                &mut toasts,
                                time.elapsed_secs_f64(),
                                &worn_body,
                            );
                            if listed.as_deref() != gizmo.worn_prop() {
                                aim_in_place(
                                    gizmo,
                                    &mut visuals_tree.view,
                                    &mut parts_tree.view,
                                    match listed {
                                        Some(rkey) => GizmoTarget::WornProp { rkey },
                                        None => GizmoTarget::None,
                                    },
                                );
                            }
                            widget_changed |= outcome.changed;
                            if let Some(label) = outcome.label {
                                undo_labels.set_avatar(label);
                            }
                            if let Some(rkey) = outcome.open_parts {
                                // Open on the item ROOT so a gizmo is aimed
                                // at once; the whole-prop selection yields
                                // by construction, the aim being one field.
                                aim_in_place(
                                    gizmo,
                                    &mut visuals_tree.view,
                                    &mut parts_tree.view,
                                    GizmoTarget::WornPart {
                                        rkey: rkey.clone(),
                                        path: Vec::new(),
                                    },
                                );
                                parts_tree
                                    .view
                                    .set_selected(vec![GenNodeId::root(rkey.clone())]);
                                *editing_parts = Some(rkey);
                            }
                        });
                    }
                    AvatarTab::Visuals => {
                        ui.allocate_ui(egui::vec2(ui.available_width(), body_height), |ui| {
                            // The tree edits a generator body's tree; a
                            // rigged body has no tree to draw. #1265 f101:
                            // this used to promise the rigged editor was
                            // still coming (it shipped, as the Body tab)
                            // and to advise a bare re-roll, which lands
                            // back on a rigged body whenever
                            // `ChassisFamily::for_seed` rolls `Humanoid` —
                            // one of four families, so a coin flip. The
                            // Chassis pin row below is the deterministic
                            // control, so the advice routes through it and
                            // names the three families by the labels that
                            // row actually shows (`ChassisFamily::label`).
                            let Some(visuals) = live_mut.0.body.visuals_mut() else {
                                ui.label(
                                    egui::RichText::new(
                                        "You're wearing a rigged body — sculpt it on the \
                                         Body tab. For a construction-kit body instead, \
                                         open Seed & re-roll below, lock Chassis to \
                                         Hover-boat, Airship or Land-skiff, and re-roll.",
                                    )
                                    .small()
                                    .weak(),
                                );
                                return;
                            };
                            let mut source = AvatarVisualsTreeSource::new(visuals);
                            // The panel's selection is the widget's I/O,
                            // not the truth: seed it from the aim, fold it
                            // back after (#1161). The one-shot focus
                            // request rides the panel now, consumed by the
                            // tree it belongs to.
                            let aimed = gizmo.visuals_path().map(<[usize]>::to_vec);
                            visuals_tree.selection = crate::ui::room::generators::TreeSelection {
                                root: aimed
                                    .as_ref()
                                    .map(|_| AvatarVisualsTreeSource::ROOT_NAME.to_string()),
                                path: aimed.clone(),
                            };
                            draw_generators_tab(
                                ui,
                                &mut source,
                                visuals_tree,
                                inventory.as_deref_mut(),
                                audio_editor,
                                &grammar_diag,
                                &mut widget_changed,
                                &mut blob_ctx.selected_element,
                                &mut toasts,
                                time.elapsed_secs_f64(),
                                &mut undo_labels.slot(crate::ui::shortcuts::EditorKind::Avatar),
                                // Avatars can't grow roads — no stats readout.
                                None,
                                &mut face_pick,
                                &owner_did,
                                &mut None,
                                &mut String::new(),
                                node_clipboard,
                                &mut asset_panel,
                            );
                            if visuals_tree.selection.path != aimed {
                                aim_in_place(
                                    gizmo,
                                    &mut visuals_tree.view,
                                    &mut parts_tree.view,
                                    match visuals_tree.selection.path.clone() {
                                        Some(path) => GizmoTarget::VisualsNode { path },
                                        None => GizmoTarget::None,
                                    },
                                );
                            }
                        });
                    }
                    AvatarTab::Locomotion => {
                        egui::ScrollArea::vertical()
                            .auto_shrink([true, false])
                            .max_height(body_height)
                            .show(ui, |ui| {
                                // #1265 f109: this used to teach a
                                // collapse-the-window workaround for the
                                // #814 full-body freeze. #1103 reversed
                                // that freeze — `holds_avatar_still` is
                                // exactly `has_gizmo_selection` now, and
                                // `release_hidden_selections` clears every
                                // avatar-side selection when a tab that
                                // cannot show it is picked, so no gizmo can
                                // be aimed while this tab is on screen.
                                // Name the gizmo, not the window.
                                ui.label(
                                    egui::RichText::new(
                                        "⏵ Drive with WASD while this window is open — \
                                         your avatar only holds still while a gizmo is \
                                         aimed at it.",
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
                                let fallback_seed = reroll
                                    .seed_row
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
                                    &movement,
                                    &mut toasts,
                                    time.elapsed_secs_f64(),
                                );
                            });
                    }
                }
            });

        // The whole-record clone this used to compare against is gone
        // (#1270 f273). It was a deep clone of the `AvatarRecord` at the
        // top of every frame plus a derived `PartialEq` walk at the
        // bottom, and it existed as a backstop for edit sites that did not
        // report. Every site reports now — the four `draw_*` handoffs
        // always took `&mut widget_changed`, and the three direct
        // assignments (seed re-roll, Revert, Reset) say so themselves.
        // `avatar_edits_report_themselves` is what keeps a fourth from
        // being written silently.

        // #1230 f33: re-read the stored avatar from the PDS.
        // `poll_record_task` installs it as live AND stored on a clean
        // resolution and retires the recovery marker, so the banner clears
        // itself; the button is disabled while dirty, so nothing unsaved is
        // in its way.
        if reload_avatar && let Some(s) = session.as_ref() {
            crate::loading::fetch::spawn_record_fetch::<pds::AvatarRecord>(
                &mut commands,
                s.did.clone(),
                0,
                time.elapsed_secs_f64(),
            );
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
    let now_visuals_selected = aims_at_the_avatar(&editor);
    if now_visuals_selected
        && !prev_visuals_selected
        && let Some(room) = room_editor.as_deref_mut()
    {
        room.selected_placement = None;
        room.tree.selection.clear();
        room.tree.view.set_selected(Vec::new());
    }

    if widget_changed {
        // The live record changed through `bypass_change_detection`, so no
        // tick moved and the cached wire form is stale (#1270 f273). Done
        // once here, at the end of the frame, rather than at each of the
        // seven edit sites: the next frame's first `value()` call rebuilds
        // and every later one that frame reuses it, which is the same
        // one-frame latency the tab bodies always had (they draw after the
        // footer that reads the answer).
        editor.live_baseline.touch();
        editor.pending_flush_secs = crate::config::ui::editor::MENU_DEBOUNCE_SECS;
        // Coarse per-tab undo label (#865) when no site named the edit.
        if !undo_labels.avatar_pending() {
            undo_labels.set_avatar(match editor.selected_tab {
                AvatarTab::Body => "body edit",
                AvatarTab::Attachments => "wearable edit",
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
        crate::config::http::run_or(
            fut,
            Err(crate::pds::FetchError::Network(
                crate::config::http::timed_out("wardrobe listing"),
            )),
        )
        .await
    });
    commands.spawn(WardrobeListTask { task });
}

/// Land finished wardrobe listings. A failed walk clears the spinner and
/// leaves whatever list was there — the button is the retry — and records
/// the reason so the tab can say a fetch was tried and failed rather than
/// re-showing the pristine "Refresh to list…" hint (#1141).
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
            Ok(entries) => {
                editor.wardrobe.entries = Some(entries);
                editor.wardrobe.error = None;
            }
            Err(err) => {
                warn!("wardrobe listing failed: {err:?}");
                editor.wardrobe.error = Some(err.to_string());
            }
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
        crate::config::http::run_or(
            fut,
            Err(crate::config::http::timed_out("Saving your avatar")),
        )
        .await
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
/// button is disabled until the next edit, and — for a rigged body — tell
/// the room the referenced records have moved (#1122).
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
    // A failed write is reported OUTSIDE this window (#1137): "Continue in
    // background" and Esc-closing the editor mid-save both leave the footer
    // where the failure lands unread.
    mut panels: ResMut<crate::ui::toolbar::UiPanels>,
    mut toasts: ResMut<crate::notify::Toasts>,
    // The post-publish nudge (#1122).
    mut network: bevy_symbios_multiuser::prelude::SendMessage<crate::protocol::OverlandsMessage>,
    // A result for another identity must not pin `stored` (#1204).
    session: Option<Res<AtprotoSession>>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let spawned_at = task.spawned_at;
        let Some(result) = crate::ui::editable::poll_or_expire(
            &mut task.task,
            spawned_at,
            time.elapsed_secs_f64(),
            "Saving your avatar",
        ) else {
            continue;
        };
        commands.entity(entity).despawn();
        if crate::ui::room::stale_result(
            "Saving your avatar",
            &task.did,
            session.as_deref().map(|s| s.did.as_str()),
        ) {
            continue;
        }
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
                // The stored copy is now exactly what was written, so the
                // recovery marker retires where success is known (#1199).
                commands.remove_resource::<crate::state::AvatarRecordRecovery>();
                // Tell the room (#1122). A rigged body's payload is in the
                // wardrobe + attachment records this write just changed, and
                // they sit at the SAME rkeys the live-preview broadcast
                // already named — so peers holding a resolution have no way
                // to notice from the references alone. Nothing here made
                // `LiveAvatarRecord` changed, so no broadcast fired at all,
                // and even one that did would have carried the pre-save body
                // forward (`network::inbound::carry_resolution`). Peers kept
                // the old body until their wearer next edited something.
                //
                // Only for a rigged body: a generator body's payload IS the
                // record, so the preview already showed peers the final
                // state and there is nothing to re-fetch.
                if task.published.body.rigged_ref().is_some() {
                    network.broadcast(
                        crate::protocol::OverlandsMessage::AvatarRecordsPublished,
                        bevy_symbios_multiuser::prelude::ChannelKind::Reliable,
                    );
                }
                feedback.status = PublishStatus::Success { at_secs: now };
                crate::ui::editable::report_publish_success(
                    RecordKind::Avatar,
                    &panels,
                    &mut toasts,
                    now,
                );
                session_log.info(
                    now,
                    EventPayload::RecordWriteCompleted {
                        record: RecordKind::Avatar,
                        did,
                        duration_secs,
                    },
                );
            }
            Err(e) => crate::ui::editable::report_publish_failure(
                RecordKind::Avatar,
                crate::ui::editable::WriteOp::Save,
                did,
                e,
                now,
                crate::ui::editable::FailureSinks {
                    session_log: &mut session_log,
                    feedback: &mut feedback,
                    toasts: &mut toasts,
                    panels: &mut panels,
                },
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every edit this editor makes to the live record reports itself
    /// (#1270 f273).
    ///
    /// `avatar_ui` used to deep-clone the whole `AvatarRecord` at the top
    /// of every frame and run a derived `PartialEq` over it at the bottom,
    /// purely as a backstop for edit sites that might not set
    /// `widget_changed`. That is a whole-record clone plus a whole-record
    /// walk, sixty times a second, on the editor with the least frame
    /// budget to spare — paid on every frame including the overwhelming
    /// majority where nothing happened at all.
    ///
    /// The clone is gone, so the reports have to be real. The four `draw_*`
    /// handoffs each take `&mut widget_changed` or return an `outcome`
    /// whose `changed` is ORed in, and the compiler holds those. What
    /// nothing held is a bare `live_mut.0 = …` — the seed re-roll, Revert
    /// and Reset all replace the record wholesale, and all three were
    /// silent. This is the check that a fourth cannot be.
    ///
    /// It reads the source rather than driving the editor because
    /// `avatar_ui` is a sixteen-parameter system over a live egui context,
    /// a session and a PDS; the fact being pinned is a property of the
    /// code, and the repo's other "a helper nobody is obliged to call"
    /// guards (`ui::num`, `ui::affordances`) are the same shape.
    #[test]
    fn avatar_edits_report_themselves() {
        /// Assignments to the whole live record, as `(line, reports)`.
        fn record_assignments(source: &str) -> Vec<(usize, bool)> {
            const WINDOW: usize = 6;
            let lines: Vec<&str> = source.lines().collect();
            let mut out = Vec::new();
            for (n, line) in lines.iter().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                // `live_mut.0 = …`, but not `live_mut.0 == …` and not a
                // read like `&live_mut.0`.
                let Some(at) = code.find("live_mut.0") else {
                    continue;
                };
                let tail = code[at + "live_mut.0".len()..].trim_start();
                if !tail.starts_with('=') || tail.starts_with("==") {
                    continue;
                }
                let reports = lines[n..(n + WINDOW).min(lines.len())]
                    .iter()
                    .any(|l| l.contains("widget_changed = true"));
                out.push((n + 1, reports));
            }
            out
        }

        // Controls, both ways round.
        assert_eq!(
            record_assignments("    live_mut.0 = stored.0.clone();\n"),
            vec![(1, false)],
            "a silent assignment is what this has to be able to see"
        );
        assert_eq!(
            record_assignments("    live_mut.0 = stored.0.clone();\n    widget_changed = true;\n"),
            vec![(1, true)]
        );
        assert!(
            record_assignments("    if live_mut.0 == before {\n").is_empty(),
            "a comparison is not an assignment"
        );
        assert!(
            record_assignments("    let rigged = &live_mut.0;\n").is_empty(),
            "a read is not an assignment"
        );

        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui/avatar/mod.rs"),
        )
        .expect("this file is readable");
        let source = crate::ui::fonts::glyph_coverage_tests::non_test_source(&source);
        let found = record_assignments(source);
        assert!(
            found.len() >= 3,
            "the scan found {} record assignments — it has gone blind (the seed \
             re-roll, Revert and Reset are all still there)",
            found.len()
        );
        let silent: Vec<usize> = found
            .iter()
            .filter(|(_, reports)| !reports)
            .map(|(line, _)| *line)
            .collect();
        assert!(
            silent.is_empty(),
            "src/ui/avatar/mod.rs replaces the live record at {silent:?} without setting \
             `widget_changed = true` — the edit will not arm the debounce, so the body \
             will not rebuild and no peer will see it until something else is touched"
        );
    }

    /// #1236 f139. Sequence: right-click your hat → "Edit …", the body
    /// freezes under the gizmo, press Esc. The Esc back-out ladder tested
    /// `has_visuals_selection`, which is FALSE for a worn prop and for a
    /// worn part — so the rung was skipped, the press closed an unrelated
    /// window, and the chassis stayed axis-locked at `GravityScale(0)`.
    /// The ladder now asks `has_gizmo_selection` and answers with
    /// `clear_gizmo_selections`, which is exactly the difference this
    /// pins.
    #[test]
    fn a_worn_prop_selection_is_invisible_to_the_visuals_question() {
        for aim in [
            (|s: &mut AvatarEditorState| {
                s.select_attachment_from_scene_pick(String::from("3jzfcijpj2z2a"))
            }) as fn(&mut AvatarEditorState),
            |s: &mut AvatarEditorState| {
                s.select_attachment_part_from_scene_pick(String::from("3jzfcijpj2z2a"), vec![1])
            },
        ] {
            let mut state = AvatarEditorState {
                window_visible: true,
                ..Default::default()
            };
            aim(&mut state);
            assert!(
                !state.has_visuals_selection(),
                "precondition: the old ladder question is blind to this aim"
            );
            assert!(state.has_gizmo_selection(), "the ladder's question sees it");
            assert!(state.holds_avatar_still(), "and the body is frozen by it");

            state.clear_gizmo_selections();
            assert!(
                !state.holds_avatar_still(),
                "one Esc must release the freeze it created"
            );
        }
    }

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
        assert!(state.gizmo().worn_part().is_none(), "the part gizmo let go");
        assert!(!state.holds_rig_at_rest());
        assert_eq!(
            state.editing_parts(),
            Some(rkey.as_str()),
            "the parts editor stays open, like a World-editor tab"
        );

        state.select_attachment_from_scene_pick(rkey.clone());
        state.release_on_scene_miss(true);
        assert!(
            state.gizmo().worn_prop().is_none(),
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
        assert!(state.gizmo().worn_part().is_none());
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
        assert_eq!(state.gizmo().worn_prop(), Some("3jzfcijpj2z2a"));
        assert!(!state.has_visuals_selection(), "the visuals row let go");
        assert_eq!(
            state.selected_tab,
            AvatarTab::Attachments,
            "the pick brings forward the tab that can show it"
        );
        assert!(state.pending_attachment_focus, "focus request armed");

        state.select_from_scene_pick(vec![2]);
        assert!(state.gizmo().worn_prop().is_none(), "the prop let go");
        assert!(state.has_visuals_selection());
    }

    /// #1062 → #1103 → #1106: an attachment offset is stored in its
    /// carrying joint's rest frame, so a gizmo aimed at a WHOLE worn prop
    /// pins the body to its bind pose — and only then. A PART gizmo holds
    /// the pose as it stands instead (selecting must not move anything);
    /// the tab being open is not a hold (owner direction), and a
    /// visuals-row gizmo is neither.
    /// #1158. The four booleans the player systems read are now the whole
    /// of what crosses out of the editor, so this is the one place the
    /// mapping can go wrong — and a wrong mapping is silent: the body
    /// simply stops holding, or holds when it should walk.
    ///
    /// Asserts the mirror against the predicates rather than restating
    /// their values, so it cannot drift from the rules in #1103/#1106 that
    /// the tests above pin.
    #[test]
    fn the_rig_hold_mirrors_every_predicate_including_absence() {
        use bevy::ecs::system::RunSystemOnce;

        fn mirrored(state: &AvatarEditorState) -> crate::player::RigHold {
            crate::player::RigHold {
                at_rest: state.holds_rig_at_rest(),
                pose: state.holds_rig_pose(),
                still: state.holds_avatar_still(),
                visuals_row: state.has_visuals_selection(),
            }
        }

        let mut app = App::new();
        app.init_resource::<crate::player::RigHold>();

        // No editor state at all — before login, and the headless render
        // tool. Must read as "nothing is held", which is what the
        // `Option<Res<…>>` this replaced degraded to.
        app.world_mut()
            .run_system_once(mirror_rig_hold)
            .expect("runs without an editor");
        assert_eq!(
            *app.world().resource::<crate::player::RigHold>(),
            crate::player::RigHold::default(),
            "absent editor state must hold nothing"
        );

        for aim in ["prop", "part", "visuals"] {
            let mut state = AvatarEditorState {
                window_visible: true,
                ..Default::default()
            };
            match aim {
                "prop" => state.select_attachment_from_scene_pick(String::from("3jzfcijpj2z2a")),
                "part" => state
                    .select_attachment_part_from_scene_pick(String::from("3jzfcijpj2z2a"), vec![0]),
                _ => state.select_from_scene_pick(vec![0]),
            }
            let expected = mirrored(&state);
            app.insert_resource(state);
            app.world_mut()
                .run_system_once(mirror_rig_hold)
                .expect("runs");
            assert_eq!(
                *app.world().resource::<crate::player::RigHold>(),
                expected,
                "the {aim} gizmo's hold did not reach the player systems intact"
            );
        }

        // And it RELEASES: a hold that only ever latched on would freeze
        // the body for the rest of the session.
        app.insert_resource(AvatarEditorState::default());
        app.world_mut()
            .run_system_once(mirror_rig_hold)
            .expect("runs");
        assert_eq!(
            *app.world().resource::<crate::player::RigHold>(),
            crate::player::RigHold::default(),
            "clearing the aim must release the hold"
        );
    }

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

    /// #1161, the property the enum was introduced for: **every** way of
    /// aiming leaves exactly one aim, and takes the outgoing target's
    /// tree row down with it.
    ///
    /// Under the three parallel `Option`s this was fourteen methods each
    /// remembering to clear the other two, and #1103 bugs 1 and 3 were two
    /// paths that had never been told about #1098's part. Here the aim is
    /// one field, so the first half is the type checker's; what this pins
    /// is the half that is not — the tree-row highlight that lives beside
    /// the aim, and which is what left a row lit over a gizmo it no longer
    /// owned.
    #[test]
    fn every_aim_replaces_the_last_one_and_releases_its_tree_row() {
        /// One way of aiming, and what to call it in a failure message.
        type Aim = (&'static str, fn(&mut AvatarEditorState, &str));

        let rkey = String::from("3jzfcijpj2z2a");
        let aims: [Aim; 4] = [
            ("visuals", |s, _| s.select_from_scene_pick(vec![1, 0])),
            ("prop", |s, k| {
                s.select_attachment_from_scene_pick(k.to_string())
            }),
            ("part", |s, k| {
                s.select_attachment_part_from_scene_pick(k.to_string(), vec![2])
            }),
            ("parts editor", |s, k| s.open_parts_editor(k.to_string())),
        ];
        for (first_name, first) in aims {
            for (then_name, then) in aims {
                let mut state = AvatarEditorState::default();
                first(&mut state, &rkey);
                assert!(state.has_gizmo_selection(), "{first_name} aims something");
                then(&mut state, &rkey);

                // Exactly one aim, by construction — and both tree widgets
                // agree with it, which is the part the compiler cannot see.
                let visuals_row_lit = !state.visuals_tree.view.selected().is_empty();
                let part_row_lit = !state.parts_tree.view.selected().is_empty();
                assert_eq!(
                    visuals_row_lit,
                    state.gizmo().visuals_path().is_some(),
                    "{first_name} then {then_name}: the visuals row and the aim disagree"
                );
                assert_eq!(
                    part_row_lit,
                    state.gizmo().worn_part().is_some(),
                    "{first_name} then {then_name}: the parts row and the aim disagree"
                );
            }
        }
    }

    /// The one release that is deliberately narrow: the room editor takes
    /// the gizmo from an avatar VISUALS row when a room selection rises
    /// (`room::room_admin_ui`'s half of the cross-editor mutex), and the
    /// undo restore re-seeds the same row — neither has ever claimed a
    /// worn prop's gizmo, which is aimed at something the room editor
    /// cannot select. Widening this to the whole aim would silently take
    /// down a wearable's offset gizmo.
    #[test]
    fn releasing_the_visuals_aim_leaves_a_worn_prop_alone() {
        let rkey = String::from("3jzfcijpj2z2a");
        let mut state = AvatarEditorState::default();

        state.select_from_scene_pick(vec![0]);
        state.release_visuals_aim();
        assert!(!state.has_gizmo_selection(), "the visuals row let go");

        state.select_attachment_from_scene_pick(rkey.clone());
        state.release_visuals_aim();
        assert_eq!(
            state.gizmo().worn_prop(),
            Some(rkey.as_str()),
            "a worn prop is not the room editor's to take"
        );
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
            state.gizmo(),
            &GizmoTarget::VisualsNode {
                path: vec![1, 0, 2]
            }
        );
        assert!(state.has_visuals_selection());
        assert!(state.visuals_tree.pending_focus, "focus request armed");

        // The tree widget mirrors the selection...
        let selected_id = GenNodeId::child(
            AvatarVisualsTreeSource::ROOT_NAME.to_string(),
            vec![1, 0, 2],
        );
        assert_eq!(state.visuals_tree.view.selected(), &vec![selected_id]);
        // ...and every ancestor (root, [1], [1,0]) is explicitly opened
        // so the picked row is actually visible.
        for depth in 0..3 {
            let ancestor = GenNodeId::child(
                AvatarVisualsTreeSource::ROOT_NAME.to_string(),
                vec![1, 0, 2][..depth].to_vec(),
            );
            assert_eq!(
                state.visuals_tree.view.is_open(&ancestor),
                Some(true),
                "ancestor at depth {depth} expanded"
            );
        }
    }
}

#[cfg(test)]
mod tab_tests {
    use super::*;

    /// THE SEQUENCE (#1256 f100): open the Avatar window, see four
    /// equally-weighted tabs, click Visuals and Attachments, and both say
    /// they are for the other kind of body.
    ///
    /// The correction the evidence refuter made is what this pins: it is
    /// ONE dead end per body kind, never two, and never the Body tab — on a
    /// generator body that tab is the feature's entry point, with a working
    /// "Wear a rigged body" action, not a no-op.
    #[test]
    fn exactly_one_tab_is_a_dead_end_and_it_is_never_the_body_tab() {
        let all = [
            AvatarTab::Body,
            AvatarTab::Attachments,
            AvatarTab::Visuals,
            AvatarTab::Locomotion,
        ];
        for rigged in [false, true] {
            let dead: Vec<AvatarTab> = all
                .into_iter()
                .filter(|tab| tab_disabled_reason(*tab, rigged).is_some())
                .collect();
            assert_eq!(
                dead.len(),
                1,
                "rigged={rigged}: expected one dead end, got {dead:?}"
            );
            assert!(!dead.contains(&AvatarTab::Body));
            assert!(!dead.contains(&AvatarTab::Locomotion));
        }

        // And it is the tab for the OTHER body kind, each time.
        assert!(tab_disabled_reason(AvatarTab::Visuals, true).is_some());
        assert!(tab_disabled_reason(AvatarTab::Visuals, false).is_none());
        assert!(tab_disabled_reason(AvatarTab::Attachments, false).is_some());
        assert!(tab_disabled_reason(AvatarTab::Attachments, true).is_none());
    }

    /// The reason has to name the body kind you are on and where to go —
    /// egui shows nothing at all on a disabled widget without an explicit
    /// `on_disabled_hover_text`, so this string is the entire explanation.
    #[test]
    fn a_disabled_tab_says_which_body_you_are_on_and_where_to_go() {
        let visuals = tab_disabled_reason(AvatarTab::Visuals, true).expect("dead on a rigged body");
        assert!(visuals.contains("rigged") && visuals.contains("Body tab"));
        let attachments =
            tab_disabled_reason(AvatarTab::Attachments, false).expect("dead on a generator body");
        assert!(attachments.contains("rigged") && attachments.contains("Body tab"));
    }
}
