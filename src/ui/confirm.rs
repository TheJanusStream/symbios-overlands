//! Shared destructive-action confirmation + rename dialogs (#838).
//!
//! Until #817's undo stack lands, a confirm dialog is the only thing
//! standing between a misclick and un-undoable data loss (a root delete
//! cascades through every referencing placement; a seed re-roll or
//! Reset replaces the whole record one row above Save). This module is
//! the one implementation every editor reuses, so the danger styling
//! and the Esc/backdrop-cancels semantics stay identical everywhere:
//!
//! * [`ConfirmState<T>`] — a small owner-embedded state machine: a
//!   click on something destructive [`request`](ConfirmState::request)s
//!   confirmation with a typed payload; the owner renders
//!   [`show`](ConfirmState::show) every frame and receives the payload
//!   back exactly once when (and only when) the danger button is
//!   clicked. Esc / backdrop click cancels — there is deliberately no
//!   Enter-to-confirm on a destructive dialog.
//! * [`rename_dialog`] — the shared rename modal (World Editor
//!   generators, Inventory items): keeps itself open on invalid input
//!   with the reason inline (the old copies silently closed and did
//!   nothing), Enter applies, Esc cancels, and the field is focused
//!   only on the frame the dialog opens so Tab still works.

use bevy_egui::egui;

/// A danger-styled button: white label on the theme's danger red
/// ([`crate::ui::theme::Theme::danger_fill`], #856). Shared by the
/// confirm modal and the unsaved-guard's Discard action so "this loses
/// work" reads identically everywhere.
pub fn danger_button(label: &str, th: &crate::ui::theme::Theme) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(label.to_owned()).color(egui::Color32::WHITE))
        .fill(th.danger_fill)
}

/// The text + payload of one pending confirmation.
struct PendingConfirm<T> {
    title: String,
    body: String,
    confirm_label: String,
    payload: T,
}

/// Owner-embedded confirmation state: at most one pending destructive
/// action, rendered as an [`egui::Modal`] until answered. `T` is
/// whatever the owner needs to perform the action after the human says
/// yes (a node id, a new preset, a `RecordAction`, …).
pub struct ConfirmState<T> {
    pending: Option<PendingConfirm<T>>,
}

// Manual impl: `#[derive(Default)]` would needlessly bound `T: Default`.
impl<T> Default for ConfirmState<T> {
    fn default() -> Self {
        Self { pending: None }
    }
}

impl<T> ConfirmState<T> {
    /// Park `payload` behind a confirmation dialog. A second request
    /// while one is pending replaces it — the newer click is the one
    /// the user is looking at.
    pub fn request(
        &mut self,
        title: impl Into<String>,
        body: impl Into<String>,
        confirm_label: impl Into<String>,
        payload: T,
    ) {
        self.pending = Some(PendingConfirm {
            title: title.into(),
            body: body.into(),
            confirm_label: confirm_label.into(),
            payload,
        });
    }

    /// True while a dialog is up (callers can use this to suppress
    /// conflicting input handling).
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Render the modal when pending. Returns the payload exactly once,
    /// on the frame the danger button is clicked; Esc, backdrop click,
    /// or Cancel drop the request. `salt` keeps two simultaneously-alive
    /// `ConfirmState`s (different editors) on distinct egui ids.
    pub fn show(&mut self, ctx: &egui::Context, salt: &str) -> Option<T> {
        let pending = self.pending.as_ref()?;
        let mut outcome: Option<bool> = None; // Some(true)=confirm, Some(false)=cancel

        let modal =
            egui::Modal::new(egui::Id::new(("destructive-confirm", salt))).show(ctx, |ui| {
                ui.heading(&pending.title);
                ui.add_space(4.0);
                ui.label(&pending.body);
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        outcome = Some(false);
                    }
                    // Push the danger button to the far side so it is
                    // never adjacent to Cancel (misclick separation).
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(danger_button(
                                &pending.confirm_label,
                                &crate::ui::theme::current(ui.ctx()),
                            ))
                            .clicked()
                        {
                            outcome = Some(true);
                        }
                    });
                });
            });
        if modal.should_close() && outcome.is_none() {
            outcome = Some(false);
        }

        match outcome {
            Some(true) => self.pending.take().map(|p| p.payload),
            Some(false) => {
                self.pending = None;
                None
            }
            None => None,
        }
    }
}

/// Outcome of [`rename_dialog`] for one frame.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RenameOutcome {
    /// Dialog still up (or kept open by invalid input).
    Open,
    /// Cancelled — Esc, backdrop, or the Cancel button.
    Cancelled,
    /// Applied with this validated name.
    Renamed(String),
}

/// Validate a draft key against a taken-set — the shared rule for both
/// rename dialogs. Renaming to the unchanged old name is a valid no-op
/// (treated as apply so Enter always dismisses); an empty or taken name
/// explains itself. Pure, unit-tested below.
pub fn validate_new_key(
    draft: &str,
    old: &str,
    is_taken: impl Fn(&str) -> bool,
) -> Result<(), String> {
    let trimmed = draft.trim();
    if trimmed.is_empty() {
        return Err("Name cannot be empty.".to_owned());
    }
    if trimmed != old && is_taken(trimmed) {
        return Err(format!("\"{trimmed}\" is already taken."));
    }
    Ok(())
}

/// The shared rename modal (#838): edits `draft` in place and reports
/// the frame's outcome. Stays open on invalid input with the reason
/// inline; Enter applies (when valid), Esc / backdrop / Cancel dismiss.
/// The text field grabs focus only on the dialog's first frame — the
/// old copies called `request_focus()` every frame, which made Tab
/// useless.
pub fn rename_dialog(
    ctx: &egui::Context,
    title: &str,
    old_name: &str,
    draft: &mut String,
    is_taken: impl Fn(&str) -> bool,
) -> RenameOutcome {
    let modal_id = egui::Id::new(("rename-dialog", title));
    // First frame = egui has no area rect for the modal yet.
    let first_frame = ctx.memory(|m| m.area_rect(modal_id).is_none());

    let mut outcome = RenameOutcome::Open;
    let validation = validate_new_key(draft, old_name, &is_taken);

    let modal = egui::Modal::new(modal_id).show(ctx, |ui| {
        ui.heading(title);
        ui.add_space(4.0);
        let field = ui.text_edit_singleline(draft);
        if first_frame {
            field.request_focus();
        }
        if let Err(reason) = &validation {
            ui.colored_label(crate::ui::theme::current(ui.ctx()).status.error, reason);
        }
        ui.add_space(8.0);

        let enter_applied = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.horizontal(|ui| {
            if ui
                .add_enabled(validation.is_ok(), egui::Button::new("Apply"))
                .clicked()
                || (enter_applied && validation.is_ok())
            {
                outcome = RenameOutcome::Renamed(draft.trim().to_owned());
            }
            if ui.button("Cancel").clicked() {
                outcome = RenameOutcome::Cancelled;
            }
        });
        // Enter on an invalid draft: keep the dialog open (the inline
        // reason explains why) but hand focus back so typing continues.
        if enter_applied && validation.is_err() {
            field.request_focus();
        }
    });
    if outcome == RenameOutcome::Open && modal.should_close() {
        outcome = RenameOutcome::Cancelled;
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirm_state_holds_payload_until_answered() {
        let mut state: ConfirmState<u32> = ConfirmState::default();
        assert!(!state.is_pending());
        state.request("Delete?", "This deletes things.", "Delete", 7);
        assert!(state.is_pending());
        // A newer request replaces the pending one.
        state.request("Delete other?", "Other things.", "Delete", 9);
        assert!(state.is_pending());
        assert_eq!(state.pending.as_ref().unwrap().payload, 9);
    }

    #[test]
    fn validate_new_key_rules() {
        let taken = |s: &str| s == "existing";
        assert!(validate_new_key("fresh", "old", taken).is_ok());
        // Unchanged name is a valid no-op apply.
        assert!(validate_new_key("old", "old", taken).is_ok());
        // Whitespace-only = empty.
        assert!(
            validate_new_key("   ", "old", taken)
                .unwrap_err()
                .contains("empty")
        );
        assert!(
            validate_new_key("existing", "old", taken)
                .unwrap_err()
                .contains("already taken")
        );
        // Trimming applies before the taken check.
        assert!(validate_new_key("  existing  ", "old", taken).is_err());
    }

    /// Headless egui frame: the confirm modal renders without panicking
    /// and stays pending while unanswered.
    #[test]
    fn confirm_modal_renders_and_stays_pending() {
        let ctx = egui::Context::default();
        let mut state: ConfirmState<&'static str> = ConfirmState::default();
        state.request("Reset?", "Replaces everything.", "Reset", "payload");
        let mut returned = None;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            returned = state.show(ctx, "test");
        });
        assert_eq!(returned, None);
        assert!(state.is_pending(), "unanswered modal must stay pending");
    }

    /// Headless egui frame: the rename dialog renders, reports Open on
    /// an untouched frame, and surfaces the inline reason for a taken
    /// name without closing.
    #[test]
    fn rename_dialog_renders_and_stays_open() {
        let ctx = egui::Context::default();
        let mut draft = "existing".to_owned();
        let mut outcome = RenameOutcome::Cancelled;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            outcome = rename_dialog(ctx, "Rename Item", "old", &mut draft, |s| s == "existing");
        });
        assert_eq!(outcome, RenameOutcome::Open);
    }
}
