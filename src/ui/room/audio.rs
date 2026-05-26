//! Audio bridge widget — the sovereign-side mirror of
//! [`super::material::draw_texture_bridge`].
//!
//! Renders a [`SovereignAudioConfig`] picker plus the per-variant body
//! editor: shared asset-reference editor for the `Referenced` variant,
//! and a multi-line JSON text area for the procedural `Patch` /
//! `Sequence` variants (until follow-up #311 lands proper structured
//! Sovereign* mirrors of the audio crate's authoring types).

use bevy_egui::egui;

use crate::pds::SovereignAudioConfig;
use crate::pds::asset_reference::SovereignAssetReference;

/// Variant picker + per-variant editor for an audio slot.
///
/// `salt` namespaces the inner combo box so multiple bridges on the
/// same egui frame (e.g. a future per-construct audio slot alongside
/// the room-ambient slot) don't collide on the egui id stack.
pub(super) fn draw_audio_bridge(
    ui: &mut egui::Ui,
    audio: &mut SovereignAudioConfig,
    salt: &str,
    dirty: &mut bool,
) {
    egui::ComboBox::from_id_salt(format!("{}_audio_ty", salt))
        .selected_text(audio.label())
        .show_ui(ui, |ui| {
            // Variant presets — switching variants resets the inner
            // payload because the strings of one variant are not the
            // strings of another (a URL is not patch JSON, etc).
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
                        patch_json: String::new(),
                    },
                ),
                (
                    "Sequence",
                    SovereignAudioConfig::Sequence {
                        recipe_json: String::new(),
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

    match audio {
        SovereignAudioConfig::None | SovereignAudioConfig::Unknown => {}
        SovereignAudioConfig::Referenced { source } => {
            super::widgets::draw_asset_reference_editor(ui, source, salt, dirty);
        }
        SovereignAudioConfig::Patch { patch_json } => {
            ui.label(
                egui::RichText::new(
                    "Paste a `bevy_symbios_audio::AudioPatch` JSON blob. The CLI \
                     (`symbios-audio-cli`) is a convenient way to author one offline.",
                )
                .small()
                .color(egui::Color32::GRAY),
            );
            if ui
                .add(
                    egui::TextEdit::multiline(patch_json)
                        .desired_rows(8)
                        .desired_width(f32::INFINITY)
                        .code_editor(),
                )
                .changed()
            {
                *dirty = true;
            }
        }
        SovereignAudioConfig::Sequence { recipe_json } => {
            ui.label(
                egui::RichText::new(
                    "Paste a `bevy_symbios_audio::SequenceRecipe` JSON blob. Set \
                     `loop_start_beats` for a seamless ambient loop.",
                )
                .small()
                .color(egui::Color32::GRAY),
            );
            if ui
                .add(
                    egui::TextEdit::multiline(recipe_json)
                        .desired_rows(12)
                        .desired_width(f32::INFINITY)
                        .code_editor(),
                )
                .changed()
            {
                *dirty = true;
            }
        }
    }
}
