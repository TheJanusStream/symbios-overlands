//! Shared egui widgets and helpers used across every editor tab: fixed-point
//! slider, u32/u64 drag, RGB/RGBA colour pickers, generator-kind combo, the
//! transform editor, unique-key helpers, and the ternary-tree L-system
//! preset factory.

use bevy_egui::egui;

use crate::pds::{
    Fp, Fp3, Fp4, GeneratorKind, SovereignAssetReference, SovereignGeneratorKind, TransformData,
};

pub(super) fn draw_transform(ui: &mut egui::Ui, t: &mut TransformData, dirty: &mut bool) {
    ui.label("Translation");
    let mut tr = t.translation.0;
    ui.horizontal(|ui| {
        for v in tr.iter_mut() {
            if ui.add(egui::DragValue::new(v).speed(0.5)).changed() {
                *dirty = true;
            }
        }
    });
    t.translation = Fp3(tr);

    ui.label("Scale");
    let mut sc = t.scale.0;
    ui.horizontal(|ui| {
        for v in sc.iter_mut() {
            if ui
                .add(egui::DragValue::new(v).speed(0.05).range(0.01..=1000.0))
                .changed()
            {
                *dirty = true;
            }
        }
    });
    t.scale = Fp3(sc);

    ui.label("Rotation (quaternion xyzw)");
    let mut rot = t.rotation.0;
    ui.horizontal(|ui| {
        for v in rot.iter_mut() {
            if ui.add(egui::DragValue::new(v).speed(0.01)).changed() {
                *dirty = true;
            }
        }
    });
    t.rotation = Fp4(rot);
}

pub(super) fn draw_transform_no_scale(ui: &mut egui::Ui, t: &mut TransformData, dirty: &mut bool) {
    ui.label("Translation");
    let mut tr = t.translation.0;
    ui.horizontal(|ui| {
        if ui
            .add(egui::DragValue::new(&mut tr[0]).speed(0.5))
            .changed()
        {
            *dirty = true;
        }
        if ui
            .add(egui::DragValue::new(&mut tr[1]).speed(0.5))
            .changed()
        {
            *dirty = true;
        }
        if ui
            .add(egui::DragValue::new(&mut tr[2]).speed(0.5))
            .changed()
        {
            *dirty = true;
        }
    });
    t.translation = Fp3(tr);

    ui.label("Rotation (quaternion xyzw)");
    let mut rot = t.rotation.0;
    ui.horizontal(|ui| {
        if ui
            .add(egui::DragValue::new(&mut rot[0]).speed(0.01))
            .changed()
        {
            *dirty = true;
        }
        if ui
            .add(egui::DragValue::new(&mut rot[1]).speed(0.01))
            .changed()
        {
            *dirty = true;
        }
        if ui
            .add(egui::DragValue::new(&mut rot[2]).speed(0.01))
            .changed()
        {
            *dirty = true;
        }
        if ui
            .add(egui::DragValue::new(&mut rot[3]).speed(0.01))
            .changed()
        {
            *dirty = true;
        }
    });
    t.rotation = Fp4(rot);

    ui.label(
        egui::RichText::new(format!(
            "Scale: {:.2} x {:.2} x {:.2} (Configure scale in Generator)",
            t.scale.0[0], t.scale.0[1], t.scale.0[2]
        ))
        .small()
        .color(egui::Color32::GRAY),
    );
}

pub(super) fn fp_slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Fp,
    lo: f32,
    hi: f32,
    dirty: &mut bool,
) {
    let mut v = value.0;
    if ui
        .add(egui::Slider::new(&mut v, lo..=hi).text(label))
        .changed()
    {
        *value = Fp(v);
        *dirty = true;
    }
}

pub(super) fn drag_u32(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut u32,
    lo: u32,
    hi: u32,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.add(egui::DragValue::new(value).range(lo..=hi)).changed() {
            *dirty = true;
        }
    });
}

pub(super) fn drag_u64(ui: &mut egui::Ui, label: &str, value: &mut u64, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.add(egui::DragValue::new(value)).changed() {
            *dirty = true;
        }
    });
}

pub(super) fn color_picker(ui: &mut egui::Ui, label: &str, value: &mut Fp3, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut rgb = value.0;
        if ui.color_edit_button_rgb(&mut rgb).changed() {
            *value = Fp3(rgb);
            *dirty = true;
        }
    });
}

/// RGBA colour picker — mirrors [`color_picker`] but for [`Fp4`] fields
/// where the alpha channel carries renderer-relevant information (fog
/// opacity, sun-glow strength). Uses the unmultiplied variant so the
/// alpha edits independently of RGB rather than being pre-scaled.
pub(super) fn color_picker_rgba(ui: &mut egui::Ui, label: &str, value: &mut Fp4, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut rgba = value.0;
        if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
            *value = Fp4(rgba);
            *dirty = true;
        }
    });
}

pub(super) fn kind_combo(ui: &mut egui::Ui, kind: &mut SovereignGeneratorKind) -> bool {
    let mut changed = false;
    egui::ComboBox::from_label("Kind")
        .selected_text(match kind {
            SovereignGeneratorKind::FbmNoise => "FBM Noise",
            SovereignGeneratorKind::DiamondSquare => "Diamond Square",
            SovereignGeneratorKind::VoronoiTerracing => "Voronoi Terracing",
        })
        .show_ui(ui, |ui| {
            changed |= ui
                .selectable_value(kind, SovereignGeneratorKind::FbmNoise, "FBM Noise")
                .changed();
            changed |= ui
                .selectable_value(
                    kind,
                    SovereignGeneratorKind::DiamondSquare,
                    "Diamond Square",
                )
                .changed();
            changed |= ui
                .selectable_value(
                    kind,
                    SovereignGeneratorKind::VoronoiTerracing,
                    "Voronoi Terracing",
                )
                .changed();
        });
    changed
}

pub(super) fn generator_combo(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    names: &[String],
    dirty: &mut bool,
) {
    egui::ComboBox::from_label(label)
        .selected_text(value.clone())
        .show_ui(ui, |ui| {
            for n in names {
                if ui.selectable_value(value, n.clone(), n).changed() {
                    *dirty = true;
                }
            }
        });
}

/// Sub-source picker + per-variant editor for a [`SovereignAssetReference`].
///
/// Shared by the texture-bridge dropdown (when "Referenced" is selected)
/// and the future audio-bridge dropdown, so the same UX shape (URL /
/// AtProto blob / DID profile picture) is presented for every asset class.
///
/// `salt` namespaces the inner combo box so multiple references on the
/// same egui frame (e.g. four terrain layers each pointing at a different
/// referenced texture) don't collide on the egui id stack.
pub(super) fn draw_asset_reference_editor(
    ui: &mut egui::Ui,
    value: &mut SovereignAssetReference,
    salt: &str,
    dirty: &mut bool,
) {
    egui::ComboBox::from_id_salt(format!("{}_ref_src", salt))
        .selected_text(value.label())
        .show_ui(ui, |ui| {
            // Each source preset starts with empty strings; the user fills
            // them in via the body editor below. Switching variants resets
            // the payload because the strings of one variant are not the
            // strings of another (a URL is not a DID, etc).
            let presets: [(&'static str, SovereignAssetReference); 3] = [
                ("URL", SovereignAssetReference::Url { url: String::new() }),
                (
                    "ATProto Blob (DID + CID)",
                    SovereignAssetReference::AtprotoBlob {
                        did: String::new(),
                        cid: String::new(),
                    },
                ),
                (
                    "DID Profile Picture",
                    SovereignAssetReference::DidPfp { did: String::new() },
                ),
            ];
            for (label, preset) in presets {
                // Variant-tag comparison: same discriminant → already selected.
                let selected = std::mem::discriminant(value) == std::mem::discriminant(&preset);
                if ui.selectable_label(selected, label).clicked() && !selected {
                    *value = preset;
                    *dirty = true;
                }
            }
        });

    match value {
        SovereignAssetReference::Url { url } => {
            ui.horizontal(|ui| {
                ui.label("URL");
                if ui.text_edit_singleline(url).changed() {
                    *dirty = true;
                }
            });
        }
        SovereignAssetReference::AtprotoBlob { did, cid } => {
            ui.horizontal(|ui| {
                ui.label("DID");
                if ui.text_edit_singleline(did).changed() {
                    *dirty = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("CID");
                if ui.text_edit_singleline(cid).changed() {
                    *dirty = true;
                }
            });
        }
        SovereignAssetReference::DidPfp { did } => {
            ui.horizontal(|ui| {
                ui.label("DID");
                if ui.text_edit_singleline(did).changed() {
                    *dirty = true;
                }
            });
        }
        SovereignAssetReference::Unknown => {
            ui.label(
                egui::RichText::new("Unrecognised source — authored by a newer client.")
                    .small()
                    .color(egui::Color32::GRAY),
            );
        }
    }
}

pub(super) fn unique_key<T>(map: &std::collections::HashMap<String, T>, prefix: &str) -> String {
    let mut n = 0;
    loop {
        let key = if n == 0 {
            prefix.to_string()
        } else {
            format!("{prefix}_{n}")
        };
        if !map.contains_key(&key) {
            return key;
        }
        n += 1;
    }
}

/// Default LSystem starter — delegates to the foliage-rich
/// "Ternary Tree (Foliage)" entry in the
/// [`crate::catalogue`]. Used by the per-node kind picker that swaps
/// an existing node's variant in place, and by the "+ New" menu in
/// the Generators tab via [`super::construct::make_default_for_kind`].
/// Picking a different LSystem preset is done from the "+ From
/// Catalogue" submenu (full list) or the Catalogue window
/// (drag-to-place).
pub(super) fn default_lsystem_kind() -> GeneratorKind {
    use crate::catalogue::CatalogueEntry;
    // Only the kind discriminant is consumed here; the local-DID
    // parameter is irrelevant for TernaryPropsTree (no DID slot).
    crate::catalogue::items::lsys_ternary_props::TernaryPropsTree
        .build("")
        .kind
}

/// Default starter preset for a freshly added Shape generator. A detailed
/// modern villa adapted from `bevy_symbios_shape`'s `detailed_villa` example —
/// a two-storey brick / stucco main house with a gable shingle roof, attached
/// metal-roofed garage, paver driveway, and wood deck. The full material
/// palette (brick / stucco / concrete / shingle / metal / glass / wood /
/// pavers / grass) is wired up so the fallback render shows something
/// architecturally legible out of the box. Used by the per-node kind picker
/// and by the "+ New" menu in the Generators tab via
/// [`super::construct::make_default_for_kind`].
pub(super) fn default_shape_kind() -> GeneratorKind {
    use crate::catalogue::CatalogueEntry;
    // Only the kind discriminant is consumed here; Villa has no DID
    // slot, so the local-DID parameter is irrelevant.
    crate::catalogue::items::villa::Villa.build("").kind
}
