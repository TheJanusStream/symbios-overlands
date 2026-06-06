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
/// converted [`SovereignAudioConfig`] in [`Self::committed`], keyed by
/// the bound [`Self::salt`]. The matching bridge call site — which *does*
/// own the live `&mut SovereignAudioConfig` for its slot — pulls that
/// value the next time it runs. This "commit buffer" keeps the window
/// slot-agnostic, so the same editor serves both the room-ambient slot
/// and any per-construct slot symmetrically.
#[derive(Default)]
pub struct AudioEditorState {
    /// Whether the pop-out editor window is open.
    pub open: bool,
    /// Which slot the open editor is bound to (the bridge `salt`).
    salt: String,
    /// Native working copy + canvas view-state for a `Patch` slot.
    patch: Option<(bevy_symbios_audio::AudioPatch, PatchEditorState)>,
    /// Native working copy + timeline view-state for a `Sequence` slot.
    sequence: Option<(bevy_symbios_audio::SequenceRecipe, SequenceEditorState)>,
    /// A committed edit awaiting pickup by the bound slot's bridge. Keyed
    /// implicitly by [`Self::salt`] — the bridge only takes it when its
    /// own salt matches.
    committed: Option<SovereignAudioConfig>,
}

impl AudioEditorState {
    /// Seed the working copy from the sovereign value and open the
    /// window. Exactly one of `patch` / `sequence` is populated to match
    /// the variant; the other is cleared so a stale copy from a previous
    /// slot can't leak in.
    fn open_for(&mut self, audio: &SovereignAudioConfig, salt: &str) {
        self.salt = salt.to_string();
        self.patch = None;
        self.sequence = None;
        self.committed = None;
        match audio {
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

    /// Drop the working copy and close the window.
    fn close(&mut self) {
        self.open = false;
        self.patch = None;
        self.sequence = None;
        self.committed = None;
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
    dirty: &mut bool,
    editor: &mut AudioEditorState,
) {
    // Pick up any committed edit the pop-out editor staged for this slot
    // (it edits a native working copy and writes back here, keyed by
    // salt, so the window itself stays slot-agnostic — see
    // [`AudioEditorState`]).
    if editor.salt == salt
        && let Some(committed) = editor.committed.take()
    {
        *audio = committed;
        *dirty = true;
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
    // "Edit audio…" reseeds cleanly.
    if std::mem::discriminant(&*audio) != prev_variant && editor.salt == salt {
        editor.close();
    }

    match audio {
        SovereignAudioConfig::None | SovereignAudioConfig::Unknown => {}
        SovereignAudioConfig::Referenced { source } => {
            super::widgets::draw_asset_reference_editor(ui, source, salt, dirty);
        }
        SovereignAudioConfig::Patch { patch } => {
            draw_patch_summary(ui, patch);
            edit_button(ui, audio, salt, editor);
        }
        SovereignAudioConfig::Sequence { recipe } => {
            draw_sequence_summary(ui, recipe);
            edit_button(ui, audio, salt, editor);
        }
    }
}

/// "Edit audio…" button — seeds the working copy and opens the pop-out.
fn edit_button(
    ui: &mut egui::Ui,
    audio: &SovereignAudioConfig,
    salt: &str,
    editor: &mut AudioEditorState,
) {
    let is_open = editor.open && editor.salt == salt;
    let label = if is_open {
        "Editing… (window open)"
    } else {
        "\u{270E} Edit audio\u{2026}"
    };
    if ui
        .add_enabled(!is_open, egui::Button::new(label))
        .on_hover_text("Open the structured node-graph / sequence editor")
        .clicked()
    {
        editor.open_for(audio, salt);
    }
}

/// One-line read-only summary of a `Patch` so the owner can confirm
/// what's slotted without opening the editor.
fn draw_patch_summary(ui: &mut egui::Ui, patch: &SovereignAudioPatch) {
    let n = patch.graph.nodes.len();
    let out = patch.graph.output.0;
    ui.label(
        egui::RichText::new(format!(
            "AudioPatch — {n} node{}, output #{out}, seed {}",
            if n == 1 { "" } else { "s" },
            patch.seed,
        ))
        .small()
        .color(egui::Color32::GRAY),
    );
}

/// One-line read-only summary of a `Sequence`.
fn draw_sequence_summary(ui: &mut egui::Ui, recipe: &SovereignSequenceRecipe) {
    let instruments = recipe.instruments.len();
    let events: usize = recipe.tracks.iter().map(|t| t.events.len()).sum();
    ui.label(
        egui::RichText::new(format!(
            "SequenceRecipe — {:.0} BPM, {instruments} instrument{}, {} track{}, {events} event{}",
            recipe.bpm.0,
            if instruments == 1 { "" } else { "s" },
            recipe.tracks.len(),
            if recipe.tracks.len() == 1 { "" } else { "s" },
            if events == 1 { "" } else { "s" },
        ))
        .small()
        .color(egui::Color32::GRAY),
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
) {
    if !editor.open {
        return;
    }

    let id = egui::Id::new(&editor.salt).with("audio_editor");
    let mut keep_open = true;
    egui::Window::new(format!("Audio Editor — {}", editor.salt))
        .id(id.with("window"))
        .open(&mut keep_open)
        .resizable(true)
        .default_width(900.0)
        .default_height(640.0)
        .default_pos([60.0, 60.0])
        .show(ctx, |ui| {
            // The crate's editors return EditorResponse { changed,
            // rebake }; we treat `rebake` (a committed edit — drag ended
            // or a non-drag widget changed) as the write-back trigger.
            // The working copy itself is mutated in place every frame, so
            // mid-drag `changed` needs no extra handling here.
            if let Some((patch, state)) = editor.patch.as_mut() {
                let res = audio_patch_canvas(ui, patch, state, id.with("patch"));
                if res.rebake {
                    editor.committed = Some(SovereignAudioConfig::from_patch(patch));
                }
                ui.separator();
                audition_row(ui, monitor, requests, || MonitorRequest::PlayPatch {
                    patch: patch.clone(),
                    sample_rate: AUDITION_SAMPLE_RATE,
                    duration_secs: AUDITION_PATCH_SECS,
                });
            } else if let Some((recipe, state)) = editor.sequence.as_mut() {
                let res = sequence_recipe_editor(ui, recipe, state, id.with("seq"));
                ui.separator();
                let canvas = active_instrument_canvas(ui, recipe, state, id.with("seq_canvas"));
                if res.rebake || canvas.rebake {
                    editor.committed = Some(SovereignAudioConfig::from_sequence(recipe));
                }
                ui.separator();
                audition_row(ui, monitor, requests, || MonitorRequest::PlaySequence {
                    recipe: recipe.clone(),
                });
            } else {
                ui.label("No editable audio in this slot.");
            }
        });

    // Honour the window's [x] close button, and drop the working copy
    // (a fresh "Edit audio…" reseeds from the committed sovereign value).
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
                .color(egui::Color32::GRAY),
        );
    });

    if !monitor.last_samples.is_empty() {
        waveform(ui, &monitor.last_samples);
    }
}
