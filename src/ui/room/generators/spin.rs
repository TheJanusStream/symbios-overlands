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

    draw_direction(
        ui,
        "Axis",
        "Turn the other way round the same axis.",
        &mut current.axis,
        salt,
        dirty,
    );

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

/// A direction row - the spin's axis, a vane's downwind side: X, Y or Z of
/// the node's own frame, the other way round, or any direction typed in.
fn draw_direction(
    ui: &mut egui::Ui,
    label: &str,
    flip_help: &str,
    axis: &mut Fp3,
    salt: &str,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label(label);
        let v = bevy::math::Vec3::from_array(axis.0).normalize_or_zero();
        for (name, along) in AXES {
            let along = bevy::math::Vec3::from_array(along);
            // Selected either way round: "Flip" keeps the line, reversed.
            let selected = v.dot(along).abs() > 0.999;
            if ui.selectable_label(selected, name).clicked() && !selected {
                *axis = Fp3(along.to_array());
                *dirty = true;
            }
        }
        if ui.button("Flip").on_hover_text(flip_help).clicked() {
            *axis = Fp3(axis.0.map(|c| -c));
            *dirty = true;
        }
    });
    egui::CollapsingHeader::new("Any direction")
        .id_salt((salt, label, "free"))
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
        SpinTerm::Speed { idle, gain } => {
            let rate = limits::MAX_SPIN_RATE_DEG;
            number(ui, "Idle", idle, -rate..=rate, 1.0, " °/s", dirty);
            let per_m = limits::MAX_SPEED_GAIN_DEG_PER_M;
            number(ui, "Per metre", gain, -per_m..=per_m, 1.0, " °/m", dirty).on_hover_text(
                "Degrees a second more for each metre a second the part travels \
                 forward - toward its parent's +Z, as vehicles are drawn. It \
                 slows, and turns back, as the vehicle reverses.",
            );
        }
        SpinTerm::Lean { gain, period } => {
            hanging_magnitude(ui, "Per m/s²", gain, limits::MAX_LEAN_GAIN, 0.1, "°", dirty);
            let periods = limits::MIN_SWING_PERIOD_S..=limits::MAX_LEAN_PERIOD_S;
            number(ui, "Swings every", period, periods, 0.05, " s", dirty);
            weak_note(
                ui,
                "Leans against its vehicle's speeding up and slowing down, and \
                 swings back. 5.84° per m/s² is a real pendulum.",
            );
        }
        SpinTerm::Wind { amplitude } => {
            hanging_magnitude(
                ui,
                "Sway",
                amplitude,
                limits::MAX_WIND_AMPLITUDE_DEG,
                0.5,
                "°",
                dirty,
            );
            weak_note(
                ui,
                "In the world's wind, the one its trees sway in (Environment), \
                 when it crosses the axis squarely; further in a stronger wind.",
            );
        }
        SpinTerm::Wobble { amplitude, period } => {
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
            number(ui, "About every", period, periods, 0.05, " s", dirty);
        }
        SpinTerm::Vane { facing } => {
            draw_direction(
                ui,
                "Downwind",
                "Point the other side of the part downwind.",
                facing,
                "spin_vane",
                dirty,
            );
            weak_note(
                ui,
                "Turns until this side of the part points downwind in the \
                 world's wind (Environment) - a weather vane's tail, a \
                 windmill's head.",
            );
        }
        SpinTerm::Unknown => {
            unrecognised_value_line(ui, "spin term", Some("it turns nothing here"));
        }
    }
}

/// A weak note under a term's fields.
fn weak_note(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .small()
            .color(crate::ui::theme::current(ui.ctx()).text_weak),
    );
}

/// A magnitude whose sign says whether the part hangs below its axis
/// (positive) or stands on it (negative): one number and a checkbox, rather
/// than a sign the owner has to remember.
fn hanging_magnitude(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Fp,
    max: f32,
    speed: f64,
    suffix: &str,
    dirty: &mut bool,
) {
    let mut magnitude = Fp(value.0.abs());
    // By the sign bit, so unticking at a zero magnitude (which writes -0.0)
    // stays unticked rather than reading back as "hangs".
    let mut hangs = !value.0.is_sign_negative();
    let mut changed = false;
    number(
        ui,
        label,
        &mut magnitude,
        0.0..=max,
        speed,
        suffix,
        &mut changed,
    );
    changed |= ui
        .checkbox(&mut hangs, "Hangs below its axis")
        .on_hover_text(
            "Ticked, its bottom swings - a sign, a lantern. Unticked, it stands \
             on its axis and its top moves - a mast, an aerial.",
        )
        .changed();
    if changed {
        *value = Fp(if hangs { magnitude.0 } else { -magnitude.0 });
        *dirty = true;
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
        "Speed" => "Turn faster as the part travels: a propeller, a fan.",
        "Lean" => "Lean against speeding up and slowing down: a hanging lantern.",
        "Wind" => "Sway in the world's wind: a hanging sign, a banner.",
        "Wobble" => "Wander to and fro without a beat: a bobbing buoy, a hovering drone.",
        "Vane" => "Turn to point downwind: a weather vane, a windmill's head.",
        _ => "",
    }
}
