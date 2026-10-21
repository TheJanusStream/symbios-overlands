//! The crosshair (#1754): where mouse look in first person points - the
//! screen's centre, while it holds the pointer - which is where a click acts
//! and a pick is aimed (`editor_gizmo::ScenePick::cursor_position`).

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

/// How long each of the crosshair's four arms is, and the gap they leave at
/// its middle, in points.
const ARM: f32 = 7.0;
const GAP: f32 = 2.5;

/// Draw the crosshair while mouse look holds the pointer: four short arms
/// round a gap, light over a dark edge, so it reads on snow and at night alike.
/// On egui's foreground order, above the world and every window.
pub fn crosshair_ui(
    mut contexts: EguiContexts,
    first_person: Option<Res<crate::camera::FirstPerson>>,
) {
    if !first_person
        .as_deref()
        .is_some_and(crate::camera::FirstPerson::mouse_look_captured)
    {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let centre = ctx.content_rect().center();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("first_person_crosshair"),
    ));
    let edge = egui::Stroke::new(3.0, egui::Color32::from_black_alpha(150));
    let line = egui::Stroke::new(1.5, egui::Color32::from_white_alpha(230));
    for stroke in [edge, line] {
        for way in [egui::Vec2::X, -egui::Vec2::X, egui::Vec2::Y, -egui::Vec2::Y] {
            painter.line_segment([centre + way * GAP, centre + way * (GAP + ARM)], stroke);
        }
    }
}
