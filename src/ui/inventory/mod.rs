//! Personal inventory stash UI.
//!
//! The Inventory window lists every `Generator` the owner has tucked aside
//! across editing sessions. Each entry can be renamed in place via a modal,
//! removed, or kept — and once the stash diverges from the PDS-persisted
//! copy, "Save to PDS" commits the live-vs-stored diff as one atomic
//! `com.atproto.repo.applyWrites` batch — one record per item (#696).
//! The stash is loaded during `AppState::Loading`
//! (see [`crate::loading::start_inventory_record_fetch`]) so a freshly-logged-in
//! owner always sees the previously-saved items the moment they land in-game.
//!
//! `InventoryRecord` does not derive `PartialEq` (the underlying `Generator`
//! enum doesn't either, because its variants carry types that themselves
//! would need full equality), so the dirty check round-trips through
//! `serde_json` — same pattern the world editor uses for its Reset button.
//!
//! Drag-to-place: each row is a drag source. When the owner releases a drag
//! over the 3D viewport while standing in their own room,
//! [`drop::handle_generator_drop`] raycasts against the terrain and appends
//! a fresh `Placement::Absolute` to the live `RoomRecord`, copying the
//! dragged generator into the room's `generators` map on first use.
//!
//! Drag-to-gift: releasing the same drag over a peer row in the People
//! window routes it into an `ItemOffer` instead. Gifting works in ANY room
//! (#699) — only the ground-placement branch is owner-gated, and the drop
//! handler enforces that, not the drag source.

mod drop;

pub use drop::{handle_generator_drop, preview_generator_drop};

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::diagnostics::SessionLog;
use crate::diagnostics::event::{EventPayload, RecordKind};
use crate::pds::{Generator, GeneratorKind, InventoryRecord};
use crate::state::{
    CurrentRoomDid, LiveInventoryRecord, PublishFeedback, PublishStatus, StoredInventoryRecord,
};
use crate::ui::editable::{RecordAction, publish_status_line, save_load_reset_row};

/// Persistent UI-only state for the Inventory window. Held in a `Local` so
/// it lives for the lifetime of the system without polluting the global
/// resource table.
#[derive(Default)]
pub struct InventoryEditorState {
    /// Active rename modal: `(original_key, draft_key)`.
    pub renaming_generator: Option<(String, String)>,
    /// Pending Revert/Reset confirmation for the shared save row (#838).
    pub row_confirm: crate::ui::confirm::ConfirmState<RecordAction>,
    /// Pending row-delete confirmation (#1200): the stash has no undo by
    /// owner decision, and the red minus sits beside Rename. Carries the
    /// item name; the body says whether it is worn.
    pub delete_confirm: crate::ui::confirm::ConfirmState<String>,
    /// Pending publish-after-degraded-fetch confirmation (#840): while
    /// [`crate::state::InventoryRecordRecovery`] is present the stash
    /// shows the empty default and saving would wipe the stored one —
    /// the first publish asks first.
    pub publish_guard: crate::ui::confirm::ConfirmState<()>,
    /// Serialized form of [`StoredInventoryRecord`] for the per-frame dirty
    /// check (#1135), the same cache the room editor got in #674.
    ///
    /// The uncached form ran `records_differ` twice — live-vs-stored and
    /// live-vs-default — and `records_differ` is
    /// `serde_json::to_value(a) != serde_json::to_value(b)`, so an open panel
    /// built three whole `Value` trees of the stash every frame. The stash
    /// holds up to `MAX_INVENTORY_ITEMS` generator trees, each allowed the
    /// full 100 KiB record budget, and a decorating session parks this panel
    /// open for minutes at a time. #674 fixed exactly this for the room and
    /// avatar editors and said the pattern applied here too; inventory was
    /// left behind.
    ///
    /// Keyed by the resource's `last_changed` tick rather than
    /// `is_changed()`, for the reason #674 records: the change flag is
    /// consumed even on frames where this system early-returns, which would
    /// leave a stale baseline behind afterwards.
    stored_baseline: Option<(bevy::ecs::change_detection::Tick, Option<serde_json::Value>)>,
    /// Serialized form of the default (empty) stash, for the `can_reset`
    /// comparison. Unlike the room's, this has no DID to key on and no
    /// procedural build behind it — `InventoryRecord::default()` is empty —
    /// so it is built once on first use and never invalidated.
    default_baseline: Option<Option<serde_json::Value>>,
}

/// Async task for publishing the inventory record to the owner's PDS. Carries
/// the target `did` + dispatch time so [`poll_publish_inventory_tasks`] can emit
/// a typed `RecordWrite*` session event (with the write's duration) on resolve.
#[derive(Component)]
pub struct PublishInventoryTask {
    pub task: bevy::tasks::Task<Result<(), String>>,
    pub did: String,
    pub spawned_at: f64,
    /// Serialized size of the record being written, measured at dispatch so
    /// the poll system can gauge + log it (#694).
    pub record_bytes: Option<usize>,
    /// The exact stash this task handed to the PDS. On success `stored` is
    /// pinned to THIS, never to whatever `live` holds when the task lands
    /// (#1116). The window is routine here rather than theoretical: the
    /// accept-a-gift path publishes without the Inventory window open and
    /// re-opens the dialog for the next offer immediately, so a second
    /// gift, a Wear, a rename or a delete inside one round trip is normal
    /// play — and every one of them used to be marked clean without ever
    /// having been written. `stored` is also what the next save's diff is
    /// computed against, so the loss compounds instead of self-healing.
    pub published: InventoryRecord,
}

/// Origin of a drag-to-place operation. The raycast + placement path
/// is identical for every source; only the generator lookup differs.
/// Inventory drops copy a blueprint into the room's `generators` map
/// under a collision-safe key; catalogue drops resolve the slug against
/// [`crate::catalogue::by_slug`] and stamp a fresh deep-copied
/// blueprint into the room's `generators` map. (A `RoomGenerators`
/// variant for World-Editor-tab drags was documented but never armed
/// by any UI — deleted in #832; the scene context menu's Duplicate
/// (#824) covers stamping another instance of an existing generator.)
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum DropSource {
    #[default]
    Inventory,
    Catalogue,
}

/// Egui → world drag handoff. The UI side sets the generator name and source
/// on drag-start; [`drop::handle_generator_drop`] consumes it on mouse release,
/// runs the raycast, and clears it — whether or not the release landed on a
/// valid ground hit.
///
/// `peer_target` is refreshed every frame by [`crate::ui::people::people_ui`]
/// while a drag is active: it is set to the peer whose row the cursor is
/// currently over (or cleared when the cursor isn't over a peer row). The
/// drop handler consumes it on release to route the drag into an
/// [`crate::protocol::OverlandsMessage::ItemOffer`] instead of a terrain
/// placement. It intentionally is **not** cleared by the inventory or
/// world-editor drag source on its own — the People UI owns the signal
/// because only it can resolve "cursor is over peer row N" via egui's
/// layout.
#[derive(Resource, Default)]
pub struct PendingGeneratorDrop {
    pub generator_name: Option<String>,
    pub source: DropSource,
    pub peer_target: Option<PeerDropTarget>,
}

/// The follow-the-cursor tooltip a live drag carries (#1220 f132).
///
/// Shared by the Inventory row and the Catalogue tree, because the Catalogue
/// is where a new user MEETS drag-to-place and drag-to-gift — and it is also
/// the source that auto-opens the People window mid-drag, steering the user
/// straight at a target whose affordance nothing explained. `egui_ltreeview`
/// does paint the dragged row at the cursor, so the drag was visibly live;
/// what was missing is the sentence saying a drop on a peer gifts it, and
/// the visitor-versus-owner distinction.
///
/// `label` is the display name, never a catalogue slug: the tooltip is read
/// by the person dragging, and `stone_cottage_a` is not a name.
pub fn drag_tooltip(ui: &egui::Ui, id_salt: &str, label: &str, owns_room: bool) {
    egui::Tooltip::always_open(
        ui.ctx().clone(),
        ui.layer_id(),
        egui::Id::new((id_salt, label)),
        egui::PopupAnchor::Pointer,
    )
    .show(|ui| {
        if owns_room {
            ui.label(format!(
                "Place “{label}” — or drop on a peer in the People list to gift"
            ));
        } else {
            // A visitor cannot place: ground placement is owner-only, so
            // gifting is the whole of what this drag can do for them.
            ui.label(format!(
                "Offer “{label}” — drop on a peer in the People list"
            ));
        }
    });
}

/// The sentence the SENDER sees when a gift is refused (#1220 f127).
///
/// One place, because there are four reasons and they used to share one
/// sentence — "@them declined" — which misattributed a mechanical throttle
/// to a person's choice and, worse, taught the sender not to retry in the
/// one case where retrying works. A muted sender reads `Declined`
/// deliberately: telling somebody they have been muted is a privacy leak.
pub fn offer_refusal_line(reason: crate::protocol::DeclineReason, who: &str, item: &str) -> String {
    use crate::protocol::DeclineReason;
    match reason {
        DeclineReason::Declined => format!("{who} declined \"{item}\"."),
        DeclineReason::Busy => {
            format!("{who} was answering another offer — try \"{item}\" again in a moment.")
        }
        DeclineReason::Unavailable => {
            format!("{who} couldn't take \"{item}\" — their inventory is full.")
        }
        DeclineReason::Unanswered => format!("{who} didn't answer about \"{item}\" in time."),
    }
}

/// Per-frame hover snapshot for the peer the cursor is currently over
/// during an armed drag. Populated by the People GUI so the drop handler
/// can route release events without reaching into egui itself.
#[derive(Clone, Debug)]
pub struct PeerDropTarget {
    pub peer_id: bevy_symbios_multiuser::prelude::PeerId,
    pub did: String,
    /// The recipient's name off the ONE ladder
    /// ([`crate::network::PeerLabel`], #1218 f299), already carrying its `@`
    /// when it is a real handle and not when it is a DID head.
    pub label: String,
    /// Why this row cannot take the gift, from
    /// [`crate::ui::people::gift_block_reason`] (#1220 f330).
    ///
    /// An ineligible row is recorded anyway, WITH its reason, so a release
    /// on it can be explained. Before this the row simply was not recorded,
    /// `handle_generator_drop` found no target, and the release fell through
    /// to the silent cancel written for drops over the Inventory window —
    /// so the most common failure in the app's one gifting gesture was
    /// indistinguishable from the feature being broken.
    pub blocked: Option<&'static str>,
}

/// Auto-open the People window the moment a gift-capable drag arms with
/// peers present (#846). Peer drop targets exist ONLY as rendered People
/// rows — with the window closed (the default) a drag had nothing to
/// land on and a visitor's release was a silent no-op. Rising-edge only,
/// so closing People mid-drag is respected.
pub fn open_people_for_gift_drag(
    pending: Res<PendingGeneratorDrop>,
    peers: Query<(), With<crate::state::RemotePeer>>,
    mut panels: ResMut<crate::ui::toolbar::UiPanels>,
    mut was_armed: Local<bool>,
) {
    let armed = pending.generator_name.is_some();
    let rising = armed && !*was_armed;
    *was_armed = armed;
    if rising && !peers.is_empty() && !panels.people {
        panels.people = true;
    }
}

/// The inventory is the wear surface (#1096): everything a Wear / Take off
/// click on a row needs, bundled to stay under Bevy's 16-parameter system
/// ceiling. The avatar record is mutated only on a click (guarded-dirty);
/// the editor state carries the detach queue the publish bundle drains.
#[derive(bevy::ecs::system::SystemParam)]
pub struct WearSurface<'w> {
    live_avatar: Option<ResMut<'w, crate::state::LiveAvatarRecord>>,
    avatar_editor: ResMut<'w, crate::ui::avatar::AvatarEditorState>,
    undo_labels: ResMut<'w, crate::ui::undo::PendingUndoLabels>,
    toasts: ResMut<'w, crate::ui::toast::Toasts>,
}

/// A Wear / Take off click on an inventory row (#1096), applied after the
/// list has released its borrow of the stash.
enum WearAction {
    Wear(String),
    TakeOff(String),
}

/// The Wear / Take off buttons for a wearable row. Which one shows is
/// the item's worn state on the live avatar; reasons a body cannot be
/// dressed render as a disabled button with the reason on hover, the same
/// vocabulary the catalogue uses.
fn wear_buttons(
    ui: &mut egui::Ui,
    name: &str,
    socket: &str,
    action: &mut Option<WearAction>,
    live_avatar: Option<&crate::state::LiveAvatarRecord>,
) {
    // Take off comes before the blocked reasons: something already worn
    // can always come off, even from a body that could not take another.
    if let Some(rig) = live_avatar.and_then(|live| live.0.body.rigged_ref())
        && crate::ui::avatar::is_worn_from(rig, name)
    {
        if ui
            .small_button("Take off")
            .on_hover_text("Take this item off your avatar")
            .clicked()
        {
            *action = Some(WearAction::TakeOff(name.to_string()));
        }
        return;
    }
    // This row is drawn FROM the inventory, so it is loaded by
    // construction (#1233 f261).
    if let Some(reason) =
        crate::ui::avatar::wear_blocked_reason(live_avatar.map(|live| &live.0), true)
    {
        ui.add_enabled(false, egui::Button::new("Wear").small())
            .on_disabled_hover_text(reason);
        return;
    }
    if ui
        .small_button("Wear")
        .on_hover_text(format!("Wear this item at the {socket} socket"))
        .clicked()
    {
        *action = Some(WearAction::Wear(name.to_string()));
    }
}

/// Dress or undress the live avatar from an inventory row (#1096). Both
/// halves of the wardrobe record move together through the avatar
/// editor's own helpers, the undo ring gets a label, and detached records
/// join the editor's delete queue so the next publish tidies them up.
#[allow(clippy::too_many_arguments)]
fn apply_wear_action(
    action: WearAction,
    inventory: &InventoryRecord,
    live_avatar: Option<&mut crate::state::LiveAvatarRecord>,
    avatar_editor: &mut crate::ui::avatar::AvatarEditorState,
    did: &str,
    undo_labels: &mut crate::ui::undo::PendingUndoLabels,
    toasts: &mut crate::ui::toast::Toasts,
    now: f64,
) {
    let Some(live) = live_avatar else {
        toasts.warn(String::from("No avatar loaded yet."), now);
        return;
    };
    match action {
        WearAction::Wear(name) => {
            // Says why rather than returning silently (#1141). The row's
            // button is disabled for every one of these reasons, so this
            // arm should be unreachable — but "should be unreachable" is
            // exactly the assumption that let the catalogue toast a
            // success over a wear that never happened.
            if let Some(reason) = crate::ui::avatar::wear_blocked_reason(Some(&live.0), true) {
                toasts.warn(format!("Could not wear \"{name}\" — {reason}"), now);
                return;
            }
            let Some(record) = crate::ui::avatar::record_for_inventory_item(inventory, &name)
            else {
                toasts.warn(format!("\"{name}\" is not wearable."), now);
                return;
            };
            // No `else` arm: the check above already established a rigged,
            // resolved body under the cap, which is exactly the condition
            // `attach_record` returns `Some` on.
            if let Some(rig) = live.0.body.rigged_mut()
                && crate::ui::avatar::attach_record(rig, record, did).is_some()
            {
                undo_labels.set_avatar(format!("wear {name}"));
            }
        }
        WearAction::TakeOff(name) => {
            let Some(rig) = live.0.body.rigged_mut() else {
                return;
            };
            let detached = crate::ui::avatar::worn_rkeys_from(rig, &name);
            if crate::ui::avatar::take_off_source(rig, &name) > 0 {
                avatar_editor.forget_attachments(detached);
                undo_labels.set_avatar(format!("take off {name}"));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn inventory_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<crate::ui::toolbar::UiPanels>,
    mut chrome: crate::ui::layout::WindowChrome,
    mut commands: Commands,
    session: Option<Res<AtprotoSession>>,
    refresh_ctx: Option<Res<crate::oauth::OauthRefreshCtx>>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut live: Option<ResMut<LiveInventoryRecord>>,
    stored: Option<Res<StoredInventoryRecord>>,
    mut feedback: ResMut<PublishFeedback<InventoryRecord>>,
    mut pending_drop: ResMut<PendingGeneratorDrop>,
    mut state: Local<InventoryEditorState>,
    time: Res<Time>,
    mut publish_shortcut: ResMut<crate::ui::shortcuts::PublishShortcut>,
    recovery: Option<Res<crate::state::InventoryRecordRecovery>>,
    mut wear: WearSurface,
) {
    let WearSurface {
        live_avatar,
        avatar_editor,
        undo_labels,
        toasts,
    } = &mut wear;
    let (Some(live), Some(stored), Some(session), Some(refresh_ctx)) =
        (live.as_mut(), stored, session, refresh_ctx)
    else {
        return;
    };
    // Rows are draggable everywhere: releasing over a peer row in the People
    // window offers the item as a gift, which is a personal transaction and
    // valid in ANY room (#699 — gating the drag on room ownership locked
    // visitors out of gifting entirely). Ground placement is the drop
    // handler's job to police: [`drop::handle_generator_drop`] only mutates
    // the `RoomRecord` when `session.did == room_did`, so a viewport release
    // in someone else's room is a no-op. Ownership here only tunes the drag
    // tooltip so a visitor isn't promised a placement that can't happen.
    let owns_room = room_did
        .as_ref()
        .map(|r| r.0 == session.did)
        .unwrap_or(false);

    let ctx = contexts.ctx_mut().unwrap();

    // Rename dialog — the shared modal (#838): keeps itself open on an
    // empty/taken name with the reason inline, Enter applies, Esc cancels.
    if let Some((old_name, mut new_name)) = state.renaming_generator.clone() {
        match crate::ui::confirm::rename_dialog(
            ctx,
            "Rename Inventory Item",
            &old_name,
            &mut new_name,
            |draft| live.0.generators.contains_key(draft),
        ) {
            crate::ui::confirm::RenameOutcome::Open => {
                state.renaming_generator = Some((old_name, new_name));
            }
            crate::ui::confirm::RenameOutcome::Cancelled => {
                state.renaming_generator = None;
            }
            crate::ui::confirm::RenameOutcome::Renamed(applied) => {
                // Through the record's own rename so wear metadata (#1096)
                // travels with the item — and then the worn props' own
                // provenance, which the stash cannot reach (#1141). The
                // name is the only link between a row and the prop it put
                // on the body; moving one without the other left the row
                // offering Wear on something already worn.
                if live.0.rename_item(&old_name, applied.clone())
                    && let Some(rig) = live_avatar
                        .as_deref_mut()
                        .and_then(|avatar| avatar.0.body.rigged_mut())
                    && crate::ui::avatar::rename_worn_source(rig, &old_name, &applied) > 0
                {
                    // The provenance rides the published attachment
                    // record, so this is a real edit to the avatar and
                    // the save row must show it.
                    undo_labels.set_avatar(format!("rename {old_name} to {applied}"));
                }
                state.renaming_generator = None;
            }
        }
    }

    let (pos, size) = chrome.place(crate::ui::layout::UiWindow::Inventory, ctx);
    // Guarded-dirty (#879): `.open(&mut panels.inventory)` through the
    // `ResMut` would mark UiPanels changed every frame, starving the
    // prefs save debounce — local copy in, write back only on close.
    let mut open = panels.inventory;
    let response = egui::Window::new("Inventory")
        .open(&mut open)
        .default_pos(pos)
        .default_size(size)
        .constrain_to(chrome.available_rect(ctx))
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
            // Deferred out of the closure: the re-read is a `Commands` write
            // and the banner is deep inside the window body.
            let mut reload_stash = false;
            // Degraded-session banner (#840): the fetch fell back to an
            // empty default, so this stash is NOT what's on the PDS.
            if let Some(rec) = recovery.as_deref() {
                egui::Frame::new()
                    .fill(crate::ui::theme::current(ui.ctx()).danger_surface)
                    .inner_margin(6.0)
                    .corner_radius(4.0)
                    .show(ui, |ui| {
                        ui.colored_label(
                            crate::ui::theme::current(ui.ctx()).danger_surface_text,
                            "⚠ Your stash could not be loaded — this shows an empty default.",
                        );
                        ui.label(egui::RichText::new(format!("Reason: {}", rec.reason)).small());
                        ui.label(
                            egui::RichText::new(
                                "Saving would overwrite the stored stash (you'll be asked \
                                 first).",
                            )
                            .small(),
                        );
                        // The non-destructive direction (#1230 f33). The
                        // failure is almost always transient and has
                        // usually healed by the time the user reads this;
                        // the only route back to the real stash used to be
                        // a full logout.
                        if crate::ui::editable::recovery_reload_button(
                            ui,
                            RecordKind::Inventory,
                            crate::state::records_differ(&live.0, &stored.0),
                        )
                        .clicked()
                        {
                            reload_stash = true;
                        }
                    });
                ui.add_space(4.0);
            }
            // Over-cap surfacing (#841): a legacy stash past the cap used
            // to be silently truncated by sanitize on the next login —
            // now it loads intact, reads red here, and blocks publishing
            // until the user decides what to prune.
            let cap = crate::config::state::MAX_INVENTORY_ITEMS;
            let count = live.0.generators.len();
            let over_cap = count > cap;
            if over_cap {
                ui.colored_label(
                    crate::ui::theme::current(ui.ctx()).status.error,
                    format!(
                        "Stored Generators: {count}/{cap} — over the {cap}-item cap; \
                         remove {} to enable saving",
                        if count - cap == 1 {
                            "1 item".to_owned()
                        } else {
                            format!("{} items", count - cap)
                        }
                    ),
                );
            } else {
                ui.label(format!("Stored Generators: {count}/{cap}"));
            }
            ui.separator();


            // Footer FIRST, bottom-up, so its height is measured and the
            // list gets exactly what is left. This used to reserve a flat
            // 80 pt for "the separator + Publish row + feedback line" and
            // hand the scroll area `available_height() - 80` with
            // `auto_shrink([true, false])`, which claims that height
            // whatever its content — so a `publish_status_line` carrying a
            // long XRPC failure, wrapped to four lines in this 300 pt-wide
            // window, made the content taller than the window and egui's
            // `Resize` ratcheted it up every frame until it filled the
            // screen. Chat hit exactly that (#1280); this one had ~26 pt of
            // headroom left and was one wrapped error away.
            //
            // The footer now renders before the list, so a delete made this
            // frame reaches the Save row's dirty check on the next one — a
            // single frame of lag on an indicator, against a window that
            // could climb off the screen.
            crate::ui::layout::bottom_anchored(ui, |ui| {
                // Shared Save / Load / Reset row + status line
                // (`ui::editable`), identical to the World and Avatar
                // editors. Dirty is derived (a serialized diff against the stored
                // snapshot) so the row needs no per-edit flag; Inventory now
                // also gets Load-from-PDS (revert) and Reset-to-default
                // (empty the stash) — it previously had Publish only.
                //
                // Both baselines are cached (#1135, the #674 pattern): the stored
                // side re-serializes only when the resource changes and the empty
                // default only once, so an open panel serializes the LIVE stash
                // ONCE per frame instead of three whole trees. The comparisons are
                // value-identical to `records_differ` — `Option<Value>` on both
                // sides, `.ok()` semantics preserved — so dirty and can_reset are
                // frame-accurate exactly as before.
                if state
                    .stored_baseline
                    .as_ref()
                    .is_none_or(|(tick, _)| *tick != stored.last_changed())
                {
                    state.stored_baseline =
                        Some((stored.last_changed(), serde_json::to_value(&stored.0).ok()));
                }
                if state.default_baseline.is_none() {
                    state.default_baseline =
                        Some(serde_json::to_value(InventoryRecord::default()).ok());
                }
                let live_value = serde_json::to_value(&live.0).ok();
                let dirty = match state.stored_baseline.as_ref() {
                    Some((_, baseline)) => *baseline != live_value,
                    None => true,
                };
                let can_reset = state
                    .default_baseline
                    .as_ref()
                    .is_none_or(|baseline| *baseline != live_value);
                // Publishing is blocked while over the cap (#841) — the red
                // header line explains; mirrors the hard-ceiling size block.
                let within_cap = live.0.generators.len() <= crate::config::state::MAX_INVENTORY_ITEMS;
                // `session` + `refresh_ctx` are guaranteed present (the early
                // return above bails otherwise), so a publish is always
                // attemptable while dirty.
                //
                // Size readout: the stash is one record PER ITEM (#696), so
                // the per-record budget applies to the largest single item —
                // not the whole stash. Same throttled cache as the other
                // editors, custom measurement.
                let now = time.elapsed_secs_f64();
                crate::ui::editable::refresh_size_readout(
                    &mut *feedback,
                    &live.0,
                    now,
                    crate::pds::inventory::measure_publish,
                );
                let size = feedback.live_size.clone();
                let ctrl_s = publish_shortcut.take(crate::ui::shortcuts::EditorKind::Inventory);
                let mut do_publish = false;
                match save_load_reset_row(
                    ui,
                    crate::ui::editable::SaveRow {
                        kind: RecordKind::Inventory,
                        dirty,
                        can_publish: within_cap,
                        can_reset,
                        size: &size,
                        publish_shortcut: ctrl_s,
                        status: &mut feedback.status,
                        // Inventory has no undo stack (#866) — keep the modal.
                        confirm: Some(&mut state.row_confirm),
                        reset: crate::ui::editable::ResetWording::EmptyStash {
                            items: live.0.generators.len(),
                        },
                    },
                ) {
                    RecordAction::None => {}
                    RecordAction::Refused(reason) => {
                        toasts.info(crate::ui::editable::ctrl_s_refused(&reason), now);
                    }
                    RecordAction::Publish => {
                        // Clobber protection (#840): while the session is
                        // degraded, saving this (empty-default) stash would
                        // wipe whatever is actually stored — ask first.
                        match recovery.as_deref() {
                            Some(rec) => crate::ui::editable::request_overwrite_confirm(
                                &mut state.publish_guard,
                                RecordKind::Inventory,
                                &rec.reason,
                            ),
                            None => do_publish = true,
                        }
                    }
                    RecordAction::Load => {
                        live.0 = stored.0.clone();
                    }
                    RecordAction::Reset => {
                        // The baseline above is the default's serialized FORM; the
                        // record itself is `default()`, which for an inventory is
                        // simply empty and costs nothing to rebuild here.
                        live.0 = InventoryRecord::default();
                    }
                }
                if state
                    .publish_guard
                    .show(ui.ctx(), "inventory-recovery-publish")
                    .is_some()
                {
                    // Acknowledged. The marker retires when the poll system
                    // sees the write land (#1199), not here.
                    do_publish = true;
                }
                if do_publish {
                    feedback.status = PublishStatus::Publishing { since_secs: now };
                    spawn_publish_inventory_task(
                        &mut commands,
                        &session,
                        &refresh_ctx,
                        live.0.clone(),
                        stored.0.clone(),
                        now,
                    );
                }

                publish_status_line(ui, &feedback.status, now, dirty);

                crate::ui::layout::fill_above(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([true, false])
                        .show(ui, |ui| {
                            let mut to_remove: Option<String> = None;
                            let mut wear_action: Option<WearAction> = None;
                            let mut names: Vec<String> = live.0.generators.keys().cloned().collect();
                            // Case-insensitive (#841): plain `sort()` put "Zebra"
                            // before "apple".
                            names.sort_by_key(|name| name.to_lowercase());

                            for name in names {
                                ui.horizontal(|ui| {
                                    // Generators that make no sense as a dropped
                                    // placement (terrain + water are room-scoped, not
                                    // point-placed) render as a plain label so the
                                    // drag sense doesn't arm a release we'd ignore.
                                    let is_placeable = live
                                        .0
                                        .generators
                                        .get(&name)
                                        .map(is_drop_placeable)
                                        .unwrap_or(false);
                                    // An item this build cannot decode (#1207): a
                                    // gift from a newer client, or a stash saved by
                                    // one. It cannot be placed, worn, renamed or
                                    // written back — only kept or deleted — and it
                                    // is what disables Save for the whole stash.
                                    let unreadable = live
                                        .0
                                        .generators
                                        .get(&name)
                                        .is_some_and(|g| matches!(g.kind, GeneratorKind::Unknown));
                                    // What KIND of blueprint each row is (#841) —
                                    // names alone ("cuboid_2", "my_tree") didn't say.
                                    let kind_tag = live
                                        .0
                                        .generators
                                        .get(&name)
                                        .map(|g| g.kind_tag())
                                        .unwrap_or("?");
                                    // Wearables say so, and where (#1096).
                                    let kind_tag = match live.0.wear.get(&name) {
                                        Some(meta) => format!("{kind_tag} · wearable, {}", meta.socket),
                                        None => kind_tag.to_string(),
                                    };
                                    if is_placeable {
                                        // The ⠿ handle + grab cursor make the row
                                        // read as draggable (#832) — it used to be
                                        // a plain label whose drag sense was
                                        // discoverable only by accident.
                                        let label = egui::Label::new(format!("☰ {name}"))
                                            .sense(egui::Sense::click_and_drag());
                                        let resp = ui.add(label).on_hover_cursor(egui::CursorIcon::Grab);
                                        ui.label(
                                            egui::RichText::new(format!("({kind_tag})"))
                                                .small()
                                                .color(crate::ui::theme::current(ui.ctx()).text_weak),
                                        );
                                        if resp.drag_started() {
                                            pending_drop.generator_name = Some(name.clone());
                                            pending_drop.source = DropSource::Inventory;
                                        }
                                        if resp.dragged()
                                            && pending_drop.generator_name.as_deref() == Some(name.as_str())
                                        {
                                            // Follow-the-cursor tooltip keeps the
                                            // dragger oriented while they hunt for a
                                            // target — without it, the drag is
                                            // invisible once the pointer leaves the
                                            // row. Shared with the Catalogue since
                                            // #1220 f132.
                                            drag_tooltip(ui, "inv_drag_tip", &name, owns_room);
                                        }
                                    } else if unreadable {
                                        ui.label(&name);
                                        ui.label(
                                            egui::RichText::new(UNREADABLE_ITEM_TAG)
                                                .small()
                                                .color(crate::ui::theme::current(ui.ctx()).status.warn),
                                        )
                                        .on_hover_text(UNREADABLE_ITEM_HOVER);
                                    } else {
                                        // Room-scoped kinds (terrain/water) can't be
                                        // point-placed — say so instead of rendering
                                        // an identical-looking row that silently
                                        // refuses to drag (#832; the catalogue
                                        // already explains the same distinction).
                                        ui.label(&name);
                                        ui.label(
                                            egui::RichText::new(format!("({kind_tag} — room-scoped)"))
                                                .small()
                                                .color(crate::ui::theme::current(ui.ctx()).text_weak),
                                        );
                                    }
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if crate::ui::affordances::remove_button(
                                                ui,
                                                "Delete this item from your stash",
                                            )
                                            .clicked()
                                            {
                                                to_remove = Some(name.clone());
                                            }
                                            if ui
                                                .add_enabled(
                                                    !unreadable,
                                                    egui::Button::new("Rename").small(),
                                                )
                                                .on_disabled_hover_text(UNREADABLE_ITEM_HOVER)
                                                .clicked()
                                            {
                                                state.renaming_generator =
                                                    Some((name.clone(), name.clone()));
                                            }
                                            // Wear / Take off (#1096) for items that
                                            // carry wear metadata.
                                            if let Some(meta) = live.0.wear.get(&name) {
                                                wear_buttons(
                                                    ui,
                                                    &name,
                                                    meta.socket.as_str(),
                                                    &mut wear_action,
                                                    live_avatar.as_deref(),
                                                );
                                            }
                                        },
                                    );
                                });
                            }
                            // Applied after the list so the borrow of `live` the rows
                            // hold is released before the avatar record is dressed.
                            if let Some(action) = wear_action.take() {
                                apply_wear_action(
                                    action,
                                    &live.0,
                                    live_avatar.as_deref_mut(),
                                    avatar_editor.as_mut(),
                                    &session.did,
                                    undo_labels.as_mut(),
                                    toasts.as_mut(),
                                    time.elapsed_secs_f64(),
                                );
                            }
                            // Delete asks first (#1200): this is the one surface with
                            // no undo, and the click sits beside Rename. A worn item
                            // is named as such — deleting takes it off too, so the
                            // prop cannot linger on the body with the only row that
                            // offered "Take off" gone (finding 131).
                            if let Some(name) = to_remove {
                                let worn = live_avatar
                                    .as_deref()
                                    .and_then(|avatar| avatar.0.body.rigged_ref())
                                    .is_some_and(|rig| crate::ui::avatar::is_worn_from(rig, &name));
                                let body = if worn {
                                    format!(
                                        "You are wearing \"{name}\" — deleting it also takes it off. \
                                         The inventory has no undo; the item stays on your PDS until \
                                         you save."
                                    )
                                } else {
                                    String::from(
                                        "The inventory has no undo; the item stays on your PDS until \
                                         you save.",
                                    )
                                };
                                state.delete_confirm.request(
                                    format!("Delete \"{name}\"?"),
                                    body,
                                    "Delete",
                                    name,
                                );
                            }
                            if let Some(name) = state.delete_confirm.show(ui.ctx(), "inventory-delete") {
                                let worn = live_avatar
                                    .as_deref()
                                    .and_then(|avatar| avatar.0.body.rigged_ref())
                                    .is_some_and(|rig| crate::ui::avatar::is_worn_from(rig, &name));
                                if worn {
                                    apply_wear_action(
                                        WearAction::TakeOff(name.clone()),
                                        &live.0,
                                        live_avatar.as_deref_mut(),
                                        avatar_editor.as_mut(),
                                        &session.did,
                                        undo_labels.as_mut(),
                                        toasts.as_mut(),
                                        time.elapsed_secs_f64(),
                                    );
                                }
                                if live.0.remove_item(&name).is_some() {
                                    toasts.info(
                                        if worn {
                                            format!("Deleted \"{name}\" and took it off.")
                                        } else {
                                            format!("Deleted \"{name}\".")
                                        },
                                        time.elapsed_secs_f64(),
                                    );
                                }
                            }
                        });
                });
                reload_stash
            })
        });
    // #1230 f33: re-read the stored stash from the PDS. `poll_record_task`
    // installs it as live AND stored on a clean resolution and retires the
    // recovery marker, so the banner clears itself; the button is disabled
    // while dirty, so nothing unsaved is in its way.
    if response.as_ref().and_then(|r| r.inner).unwrap_or(false) {
        crate::loading::fetch::spawn_record_fetch::<InventoryRecord>(
            &mut commands,
            session.did.clone(),
            0,
            time.elapsed_secs_f64(),
        );
    }
    if let Some(response) = response {
        chrome.remember(
            crate::ui::layout::UiWindow::Inventory,
            response.response.rect,
        );
    }
    if panels.inventory && !open {
        panels.inventory = false;
    }
}

/// Spawn the async inventory publish. Since #696 this commits the
/// live-vs-`stored` diff as per-item records in ONE atomic `applyWrites`
/// batch (see [`crate::pds::inventory`]), so the caller must pass the
/// stored snapshot the diff is computed against. `pub(crate)` because the
/// unsaved-edits guard ([`crate::ui::unsaved_guard`]) and the offer-accept
/// path ([`crate::ui::people`]) drive the same pipeline — the shared
/// [`poll_publish_inventory_tasks`] system lands the result either way.
pub(crate) fn spawn_publish_inventory_task(
    commands: &mut Commands,
    session: &AtprotoSession,
    refresh: &crate::oauth::OauthRefreshCtx,
    record: InventoryRecord,
    stored: InventoryRecord,
    now: f64,
) {
    // The inventory record is the local user's own, saved to their PDS → the
    // write DID is the session DID (like the avatar save).
    let did = session.did.clone();
    let session_clone = session.clone();
    let refresh_clone = refresh.clone();
    // Per-item wire format → the budget gauge tracks the largest single
    // item record, not the whole stash (#694/#696).
    let record_bytes = crate::pds::inventory::max_item_bytes(&record);
    let published = record.clone();
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            let client = crate::config::http::default_client();
            crate::pds::publish_inventory_record(
                &client,
                &session_clone,
                &refresh_clone,
                &record,
                &stored,
            )
            .await
        };
        crate::config::http::run_or(
            fut,
            Err(crate::config::http::timed_out("inventory publish")),
        )
        .await
    });
    commands.spawn(PublishInventoryTask {
        task,
        did,
        spawned_at: now,
        record_bytes,
        published,
    });
}

#[allow(clippy::too_many_arguments)]
pub fn poll_publish_inventory_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PublishInventoryTask)>,
    mut stored: Option<ResMut<StoredInventoryRecord>>,
    mut feedback: ResMut<PublishFeedback<InventoryRecord>>,
    mut session_log: ResMut<SessionLog>,
    mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
    time: Res<Time>,
    mut panels: ResMut<crate::ui::toolbar::UiPanels>,
    mut toasts: ResMut<crate::ui::toast::Toasts>,
    // A result for another identity must not pin `stored` (#1204).
    session: Option<Res<AtprotoSession>>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let spawned_at = task.spawned_at;
        let Some(result) = crate::ui::editable::poll_or_expire(
            &mut task.task,
            spawned_at,
            time.elapsed_secs_f64(),
            "inventory publish",
        ) else {
            continue;
        };
        commands.entity(entity).despawn();
        if crate::ui::room::stale_result(
            "inventory publish",
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
            RecordKind::Inventory,
            task.record_bytes,
        );
        match result {
            Ok(()) => {
                info!("Inventory record saved to PDS");
                // The published snapshot, not `live` (#1116) — see
                // `PublishInventoryTask::published`.
                if let Some(stored) = stored.as_mut() {
                    stored.0 = task.published.clone();
                }
                // The stored copy is now exactly what was written, so the
                // recovery marker retires where success is known (#1199).
                commands.remove_resource::<crate::state::InventoryRecordRecovery>();
                feedback.status = PublishStatus::Success { at_secs: now };
                session_log.info(
                    now,
                    EventPayload::RecordWriteCompleted {
                        record: RecordKind::Inventory,
                        did,
                        duration_secs,
                    },
                );
            }
            // Surfaced OUTSIDE the Inventory window (#843): the
            // accept-a-gift flow publishes without the window open, so its
            // failure used to be invisible — the item looked saved and
            // evaporated on the next login. Since #1137 all three records
            // report through the one helper.
            Err(e) => crate::ui::editable::report_publish_failure(
                RecordKind::Inventory,
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

/// The row tag for an item this build cannot decode (#1207). It used to
/// share the terrain/water "room-scoped" tag, which sent the owner looking
/// for a placement rule when the item was simply unreadable.
pub const UNREADABLE_ITEM_TAG: &str = "(from a newer version of Overlands)";

/// What the owner can do about an unreadable item.
pub const UNREADABLE_ITEM_HOVER: &str = "This item was made by a newer version of Overlands. \
     This build cannot read it, so it cannot be placed, worn, renamed or saved — and while it \
     is in your stash, the stash cannot be saved at all. Update Overlands, or delete the item.";

/// Which generator kinds can be point-placed via drag-and-drop.
/// Terrain + water describe whole-room scope (one heightmap / one water
/// plane) so a ground-level placement of them is nonsensical; they stay
/// editable via the World Editor tabs.
pub fn is_drop_placeable(generator: &Generator) -> bool {
    !matches!(
        generator.kind,
        GeneratorKind::Terrain(_) | GeneratorKind::Water { .. } | GeneratorKind::Unknown
    )
}

/// Pick an inventory key for a gift arriving via [`crate::protocol::OverlandsMessage::ItemOffer`].
/// Policy: if the incoming name is free, use it verbatim; otherwise
/// append `_2`, `_3`, … until we find an unused slot. This matches the
/// user-approved design ("auto-rename with _2 suffix"). Equality of
/// existing entries is not consulted — a gift always lands as a new item,
/// because two players may each have tweaked the same base blueprint and
/// silently coalescing would lose data.
pub fn choose_inventory_gift_key(
    existing: &HashMap<String, Generator>,
    incoming_name: &str,
) -> String {
    choose_gift_key(|name| existing.contains_key(name), incoming_name)
}

/// [`choose_inventory_gift_key`] over any taken-set. The accept path asks
/// with BOTH the live and the stored stash (#1200): a key free in `live`
/// only because the owner deleted that item locally would, published
/// from `stored`, overwrite the stored item with the gift.
fn choose_gift_key(is_taken: impl Fn(&str) -> bool, incoming_name: &str) -> String {
    if !is_taken(incoming_name) {
        return incoming_name.to_string();
    }
    for i in 2u32..u32::MAX {
        let candidate = format!("{incoming_name}_{i}");
        if !is_taken(&candidate) {
            return candidate;
        }
    }
    incoming_name.to_string()
}

/// Accept a gift (#1200): land it in `live` and return the record the
/// auto-publish writes, which is `stored` plus the gift — never `live`.
///
/// The publish on accept exists so the gift survives a session that ends
/// before the owner presses Save. Publishing the whole live stash for it
/// committed every OTHER unsaved edit at a moment a remote peer chose:
/// `plan_item_writes` turns each name present in `stored` and missing
/// from `live` into a delete, so a mis-click delete the owner meant to
/// "Revert to saved" became permanent on Accept, and the revert itself
/// stopped working because the poll pinned `stored` to the published
/// snapshot. The gift's key is free in both stashes for the same reason.
pub fn accept_gift(
    live: &mut InventoryRecord,
    stored: &InventoryRecord,
    incoming_name: &str,
    generator: Generator,
    wear: Option<crate::pds::inventory::WearMeta>,
) -> (String, InventoryRecord) {
    let key = choose_gift_key(
        |name| live.generators.contains_key(name) || stored.generators.contains_key(name),
        incoming_name,
    );
    live.put_item(key.clone(), generator.clone(), wear.clone());
    let mut payload = stored.clone();
    payload.put_item(key.clone(), generator, wear);
    (key, payload)
}

/// Land an accepted gift in the stash (#1108): under
/// [`choose_inventory_gift_key`]'s slot, through
/// [`InventoryRecord::put_item`] so the wear side table stays in step — a
/// gifted wearable is wearable from the Inventory window straight away,
/// exactly as a catalogue copy would be; decor stays decor. Returns the key
/// it landed under.
pub fn store_accepted_gift(
    inventory: &mut InventoryRecord,
    incoming_name: &str,
    generator: Generator,
    wear: Option<crate::pds::inventory::WearMeta>,
) -> String {
    let key = choose_inventory_gift_key(&inventory.generators, incoming_name);
    inventory.put_item(key.clone(), generator, wear);
    key
}

#[cfg(test)]
mod gift_tests {
    use super::*;
    use crate::pds::inventory::WearMeta;

    /// #1200 (finding 117). Sequence: the owner deletes "lantern" and
    /// "bench" by mis-click, meaning to "Revert to saved"; a gift named
    /// "lantern" arrives and they press Accept. The old accept published
    /// the whole live stash, so both deletes went to the PDS forever. The
    /// publish payload must be the stored stash plus the gift and nothing
    /// else — and the gift must not land on the stored "lantern" either.
    #[test]
    fn accepting_a_gift_publishes_the_gift_and_nothing_else() {
        let mut stored = InventoryRecord::default();
        stored.put_item(String::from("lantern"), Generator::default(), None);
        stored.put_item(String::from("bench"), Generator::default(), None);
        let mut live = stored.clone();
        live.remove_item("lantern");
        live.remove_item("bench");
        live.put_item(String::from("unsaved_new"), Generator::default(), None);

        let (key, payload) = accept_gift(
            &mut live,
            &stored,
            "lantern",
            Generator::default_cuboid(),
            None,
        );
        assert_eq!(
            key, "lantern_2",
            "a name the owner deleted locally is still taken on the PDS"
        );
        assert!(live.generators.contains_key("lantern_2"));
        assert!(
            !live.generators.contains_key("lantern"),
            "live keeps its edits"
        );
        // The payload is exactly stored + gift.
        assert!(payload.generators.contains_key("lantern"));
        assert!(payload.generators.contains_key("bench"));
        assert!(payload.generators.contains_key("lantern_2"));
        assert!(
            !payload.generators.contains_key("unsaved_new"),
            "an unsaved local addition must not ride along"
        );
        assert_eq!(payload.generators.len(), 3);
        // And the owner's escape hatch survives: live still differs from
        // what will be pinned as stored, so Revert to saved has something
        // to revert to.
        assert!(crate::state::records_differ(&live, &payload));
    }

    /// #1108: before this, a gift went straight into `generators` and the
    /// wear side table never heard of it — every gifted wearable arrived as
    /// decor. The accept path must land wear metadata with the item, under
    /// the collision-renamed key, and leave decor as decor.
    #[test]
    fn an_accepted_wearable_gift_is_wearable_and_decor_stays_decor() {
        let mut inventory = InventoryRecord::default();
        inventory.put_item(String::from("circlet"), Generator::default(), None);

        let meta = WearMeta::for_entry(symbios_avatar::Socket::Crown, None);
        let key = store_accepted_gift(
            &mut inventory,
            "circlet",
            Generator::default(),
            Some(meta.clone()),
        );
        assert_eq!(
            key, "circlet_2",
            "a taken name is suffixed, never coalesced"
        );
        assert!(
            inventory.is_wearable(&key),
            "the gifted wearable can be worn"
        );
        assert_eq!(inventory.wear.get(&key), Some(&meta));
        assert!(
            !inventory.is_wearable("circlet"),
            "the existing decor stayed decor"
        );

        let key = store_accepted_gift(&mut inventory, "bench", Generator::default(), None);
        assert_eq!(key, "bench");
        assert!(!inventory.is_wearable("bench"));
    }
}

#[cfg(test)]
mod refusal_tests {
    use super::*;
    use crate::protocol::DeclineReason;

    /// #1220 f119. The sequence: a friend gifts you "lantern", you already
    /// have one, you accept — and the item is in your stash as "lantern_2"
    /// with no explanation, under a name the modal never showed you. The
    /// accept arm used to discard the landed key entirely, so the one moment
    /// a gift becomes yours was the least-confirmed event in the lifecycle
    /// AND the recipient could not find the item afterwards.
    ///
    /// This pins the fact the toast branches on: a collision changes the
    /// key, so a confirmation that echoed the OFFERED name would be wrong.
    #[test]
    fn a_colliding_gift_lands_under_a_different_name() {
        let mut live = crate::pds::InventoryRecord::default();
        live.put_item(
            String::from("lantern"),
            crate::pds::Generator::default(),
            None,
        );
        let stored = live.clone();

        let (key, payload) = accept_gift(
            &mut live,
            &stored,
            "lantern",
            crate::pds::Generator::default(),
            None,
        );
        assert_ne!(
            key, "lantern",
            "the name the modal showed is not the name it has"
        );
        assert!(live.generators.contains_key(&key));
        assert!(payload.generators.contains_key(&key));

        // No collision: the key is the offered name, and the confirmation
        // has nothing to explain.
        let mut empty = crate::pds::InventoryRecord::default();
        let (key, _) = accept_gift(
            &mut empty,
            &crate::pds::InventoryRecord::default(),
            "lantern",
            crate::pds::Generator::default(),
            None,
        );
        assert_eq!(key, "lantern");
    }

    /// #1220 f127. The sequence: you gift two friends in quick succession,
    /// the second one's client is still showing the first dialog, and you
    /// are told "@second declined" — a mechanical throttle reported as a
    /// person's choice, and phrasing that teaches you not to retry in the
    /// one case where retrying works.
    #[test]
    fn each_refusal_reads_as_the_thing_that_actually_happened() {
        let declined = offer_refusal_line(DeclineReason::Declined, "@them", "lantern");
        assert!(declined.contains("declined"), "{declined}");

        let busy = offer_refusal_line(DeclineReason::Busy, "@them", "lantern");
        assert!(
            !busy.contains("declined"),
            "a throttle is not a refusal: {busy}"
        );
        assert!(busy.contains("again"), "and it must invite a retry: {busy}");

        let full = offer_refusal_line(DeclineReason::Unavailable, "@them", "lantern");
        assert!(full.contains("full"), "{full}");
        assert!(!full.contains("declined"), "{full}");

        let quiet = offer_refusal_line(DeclineReason::Unanswered, "@them", "lantern");
        assert!(quiet.contains("didn't answer"), "{quiet}");
        assert!(!quiet.contains("declined"), "{quiet}");
    }

    /// Every sentence names the item and the person, because the sender may
    /// have several offers out at once and a toast that says only "declined"
    /// is unattributable.
    #[test]
    fn every_refusal_names_the_person_and_the_item() {
        for reason in [
            DeclineReason::Declined,
            DeclineReason::Busy,
            DeclineReason::Unavailable,
            DeclineReason::Unanswered,
        ] {
            let line = offer_refusal_line(reason, "@them", "lantern");
            assert!(line.contains("@them"), "{line}");
            assert!(line.contains("lantern"), "{line}");
        }
    }
}
