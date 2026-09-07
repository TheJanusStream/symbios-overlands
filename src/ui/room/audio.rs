//! Audio bridge widget — the sovereign-side mirror of
//! [`super::material::draw_texture_bridge`].
//!
//! Renders a [`SovereignAudioConfig`] picker plus a per-variant body:
//! the shared asset-reference editor for the `Referenced` variant, and
//! for the procedural `Patch` / `Sequence` variants a compact summary
//! plus an "Edit audio…" button that pops out the full structured
//! node-graph / sequence editor shipped by `bevy_symbios_audio`'s
//! `egui` feature.
//!
//! The crate's editors operate on **native** `AudioPatch` /
//! `SequenceRecipe` and are *stateful* (canvas layout, selection, and
//! zoom persist across frames). Overlands stores the Fp-quantised
//! `Sovereign*` mirror, so the bridge keeps a native *working copy* plus
//! the editor's view-state in [`AudioEditorState`], edits that directly,
//! and writes back to the sovereign record (sanitised) only when the
//! editor reports a committed change. This avoids Fp-snapping values
//! mid-drag and losing canvas layout that a naive per-frame
//! `to_native`/`from_native` would cause.

use bevy::prelude::*;
use bevy_egui::egui;
use bevy_symbios_audio::ui::{
    AudioMonitor, MonitorRequest, MonitorStatus, PatchEditorState, SequenceEditorState,
    active_instrument_canvas, audio_patch_canvas, sequence_recipe_editor, waveform,
};

use crate::pds::SovereignAudioConfig;
use crate::pds::asset_reference::SovereignAssetReference;
use crate::pds::audio::{SovereignAudioPatch, SovereignSequenceRecipe};

/// Sample rate used when auditioning a standalone `Patch` (sequences
/// carry their own). Matches the loading path's ambient bake.
const AUDITION_SAMPLE_RATE: u32 = 44_100;
/// Duration baked when auditioning a standalone `Patch`, in seconds.
const AUDITION_PATCH_SECS: f32 = 4.0;

/// Persistent state for the pop-out audio editor window. Lives on
/// [`super::RoomEditorState`]; default is "closed, no working copy".
///
/// The window edits only its native *working copy*; it never holds a
/// reference to the sovereign record. On a committed edit it stashes the
/// converted [`SovereignAudioConfig`] in [`Self::pending`] under the
/// bound [`Self::salt`]. The matching bridge call site — which *does*
/// own the live `&mut SovereignAudioConfig` for its slot — takes that
/// value the next time it runs. This keeps the window slot-agnostic, so
/// the same editor serves both the room-ambient slot and any
/// per-construct slot symmetrically.
///
/// The pending value is **delivery**, not a draft awaiting approval
/// (#1202): the crate's editors commit on every drag end, and nothing in
/// this window can decline a commit. It therefore survives the window
/// closing, the Esc ladder, and the bound slot leaving the screen — the
/// World Editor closing, a tab change, the tree selection moving — and
/// lands the next time that slot's bridge draws. It used to be a single
/// `committed` slot wiped by `close()`, and the bridge only ran while the
/// slot was on screen, so twenty minutes of node-graph work vanished on
/// the ordinary gesture of closing the window after the selection had
/// moved — while the audition kept playing the doomed working copy.
#[derive(Default)]
pub struct AudioEditorState {
    /// Whether the pop-out editor window is open.
    pub open: bool,
    /// Which slot the open editor is bound to (the bridge `salt`).
    salt: String,
    /// What the bound slot is, in the owner's words, for the title
    /// (#1202): the salt is an egui id string nobody sees elsewhere.
    label: String,
    /// Native working copy + canvas view-state for a `Patch` slot.
    patch: Option<(bevy_symbios_audio::AudioPatch, PatchEditorState)>,
    /// Native working copy + timeline view-state for a `Sequence` slot.
    sequence: Option<(bevy_symbios_audio::SequenceRecipe, SequenceEditorState)>,
    /// Committed edits awaiting pickup, keyed by slot salt. One per slot:
    /// a later commit for the same slot replaces the earlier one.
    pending: std::collections::HashMap<String, SovereignAudioConfig>,
    /// The egui frame on which the bound slot's bridge last drew, so the
    /// window can say when its edits have nowhere to land yet.
    bound_seen_frame: Option<u64>,
}

impl AudioEditorState {
    /// Seed the working copy and open the window. Exactly one of `patch`
    /// / `sequence` is populated to match the variant; the other is
    /// cleared so a stale copy from a previous slot can't leak in.
    ///
    /// Seeds from a pending commit for this slot when one is stranded
    /// there, not from `audio`: the record is behind the owner's last
    /// edit until the bridge lands it, and reseeding from the record
    /// would silently roll that edit back.
    fn open_for(&mut self, audio: &SovereignAudioConfig, salt: &str, label: &str) {
        self.salt = salt.to_string();
        self.label = label.to_string();
        self.patch = None;
        self.sequence = None;
        let seed = self.pending.get(salt).unwrap_or(audio);
        match seed {
            SovereignAudioConfig::Patch { patch } => {
                self.patch = Some((patch.to_native(), PatchEditorState::default()));
            }
            SovereignAudioConfig::Sequence { recipe } => {
                self.sequence = Some((recipe.to_native(), SequenceEditorState::default()));
            }
            // Only procedural variants have an editor; others never set
            // open via the bridge button.
            _ => {}
        }
        self.open = true;
    }

    /// Stage a committed edit for the bound slot.
    fn commit(&mut self, audio: SovereignAudioConfig) {
        self.pending.insert(self.salt.clone(), audio);
    }

    /// The bridge for `salt` takes its pending commit, if any.
    pub(crate) fn take_pending(&mut self, salt: &str) -> Option<SovereignAudioConfig> {
        self.pending.remove(salt)
    }

    /// Forget a pending commit for `salt` — the slot changed variant
    /// under it, so the edit is for a value that no longer exists.
    fn discard(&mut self, salt: &str) {
        self.pending.remove(salt);
    }

    /// Whether an edit is staged for `salt` and has not landed yet.
    pub(crate) fn has_pending(&self, salt: &str) -> bool {
        self.pending.contains_key(salt)
    }

    /// Drop the working copy and close the window. Pending commits stay:
    /// closing the window is not a way to un-commit an edit (#1202).
    pub(crate) fn close(&mut self) {
        self.open = false;
        self.patch = None;
        self.sequence = None;
    }
}

/// Variant picker + per-variant body for an audio slot.
///
/// `salt` namespaces the inner combo box so multiple bridges on the same
/// egui frame (room-ambient vs. per-construct slots) don't collide on
/// the egui id stack. The pop-out editor itself is drawn separately by
/// [`draw_audio_editor_window`] so it can float above the room editor.
pub(super) fn draw_audio_bridge(
    ui: &mut egui::Ui,
    audio: &mut SovereignAudioConfig,
    salt: &str,
    label: &str,
    dirty: &mut bool,
    editor: &mut AudioEditorState,
    assets: &mut super::assets::AssetPanel<'_>,
) {
    // Pick up any committed edit the pop-out editor staged for this slot
    // (it edits a native working copy and writes back here, keyed by
    // salt, so the window itself stays slot-agnostic — see
    // [`AudioEditorState`]). Whether or not the window is still open or
    // still bound here: a commit is delivered, never dropped (#1202).
    if let Some(committed) = editor.take_pending(salt) {
        *audio = committed;
        *dirty = true;
    }
    if editor.salt == salt {
        editor.bound_seen_frame = Some(ui.ctx().cumulative_frame_nr());
    }

    let prev_variant = std::mem::discriminant(&*audio);

    egui::ComboBox::from_id_salt(format!("{}_audio_ty", salt))
        .selected_text(audio.label())
        .show_ui(ui, |ui| {
            // Variant presets — switching variants resets the inner
            // payload because each variant carries different state.
            let presets: [(&'static str, SovereignAudioConfig); 4] = [
                ("None", SovereignAudioConfig::None),
                (
                    "Referenced",
                    SovereignAudioConfig::Referenced {
                        source: SovereignAssetReference::default(),
                    },
                ),
                (
                    "Patch",
                    SovereignAudioConfig::Patch {
                        patch: SovereignAudioPatch::default(),
                    },
                ),
                (
                    "Sequence",
                    SovereignAudioConfig::Sequence {
                        recipe: SovereignSequenceRecipe::default(),
                    },
                ),
            ];
            for (label, preset) in presets {
                let selected = std::mem::discriminant(audio) == std::mem::discriminant(&preset);
                if ui.selectable_label(selected, label).clicked() && !selected {
                    *audio = preset;
                    *dirty = true;
                }
            }
        });

    // A variant switch invalidates any open editor bound to this slot —
    // its working copy is for the old variant. Close it so the next
    // "Edit audio…" reseeds cleanly, and drop anything it had staged.
    if std::mem::discriminant(&*audio) != prev_variant {
        editor.discard(salt);
        if editor.salt == salt {
            editor.close();
        }
    }

    match audio {
        SovereignAudioConfig::None | SovereignAudioConfig::Unknown => {}
        SovereignAudioConfig::Referenced { source } => {
            // The status row inside the reference editor is the whole of
            // #1246 f349's gap for this slot: Patch and Sequence have a
            // transport with a live error label, and the ONE variant whose
            // failure is external and likely had no verification at all.
            super::widgets::draw_asset_reference_editor(
                ui,
                source,
                salt,
                dirty,
                super::widgets::ReferenceClass::Audio,
                assets,
            );
        }
        SovereignAudioConfig::Patch { patch } => {
            draw_patch_summary(ui, patch);
            edit_button(ui, audio, salt, label, editor);
        }
        SovereignAudioConfig::Sequence { recipe } => {
            draw_sequence_summary(ui, recipe);
            edit_button(ui, audio, salt, label, editor);
        }
    }
}

/// "Edit audio…" button — seeds the working copy and opens the pop-out.
fn edit_button(
    ui: &mut egui::Ui,
    audio: &SovereignAudioConfig,
    salt: &str,
    label: &str,
    editor: &mut AudioEditorState,
) {
    let is_open = editor.open && editor.salt == salt;
    let button_text = if is_open {
        "Editing… (window open)"
    } else {
        "\u{270E} Edit audio\u{2026}"
    };
    if ui
        .add_enabled(!is_open, egui::Button::new(button_text))
        .on_hover_text("Open the structured node-graph / sequence editor")
        .on_disabled_hover_text("The audio editor is already open for this patch")
        .clicked()
    {
        editor.open_for(audio, salt, label);
    }
}

/// One-line read-only summary of a `Patch` so the owner can confirm
/// what's slotted without opening the editor.
fn draw_patch_summary(ui: &mut egui::Ui, patch: &SovereignAudioPatch) {
    let n = patch.graph.nodes.len();
    let out = patch.graph.output.0;
    ui.label(
        egui::RichText::new(format!(
            "Sound patch — {n} node{}, output #{out}, seed {}",
            if n == 1 { "" } else { "s" },
            patch.seed,
        ))
        .small()
        .color(crate::ui::theme::current(ui.ctx()).text_weak),
    );
}

/// One-line read-only summary of a `Sequence`.
fn draw_sequence_summary(ui: &mut egui::Ui, recipe: &SovereignSequenceRecipe) {
    let instruments = recipe.instruments.len();
    let events: usize = recipe.tracks.iter().map(|t| t.events.len()).sum();
    ui.label(
        egui::RichText::new(format!(
            "Sequence — {:.0} BPM, {instruments} instrument{}, {} track{}, {events} event{}",
            recipe.bpm.0,
            if instruments == 1 { "" } else { "s" },
            recipe.tracks.len(),
            if recipe.tracks.len() == 1 { "" } else { "s" },
            if events == 1 { "" } else { "s" },
        ))
        .small()
        .color(crate::ui::theme::current(ui.ctx()).text_weak),
    );
}

/// Render the pop-out audio editor window, if open. Edits the native
/// working copy held in `editor`; on a committed change stashes the
/// converted sovereign value in `editor.committed` for the bound slot's
/// bridge to pick up (see [`AudioEditorState`]).
///
/// Drawn as a top-level [`egui::Window`] sibling to the World Editor so
/// the big node canvas has room to pan/zoom. Slot-agnostic: it does not
/// touch the live record, which is why one window serves every audio
/// slot.
pub(crate) fn draw_audio_editor_window(
    ctx: &egui::Context,
    editor: &mut AudioEditorState,
    monitor: &AudioMonitor,
    requests: &mut MessageWriter<MonitorRequest>,
    chrome: &mut crate::ui::layout::WindowChrome,
) {
    if !editor.open {
        return;
    }

    let id = egui::Id::new(&editor.salt).with("audio_editor");
    let mut keep_open = true;
    // One shared layout slot for every audio slot's pop-out: the window
    // id is salted per slot, but geometry-wise they are the same tool.
    let (pos, size) = chrome.place(crate::ui::layout::UiWindow::AudioEditor, ctx);
    // The bridge for the bound slot draws BEFORE this window in the same
    // system, so "seen this frame" means the slot is on screen and every
    // commit lands at once; anything else means the edits are stranded
    // until it is shown again — say so (#1202).
    let slot_on_screen = editor.bound_seen_frame == Some(ctx.cumulative_frame_nr());
    let stranded = editor.has_pending(&editor.salt);
    let response = egui::Window::new(format!("Audio Editor — {}", editor.label))
        .id(id.with("window"))
        .open(&mut keep_open)
        .resizable(true)
        .default_size(size)
        .default_pos(pos)
        .constrain_to(chrome.available_rect(ctx))
        .show(ctx, |ui| {
            if !slot_on_screen {
                ui.colored_label(
                    crate::ui::theme::current(ui.ctx()).status.warn,
                    if stranded {
                        "Edits are kept but not applied yet — the slot this window edits \
                         is not on screen. Reselect it in its editor to apply them."
                    } else {
                        "The slot this window edits is not on screen. Edits are kept and \
                         apply when it is shown again."
                    },
                );
                ui.add_space(4.0);
            }
            // The crate's editors return EditorResponse { changed,
            // rebake }; we treat `rebake` (a committed edit — drag ended
            // or a non-drag widget changed) as the write-back trigger.
            // The working copy itself is mutated in place every frame, so
            // mid-drag `changed` needs no extra handling here.
            if let Some((patch, state)) = editor.patch.as_mut() {
                let res = audio_patch_canvas(ui, patch, state, id.with("patch"));
                let committed = res.rebake.then(|| SovereignAudioConfig::from_patch(patch));
                ui.separator();
                audition_row(ui, monitor, requests, || MonitorRequest::PlayPatch {
                    patch: patch.clone(),
                    sample_rate: AUDITION_SAMPLE_RATE,
                    duration_secs: AUDITION_PATCH_SECS,
                });
                if let Some(committed) = committed {
                    editor.commit(committed);
                }
            } else if let Some((recipe, state)) = editor.sequence.as_mut() {
                let res = sequence_recipe_editor(ui, recipe, state, id.with("seq"));
                ui.separator();
                let canvas = active_instrument_canvas(ui, recipe, state, id.with("seq_canvas"));
                let committed = (res.rebake || canvas.rebake)
                    .then(|| SovereignAudioConfig::from_sequence(recipe));
                if let Some(committed) = committed {
                    editor.commit(committed);
                }
                let Some((recipe, _)) = editor.sequence.as_mut() else {
                    return;
                };
                ui.separator();
                audition_row(ui, monitor, requests, || MonitorRequest::PlaySequence {
                    recipe: recipe.clone(),
                });
            } else {
                ui.label("No editable audio in this slot.");
            }
        });
    if let Some(response) = response.as_ref() {
        chrome.remember(
            crate::ui::layout::UiWindow::AudioEditor,
            response.response.rect,
        );
    }

    // Honour the window's [x] close button, and drop the working copy
    // (a fresh "Edit audio…" reseeds from the pending or committed value).
    if !keep_open {
        // Stop any audition that was looping for this slot.
        requests.write(MonitorRequest::Stop);
        editor.close();
    }
}

/// Transport row: play/stop the working copy plus a live waveform of the
/// last baked buffer. `make_request` builds the play message lazily so
/// the (cloned) working copy is only captured when Play is pressed.
fn audition_row(
    ui: &mut egui::Ui,
    monitor: &AudioMonitor,
    requests: &mut MessageWriter<MonitorRequest>,
    make_request: impl FnOnce() -> MonitorRequest,
) {
    ui.horizontal(|ui| {
        let baking = matches!(monitor.status, MonitorStatus::Baking);
        if ui
            .add_enabled(!baking, egui::Button::new("\u{25B6} Audition"))
            .on_hover_text("Bake this audio off-thread and loop it")
            .on_disabled_hover_text("Still baking this audio — it will play when the bake finishes")
            .clicked()
        {
            requests.write(make_request());
        }
        if ui.button("\u{23F9} Stop").clicked() {
            requests.write(MonitorRequest::Stop);
        }
        let status = match &monitor.status {
            MonitorStatus::Idle => "idle".to_string(),
            MonitorStatus::Baking => "baking…".to_string(),
            MonitorStatus::Playing => "playing".to_string(),
            MonitorStatus::Error(e) => format!("error: {e}"),
        };
        ui.label(
            egui::RichText::new(status)
                .small()
                .color(crate::ui::theme::current(ui.ctx()).text_weak),
        );
    });

    if !monitor.last_samples.is_empty() {
        waveform(ui, &monitor.last_samples);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::audio::SovereignAudioPatch;

    fn patch_slot() -> SovereignAudioConfig {
        SovereignAudioConfig::Patch {
            patch: SovereignAudioPatch::default(),
        }
    }

    /// #1202 (finding 76). Sequence: open "Edit audio…" on a construct,
    /// commit an edit (drag end), click another tree row so the bound
    /// slot's bridge stops drawing, close the pop-out, reselect the row.
    /// The commit used to live in one slot that `close()` wiped, so the
    /// edit was gone before the bridge could ever take it. It must survive
    /// the close and land when the slot's bridge next runs.
    #[test]
    fn a_commit_survives_closing_the_window_and_lands_when_its_slot_is_next_drawn() {
        let mut editor = AudioEditorState::default();
        editor.open_for(&patch_slot(), "gen_oak_1", "oak / Cylinder");
        let edited = SovereignAudioPatch {
            seed: 77,
            ..Default::default()
        };
        editor.commit(SovereignAudioConfig::Patch { patch: edited });

        // The selection moves: another slot's bridge draws and takes nothing.
        assert!(editor.take_pending("gen_birch_0").is_none());
        // The window closes.
        editor.close();
        assert!(!editor.open);
        assert!(
            editor.has_pending("gen_oak_1"),
            "closing is not un-committing"
        );
        // The row is reselected: the bridge lands the edit.
        let landed = editor.take_pending("gen_oak_1").expect("the edit lands");
        assert!(matches!(landed, SovereignAudioConfig::Patch { patch } if patch.seed == 77));
        assert!(!editor.has_pending("gen_oak_1"));
    }

    /// Reopening the window for a slot whose commit is still stranded
    /// seeds from that commit, not from the record — the record is behind
    /// the owner's last edit until the bridge lands it.
    #[test]
    fn reopening_a_slot_with_a_stranded_commit_seeds_from_the_commit() {
        let mut editor = AudioEditorState::default();
        editor.open_for(&patch_slot(), "environment", "Room ambient");
        let edited = SovereignAudioPatch {
            seed: 5,
            ..Default::default()
        };
        editor.commit(SovereignAudioConfig::Patch { patch: edited });
        editor.close();
        editor.open_for(&patch_slot(), "environment", "Room ambient");
        let (working, _) = editor.patch.as_ref().expect("a patch working copy");
        assert_eq!(
            working.seed, 5,
            "the working copy carries the stranded edit"
        );
        assert_eq!(editor.label, "Room ambient");
    }

    /// A variant switch on the slot is the one thing that discards a
    /// pending commit: the edit is for a value that no longer exists.
    #[test]
    fn a_variant_switch_discards_the_pending_commit_for_that_slot_only() {
        let mut editor = AudioEditorState::default();
        editor.open_for(&patch_slot(), "a", "A");
        editor.commit(patch_slot());
        editor.open_for(&patch_slot(), "b", "B");
        editor.commit(patch_slot());
        editor.discard("a");
        assert!(!editor.has_pending("a"));
        assert!(
            editor.has_pending("b"),
            "another slot's commit is untouched"
        );
    }
}
