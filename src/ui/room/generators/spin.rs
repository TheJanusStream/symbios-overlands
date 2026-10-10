//! The Spin section of the generator detail panel (#1604): whether a node
//! turns, about which axis, and by which terms. Like the rest of the panel
//! it serves a world's items, a body's visuals and a worn prop's parts.
//!
//! A spin is animation each client plays for itself; the transform above
//! it stays the pose on record. The panel says the three things about it an
//! owner cannot see from the controls: everything is paused while anything
//! is selected, a turning part has no collision, and the motion terms
//! follow the part's own travel.

use bevy_egui::egui;

use crate::pds::sanitize::limits;
use crate::pds::{Fp, Fp3, GeneratorKind, Spin, SpinTerm};

use super::super::widgets::unrecognised_value_line;

/// The axes the picker names, in the node's own frame.
const AXES: [(&str, [f32; 3]); 3] = [
    ("X", [1.0, 0.0, 0.0]),
    ("Y", [0.0, 1.0, 0.0]),
    ("Z", [0.0, 0.0, 1.0]),
];

/// Draw the Spin section for one node. `refusal` is the tree's reason its
/// root may not turn, passed for the root only.
pub(super) fn draw_spin_section(
    ui: &mut egui::Ui,
    spin: &mut Option<Spin>,
    kind: &GeneratorKind,
    refusal: Option<&str>,
    salt: &str,
    dirty: &mut bool,
) {
    let theme = crate::ui::theme::current(ui.ctx());
    ui.label(
        egui::RichText::new("Spin")
            .strong()
            .color(theme.text_strong),
    );
    let weak = |ui: &mut egui::Ui, text: &str| {
        ui.label(egui::RichText::new(text).small().color(theme.text_weak));
    };
    let reason = if !kind.may_spin() {
        Some("Terrain, water and roads are laid in the world's own terms, so they cannot turn.")
    } else {
        refusal
    };
    if let Some(reason) = reason {
        weak(ui, reason);
        return;
    }

    let mut on = spin.is_some();
    if ui
        .checkbox(&mut on, "Turn this part")
        .on_hover_text(
            "Every visitor's client turns it, and everything attached below it, \
             for itself. The transform above stays the pose on record.",
        )
        .changed()
    {
        *spin = on.then(|| {
            Spin::about(
                [0.0, 1.0, 0.0],
                SpinTerm::fresh("Constant").expect("a kind the panel offers"),
            )
        });
        *dirty = true;
    }
    let Some(current) = spin.as_mut() else {
        weak(
            ui,
            "A windmill's sails, a pendulum, a wheel that rolls with its vehicle.",
        );
        return;
    };

    draw_axis(ui, &mut current.axis, salt, dirty);

    let mut remove = None;
    for (i, term) in current.terms.iter_mut().enumerate() {
        ui.push_id((salt, "spin_term", i), |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(term.label()).strong());
                if ui.small_button("Remove").clicked() {
                    remove = Some(i);
                }
            });
            ui.indent("spin_term_fields", |ui| draw_term(ui, term, dirty));
        });
    }
    if let Some(i) = remove {
        current.terms.remove(i);
        *dirty = true;
    }

    let full = current.terms.len() >= limits::MAX_SPIN_TERMS;
    let full_reason = format!("{} terms at most.", limits::MAX_SPIN_TERMS);
    ui.horizontal(|ui| {
        let mut add = ui.add_enabled_ui(!full, |ui| {
            ui.menu_button("+ Add term", |ui| {
                for kind in SpinTerm::KINDS {
                    if ui.button(kind).on_hover_text(term_help(kind)).clicked() {
                        current.terms.extend(SpinTerm::fresh(kind));
                        *dirty = true;
                        ui.close();
                    }
                }
            });
        });
        add.response = add.response.on_disabled_hover_text(&full_reason);
        if full {
            // Painted as well: a disabled control's hover never fires inside
            // an open popup (#1289).
            weak(ui, &full_reason);
        }
    });
    // The last term gone is no spin: the checkbox reads off, and the record
    // carries none.
    if current.terms.is_empty() {
        *spin = None;
    }

    weak(
        ui,
        "Paused while anything is selected - deselect to watch it. A turning \
         part, and everything below it, has no collision.",
    );
}

/// The axis row: X, Y or Z of the node's own frame, the other way round, or
/// any direction typed in.
fn draw_axis(ui: &mut egui::Ui, axis: &mut Fp3, salt: &str, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label("Axis");
        let v = bevy::math::Vec3::from_array(axis.0).normalize_or_zero();
        for (name, along) in AXES {
            let along = bevy::math::Vec3::from_array(along);
            // Selected either way round: "Flip" keeps the axis, reversed.
            let selected = v.dot(along).abs() > 0.999;
            if ui.selectable_label(selected, name).clicked() && !selected {
                *axis = Fp3(along.to_array());
                *dirty = true;
            }
        }
        if ui
            .button("Flip")
            .on_hover_text("Turn the other way round the same axis.")
            .clicked()
        {
            *axis = Fp3(axis.0.map(|c| -c));
            *dirty = true;
        }
    });
    egui::CollapsingHeader::new("Any direction")
        .id_salt((salt, "spin_axis_free"))
        .default_open(false)
        .show(ui, |ui| {
            let mut v = axis.0;
            let mut changed = false;
            ui.horizontal(|ui| {
                for c in v.iter_mut() {
                    changed |= ui
                        .add(crate::ui::num::drag(c).speed(0.01).range(-1.0..=1.0))
                        .changed();
                }
            });
            if changed {
                *axis = Fp3(v);
                *dirty = true;
            }
            ui.label(
                egui::RichText::new("In the part's own frame; only the direction counts.")
                    .small()
                    .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );
        });
}

/// One term's fields.
fn draw_term(ui: &mut egui::Ui, term: &mut SpinTerm, dirty: &mut bool) {
    match term {
        SpinTerm::Constant { rate } => {
            let cap = limits::MAX_SPIN_RATE_DEG;
            number(ui, "Rate", rate, -cap..=cap, 1.0, " °/s", dirty);
        }
        SpinTerm::Swing {
            amplitude,
            period,
            phase,
        } => {
            let reach = limits::MAX_SWING_AMPLITUDE_DEG;
            number(
                ui,
                "Either side",
                amplitude,
                -reach..=reach,
                0.5,
                "°",
                dirty,
            );
            let periods = limits::MIN_SWING_PERIOD_S..=limits::MAX_SWING_PERIOD_S;
            number(ui, "One swing every", period, periods, 0.05, " s", dirty);
            let turn = limits::MAX_SWING_PHASE_DEG;
            number(ui, "Starting at", phase, -turn..=turn, 1.0, "°", dirty);
        }
        SpinTerm::Roll { radius } => {
            let radii = limits::MIN_ROLL_RADIUS_M..=limits::MAX_ROLL_RADIUS_M;
            number(ui, "Wheel radius", radius, radii, 0.01, " m", dirty);
            ui.label(
                egui::RichText::new(
                    "Rolls with this part's own travel over the ground, as a \
                     wheel of this radius would.",
                )
                .small()
                .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );
        }
        SpinTerm::Steer { gain, limit } => {
            let g = limits::MAX_STEER_GAIN;
            number(ui, "Gain", gain, -g..=g, 0.01, " s", dirty).on_hover_text(
                "Degrees of turn for each degree per second the part above \
                 this one turns. Negative steers against the turn.",
            );
            number(
                ui,
                "At most",
                limit,
                0.0..=limits::MAX_STEER_LIMIT_DEG,
                0.5,
                "°",
                dirty,
            );
        }
        SpinTerm::Unknown => {
            unrecognised_value_line(ui, "spin term", Some("it turns nothing here"));
        }
    }
}

/// A labelled number for one of a term's fields.
fn number(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Fp,
    range: std::ops::RangeInclusive<f32>,
    speed: f64,
    suffix: &str,
    dirty: &mut bool,
) -> egui::Response {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut v = value.0;
        let response = ui.add(
            crate::ui::num::drag(&mut v)
                .speed(speed)
                .range(range)
                .suffix(suffix),
        );
        if response.changed() {
            *value = Fp(v);
            *dirty = true;
        }
        response
    })
    .inner
}

/// What each kind of term does, for the add menu.
fn term_help(kind: &str) -> &'static str {
    match kind {
        "Constant" => "Turn at a steady rate: a windmill's sails, a fan, a turntable.",
        "Swing" => "Swing to and fro: a pendulum, a hanging sign.",
        "Roll" => "Roll with the part's own travel, like a wheel.",
        "Steer" => "Turn with the turning of the part above: front wheels into a bend.",
        _ => "",
    }
}
