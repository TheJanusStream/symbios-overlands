//! Global keyboard shortcuts (#836).
//!
//! Until this existed the app had ZERO global keys — only chat's
//! in-widget Enter and the gizmo drag's Escape. This module adds the
//! three that make the whole UI navigable from the keyboard:
//!
//! * **Esc — back-out ladder.** One step per press, first applicable
//!   wins: abort an active gizmo drag (handled where it always was, in
//!   `editor_gizmo::drag`) → step out of blob-element editing (handled
//!   in `editor_gizmo::blob`) → clear the ordinary editor selection
//!   (previously only possible by clicking empty scenery) → close the
//!   audio pop-out → close the top-most open window. "Top-most" is
//!   egui's own area order, so it matches what the user sees stacked.
//! * **Enter — open/focus chat.** Flips the Chat panel on and requests
//!   focus on its input via [`crate::ui::chat::ChatFocusRequest`], so a
//!   reply is two keystrokes away and typing never steers the avatar.
//! * **Ctrl+S — save the front-most dirty editor.** Routed through
//!   [`PublishShortcut`] into the shared Save/Load/Reset row, so it is
//!   IDENTICAL to clicking "Save to PDS" — same dirty gate, same
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
//! * **Is a text field focused?** Plain keys stand down, so typing "s" in
//!   chat never publishes and Enter keeps its in-widget meaning. The Ctrl
//!   chords are the exception for Ctrl+S: egui's `TextEdit` does not claim
//!   it, so saving from inside a name or seed field is a legitimate thing
//!   to want — and on wasm the browser's own dialog is suppressed anyway,
//!   so the chord produced literally nothing. Ctrl+Z/Y keep the gate:
//!   `TextEdit` owns those for text undo/redo.
//!
//! Gizmo-style S/R/G/X/Y/Z keys are deliberately NOT bound — they collide
//! with WASD/Shift movement.

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
/// it exactly like a "Save to PDS" click.
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
    /// it — a green, enabled "Save to PDS" button beside a Ctrl+S that did
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
}

impl ShortcutGate {
    /// The Esc back-out ladder. A modal answers its own Esc; a focused
    /// text field has egui consume Esc to release focus, and the ladder
    /// resumes on the next press.
    fn allows_esc(self) -> bool {
        !self.modal_open && !self.text_focus
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
    /// chords for editing the text itself.
    fn allows_undo(self) -> bool {
        !self.modal_open && !self.text_focus
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
    mut room_editor: ResMut<crate::ui::room::RoomEditorState>,
    mut avatar_editor: ResMut<crate::ui::avatar::AvatarEditorState>,
    blob_ctx: Res<crate::editor_gizmo::BlobEditContext>,
    gizmo_targets: Query<&GizmoTarget>,
    mut audio_requests: MessageWriter<bevy_symbios_audio::ui::MonitorRequest>,
    dirty: EditorDirtyState,
    mut undo: ResMut<crate::ui::undo::UndoShortcut>,
    mut toasts: ResMut<crate::ui::toast::Toasts>,
    time: Res<Time>,
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
    };

    // ── Esc: the back-out ladder ─────────────────────────────────────
    if keyboard.just_pressed(KeyCode::Escape) && gate.allows_esc() {
        if gizmo_targets.iter().any(|t| t.is_active()) {
            // Step 1 — abort the active gizmo drag. Owned by
            // `editor_gizmo::drag::manage_gizmo_drag` (PostUpdate, later
            // this same frame); doing nothing here lets it consume the
            // press exactly as before.
        } else if blob_ctx.selected_element.is_some() {
            // Step 2 — exit blob-element editing. Owned by
            // `editor_gizmo::blob::resolve_blob_edit`, same pattern.
        } else if room_editor.has_selection() || avatar_editor.has_visuals_selection() {
            // Step 3 — clear the ordinary selection (both editors; the
            // cross-editor mutex means at most one actually holds one).
            // Previously the only deselect was clicking empty scenery.
            room_editor.clear_selection();
            avatar_editor.clear_visuals_selection();
        } else if room_editor.audio_editor.open || avatar_editor.audio_editor.open {
            // Step 4 — close the audio pop-out, exactly like its [x]:
            // stop any looping audition, drop the working copy. Its egui
            // area id is salted per slot, so it gets an explicit step
            // rather than a slot in the generic top-most scan below.
            audio_requests.write(bevy_symbios_audio::ui::MonitorRequest::Stop);
            room_editor.audio_editor.close();
            avatar_editor.audio_editor.close();
        } else {
            // Step 5 — close the top-most open window, in egui's own
            // stacking order so it matches what the user sees.
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
    if ctrl && (z || y) && gate.allows_undo() && !gizmo_targets.iter().any(|t| t.is_active()) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::AvatarRecord;
    use crate::pds::avatar::{body, wardrobe};
    use bevy::ecs::system::RunSystemOnce;

    const NOTHING_IN_THE_WAY: ShortcutGate = ShortcutGate {
        modal_open: false,
        text_focus: false,
    };

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
        };
        assert!(!modal.allows_esc());
        assert!(!modal.allows_enter());
        assert!(!modal.allows_save());
        assert!(!modal.allows_undo());
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
    /// nothing at all, while the green "Save to PDS" button beside it is
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
