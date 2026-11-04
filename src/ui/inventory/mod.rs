//! Personal inventory stash UI.
//!
//! The Inventory window lists every item the owner has tucked aside across
//! editing sessions: `Generator` blueprints, and since #1838 whole avatars
//! saved from the Avatar editor. An avatar row's expansion offers Wear,
//! which replaces the live avatar in one undo step - unsaved until the
//! Avatar editor's Save - plus Rename and delete; its pane says what kind of
//! avatar it is and how many wearables it carries. Since #1839 the stash
//! also holds whole worlds saved from the World Editor: a world row's
//! expansion offers Load into my world - in your own world only, after a
//! confirm, replacing the world as a re-roll does, one undo step and unsaved
//! until the World Editor's Save ([`world`]) - plus Rename and delete; its
//! pane is a text card. A click selects a row (#1301): the selected row
//! expands in place to carry that item's controls - Wear / Take off,
//! Rename, delete - and a pane beside the list pictures it and says what
//! the row does not ([`browser`]). Once the stash diverges from the
//! PDS-persisted copy, "Save" commits the live-vs-stored diff as one atomic
//! `com.atproto.repo.applyWrites` batch - one record per item (#696).
//! The stash is loaded during `AppState::Loading`
//! (see [`crate::loading::start_inventory_record_fetch`]) so a freshly-logged-in
//! owner always sees the previously-saved items the moment they land in-game.
//!
//! `InventoryRecord` does not derive `PartialEq` (the underlying `Generator`
//! enum doesn't either, because its variants carry types that themselves
//! would need full equality), so the dirty check round-trips through
//! `serde_json` - same pattern the world editor uses for its Reset button.
//!
//! Drag-to-place: each thing's row is a drag source. When the owner releases
//! a drag over the 3D viewport while standing in their own room,
//! [`drop::handle_generator_drop`] raycasts against the terrain and appends
//! a fresh `Placement::Absolute` to the live `RoomRecord`, copying the
//! dragged generator into the room's `generators` map on first use.
//!
//! Drag-to-gift: releasing the same drag over a peer row in the People
//! window routes it into an `ItemOffer` instead. Gifting works in ANY room
//! (#699) - only the ground-placement branch is owner-gated, and the drop
//! handler enforces that, not the drag source. Since #1844 an avatar row,
//! and a world row whose world is saved, arm a GIFT-ONLY drag: it is
//! offered the same way and places nothing. An accepted thing or avatar
//! lands at the click; an accepted world lands when its parts have been
//! fetched from the giver's saved copy ([`gift_world`]). Every kind lands
//! by one rule ([`accept_gift`]).

pub(crate) mod browser;
mod drop;
pub(crate) mod gift_world;
pub(crate) mod world;

pub use browser::InventoryBrowser;
/// The drop's generator keying, for the agent's `place` (#1422) -
/// Unix-only like the agent.
#[cfg(unix)]
pub(crate) use drop::choose_room_generator_key;
/// The gift's send path, shared with the agent's `gift give` (#1423) -
/// Unix-only like the agent.
#[cfg(unix)]
pub(crate) use drop::{GiftSending, gift_contents, send_gift_offer};
pub(crate) use drop::{handle_generator_drop, preview_generator_drop};
pub use gift_world::{GiftWorldFetch, GiftWorldTask, poll_gift_world_tasks};
pub use world::{LoadWorldTask, WorldLoadsEnded, poll_world_load_tasks};

use browser::{
    AvatarRowAction, PaneItem, PaneWhat, RowGesture, RowKind, WorldRowAction, apply_row_gesture,
    avatar_row_controls, avatar_tag, draw_pane, stash_row, world_row_controls, world_tag,
};

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::diagnostics::SessionLog;
use crate::diagnostics::event::{EventPayload, RecordKind};
use crate::pds::{Generator, GeneratorKind, InventoryRecord};
use crate::state::{
    CurrentRoomDid, LiveInventoryRecord, PublishFeedback, PublishStatus, StoredInventoryRecord,
};
use crate::ui::editable::{RecordAction, publish_status_line_within, save_load_reset_row};

/// Whether one stash row matches the (already trimmed and lower-cased)
/// search query (#1275 f134).
///
/// Pure, and deliberately so - the Catalogue's `matches` is, and this
/// window has no harness of any kind. The three facets are exactly the
/// three the row already DRAWS: its name, the kind tag beside it, and the
/// wearable/socket suffix that replaces the tag on a worn item. Filtering
/// on a facet the row does not show would leave the user unable to see why
/// something matched.
///
/// **What it must not do:** the filter decides which rows are drawn and
/// nothing else. Drag, gift, wear, rename and delete all act on the row's
/// own `name`, the over-cap count reads `item_count()`, and the Save
/// row's dirty check compares whole records - so a hidden row is still in
/// the stash, still saved, and still worn. That is the property
/// `hiding_a_row_does_not_change_the_stash` pins.
pub(crate) fn row_matches(
    name: &str,
    kind_tag: &str,
    wear: Option<&crate::pds::inventory::WearMeta>,
    q: &str,
) -> bool {
    if q.is_empty() {
        return true;
    }
    if name.to_lowercase().contains(q) || kind_tag.to_lowercase().contains(q) {
        return true;
    }
    match wear {
        // "wearable" is the app's word for these (#1266's decision 4) and
        // it is printed on the row, so it is searchable; so is the socket.
        Some(meta) => {
            "wearable".contains(q)
                || crate::pds::avatar::socket_label(&meta.socket)
                    .to_lowercase()
                    .contains(q)
        }
        None => false,
    }
}

/// The sentence the stash list shows when no row is drawn (#1275 f134),
/// mirroring the Catalogue's [`crate::ui::catalogue::empty_state`]: an
/// empty rectangle cannot say whether the search was wrong or the stash is
/// empty, so each arm names its own rule.
pub(crate) fn empty_state(stash_is_empty: bool) -> &'static str {
    if stash_is_empty {
        "Your inventory is empty. Copy an item from the Catalogue, or accept \
         a gift, and it lands here."
    } else {
        "No items match that search. This looks at the name, the kind and the \
         socket a wearable sits on."
    }
}

/// Persistent UI-only state for the Inventory window. Held in a `Local` so
/// it lives for the lifetime of the system without polluting the global
/// resource table.
#[derive(Default)]
pub struct InventoryEditorState {
    /// Active rename modal: `(original_key, draft_key)`.
    pub renaming_generator: Option<(String, String)>,
    /// Search query for the stash list (#1275 f134). A `Local` field, not a
    /// resource, so it survives frames without a change tick to starve -
    /// the Catalogue's equivalent lives on a `ResMut` and has to bypass
    /// change detection for exactly that reason.
    pub search: String,
    /// Pending Revert/Reset confirmation for the shared save row (#838).
    pub row_confirm: crate::ui::confirm::ConfirmState<RecordAction>,
    /// Pending row-delete confirmation (#1200): the stash has no undo by
    /// owner decision, and the red minus sits beside Rename. Carries the
    /// item name; the body says whether it is worn.
    pub delete_confirm: crate::ui::confirm::ConfirmState<String>,
    /// Pending Load into my world confirmation (#1839), carrying the world
    /// item's name: a load replaces everything in the world.
    pub load_confirm: crate::ui::confirm::ConfirmState<String>,
    /// Pending publish-after-degraded-fetch confirmation (#840): while
    /// [`crate::state::InventoryRecordRecovery`] is present the stash
    /// shows the empty default and saving would wipe the stored one -
    /// the first publish asks first.
    pub publish_guard: crate::ui::confirm::ConfirmState<()>,
    /// Serialized form of [`StoredInventoryRecord`] for the per-frame dirty
    /// check (#1135), the same cache the room editor got in #674.
    ///
    /// The uncached form ran `records_differ` twice - live-vs-stored and
    /// live-vs-default - and `records_differ` is
    /// `serde_json::to_value(a) != serde_json::to_value(b)`, so an open panel
    /// built three whole `Value` trees of the stash every frame. The stash
    /// holds up to `MAX_INVENTORY_ITEMS` generator trees, each allowed the
    /// full 100 KiB record budget, and a decorating session parks this panel
    /// open for minutes at a time. #674 fixed exactly this for the room
    /// editor and said the pattern applied elsewhere too; inventory was
    /// left behind.
    ///
    /// #1135 took it from three trees a frame to one, and stopped there.
    /// The remaining one was still the whole stash, EVERY frame: measured
    /// at 3.0 ms with the old 50-item cap and 33.9 ms at 500. See
    /// [`Self::live_baseline`], which #1292 added to close it - that, and
    /// not any limit on the wire, is what the item cap was really bounded
    /// by.
    ///
    /// The original wording of this comment said "the room and avatar
    /// editors", and that was wrong: the avatar editor had a cache for the
    /// default RECORD and no serialized baseline on either side, so it
    /// went on paying six whole-record `Value` trees a frame until #1270
    /// f273. Corrected rather than deleted, because a comment asserting a
    /// fix that does not exist is how the gap survived two tranches.
    ///
    /// Keyed by the resource's `last_changed` tick rather than
    /// `is_changed()`, for the reason #674 records: the change flag is
    /// consumed even on frames where this system early-returns, which would
    /// leave a stale baseline behind afterwards.
    stored_baseline: Option<(bevy::ecs::change_detection::Tick, Option<serde_json::Value>)>,
    /// Serialized form of the default (empty) stash, for the `can_reset`
    /// comparison. Unlike the room's, this has no DID to key on and no
    /// procedural build behind it - `InventoryRecord::default()` is empty -
    /// so it is built once on first use and never invalidated.
    default_baseline: Option<Option<serde_json::Value>>,
    /// Serialized form of [`LiveInventoryRecord`] for the same dirty check
    /// (#1292), keyed on the resource's change tick like
    /// [`Self::stored_baseline`].
    ///
    /// #1135 cut this footer from three whole-record serializations a frame
    /// to one, and stopped there because the LIVE side has no tick to key
    /// on - except that it does, and the cap was small enough that nobody
    /// measured. It is not small any more: a whole-stash
    /// `serde_json::Value` costs 3.0 ms at 50 items, 17.5 ms at 200 - past
    /// the entire 60 fps frame budget - and 33.9 ms at 500, EVERY FRAME the
    /// panel is open. That, not any wire limit, was what
    /// `MAX_INVENTORY_ITEMS` was really bounded by.
    ///
    /// Cached, an idle panel pays nothing and only an edit re-serializes.
    /// The one-frame lag this introduces is the one the footer already
    /// documents and accepts for `stored_baseline`.
    live_baseline: Option<(bevy::ecs::change_detection::Tick, Option<serde_json::Value>)>,
    /// Change tick the size readout was last MEASURED at (#1292) - the
    /// generation latch the room and avatar editors already use for the
    /// same call (#1270 f418).
    ///
    /// A latch and not a one-frame "was edited" flag, because
    /// `refresh_size_readout` may decline: it throttles to
    /// `SIZE_READOUT_REFRESH_SECS`, so an edit landing inside that window
    /// would have its single frame of "changed" swallowed and the readout
    /// would show the pre-edit size until the NEXT edit. Advanced only when
    /// a measurement actually lands, so a declined refresh is retried.
    size_readout_tick: Option<bevy::ecs::change_detection::Tick>,
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
    /// play - and every one of them used to be marked clean without ever
    /// having been written. `stored` is also what the next save's diff is
    /// computed against, so the loss compounds instead of self-healing.
    pub published: InventoryRecord,
    /// How the save is bounded (#1839): the poll declares the task dead
    /// past [`crate::pds::inventory::SaveBound::deadline`], which a save
    /// writing a world grows by the requests it can make.
    pub bound: crate::pds::inventory::SaveBound,
}

/// Origin of a drag-to-place operation. The raycast + placement path
/// is identical for every source; only the generator lookup differs.
/// Inventory drops copy a blueprint into the room's `generators` map
/// under a collision-safe key; catalogue drops resolve the slug against
/// [`crate::catalogue::by_slug`] and stamp a fresh deep-copied
/// blueprint into the room's `generators` map. (A `RoomGenerators`
/// variant for World-Editor-tab drags was documented but never armed
/// by any UI - deleted in #832; the scene context menu's Duplicate
/// (#824) covers stamping another instance of an existing generator.)
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum DropSource {
    #[default]
    Inventory,
    Catalogue,
}

/// Egui → world drag handoff. The UI side sets the generator name and source
/// on drag-start; [`drop::handle_generator_drop`] consumes it on mouse release,
/// runs the raycast, and clears it - whether or not the release landed on a
/// valid ground hit.
///
/// `peer_target` is refreshed every frame by [`crate::ui::people::people_ui`]
/// while a drag is active: it is set to the peer whose row the cursor is
/// currently over (or cleared when the cursor isn't over a peer row). The
/// drop handler consumes it on release to route the drag into an
/// [`crate::protocol::OverlandsMessage::ItemOffer`] instead of a terrain
/// placement. It intentionally is **not** cleared by the inventory or
/// world-editor drag source on its own - the People UI owns the signal
/// because only it can resolve "cursor is over peer row N" via egui's
/// layout.
#[derive(Resource, Default)]
pub struct PendingGeneratorDrop {
    pub generator_name: Option<String>,
    pub source: DropSource,
    /// What the drag would give (#1844), set with `generator_name` by the
    /// row that arms it: a thing, or a whole avatar or world - which the
    /// People row gates on the peer's build, the ground preview never marks
    /// and a release over the world view never places.
    pub kind: crate::protocol::GiftKind,
    pub peer_target: Option<PeerDropTarget>,
}

/// The follow-the-cursor tooltip a live drag carries (#1220 f132).
///
/// Shared by the Inventory row and the Catalogue tree, because the Catalogue
/// is where a new user MEETS drag-to-place and drag-to-gift - and it is also
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
                "Place “{label}” - or drop on a peer in the People list to gift"
            ));
        } else {
            // A visitor cannot place: ground placement is owner-only, so
            // gifting is the whole of what this drag can do for them.
            ui.label(format!(
                "Offer “{label}” - drop on a peer in the People list"
            ));
        }
    });
}

/// The follow-the-cursor tooltip a whole avatar's or world's drag carries
/// (#1844): it places nothing, so the one thing it says is where it goes -
/// in anyone's world, the owner's own included.
pub fn gift_drag_tooltip(ui: &egui::Ui, id_salt: &str, label: &str) {
    egui::Tooltip::always_open(
        ui.ctx().clone(),
        ui.layer_id(),
        egui::Id::new((id_salt, label)),
        egui::PopupAnchor::Pointer,
    )
    .show(|ui| {
        ui.label(format!(
            "Give “{label}” - drop it on a person in the People list"
        ));
    });
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
    /// to the silent cancel written for drops over the Inventory window -
    /// so the most common failure in the app's one gifting gesture was
    /// indistinguishable from the feature being broken.
    pub blocked: Option<&'static str>,
}

/// Auto-open the People window the moment a gift-capable drag arms with
/// peers present (#846). Peer drop targets exist ONLY as rendered People
/// rows - with the window closed (the default) a drag had nothing to
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
/// ceiling - and since #1840 what the Catalogue's avatar entries wear
/// through as well. The avatar record is mutated only on a click (guarded-dirty);
/// the editor state carries the detach queue the publish bundle drains.
#[derive(bevy::ecs::system::SystemParam)]
pub struct WearSurface<'w> {
    pub(crate) live_avatar: Option<ResMut<'w, crate::state::LiveAvatarRecord>>,
    /// What the avatar save last wrote - where a worn avatar item may
    /// reuse a wardrobe key instead of growing the wardrobe (#1838).
    pub(crate) stored_avatar: Option<Res<'w, crate::state::StoredAvatarRecord>>,
    pub(crate) avatar_editor: ResMut<'w, crate::ui::avatar::AvatarEditorState>,
    pub(crate) undo_labels: ResMut<'w, crate::ui::undo::PendingUndoLabels>,
    pub(crate) toasts: ResMut<'w, crate::notify::Toasts>,
}

/// What a stash row reads and writes besides the stash itself (#1301): the
/// drop bus a drag arms, the selection a click makes, and the picture the
/// pane beside the list draws. Bundled because `inventory_ui` sits at
/// Bevy's 16-parameter ceiling and the drop bus used to be a parameter of
/// its own - this bundle took its slot. The two `ResMut`s are written only
/// on a real gesture ([`RowGesture::acts`]); the preview is `Option`
/// because `item_preview`'s `Startup` system inserts it.
#[derive(bevy::ecs::system::SystemParam)]
pub struct RowSurface<'w, 's> {
    pending_drop: ResMut<'w, PendingGeneratorDrop>,
    browser: ResMut<'w, InventoryBrowser>,
    preview: Option<Res<'w, crate::item_preview::ItemPreview>>,
    /// A world being loaded (#1839): one at a time, so a world row's Load
    /// waits while one is in flight.
    world_loads: Query<'w, 's, (), With<LoadWorldTask>>,
}

/// A Wear / Take off click on an inventory row (#1096), applied after the
/// list has released its borrow of the stash.
enum WearAction {
    Wear(String),
    TakeOff(String),
    /// Put on the named avatar item, replacing the live avatar (#1838).
    WearAvatar(String),
}

/// The Wear / Take off buttons for a wearable row. Which one shows is
/// the item's worn state on the live avatar; reasons a body cannot be
/// dressed render as a disabled button with the reason on hover, the same
/// vocabulary the catalogue uses.
fn wear_buttons(
    ui: &mut egui::Ui,
    name: &str,
    socket: &str,
    fit_beast_mm: u32,
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
    let avatar = live_avatar.map(|live| &live.0);
    if let Some(reason) = crate::ui::avatar::wear_blocked_reason(avatar, true)
        .or_else(|| crate::ui::avatar::wear_refused_reason(avatar, socket, fit_beast_mm))
    {
        ui.add_enabled(false, egui::Button::new("Wear").small())
            .on_disabled_hover_text(reason);
        return;
    }
    if ui
        .small_button("Wear")
        .on_hover_text(format!(
            "Wear this on your {}",
            crate::pds::avatar::socket_label(socket).to_lowercase()
        ))
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
    // A `Mut`, not a `&mut` (critic of #1838): a refusal reads it through
    // `Deref`, so only a write that happens stamps the change tick.
    live_avatar: Option<Mut<'_, crate::state::LiveAvatarRecord>>,
    stored_avatar: Option<&crate::pds::AvatarRecord>,
    avatar_editor: &mut crate::ui::avatar::AvatarEditorState,
    did: &str,
    undo_labels: &mut crate::ui::undo::PendingUndoLabels,
    toasts: &mut crate::notify::Toasts,
    now: f64,
) {
    let Some(mut live) = live_avatar else {
        toasts.warn(String::from("No avatar loaded yet."), now);
        return;
    };
    match action {
        WearAction::Wear(name) => {
            // Says why rather than returning silently (#1141). The row's
            // button is disabled for every one of these reasons, so this
            // arm should be unreachable - but "should be unreachable" is
            // exactly the assumption that let the catalogue toast a
            // success over a wear that never happened.
            if let Some(reason) = crate::ui::avatar::wear_blocked_reason(Some(&live.0), true) {
                toasts.warn(format!("Could not wear \"{name}\" - {reason}"), now);
                return;
            }
            let Some(record) = crate::ui::avatar::record_for_inventory_item(inventory, &name)
            else {
                toasts.warn(format!("\"{name}\" is not wearable."), now);
                return;
            };
            if let Some(reason) = crate::ui::avatar::wear_refused_reason(
                Some(&live.0),
                &record.socket,
                record.fit_beast_mm,
            ) {
                toasts.warn(format!("Could not wear \"{name}\" - {reason}"), now);
                return;
            }
            // No `else` arm: the check above already established a rigged,
            // resolved body under the cap, which is exactly the condition
            // `attach_record` returns `Some` on.
            if let Some(rig) = live.0.body.rigged_mut()
                && crate::ui::avatar::attach_record(rig, record, did).is_some()
            {
                undo_labels.set_avatar(format!("wear {name}"));
            }
        }
        WearAction::WearAvatar(name) => {
            let Some(item) = inventory.avatars.get(&name) else {
                toasts.warn(format!("\"{name}\" is not in your inventory."), now);
                return;
            };
            // One wear path for every surface (#1840): the refusals, the
            // already-worn check and the one-write undo step live there.
            // A refusal has already said why in a toast.
            let _ = crate::ui::avatar::wear_avatar_on_live(
                item,
                &name,
                live,
                stored_avatar,
                avatar_editor,
                did,
                undo_labels,
                toasts,
                now,
            );
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
    mut rows: RowSurface,
    mut state: Local<InventoryEditorState>,
    time: Res<Time>,
    mut publish_shortcut: ResMut<crate::ui::shortcuts::PublishShortcut>,
    recovery: Option<Res<crate::state::InventoryRecordRecovery>>,
    mut wear: WearSurface,
) {
    let WearSurface {
        live_avatar,
        stored_avatar,
        avatar_editor,
        undo_labels,
        toasts,
    } = &mut wear;
    let RowSurface {
        pending_drop,
        browser,
        preview,
        world_loads,
    } = &mut rows;
    let world_loading = !world_loads.is_empty();
    let (Some(live), Some(stored), Some(session), Some(refresh_ctx)) =
        (live.as_mut(), stored, session, refresh_ctx)
    else {
        return;
    };
    // A selection whose name no longer resolves - a delete, a Load or
    // Reset of the whole stash, a reload - is dropped rather than left
    // naming nothing (#1301). Asked through `Deref`; the resource is
    // written only when the answer is yes.
    if browser.is_stale(&live.0) {
        browser.clear();
    }
    // Rows are draggable everywhere: releasing over a peer row in the People
    // window offers the item as a gift, which is a personal transaction and
    // valid in ANY room (#699 - gating the drag on room ownership locked
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

    // Rename dialog - the shared modal (#838): keeps itself open on an
    // empty/taken name with the reason inline, Enter applies, Esc cancels.
    if let Some((old_name, mut new_name)) = state.renaming_generator.clone() {
        match crate::ui::confirm::rename_dialog(
            ctx,
            "Rename Inventory Item",
            &old_name,
            &mut new_name,
            // Taken across every kind of item: one namespace (#1838).
            |draft| live.0.has_name(draft),
        ) {
            crate::ui::confirm::RenameOutcome::Open => {
                state.renaming_generator = Some((old_name, new_name));
            }
            crate::ui::confirm::RenameOutcome::Cancelled => {
                state.renaming_generator = None;
            }
            crate::ui::confirm::RenameOutcome::Renamed(applied) => {
                // The selection follows the name (#1301): the name is the
                // item's key, so without this a rename would drop the
                // selection the rename was made from. Asked before the
                // rename, which is what can refuse (a taken name).
                let carry = browser.is_selected(&old_name);
                // Through the record's own rename so wear metadata (#1096)
                // travels with the item - and then the worn props' own
                // provenance, which the stash cannot reach (#1141). The
                // name is the only link between a row and the prop it put
                // on the body; moving one without the other left the row
                // offering Wear on something already worn.
                if live.0.rename_item(&old_name, applied.clone())
                    // Provenance names generator items only (#1838).
                    && live.0.generators.contains_key(&applied)
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
                if carry && !live.0.has_name(&old_name) && live.0.has_name(&applied) {
                    browser.follow_rename(&old_name, &applied);
                }
                state.renaming_generator = None;
            }
        }
    }

    let (pos, size) = chrome.place(crate::ui::layout::UiWindow::Inventory, ctx);
    // Guarded-dirty (#879): `.open(&mut panels.inventory)` through the
    // `ResMut` would mark UiPanels changed every frame, starving the
    // prefs save debounce - local copy in, write back only on close.
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
                            "⚠ Your inventory could not be loaded - this shows an empty default.",
                        );
                        ui.label(egui::RichText::new(format!("Reason: {}", rec.reason)).small());
                        ui.label(
                            egui::RichText::new(
                                "Saving would overwrite the stored copy (you'll be asked \
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
            // to be silently truncated by sanitize on the next login -
            // now it loads intact, reads red here, and blocks publishing
            // until the user decides what to prune.
            let cap = crate::config::state::MAX_INVENTORY_ITEMS;
            // Every kind of item counts against the cap (#1838).
            let count = live.0.item_count();
            let over_cap = count > cap;
            if over_cap {
                ui.colored_label(
                    crate::ui::theme::current(ui.ctx()).status.error,
                    format!(
                        "Saved items: {count}/{cap} - over the {cap}-item cap; \
                         remove {} to enable saving",
                        if count - cap == 1 {
                            "1 item".to_owned()
                        } else {
                            format!("{} items", count - cap)
                        }
                    ),
                );
            } else {
                ui.label(format!("Saved items: {count}/{cap}"));
            }
            // Search over the stash (#1275 f134). Mirrors the Catalogue's
            // top bar - same hint-text shape, same CROSS clear button -
            // because these are the two windows a decorating session moves
            // between and the Inventory is the one you cannot re-derive by
            // browsing. Drawn only when there is something to search: a
            // filter over an empty stash is noise, and the empty state
            // below already says what to do about that.
            if count > 0 {
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    crate::ui::affordances::text_edit(
                        ui,
                        egui::TextEdit::singleline(&mut state.search)
                            .hint_text("name / kind / socket")
                            .desired_width(120.0),
                    );
                    if !state.search.is_empty()
                        && ui.small_button(crate::ui::affordances::CROSS).clicked()
                    {
                        state.search.clear();
                    }
                });
            }
            ui.separator();

            // Footer FIRST, bottom-up, so its height is measured and the
            // list gets exactly what is left. This used to reserve a flat
            // 80 pt for "the separator + Publish row + feedback line" and
            // hand the scroll area `available_height() - 80` with
            // `auto_shrink([true, false])`, which claims that height
            // whatever its content - so a `publish_status_line` carrying a
            // long XRPC failure, wrapped to four lines in this 300 pt-wide
            // window, made the content taller than the window and egui's
            // `Resize` ratcheted it up every frame until it filled the
            // screen. Chat hit exactly that (#1280); this one had ~26 pt of
            // headroom left and was one wrapped error away.
            //
            // The footer now renders before the list, so a delete made this
            // frame reaches the Save row's dirty check on the next one - a
            // single frame of lag on an indicator, against a window that
            // could climb off the screen.
            crate::ui::layout::footer(ui, "inventory_footer", |ui| {
                // Shared Save / Load / Reset row + status line
                // (`ui::editable`), identical to the World and Avatar
                // editors. Dirty is derived (a serialized diff against the stored
                // snapshot) so the row needs no per-edit flag; Inventory now
                // also gets Load-from-PDS (revert) and Reset-to-default
                // (empty the stash) - it previously had Publish only.
                //
                // Both baselines are cached (#1135, the #674 pattern): the stored
                // side re-serializes only when the resource changes and the empty
                // default only once, so an open panel serializes the LIVE stash
                // ONCE per frame instead of three whole trees. The comparisons are
                // value-identical to `records_differ` - `Option<Value>` on both
                // sides, `.ok()` semantics preserved - so dirty and can_reset are
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
                // The live side is cached on its own change tick too
                // (#1292) - see `live_baseline`. `edited` is the fact the
                // size readout below needs as well, so it is derived once.
                let edited = state
                    .live_baseline
                    .as_ref()
                    .is_none_or(|(tick, _)| *tick != live.last_changed());
                if edited {
                    state.live_baseline =
                        Some((live.last_changed(), serde_json::to_value(&live.0).ok()));
                }
                let live_value = state
                    .live_baseline
                    .as_ref()
                    .and_then(|(_, value)| value.clone());
                let dirty = match state.stored_baseline.as_ref() {
                    Some((_, baseline)) => *baseline != live_value,
                    None => true,
                };
                let can_reset = state
                    .default_baseline
                    .as_ref()
                    .is_none_or(|baseline| *baseline != live_value);
                // Publishing is blocked while over the cap (#841) - the red
                // header line explains; mirrors the hard-ceiling size block.
                let within_cap = live.0.item_count() <= crate::config::state::MAX_INVENTORY_ITEMS;
                // `session` + `refresh_ctx` are guaranteed present (the early
                // return above bails otherwise), so a publish is always
                // attemptable while dirty.
                //
                // Size readout: the stash is one record PER ITEM (#696), so
                // the per-record budget applies to the largest single item -
                // not the whole stash. Same throttled cache as the other
                // editors, custom measurement.
                let now = time.elapsed_secs_f64();
                if crate::ui::editable::refresh_size_readout(
                    &mut *feedback,
                    &live.0,
                    now,
                    // Was unconditional, on the reasoning that "the stash is
                    // capped small (#841) and has no live-value cache to
                    // answer the question from". Both halves stopped being
                    // true in #1292: the cap is 500, and the live side has a
                    // tick-keyed cache now. `measure_publish` serializes
                    // EVERY item to find the largest - 18.2 ms at 500 - and
                    // the 0.5 s throttle alone meant paying that twice a
                    // second forever on a stash nobody was touching.
                    state.size_readout_tick != Some(live.last_changed()),
                    crate::pds::inventory::measure_publish,
                ) {
                    state.size_readout_tick = Some(live.last_changed());
                }
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
                        // Inventory has no undo stack (#866) - keep the modal.
                        confirm: Some(&mut state.row_confirm),
                        reset: crate::ui::editable::ResetWording::EmptyStash {
                            items: live.0.item_count(),
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
                        // wipe whatever is actually stored - ask first.
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

                publish_status_line_within(
                    ui,
                    &feedback.status,
                    now,
                    dirty,
                    feedback.gives_up_after_secs,
                );
            });

            crate::ui::layout::fill_above(ui, |ui| {
                let now = time.elapsed_secs_f64();
                let q = state.search.trim().to_lowercase();
                // Every kind of item, in one list (#1838). An avatar row's
                // facet is its tag - "avatar" and its kind - as drawn.
                let mut names: Vec<String> = live
                    .0
                    .names()
                    .filter(|name| match live.0.avatars.get(*name) {
                        Some(item) => row_matches(name, &avatar_tag(item), None, &q),
                        None if live.0.worlds.contains_key(*name) => {
                            row_matches(name, &world_tag(&live.0.worlds[*name]), None, &q)
                        }
                        None => row_matches(
                            name,
                            live.0
                                .generators
                                .get(*name)
                                .map(|g| g.kind_tag())
                                .unwrap_or("?"),
                            live.0.wear.get(*name),
                            &q,
                        ),
                    })
                    .cloned()
                    .collect();
                // Case-insensitive (#841): plain `sort()` put "Zebra"
                // before "apple".
                names.sort_by_key(|name| name.to_lowercase());

                // An empty stash is never a bare rectangle (#1275 f134), and
                // has no pane: there is nothing to select.
                if count == 0 {
                    ui.add_space(4.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(empty_state(true))
                                .color(crate::ui::theme::current(ui.ctx()).text_weak),
                        )
                        .wrap(),
                    );
                    return;
                }

                // The pane's item is read before the split: the list below
                // writes the selection on a click, and both halves are
                // closures alive at once. A click lands in the pane on the
                // next frame, which is also when its row expands.
                let pane_name = browser.selected_name().map(str::to_owned);
                let (to_remove, wear_action, load_request) = crate::ui::layout::beside_pane(
                    ui,
                    crate::ui::item_picture::INVENTORY_SIDE,
                    |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("inventory_list_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let mut to_remove: Option<String> = None;
                                let mut wear_action: Option<WearAction> = None;
                                let mut load_request: Option<String> = None;
                                // "Nothing matched" is a different fact from
                                // "nothing here" (#1275 f134), and the count
                                // line above reports the whole stash, so the
                                // filter says what it is hiding - otherwise a
                                // forgotten query reads as a lost item.
                                if names.is_empty() {
                                    ui.add_space(4.0);
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(empty_state(false)).color(
                                                crate::ui::theme::current(ui.ctx()).text_weak,
                                            ),
                                        )
                                        .wrap(),
                                    );
                                } else if names.len() != count {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "Showing {} of {count}",
                                            names.len()
                                        ))
                                        .small()
                                        .color(crate::ui::theme::current(ui.ctx()).text_weak),
                                    );
                                }
                                for name in &names {
                                    // A whole world (#1839): click to select;
                                    // Load, Rename and delete in the
                                    // expansion. A saved one drags to give
                                    // (#1844); an unsaved one never drags,
                                    // and its pane says why.
                                    if let Some(item) = live.0.worlds.get(name) {
                                        let selected = browser.is_selected(name);
                                        let tag = format!("({})", world_tag(item));
                                        let giftable =
                                            crate::pds::inventory::world::gift_blocked_reason(
                                                item.head(),
                                                Some(&stored.0.worlds),
                                            )
                                            .is_none();
                                        let kind = RowKind::World { giftable };
                                        let response = stash_row(ui, name, &tag, kind, selected);
                                        let gesture = RowGesture::of(&response);
                                        if gesture.acts() {
                                            apply_row_gesture(
                                                gesture,
                                                name,
                                                kind,
                                                browser,
                                                pending_drop,
                                                now,
                                            );
                                        }
                                        if gesture.dragged
                                            && pending_drop.generator_name.as_deref()
                                                == Some(name.as_str())
                                        {
                                            gift_drag_tooltip(ui, "inv_drag_tip", name);
                                        }
                                        if selected {
                                            let blocked = world::world_load_blocked_reason(
                                                owns_room,
                                                world_loading,
                                            );
                                            let action = ui
                                                .indent(
                                                    ("inventory_row_controls", name.as_str()),
                                                    |ui| world_row_controls(ui, blocked),
                                                )
                                                .inner;
                                            match action {
                                                Some(WorldRowAction::Load) => {
                                                    load_request = Some(name.clone());
                                                }
                                                Some(WorldRowAction::Rename) => {
                                                    state.renaming_generator =
                                                        Some((name.clone(), name.clone()));
                                                }
                                                Some(WorldRowAction::Delete) => {
                                                    to_remove = Some(name.clone());
                                                }
                                                None => {}
                                            }
                                        }
                                        continue;
                                    }
                                    if let Some(item) = live.0.avatars.get(name) {
                                        if !item.is_readable() {
                                            if unreadable_row(ui, name) {
                                                to_remove = Some(name.clone());
                                            }
                                            continue;
                                        }
                                        let selected = browser.is_selected(name);
                                        let tag = format!("({})", avatar_tag(item));
                                        let response =
                                            stash_row(ui, name, &tag, RowKind::Avatar, selected);
                                        let gesture = RowGesture::of(&response);
                                        if gesture.acts() {
                                            apply_row_gesture(
                                                gesture,
                                                name,
                                                RowKind::Avatar,
                                                browser,
                                                pending_drop,
                                                now,
                                            );
                                        }
                                        // A gift-only drag (#1844): it is
                                        // given, never placed.
                                        if gesture.dragged
                                            && pending_drop.generator_name.as_deref()
                                                == Some(name.as_str())
                                        {
                                            gift_drag_tooltip(ui, "inv_drag_tip", name);
                                        }
                                        if selected {
                                            let blocked =
                                                crate::ui::avatar::wear_avatar_blocked_reason(
                                                    live_avatar.as_deref().map(|live| &live.0),
                                                    item,
                                                );
                                            let action = ui
                                                .indent(
                                                    ("inventory_row_controls", name.as_str()),
                                                    |ui| avatar_row_controls(ui, blocked),
                                                )
                                                .inner;
                                            match action {
                                                Some(AvatarRowAction::Wear) => {
                                                    wear_action =
                                                        Some(WearAction::WearAvatar(name.clone()));
                                                }
                                                Some(AvatarRowAction::Rename) => {
                                                    state.renaming_generator =
                                                        Some((name.clone(), name.clone()));
                                                }
                                                Some(AvatarRowAction::Delete) => {
                                                    to_remove = Some(name.clone());
                                                }
                                                None => {}
                                            }
                                        }
                                        continue;
                                    }
                                    let Some(generator) = live.0.generators.get(name) else {
                                        continue;
                                    };
                                    // What KIND of blueprint each row is
                                    // (#841); wearables say so, and where
                                    // (#1096). These are the search facets,
                                    // so they stay on the row.
                                    let kind_tag = generator.kind_tag();
                                    let tag = match live.0.wear.get(name) {
                                        Some(meta) => format!(
                                            "{} · wearable, {}",
                                            crate::pds::GeneratorKind::display_name(kind_tag),
                                            crate::pds::avatar::socket_label(&meta.socket)
                                                .to_lowercase()
                                        ),
                                        None => kind_tag.to_string(),
                                    };
                                    // An item this build cannot decode
                                    // (#1207): a gift from a newer client, or
                                    // a stash saved by one. It cannot be
                                    // placed, worn, renamed or written back,
                                    // and it is what disables Save for the
                                    // whole stash - so it is not selectable,
                                    // and its delete stays on the row: it is
                                    // the one thing the owner can do about it.
                                    if matches!(generator.kind, GeneratorKind::Unknown) {
                                        if unreadable_row(ui, name) {
                                            to_remove = Some(name.clone());
                                        }
                                        continue;
                                    }
                                    // Terrain and water are room-scoped, not
                                    // point-placed: selectable, so their
                                    // Rename and delete are reachable, and no
                                    // drag sense, so a release is never armed
                                    // only to be refused (#832).
                                    let (kind, tag) =
                                        if crate::pds::inventory::is_drop_placeable(generator) {
                                            (RowKind::Placeable, format!("({tag})"))
                                        } else {
                                            (RowKind::RoomScoped, format!("({tag} - room-scoped)"))
                                        };
                                    let selected = browser.is_selected(name);
                                    let response = stash_row(ui, name, &tag, kind, selected);
                                    let gesture = RowGesture::of(&response);
                                    if gesture.acts() {
                                        apply_row_gesture(
                                            gesture,
                                            name,
                                            kind,
                                            browser,
                                            pending_drop,
                                            now,
                                        );
                                    }
                                    if gesture.dragged
                                        && pending_drop.generator_name.as_deref()
                                            == Some(name.as_str())
                                    {
                                        // Follow-the-cursor tooltip keeps the
                                        // dragger oriented while they hunt for
                                        // a target - without it, the drag is
                                        // invisible once the pointer leaves
                                        // the row. Shared with the Catalogue
                                        // since #1220 f132.
                                        drag_tooltip(ui, "inv_drag_tip", name, owns_room);
                                    }
                                    // The expansion (#1301): the selected row
                                    // carries the controls every row used to
                                    // cram inline, so the unselected rows fall
                                    // quiet.
                                    if selected {
                                        ui.indent(
                                            ("inventory_row_controls", name.as_str()),
                                            |ui| {
                                                ui.horizontal(|ui| {
                                                    // Wear / Take off (#1096) for
                                                    // items that carry wear
                                                    // metadata.
                                                    if let Some(meta) = live.0.wear.get(name) {
                                                        wear_buttons(
                                                            ui,
                                                            name,
                                                            meta.socket.as_str(),
                                                            meta.fit_beast_mm,
                                                            &mut wear_action,
                                                            live_avatar.as_deref(),
                                                        );
                                                    }
                                                    if ui
                                                        .small_button("Rename")
                                                        .on_hover_text("Rename this item")
                                                        .clicked()
                                                    {
                                                        state.renaming_generator =
                                                            Some((name.clone(), name.clone()));
                                                    }
                                                    ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        if crate::ui::affordances::remove_button(
                                                            ui,
                                                            "Delete this item from your inventory",
                                                        )
                                                        .clicked()
                                                        {
                                                            to_remove = Some(name.clone());
                                                        }
                                                    },
                                                );
                                                });
                                            },
                                        );
                                    }
                                }
                                (to_remove, wear_action, load_request)
                            })
                            .inner
                    },
                    |ui| {
                        // ScrollArea: a persisted rect narrower or shorter
                        // than the slot must scroll the pane, not clip it
                        // (#1235).
                        egui::ScrollArea::vertical()
                            .id_salt("inventory_pane_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let item = pane_name.as_deref().and_then(|name| {
                                    if let Some(world) = live.0.worlds.get(name) {
                                        return Some(PaneItem {
                                            name,
                                            what: PaneWhat::World {
                                                item: world,
                                                gift_blocked:
                                                    crate::pds::inventory::world::gift_blocked_reason(
                                                        world.head(),
                                                        Some(&stored.0.worlds),
                                                    ),
                                            },
                                        });
                                    }
                                    if let Some(avatar) = live.0.avatars.get(name) {
                                        return Some(PaneItem {
                                            name,
                                            what: PaneWhat::Avatar(avatar),
                                        });
                                    }
                                    let generator = live.0.generators.get(name)?;
                                    let worn = live_avatar
                                        .as_deref()
                                        .and_then(|avatar| avatar.0.body.rigged_ref())
                                        .is_some_and(|rig| {
                                            crate::ui::avatar::is_worn_from(rig, name)
                                        });
                                    Some(PaneItem {
                                        name,
                                        what: PaneWhat::Generator {
                                            generator,
                                            wear: live.0.wear.get(name),
                                            worn,
                                        },
                                    })
                                });
                                draw_pane(ui, item, preview.as_deref());
                            });
                    },
                );
                // Applied after the split so the borrow of `live` the rows
                // and the pane hold is released before the avatar record is
                // dressed.
                if let Some(action) = wear_action {
                    apply_wear_action(
                        action,
                        &live.0,
                        live_avatar.as_mut().map(|avatar| avatar.reborrow()),
                        stored_avatar.as_deref().map(|stored| &stored.0),
                        avatar_editor.as_mut(),
                        &session.did,
                        undo_labels.as_mut(),
                        toasts.as_mut(),
                        now,
                    );
                }
                // Load into my world asks first (#1839): it replaces
                // everything in the world. The reasons it cannot are asked
                // again on the confirm, which a travel or a second load may
                // have overtaken.
                if let Some(name) = load_request {
                    let (title, body) = world::load_confirm_text(&name);
                    state.load_confirm.request(title, body, "Load", name);
                }
                if let Some(name) = state.load_confirm.show(ui.ctx(), "inventory-load-world") {
                    match (
                        live.0.worlds.get(&name),
                        world::world_load_blocked_reason(owns_room, world_loading),
                    ) {
                        (Some(item), None) => {
                            world::spawn_world_load_task(
                                &mut commands,
                                &session.did,
                                &name,
                                item,
                                now,
                            );
                            toasts.info(format!("Loading \"{name}\" into your world\u{2026}"), now);
                        }
                        (Some(_), Some(reason)) => {
                            toasts.warn(format!("Could not load \"{name}\" - {reason}"), now);
                        }
                        (None, _) => {
                            toasts.warn(format!("\"{name}\" is not in your inventory."), now);
                        }
                    }
                }
                // Delete asks first (#1200): this is the one surface with
                // no undo. A worn item is named as such - deleting takes it
                // off too, so the prop cannot linger on the body with the
                // only row that offered "Take off" gone (finding 131).
                if let Some(name) = to_remove {
                    // Provenance names generator items only (#1838).
                    let worn = live.0.generators.contains_key(&name)
                        && live_avatar
                            .as_deref()
                            .and_then(|avatar| avatar.0.body.rigged_ref())
                            .is_some_and(|rig| crate::ui::avatar::is_worn_from(rig, &name));
                    let body = if worn {
                        format!(
                            "You are wearing \"{name}\" - deleting it also takes it off. \
                             The inventory has no undo; the stored copy keeps it until \
                             you save."
                        )
                    } else {
                        String::from(
                            "The inventory has no undo; the stored copy keeps it until \
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
                    let worn = live.0.generators.contains_key(&name)
                        && live_avatar
                            .as_deref()
                            .and_then(|avatar| avatar.0.body.rigged_ref())
                            .is_some_and(|rig| crate::ui::avatar::is_worn_from(rig, &name));
                    if worn {
                        apply_wear_action(
                            WearAction::TakeOff(name.clone()),
                            &live.0,
                            live_avatar.as_mut().map(|avatar| avatar.reborrow()),
                            stored_avatar.as_deref().map(|stored| &stored.0),
                            avatar_editor.as_mut(),
                            &session.did,
                            undo_labels.as_mut(),
                            toasts.as_mut(),
                            now,
                        );
                    }
                    if live.0.remove_item(&name) {
                        toasts.info(
                            if worn {
                                format!("Deleted \"{name}\" and took it off.")
                            } else {
                                format!("Deleted \"{name}\".")
                            },
                            now,
                        );
                    }
                }
            });
            reload_stash
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
/// path ([`crate::ui::people`]) drive the same pipeline - the shared
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
    // A save that writes a world is many requests, each bounded inside the
    // publish (#1839); any other is one bound over the whole, as before.
    let bound = crate::pds::inventory::SaveBound::of(&record, &stored);
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
        match bound {
            crate::pds::inventory::SaveBound::Whole => {
                crate::config::http::run_or(
                    fut,
                    Err(crate::config::http::timed_out("Saving your inventory")),
                )
                .await
            }
            crate::pds::inventory::SaveBound::PerRequest(_) => fut.await,
        }
    });
    commands.spawn(PublishInventoryTask {
        task,
        did,
        spawned_at: now,
        record_bytes,
        published,
        bound,
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
    mut toasts: ResMut<crate::notify::Toasts>,
    // A result for another identity must not pin `stored` (#1204).
    session: Option<Res<AtprotoSession>>,
    // A saved world's contents are let go (#1839).
    mut live: Option<ResMut<LiveInventoryRecord>>,
) {
    // The status line names the deadline the signed-in account's save in
    // flight is given up at (#1839 end review): ONE value a frame, from that
    // account's task alone - a task left over from another identity must not
    // take turns with it - written only when it moves, so the feedback's
    // change tick does not move every frame a save is in flight.
    let current = session.as_deref().map(|s| s.did.as_str());
    let gives_up = tasks
        .iter()
        .find(|(_, task)| Some(task.did.as_str()) == current)
        .map(|(_, task)| task.bound.deadline().as_secs_f64());
    if gives_up.is_some() && feedback.gives_up_after_secs != gives_up {
        feedback.gives_up_after_secs = gives_up;
    }
    for (entity, mut task) in tasks.iter_mut() {
        let spawned_at = task.spawned_at;
        let deadline = task.bound.deadline();
        let Some(result) = crate::ui::editable::poll_or_expire_within(
            &mut task.task,
            spawned_at,
            time.elapsed_secs_f64(),
            "Saving your inventory",
            deadline,
        ) else {
            continue;
        };
        commands.entity(entity).despawn();
        if crate::ui::room::stale_result(
            "Saving your inventory",
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
                // The published snapshot, not `live` (#1116) - see
                // `PublishInventoryTask::published`.
                if let Some(stored) = stored.as_mut() {
                    stored.0 = task.published.clone();
                    // A saved world's parts are on the data server now, and
                    // a load fetches them again (#1839): neither copy keeps
                    // megabytes it no longer needs - wasm's heap never
                    // shrinks. Live's copy goes through
                    // `bypass_change_detection`, because nothing it compares
                    // or serialises moves.
                    stored.0.forget_saved_world_contents(&task.published);
                }
                if let Some(live) = live.as_mut()
                    && live.0.holds_world_contents()
                {
                    live.bypass_change_detection()
                        .0
                        .forget_saved_world_contents(&task.published);
                }
                // The stored copy is now exactly what was written, so the
                // recovery marker retires where success is known (#1199).
                commands.remove_resource::<crate::state::InventoryRecordRecovery>();
                feedback.status = PublishStatus::Success { at_secs: now };
                crate::ui::editable::report_publish_success(
                    RecordKind::Inventory,
                    &panels,
                    &mut toasts,
                    now,
                );
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
            // failure used to be invisible - the item looked saved and
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

/// The row for an item this build cannot read (#1207), of either kind
/// (#1838): not selectable - it cannot be placed, worn, renamed or written
/// back - with its delete on the row, the one thing the owner can do about
/// it. Whether the delete was clicked.
fn unreadable_row(ui: &mut egui::Ui, name: &str) -> bool {
    let mut delete = false;
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::ui::affordances::remove_button(ui, "Delete this item from your inventory")
                .clicked()
            {
                delete = true;
            }
            let warn = crate::ui::theme::current(ui.ctx()).status.warn;
            ui.add_enabled_ui(false, |ui| {
                ui.add(
                    egui::Button::selectable(false, name)
                        .right_text(egui::RichText::new(UNREADABLE_ITEM_TAG).small().color(warn))
                        .truncate()
                        .min_size(egui::vec2(ui.available_width(), 0.0)),
                );
            })
            .response
            .on_disabled_hover_text(UNREADABLE_ITEM_HOVER);
        });
    });
    delete
}

/// The row tag for an item this build cannot decode (#1207). It used to
/// share the terrain/water "room-scoped" tag, which sent the owner looking
/// for a placement rule when the item was simply unreadable.
pub const UNREADABLE_ITEM_TAG: &str = "(from a newer version of Overlands)";

/// What the owner can do about an unreadable item.
pub const UNREADABLE_ITEM_HOVER: &str = "This item was made by a newer version of Overlands. \
     This build cannot read it, so it cannot be placed, worn, renamed or saved - and while it \
     is in your inventory, the inventory cannot be saved at all. Update Overlands, or delete the item.";

/// Pick an inventory key for a gift arriving via [`crate::protocol::OverlandsMessage::ItemOffer`].
/// Policy: if the incoming name, as the sanitiser keeps it, is free, use
/// it; otherwise append `_2`, `_3`, … until we find an unused slot. This matches the
/// user-approved design ("auto-rename with _2 suffix"). Equality of
/// existing entries is not consulted - a gift always lands as a new item,
/// because two players may each have tweaked the same base blueprint and
/// silently coalescing would lose data.
///
/// Free across every kind of item (#1838): an avatar item's name is as
/// taken as a generator item's.
pub fn choose_inventory_gift_key(existing: &InventoryRecord, incoming_name: &str) -> String {
    choose_gift_key(|name| existing.has_name(name), incoming_name)
}

/// [`choose_inventory_gift_key`] over any taken-set. The accept path asks
/// with BOTH the live and the stored stash (#1200): a key free in `live`
/// only because the owner deleted that item locally would, published
/// from `stored`, overwrite the stored item with the gift.
fn choose_gift_key(is_taken: impl Fn(&str) -> bool, incoming_name: &str) -> String {
    use crate::pds::sanitize::names::with_suffix;
    let cap = crate::config::state::MAX_INVENTORY_NAME_CHARS;
    // The name as the inventory's sanitiser keeps it (#1717): an offer's
    // name is cut at 64 characters, which can leave a trailing space, and
    // a raw name free here can be cleaned onto a taken one after this
    // check, and the gift dropped.
    let name = with_suffix(incoming_name, "", cap);
    if !is_taken(&name) {
        return name;
    }
    for i in 2u32..u32::MAX {
        // Cut to leave the suffix room under the cap, or the inventory's
        // sanitiser cuts it back onto the name it was made unique from,
        // and drops the gift (#1717).
        let candidate = with_suffix(&name, &format!("_{i}"), cap);
        if !is_taken(&candidate) {
            return candidate;
        }
    }
    name
}

/// What an accepted gift puts in the stash (#1844): a thing with its wear
/// metadata, a whole avatar, or a whole world - the recipient's OWN snapshot
/// of the world it fetched and sanitised, never the giver's text.
#[derive(Clone, Debug)]
pub enum GiftLanding {
    /// A generator, and its wear metadata when it is a wearable (#1108).
    Generator {
        generator: Box<Generator>,
        wear: Option<crate::pds::inventory::WearMeta>,
    },
    /// A whole avatar.
    Avatar(Box<crate::pds::inventory::AvatarItem>),
    /// A whole world, its parts held until the stash is saved.
    World(crate::pds::inventory::WorldItem),
}

impl GiftLanding {
    /// Put the item in `record` under `key` - a key free across every kind,
    /// so the put cannot be refused.
    fn put_into(self, record: &mut InventoryRecord, key: String) {
        match self {
            Self::Generator { generator, wear } => {
                record.put_item(key, *generator, wear);
            }
            Self::Avatar(item) => {
                record.put_avatar(key, *item);
            }
            Self::World(item) => {
                record.put_world(key, item);
            }
        }
    }
}

/// Accept a gift (#1200): land it in `live` and return the record the
/// auto-publish writes, which is `stored` plus the gift - never `live`.
///
/// The publish on accept exists so the gift survives a session that ends
/// before the owner presses Save. Publishing the whole live stash for it
/// committed every OTHER unsaved edit at a moment a remote peer chose:
/// `plan_item_writes` turns each name present in `stored` and missing
/// from `live` into a delete, so a mis-click delete the owner meant to
/// "Revert to saved" became permanent on Accept, and the revert itself
/// stopped working because the poll pinned `stored` to the published
/// snapshot. The gift's key is free in both stashes for the same reason.
///
/// One rule for every kind (#1844): a thing and an avatar land here at the
/// click, a world when its parts have been fetched ([`gift_world`]).
pub fn accept_gift(
    live: &mut InventoryRecord,
    stored: &InventoryRecord,
    incoming_name: &str,
    item: GiftLanding,
) -> (String, InventoryRecord) {
    let key = choose_gift_key(
        |name| live.has_name(name) || stored.has_name(name),
        incoming_name,
    );
    item.clone().put_into(live, key.clone());
    let mut payload = stored.clone();
    item.put_into(&mut payload, key.clone());
    (key, payload)
}

/// A gift that has just landed (#1844), for [`after_gift_landed`].
pub(crate) struct Landed<'a> {
    /// The name it landed under.
    pub key: &'a str,
    /// The name it was offered under.
    pub offered: &'a str,
    /// What a save writes: the stored stash plus the gift ([`accept_gift`]).
    pub payload: InventoryRecord,
    /// The stored stash the save's diff is computed against.
    pub stored: InventoryRecord,
    /// Whether to save at once, as Accept does.
    pub publish: bool,
}

/// What [`after_gift_landed`] writes to.
pub(crate) struct LandingSinks<'a, 'w, 's> {
    pub commands: &'a mut Commands<'w, 's>,
    pub session: Option<&'a AtprotoSession>,
    pub refresh: Option<&'a crate::oauth::OauthRefreshCtx>,
    /// Whether the stash never loaded (#1199) - then nothing is saved.
    pub recovery: bool,
    /// A `Mut`, so its tick moves only when a save starts.
    pub feedback: Mut<'a, PublishFeedback<InventoryRecord>>,
    pub toasts: &'a mut crate::notify::Toasts,
}

/// Say where a gift landed and save it, by the rules every kind shares
/// (#1200, #1199, #1844): the toast names the key, and says the rename when
/// the offered name was taken; a stash that never loaded is not saved over -
/// the gift stays local and dirty, and a toast says to save it deliberately;
/// otherwise, when `publish` says to and a session can, the save of stored
/// plus the gift starts at once. Whether it did.
pub(crate) fn after_gift_landed(landed: Landed<'_>, sinks: &mut LandingSinks, now: f64) -> bool {
    let Landed {
        key,
        offered,
        payload,
        stored,
        publish,
    } = landed;
    sinks.toasts.success(
        if key == offered {
            format!("\"{key}\" is in your inventory.")
        } else {
            // Say the rename rather than hide it: the recipient is the one
            // person who cannot find the item afterwards if the name they
            // were shown is not the name it has (#1220 f119).
            format!(
                "\"{offered}\" is in your inventory as \"{key}\" - you already had one by that \
                 name."
            )
        },
        now,
    );
    // Auto-publish the updated inventory immediately, so an accepted item is
    // persistent even if the session ends before Save. The poll lands the
    // write, so only the I/O starts here. Unless the stash never loaded
    // (#1199): then `stored` is the empty default, the diff would delete a
    // legacy monolith the owner still has, and the banner in the Inventory
    // window promised they would be asked first.
    if sinks.recovery {
        sinks.toasts.info(
            format!(
                "Saved \"{offered}\" locally - your inventory could not be loaded, so open \
                 Inventory to save it deliberately."
            ),
            now,
        );
        return false;
    }
    let (true, Some(session), Some(refresh)) = (publish, sinks.session, sinks.refresh) else {
        return false;
    };
    sinks.feedback.status = PublishStatus::Publishing { since_secs: now };
    spawn_publish_inventory_task(sinks.commands, session, refresh, payload, stored, now);
    true
}

/// Land an accepted gift in the stash (#1108): under
/// [`choose_inventory_gift_key`]'s slot, through
/// [`InventoryRecord::put_item`] so the wear side table stays in step - a
/// gifted wearable is wearable from the Inventory window straight away,
/// exactly as a catalogue copy would be; decor stays decor. Returns the key
/// it landed under.
pub fn store_accepted_gift(
    inventory: &mut InventoryRecord,
    incoming_name: &str,
    generator: Generator,
    wear: Option<crate::pds::inventory::WearMeta>,
) -> String {
    let key = choose_inventory_gift_key(inventory, incoming_name);
    inventory.put_item(key.clone(), generator, wear);
    key
}

/// A thing to land, as a test gives one.
#[cfg(test)]
pub(crate) fn thing(generator: Generator) -> GiftLanding {
    GiftLanding::Generator {
        generator: Box::new(generator),
        wear: None,
    }
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
    /// else - and the gift must not land on the stored "lantern" either.
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
            thing(Generator::default_cuboid()),
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
    /// wear side table never heard of it - every gifted wearable arrived as
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
mod gift_arrival_tests {
    use super::*;

    /// #1844: one landing rule for every kind. An avatar given while the
    /// stash holds a THING by its name - one namespace across kinds - and a
    /// stored item by the next, lands under a name free in both, in live,
    /// and the save is the stored stash plus the gift and nothing else.
    #[test]
    fn an_accepted_avatar_lands_under_a_free_name_across_kinds() {
        use crate::pds::inventory::AvatarItem;
        use crate::pds::inventory::avatar_item_tests::rigged_record;
        let mut stored = crate::pds::InventoryRecord::default();
        stored.put_item(String::from("me_2"), Generator::default_cuboid(), None);
        let mut live = stored.clone();
        live.remove_item("me_2");
        live.put_item(String::from("me"), Generator::default_cuboid(), None);
        live.put_item(String::from("unsaved"), Generator::default_cuboid(), None);
        let avatar = AvatarItem::from_record(&rigged_record()).expect("copies");

        let (key, payload) = accept_gift(
            &mut live,
            &stored,
            "me",
            GiftLanding::Avatar(Box::new(avatar.clone())),
        );

        assert_eq!(key, "me_3", "free in live AND stored, across kinds");
        assert_eq!(live.avatars.get(&key), Some(&avatar));
        assert!(live.generators.contains_key("me"), "the thing it met stays");
        assert_eq!(payload.avatars.get(&key), Some(&avatar));
        assert!(payload.generators.contains_key("me_2"), "stored kept");
        assert!(
            !payload.generators.contains_key("unsaved") && !payload.generators.contains_key("me"),
            "no unsaved edit rides along"
        );
        assert_eq!(payload.item_count(), 2);
    }

    /// #1220 f119. The sequence: a friend gifts you "lantern", you already
    /// have one, you accept - and the item is in your stash as "lantern_2"
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
            thing(crate::pds::Generator::default()),
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
            thing(crate::pds::Generator::default()),
        );
        assert_eq!(key, "lantern");
    }

    /// A colliding gift (#1717): its key is one the sanitiser keeps, so the
    /// published stash, sanitised as any stash is on its way in, keeps it
    /// beside the item it collided with - a name at the cap, whose suffix
    /// must leave it room, and a name the offer's 64-character cut left
    /// ending in a space, which the sanitiser trims onto the item it met.
    #[test]
    fn a_colliding_gift_named_at_the_cap_survives_the_sanitiser() {
        let at_cap = "g".repeat(crate::config::state::MAX_INVENTORY_NAME_CHARS);
        for (held, offered) in [
            (at_cap.clone(), at_cap.clone()),
            ("lamp".to_owned(), "lamp ".to_owned()),
        ] {
            let mut live = crate::pds::InventoryRecord::default();
            live.put_item(held.clone(), crate::pds::Generator::default(), None);
            let stored = live.clone();
            let (key, mut payload) = accept_gift(
                &mut live,
                &stored,
                &offered,
                thing(crate::pds::Generator::default()),
            );
            assert_ne!(key, held, "{offered:?}");
            payload.sanitize();
            assert!(payload.generators.contains_key(&held), "the item it met");
            assert!(
                payload.generators.contains_key(&key),
                "and the gift, {key:?}"
            );
        }
    }
}

#[cfg(test)]
mod search_tests {
    use super::*;
    use crate::pds::inventory::WearMeta;

    fn hat() -> WearMeta {
        WearMeta {
            socket: String::from("head"),
            fit_band_mm: 0,
            fit_beast_mm: 0,
            offset: crate::pds::TransformData::default(),
        }
    }

    /// The three facets the row draws are the three the filter reads
    /// (#1275 f134).
    #[test]
    fn row_matches_reads_name_kind_and_socket() {
        let worn = hat();
        // Empty query is everything, which is what an untouched field means.
        assert!(row_matches("lantern", "prim", None, ""));

        assert!(row_matches("Ship's Lantern", "prim", None, "lantern"));
        assert!(row_matches("lantern", "lsystem", None, "lsystem"));
        assert!(!row_matches("lantern", "prim", None, "lsystem"));

        // The wearable facets, which only a worn row carries.
        assert!(row_matches("circlet", "prim", Some(&worn), "wearable"));
        assert!(row_matches("circlet", "prim", Some(&worn), "head"));
        assert!(!row_matches("circlet", "prim", None, "wearable"));
        assert!(!row_matches("circlet", "prim", None, "head"));

        // The caller lower-cases and trims; the needle is matched as given.
        assert!(row_matches("LANTERN", "prim", None, "lantern"));
    }

    /// The filter draws fewer rows and changes nothing else (#1275 f134).
    ///
    /// This is the property that matters, because the Inventory is the only
    /// wear surface: a row hidden by a forgotten query must still be in the
    /// stash the Save row writes, still counted against the cap, and still
    /// on the body if it was worn. Pinned over the record itself rather
    /// than over egui, which this window has no harness for.
    #[test]
    fn hiding_a_row_does_not_change_the_stash() {
        let mut live = crate::pds::InventoryRecord::default();
        live.put_item(
            String::from("lantern"),
            crate::pds::Generator::default(),
            None,
        );
        live.put_item(
            String::from("circlet"),
            crate::pds::Generator::default(),
            Some(hat()),
        );
        let before = serde_json::to_value(&live).expect("record serializes");

        let q = "circ";
        let shown: Vec<&String> = live
            .generators
            .keys()
            .filter(|n| row_matches(n, "prim", live.wear.get(*n), q))
            .collect();
        assert_eq!(shown, vec![&String::from("circlet")], "the filter narrows");

        // The record the rest of the window reads is untouched: the cap
        // count, the dirty diff and the wear side table all still see two.
        assert_eq!(live.generators.len(), 2);
        assert_eq!(live.wear.len(), 1);
        assert_eq!(
            serde_json::to_value(&live).expect("record serializes"),
            before
        );
    }

    /// "Nothing here" and "nothing matched" are different sentences, and
    /// the first one says what to do (#1275 f134).
    #[test]
    fn the_two_empty_states_say_different_things() {
        assert_ne!(empty_state(true), empty_state(false));
        assert!(empty_state(true).contains("Catalogue"));
        assert!(empty_state(false).contains("search"));
    }
}
