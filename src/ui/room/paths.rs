//! The terrain panel's Paths section (#1779): the footways a seeded world
//! routed from its landing to its buildings, painted on its ground - the
//! layer that paints them, how much of it, how wide, or none at all - and
//! its tracks (#1782), the wider ways of its airstrip and its car's track,
//! each its own width.

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
        .on_hover_text("The ground layer the paths are painted in: the Material section's four.");
    if layer != before {
        paths.layer = layer;
        *dirty = true;
    }
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
    /// client wrote out of range.
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
                ..seeded
            },
        ];
        for start in starts {
            let ctx = egui::Context::default();
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
