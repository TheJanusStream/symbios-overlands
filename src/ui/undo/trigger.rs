//! Undo/redo triggers (#864): the Ctrl+Z / Ctrl+Shift+Z (/ Ctrl+Y)
//! chords, the editors' Undo/Redo header buttons, and the toast that
//! names every step.
//!
//! Both triggers funnel through one [`UndoShortcut`] request resource
//! and one [`apply_undo_shortcut`] system, mirroring the Ctrl+S
//! `PublishShortcut` pattern: `global_shortcuts` (chord) and the editor
//! header rows (button click) only *stamp* a request; the apply system
//! owns the heavy borrows (history + record + editor state) and runs
//! the restore through `step_room`/`step_avatar`. The chord routes to
//! the **front-most open editor window** (decision 2026-07-18) via the
//! same `topmost()` scan Ctrl+S uses; a button always targets its own
//! editor.
//!
//! Requests stamped by the egui pass (buttons, inside `PostUpdate`) are
//! consumed by the next frame's `Update`; chord requests (stamped in
//! `Update` by `global_shortcuts`) are consumed the same frame — the
//! apply system is registered `.after(global_shortcuts)`.

use bevy::prelude::*;
use bevy_egui::egui;

use crate::state::{LiveAvatarRecord, LiveRoomRecord};
use crate::ui::shortcuts::EditorKind;
use crate::ui::toast::Toasts;

use super::super::avatar::AvatarEditorState;
use super::super::room::RoomEditorState;
use super::restore::{StepKind, step_avatar, step_room};
use super::{AvatarUndoHistory, RoomUndoHistory, UndoHistory};

/// A pending undo/redo request. `target: None` records "the chord was
/// pressed with no editor window open" so the apply system can toast
/// the no-op instead of silently eating the keypress.
#[derive(Resource, Default)]
pub struct UndoShortcut {
    pending: Option<(Option<EditorKind>, StepKind)>,
}

impl UndoShortcut {
    /// Stamp a request; a second stamp in the same frame replaces the
    /// first (last click wins, same as every confirm request).
    pub fn request(&mut self, target: Option<EditorKind>, kind: StepKind) {
        self.pending = Some((target, kind));
    }

    fn take(&mut self) -> Option<(Option<EditorKind>, StepKind)> {
        self.pending.take()
    }
}

/// Consume a pending request and run the restore. `Update`, ordered
/// after `global_shortcuts` so a chord applies the same frame it was
/// pressed; a button click (stamped during the egui pass) applies on
/// the next frame's run.
#[allow(clippy::too_many_arguments)]
pub fn apply_undo_shortcut(
    mut shortcut: ResMut<UndoShortcut>,
    mut room_history: ResMut<RoomUndoHistory>,
    mut avatar_history: ResMut<AvatarUndoHistory>,
    room_record: Option<ResMut<LiveRoomRecord>>,
    avatar_record: Option<ResMut<LiveAvatarRecord>>,
    mut room_editor: ResMut<RoomEditorState>,
    mut avatar_editor: ResMut<AvatarEditorState>,
    mut toasts: ResMut<Toasts>,
    time: Res<Time>,
) {
    // Guarded take: don't flip the resource's change tick every frame.
    if shortcut.pending.is_none() {
        return;
    }
    let Some((target, kind)) = shortcut.take() else {
        return;
    };
    let now = time.elapsed_secs_f64();
    let verb = match kind {
        StepKind::Undo => "undo",
        StepKind::Redo => "redo",
    };
    let stepped = match target {
        None => {
            toasts.info(format!("Nothing to {verb} — no editor open"), now);
            return;
        }
        Some(EditorKind::World) => {
            let Some(mut record) = room_record else {
                return;
            };
            step_room(kind, &mut room_history, &mut record, &mut room_editor)
        }
        Some(EditorKind::Avatar) => {
            let Some(mut record) = avatar_record else {
                return;
            };
            step_avatar(kind, &mut avatar_history, &mut record, &mut avatar_editor)
        }
        // Inventory has no undo stack (decision 2026-07-18) and is
        // never stamped as a target.
        Some(EditorKind::Inventory) => return,
    };
    match (stepped, kind) {
        (Some(label), StepKind::Undo) => toasts.info(format!("Undid: {label}"), now),
        (Some(label), StepKind::Redo) => toasts.info(format!("Redid: {label}"), now),
        (None, _) => toasts.info(format!("Nothing to {verb}"), now),
    }
}

/// The Undo/Redo pair for an editor's header row. Enabled state and
/// hover text derive from the history; a click stamps the shared
/// [`UndoShortcut`], so buttons and chords share one application path
/// (and one toast). Text labels, not glyphs — the #816 glyph-coverage
/// audit is the reason there are no ⟲/⟳ arrows here.
pub fn undo_redo_buttons<R, S>(
    ui: &mut egui::Ui,
    history: &UndoHistory<R, S>,
    kind: EditorKind,
    shortcut: &mut UndoShortcut,
) where
    R: Send + Sync + 'static,
    S: Send + Sync + 'static,
{
    let undo = ui
        .add_enabled(history.can_undo(), egui::Button::new("Undo"))
        .on_hover_text(match history.undo_label() {
            Some(label) => format!("Undo {label} (Ctrl+Z)"),
            None => "Undo (Ctrl+Z)".to_string(),
        })
        .on_disabled_hover_text("Nothing to undo (Ctrl+Z)");
    if undo.clicked() {
        shortcut.request(Some(kind), StepKind::Undo);
    }
    let redo = ui
        .add_enabled(history.can_redo(), egui::Button::new("Redo"))
        .on_hover_text(match history.redo_label() {
            Some(label) => format!("Redo {label} (Ctrl+Shift+Z)"),
            None => "Redo (Ctrl+Shift+Z)".to_string(),
        })
        .on_disabled_hover_text("Nothing to redo (Ctrl+Shift+Z)");
    if redo.clicked() {
        shortcut.request(Some(kind), StepKind::Redo);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_replaces_and_take_consumes() {
        let mut s = UndoShortcut::default();
        s.request(Some(EditorKind::World), StepKind::Undo);
        s.request(Some(EditorKind::Avatar), StepKind::Redo);
        assert_eq!(s.take(), Some((Some(EditorKind::Avatar), StepKind::Redo)));
        assert_eq!(s.take(), None);
    }
}
