//! Shared destructive-action confirmation + rename dialogs (#838).
//!
//! With the #817 undo stack live, confirms guard a narrower set (#866):
//! actions whose blast radius deserves a beat of attention even though
//! undo can restore them (cascade root delete, kind change), actions in
//! an editor with NO undo stack (Inventory's Revert/Reset), and actions
//! undo cannot recall at all — network writes (publish, the recovery
//! banner's delete-then-put PDS reset, publish-over-recovery). One-click
//! in-record replacements that used to confirm here (seed re-roll,
//! locomotion preset switch, Room/Avatar Revert/Reset) now fire
//! directly — they are one Ctrl+Z away, and a modal would only
//! double-charge a recoverable click. This module is the one
//! implementation every editor reuses, so the danger styling and the
//! Esc/backdrop-cancels semantics stay identical everywhere:
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

use bevy::ecs::resource::Resource;
use bevy::ecs::system::ResMut;
use bevy_egui::egui;

/// egui temp-data key under which every modal renderer records the pass it
/// drew on. See [`note_modal_open`].
const MODAL_OPEN_ID: &str = "overlands-modal-open";

/// Record that a modal dialog owned attention on this egui pass (#1139).
///
/// Every modal in the app calls this as it draws: the destructive confirm,
/// the rename dialog, the unsaved-edits guard and the incoming-gift offer.
///
/// egui knows which layer is the top modal but keeps
/// `Memory::top_modal_layer` `pub(crate)`, and the obvious substitute —
/// `egui_wants_keyboard_input()` — is literally "some widget has focus",
/// which a dialog made only of buttons never has: egui 0.35 does not give a
/// clicked button focus. So `global_shortcuts` believed nothing was in the
/// way and ran the Esc back-out ladder in the same frame the modal consumed
/// the Esc that cancelled it, closing the window BEHIND the dialog. This
/// stamp is the missing signal, kept in egui's own per-context store so a
/// renderer holding nothing but a `&Context` can set it.
pub fn note_modal_open(ctx: &egui::Context) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|data| data.insert_temp(egui::Id::new(MODAL_OPEN_ID), pass));
}

/// True when a modal owned attention on the most recent egui pass.
///
/// Read from `Update`, which runs BEFORE the egui pass that will draw this
/// frame's modals — so the freshest stamp available is the previous pass's,
/// and one pass of slack is the whole tolerance. That lag is wanted rather
/// than merely accepted: the press that dismisses a modal must not also
/// step the ladder, and it is the frame *after* the dialog drew that the
/// keypress arrives in.
pub fn modal_is_open(ctx: &egui::Context) -> bool {
    let now = ctx.cumulative_pass_nr();
    ctx.data(|data| data.get_temp::<u64>(egui::Id::new(MODAL_OPEN_ID)))
        .is_some_and(|stamped| now.saturating_sub(stamped) <= 1)
}

/// egui temp-data key under which a non-modal popup that OWNS Escape
/// records the pass it drew on. See [`note_popup_open`].
const POPUP_OPEN_ID: &str = "overlands-popup-open";

/// Record that a menu / popup which egui will close on Escape is open
/// (#1236 f37).
///
/// Only needed by popups egui does NOT track in its own memory: a
/// [`egui::Popup`] built with `open_bool` keeps its open flag in the
/// caller's state, so [`egui::Popup::is_any_open`] cannot see it. Every
/// `menu_button` and combo box IS memory-tracked and needs no stamp.
///
/// Deliberately a SEPARATE signal from [`note_modal_open`]: a menu owns
/// the Escape key, but it does not own attention. Conflating the two would
/// freeze the avatar under an open combo box, which is what
/// [`ModalOpen`] mirrors and #1241 gates movement on.
pub fn note_popup_open(ctx: &egui::Context) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|data| data.insert_temp(egui::Id::new(POPUP_OPEN_ID), pass));
}

/// True when a menu, submenu, combo box or context menu will consume the
/// next Escape (#1236 f37).
///
/// Two sources because egui has two: memory-tracked popups (`menu_button`,
/// `ComboBox`, `Popup::menu`) answer [`egui::Popup::is_any_open`], and
/// `open_bool` popups — the in-scene right-click menu is the only one —
/// stamp [`note_popup_open`] instead. Same one-pass tolerance as
/// [`modal_is_open`], and for the same reason: the press that closes the
/// menu must not also step the back-out ladder.
pub fn popup_is_open(ctx: &egui::Context) -> bool {
    if egui::Popup::is_any_open(ctx) {
        return true;
    }
    let now = ctx.cumulative_pass_nr();
    ctx.data(|data| data.get_temp::<u64>(egui::Id::new(POPUP_OPEN_ID)))
        .is_some_and(|stamped| now.saturating_sub(stamped) <= 1)
}

/// ECS mirror of [`modal_is_open`] (#1236, consumed by #1241 f164).
///
/// [`note_modal_open`] lives in egui's per-context store, which only a
/// system holding an egui context can read — and the systems that most
/// need the answer are the FixedUpdate drive systems, which hold no egui
/// context at all. Before this, `player::guard_modal_open` asked
/// `Option<Res<UnsavedGuard>>` and therefore knew about exactly one of the
/// six modals in the app: a gift offer from a stranger blocked every click
/// while W kept walking the avatar into a portal.
///
/// Written once per frame by [`mirror_modal_open`] in `PreUpdate`, so the
/// FixedUpdate steps later in the same frame read a value at most one
/// frame old — the same slack [`modal_is_open`] already runs on.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModalOpen(pub bool);

/// Copy the egui modal stamp into [`ModalOpen`] (#1236).
///
/// Guarded (#879): an every-frame `ResMut` write would mark the resource
/// changed on every frame of the app's life.
pub fn mirror_modal_open(mut contexts: bevy_egui::EguiContexts, mut open: ResMut<ModalOpen>) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let now = modal_is_open(ctx);
    if open.0 != now {
        open.0 = now;
    }
}

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

    /// Drop a pending request without an answer. Used by the undo
    /// restore (#863): a parked payload (a `GenNodeId`, a
    /// `RecordAction`) was resolved against the pre-restore record and
    /// could re-resolve to a different node after the tree changes
    /// under the open dialog.
    pub fn cancel(&mut self) {
        self.pending = None;
    }

    /// Render the modal when pending. Returns the payload exactly once,
    /// on the frame the danger button is clicked; Esc, backdrop click,
    /// or Cancel drop the request. `salt` keeps two simultaneously-alive
    /// `ConfirmState`s (different editors) on distinct egui ids.
    pub fn show(&mut self, ctx: &egui::Context, salt: &str) -> Option<T> {
        let pending = self.pending.as_ref()?;
        note_modal_open(ctx);
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
/// (treated as apply so Enter always dismisses); an empty, invisible,
/// over-long or taken name explains itself. Pure, unit-tested below.
///
/// The length and invisible-character rules (#1205) are the typing-time
/// half of `pds::sanitize::names`: whatever this refuses, the record
/// sanitiser would otherwise have to repair on the next load — and a
/// repaired name is a name the owner did not choose.
pub fn validate_new_key(
    draft: &str,
    old: &str,
    is_taken: impl Fn(&str) -> bool,
) -> Result<(), String> {
    let trimmed = draft.trim();
    if trimmed.is_empty() {
        return Err("Name cannot be empty.".to_owned());
    }
    if crate::pds::sanitize::names::has_invisible(trimmed) {
        return Err("Name contains invisible characters — please retype it.".to_owned());
    }
    let max = crate::pds::limits::MAX_GENERATOR_NAME_CHARS;
    if trimmed.chars().count() > max {
        return Err(format!(
            "Name is too long — keep it under {max} characters."
        ));
    }
    if trimmed != old && is_taken(trimmed) {
        return Err(format!("\"{trimmed}\" is already taken."));
    }
    // The road layer's derived namespace is reserved (#1245 f382). Its
    // prefix is an idempotency key, not a name: anything wearing it is
    // deleted and regrown by the next layout edit, so a generator the owner
    // named into it would silently disappear.
    if crate::terrain::is_derived_generator_key(trimmed) {
        return Err(
            "Names starting with \"lot_building_\" or \"street_prop_\" belong to \
             the road layer — it deletes and regrows whatever wears them."
                .to_owned(),
        );
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
    note_modal_open(ctx);
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

        // The same IME guard chat's Send uses (#1263 f372) — a
        // half-composed Apply writes a garbage key into the record.
        let enter_applied = crate::ui::shortcuts::enter_submitted(ui, &field);
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

    /// #1205: a name the record sanitiser would cut or strip is refused
    /// at the point of typing, so the owner never publishes a name that
    /// comes back different at the next login.
    #[test]
    fn validate_new_key_refuses_over_long_and_invisible_names() {
        let taken = |_: &str| false;
        let max = crate::pds::limits::MAX_GENERATOR_NAME_CHARS;
        assert!(validate_new_key(&"あ".repeat(max), "old", taken).is_ok());
        assert!(
            validate_new_key(&"あ".repeat(max + 1), "old", taken)
                .unwrap_err()
                .contains("too long")
        );
        assert!(
            validate_new_key("Tree\u{200B}", "old", taken)
                .unwrap_err()
                .contains("invisible")
        );
        // A zero-width-only draft is invisible, not "empty": the field
        // is visibly non-blank and the message must say why it fails.
        assert!(
            validate_new_key("\u{200B}", "old", taken)
                .unwrap_err()
                .contains("invisible")
        );
        // Joined emoji are visible and allowed.
        assert!(validate_new_key("👨\u{200D}👩\u{200D}👧", "old", taken).is_ok());
    }

    /// Headless egui frame: the confirm modal renders without panicking
    /// and stays pending while unanswered.
    #[test]
    fn confirm_modal_renders_and_stays_pending() {
        let ctx = egui::Context::default();
        let mut state: ConfirmState<&'static str> = ConfirmState::default();
        state.request("Reset?", "Replaces everything.", "Reset", "payload");
        let mut returned = None;
        let _ = ctx.run_ui(egui::RawInput::default(), |root| {
            returned = state.show(root.ctx(), "test");
        });
        assert_eq!(returned, None);
        assert!(state.is_pending(), "unanswered modal must stay pending");
    }

    /// #1139: the signal `global_shortcuts` reads instead of egui's
    /// `pub(crate)` top-modal layer. A confirm modal is buttons only, so
    /// `egui_wants_keyboard_input()` — the gate the ladder used to share —
    /// stays false while it is up; the stamp is what tells the next
    /// `Update` that a dialog owns attention.
    #[test]
    fn a_confirm_modal_stamps_the_pass_it_drew_on() {
        let ctx = egui::Context::default();
        assert!(
            !modal_is_open(&ctx),
            "nothing has drawn yet — the ladder is free"
        );

        let mut state: ConfirmState<&'static str> = ConfirmState::default();
        state.request("Delete item?", "Cannot be undone.", "Delete", "payload");
        let _ = ctx.run_ui(egui::RawInput::default(), |root| {
            state.show(root.ctx(), "test");
        });

        assert!(
            !ctx.egui_wants_keyboard_input(),
            "precondition: a buttons-only modal focuses nothing, which is the whole bug"
        );
        assert!(
            modal_is_open(&ctx),
            "the ladder must stand down while the dialog is up"
        );
    }

    /// Modal renderers that deliberately refuse Esc, and the reason. A
    /// file listed here must SAY so on the dialog instead — a silent
    /// refusal is what #1236 f53 is about; a stated one is defensible.
    const MODALS_THAT_REFUSE_DISMISSAL: &[(&str, &str)] = &[(
        "other_session.rs",
        "both answers are consequential and there is no neutral third",
    )];

    /// #1236 f53. Sequence: hit "Log out" by accident, the unsaved-changes
    /// dialog appears, press Esc as on every other dialog in this app —
    /// nothing at all happens. The contract is stated at the top of this
    /// module ("Esc / backdrop click cancels") and enforced by
    /// `egui::Modal::should_close()`, which three of the six renderers
    /// never inspected: they stamp `note_modal_open`, so
    /// `ShortcutGate::allows_esc` is false and the press is consumed by
    /// nothing and produces no response anywhere.
    ///
    /// The review named one renderer; two more (`reauth`, `other_session`)
    /// were added after it was written, which is exactly why this is a
    /// walk and not three fixes.
    #[test]
    fn every_modal_renderer_answers_esc_or_says_why_not() {
        let ui_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui");
        let mut sources = Vec::new();
        let mut stack = vec![ui_root];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("src/ui is readable") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    sources.push(path);
                }
            }
        }

        let mut renderers = 0usize;
        for path in sources {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .expect("utf-8 file name")
                .to_owned();
            let source = std::fs::read_to_string(&path).expect("UI source is readable");
            if !source.contains("note_modal_open(ctx)") {
                continue;
            }
            renderers += 1;
            if let Some((_, why)) = MODALS_THAT_REFUSE_DISMISSAL
                .iter()
                .find(|(file, _)| *file == name)
            {
                assert!(
                    !source.contains("should_close()"),
                    "{name} is listed as refusing dismissal ({why}) but now honours it — drop it from MODALS_THAT_REFUSE_DISMISSAL"
                );
                assert!(
                    source.contains("no dismiss"),
                    "{name} refuses Esc ({why}); the dialog must say so"
                );
                continue;
            }
            assert!(
                source.contains("should_close()"),
                "{name} stamps note_modal_open, so the Esc ladder stands down for it — it must answer the key itself (or join MODALS_THAT_REFUSE_DISMISSAL and say so on the dialog)"
            );
        }
        assert!(
            renderers >= 5,
            "the walk found only {renderers} modal renderers; it has stopped working"
        );
    }

    /// #1236 f37. Sequence: right-click the ground, open the scene menu,
    /// press Esc. egui closes the popup itself and stamps nothing, and
    /// this one is an `open_bool` popup — so `Popup::is_any_open`, which
    /// reads egui's own memory, cannot see it either. Without the stamp
    /// the ladder ran on the same press and cleared the selection under
    /// the menu.
    #[test]
    fn an_open_bool_popup_is_only_visible_through_the_stamp() {
        let ctx = egui::Context::default();
        assert!(!popup_is_open(&ctx), "nothing has drawn yet");

        let mut open = true;
        let _ = ctx.run_ui(egui::RawInput::default(), |root| {
            egui::Popup::new(
                egui::Id::new("test-menu"),
                root.ctx().clone(),
                egui::pos2(10.0, 10.0),
                egui::LayerId::new(egui::Order::Foreground, egui::Id::new("test-menu-layer")),
            )
            .kind(egui::PopupKind::Menu)
            .open_bool(&mut open)
            .show(|ui| {
                note_popup_open(ui.ctx());
                let _ = ui.button("Delete");
            });
        });

        assert!(
            !egui::Popup::is_any_open(&ctx),
            "precondition: an open_bool popup is NOT in egui's memory, which is the whole bug"
        );
        assert!(
            !modal_is_open(&ctx),
            "a menu is not a modal — conflating them would freeze the avatar under it"
        );
        assert!(popup_is_open(&ctx), "the Esc ladder must stand down");
    }

    /// And the popup stamp expires the same way the modal one does — one
    /// pass of slack, no more, or a menu closed long ago would keep
    /// eating Escape.
    #[test]
    fn the_popup_stamp_expires_after_one_pass() {
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |root| {
            note_popup_open(root.ctx());
        });
        assert!(popup_is_open(&ctx));
        for _ in 0..3 {
            let _ = ctx.run_ui(egui::RawInput::default(), |_| {});
        }
        assert!(!popup_is_open(&ctx));
    }

    /// And the stamp expires: it is a per-pass mark, not a latch, so
    /// closing the dialog releases the shortcuts again. Without this the
    /// first modal of a session would disable Esc for good.
    #[test]
    fn the_modal_stamp_goes_stale_once_nothing_draws() {
        let ctx = egui::Context::default();
        let mut state: ConfirmState<&'static str> = ConfirmState::default();
        state.request("Delete item?", "Cannot be undone.", "Delete", "payload");
        let _ = ctx.run_ui(egui::RawInput::default(), |root| {
            state.show(root.ctx(), "test");
        });
        assert!(modal_is_open(&ctx));

        // Two quiet passes: one is within the tolerance that covers the
        // Update-runs-before-the-egui-pass ordering.
        for _ in 0..2 {
            let _ = ctx.run_ui(egui::RawInput::default(), |_| {});
        }
        assert!(!modal_is_open(&ctx));
    }

    /// Headless egui frame: the rename dialog renders, reports Open on
    /// an untouched frame, and surfaces the inline reason for a taken
    /// name without closing.
    #[test]
    fn rename_dialog_renders_and_stays_open() {
        let ctx = egui::Context::default();
        let mut draft = "existing".to_owned();
        let mut outcome = RenameOutcome::Cancelled;
        let _ = ctx.run_ui(egui::RawInput::default(), |root| {
            outcome = rename_dialog(root.ctx(), "Rename Item", "old", &mut draft, |s| {
                s == "existing"
            });
        });
        assert_eq!(outcome, RenameOutcome::Open);
    }

    /// #1241 f164. Sequence: a stranger's gift offer pops up. You cannot
    /// click anything in the world, but W still walks you — straight into
    /// a portal — and now a second modal stacks on top of the first.
    ///
    /// The movement gate asked `Option<Res<UnsavedGuard>>` and therefore
    /// knew about one of six modals. The drive systems' OTHER gate,
    /// `not(egui_wants_any_keyboard_input)`, covers a dialog that focuses
    /// a text field — but a buttons-only dialog focuses nothing, which is
    /// the whole reason `note_modal_open` exists. This pins that the ECS
    /// mirror carries the stamp across, and that it goes back down.
    #[test]
    fn the_ecs_mirror_carries_a_buttons_only_modal_to_the_drive_systems() {
        use bevy::prelude::*;

        let ctx = egui::Context::default();
        let mut state: ConfirmState<&'static str> = ConfirmState::default();
        state.request("Delete item?", "Cannot be undone.", "Delete", "payload");
        let _ = ctx.run_ui(egui::RawInput::default(), |root| {
            state.show(root.ctx(), "test");
        });
        assert!(
            !ctx.egui_wants_keyboard_input(),
            "precondition: the OTHER gate cannot see a buttons-only dialog"
        );
        assert!(modal_is_open(&ctx));

        // The mirror is the pure half of the copy — `mirror_modal_open`
        // needs an `EguiContexts`, which no test builds.
        let mut world = World::new();
        world.init_resource::<ModalOpen>();
        world.resource_mut::<ModalOpen>().0 = modal_is_open(&ctx);
        assert!(world.resource::<ModalOpen>().0);

        // …and once the dialog stops drawing, movement comes back.
        for _ in 0..3 {
            let _ = ctx.run_ui(egui::RawInput::default(), |_| {});
        }
        world.resource_mut::<ModalOpen>().0 = modal_is_open(&ctx);
        assert!(
            !world.resource::<ModalOpen>().0,
            "a stuck mirror would freeze the player for the session"
        );
    }

    /// #1241 f164, the structural half: nothing may go back to asking
    /// about the unsaved guard alone. The gate is one run condition wired
    /// to six systems plus the portal handler, so the question has to be
    /// right in one place.
    #[test]
    fn the_movement_gate_asks_about_every_modal() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for rel in ["src/player/mod.rs", "src/player/portal.rs"] {
            let src = std::fs::read_to_string(root.join(rel)).expect("source is readable");
            assert!(
                src.contains("ModalOpen"),
                "{rel} gates movement on the unsaved guard alone again — a gift \
                 offer blocks the pointer but not the keys"
            );
        }
    }
}
