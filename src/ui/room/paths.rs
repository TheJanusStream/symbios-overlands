//! The terrain panel's Paths section (#1779): the footways a seeded world
//! routed from its landing to its buildings, painted on its ground - the
//! layer that paints them, how much of it, how wide, or none at all - its
//! tracks (#1782), the wider ways of its airstrip and its car's track, each
//! its own width, and its fields (#1791), strips of crops in turn.

use bevy_egui::egui;

use super::widgets::fp_slider;
use crate::pds::SovereignPathsConfig;
use crate::pds::sanitize::limits;

/// The terrain's four ground layers, as the Material section names them.
const LAYERS: [&str; 4] = ["Grass (R)", "Dirt (G)", "Rock (B)", "Snow (A)"];

/// The Paths section: what `paths` holds, and its settings while it holds
/// any. A setting is written only when the owner moves it.
pub(super) fn draw_paths_section(
    ui: &mut egui::Ui,
    paths: &mut SovereignPathsConfig,
    dirty: &mut bool,
) {
    let weak = crate::ui::theme::current(ui.ctx()).text_weak;
    if paths.is_empty() {
        ui.label(
            egui::RichText::new(
                "This world has no paths. A world seeded on its own ground routes them \
                 from its landing to each of its buildings.",
            )
            .small()
            .color(weak),
        );
        draw_fields(ui, paths, dirty);
        return;
    }
    if !paths.lines.is_empty() {
        let length: f32 = paths.lines.iter().map(|line| run_of(line)).sum();
        ui.label(
            egui::RichText::new(format!(
                "{} paths, {length:.0} m in all, from the landing to the world's buildings: \
                 painted on the ground, where no grass, tree or rock grows.",
                paths.lines.len()
            ))
            .small()
            .color(weak),
        );
        ui.add_space(4.0);
    }

    let before = paths.layer;
    let mut layer = before;
    egui::ComboBox::from_label("Painted with")
        .selected_text(LAYERS[usize::from(layer.min(3))])
        .show_ui(ui, |ui| {
            for (k, name) in (0u8..).zip(LAYERS) {
                ui.selectable_value(&mut layer, k, name);
            }
        })
        .response
        .on_hover_text(
            "The ground layer the paths, the tracks and the fields are painted in: the \
             Material section's four.",
        );
    if layer != before {
        paths.layer = layer;
        *dirty = true;
    }
    // How much paint and how wide: the paths' and the tracks', not a
    // field's, which is wholly painted to its own edge.
    if !paths.lines.is_empty() || !paths.tracks.is_empty() {
        fp_slider(ui, "Paint", &mut paths.share, 0.0, 1.0, dirty).on_hover_text(
            "How much of that layer shows at a path's middle: less over snow, where earth \
             reads as a dark smear.",
        );
        fp_slider(
            ui,
            "Width (m)",
            &mut paths.width,
            limits::MIN_PATH_WIDTH,
            limits::MAX_PATH_WIDTH,
            dirty,
        )
        .on_hover_text("How wide each path is.");
    }
    if !paths.lines.is_empty()
        && ui
            .button("Clear paths")
            .on_hover_text(
                "Take the paths off the ground: the grass, trees and rocks they kept off \
                 grow back. The tracks stay.",
            )
            .clicked()
    {
        paths.lines.clear();
        *dirty = true;
    }
    draw_tracks(ui, paths, dirty);
    draw_fields(ui, paths, dirty);
}

/// The fields (#1791): each one's middle, size, bearing, strips and crops,
/// a way to remove it, and a way to add one.
fn draw_fields(ui: &mut egui::Ui, paths: &mut SovereignPathsConfig, dirty: &mut bool) {
    let weak = crate::ui::theme::current(ui.ctx()).text_weak;
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "Fields: rectangles of ground in strips of crops in turn, painted in the same \
             layer - no grass, tree or rock grows on them.",
        )
        .small()
        .color(weak),
    );
    let mut removed = None;
    for (k, field) in paths.fields.iter_mut().enumerate() {
        let title = format!(
            "A field, {:.0} by {:.0} m",
            field.size.0[0], field.size.0[1]
        );
        ui.push_id(("field", k), |ui| {
            egui::CollapsingHeader::new(title)
                .id_salt("field")
                .default_open(false)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Middle (m)");
                        for (axis, value) in ["x", "z"].iter().zip(field.center.0.iter_mut()) {
                            let mut at = *value;
                            if ui
                                .add(
                                    crate::ui::num::drag(&mut at)
                                        .prefix(format!("{axis} "))
                                        .speed(0.5)
                                        .range(-limits::MAX_PATH_REACH..=limits::MAX_PATH_REACH),
                                )
                                .changed()
                                && at != *value
                            {
                                *value = at;
                                *dirty = true;
                            }
                        }
                    });
                    for (label, side) in ["Length (m)", "Depth (m)"]
                        .into_iter()
                        .zip(field.size.0.iter_mut())
                    {
                        let mut value = crate::pds::Fp(*side);
                        fp_slider(
                            ui,
                            label,
                            &mut value,
                            limits::MIN_FIELD_SIDE,
                            limits::MAX_FIELD_SIDE,
                            dirty,
                        );
                        *side = value.0;
                    }
                    // To a half turn, which the record folds back to nought.
                    fp_slider(ui, "Bearing", &mut field.bearing, 0.0, 180.0, dirty)
                        .on_hover_text("The way its strips run, degrees clockwise from north.");
                    fp_slider(
                        ui,
                        "Strip width (m)",
                        &mut field.strip,
                        limits::MIN_FIELD_STRIP,
                        limits::MAX_FIELD_STRIP,
                        dirty,
                    );
                    ui.label(
                        egui::RichText::new("Its strips' colours, in turn:")
                            .small()
                            .color(weak),
                    );
                    for (j, crop) in field.crops.iter_mut().enumerate() {
                        super::widgets::color_picker(ui, &format!("Crop {}", j + 1), crop, dirty);
                    }
                    ui.horizontal(|ui| {
                        if field.crops.len() < limits::MAX_FIELD_CROPS
                            && ui.button("Add a crop").clicked()
                        {
                            let next =
                                crate::pds::SovereignField::default().crops[field.crops.len() % 2];
                            field.crops.push(next);
                            *dirty = true;
                        }
                        if field.crops.len() > 1 && ui.button("Remove the last crop").clicked() {
                            field.crops.pop();
                            *dirty = true;
                        }
                    });
                    if ui
                        .button("Remove this field")
                        .on_hover_text("Its ground grows grass, trees and rocks again.")
                        .clicked()
                    {
                        removed = Some(k);
                    }
                });
        });
    }
    if let Some(k) = removed {
        paths.fields.remove(k);
        *dirty = true;
    }
    if paths.fields.len() < limits::MAX_FIELDS
        && ui
            .button("Add a field")
            .on_hover_text(
                "A field of ploughed earth and green crops at the world's middle: move it \
                 from there.",
            )
            .clicked()
    {
        paths.fields.push(crate::pds::SovereignField::default());
        *dirty = true;
    }
}

/// The tracks (#1782): each described by its shape and length, with its
/// width, and a way to clear them all.
fn draw_tracks(ui: &mut egui::Ui, paths: &mut SovereignPathsConfig, dirty: &mut bool) {
    if paths.tracks.is_empty() {
        return;
    }
    let weak = crate::ui::theme::current(ui.ctx()).text_weak;
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "Tracks: ways wider than the paths, in the same paint - an airstrip, a car's \
             track.",
        )
        .small()
        .color(weak),
    );
    for (k, track) in paths.tracks.iter_mut().enumerate() {
        let closed = track.line.len() > 2 && track.line.first() == track.line.last();
        let what = if closed { "A loop" } else { "A straight" };
        let label = format!("{what}, {:.0} m: width (m)", run_of(&track.line));
        ui.push_id(("track", k), |ui| {
            fp_slider(
                ui,
                &label,
                &mut track.width,
                limits::MIN_TRACK_WIDTH,
                limits::MAX_TRACK_WIDTH,
                dirty,
            )
            .on_hover_text("How wide this track is painted, and kept clear of trees and rocks.");
        });
    }
    if ui
        .button("Clear tracks")
        .on_hover_text(
            "Take the tracks off the ground: the grass, trees and rocks they kept off grow back.",
        )
        .clicked()
    {
        paths.tracks.clear();
        *dirty = true;
    }
}

/// How long `line` runs, end to end (m).
fn run_of(line: &[crate::pds::Fp2]) -> f32 {
    line.windows(2)
        .map(|pair| (pair[1].0[0] - pair[0].0[0]).hypot(pair[1].0[1] - pair[0].0[1]))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{Fp, Fp2};

    /// The section shown and not touched writes nothing (the rule of every
    /// editor widget): none, a seeded world's, and one a newer or hostile
    /// client wrote out of range - every field's settings drawn open.
    #[test]
    fn an_untouched_paths_section_writes_nothing() {
        let strip = crate::pds::SovereignTrack {
            line: vec![Fp2([40.0, -20.0]), Fp2([120.0, -20.0])],
            width: Fp(12.0),
        };
        let seeded = SovereignPathsConfig {
            lines: vec![
                vec![Fp2([0.0, 60.0]), Fp2([0.5, 30.0]), Fp2([0.0, 9.0])],
                vec![Fp2([0.0, 60.0]), Fp2([8.0, 55.0])],
            ],
            tracks: vec![strip.clone()],
            fields: vec![crate::pds::SovereignField {
                center: Fp2([60.0, -40.0]),
                bearing: Fp(35.0),
                ..Default::default()
            }],
            ..Default::default()
        };
        let starts = [
            SovereignPathsConfig::default(),
            seeded.clone(),
            SovereignPathsConfig {
                lines: Vec::new(),
                ..seeded.clone()
            },
            SovereignPathsConfig {
                layer: 9,
                share: Fp(2.0),
                width: Fp(40.0),
                tracks: vec![crate::pds::SovereignTrack {
                    width: Fp(500.0),
                    ..strip
                }],
                fields: vec![crate::pds::SovereignField {
                    center: Fp2([2.0e4, -2.0e4]),
                    size: Fp2([300.0, 1.0]),
                    bearing: Fp(179.6365),
                    strip: Fp(50.0),
                    crops: vec![crate::pds::Fp3([2.0, -1.0, 0.5]); 9],
                }],
                ..seeded
            },
        ];
        for start in starts {
            let ctx = egui::Context::default();
            // Every field's header open, so its body is drawn too.
            ctx.memory_mut(|m| m.set_everything_is_visible(true));
            let mut drawn = start.clone();
            let mut dirty = false;
            let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
                draw_paths_section(ui, &mut drawn, &mut dirty);
            });
            assert_eq!(drawn, start);
            assert!(!dirty, "{start:?}");
        }
    }
}
