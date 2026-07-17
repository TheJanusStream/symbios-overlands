//! Shared egui widgets and helpers used across every editor tab: fixed-point
//! slider, u32/u64 drag, RGB/RGBA colour pickers, generator-kind combo, the
//! transform editor, unique-key helpers, and the ternary-tree L-system
//! preset factory.

use bevy_egui::egui;

use crate::pds::{
    Fp, Fp3, Fp4, GeneratorKind, SovereignAssetReference, SovereignGeneratorKind, TransformData,
};

/// Quaternion → yaw/pitch/roll in degrees (`EulerRot::YXZ`: yaw about Y,
/// then pitch about X, then roll about Z — the convention the BlobGroup
/// element editor established). Pure for round-trip tests.
pub(super) fn quat_to_ypr_degrees(q: [f32; 4]) -> [f32; 3] {
    let (yaw, pitch, roll) = bevy::math::Quat::from_array(q).to_euler(bevy::math::EulerRot::YXZ);
    [yaw.to_degrees(), pitch.to_degrees(), roll.to_degrees()]
}

/// Yaw/pitch/roll in degrees → quaternion. Inverse of
/// [`quat_to_ypr_degrees`] away from the ±90° pitch fold.
pub(super) fn ypr_degrees_to_quat(ypr: [f32; 3]) -> [f32; 4] {
    bevy::math::Quat::from_euler(
        bevy::math::EulerRot::YXZ,
        ypr[0].to_radians(),
        ypr[1].to_radians(),
        ypr[2].to_radians(),
    )
    .to_array()
}

/// Rotation editor row (#826): yaw/pitch/roll DEGREE drags backed by the
/// record's quaternion — "rotate 45° around Y" is typed as `Yaw 45`
/// instead of hand-computing quaternion components. Stateless
/// quat→euler→quat per edit, the same pattern the BlobGroup element
/// editor proved out; the RECORD keeps the quaternion (no schema
/// change), and gizmo commits still write quats directly — this row
/// re-derives its angles from whatever the quat currently is. Near the
/// ±90° pitch fold the displayed yaw/roll pair can re-canonicalise
/// (Euler ambiguity); the underlying rotation stays exact.
pub(super) fn euler_rotation_row(
    ui: &mut egui::Ui,
    label: &str,
    rotation: &mut Fp4,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut ypr = quat_to_ypr_degrees(rotation.0);
        let mut changed = false;
        for (angle, name) in ypr.iter_mut().zip(["Yaw", "Pitch", "Roll"]) {
            changed |= ui
                .add(
                    egui::DragValue::new(angle)
                        .speed(1.0)
                        .range(-180.0..=180.0)
                        .suffix("°"),
                )
                .on_hover_text(name)
                .changed();
        }
        if changed {
            *rotation = Fp4(ypr_degrees_to_quat(ypr));
            *dirty = true;
        }
    });
}

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

    ui.label("Rotation (yaw / pitch / roll)");
    euler_rotation_row(ui, "", &mut t.rotation, dirty);
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

    ui.label("Rotation (yaw / pitch / roll)");
    euler_rotation_row(ui, "", &mut t.rotation, dirty);

    ui.label(
        egui::RichText::new(format!(
            "Scale: {:.2} x {:.2} x {:.2} (Configure scale in Generator)",
            t.scale.0[0], t.scale.0[1], t.scale.0[2]
        ))
        .small()
        .color(crate::ui::theme::current(ui.ctx()).text_weak),
    );
}

/// Compile-status line for a grammar forge (#829): the latest outcome of
/// this generator's L-system / Shape compile, from
/// [`crate::world_builder::grammar_diag::GrammarDiagnostics`]. Errors
/// render red with the parser's line-numbered message; success renders a
/// quiet tick so silence is distinguishable from "compiled fine".
/// `None` = not compiled yet this session (freshly loaded editor).
pub(super) fn grammar_status_line(
    ui: &mut egui::Ui,
    status: Option<&crate::world_builder::grammar_diag::GrammarStatus>,
) {
    use crate::world_builder::grammar_diag::GrammarStatus;
    match status {
        Some(GrammarStatus::Error { message }) => {
            ui.colored_label(
                crate::ui::theme::current(ui.ctx()).status.error,
                egui::RichText::new(format!("✗ {message}")).small(),
            );
        }
        Some(GrammarStatus::Ok) => {
            ui.label(
                egui::RichText::new(format!(
                    "{} grammar compiled",
                    crate::ui::affordances::CHECK
                ))
                .small()
                .color(crate::ui::theme::current(ui.ctx()).status.ok),
            );
        }
        None => {}
    }
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
                    .color(crate::ui::theme::current(ui.ctx()).text_weak),
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
    crate::catalogue::items::plants::lsys_ternary_props::TernaryPropsTree
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
    crate::catalogue::items::ancient::villa::Villa
        .build("")
        .kind
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quat_close(a: [f32; 4], b: [f32; 4]) -> bool {
        let qa = bevy::math::Quat::from_array(a);
        let qb = bevy::math::Quat::from_array(b);
        qa.angle_between(qb) < 1e-4
    }

    #[test]
    fn typing_yaw_45_is_a_pure_y_rotation() {
        let q = ypr_degrees_to_quat([45.0, 0.0, 0.0]);
        let expected = bevy::math::Quat::from_rotation_y(45f32.to_radians()).to_array();
        assert!(quat_close(q, expected), "{q:?} vs {expected:?}");
    }

    #[test]
    fn euler_round_trip_is_stable_for_composite_rotations() {
        // A rotation touching all three axes (pitch well below the ±90°
        // fold): quat → degrees → quat must return the same rotation, and
        // a second pass must return the same DISPLAYED angles — the
        // stateless per-frame re-derivation the row relies on.
        let original = bevy::math::Quat::from_euler(
            bevy::math::EulerRot::YXZ,
            35f32.to_radians(),
            -20f32.to_radians(),
            110f32.to_radians(),
        )
        .to_array();
        let ypr = quat_to_ypr_degrees(original);
        let back = ypr_degrees_to_quat(ypr);
        assert!(quat_close(original, back));
        let ypr2 = quat_to_ypr_degrees(back);
        for (a, b) in ypr.iter().zip(ypr2.iter()) {
            assert!((a - b).abs() < 1e-2, "{ypr:?} vs {ypr2:?}");
        }
    }

    #[test]
    fn identity_quat_reads_as_all_zero_degrees() {
        let ypr = quat_to_ypr_degrees([0.0, 0.0, 0.0, 1.0]);
        for a in ypr {
            assert!(a.abs() < 1e-4);
        }
    }
}
