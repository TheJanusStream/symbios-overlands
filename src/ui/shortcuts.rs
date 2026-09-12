//! Global keyboard shortcuts (#836).
//!
//! Until this existed the app had ZERO global keys — only chat's
//! in-widget Enter and the gizmo drag's Escape. This module adds the
//! three that make the whole UI navigable from the keyboard:
//!
//! * **Esc — back-out ladder.** One step per press, first applicable
//!   wins, and the rungs are [`esc_step`] — pure, so the order is
//!   testable without an egui context: abort an active gizmo drag
//!   (handled where it always was, in `editor_gizmo::drag`) → step out of
//!   blob-element editing (handled in `editor_gizmo::blob`) → disarm an
//!   armed drag-to-place → clear the ordinary editor selection
//!   (previously only possible by clicking empty scenery) → close the
//!   audio pop-out → dismiss the gateway destination picker → close the
//!   top-most open window. "Top-most" is egui's own area order, so it
//!   matches what the user sees stacked.
//!
//!   Three rungs were bolted on by #1236, and each was a place the press
//!   did something OTHER than one step: the drag disarm read the key on
//!   its own in `Update` (so one press cancelled the drag *and* stepped
//!   the ladder), the picker had no rung at all (so the press closed a
//!   toolbar window behind it), and the selection rung asked the avatar
//!   editor only about its VISUALS row — leaving a worn prop's gizmo up
//!   and the chassis frozen while Esc chewed through windows.
//! * **Enter — open/focus chat.** Flips the Chat panel on and requests
//!   focus on its input via [`crate::ui::chat::ChatFocusRequest`], so a
//!   reply is two keystrokes away and typing never steers the avatar.
//! * **Ctrl+S — save the front-most dirty editor.** Routed through
//!   [`PublishShortcut`] into the shared Save/Load/Reset row, so it is
//!   IDENTICAL to clicking "Save" — same dirty gate, same
//!   record-size hard-ceiling block. On wasm a capture-phase JS handler
//!   swallows the browser's own save dialog (see
//!   `install_ctrl_s_blocker` — wasm-only, so not linkable from a
//!   native doc build) because `prevent_default_event_handling` is
//!   deliberately `false` (F5, Ctrl+R and friends must keep working).
//!   The chord never fires silently (#1208): [`SaveChord`] says what it
//!   did, and a request the row then refuses comes back as
//!   [`crate::ui::editable::RecordAction::Refused`] with the reason.
//!
//! Routing — which chord may fire at all — is [`ShortcutGate`], and it
//! answers two questions, not one (#1139):
//!
//! * **Is a modal up?** If so nothing global fires. A dialog made only of
//!   buttons focuses no widget, so the keyboard-focus test below sees
//!   nothing in the way; Esc used to cancel the dialog AND close the
//!   window behind it in the same frame.
//! * **Is a menu up?** Esc alone stands down (#1236 f37). egui closes a
//!   popup on Escape without telling anyone, so the identical
//!   double-step applied to every `menu_button`, every combo box and the
//!   in-scene right-click menu — see
//!   [`crate::ui::confirm::popup_is_open`].
//! * **Is a text field focused?** Plain keys stand down, so typing "s" in
//!   chat never publishes and Enter keeps its in-widget meaning. The Ctrl
//!   chords are the exception for Ctrl+S: egui's `TextEdit` does not claim
//!   it, so saving from inside a name or seed field is a legitimate thing
//!   to want — and on wasm the browser's own dialog is suppressed anyway,
//!   so the chord produced literally nothing. Ctrl+Z/Y keep the gate:
//!   `TextEdit` owns those for text undo/redo.
//!
//! * **F — go to the selection** (#1244 f148). The camera's
//!   `target_focus` is pinned to the chassis every frame, so there is no
//!   "frame selection" to bind: the only way to bring an off-screen
//!   selection into view in this world is to walk the player to it.
//!
//! Gizmo-style S/R/G/X/Y/Z keys are deliberately NOT bound — they collide
//! with WASD/Shift movement.
//!
//! ## Enter under an input method (#1263)
//!
//! **The one Enter this module does not own is the one inside a text
//! field, and it needed a guard anyway.** Chat's Send and the rename
//! dialog's Apply both read `lost_focus() && key_pressed(Enter)`, which
//! is egui's own idiom and is right for a Latin keyboard. Under an IME
//! the first Enter means "accept the candidate", and egui's `TextEdit`
//! surrenders focus on the return key with no composition check — so
//! that press could send a half-composed line to a room with no edit and
//! no delete. [`enter_submitted`] is the single place both sites ask,
//! and [`ime_composing`] is how it knows.
//!
//! **In the browser this is moot, because IME input cannot happen at
//! all.** winit's web backend emits no `Ime` events and documents
//! `set_ime_allowed` as unimplemented, and nothing mounts a hidden input
//! over the canvas — so every text field in the wasm build silently
//! swallows the keystrokes of anyone typing CJK, Korean or Vietnamese.
//! That is not fixable from this crate; what is fixable is the silence.
//! `install_ime_probe` watches for a keystroke going to an IME rather
//! than to the page and `report_ime_dead_end` says so once, naming paste
//! as the way through (both wasm-only, so not linkable from a native doc
//! build). The real fix is upstream — a focused, visually-hidden input
//! whose composition events become `egui::Event::Ime`, the shape
//! eframe's own web backend uses.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use transform_gizmo_bevy::GizmoTarget;

use crate::state::{
    LiveAvatarRecord, LiveInventoryRecord, LiveRoomRecord, StoredAvatarRecord,
    StoredInventoryRecord, StoredRoomRecord, records_differ,
};
use crate::ui::layout::UiWindow;
use crate::ui::toolbar::UiPanels;

/// Which editor a [`PublishShortcut`] request targets — the three
/// consumers of the shared Save/Load/Reset row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditorKind {
    World,
    Avatar,
    Inventory,
}

/// Frames a pending Ctrl+S request stays alive waiting for its editor
/// window to render and consume it. The shortcut opens and expands the
/// window it targets, so consumption is normally the same frame's egui
/// pass — the TTL just stops a request from firing much later if the
/// window closes in the same instant.
const PUBLISH_REQUEST_TTL_FRAMES: u8 = 3;

/// What Ctrl+S does this frame (#1208), decided from the two facts the
/// chord already had: the front-most OPEN dirty editor in egui's stacking
/// order, and which records are dirty at all. Before this the second fact
/// was never consulted — an empty candidate list did nothing and said
/// nothing, while the undo chord two blocks away toasts its own no-op.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SaveChord {
    /// An open editor with unsaved edits is front-most: save it. Its
    /// window is expanded first if collapsed — a collapsed `egui::Window`
    /// never runs its body, and the Save row that consumes the request
    /// lives in the body, so the request used to age out unseen.
    Save(EditorKind),
    /// No open editor is dirty, but this record is: open its window, save
    /// it, and say so. Reachable by editing, Esc-closing the window, and
    /// pressing the chord the Controls sheet advertises.
    OpenAndSave(EditorKind),
    /// Nothing anywhere is dirty. Said aloud rather than eaten.
    NothingToSave,
}

/// The decision behind [`SaveChord`], pure so it is testable without an
/// egui context. `front_most` is the top-most open dirty editor; `dirty`
/// answers for any editor, open or not. With several dirty and none open,
/// the first in World → Avatar → Inventory order wins — the same order
/// the candidate scan lists them.
pub fn resolve_save_chord(
    front_most: Option<EditorKind>,
    dirty: impl Fn(EditorKind) -> bool,
) -> SaveChord {
    if let Some(kind) = front_most {
        return SaveChord::Save(kind);
    }
    [EditorKind::World, EditorKind::Avatar, EditorKind::Inventory]
        .into_iter()
        .find(|kind| dirty(*kind))
        .map_or(SaveChord::NothingToSave, SaveChord::OpenAndSave)
}

impl EditorKind {
    /// The toolbar window that hosts this editor's Save row.
    fn window(self) -> UiWindow {
        match self {
            Self::World => UiWindow::WorldEditor,
            Self::Avatar => UiWindow::Avatar,
            Self::Inventory => UiWindow::Inventory,
        }
    }

    /// The record, as the user hears it.
    fn noun(self) -> &'static str {
        match self {
            Self::World => "world",
            Self::Avatar => "avatar",
            Self::Inventory => "inventory",
        }
    }
}

/// Pending Ctrl+S publish request (#836). The shortcut system decides
/// WHICH editor (front-most open + dirty) and parks it here; that
/// editor's Save/Load/Reset row takes it on its next render and treats
/// it exactly like a "Save" click.
#[derive(Resource, Default)]
pub struct PublishShortcut {
    pending: Option<(EditorKind, u8)>,
}

impl PublishShortcut {
    fn request(&mut self, kind: EditorKind) {
        self.pending = Some((kind, PUBLISH_REQUEST_TTL_FRAMES));
    }

    /// Consume the pending request if it targets `kind`.
    pub fn take(&mut self, kind: EditorKind) -> bool {
        if matches!(self.pending, Some((k, _)) if k == kind) {
            self.pending = None;
            true
        } else {
            false
        }
    }

    /// Age the pending request; drops it once the TTL runs out.
    fn tick(&mut self) {
        if let Some((_, ttl)) = &mut self.pending {
            *ttl = ttl.saturating_sub(1);
            if *ttl == 0 {
                self.pending = None;
            }
        }
    }
}

/// Dirty state of the three publishable records, grouped so the
/// shortcut system stays under Bevy's parameter ceiling.
#[derive(bevy::ecs::system::SystemParam)]
pub struct EditorDirtyState<'w> {
    live_room: Option<Res<'w, LiveRoomRecord>>,
    stored_room: Option<Res<'w, StoredRoomRecord>>,
    live_avatar: Option<Res<'w, LiveAvatarRecord>>,
    stored_avatar: Option<Res<'w, StoredAvatarRecord>>,
    live_inventory: Option<Res<'w, LiveInventoryRecord>>,
    stored_inventory: Option<Res<'w, StoredInventoryRecord>>,
}

impl EditorDirtyState<'_> {
    /// The same live-vs-stored derivation the editors' own save rows use —
    /// no per-edit flags to drift out of sync with.
    ///
    /// Per record type, because they do not share one (#1138): World and
    /// Inventory compare serialised forms, but an avatar's rigged payload
    /// lives on a `serde(skip)` field, so it asks
    /// [`avatar_is_dirty`](crate::pds::avatar::avatar_is_dirty). This doc
    /// used to claim all three were the same derivation, which is how the
    /// avatar arm stayed on `records_differ` after the Save row moved off
    /// it — a green, enabled "Save" button beside a Ctrl+S that did
    /// nothing at all for a sculpted body.
    fn dirty(&self, kind: EditorKind) -> bool {
        match kind {
            EditorKind::World => match (&self.live_room, &self.stored_room) {
                (Some(live), Some(stored)) => records_differ(&live.0, &stored.0),
                _ => false,
            },
            EditorKind::Avatar => match (&self.live_avatar, &self.stored_avatar) {
                (Some(live), Some(stored)) => {
                    crate::pds::avatar::avatar_is_dirty(&live.0, &stored.0)
                }
                _ => false,
            },
            EditorKind::Inventory => match (&self.live_inventory, &self.stored_inventory) {
                (Some(live), Some(stored)) => records_differ(&live.0, &stored.0),
                _ => false,
            },
        }
    }
}

/// Which global chords may fire this frame (#1139).
///
/// The routing policy in one place, as data, so it can be stated once and
/// tested without an egui context: `global_shortcuts` builds one of these
/// per frame from egui's focus state and the modal stamp
/// ([`crate::ui::confirm::modal_is_open`]) and asks it per branch.
#[derive(Clone, Copy, Debug)]
struct ShortcutGate {
    /// A modal dialog owned attention on the last egui pass.
    modal_open: bool,
    /// Some egui widget has keyboard focus — in practice a text field,
    /// since egui 0.35 does not focus a clicked button.
    text_focus: bool,
    /// A menu, submenu, combo box or the in-scene right-click menu was
    /// open on the last egui pass (#1236 f37). egui closes those on
    /// Escape itself, so the press is already spoken for.
    popup_open: bool,
    /// The audio pop-out's editor took the last egui pass's keyboard, so
    /// its own history owns the undo chord (#1333).
    audio_editor_keys: bool,
}

impl ShortcutGate {
    /// The Esc back-out ladder. A modal answers its own Esc; a focused
    /// text field has egui consume Esc to release focus, and the ladder
    /// resumes on the next press. An open menu is the third of those
    /// (#1236 f37): egui closes a popup on Escape without telling anyone,
    /// so the same press used to cancel the menu AND clear the selection
    /// the user was about to gizmo — the #1139 bug shape, written for
    /// popups instead of modals.
    ///
    /// Deliberately Esc only. Enter and the Ctrl chords are not keys egui
    /// popups claim, and a Ctrl+S typed with a colour picker open is still
    /// a save the user meant.
    fn allows_esc(self) -> bool {
        !self.modal_open && !self.text_focus && !self.popup_open
    }

    /// Enter opens/focuses Chat. Behind a modal this was the worst of the
    /// three: the user pressing Enter to answer a gift offer or an
    /// unsaved-edits dialog got a Chat window opened behind it with the
    /// focus moved into its input, while the modal still blocked the
    /// pointer.
    fn allows_enter(self) -> bool {
        !self.modal_open && !self.text_focus
    }

    /// Ctrl+S. Deliberately NOT gated on text focus: `TextEdit` ignores
    /// the chord, and a save requested from inside a name field is a save
    /// the user meant.
    fn allows_save(self) -> bool {
        !self.modal_open
    }

    /// Ctrl+Z / Ctrl+Y. Gated on text focus because `TextEdit` owns those
    /// chords for editing the text itself, and on the audio pop-out, whose
    /// editors keep a history of their own.
    ///
    /// This module reads Bevy's `ButtonInput<KeyCode>`, not egui's input,
    /// so a chord the crate's editor has already consumed still arrives
    /// here: without the gate the one press both walked the editor's
    /// history and reverted the room record behind it (#1333).
    fn allows_undo(self) -> bool {
        !self.modal_open && !self.text_focus && !self.audio_editor_keys
    }
}

/// One rung of the Esc back-out ladder (#1236). The module contract is
/// "one step per press, first applicable wins", and before this the rungs
/// were an `if/else if` chain inside `global_shortcuts` that two other Esc
/// consumers were not part of at all — the drag-to-place disarm read the
/// key independently in `Update`, and the gateway picker had no rung, so a
/// press aimed at it closed a toolbar window behind it instead.
///
/// Naming the rungs makes the order testable without an egui context,
/// which is the only way this ladder is checkable at all.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EscStep {
    /// Abort the active gizmo drag. Owned by `editor_gizmo::drag`
    /// (PostUpdate, later this same frame) — the ladder stands down.
    GizmoDrag,
    /// Step out of blob-element editing. Owned by `editor_gizmo::blob`,
    /// same pattern.
    BlobElement,
    /// Disarm an armed drag-to-place / drag-to-gift (#831).
    DragToPlace,
    /// Clear the ordinary editor selection — room OR avatar.
    Selection,
    /// The audio pop-out cleared its own selection; the ladder does
    /// nothing more with this press. One step per press, so the window
    /// closes on the next one (#1333).
    AudioEditorSelection,
    /// Close the audio pop-out, exactly like its title-bar close button.
    AudioPopout,
    /// Dismiss the gateway destination picker, exactly like its Close
    /// button (which also arms the re-open chip).
    GatewayPicker,
    /// Close the top-most open toolbar window.
    Window,
}

/// Everything the ladder can see this frame, as data (#1236).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct EscFacts {
    /// A `GizmoTarget` reports an active drag.
    pub gizmo_dragging: bool,
    /// `BlobEditContext` has an element selected.
    pub blob_element: bool,
    /// `PendingGeneratorDrop` is armed.
    pub drag_armed: bool,
    /// Either editor holds a gizmo selection. For the avatar this is
    /// `has_gizmo_selection()` — all THREE selections (visuals row, worn
    /// prop, worn part), not just the visuals one (#1236 f139): the other
    /// two are what `holds_avatar_still` freezes the chassis on, so a
    /// ladder that could not see them left the body frozen and answered
    /// the press by closing an unrelated window.
    pub has_selection: bool,
    /// Either editor's audio pop-out is open.
    pub audio_popout: bool,
    /// The open audio pop-out answered this Escape by clearing a selection
    /// inside itself, so this press is spent (#1333).
    pub audio_editor_selection: bool,
    /// The gateway destination picker is up (#1236 f26).
    pub gateway_picker: bool,
}

/// The back-out ladder, pure. First applicable rung wins; [`EscStep::Window`]
/// is the floor, and with nothing open at all it closes nothing.
pub fn esc_step(facts: EscFacts) -> EscStep {
    if facts.gizmo_dragging {
        EscStep::GizmoDrag
    } else if facts.blob_element {
        EscStep::BlobElement
    } else if facts.drag_armed {
        EscStep::DragToPlace
    } else if facts.audio_editor_selection {
        // Above the room's own selection: the press was aimed at the
        // window on top, which is the pop-out.
        EscStep::AudioEditorSelection
    } else if facts.has_selection {
        EscStep::Selection
    } else if facts.audio_popout {
        EscStep::AudioPopout
    } else if facts.gateway_picker {
        EscStep::GatewayPicker
    } else {
        EscStep::Window
    }
}

/// The Esc ladder's own world access, bundled so `global_shortcuts` stays
/// well under Bevy's 16-parameter ceiling (#1236). Two of these — the
/// gateway picker and the pending drag — are new rungs, and the system was
/// at 14 with no `Commands` at all, so the bundle came first.
#[derive(bevy::ecs::system::SystemParam)]
pub struct EscLadder<'w, 's> {
    commands: Commands<'w, 's>,
    room_editor: ResMut<'w, crate::ui::room::RoomEditorState>,
    avatar_editor: ResMut<'w, crate::ui::avatar::AvatarEditorState>,
    blob_ctx: Res<'w, crate::editor_gizmo::BlobEditContext>,
    gizmo_targets: Query<'w, 's, &'static GizmoTarget>,
    audio_requests: MessageWriter<'w, bevy_symbios_audio::ui::MonitorRequest>,
    picker: Option<Res<'w, crate::ui::gateway::GatewayPicker>>,
    pending_drop: ResMut<'w, crate::ui::inventory::PendingGeneratorDrop>,
}

impl EscLadder<'_, '_> {
    /// True while a gizmo drag is live — also read by the undo chord,
    /// which must not restore a record under an in-flight drag.
    fn gizmo_dragging(&self) -> bool {
        self.gizmo_targets.iter().any(|t| t.is_active())
    }

    /// Whether the audio pop-out's editor took the last egui pass's
    /// keyboard, in either editor (#1333). Bypasses change detection: this
    /// asks what happened, it does not change anything.
    fn audio_editor_keys(&self) -> bool {
        self.room_editor.audio_editor.wants_keyboard()
            || self.avatar_editor.audio_editor.wants_keyboard()
    }

    /// Read the ladder's world state into [`EscFacts`].
    fn facts(&self) -> EscFacts {
        EscFacts {
            gizmo_dragging: self.gizmo_dragging(),
            blob_element: self.blob_ctx.selected_element.is_some(),
            drag_armed: self.pending_drop.generator_name.is_some(),
            has_selection: self.room_editor.has_selection()
                || self.avatar_editor.has_gizmo_selection(),
            audio_popout: self.room_editor.audio_editor.open
                || self.avatar_editor.audio_editor.open,
            audio_editor_selection: self.room_editor.audio_editor.took_escape()
                || self.avatar_editor.audio_editor.took_escape(),
            gateway_picker: self.picker.is_some(),
        }
    }
}

/// Among `candidates` (an egui area id each), the one drawn top-most —
/// `Memory::layer_ids()` is back-to-front, so the last hit wins.
fn topmost<T: Copy>(ctx: &egui::Context, candidates: &[(egui::Id, T)]) -> Option<T> {
    ctx.memory(|memory| {
        memory
            .layer_ids()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .find_map(|layer| {
                candidates
                    .iter()
                    .find(|(id, _)| *id == layer.id)
                    .map(|(_, value)| *value)
            })
    })
}

/// The title a toolbar-managed window is drawn with — which is also its
/// egui identity, see [`window_area_id`].
fn window_title(window: UiWindow) -> &'static str {
    match window {
        UiWindow::Chat => "Chat",
        UiWindow::People => "People",
        UiWindow::Avatar => "Avatar",
        UiWindow::Inventory => "Inventory",
        UiWindow::Catalogue => "Catalogue",
        UiWindow::WorldEditor => "World Editor",
        UiWindow::Diagnostics => "Diagnostics",
        UiWindow::AudioEditor => "Audio Editor",
        UiWindow::Controls => "Controls",
        UiWindow::Settings => "Settings",
    }
}

/// The egui area id of a toolbar-managed window. The audio pop-out salts
/// its own id and is handled as an explicit ladder step instead.
///
/// Derived the way `egui::Window::new` derives it — `Id::new` over the
/// title's `Atoms::text()`, an `Option<Cow<str>>` — not over the bare
/// `&str`. Those hash differently, and from the egui 0.35 upgrade until
/// #1208 this function hashed the `&str`: `topmost` matched no window, so
/// Ctrl+S never parked a request, Ctrl+Z always reported "no editor open"
/// and Esc never closed a window. A test pins the id against the layer
/// egui actually registers.
fn window_area_id(window: UiWindow) -> egui::Id {
    use egui::IntoAtoms as _;
    egui::Id::new(window_title(window).into_atoms().text())
}

/// Un-collapse a toolbar window so its body runs on the next egui pass
/// (#1208). `egui::Window` keeps the title-bar collapse flag in a
/// `CollapsingState` stored under the area id salted with `"collapsing"`
/// (`Window::show_dyn`); writing `open = true` there is exactly what the
/// title-bar arrow does. A window never drawn has no stored state and
/// opens expanded by default, so there is nothing to do for it.
fn expand_window(ctx: &egui::Context, window: UiWindow) {
    let id = window_area_id(window).with("collapsing");
    if let Some(mut state) = egui::collapsing_header::CollapsingState::load(ctx, id)
        && !state.is_open()
    {
        state.set_open(true);
        state.store(ctx);
    }
}

/// The one global-shortcut system (Update, `InGame` only).
#[allow(clippy::too_many_arguments)]
pub fn global_shortcuts(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut contexts: EguiContexts,
    mut panels: ResMut<UiPanels>,
    mut chat_focus: ResMut<crate::ui::chat::ChatFocusRequest>,
    mut publish: ResMut<PublishShortcut>,
    mut esc: EscLadder,
    dirty: EditorDirtyState,
    mut undo: ResMut<crate::ui::undo::UndoShortcut>,
    mut toasts: ResMut<crate::notify::Toasts>,
    time: Res<Time>,
    // "Go to selection" (#1244 f148): where the gizmo host is, and the
    // channel that moves the player there.
    focus: Res<crate::editor_gizmo::GizmoFocus>,
    mut player_move: ResMut<crate::player::PlayerMoveRequest>,
    players: Query<&Transform, With<crate::state::LocalPlayer>>,
) {
    // Guarded so the every-frame system doesn't flag the resource
    // changed while nothing is pending.
    if publish.pending.is_some() {
        publish.tick();
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let gate = ShortcutGate {
        modal_open: crate::ui::confirm::modal_is_open(ctx),
        text_focus: ctx.egui_wants_keyboard_input(),
        popup_open: crate::ui::confirm::popup_is_open(ctx),
        // Read without touching change detection: this is a question about
        // the last egui pass, not an edit.
        audio_editor_keys: esc.audio_editor_keys(),
    };

    // ── Esc: the back-out ladder ─────────────────────────────────────
    // The rungs live in `esc_step`; this is only the world-writing half.
    if keyboard.just_pressed(KeyCode::Escape) && gate.allows_esc() {
        match esc_step(esc.facts()) {
            // Owned elsewhere (`editor_gizmo::drag` / `::blob`, PostUpdate,
            // later this same frame); doing nothing here lets them consume
            // the press exactly as before.
            EscStep::GizmoDrag | EscStep::BlobElement => {}
            EscStep::DragToPlace => {
                // #1236 f37. This used to be read independently in
                // `handle_generator_drop`, off the ladder entirely, so one
                // press disarmed the drag AND cleared the selection under
                // it (nothing consumes `ButtonInput`).
                esc.pending_drop.generator_name = None;
                esc.pending_drop.peer_target = None;
            }
            EscStep::Selection => {
                // Clear the ordinary selection (both editors; the
                // cross-editor mutex means at most one actually holds one).
                // Previously the only deselect was clicking empty scenery.
                esc.room_editor.clear_selection();
                esc.avatar_editor.clear_gizmo_selections();
            }
            EscStep::AudioEditorSelection => {
                // Nothing to do: the crate's editor consumed the Escape in
                // egui and cleared its own selection. The rung exists so
                // this press is spent on that and does not also close the
                // window behind it (#1333, #1236's one step per press).
            }
            EscStep::AudioPopout => {
                // Exactly like its [x]: stop any looping audition, drop the
                // working copy. Its egui area id is salted per slot, so it
                // gets an explicit rung rather than a slot in the generic
                // top-most scan below.
                esc.audio_requests
                    .write(bevy_symbios_audio::ui::MonitorRequest::Stop);
                esc.room_editor.audio_editor.close();
                esc.avatar_editor.audio_editor.close();
            }
            EscStep::GatewayPicker => {
                // #1236 f26 — the same pair the picker's own Close button
                // writes, so the re-open chip appears exactly as it does
                // after a click. The picker is not a `UiPanels` flag and so
                // was never a candidate in the window scan below; the press
                // closed a toolbar window behind it instead.
                esc.commands
                    .remove_resource::<crate::ui::gateway::GatewayPicker>();
                esc.commands
                    .insert_resource(crate::ui::gateway::GatewayDismissed);
            }
            EscStep::Window => {
                // Close the top-most open window, in egui's own stacking
                // order so it matches what the user sees.
                let candidates: Vec<(egui::Id, UiWindow)> = [
                    (UiWindow::Chat, panels.chat),
                    (UiWindow::People, panels.people),
                    (UiWindow::Avatar, panels.avatar),
                    (UiWindow::Inventory, panels.inventory),
                    (UiWindow::Catalogue, panels.catalogue),
                    (UiWindow::WorldEditor, panels.world_editor),
                    (UiWindow::Diagnostics, panels.diagnostics),
                    (UiWindow::Controls, panels.controls),
                    (UiWindow::Settings, panels.settings),
                ]
                .into_iter()
                .filter(|(_, open)| *open)
                .map(|(w, _)| (window_area_id(w), w))
                .collect();
                match topmost(ctx, &candidates) {
                    Some(UiWindow::Chat) => panels.chat = false,
                    Some(UiWindow::People) => panels.people = false,
                    Some(UiWindow::Avatar) => panels.avatar = false,
                    Some(UiWindow::Inventory) => panels.inventory = false,
                    Some(UiWindow::Catalogue) => panels.catalogue = false,
                    Some(UiWindow::WorldEditor) => panels.world_editor = false,
                    Some(UiWindow::Diagnostics) => panels.diagnostics = false,
                    Some(UiWindow::Controls) => panels.controls = false,
                    Some(UiWindow::Settings) => panels.settings = false,
                    Some(UiWindow::AudioEditor) | None => {}
                }
            }
        }
    }

    // ── Enter: open / focus chat ─────────────────────────────────────
    // Pressing Enter INSIDE the chat input keeps its existing send
    // semantics untouched, and a modal answers its own Enter.
    if (keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter))
        && gate.allows_enter()
    {
        panels.chat = true;
        chat_focus.0 = true;
    }

    // ── Ctrl+S: publish the front-most dirty editor ──────────────────
    // Cmd+S included for wasm-on-macOS muscle memory.
    let ctrl = keyboard.pressed(KeyCode::ControlLeft)
        || keyboard.pressed(KeyCode::ControlRight)
        || keyboard.pressed(KeyCode::SuperLeft)
        || keyboard.pressed(KeyCode::SuperRight);
    if ctrl && keyboard.just_pressed(KeyCode::KeyS) && gate.allows_save() {
        let candidates: Vec<(egui::Id, EditorKind)> = [
            (
                UiWindow::WorldEditor,
                EditorKind::World,
                panels.world_editor,
            ),
            (UiWindow::Avatar, EditorKind::Avatar, panels.avatar),
            (UiWindow::Inventory, EditorKind::Inventory, panels.inventory),
        ]
        .into_iter()
        .filter(|(_, kind, open)| *open && dirty.dirty(*kind))
        .map(|(w, kind, _)| (window_area_id(w), kind))
        .collect();
        let now = time.elapsed_secs_f64();
        match resolve_save_chord(topmost(ctx, &candidates), |kind| dirty.dirty(kind)) {
            SaveChord::Save(kind) => {
                publish.request(kind);
                expand_window(ctx, kind.window());
            }
            SaveChord::OpenAndSave(kind) => {
                match kind {
                    EditorKind::World => panels.world_editor = true,
                    EditorKind::Avatar => panels.avatar = true,
                    EditorKind::Inventory => panels.inventory = true,
                }
                publish.request(kind);
                expand_window(ctx, kind.window());
                toasts.info(
                    format!(
                        "Opened the {} to save your {}",
                        window_title(kind.window()),
                        kind.noun()
                    ),
                    now,
                );
            }
            SaveChord::NothingToSave => {
                toasts.info("Nothing to save — no unsaved edits", now);
            }
        }
    }

    // ── F: go to the selection (#1244 f148) ─────────────────────────
    // Unmodified, and not on the movement letters. The camera is pinned
    // to the chassis every frame, so there is no "look at" to bind — the
    // only way to bring an off-screen selection into view is to go to it.
    if keyboard.just_pressed(KeyCode::KeyF) && gate.allows_esc() {
        let now = time.elapsed_secs_f64();
        match (focus.centre, players.single().ok()) {
            (Some(centre), Some(player)) => {
                player_move.request(crate::player::PlayerMove::GoTo(crate::player::go_to_pose(
                    centre,
                    focus.radius,
                    player.translation,
                )));
            }
            // Said aloud rather than eaten, like the save chord's own
            // no-op (#1208).
            _ => toasts.info("Nothing is selected to go to", now),
        }
    }

    // ── Ctrl+Z / Ctrl+Shift+Z (or Ctrl+Y): undo / redo (#864) ────────
    // Routes to the front-most OPEN editor window — same `topmost` scan
    // as Ctrl+S, minus the dirty gate (an empty history toasts its own
    // no-op). Suppressed mid-gizmo-drag: restoring the record under an
    // active drag would let the drag-end commit write stale transforms
    // into the restored state; Esc-abort the drag first.
    //
    // Inventory is a CANDIDATE even though it has no undo stack (#1139):
    // it is an `EditorKind` and a Ctrl+S target, so skipping it here meant
    // Ctrl+Z with the Inventory window front-most silently restored the
    // World editor stacked beneath it — a whole-record replacement, with
    // a peer broadcast, from a keypress aimed at another window. As a
    // candidate it wins the scan and `apply_undo_shortcut` says so.
    let z = keyboard.just_pressed(KeyCode::KeyZ);
    let y = keyboard.just_pressed(KeyCode::KeyY);
    if ctrl && (z || y) && gate.allows_undo() && !esc.gizmo_dragging() {
        let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
        let kind = if y || shift {
            crate::ui::undo::StepKind::Redo
        } else {
            crate::ui::undo::StepKind::Undo
        };
        let candidates: Vec<(egui::Id, EditorKind)> = [
            (
                UiWindow::WorldEditor,
                EditorKind::World,
                panels.world_editor,
            ),
            (UiWindow::Avatar, EditorKind::Avatar, panels.avatar),
            (UiWindow::Inventory, EditorKind::Inventory, panels.inventory),
        ]
        .into_iter()
        .filter(|(_, _, open)| *open)
        .map(|(w, kind, _)| (window_area_id(w), kind))
        .collect();
        undo.request(topmost(ctx, &candidates), kind);
    }
}

/// Whether an IME composition is in flight right now (#1263 f372).
///
/// **Not** the check the issue proposed. `TextEditState::cursor_purpose`
/// and the whole `TextEditCursorPurpose` enum are `pub(crate)` to egui
/// 0.35 (`widgets/text_edit/state.rs:44` and `:85`), so the state that
/// answers this question exactly is not reachable from here. What IS
/// reachable is the event stream egui builds it from: `Event::Ime`, whose
/// `ImeEvent` is public.
///
/// A non-empty `Preedit` means the IME is composing and an empty one
/// means it was dismissed — epaint's own words — and `Commit` is the
/// composition ending, which is the frame the dangerous Enter arrives on.
///
/// The one-pass tail is the reason this is not a bare event test. Whether
/// a platform delivers the IME commit and the `Key::Enter` in the same
/// event batch is exactly what the finding could not establish from code,
/// and it varies by IME; a guard that only covers the same-frame case
/// would work on the author's machine and not on the user's. One pass of
/// slack is ~16 ms, which no human can type an intentional second Enter
/// inside of, so it costs nothing and covers the split-batch case too.
pub fn ime_composing(ctx: &egui::Context) -> bool {
    /// egui-memory slot holding the pass an IME event last arrived on.
    fn last_pass_id() -> egui::Id {
        egui::Id::new("symbios-ime-last-pass")
    }

    let pass = ctx.cumulative_pass_nr();
    if ctx.input(|i| i.events.iter().any(is_composition_event)) {
        ctx.data_mut(|d| d.insert_temp(last_pass_id(), pass));
        return true;
    }
    ctx.data(|d| d.get_temp::<u64>(last_pass_id()))
        .is_some_and(|last| pass.saturating_sub(last) <= 1)
}

/// Whether `event` is an IME composition step, as opposed to any other
/// input.
///
/// The deprecated `Enabled`/`Disabled` variants fall through the wildcard
/// rather than being named: egui no longer emits them, and naming a
/// deprecated variant would be a warning under the `-D warnings` gate.
fn is_composition_event(event: &egui::Event) -> bool {
    match event {
        egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) => !text.is_empty(),
        egui::Event::Ime(egui::ImeEvent::Commit(_)) => true,
        _ => false,
    }
}

/// Did this text field just commit on Enter? (#1263 f372)
///
/// The one place both text commits in the app ask the question — chat's
/// Send and the rename dialog's Apply, which were three lines apart in
/// shape and both read the key alone.
///
/// `lost_focus() && key_pressed(Enter)` is egui's own idiom and it is
/// right for a Latin keyboard, where Enter means "I am finished". Under
/// an IME, Enter also means "accept the candidate the IME is offering",
/// and egui's `TextEdit` surrenders focus on the return key with no
/// composition check of its own (`text_edit/builder.rs:1100-1117`). So
/// the first Enter of a Japanese sentence could send the half-composed
/// line to a room that has no edit and no delete, or write a
/// half-composed key into a record. Composition suppresses the commit;
/// the NEXT Enter, on a settled field, submits normally.
pub fn enter_submitted(ui: &egui::Ui, response: &egui::Response) -> bool {
    response.lost_focus()
        && ui.input(|i| i.key_pressed(egui::Key::Enter))
        && !ime_composing(ui.ctx())
}

/// Whether a browser keydown is the IME swallowing the keystroke rather
/// than the page receiving it (#1263 f357).
///
/// Both spellings, because they are the same fact from two eras of the
/// DOM: `key == "Process"` is the modern one and `keyCode == 229` the
/// legacy sentinel every browser still sends. Composition events are the
/// obvious signal and are listened for too, but they are unreliable here
/// for a specific reason: a browser starts a composition against the
/// focused *editable* element, and this app's focused element is a
/// canvas. The keystroke still gets marked as consumed by the IME, which
/// is why this is the trigger that actually fires.
///
/// Pure, so the wasm-only path has one thing a native test gate can see.
pub fn is_ime_keystroke(key: &str, key_code: u32) -> bool {
    key == "Process" || key_code == 229
}

/// What to tell a browser user whose IME just swallowed a keystroke
/// (#1263 f357).
///
/// A dead end, stated as one. There is no IME path in the wasm build to
/// fix from here: winit's web backend documents `set_ime_allowed` as
/// "Currently not implemented" (`platform_impl/web/window.rs:332`),
/// emits no `Ime` events at all, and `bevy_window` states
/// "iOS / Android / Web: Unsupported". The real fix is a focused,
/// visually-hidden input element mounted over the canvas whose
/// composition events are forwarded as `egui::Event::Ime` — the shape
/// eframe's own web backend uses — and that is upstream-shaped work in
/// bevy_egui or winit, not a change this crate can make.
///
/// So the deliverable is honesty plus the workaround that does work:
/// composing elsewhere and pasting is a real path through, and a user
/// who is told about it can finish what they were doing. A field that
/// silently swallows keystrokes is indistinguishable from a broken app.
pub const IME_UNSUPPORTED_NOTICE: &str = "Typing with an input method editor isn't supported in the browser yet — your \
     keystrokes are going to the IME and not to this field. Compose the text in another \
     app and paste it in with Ctrl+V.";

/// wasm: swallow the browser's own Ctrl+S/Cmd+S "save page" dialog with
/// a capture-phase keydown listener. The app deliberately leaves
/// `prevent_default_event_handling` false so F5 / Ctrl+R keep working —
/// this hook preventDefaults ONLY the save chord, and the Bevy/egui
/// pipeline still receives the key event normally. The listener is
/// installed once at startup and leaked (`Closure::forget`): it must
/// live for the whole page lifetime anyway.
#[cfg(target_arch = "wasm32")]
pub fn install_ctrl_s_blocker() {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    let Some(window) = web_sys::window() else {
        return;
    };
    let closure =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |event: web_sys::KeyboardEvent| {
            if (event.ctrl_key() || event.meta_key()) && event.key().eq_ignore_ascii_case("s") {
                event.prevent_default();
            }
        });
    if let Err(e) = window.add_event_listener_with_callback_and_bool(
        "keydown",
        closure.as_ref().unchecked_ref(),
        true, // capture phase — runs before the browser's default
    ) {
        warn!("failed to install Ctrl+S blocker: {e:?}");
    }
    closure.forget();
}

/// Set by the browser listeners below the first time a keystroke is seen
/// to go to an IME rather than to the page (#1263 f357).
///
/// A `static` and not a resource because the writer is a JS callback with
/// no world to write into; [`report_ime_dead_end`] is what moves it into
/// the app.
#[cfg(target_arch = "wasm32")]
static IME_ATTEMPTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// wasm: notice when a keystroke is swallowed by an input method editor,
/// so the app can say so instead of looking broken (#1263 f357).
///
/// Two listeners for one fact. `compositionstart` is the obvious one and
/// fires when the browser targets an editable element; this app's focused
/// element is a canvas, so it often will not. The keydown sentinel
/// ([`is_ime_keystroke`]) fires either way, because the browser marks the
/// keystroke as consumed by the IME whatever the target is. Both are
/// capture-phase and neither calls `preventDefault`: this listens, it
/// does not intercept.
///
/// Leaked with `Closure::forget` for the same reason the Ctrl+S blocker
/// is — it must live for the whole page lifetime.
#[cfg(target_arch = "wasm32")]
pub fn install_ime_probe() {
    use std::sync::atomic::Ordering;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    let Some(window) = web_sys::window() else {
        return;
    };

    let on_key =
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |event: web_sys::KeyboardEvent| {
            if is_ime_keystroke(&event.key(), event.key_code()) {
                IME_ATTEMPTED.store(true, Ordering::Relaxed);
            }
        });
    if let Err(e) = window.add_event_listener_with_callback_and_bool(
        "keydown",
        on_key.as_ref().unchecked_ref(),
        true,
    ) {
        warn!("failed to install the IME probe's keydown listener: {e:?}");
    }
    on_key.forget();

    let on_composition = Closure::<dyn FnMut(web_sys::Event)>::new(move |_: web_sys::Event| {
        IME_ATTEMPTED.store(true, Ordering::Relaxed);
    });
    if let Err(e) = window.add_event_listener_with_callback_and_bool(
        "compositionstart",
        on_composition.as_ref().unchecked_ref(),
        true,
    ) {
        warn!("failed to install the IME probe's composition listener: {e:?}");
    }
    on_composition.forget();
}

/// wasm: say the dead end out loud, once (#1263 f357).
///
/// Once per page load, not once per attempt: a user who keeps typing
/// under a live IME would otherwise get a toast per keystroke, and the
/// queue evicts oldest-first, so the message would bury everything else
/// the app had to say.
#[cfg(target_arch = "wasm32")]
pub fn report_ime_dead_end(
    mut toasts: ResMut<crate::notify::Toasts>,
    time: Res<Time>,
    mut told: Local<bool>,
) {
    use std::sync::atomic::Ordering;
    if *told || !IME_ATTEMPTED.load(Ordering::Relaxed) {
        return;
    }
    *told = true;
    warn!("IME input attempted in the browser build, where it cannot work (#1263)");
    toasts.warn(IME_UNSUPPORTED_NOTICE, time.elapsed_secs_f64());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::AvatarRecord;
    use crate::pds::avatar::{body, wardrobe};
    use bevy::ecs::system::RunSystemOnce;

    const NOTHING_IN_THE_WAY: ShortcutGate = ShortcutGate {
        modal_open: false,
        text_focus: false,
        popup_open: false,
        audio_editor_keys: false,
    };

    /// #1333 B12/A9. The audio pop-out's editors read Ctrl+Z from egui, but
    /// this module reads Bevy's `ButtonInput<KeyCode>`, which egui cannot
    /// consume from. Without a gate the one press both walked the editor's
    /// history and reverted the room record behind it.
    #[test]
    fn the_room_undo_chord_stands_down_while_the_audio_pop_out_owns_the_keys() {
        let editing_audio = ShortcutGate {
            audio_editor_keys: true,
            ..NOTHING_IN_THE_WAY
        };
        assert!(!editing_audio.allows_undo());
        // Only undo: Ctrl+S from inside the pop-out is still a save, and
        // the Esc ladder has its own rung rather than a gate.
        assert!(editing_audio.allows_save());
        assert!(editing_audio.allows_esc());
        // And with the pop-out not holding the keys the chord is the
        // room's again.
        assert!(NOTHING_IN_THE_WAY.allows_undo());
    }

    /// #1333: one step per press (#1236's contract). With a node selected
    /// in the pop-out, Esc clears that; the next press closes the window.
    #[test]
    fn escape_clears_the_audio_editors_selection_before_it_closes_the_window() {
        let popout_with_a_selection = EscFacts {
            audio_editor_selection: true,
            audio_popout: true,
            ..EscFacts::default()
        };
        assert_eq!(
            esc_step(popout_with_a_selection),
            EscStep::AudioEditorSelection
        );

        // The crate cleared it, so the next press falls through to closing.
        let popout_only = EscFacts {
            audio_popout: true,
            ..EscFacts::default()
        };
        assert_eq!(esc_step(popout_only), EscStep::AudioPopout);

        // The pop-out's selection outranks the room's: the window the press
        // was aimed at is the one on top.
        let both = EscFacts {
            audio_editor_selection: true,
            audio_popout: true,
            has_selection: true,
            ..EscFacts::default()
        };
        assert_eq!(esc_step(both), EscStep::AudioEditorSelection);

        // But a gizmo drag in progress still outranks everything.
        let dragging = EscFacts {
            gizmo_dragging: true,
            ..both
        };
        assert_eq!(esc_step(dragging), EscStep::GizmoDrag);
    }

    /// #1139, finding 114. Sequence: click into the World Editor's name or
    /// seed field, type, press Ctrl+S. The chord shared one gate with the
    /// plain letters, so a focused text field killed it — and on wasm the
    /// capture-phase blocker had already eaten the browser's own save
    /// dialog, so the keypress produced nothing whatsoever. `TextEdit`
    /// never claims Ctrl+S, so there is nothing to yield to.
    #[test]
    fn ctrl_s_still_fires_from_inside_a_text_field() {
        let typing = ShortcutGate {
            modal_open: false,
            text_focus: true,
            popup_open: false,
            audio_editor_keys: false,
        };
        assert!(typing.allows_save());
        // The plain keys still stand down, or typing "s" would publish and
        // Enter would lose its in-widget send.
        assert!(!typing.allows_esc());
        assert!(!typing.allows_enter());
        // And Ctrl+Z stays with the text field, which owns it for text
        // undo.
        assert!(!typing.allows_undo());
    }

    /// #1139, findings 107 and 124. Sequence: click "Revert to saved",
    /// then press Esc to back out of the confirm — or press Enter meaning
    /// "yes" on a gift offer. A modal made only of buttons focuses no
    /// widget (egui 0.35 does not focus a clicked button), so the focus
    /// test saw nothing in the way: one Esc cancelled the dialog AND closed
    /// the window behind it, and Enter opened Chat behind the modal and
    /// pulled focus into it.
    #[test]
    fn nothing_global_fires_while_a_modal_owns_attention() {
        let modal = ShortcutGate {
            modal_open: true,
            text_focus: false,
            popup_open: false,
            audio_editor_keys: false,
        };
        assert!(!modal.allows_esc());
        assert!(!modal.allows_enter());
        assert!(!modal.allows_save());
        assert!(!modal.allows_undo());
    }

    /// #1236 f37. Sequence: right-click the ground, open `Create new…`,
    /// change your mind, press Esc. egui closes the menu on Escape and
    /// tells nobody, so the same press also cleared the selection you were
    /// about to gizmo — the #1139 double-step, written for popups instead
    /// of modals. Esc alone stands down: a Ctrl+S typed with a colour
    /// picker open is still a save the user meant.
    #[test]
    fn esc_alone_stands_down_while_a_menu_is_open() {
        let menu = ShortcutGate {
            modal_open: false,
            text_focus: false,
            popup_open: true,
            audio_editor_keys: false,
        };
        assert!(!menu.allows_esc());
        assert!(menu.allows_enter());
        assert!(menu.allows_save());
        assert!(menu.allows_undo());
    }

    /// #1236. The ladder's whole contract in one place: one step per
    /// press, first applicable wins. Written as the FULL descending
    /// sequence rather than six independent cases, because the defect the
    /// rungs fix is precisely that a lower rung fired while a higher one
    /// was live.
    #[test]
    fn the_esc_ladder_takes_exactly_one_step_from_the_top() {
        let everything = EscFacts {
            gizmo_dragging: true,
            blob_element: true,
            drag_armed: true,
            has_selection: true,
            audio_popout: true,
            audio_editor_selection: true,
            gateway_picker: true,
        };
        let mut facts = everything;
        assert_eq!(esc_step(facts), EscStep::GizmoDrag);
        facts.gizmo_dragging = false;
        assert_eq!(esc_step(facts), EscStep::BlobElement);
        facts.blob_element = false;
        assert_eq!(esc_step(facts), EscStep::DragToPlace);
        facts.drag_armed = false;
        assert_eq!(esc_step(facts), EscStep::AudioEditorSelection);
        facts.audio_editor_selection = false;
        assert_eq!(esc_step(facts), EscStep::Selection);
        facts.has_selection = false;
        assert_eq!(esc_step(facts), EscStep::AudioPopout);
        facts.audio_popout = false;
        assert_eq!(esc_step(facts), EscStep::GatewayPicker);
        facts.gateway_picker = false;
        assert_eq!(esc_step(facts), EscStep::Window);
        assert_eq!(esc_step(EscFacts::default()), EscStep::Window);
    }

    /// #1236 f37. Sequence: drag an item out of Inventory, change your
    /// mind mid-flight, press Esc. The disarm used to be read
    /// independently in `handle_generator_drop`; nothing consumes
    /// `ButtonInput`, so `global_shortcuts` ran its ladder on the same
    /// press and deselected the placement underneath. As a rung it wins,
    /// and nothing below it runs.
    #[test]
    fn an_armed_drag_is_the_whole_step() {
        let dragging_over_a_selection = EscFacts {
            drag_armed: true,
            has_selection: true,
            ..EscFacts::default()
        };
        assert_eq!(esc_step(dragging_over_a_selection), EscStep::DragToPlace);
    }

    /// #1236 f26. Sequence: walk into a gateway with Chat open, decide not
    /// to travel, press Esc. `GatewayPicker` is not a `UiPanels` flag and
    /// had no rung, so the press fell through to the top-most-window scan
    /// and closed Chat while the picker stayed up. It is the one window
    /// the app opens without being asked, so it is the one users most want
    /// to dismiss reflexively.
    #[test]
    fn the_gateway_picker_is_dismissed_before_any_toolbar_window() {
        let picker_over_chat = EscFacts {
            gateway_picker: true,
            ..EscFacts::default()
        };
        assert_eq!(esc_step(picker_over_chat), EscStep::GatewayPicker);
    }

    /// #1236 f139. Sequence: right-click your hat → "Edit …" (the avatar
    /// freezes under the gizmo), press Esc. The ladder asked
    /// `has_visuals_selection`, which is one of THREE avatar gizmo
    /// selections; with only a worn prop or a worn part aimed, the rung
    /// was skipped and the press closed the Chat window behind instead,
    /// leaving the chassis axis-locked at `GravityScale(0)` until Esc had
    /// chewed through enough windows to close the Avatar one.
    ///
    /// The behavioural half is
    /// `ui::avatar::tests::a_worn_prop_selection_is_invisible_to_the_visuals_question`;
    /// this pins that the ladder asks the question that covers all three,
    /// which is the half a type checker cannot.
    #[test]
    fn the_selection_rung_asks_about_every_avatar_gizmo_selection() {
        let src = include_str!("shortcuts.rs");
        let ladder = src
            .split_once("impl EscLadder")
            .expect("EscLadder impl block")
            .1
            .split_once("\n}\n")
            .expect("end of the impl block")
            .0;
        assert!(
            ladder.contains("has_gizmo_selection()"),
            "the ladder must see worn props and worn parts, not just the visuals row"
        );
        assert!(
            !ladder.contains("has_visuals_selection"),
            "asking only about the visuals row is #1236 f139"
        );
        let arm = src
            .split_once("EscStep::Selection => {")
            .expect("the Selection arm")
            .1
            .split_once("}\n")
            .expect("end of the arm")
            .0;
        assert!(
            arm.contains("clear_gizmo_selections()"),
            "clearing only the visuals row leaves the chassis frozen"
        );
    }

    #[test]
    fn every_chord_fires_with_nothing_in_the_way() {
        assert!(NOTHING_IN_THE_WAY.allows_esc());
        assert!(NOTHING_IN_THE_WAY.allows_enter());
        assert!(NOTHING_IN_THE_WAY.allows_save());
        assert!(NOTHING_IN_THE_WAY.allows_undo());
    }

    /// A rigged record and a copy of it whose ONLY difference is a sculpt —
    /// a value on the serde-skipped `resolved`, so the two are byte-identical
    /// on the wire.
    fn saved_and_sculpted() -> (AvatarRecord, AvatarRecord) {
        let mut saved = AvatarRecord::wearing("3jzfcijpj2z2a");
        if let Some(rig) = saved.body.rigged_mut() {
            rig.resolved = Some(body::ResolvedRig {
                body: wardrobe::engine_default_for_did("did:plc:ctrl-s-test"),
                attachments: Vec::new(),
            });
        }
        let mut sculpted = saved.clone();
        if let Some(resolved) = sculpted
            .body
            .rigged_mut()
            .and_then(|rig| rig.resolved.as_mut())
        {
            resolved.body.composites.femininity += 0.25;
        }
        (saved, sculpted)
    }

    /// #1138. Sequence: open the Avatar window on a rigged body, drag a
    /// sculpt slider (or nudge a worn prop's offset), press Ctrl+S. The
    /// chord filters its candidate windows on `dirty(kind)`, so an avatar
    /// this gate calls clean is never even requested — the keypress does
    /// nothing at all, while the green "Save" button beside it is
    /// enabled and works. This gate asked `records_differ`, which cannot
    /// see a rigged edit.
    #[test]
    fn ctrl_s_sees_a_sculpted_rigged_body_as_dirty() {
        let (saved, sculpted) = saved_and_sculpted();
        assert!(
            !records_differ(&sculpted, &saved),
            "precondition: the wire forms are identical, which is why the old gate said clean"
        );

        let mut world = World::new();
        world.insert_resource(LiveAvatarRecord(sculpted));
        world.insert_resource(StoredAvatarRecord(saved));

        let dirty = world
            .run_system_once(|state: EditorDirtyState| state.dirty(EditorKind::Avatar))
            .expect("dirty query");
        assert!(
            dirty,
            "Ctrl+S must see the same unsaved work the Save row does"
        );
    }

    /// #1208, finding 262. Sequence: edit in the World Editor, Esc-close
    /// the window (or delete an item from the scene menu, which never
    /// opens a window), press Ctrl+S. The candidate scan was empty and the
    /// chord did nothing and said nothing — while Ctrl+Z in the same state
    /// toasts "no editor open". The chord now opens the dirty record's
    /// window and saves it.
    #[test]
    fn ctrl_s_with_no_editor_open_opens_the_dirty_one_and_saves() {
        let only_world = |kind: EditorKind| kind == EditorKind::World;
        assert_eq!(
            resolve_save_chord(None, only_world),
            SaveChord::OpenAndSave(EditorKind::World)
        );
        let only_inventory = |kind: EditorKind| kind == EditorKind::Inventory;
        assert_eq!(
            resolve_save_chord(None, only_inventory),
            SaveChord::OpenAndSave(EditorKind::Inventory)
        );
        // Several dirty and none open: the scan's own order decides, and
        // the toast names which one opened.
        assert_eq!(
            resolve_save_chord(None, |_| true),
            SaveChord::OpenAndSave(EditorKind::World)
        );
    }

    /// The no-op half: nothing dirty anywhere is said aloud, mirroring
    /// the undo chord's "Nothing to undo".
    #[test]
    fn ctrl_s_with_nothing_dirty_says_so() {
        assert_eq!(
            resolve_save_chord(None, |_| false),
            SaveChord::NothingToSave
        );
    }

    /// The control: an open dirty editor front-most saves exactly as
    /// before, whatever else is dirty behind it.
    #[test]
    fn ctrl_s_saves_the_front_most_open_dirty_editor() {
        assert_eq!(
            resolve_save_chord(Some(EditorKind::Avatar), |_| true),
            SaveChord::Save(EditorKind::Avatar)
        );
    }

    /// The id every routed chord and the Esc ladder's window step depend
    /// on: [`window_area_id`] must be the id egui actually keys the window
    /// on, or `topmost` matches nothing and Ctrl+S, Ctrl+Z and Esc-close
    /// all go quiet. egui 0.35 changed `Window::new` to hash the title's
    /// `Atoms::text()` — an `Option<Cow<str>>`, not the `&str` — and a
    /// hand-built `Id::new(title)` stopped matching.
    #[test]
    fn window_area_id_is_the_id_egui_keys_the_window_on() {
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |root| {
            egui::Window::new(window_title(UiWindow::WorldEditor)).show(root.ctx(), |_ui| {});
        });
        let expected = window_area_id(UiWindow::WorldEditor);
        let found = ctx.memory(|memory| memory.layer_ids().any(|layer| layer.id == expected));
        assert!(found, "the derived id must be the window's own area id");
    }

    /// #1208, finding 72. Sequence: collapse the World Editor with its
    /// title-bar arrow to see the world, edit through the gizmo, press
    /// Ctrl+S. The request was parked for an open window, but a collapsed
    /// `egui::Window` never runs its body — and the Save row that consumes
    /// the request is in the body — so it aged out on the TTL with no
    /// effect. The chord now expands the window, and the body runs on the
    /// One Enter press, as an egui event.
    #[cfg(test)]
    fn enter_key() -> egui::Event {
        egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// A composition step, as an egui event.
    #[cfg(test)]
    fn preedit(text: &str) -> egui::Event {
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: text.to_owned(),
            active_range_chars: None,
        })
    }

    /// An IME composition is seen, and it stops being seen (#1263 f372).
    ///
    /// The second half is the one that would ruin the feature if it were
    /// wrong: a guard that never clears is a chat window whose Enter key
    /// has stopped working, which is worse than the defect it fixes.
    #[test]
    fn a_composition_is_visible_for_its_pass_and_one_more() {
        let ctx = egui::Context::default();
        let pass = |events: Vec<egui::Event>| {
            let mut composing = false;
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            let _ = ctx.run_ui(input, |ui| composing = ime_composing(ui.ctx()));
            composing
        };

        assert!(!pass(vec![]), "control: a quiet field is not composing");
        assert!(
            pass(vec![preedit("\u{304B}")]),
            "a preedit is a composition"
        );
        assert!(
            pass(vec![]),
            "the one-pass tail covers an IME that splits its commit from the key"
        );
        assert!(!pass(vec![]), "and then it clears");

        // A commit is the dangerous frame: the composition is ENDING, which
        // is exactly when the platform may also deliver a Key::Enter.
        assert!(pass(vec![egui::Event::Ime(egui::ImeEvent::Commit(
            "\u{6F22}\u{5B57}".to_owned()
        ))]));

        // An empty preedit is egui's "the IME was dismissed", not a
        // composition — treating it as one would extend the guard past the
        // end of every composition by a pass for no reason.
        let _ = pass(vec![]);
        let _ = pass(vec![]);
        assert!(!pass(vec![preedit("")]));
    }

    /// Enter submits a settled field and does not submit a composing one
    /// (#1263 f372).
    ///
    /// Driven through a real `TextEdit` in a real context, because the
    /// thing being asserted is the interaction between egui's own
    /// return-key arm — which surrenders focus with no composition check
    /// (`text_edit/builder.rs:1100-1117`) — and the app's read of
    /// `lost_focus()`. A hand-rolled `Response` would assert nothing about
    /// that.
    #[test]
    fn enter_commits_a_settled_field_but_not_a_composing_one() {
        let ctx = egui::Context::default();
        let mut text = String::new();

        /// What one egui pass reported: the guarded answer the app now
        /// uses, and the OLD key-only idiom's answer beside it.
        ///
        /// The old answer is a control, not decoration. Without it the
        /// negative assertion could be passing because the field never
        /// lost focus at all, which would prove nothing about the guard
        /// (#1280's lesson: if the failing case looks like the passing
        /// case there is no check).
        struct Pass {
            submitted: bool,
            unguarded: bool,
        }

        let pass = |text: &mut String, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 200.0),
                )),
                events,
                ..Default::default()
            };
            let mut out = Pass {
                submitted: false,
                unguarded: false,
            };
            let _ = ctx.run_ui(input, |ui| {
                let response = ui.text_edit_singleline(text);
                // Read the outcome BEFORE re-focusing. `lost_focus()` is
                // "focused last pass, not now", so calling `request_focus`
                // first puts the id back and answers false — which is how
                // this harness reported no submit on a plain Enter until
                // the order was fixed. Chat does the same two things in
                // the same order for the same reason.
                out.submitted = enter_submitted(ui, &response);
                out.unguarded =
                    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if !response.has_focus() {
                    response.request_focus();
                }
            });
            out
        };

        // Focus first: `lost_focus()` needs a field that HAD focus.
        pass(&mut text, vec![]);
        assert!(
            !pass(&mut text, vec![]).submitted,
            "control: no Enter, no submit"
        );

        assert!(
            pass(&mut text, vec![enter_key()]).submitted,
            "a settled field submits on Enter"
        );

        // Re-settle, then the IME case: the commit and the key in one
        // batch, which is the platform behaviour the finding could not
        // rule out and the one that sends a half-composed line.
        pass(&mut text, vec![]);
        pass(&mut text, vec![]);
        let composing = pass(
            &mut text,
            vec![
                egui::Event::Ime(egui::ImeEvent::Commit("\u{6F22}\u{5B57}".to_owned())),
                enter_key(),
            ],
        );
        assert!(
            composing.unguarded,
            "the control: on this very pass the old key-only idiom DOES fire, \
             so the field really did lose focus to an Enter"
        );
        assert!(
            !composing.submitted,
            "the Enter that accepts an IME candidate must not send the line"
        );

        // And the next deliberate Enter, on a settled field, does send it —
        // otherwise the guard has broken the key it was protecting.
        pass(&mut text, vec![]);
        pass(&mut text, vec![]);
        assert!(
            pass(&mut text, vec![enter_key()]).submitted,
            "the second Enter submits normally"
        );
    }

    /// The browser's two spellings of "the IME took this keystroke"
    /// (#1263 f357).
    ///
    /// The wasm listener that uses this cannot be reached by any gate in
    /// this repo, so the classification it turns on is pulled out where a
    /// native test can hold it.
    #[test]
    fn an_ime_keystroke_is_recognised_by_either_spelling() {
        assert!(is_ime_keystroke("Process", 0), "the modern spelling");
        assert!(is_ime_keystroke("Unidentified", 229), "the legacy sentinel");
        assert!(!is_ime_keystroke("a", 65));
        assert!(!is_ime_keystroke("Enter", 13));
    }

    /// very next pass, inside the TTL.
    #[test]
    fn a_collapsed_editor_window_is_expanded_so_its_save_row_runs() {
        let ctx = egui::Context::default();
        // One egui pass drawing the World Editor; reports whether its body
        // ran. Time advances a whole second per pass so egui's collapse
        // animation settles.
        let pass = |t: f64| {
            let mut body_ran = false;
            let input = egui::RawInput {
                time: Some(t),
                ..Default::default()
            };
            let _ = ctx.run_ui(input, |root| {
                egui::Window::new(window_title(UiWindow::WorldEditor))
                    .collapsible(true)
                    .show(root.ctx(), |_ui| body_ran = true);
            });
            body_ran
        };
        assert!(pass(0.0), "control: an expanded window runs its body");

        // Collapse it the way the title-bar arrow does.
        let id = window_area_id(UiWindow::WorldEditor).with("collapsing");
        let mut state = egui::collapsing_header::CollapsingState::load(&ctx, id)
            .expect("drawn once, so the collapse flag is stored");
        state.set_open(false);
        state.store(&ctx);
        let mut t = 1.0;
        for _ in 0..30 {
            pass(t);
            t += 1.0;
        }
        assert!(
            !pass(t),
            "the swallow: a collapsed window does not run the closure that takes Ctrl+S"
        );

        expand_window(&ctx, UiWindow::WorldEditor);
        assert!(
            pass(t + 1.0),
            "after expanding, the body runs on the next pass — inside PUBLISH_REQUEST_TTL_FRAMES"
        );
    }

    /// The other half of "one derivation": a rigged body nobody has touched
    /// must not arm the shortcut, or Ctrl+S would publish on every press and
    /// the Save button would never grey out.
    #[test]
    fn an_untouched_rigged_body_is_not_dirty() {
        let (saved, _) = saved_and_sculpted();

        let mut world = World::new();
        world.insert_resource(LiveAvatarRecord(saved.clone()));
        world.insert_resource(StoredAvatarRecord(saved));

        let dirty = world
            .run_system_once(|state: EditorDirtyState| state.dirty(EditorKind::Avatar))
            .expect("dirty query");
        assert!(!dirty);
    }
}
