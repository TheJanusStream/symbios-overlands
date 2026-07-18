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
//!
//! Non-Esc keys are gated on egui not wanting keyboard input, so typing
//! "s" in chat never publishes a record. Esc is state-gated instead:
//! while a text field has focus egui itself consumes Esc to release it,
//! and the ladder stays out of the way. Gizmo-style S/R/G/X/Y/Z keys
//! are deliberately NOT bound — they collide with WASD/Shift movement.

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
/// window to render and consume it. The shortcut only targets an OPEN
/// window, so consumption is normally next egui pass — the TTL just
/// stops a request from firing much later if the window closes in the
/// same instant.
const PUBLISH_REQUEST_TTL_FRAMES: u8 = 3;

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
    /// The same live-vs-stored derivation the editors' own save rows
    /// use — no per-edit flags to drift out of sync with.
    fn dirty(&self, kind: EditorKind) -> bool {
        match kind {
            EditorKind::World => match (&self.live_room, &self.stored_room) {
                (Some(live), Some(stored)) => records_differ(&live.0, &stored.0),
                _ => false,
            },
            EditorKind::Avatar => match (&self.live_avatar, &self.stored_avatar) {
                (Some(live), Some(stored)) => records_differ(&live.0, &stored.0),
                _ => false,
            },
            EditorKind::Inventory => match (&self.live_inventory, &self.stored_inventory) {
                (Some(live), Some(stored)) => records_differ(&live.0, &stored.0),
                _ => false,
            },
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

/// The egui area id of a toolbar-managed window — `egui::Window` keys
/// its area by `Id::new(title)`. The audio pop-out salts its own id and
/// is handled as an explicit ladder step instead.
fn window_area_id(window: UiWindow) -> egui::Id {
    egui::Id::new(match window {
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
    })
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
) {
    // Guarded so the every-frame system doesn't flag the resource
    // changed while nothing is pending.
    if publish.pending.is_some() {
        publish.tick();
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let egui_wants_keyboard = ctx.wants_keyboard_input();

    // ── Esc: the back-out ladder ─────────────────────────────────────
    // While a text field is focused egui consumes Esc to release it;
    // the ladder resumes on the next press.
    if keyboard.just_pressed(KeyCode::Escape) && !egui_wants_keyboard {
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
    // Gated on egui not wanting keys: pressing Enter INSIDE the chat
    // input keeps its existing send semantics untouched.
    if (keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter))
        && !egui_wants_keyboard
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
    if ctrl && keyboard.just_pressed(KeyCode::KeyS) && !egui_wants_keyboard {
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
        if let Some(kind) = topmost(ctx, &candidates) {
            publish.request(kind);
        }
    }

    // ── Ctrl+Z / Ctrl+Shift+Z (or Ctrl+Y): undo / redo (#864) ────────
    // Routes to the front-most OPEN editor window — same `topmost` scan
    // as Ctrl+S, minus the dirty gate (an empty history toasts its own
    // no-op). Suppressed mid-gizmo-drag: restoring the record under an
    // active drag would let the drag-end commit write stale transforms
    // into the restored state; Esc-abort the drag first.
    let z = keyboard.just_pressed(KeyCode::KeyZ);
    let y = keyboard.just_pressed(KeyCode::KeyY);
    if ctrl && (z || y) && !egui_wants_keyboard && !gizmo_targets.iter().any(|t| t.is_active()) {
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
