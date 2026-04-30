//! Sign-generator detail panel: source picker, panel size, UV repeat /
//! offset, the `StandardMaterial` toggles, and the alpha-mode picker.
//! [`draw_sign_source`] is also reused by the particle texture editor.

use bevy_egui::egui;

use crate::pds::{AlphaModeKind, Fp, Fp2, SignSource, SovereignMaterialSettings};

use super::super::widgets::fp_slider;

/// Editor for the [`crate::pds::GeneratorKind::Sign`] panel: source picker,
/// panel size, UV repeat / offset, the StandardMaterial toggles
/// (double_sided / unlit / alpha_mode), and the shared material PBR
/// section.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_generator_sign(
    ui: &mut egui::Ui,
    source: &mut SignSource,
    size: &mut Fp2,
    uv_repeat: &mut Fp2,
    uv_offset: &mut Fp2,
    material: &mut SovereignMaterialSettings,
    double_sided: &mut bool,
    alpha_mode: &mut AlphaModeKind,
    unlit: &mut bool,
    salt: &str,
    dirty: &mut bool,
) {
    draw_sign_source(ui, source, salt, dirty);
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label("Panel size X/Z:");
        let mut v = size.0;
        let mut changed = false;
        for axis in v.iter_mut() {
            changed |= ui
                .add(egui::DragValue::new(axis).speed(0.1).range(0.01..=100.0))
                .changed();
        }
        if changed {
            *size = Fp2(v);
            *dirty = true;
        }
    });

    ui.horizontal(|ui| {
        ui.label("UV repeat U/V:");
        let mut v = uv_repeat.0;
        let mut changed = false;
        for axis in v.iter_mut() {
            changed |= ui
                .add(egui::DragValue::new(axis).speed(0.05).range(0.001..=1000.0))
                .changed();
        }
        if changed {
            *uv_repeat = Fp2(v);
            *dirty = true;
        }
    });

    ui.horizontal(|ui| {
        ui.label("UV offset U/V:");
        let mut v = uv_offset.0;
        let mut changed = false;
        for axis in v.iter_mut() {
            changed |= ui
                .add(
                    egui::DragValue::new(axis)
                        .speed(0.05)
                        .range(-1000.0..=1000.0),
                )
                .changed();
        }
        if changed {
            *uv_offset = Fp2(v);
            *dirty = true;
        }
    });

    ui.add_space(4.0);
    if ui.checkbox(double_sided, "Double-sided").changed() {
        *dirty = true;
    }
    if ui.checkbox(unlit, "Unlit").changed() {
        *dirty = true;
    }

    draw_alpha_mode(ui, alpha_mode, salt, dirty);

    ui.add_space(2.0);
    egui::CollapsingHeader::new("Material")
        .id_salt(format!("{}_sign_mat", salt))
        .default_open(false)
        .show(ui, |ui| {
            // Sign panels paint the loaded image into `base_color_texture`
            // and use the universal material's PBR knobs (tint /
            // emission / roughness / metallic) on top. The procedural
            // texture slot is intentionally hidden — the Sign's source
            // already supplies the texture.
            super::super::widgets::color_picker(ui, "Tint", &mut material.base_color, dirty);
            super::super::widgets::color_picker(
                ui,
                "Emission",
                &mut material.emission_color,
                dirty,
            );
            fp_slider(
                ui,
                "Emission strength",
                &mut material.emission_strength,
                0.0,
                20.0,
                dirty,
            );
            fp_slider(ui, "Roughness", &mut material.roughness, 0.0, 1.0, dirty);
            fp_slider(ui, "Metallic", &mut material.metallic, 0.0, 1.0, dirty);
        });
}

/// Source-variant picker for a Sign generator. Combo box selects the
/// variant (URL / atproto_blob / did_pfp); the per-variant payload
/// fields render below. Switching variants reseeds the payload from the
/// previous variant where possible (e.g. URL → did_pfp keeps the URL
/// in the URL field if the user switches back) — implemented by
/// only overwriting when the variant truly changes.
pub(super) fn draw_sign_source(
    ui: &mut egui::Ui,
    source: &mut SignSource,
    salt: &str,
    dirty: &mut bool,
) {
    let current = match source {
        SignSource::Url { .. } => "URL",
        SignSource::AtprotoBlob { .. } => "ATProto blob",
        SignSource::DidPfp { .. } => "DID profile picture",
        SignSource::Unknown => "Unknown",
    };

    ui.horizontal(|ui| {
        ui.label("Source:");
        egui::ComboBox::from_id_salt(format!("{}_sign_source", salt))
            .selected_text(current)
            .show_ui(ui, |ui| {
                if ui.selectable_label(current == "URL", "URL").clicked()
                    && !matches!(source, SignSource::Url { .. })
                {
                    *source = SignSource::Url { url: String::new() };
                    *dirty = true;
                }
                if ui
                    .selectable_label(current == "ATProto blob", "ATProto blob")
                    .clicked()
                    && !matches!(source, SignSource::AtprotoBlob { .. })
                {
                    *source = SignSource::AtprotoBlob {
                        did: String::new(),
                        cid: String::new(),
                    };
                    *dirty = true;
                }
                if ui
                    .selectable_label(current == "DID profile picture", "DID profile picture")
                    .clicked()
                    && !matches!(source, SignSource::DidPfp { .. })
                {
                    *source = SignSource::DidPfp { did: String::new() };
                    *dirty = true;
                }
            });
    });

    match source {
        SignSource::Url { url } => {
            ui.horizontal(|ui| {
                ui.label("URL:");
                if ui
                    .add(egui::TextEdit::singleline(url).hint_text("https://…"))
                    .changed()
                {
                    *dirty = true;
                }
            });
        }
        SignSource::AtprotoBlob { did, cid } => {
            ui.horizontal(|ui| {
                ui.label("DID:");
                if ui
                    .add(egui::TextEdit::singleline(did).hint_text("did:plc:…"))
                    .changed()
                {
                    *dirty = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("CID:");
                if ui
                    .add(egui::TextEdit::singleline(cid).hint_text("bafy…"))
                    .changed()
                {
                    *dirty = true;
                }
            });
        }
        SignSource::DidPfp { did } => {
            ui.horizontal(|ui| {
                ui.label("DID:");
                if ui
                    .add(egui::TextEdit::singleline(did).hint_text("did:plc:…"))
                    .changed()
                {
                    *dirty = true;
                }
            });
        }
        SignSource::Unknown => {
            ui.colored_label(
                egui::Color32::from_rgb(220, 160, 80),
                "Unknown source variant — pick one above to replace it.",
            );
        }
    }
}

/// Alpha-mode picker for a Sign generator. Combo selects the variant;
/// when `Mask` is selected, the cutoff slider renders below.
fn draw_alpha_mode(
    ui: &mut egui::Ui,
    alpha_mode: &mut AlphaModeKind,
    salt: &str,
    dirty: &mut bool,
) {
    let current = match alpha_mode {
        AlphaModeKind::Opaque => "Opaque",
        AlphaModeKind::Mask { .. } => "Mask",
        AlphaModeKind::Blend => "Blend",
        AlphaModeKind::Unknown => "Unknown",
    };

    ui.horizontal(|ui| {
        ui.label("Alpha mode:");
        egui::ComboBox::from_id_salt(format!("{}_alpha_mode", salt))
            .selected_text(current)
            .show_ui(ui, |ui| {
                if ui.selectable_label(current == "Opaque", "Opaque").clicked()
                    && !matches!(alpha_mode, AlphaModeKind::Opaque)
                {
                    *alpha_mode = AlphaModeKind::Opaque;
                    *dirty = true;
                }
                if ui.selectable_label(current == "Mask", "Mask").clicked()
                    && !matches!(alpha_mode, AlphaModeKind::Mask { .. })
                {
                    *alpha_mode = AlphaModeKind::Mask { cutoff: Fp(0.5) };
                    *dirty = true;
                }
                if ui.selectable_label(current == "Blend", "Blend").clicked()
                    && !matches!(alpha_mode, AlphaModeKind::Blend)
                {
                    *alpha_mode = AlphaModeKind::Blend;
                    *dirty = true;
                }
            });
    });

    if let AlphaModeKind::Mask { cutoff } = alpha_mode {
        fp_slider(ui, "Mask cutoff", cutoff, 0.0, 1.0, dirty);
    }
}
