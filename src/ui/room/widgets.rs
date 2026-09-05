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
pub(crate) fn euler_rotation_row(
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
                egui::RichText::new(format!("{} {message}", crate::ui::affordances::CROSS)).small(),
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

/// Returns the widget's [`egui::Response`] so a caller can hang an
/// `on_hover_text` on it (#1233 f264).
///
/// A return value rather than a `hover: &str` parameter: this and the two
/// below have 178 call sites between them, and threading an argument
/// through every one of them to explain nine sliders in the terrain forge
/// would be a worse trade than letting the callers that have something to
/// say say it. `egui::Response` is not `#[must_use]`, so the sites that do
/// not care are unchanged.
pub(super) fn fp_slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Fp,
    lo: f32,
    hi: f32,
    dirty: &mut bool,
) -> egui::Response {
    let mut v = value.0;
    let response = ui.add(egui::Slider::new(&mut v, lo..=hi).text(label));
    if response.changed() {
        *value = Fp(v);
        *dirty = true;
    }
    response
}

/// A min/max PAIR of [`fp_slider`]s that cannot be inverted (#1238 f90).
///
/// Four such pairs shipped as two independent sliders with no
/// cross-validation, and what happened downstream to an inverted pair was
/// neither uniform nor universal: the particle sanitiser clamps the max UP
/// to the min (losing the typed max), the road-lot sanitiser SWAPS the
/// pair (preserving both), and the splat rules have no sanitiser at all,
/// so an inverted rule is never corrected and never flagged. Where a
/// repair does happen it lands about a quarter-second later, in the panel
/// the user is looking at, unexplained.
///
/// Clamping at the widget removes the question: the max slider starts at
/// the current min and the min slider stops at the current max, so an
/// inverted pair cannot be entered and no sanitiser has to guess what was
/// meant.
/// [`fp_slider`] on a logarithmic track (#1254 f317).
///
/// For the ranges the record permits and a person almost never wants: a
/// 0..600 s decal TTL or a 0..1000 m/s trigger speed on a linear track puts
/// every useful value in the first two pixels. The wide bound has to be
/// REACHABLE — a legal value the GUI cannot author sends the owner to the
/// Raw JSON tab — and the low end has to stay tunable.
pub(super) fn fp_slider_log(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Fp,
    lo: f32,
    hi: f32,
    dirty: &mut bool,
) -> egui::Response {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut v = value.0;
        let response = ui.add(
            egui::Slider::new(&mut v, lo..=hi)
                .logarithmic(true)
                .smallest_positive(0.01),
        );
        if response.changed() {
            *value = Fp(v);
            *dirty = true;
        }
        response
    })
    .inner
}

#[allow(clippy::too_many_arguments)]
pub(super) fn fp_range_sliders(
    ui: &mut egui::Ui,
    label_min: &str,
    label_max: &str,
    min: &mut Fp,
    max: &mut Fp,
    lo: f32,
    hi: f32,
    dirty: &mut bool,
) {
    // Read the partner BEFORE either drag: reading it after would let a
    // drag on one slider widen its own bound within the same frame.
    let (current_min, current_max) = (min.0, max.0);
    fp_slider(ui, label_min, min, lo, current_max.clamp(lo, hi), dirty);
    fp_slider(ui, label_max, max, current_min.clamp(lo, hi), hi, dirty);
}

/// The drag's own response, for the same reason [`fp_slider`] returns one
/// — not the row's, so a tooltip lands on the control rather than the
/// whole line.
pub(super) fn drag_u32(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut u32,
    lo: u32,
    hi: u32,
    dirty: &mut bool,
) -> egui::Response {
    ui.horizontal(|ui| {
        ui.label(label);
        let response = ui.add(egui::DragValue::new(value).range(lo..=hi));
        if response.changed() {
            *dirty = true;
        }
        response
    })
    .inner
}

pub(super) fn drag_u64(ui: &mut egui::Ui, label: &str, value: &mut u64, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.add(egui::DragValue::new(value)).changed() {
            *dirty = true;
        }
    });
}

/// A colour swatch that shows the colour the world will actually render
/// (#1249 f58).
///
/// **The bug this closes.** `Ui::color_edit_button_rgb` is documented as
/// taking *linear* RGB, and the record's numbers are read by the world as
/// sRGB — `Color::srgb(sun[0], …)` for the sun, sky, cloud, fog, extinction,
/// inscattering, sun glow and water crest. So the same triple meant two
/// different colours at the two ends: a picked mid-grey showed as 0.5 in the
/// swatch, stored as 0.21, and lit the world as sRGB 0.21. The Environment
/// tab's entire job is art direction, and its primary instrument was
/// systematically darker and more saturated than what it showed.
///
/// The conversion lives HERE, at the widget boundary, and nowhere else. What
/// changes is what the swatch shows; every stored value keeps exactly the
/// meaning it already had, so no record is migrated and no world re-lights
/// itself on upgrade.
pub(super) fn color_picker(ui: &mut egui::Ui, label: &str, value: &mut Fp3, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        if edit_srgb_rgb(ui, &mut value.0) {
            *dirty = true;
        }
    });
}

/// The stored sRGB triple, edited through egui's linear-space picker.
/// Returns whether it changed. Shared by [`color_picker`], the RGBA picker
/// and the road-appearance override rows, which is the whole set — a
/// twenty-fifth picker that called egui directly would be a twenty-fifth
/// swatch telling a different story.
pub(super) fn edit_srgb_rgb(ui: &mut egui::Ui, value: &mut [f32; 3]) -> bool {
    let mut linear = value.map(egui::ecolor::linear_from_gamma);
    if ui.color_edit_button_rgb(&mut linear).changed() {
        *value = linear.map(egui::ecolor::gamma_from_linear);
        return true;
    }
    false
}

/// RGBA colour picker — mirrors [`color_picker`] but for [`Fp4`] fields
/// where the alpha channel carries renderer-relevant information (fog
/// opacity, sun-glow strength). Uses the unmultiplied variant so the
/// alpha edits independently of RGB rather than being pre-scaled.
pub(super) fn color_picker_rgba(ui: &mut egui::Ui, label: &str, value: &mut Fp4, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        // Same conversion as [`color_picker`], on the three colour channels
        // only: alpha is a coverage fraction in both spaces and gamma is
        // not applied to it anywhere in the renderer.
        let mut rgba = [
            egui::ecolor::linear_from_gamma(value.0[0]),
            egui::ecolor::linear_from_gamma(value.0[1]),
            egui::ecolor::linear_from_gamma(value.0[2]),
            value.0[3],
        ];
        if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
            *value = Fp4([
                egui::ecolor::gamma_from_linear(rgba[0]),
                egui::ecolor::gamma_from_linear(rgba[1]),
                egui::ecolor::gamma_from_linear(rgba[2]),
                rgba[3],
            ]);
            *dirty = true;
        }
    });
}

/// Terrain-algorithm picker, driven by the generator roster rather than a
/// hand-written option list: a fourth algorithm added to
/// `gen_jobs::for_each_heightmap_generator!` appears here with no edit.
///
/// `Unknown` is an algorithm from a newer engine (#1119). It is named, not
/// hidden — but it is absent from `SELECTABLE`, because picking a real
/// algorithm is how the owner *deliberately* replaces it, and until they do
/// the save stays refused rather than silently downgrading their choice.
pub(super) fn kind_combo(ui: &mut egui::Ui, kind: &mut SovereignGeneratorKind) -> bool {
    let mut changed = false;
    egui::ComboBox::from_label("Kind")
        .selected_text(kind.label())
        .show_ui(ui, |ui| {
            for &option in SovereignGeneratorKind::SELECTABLE {
                changed |= ui.selectable_value(kind, option, option.label()).changed();
            }
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
    // An empty reference SAYS it is empty (#1239 f71). Records written
    // before `+ Absolute` learnt to refuse still carry rows whose target
    // is the empty string, and an empty selected text beside an empty
    // option list reads as a rendering fault rather than as the thing to
    // fix.
    let selected = if value.is_empty() {
        String::from("(none — pick one)")
    } else {
        value.clone()
    };
    egui::ComboBox::from_label(label)
        .selected_text(selected)
        .show_ui(ui, |ui| {
            if names.is_empty() {
                ui.label(
                    egui::RichText::new("No region assets in this world yet")
                        .small()
                        .color(crate::ui::theme::current(ui.ctx()).text_weak),
                );
            }
            for n in names {
                if ui.selectable_value(value, n.clone(), n).changed() {
                    *dirty = true;
                }
            }
        });
}

/// The one sentence a forward-compat value gets (#1251).
///
/// An open union keeps a newer client's data safe on the wire and becomes a
/// LIE at the UI layer unless the panel names it: an `Unknown` texture is an
/// empty body under one word, an unrecognised lot theme reads as the active
/// selection while the injector ignores it, and picking anything in either
/// replaces content a newer client could still render.
///
/// There were four hand-written spellings of this by the time #1251 was
/// filed — the asset-reference editor's, the Sign source picker's, #1119's
/// terrain-algorithm line and the one this collapses them into. Every new
/// forward-compat arm was a fifth waiting to be written slightly
/// differently.
///
/// `noun` names the thing ("texture", "terrain algorithm"); `meanwhile` says
/// what the world is doing instead, for the values that are IGNORED rather
/// than merely unrenderable. Warn-coloured, because the value on screen is
/// not the value in effect.
pub(super) fn unrecognised_value_line(ui: &mut egui::Ui, noun: &str, meanwhile: Option<&str>) {
    let mut text = format!("This {noun} was authored by a newer version of Overlands");
    match meanwhile {
        Some(meanwhile) => text.push_str(&format!(" — {meanwhile}.")),
        None => text.push('.'),
    }
    text.push_str(" Picking one above replaces it.");
    ui.label(
        egui::RichText::new(text)
            .small()
            .color(crate::ui::theme::current(ui.ctx()).status.warn),
    );
}

/// The weak line under an asset field naming what this client can load
/// (#1248 f350). Every one of these caps was enforced firmly and stated
/// nowhere, so each was discovered as an unexplained blank.
pub(super) fn caps_line(ui: &mut egui::Ui, caps: &str) {
    ui.label(
        egui::RichText::new(caps)
            .small()
            .color(crate::ui::theme::current(ui.ctx()).text_weak),
    );
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
    // Which cache answers "did this arrive?" for this slot (#1246). The
    // same three variants are fetched by two different resolvers, and a
    // texture field asking the audio cache would report "nothing here" over
    // a source that really did fail.
    class: ReferenceClass,
    assets: &mut super::assets::AssetPanel<'_>,
) {
    // A profile picture is not a sound (#1251 f344). The reference type's own
    // doc says so — "the audio bridge UI should hide this variant from its
    // sub-picker" — and `AudioReferenceKey::from_reference` returns `None`
    // for it, so `request_blob_audio` took its early-return no-op path: no
    // task, no warning, nothing ever played. A control guaranteed to do
    // nothing, with no disabled state and no tooltip.
    let allow_did_pfp = class == ReferenceClass::Texture;
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
                if matches!(preset, SovereignAssetReference::DidPfp { .. }) && !allow_did_pfp {
                    continue;
                }
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
            // The same deferred-commit row the Sign source uses (#1248
            // f340): this reference goes through `is_fetchable_reference`
            // by delegation, so a refused URL was blanked here too.
            ui.horizontal(|ui| {
                ui.label("URL");
                let out = text_draft_row(
                    ui,
                    (salt, "ref_url"),
                    url,
                    240.0,
                    "The address this slot loads from. Press Enter, or click \
                     away, to apply it.",
                    crate::pds::sanitize::refusal_reason,
                );
                if let Some(committed) = out.committed {
                    *url = committed;
                    *dirty = true;
                }
            });
            caps_line(
                ui,
                &match class {
                    ReferenceClass::Texture => {
                        crate::world_builder::asset_failure::image_source_caps()
                    }
                    ReferenceClass::Audio => crate::world_builder::asset_failure::audio_clip_caps(),
                },
            );
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
            // Hiding the option does not remove it from records that already
            // carry it, so the one slot that can still be holding it says so
            // rather than staying silently mute.
            if !allow_did_pfp {
                ui.label(
                    egui::RichText::new(
                        "A profile picture is an image — this slot plays sound, \
                         so nothing will be loaded. Pick another source above.",
                    )
                    .small()
                    .color(crate::ui::theme::current(ui.ctx()).status.warn),
                );
            }
        }
        SovereignAssetReference::Unknown => {
            unrecognised_value_line(ui, "source", None);
        }
    }

    // What happened to it. Silence here was the whole of #1246: a
    // Referenced texture that 404'd rendered the procedural fallback and a
    // Referenced sound rendered nothing, and both looked exactly like a
    // slot that had not finished loading.
    let (status, retry) = match class {
        ReferenceClass::Texture => (
            assets.texture_reference(value),
            super::assets::AssetPanel::texture_retry(value),
        ),
        ReferenceClass::Audio => (
            assets.audio_reference(value),
            super::assets::AssetPanel::audio_retry(value),
        ),
    };
    if super::assets::asset_status_row(ui, status, assets.now)
        && let Some(retry) = retry
    {
        assets.retry(retry);
    }
}

/// Which resolver fetches a [`SovereignAssetReference`] in this slot.
///
/// The reference type is shared by the texture bridge and the audio bridge,
/// and the two are fetched into two different caches — the image cache keys
/// by source AND sampler filter, the audio cache by source alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReferenceClass {
    Texture,
    Audio,
}

pub(crate) fn unique_key<T>(map: &std::collections::HashMap<String, T>, prefix: &str) -> String {
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

// ---------------------------------------------------------------------------
// Deferred-commit text rows (#1238 f77 / f80 / f85)
// ---------------------------------------------------------------------------

/// A text field whose draft lives in egui temp memory until the user is
/// finished with it.
///
/// Three of this tranche's findings are ONE bug written three times: a
/// field whose buffer is regenerated from the record on every frame and
/// written back on every `.changed()`. Whatever the user types that the
/// record cannot represent is erased on the next frame, before they can
/// finish typing it:
///
/// * the Shape forge's comma-separated "Turned terminals" list filtered
///   out the empty entry a trailing comma produces, so the comma vanished
///   as it was typed and the multi-id feature was unreachable except by
///   paste;
/// * the particle Seed could not be CLEARED and retyped, because an empty
///   field does not parse and the old number came straight back;
/// * a Shape material rename committed per keystroke — flashing the
///   building grey through every intermediate name, silently reverting an
///   empty or colliding draft, and re-sorting the row out from under the
///   cursor mid-word.
///
/// The road editor's "Layout seed" row already had the answer (#885). This
/// is that pattern promoted so the three surfaces cannot drift apart:
/// hold the draft, re-sync it when the record changes underneath (undo,
/// dice, a remote edit), tint it while it is not committable, and commit
/// on `lost_focus()`.
#[derive(Clone)]
pub(super) struct TextDraft {
    text: String,
    /// The record value the draft was last synced from. A change here
    /// means something OTHER than typing moved the record, so the draft is
    /// stale and must be replaced rather than fought.
    synced_to: String,
}

/// What a [`text_draft_row`] did this frame.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(super) struct DraftOutcome {
    /// The committed text, present exactly on the frame focus was lost
    /// with a value that differs from the record's.
    pub committed: Option<String>,
    /// The live draft, for a caller that wants to validate as it is typed.
    pub draft: String,
}

/// Draw a deferred-commit text field. `current` is the record's value;
/// `refusal` is asked about the live draft each frame and, when it answers,
/// tints the field, shows the reason inline, and blocks the commit.
///
/// `id_salt` must be unique per edited value — a shared salt would let two
/// rows fight over one draft.
pub(super) fn text_draft_row(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    current: &str,
    width: f32,
    hover: &str,
    refusal: impl Fn(&str) -> Option<String>,
) -> DraftOutcome {
    let id = ui.id().with(id_salt);
    let mut state = ui
        .data_mut(|d| d.get_temp::<TextDraft>(id))
        .unwrap_or_else(|| TextDraft {
            text: current.to_string(),
            synced_to: current.to_string(),
        });
    if state.synced_to != current {
        state.text = current.to_string();
        state.synced_to = current.to_string();
    }

    let refused = refusal(&state.text);
    let mut field = egui::TextEdit::singleline(&mut state.text).desired_width(width);
    if refused.is_some() {
        field = field.text_color(crate::ui::theme::current(ui.ctx()).status.error);
    }
    let response = ui.add(field).on_hover_text(hover);
    if let Some(reason) = &refused {
        ui.label(
            egui::RichText::new(reason.clone())
                .small()
                .color(crate::ui::theme::current(ui.ctx()).status.warn),
        );
    }

    let mut outcome = DraftOutcome {
        committed: None,
        draft: state.text.clone(),
    };
    if response.lost_focus() && refused.is_none() && state.text != state.synced_to {
        outcome.committed = Some(state.text.clone());
        state.synced_to = state.text.clone();
    }
    ui.data_mut(|d| d.insert_temp(id, state));
    outcome
}

#[cfg(test)]
mod colour_space_tests {
    use bevy_egui::egui;

    /// #1249 f58, as arithmetic. The picker is a linear-space widget and
    /// the world reads the same numbers as sRGB, so the swatch showed a
    /// mid-grey where the world would render 0.21 — noticeably darker and
    /// more saturated than what was picked, on every colour on the
    /// Environment tab.
    #[test]
    fn a_mid_grey_swatch_is_a_mid_grey_number() {
        // What the widget is handed for a stored mid-grey.
        let linear = egui::ecolor::linear_from_gamma(0.5);
        assert!(
            linear < 0.25,
            "sRGB 0.5 is about 0.21 linear — if this is 0.5 the two spaces \
             have stopped differing and the whole conversion is moot"
        );
        // And the round trip is what keeps every existing record meaning
        // exactly what it meant before: the conversion changes the swatch,
        // never the stored value.
        for stored in [0.0f32, 0.04, 0.18, 0.5, 0.73, 1.0] {
            let round_tripped =
                egui::ecolor::gamma_from_linear(egui::ecolor::linear_from_gamma(stored));
            assert!(
                (round_tripped - stored).abs() < 1e-4,
                "{stored} came back as {round_tripped}"
            );
        }
    }

    /// Every colour picker in the editor goes through the shared helper —
    /// a twenty-fifth that called egui directly would be a twenty-fifth
    /// swatch telling a different story, which is exactly what
    /// `detail.rs`'s road-appearance row was.
    #[test]
    fn no_editor_surface_calls_the_linear_colour_widget_directly() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui");
        let mut offenders = Vec::new();
        let mut walk = vec![root];
        while let Some(dir) = walk.pop() {
            for entry in std::fs::read_dir(&dir).expect("src/ui is readable") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    walk.push(path);
                    continue;
                }
                if path.extension().is_none_or(|e| e != "rs") {
                    continue;
                }
                // The helper itself is the one legitimate caller.
                if path.ends_with("ui/room/widgets.rs") {
                    continue;
                }
                let src = std::fs::read_to_string(&path).expect("readable");
                if src.contains("color_edit_button_rgb") {
                    offenders.push(path.display().to_string());
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "these files edit a stored sRGB colour through egui's LINEAR widget: \
             {offenders:?} — route them through `widgets::edit_srgb_rgb`"
        );
    }
}

#[cfg(test)]
mod forward_compat_tests {
    /// #1251: four hand-written spellings of "this came from a newer build"
    /// existed across the room editor — the asset-reference editor's, the
    /// Sign source picker's, #1119's terrain-algorithm line, and whatever
    /// the next forward-compat arm was going to invent. They are one helper
    /// now, and this is what keeps a fifth from being written beside it.
    #[test]
    fn only_one_place_says_authored_by_a_newer_version() {
        // The room editor's own surface. `ui::inventory`'s
        // `UNREADABLE_ITEM_TAG` says something adjacent and deliberately
        // different (#1207): an item this build cannot decode has nothing
        // to "pick above", and its sentence has to name the stash it is
        // blocking instead.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui/room");
        let mut offenders = Vec::new();
        let mut walk = vec![root];
        while let Some(dir) = walk.pop() {
            for entry in std::fs::read_dir(&dir).expect("src is readable") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    walk.push(path);
                    continue;
                }
                if path.extension().is_none_or(|e| e != "rs") {
                    continue;
                }
                let src = std::fs::read_to_string(&path).expect("readable");
                // The helper itself is allowed to say it; nobody else is.
                if path.ends_with("ui/room/widgets.rs") {
                    continue;
                }
                if src.contains("authored by a newer") || src.contains("from a newer version of") {
                    offenders.push(path.display().to_string());
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "these files spell the forward-compat sentence themselves instead of \
             calling `unrecognised_value_line`: {offenders:?}"
        );
    }
}

#[cfg(test)]
mod draft_tests {
    use super::*;

    /// #1238 f77. Sequence: type "Column, Silo" into Turned terminals. The
    /// buffer was regenerated from the record every frame and re-parsed on
    /// every change, so the trailing comma produced an empty entry, the
    /// empty was filtered, and the comma was gone before the second name
    /// could be started. Only a paste of the whole string worked.
    #[test]
    fn a_draft_survives_a_state_the_record_cannot_hold() {
        let ctx = egui::Context::default();
        let typed = ["Column", "Column,", "Column, ", "Column, Silo"];
        let mut seen = Vec::new();
        for (frame, text) in typed.iter().enumerate() {
            let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
                // The record still says "Column" the whole way through —
                // nothing is committed until focus is lost.
                let out = text_draft_row(ui, "terminals", "Column", 100.0, "", |_| None);
                seen.push(out.draft.clone());
                // Simulate the keystroke by writing the draft back, which
                // is what the TextEdit itself does.
                let id = ui.id().with("terminals");
                ui.data_mut(|d| {
                    d.insert_temp(
                        id,
                        TextDraft {
                            text: (*text).to_string(),
                            synced_to: "Column".to_string(),
                        },
                    );
                });
            });
            assert!(frame < typed.len());
        }
        assert_eq!(
            seen.last().map(String::as_str),
            Some("Column, "),
            "the draft carried the trailing comma and space across frames"
        );
    }

    /// A record value that changes underneath the draft — undo, the dice
    /// button, a peer edit — replaces it rather than being fought.
    #[test]
    fn a_record_change_underneath_replaces_the_draft() {
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let out = text_draft_row(ui, "seed", "1234", 100.0, "", |_| None);
            assert_eq!(out.draft, "1234");
        });
        let mut after = String::new();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            after = text_draft_row(ui, "seed", "9999", 100.0, "", |_| None).draft;
        });
        assert_eq!(after, "9999");
    }

    /// A refused draft is SHOWN as refused, not silently reverted — which
    /// was indistinguishable from a dead widget (#1238 f80).
    #[test]
    fn a_refused_draft_is_kept_and_explained() {
        let ctx = egui::Context::default();
        let mut refused_text = String::new();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let id = ui.id().with("slot");
            ui.data_mut(|d| {
                d.insert_temp(
                    id,
                    TextDraft {
                        text: String::new(),
                        synced_to: "Slot0".to_string(),
                    },
                );
            });
            let out = text_draft_row(ui, "slot", "Slot0", 100.0, "", |draft| {
                crate::ui::confirm::validate_new_key(draft, "Slot0", |_| false).err()
            });
            refused_text = out.draft;
            assert!(out.committed.is_none(), "a refused draft never commits");
        });
        assert_eq!(refused_text, "", "the draft is kept so it can be corrected");
    }
}

#[cfg(test)]
mod range_tests {

    /// #1238 f90. Sequence: set particle lifetime min 20, max 2. All four
    /// paired sliders in the editor were independent, and what happened to
    /// an inverted pair afterwards was three different things: particles
    /// clamp the max UP to the min (losing the typed max), road lots SWAP
    /// (preserving both), and splat rules — which have no `Sanitize` impl
    /// at all — are never corrected and never flagged. Clamping at the
    /// widget means no sanitiser has to guess.
    ///
    /// The bounds are computed here exactly as `fp_range_sliders` computes
    /// them, because what is being pinned is the ARITHMETIC: reading the
    /// partner before either drag, and clamping each partner into the
    /// pair's own outer range so a record already inverted by a hand edit
    /// still yields a usable slider.
    #[test]
    fn neither_half_of_a_pair_can_cross_the_other() {
        let (lo, hi) = (0.01_f32, 30.0_f32);
        for (min, max) in [(1.0_f32, 5.0_f32), (5.0, 5.0), (20.0, 2.0)] {
            let min_hi = max.clamp(lo, hi);
            let max_lo = min.clamp(lo, hi);
            assert!(min_hi <= hi && min_hi >= lo, "min slider's top is in range");
            assert!(
                max_lo >= lo && max_lo <= hi,
                "max slider's bottom is in range"
            );
            // Dragging the min slider to its own top lands exactly on the
            // max — the pair can meet but never cross.
            assert!(min_hi <= max.max(lo));
            // …and the same from the other side.
            assert!(max_lo >= min.min(hi));
        }
    }
}
