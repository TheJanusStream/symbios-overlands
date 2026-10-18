//! The terrain panel's Grass section (#1700): how the grass the world grows
//! looks, as its owner sets it, for every visitor.
//!
//! Each setting has an Auto box. Ticked, the setting follows what the world
//! already stores - its grass tufts and ground layers
//! (`grass::profile::derived_profile`) - and its control shows what they
//! give, greyed. Unticked, the control sets the world's own value, which
//! starts from what Auto gave, so unticking changes nothing anyone sees
//! until the control moves.

use bevy_egui::egui;

use super::widgets::{fp_slider, linear_color_button};
use crate::grass::profile::{GrassProfile, Tufts};
use crate::pds::sanitize::limits;
use crate::pds::{Fp, Fp3, SovereignGrassConfig};

/// What the Grass section needs from beyond the terrain's own node.
pub(crate) struct GrassPanel<'a> {
    /// The grass the world grows with every setting Auto: what an Auto row
    /// shows, and where a setting starts once its box is unticked.
    pub derived: &'a GrassProfile,
    /// The grass tufts the world's scatters place, and how many of them do
    /// not yet hold its grass colours: what Match the tufts recolours.
    pub tufts: Tufts,
    /// Set when the owner asks to match the tufts. The caller does it, as
    /// the only level holding the whole record: it recolours other nodes.
    pub match_tufts: &'a mut bool,
}

/// What each kind of row follows on Auto, said after "this".
const FROM_TUFTS: &str = "follows the grass tufts the world scatters most of";
const FROM_FLOWERS: &str = "follows the world's flower cards (with none, daisy white, \
                            buttercup yellow, cornflower violet and clover pink)";
const FROM_GROUND: &str = "follows each ground layer's texture and the grass tufts on it";
/// The terrain's four ground layers, as the Material section names them.
const LAYERS: [&str; 4] = ["Grass (R)", "Dirt (G)", "Rock (B)", "Snow (A)"];

/// The Grass section: a row for each setting of `grass`, the world's own,
/// over what `panel` says the world grows on Auto.
pub(super) fn draw_grass_section(
    ui: &mut egui::Ui,
    grass: &mut SovereignGrassConfig,
    panel: &mut GrassPanel<'_>,
    dirty: &mut bool,
) {
    let derived = panel.derived;
    ui.label(
        egui::RichText::new(
            "How the grass looks in this world, for everyone who visits it. A setting \
             on Auto follows the world's grass tufts and ground layers. How far out \
             the grass is drawn is each visitor's own Grass setting.",
        )
        .small()
        .color(crate::ui::theme::current(ui.ctx()).text_weak),
    );
    ui.add_space(4.0);

    setting(
        ui,
        "Root colour",
        "The colour at the foot of each blade.",
        FROM_TUFTS,
        &mut grass.color_base,
        &Fp3(derived.base),
        dirty,
        colour,
    );
    setting(
        ui,
        "Tip colour",
        "The colour at the top of each blade: most of what a field shows at a glance.",
        FROM_TUFTS,
        &mut grass.color_tip,
        &Fp3(derived.tip),
        dirty,
        colour,
    );
    setting(
        ui,
        "Dry colour",
        "The colour the dry blades turn.",
        FROM_TUFTS,
        &mut grass.color_dry,
        &Fp3(derived.dry),
        dirty,
        colour,
    );
    setting(
        ui,
        "Dry blades",
        "How many of the blades are dry, from none to all. Dry grass stands taller.",
        FROM_TUFTS,
        &mut grass.dry_share,
        &Fp(derived.dry_share),
        dirty,
        |ui, value, dirty| fp_slider(ui, "", value, 0.0, 1.0, dirty),
    );
    setting(
        ui,
        "Flowers",
        "How many of the blades are flowers, from none to all.",
        FROM_FLOWERS,
        &mut grass.flower_share,
        &Fp(derived.flower_share),
        dirty,
        |ui, value, dirty| fp_slider(ui, "", value, 0.0, 1.0, dirty),
    );
    let flowers: Vec<Fp3> = derived.flowers.iter().copied().map(Fp3).collect();
    setting(
        ui,
        "Flower colours",
        "The colours the flowers take, up to four. Each flower takes one of them.",
        FROM_FLOWERS,
        &mut grass.flower_colors,
        &flowers,
        dirty,
        flower_colours,
    );
    setting(
        ui,
        "Grows on",
        "How thickly the grass grows on each of the terrain's ground layers, from \
         none to full.",
        FROM_GROUND,
        &mut grass.layer_density,
        &derived.layer_density.map(Fp),
        dirty,
        layers,
    );
    setting(
        ui,
        "Thickness",
        "How thick the grass grows wherever it grows. None grows no grass in this world.",
        "is the full thickness the ground grows",
        &mut grass.thickness,
        &Fp(1.0),
        dirty,
        |ui, value, dirty| fp_slider(ui, "", value, 0.0, 1.0, dirty),
    );
    setting(
        ui,
        "Sway",
        "How far the wind leans the grass: at 1 as far as grass leans unset, at 0 it \
         stands still in any wind, at 2 twice as far.",
        "is 1, as far as any grass leans",
        &mut grass.sway,
        &Fp(derived.sway),
        dirty,
        |ui, value, dirty| fp_slider(ui, "", value, 0.0, limits::MAX_GRASS_SWAY, dirty),
    );

    ui.add_space(4.0);
    let why = if panel.tufts.scattered == 0 {
        "This world scatters no grass tufts to match."
    } else {
        "Every grass tuft in this world already holds its grass colours."
    };
    if ui
        .add_enabled(
            panel.tufts.unmatched > 0,
            egui::Button::new("Match the tufts"),
        )
        .on_hover_text(
            "Recolour this world's grass tufts to the grass's root, tip and dry colours and \
             its dry blades, so the tufts stand among the blades as part of the field.",
        )
        .on_disabled_hover_text(why)
        .clicked()
    {
        *panel.match_tufts = true;
    }
}

/// One setting's row, labelled `label` and explained by `hover`: its Auto
/// box, saying what the setting `auto` (follows ...) while ticked, and
/// `control` over `value` once it is set - or, on Auto, over a copy of
/// `derived`, greyed, which nothing it does can write.
#[allow(clippy::too_many_arguments)]
fn setting<T: Clone>(
    ui: &mut egui::Ui,
    label: &str,
    hover: &str,
    auto: &str,
    value: &mut Option<T>,
    derived: &T,
    dirty: &mut bool,
    control: impl FnOnce(&mut egui::Ui, &mut T, &mut bool) -> egui::Response,
) {
    ui.horizontal(|ui| {
        let mut on_auto = value.is_none();
        if ui
            .checkbox(&mut on_auto, "Auto")
            .on_hover_text(format!("Ticked, this {auto}. Untick it to set your own."))
            .changed()
        {
            *value = (!on_auto).then(|| derived.clone());
            *dirty = true;
        }
        ui.label(label).on_hover_text(hover);
        match value.as_mut() {
            Some(own) => {
                control(ui, own, dirty).on_hover_text(hover);
            }
            None => {
                let mut shown = derived.clone();
                let mut unwritten = false;
                ui.add_enabled_ui(false, |ui| control(ui, &mut shown, &mut unwritten))
                    .response
                    .on_disabled_hover_text(format!(
                        "On Auto, this {auto}. Untick Auto to set your own."
                    ));
            }
        }
    });
}

/// A colour's swatch, which opens the picker.
fn colour(ui: &mut egui::Ui, value: &mut Fp3, dirty: &mut bool) -> egui::Response {
    let response = linear_color_button(ui, value);
    if response.changed() {
        *dirty = true;
    }
    response
}

/// The flowers' colours: a swatch and a remove button each, and a button to
/// add one while there are fewer than the grass holds.
fn flower_colours(ui: &mut egui::Ui, colours: &mut Vec<Fp3>, dirty: &mut bool) -> egui::Response {
    ui.horizontal(|ui| {
        let mut gone = None;
        for (i, value) in colours.iter_mut().enumerate() {
            colour(ui, value, dirty);
            if ui
                .small_button("x")
                .on_hover_text("Take this colour out.")
                .clicked()
            {
                gone = Some(i);
            }
        }
        if let Some(i) = gone {
            colours.remove(i);
            *dirty = true;
        }
        if colours.is_empty() {
            ui.label("None: no flowers grow.");
        }
        if colours.len() < limits::MAX_GRASS_FLOWER_COLORS
            && ui
                .small_button("+")
                .on_hover_text("Add a flower colour.")
                .clicked()
        {
            let next = colours
                .last()
                .copied()
                .unwrap_or(Fp3(crate::config::grass::DEFAULT_FLOWERS[0]));
            colours.push(next);
            *dirty = true;
        }
    })
    .response
}

/// A slider for each ground layer, named as the Material section names it.
fn layers(ui: &mut egui::Ui, layers: &mut [Fp; 4], dirty: &mut bool) -> egui::Response {
    ui.vertical(|ui| {
        for (density, name) in layers.iter_mut().zip(LAYERS) {
            fp_slider(ui, name, density, 0.0, 1.0, dirty);
        }
    })
    .response
}

#[cfg(test)]
mod tests {
    use super::super::widgets::text_probe::button;
    use super::*;
    use bevy_egui::egui::accesskit::{Node, Role};

    /// The grass the default world grows on Auto.
    fn derived() -> GrassProfile {
        crate::grass::profile::derived_profile(&crate::pds::RoomRecord::default())
    }

    /// Draw `draw` for four frames, pressing the primary button on the
    /// topmost node `target` picks in the second and letting it go in the
    /// third - found on the first through AccessKit, as a person finds it by
    /// eye. Whether there was such a node.
    fn click_on(target: impl Fn(&Node) -> bool, mut draw: impl FnMut(&mut egui::Ui)) -> bool {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 1400.0));
        let mut at = None;
        for frame in 0..4 {
            let events = match (frame, at) {
                (1, Some(pos)) => vec![egui::Event::PointerMoved(pos), button(pos, true)],
                (2, Some(pos)) => vec![button(pos, false)],
                _ => Vec::new(),
            };
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                &mut draw,
            );
            if frame == 0 {
                let update = output
                    .platform_output
                    .accesskit_update
                    .expect("AccessKit is on");
                at = update
                    .nodes
                    .iter()
                    .filter(|(_, node)| target(node))
                    .filter_map(|(_, node)| node.bounds())
                    .min_by(|a, b| a.y0.total_cmp(&b.y0))
                    .map(|b| {
                        egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)
                    });
            }
        }
        at.is_some()
    }

    /// The section drawn on a world that sets nothing, frame after frame,
    /// writes nothing: no setting, no dirty record, no match asked for.
    #[test]
    fn an_auto_section_draws_without_writing() {
        let derived = derived();
        let mut grass = SovereignGrassConfig::default();
        let (mut dirty, mut matched) = (false, false);
        let clicked = click_on(
            |_| false,
            |ui| {
                let mut panel = GrassPanel {
                    derived: &derived,
                    tufts: Tufts {
                        scattered: 2,
                        unmatched: 2,
                    },
                    match_tufts: &mut matched,
                };
                draw_grass_section(ui, &mut grass, &mut panel, &mut dirty);
            },
        );
        assert!(!clicked);
        assert!(grass.is_auto(), "{grass:?}");
        assert!(!dirty && !matched);
    }

    /// Unticking a row's Auto box sets the world's own value to what Auto
    /// gave - the topmost row, the root colour - so nothing seen changes, and
    /// the record is dirty; ticking it again lets the setting go.
    #[test]
    fn unticking_auto_starts_from_what_auto_gave() {
        let derived = derived();
        let mut grass = SovereignGrassConfig::default();
        let (mut dirty, mut matched) = (false, false);
        let auto = |node: &Node| node.role() == Role::CheckBox && node.label() == Some("Auto");
        let mut draw = |grass: &mut SovereignGrassConfig, dirty: &mut bool| {
            click_on(auto, |ui| {
                let mut panel = GrassPanel {
                    derived: &derived,
                    tufts: Tufts::default(),
                    match_tufts: &mut matched,
                };
                draw_grass_section(ui, grass, &mut panel, dirty);
            })
        };
        assert!(draw(&mut grass, &mut dirty), "no Auto box drawn");
        assert_eq!(grass.color_base, Some(Fp3(derived.base)));
        assert_eq!(
            grass,
            SovereignGrassConfig {
                color_base: Some(Fp3(derived.base)),
                ..SovereignGrassConfig::default()
            },
            "one click sets one setting"
        );
        assert!(dirty);

        dirty = false;
        assert!(draw(&mut grass, &mut dirty));
        assert!(grass.is_auto(), "ticked again, the setting goes: {grass:?}");
        assert!(dirty);
    }

    /// A world that sets every setting, drawn frame after frame with no
    /// input, keeps every value it set - its sliders, colour buttons and
    /// flower list write nothing on sight - and stays clean.
    #[test]
    fn a_set_section_draws_without_writing() {
        let derived = derived();
        let set = SovereignGrassConfig {
            color_base: Some(Fp3([0.123_456, 0.2, 0.05])),
            color_tip: Some(Fp3([0.61, 0.47, 0.13])),
            color_dry: Some(Fp3([0.7, 0.58, 0.3])),
            dry_share: Some(Fp(0.333_33)),
            flower_share: Some(Fp(0.45)),
            flower_colors: Some(vec![Fp3([0.3, 0.12, 0.62]), Fp3([0.45, 0.25, 0.75])]),
            layer_density: Some([Fp(1.0), Fp(0.71), Fp(0.3), Fp(0.0)]),
            thickness: Some(Fp(0.6)),
            sway: Some(Fp(1.5)),
        };
        let mut grass = set.clone();
        let (mut dirty, mut matched) = (false, false);
        click_on(
            |_| false,
            |ui| {
                let mut panel = GrassPanel {
                    derived: &derived,
                    tufts: Tufts::default(),
                    match_tufts: &mut matched,
                };
                draw_grass_section(ui, &mut grass, &mut panel, &mut dirty);
            },
        );
        assert_eq!(grass, set);
        assert!(!dirty);
    }

    /// Match the tufts asks the caller, which holds the whole record, to
    /// recolour them; with no tufts to match it is greyed and asks nothing.
    #[test]
    fn match_the_tufts_asks_the_caller() {
        let derived = derived();
        let target = |node: &Node| node.label() == Some("Match the tufts");
        let tufts = |scattered, unmatched| Tufts {
            scattered,
            unmatched,
        };
        for (tufts, asked) in [
            (tufts(3, 2), true),
            (tufts(3, 0), false),
            (tufts(0, 0), false),
        ] {
            let mut grass = SovereignGrassConfig::default();
            let (mut dirty, mut matched) = (false, false);
            let found = click_on(target, |ui| {
                let mut panel = GrassPanel {
                    derived: &derived,
                    tufts,
                    match_tufts: &mut matched,
                };
                draw_grass_section(ui, &mut grass, &mut panel, &mut dirty);
            });
            assert!(found, "no Match the tufts button");
            assert_eq!(matched, asked, "with {tufts:?}");
            assert!(!dirty, "asking is not an edit of the terrain");
        }
    }
}
