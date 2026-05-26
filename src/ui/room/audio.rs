//! Audio bridge widget — the sovereign-side mirror of
//! [`super::material::draw_texture_bridge`].
//!
//! Renders a [`SovereignAudioConfig`] picker plus the per-variant body
//! editor: shared asset-reference editor for the `Referenced` variant,
//! and a read-only JSON preview for the procedural `Patch` /
//! `Sequence` variants until a structured node-graph editor lands as
//! a follow-up. Variant presets are still selectable from the
//! dropdown, so a room author can drop in a default ambient bed and
//! tune it via the catalogue / runtime mutation API for now.

use bevy_egui::egui;

use crate::pds::SovereignAudioConfig;
use crate::pds::asset_reference::SovereignAssetReference;
use crate::pds::audio::{SovereignAudioPatch, SovereignSequenceRecipe};

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

    match audio {
        SovereignAudioConfig::None | SovereignAudioConfig::Unknown => {}
        SovereignAudioConfig::Referenced { source } => {
            super::widgets::draw_asset_reference_editor(ui, source, salt, dirty);
        }
        SovereignAudioConfig::Patch { patch } => {
            draw_structured_preview(ui, "AudioPatch", patch);
        }
        SovereignAudioConfig::Sequence { recipe } => {
            draw_structured_preview(ui, "SequenceRecipe", recipe);
        }
    }
}

/// Read-only structured-JSON preview of a Sovereign* value. Until the
/// structured node-graph editor lands, this lets the room owner
/// inspect what's slotted (e.g. confirm the seeded ambient was
/// authored as expected) without exposing a JSON-paste path that
/// would round-trip lossy against the Fp-quantised wire format.
fn draw_structured_preview<T: serde::Serialize>(ui: &mut egui::Ui, label: &str, value: &T) {
    ui.label(
        egui::RichText::new(format!(
            "{label} (structured, Fp-encoded). Structured node-graph editor coming in a \
             follow-up; today the catalogue and runtime mutation APIs are the authoring paths."
        ))
        .small()
        .color(egui::Color32::GRAY),
    );
    let json = serde_json::to_string_pretty(value)
        .unwrap_or_else(|e| format!("// failed to serialise: {e}"));
    // Hold the rendered text in a local copy because egui's
    // TextEdit::multiline wants &mut str even for read-only display;
    // marking the widget non-interactive turns the keyboard input
    // off without preventing rendering.
    let mut display = json;
    ui.add_enabled(
        false,
        egui::TextEdit::multiline(&mut display)
            .desired_rows(10)
            .desired_width(f32::INFINITY)
            .code_editor(),
    );
}
