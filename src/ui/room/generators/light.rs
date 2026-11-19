//! The Light section of the generator detail panel (#1922): whether a node
//! gives off light, and every property its [`LightSource`] holds - point or
//! spot, a spot's aim and cone, colour, brightness, reach, size, offset,
//! dusk to dawn, and a flicker or pulse (a flicker's shimmer on a rate of
//! its own while it is too shallow to flash, #1950). Like the Spin section
//! beside it, it serves a world's items, a body's visuals and a worn prop's
//! parts.
//!
//! Every edit is made on a copy and held through the light's own sanitiser
//! before it is written, so what the panel writes is already inside the
//! sanitiser's bounds and on the wire's grid: the editor's flush
//! (`settle_edit`) finds nothing left to move, a peer decodes the light the
//! owner holds, and a rate the depth's flash cap would slow is slowed here,
//! where the owner sees it, and never later - as is a brightness its reach
//! may not carry (no light is stronger than the brightest of the longest
//! reach), the Brightness slider's top following the Reach as the Rate's
//! follows the Depth. A copy that comes back from the sanitiser unchanged
//! is not written at all, so a colour swatch's round trip through egui's
//! picker - a float's last bits - dirties nothing. What the owner asked is
//! remembered while the light is still what that asking left ([`Asked`]),
//! so a Reach, Depth or Outer cone dragged past a limit and back gives back
//! the brightness, rate or inner cone it held down, and a light switched
//! from Pulse to Flicker, or Spot to Point, and back keeps its values.
//!
//! The panel says what the controls cannot: a dusk-to-dawn light shows only
//! at night; each client draws only the nearest lights, fewer at Effects
//! Reduced and none at Off; no light casts a shadow; a flicker or pulse
//! keeps going while the part is selected (a spin pauses, because the gizmo
//! holds its pose; a flicker moves no pose); and older versions draw none.
//! Where a light would draw nowhere - on an item placed only as an instance
//! set, or water at an item's root - it says so in place of the controls.

use bevy_egui::egui;

use crate::pds::light::{FLICKER_FINE_RATE, LightFlicker, LightShape, LightSource};
use crate::pds::sanitize::Sanitize as _;
use crate::pds::sanitize::light::{max_flicker_rate_hz, max_light_brightness, max_shimmer_rate_hz};
use crate::pds::sanitize::limits;
use crate::pds::{Fp, Fp3, GeneratorKind};

use super::super::widgets::{edit_srgb_rgb, unrecognised_value_line};

/// The directions the spot's aim picker names, in the node's own frame.
const AIMS: [(&str, [f32; 3]); 6] = [
    ("Down", [0.0, -1.0, 0.0]),
    ("Up", [0.0, 1.0, 0.0]),
    ("+X", [1.0, 0.0, 0.0]),
    ("-X", [-1.0, 0.0, 0.0]),
    ("+Z", [0.0, 0.0, 1.0]),
    ("-Z", [0.0, 0.0, -1.0]),
];

/// Decimals a number field shows at most: the wire's 1/10 000 grid, so a
/// value on it is shown whole and a field that loses focus writes back the
/// value it showed, not a rounded one. The Slide and Swell sections' fields
/// show as many (`super::slide::grid_number`).
pub(super) const GRID_DECIMALS: usize = 4;

/// Draw the Light section for one node. `refusal` is why a light on this
/// node draws nowhere, where it does not (see
/// [`super::GeneratorTreeSource::light_refusal`]): painted in place of the
/// controls, or over them for a light already there, which can still be
/// taken off.
pub(super) fn draw_light_section(
    ui: &mut egui::Ui,
    light: &mut Option<LightSource>,
    kind: &GeneratorKind,
    refusal: Option<&str>,
    salt: &str,
    dirty: &mut bool,
) {
    ui.push_id((salt, "light_section"), |ui| {
        draw_section(ui, light, kind, refusal, dirty);
    });
}

/// Why a light at the root of an item that is water draws nowhere: such
/// a root is no place the world draws (it is saved as a box).
pub(super) const ROOT_WATER_REFUSAL: &str = "Water at an item's root is not drawn, so a light \
     on it would light nothing. Light a part of the item that holds the water instead.";

fn draw_section(
    ui: &mut egui::Ui,
    light: &mut Option<LightSource>,
    kind: &GeneratorKind,
    refusal: Option<&str>,
    dirty: &mut bool,
) {
    let theme = crate::ui::theme::current(ui.ctx());
    ui.label(
        egui::RichText::new("Light")
            .strong()
            .color(theme.text_strong),
    );
    // A road network is drawn by its terrain and has no place of its own
    // to light from: its light would draw nothing. One already on it (a
    // hand-written record) can still be taken off.
    if matches!(kind, GeneratorKind::RoadNetwork(_)) && light.is_none() {
        weak(
            ui,
            "A road network is drawn by its terrain, with no place of its own to \
             light from. Light a part of an item beside the road instead.",
        );
        return;
    }
    if let Some(refusal) = refusal {
        weak(ui, refusal);
        if light.is_none() {
            return;
        }
    }

    let mut on = light.is_some();
    if ui
        .checkbox(&mut on, "Light this part")
        .on_hover_text(
            "Every visitor's client draws a light at this part, lighting the \
             ground and the parts round it - but not on the copies of an \
             instance set, which draw an item's shapes alone. A glowing \
             material alone lights nothing.",
        )
        .changed()
    {
        *light = on.then(LightSource::default);
        *dirty = true;
    }
    let Some(current) = light.as_mut() else {
        weak(
            ui,
            "A lamp, a lantern, a torch's flame, a lighthouse's beam.",
        );
        return;
    };

    // What the owner asked, where the light is still what that asking left.
    let memory = ui.id().with("asked");
    let mut asked = ui
        .data(|d| d.get_temp::<Asked>(memory))
        .filter(|asked| asked.held == *current)
        .unwrap_or_else(|| Asked::of(current));
    let mut edit = current.clone();
    let mut changed = false;
    draw_shape(ui, &mut edit, &mut changed);
    draw_look(ui, &mut edit, &mut changed);
    draw_motion(ui, &mut edit, &mut changed);
    if changed {
        // The owner's change, onto what they asked before; then held as
        // every peer will hold it: bounded, on the grid, as bright as its
        // reach allows and a rate no faster than its depth may flash.
        asked.take(current, &edit);
        let mut held = asked.light.clone();
        held.sanitize();
        // Unchanged as a peer holds it (a colour's round trip through the
        // picker, a number set a hair off the grid), nothing is written.
        if held != *current {
            *current = held;
            *dirty = true;
        }
        asked.held = current.clone();
        ui.data_mut(|d| d.insert_temp(memory, asked));
    }

    weak(
        ui,
        "Lit from dusk to dawn, it shows only at night - untick that to see it \
         by day while you set it up. Each visitor's client draws only the \
         nearest lights: fewer at Effects Reduced, none at Off. Lights cast no \
         shadows. A flicker or pulse keeps going while the part is selected, \
         where a spin pauses. Older versions of Overlands draw no light.",
    );
}

/// Point or spot, and a spot's aim and cone.
fn draw_shape(ui: &mut egui::Ui, light: &mut LightSource, changed: &mut bool) {
    ui.horizontal(|ui| {
        ui.label("Kind");
        let point = matches!(light.shape, LightShape::Point);
        if ui
            .selectable_label(point, "Point")
            .on_hover_text("Shines every way from its point, like a bulb.")
            .clicked()
            && !point
        {
            light.shape = LightShape::Point;
            *changed = true;
        }
        let spot = matches!(light.shape, LightShape::Spot { .. });
        if ui
            .selectable_label(spot, "Spot")
            .on_hover_text(
                "Shines in a cone: a streetlight's head, a stage light, a \
                 lighthouse's beam.",
            )
            .clicked()
            && !spot
        {
            light.shape = LightShape::fresh_spot();
            *changed = true;
        }
    });
    match &mut light.shape {
        LightShape::Point => {}
        LightShape::Spot { aim, inner, outer } => {
            draw_aim(ui, aim, changed);
            // Read before either slider: the inner cone stops at the outer
            // one as it stood when the frame began.
            let edge = outer.0;
            slider(
                ui,
                "Inner cone",
                inner,
                0.0..=edge,
                "°",
                changed,
                "Half the cone's width, from the aim to its edge, inside which \
                 the light is full.",
            );
            slider(
                ui,
                "Outer cone",
                outer,
                limits::MIN_SPOT_ANGLE_DEG..=limits::MAX_SPOT_ANGLE_DEG,
                "°",
                changed,
                "Half the cone's width where the light has faded to nothing. \
                 The inner cone stays inside it.",
            );
        }
        LightShape::Unknown => {
            unrecognised_value_line(ui, "light kind", Some("it draws no light here"));
        }
    }
}

/// A spot's aim: a named direction of the node's own frame, or any
/// direction typed in.
fn draw_aim(ui: &mut egui::Ui, aim: &mut Fp3, changed: &mut bool) {
    let v = bevy::math::Vec3::from_array(aim.0).normalize_or_zero();
    // A zero aim points down, as the drawing reads it.
    let v = if v == bevy::math::Vec3::ZERO {
        bevy::math::Vec3::NEG_Y
    } else {
        v
    };
    ui.horizontal_wrapped(|ui| {
        ui.label("Aim");
        for (name, along) in AIMS {
            let selected = v.dot(bevy::math::Vec3::from_array(along)) > 0.999;
            if ui.selectable_label(selected, name).clicked() && !selected {
                *aim = Fp3(along);
                *changed = true;
            }
        }
    });
    egui::CollapsingHeader::new("Any aim")
        .default_open(false)
        .show(ui, |ui| {
            let mut v = aim.0;
            let mut moved = false;
            ui.horizontal(|ui| {
                for c in v.iter_mut() {
                    moved |= ui
                        .add(
                            crate::ui::num::drag(c)
                                .speed(0.01)
                                .range(-1.0..=1.0)
                                .max_decimals(GRID_DECIMALS),
                        )
                        .changed();
                }
            });
            if moved {
                *aim = Fp3(v);
                *changed = true;
            }
            weak(
                ui,
                "In the part's own frame, after its turn: a head tilted by its \
                 part shines where it points. Only the direction counts.",
            );
        });
}

/// Colour, brightness, reach, size, offset and dusk to dawn.
fn draw_look(ui: &mut egui::Ui, light: &mut LightSource, changed: &mut bool) {
    ui.horizontal(|ui| {
        ui.label("Colour");
        // A copy: egui's swatch writes its own round trip back.
        let mut srgb = light.color.0;
        if edit_srgb_rgb(ui, &mut srgb).changed() {
            light.color = Fp3(srgb);
            *changed = true;
        }
    });
    slider(
        ui,
        "Reach",
        &mut light.range,
        limits::MIN_LIGHT_RANGE_M..=limits::MAX_LIGHT_RANGE_M,
        " m",
        changed,
        "How far the light reaches before it has fallen to nothing. Give it \
         the reach it needs, not the most: long lights close together take \
         more of what a browser can draw, so fewer of them show at once. A \
         lamp high on a post needs a reach of about twice its height to \
         light a pool on the ground under it, and a spot raked to one side \
         about twice the distance along its aim to the ground.",
    );
    // The top for the reach as it stands after this frame's drag: no light
    // is stronger than the brightest of the longest reach.
    let cap = max_light_brightness(light.range.0);
    slider(
        ui,
        "Brightness",
        &mut light.brightness,
        0.0..=cap,
        "",
        changed,
        "1 is a lamp's: it lights what stands four fifths of its reach away \
         to a lamp's working light, whatever the reach. A lamp high on a post \
         lights the ground from further off and needs more: a head up to 5 m \
         with twice its height in reach (twice the distance along its aim, \
         for a raked spot) reads clearly at the most its reach allows; a \
         taller one is held by its power and draws a wider, fainter pool. A \
         lamp near the ground reads well at 1 to 8.",
    );
    let note = if cap < limits::MAX_LIGHT_BRIGHTNESS {
        format!(
            "At most {cap} at this reach: a longer reach is a stronger light, \
             and no light may be stronger than {} at {} m.",
            limits::MAX_LIGHT_BRIGHTNESS_AT_MAX_RANGE,
            limits::MAX_LIGHT_RANGE_M
        )
    } else {
        format!("At most {cap} at this reach.")
    };
    weak(ui, &note);
    slider(
        ui,
        "Size",
        &mut light.size,
        0.0..=limits::MAX_LIGHT_SIZE_M,
        " m",
        changed,
        "The light's own radius: a bigger source draws softer, wider \
         highlights on what it shines on.",
    );
    ui.horizontal(|ui| {
        ui.label("Offset")
            .on_hover_text("Where the light sits, in the part's own frame (m).");
        let reach = limits::MAX_LIGHT_OFFSET_M;
        let mut v = light.offset.0;
        let mut moved = false;
        for c in v.iter_mut() {
            moved |= ui
                .add(
                    crate::ui::num::drag(c)
                        .speed(0.01)
                        .range(-reach..=reach)
                        .max_decimals(GRID_DECIMALS),
                )
                .changed();
        }
        if moved {
            light.offset = Fp3(v);
            *changed = true;
        }
    });
    if ui
        .checkbox(&mut light.dusk_to_dawn, "Dusk to dawn")
        .on_hover_text(
            "Lit only while the world is dark, as a glow of that name is. \
             Untick it for a light that shines at every hour.",
        )
        .changed()
    {
        *changed = true;
    }
}

/// Steady, a flicker or a pulse, with its depth and rate.
fn draw_motion(ui: &mut egui::Ui, light: &mut LightSource, changed: &mut bool) {
    ui.horizontal(|ui| {
        ui.label("Motion");
        let steady = light.flicker.is_none();
        if ui.selectable_label(steady, "Steady").clicked() && !steady {
            light.flicker = None;
            *changed = true;
        }
        for kind in LightFlicker::KINDS {
            let selected = light.flicker.as_ref().map(LightFlicker::label) == Some(kind);
            if ui
                .selectable_label(selected, kind)
                .on_hover_text(motion_help(kind))
                .clicked()
                && !selected
            {
                light.flicker = LightFlicker::fresh(kind);
                *changed = true;
            }
        }
    });
    let pulse = matches!(light.flicker, Some(LightFlicker::Pulse { .. }));
    match light.flicker.as_mut() {
        None => {}
        Some(LightFlicker::Flicker { amount, rate, .. } | LightFlicker::Pulse { amount, rate }) => {
            slider(
                ui,
                "Depth",
                amount,
                0.0..=1.0,
                "",
                changed,
                "The share of its light it dips by at most: 1 goes dark. Deeper \
                 than a tenth it flashes, and may flash at most three times a \
                 second; a flicker's shimmer, a third of its dip, flashes deeper \
                 than 0.3.",
            );
            // The cap for the depth as it stands after this frame's drag: a
            // deeper light flashes, and may flash at most three times a
            // second.
            let cap = max_flicker_rate_hz(pulse, amount.0);
            let mut rate_changed = false;
            slider_log(
                ui,
                "Rate",
                rate,
                limits::MIN_LIGHT_FLICKER_RATE_HZ..=cap,
                " Hz",
                &mut rate_changed,
                if pulse {
                    "How many times a second it breathes: up to 8, and at most 3 \
                     once it dips by more than a tenth, which flashes."
                } else {
                    "How many times a second its broad waver moves: up to 8, at \
                     most 3 once the waver (two thirds of the dip) passes a tenth, \
                     which flashes, and at most 1.1111 once the shimmer does too, \
                     moving 2.7 times quicker."
                },
            );
            *changed |= rate_changed;
            let note = if cap < limits::MAX_LIGHT_FLICKER_RATE_HZ {
                format!(
                    "At most {cap} Hz at this depth: a light that dips by more \
                     than a tenth flashes, and may flash at most three times a \
                     second."
                )
            } else {
                format!("At most {cap} Hz at this depth.")
            };
            weak(ui, &note);
        }
        Some(LightFlicker::Unknown) => {
            unrecognised_value_line(ui, "light motion", Some("it shines steady here"));
        }
    }
    if let Some(LightFlicker::Flicker {
        amount,
        rate,
        shimmer_rate,
    }) = light.flicker.as_mut()
    {
        draw_shimmer(ui, *amount, *rate, shimmer_rate, changed);
    }
}

/// A flicker's shimmer on a rate of its own or coupled to its waver
/// (#1950): its own only while it is too shallow to flash.
fn draw_shimmer(
    ui: &mut egui::Ui,
    amount: Fp,
    rate: Fp,
    shimmer_rate: &mut Option<Fp>,
    changed: &mut bool,
) {
    // Why the box is greyed past 0.3: painted under it, and its hover there,
    // as egui shows a greyed control's hover only through
    // `on_disabled_hover_text`.
    const NO_OWN_RATE: &str = "Deeper than 0.3 the shimmer flashes too, so it moves 2.7 times \
                               the Rate, held with it: no own shimmer rate at this depth.";
    let fastest = max_shimmer_rate_hz(amount.0);
    let coupled = rate.0 * FLICKER_FINE_RATE;
    let mut own = shimmer_rate.is_some();
    if ui
        .add_enabled(
            fastest.is_some(),
            egui::Checkbox::new(&mut own, "Own shimmer rate"),
        )
        .on_hover_text(
            "Moves the shimmer - a third of the dip, over the broad waver - on \
             a rate of its own, up to 8 a second, rather than 2.7 times the \
             Rate: a fire's quick flutter over a slow swell. Only a shimmer \
             too shallow to flash may (a depth of 0.3 or less). Older \
             versions of Overlands move it 2.7 times the Rate.",
        )
        .on_disabled_hover_text(NO_OWN_RATE)
        .changed()
    {
        *shimmer_rate = own.then(|| {
            Fp(coupled.clamp(
                limits::MIN_LIGHT_FLICKER_RATE_HZ,
                fastest.unwrap_or(limits::MAX_LIGHT_FLICKER_RATE_HZ),
            ))
        });
        *changed = true;
    }
    match (fastest, shimmer_rate.as_mut()) {
        (Some(top), Some(shimmer)) => slider_log(
            ui,
            "Shimmer",
            shimmer,
            limits::MIN_LIGHT_FLICKER_RATE_HZ..=top,
            " Hz",
            changed,
            "How many times a second the shimmer moves: up to 8, whatever the \
             waver's Rate - it never dips past a tenth, so it never flashes.",
        ),
        (Some(_), None) => weak(
            ui,
            &format!("The shimmer moves 2.7 times the Rate: {coupled:.2} Hz."),
        ),
        (None, _) => weak(ui, NO_OWN_RATE),
    }
}

/// What the owner asked of a light, where the light holds something else
/// (#1599's lesson, `ui::room::environment`'s `AskedMiddle`): a brightness
/// over what its reach allows, an inner cone wider than the outer, a rate
/// faster than its depth may flash - each held by the sanitiser - and the
/// spot and the motions the light has been switched away from. Remembered
/// only while the light is still the one that asking left (`held`), so a
/// Reach or Depth dragged past a limit and back, or an Outer cone narrowed
/// and widened again, gives the owner back what they had, as does Point
/// to Spot and back, or Flicker to Pulse and back; an undo, or any other
/// change from outside the panel, forgets it.
#[derive(Clone, Debug, PartialEq)]
struct Asked {
    /// The light as the asking left it, sanitised.
    held: LightSource,
    /// The light as asked: every value as the owner set it.
    light: LightSource,
    /// The spot last asked, while the light is a point.
    spot: Option<LightShape>,
    /// The flicker and the pulse last asked, while the light is another.
    motions: Vec<LightFlicker>,
}

impl Asked {
    /// Nothing asked yet beyond `light` itself.
    fn of(light: &LightSource) -> Self {
        Self {
            held: light.clone(),
            light: light.clone(),
            spot: None,
            motions: Vec::new(),
        }
    }

    /// Take what the owner changed this frame - `edit` against `before`,
    /// the light it was drawn over - onto the light as asked.
    fn take(&mut self, before: &LightSource, edit: &LightSource) {
        fn moved<T: PartialEq + Copy>(asked: &mut T, before: T, edit: T) {
            if edit != before {
                *asked = edit;
            }
        }
        let asked = &mut self.light;
        if let (
            LightShape::Spot {
                aim: a0,
                inner: i0,
                outer: o0,
            },
            LightShape::Spot { aim, inner, outer },
            LightShape::Spot {
                aim: a,
                inner: i,
                outer: o,
            },
        ) = (&before.shape, &edit.shape, &mut asked.shape)
        {
            moved(a, *a0, *aim);
            moved(i, *i0, *inner);
            moved(o, *o0, *outer);
        } else if std::mem::discriminant(&before.shape) != std::mem::discriminant(&edit.shape) {
            if matches!(asked.shape, LightShape::Spot { .. }) {
                self.spot = Some(asked.shape.clone());
            }
            asked.shape = match (&edit.shape, &self.spot) {
                (LightShape::Spot { .. }, Some(spot)) => spot.clone(),
                (now, _) => now.clone(),
            };
        }
        moved(&mut asked.color, before.color, edit.color);
        moved(&mut asked.brightness, before.brightness, edit.brightness);
        moved(&mut asked.range, before.range, edit.range);
        moved(&mut asked.size, before.size, edit.size);
        moved(&mut asked.offset, before.offset, edit.offset);
        moved(
            &mut asked.dusk_to_dawn,
            before.dusk_to_dawn,
            edit.dusk_to_dawn,
        );
        let label = |f: &Option<LightFlicker>| f.as_ref().map(LightFlicker::label);
        if label(&before.flicker) != label(&edit.flicker) {
            if let Some(gone) = asked.flicker.take() {
                self.motions.retain(|m| m.label() != gone.label());
                self.motions.push(gone);
            }
            asked.flicker = edit.flicker.as_ref().map(|now| {
                self.motions
                    .iter()
                    .find(|m| m.label() == now.label())
                    .cloned()
                    .unwrap_or_else(|| now.clone())
            });
        } else {
            if let (
                Some(
                    LightFlicker::Flicker {
                        amount: m0,
                        rate: r0,
                        ..
                    }
                    | LightFlicker::Pulse {
                        amount: m0,
                        rate: r0,
                    },
                ),
                Some(
                    LightFlicker::Flicker { amount, rate, .. }
                    | LightFlicker::Pulse { amount, rate },
                ),
                Some(
                    LightFlicker::Flicker {
                        amount: m, rate: r, ..
                    }
                    | LightFlicker::Pulse { amount: m, rate: r },
                ),
            ) = (&before.flicker, &edit.flicker, &mut asked.flicker)
            {
                moved(m, *m0, *amount);
                moved(r, *r0, *rate);
            }
            // A shimmer rate the sanitiser dropped for a depth past 0.3
            // comes back when the depth does.
            if let (
                Some(LightFlicker::Flicker {
                    shimmer_rate: s0, ..
                }),
                Some(LightFlicker::Flicker { shimmer_rate, .. }),
                Some(LightFlicker::Flicker {
                    shimmer_rate: s, ..
                }),
            ) = (&before.flicker, &edit.flicker, &mut asked.flicker)
            {
                moved(s, *s0, *shimmer_rate);
            }
        }
    }
}

/// What each motion does, for its button.
fn motion_help(kind: &str) -> &'static str {
    match kind {
        "Flicker" => "An uneven waver with a quicker shimmer over it: a flame, a failing tube.",
        "Pulse" => "An even breathing, dimming and brightening: a beacon, a warning lamp.",
        _ => "",
    }
}

/// A weak note.
fn weak(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .small()
            .color(crate::ui::theme::current(ui.ctx()).text_weak),
    );
}

/// A labelled slider over one of the light's numbers, showing the grid's
/// decimals.
fn slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Fp,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    changed: &mut bool,
    hover: &str,
) {
    let mut v = value.0;
    let response = ui
        .add(
            crate::ui::num::slider(&mut v, range)
                .text(label)
                .suffix(suffix)
                .max_decimals(GRID_DECIMALS),
        )
        .on_hover_text(hover);
    if response.changed() {
        *value = Fp(v);
        *changed = true;
    }
}

/// [`slider`] on a logarithmic track, for a rate whose useful values crowd
/// its low end.
fn slider_log(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Fp,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    changed: &mut bool,
    hover: &str,
) {
    let mut v = value.0;
    let response = ui
        .add(
            crate::ui::num::slider(&mut v, range)
                .logarithmic(true)
                .text(label)
                .suffix(suffix)
                .max_decimals(GRID_DECIMALS),
        )
        .on_hover_text(hover);
    if response.changed() {
        *value = Fp(v);
        *changed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::headless::Headless as _;
    use egui::accesskit::{Action, ActionData, ActionRequest, Node, NodeId, Role, TreeId};

    /// One widget a pass drew, as AccessKit reads it, in document order.
    #[derive(Debug)]
    struct Shown {
        id: NodeId,
        role: Role,
        label: String,
        value: String,
        /// `(value, min, max)` for a numeric widget.
        range: Option<(f64, f64, f64)>,
        /// Where it was drawn.
        bounds: Option<egui::Rect>,
    }

    /// What one pass of the section drew, and whether it marked the record
    /// edited.
    struct Pass {
        shown: Vec<Shown>,
        dirty: bool,
        /// The widget holding the keyboard (the window when none does).
        focus: NodeId,
    }

    impl Pass {
        /// The interactive widget named `name` (a slider, a checkbox, a
        /// button), whatever its role.
        fn named(&self, name: &str) -> &Shown {
            self.shown
                .iter()
                .find(|s| s.label == name && !matches!(s.role, Role::Label | Role::TextRun))
                .unwrap_or_else(|| panic!("no widget {name:?} in {:#?}", self.shown))
        }

        /// The number boxes whose range is `lo..=hi`: an offset's or an
        /// aim's three, which carry no name of their own.
        fn boxes(&self, lo: f64, hi: f64) -> Vec<NodeId> {
            self.shown
                .iter()
                .filter(|s| {
                    s.role == Role::SpinButton
                        && s.range.is_some_and(|(_, min, max)| min == lo && max == hi)
                })
                .map(|s| s.id)
                .collect()
        }

        fn painted(&self, start: &str) -> bool {
            self.shown
                .iter()
                .any(|s| s.label.starts_with(start) || s.value.starts_with(start))
        }
    }

    /// A context with AccessKit on; `open` draws every folded section, and
    /// every popup with it (the colour picker's).
    fn context(open: bool) -> egui::Context {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.memory_mut(|m| m.set_everything_is_visible(open));
        ctx
    }

    /// Draw the section over `light` once, `events` as the input.
    fn pass(
        ctx: &egui::Context,
        light: &mut Option<LightSource>,
        events: Vec<egui::Event>,
    ) -> Pass {
        pass_kind(ctx, light, &GeneratorKind::default_cuboid(), events)
    }

    fn pass_kind(
        ctx: &egui::Context,
        light: &mut Option<LightSource>,
        kind: &GeneratorKind,
        events: Vec<egui::Event>,
    ) -> Pass {
        let mut dirty = false;
        let output = ctx.run_headless(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 2400.0),
                )),
                events,
                ..Default::default()
            },
            |ui| draw_light_section(ui, light, kind, None, "lamp", &mut dirty),
        );
        let update = output
            .platform_output
            .accesskit_update
            .expect("AccessKit is on");
        let by_id: std::collections::HashMap<NodeId, &Node> =
            update.nodes.iter().map(|(id, n)| (*id, n)).collect();
        // Depth first from the root: document order.
        let mut shown = Vec::new();
        let mut stack = vec![update.tree.as_ref().expect("a tree").root];
        while let Some(id) = stack.pop() {
            let Some(node) = by_id.get(&id) else { continue };
            if node.role() != Role::TextRun {
                shown.push(Shown {
                    id,
                    role: node.role(),
                    label: node.label().unwrap_or_default().to_string(),
                    value: node.value().unwrap_or_default().to_string(),
                    range: node
                        .numeric_value()
                        .zip(node.min_numeric_value())
                        .zip(node.max_numeric_value())
                        .map(|((v, lo), hi)| (v, lo, hi)),
                    bounds: node.bounds().map(|b| {
                        egui::Rect::from_min_max(
                            egui::pos2(b.x0 as f32, b.y0 as f32),
                            egui::pos2(b.x1 as f32, b.y1 as f32),
                        )
                    }),
                });
            }
            stack.extend(node.children().iter().rev().copied());
        }
        Pass {
            shown,
            dirty,
            focus: update.focus,
        }
    }

    fn request(id: NodeId, action: Action, data: Option<ActionData>) -> Vec<egui::Event> {
        vec![egui::Event::AccessKitActionRequest(ActionRequest {
            action,
            target_tree: TreeId::ROOT,
            target_node: id,
            data,
        })]
    }

    fn click(id: NodeId) -> Vec<egui::Event> {
        request(id, Action::Click, None)
    }

    /// Set the numeric widget `id` to `value`, as a typed entry does.
    fn set(id: NodeId, value: f64) -> Vec<egui::Event> {
        request(id, Action::SetValue, Some(ActionData::NumericValue(value)))
    }

    fn sanitized(light: &LightSource) -> LightSource {
        let mut again = light.clone();
        again.sanitize();
        again
    }

    fn on_grid(v: f32) -> bool {
        Fp(v).on_wire().0 == v
    }

    /// The section adds a light, makes it a spot, edits it and takes it
    /// off again, marking the record edited each time.
    #[test]
    fn the_section_adds_edits_and_removes_a_light() {
        let ctx = context(false);
        let mut light = None;
        let drawn = pass(&ctx, &mut light, Vec::new());
        assert!(!drawn.dirty && light.is_none(), "drawing adds nothing");
        let on = drawn.named("Light this part").id;

        let added = pass(&ctx, &mut light, click(on));
        assert!(added.dirty);
        assert_eq!(light, Some(LightSource::default()), "a fresh lamp");

        let spot = added.named("Spot").id;
        assert!(pass(&ctx, &mut light, click(spot)).dirty);
        assert_eq!(light, Some(LightSource::spot()));

        let drawn = pass(&ctx, &mut light, Vec::new());
        let brightness = drawn.named("Brightness").id;
        assert!(pass(&ctx, &mut light, set(brightness, 3.0)).dirty);
        assert_eq!(light.as_ref().map(|l| l.brightness), Some(Fp(3.0)));

        let pulse = pass(&ctx, &mut light, Vec::new()).named("Pulse").id;
        pass(&ctx, &mut light, click(pulse));
        assert_eq!(
            light.as_ref().and_then(|l| l.flicker.clone()),
            LightFlicker::fresh("Pulse")
        );

        let off = pass(&ctx, &mut light, Vec::new())
            .named("Light this part")
            .id;
        assert!(pass(&ctx, &mut light, click(off)).dirty);
        assert_eq!(light, None, "unticked, the node carries no light");
    }

    /// Every control writes inside the sanitiser's bounds and on the wire's
    /// grid, whatever is typed into it: the light it leaves is the light a
    /// peer decodes, so the sanitiser has nothing left to move.
    #[test]
    fn every_control_writes_on_the_grid_inside_its_bounds() {
        let ctx = context(true);
        let mut light = Some(LightSource {
            flicker: LightFlicker::fresh("Pulse"),
            ..LightSource::spot()
        });
        let typed = [2.345_678, -7.0, 123.456_789, 0.000_04, 17.777_77];
        for name in [
            "Brightness",
            "Reach",
            "Size",
            "Inner cone",
            "Outer cone",
            "Depth",
            "Rate",
        ] {
            for value in typed {
                let id = pass(&ctx, &mut light, Vec::new()).named(name).id;
                pass(&ctx, &mut light, set(id, value));
                let held = light.as_ref().expect("a light");
                assert_eq!(&sanitized(held), held, "{name} = {value}");
            }
        }
        let drawn = pass(&ctx, &mut light, Vec::new());
        let offsets = drawn.boxes(
            -f64::from(limits::MAX_LIGHT_OFFSET_M),
            f64::from(limits::MAX_LIGHT_OFFSET_M),
        );
        let aims = drawn.boxes(-1.0, 1.0);
        assert_eq!((offsets.len(), aims.len()), (3, 3), "{:#?}", drawn.shown);
        for (id, value) in offsets.into_iter().zip([12.345_678, -99.0, 0.000_06]) {
            assert!(pass(&ctx, &mut light, set(id, value)).dirty);
        }
        for (id, value) in aims.into_iter().zip([0.333_333, -0.666_667, 0.123_456]) {
            assert!(pass(&ctx, &mut light, set(id, value)).dirty);
        }
        let held = light.expect("a light");
        assert_eq!(sanitized(&held), held);
        assert_eq!(
            held.offset,
            Fp3([12.3457, -limits::MAX_LIGHT_OFFSET_M, 0.0001])
        );
        let LightShape::Spot { aim, inner, outer } = held.shape else {
            panic!("a spot: {held:?}");
        };
        assert_eq!(aim, Fp3([0.3333, -0.6667, 0.1235]));
        assert!(inner.0 <= outer.0, "the inner cone inside the outer");
        let Some(LightFlicker::Pulse { amount, rate }) = held.flicker else {
            panic!("a pulse: {held:?}");
        };
        for v in [
            held.brightness.0,
            held.range.0,
            held.size.0,
            inner.0,
            outer.0,
            amount.0,
            rate.0,
        ] {
            assert!(on_grid(v), "{v} off the grid in {held:?}");
        }
        assert!(held.range.0 >= limits::MIN_LIGHT_RANGE_M);
        assert!(held.brightness.0 <= limits::MAX_LIGHT_BRIGHTNESS);
        assert!(rate.0 <= max_flicker_rate_hz(true, amount.0));
    }

    /// The rate offers no more than the depth's flash cap and says what the
    /// cap is; a depth deepened past what the rate may flash at slows the
    /// rate here, so nothing the panel writes is slowed by the sanitiser.
    #[test]
    fn the_rate_shows_the_cap_for_its_depth_and_never_writes_past_it() {
        let ctx = context(false);
        let mut light = Some(LightSource {
            dusk_to_dawn: false,
            flicker: LightFlicker::fresh("Pulse"),
            ..LightSource::default()
        });
        let top = |pass: &Pass| pass.named("Rate").range.expect("a slider").2 as f32;
        let drawn = pass(&ctx, &mut light, Vec::new());
        assert_eq!(top(&drawn), limits::MAX_LIGHT_FLASHES_HZ, "a 0.6 pulse");
        assert!(
            drawn.painted("At most 3 Hz at this depth"),
            "{:#?}",
            drawn.shown
        );

        let shallow = pass(&ctx, &mut light, set(drawn.named("Depth").id, 0.05));
        assert_eq!(top(&shallow), limits::MAX_LIGHT_FLICKER_RATE_HZ);
        pass(&ctx, &mut light, set(shallow.named("Rate").id, 8.0));
        let rate = |light: &Option<LightSource>| match light
            .as_ref()
            .and_then(|l| l.flicker.clone())
        {
            Some(LightFlicker::Pulse { rate, .. } | LightFlicker::Flicker { rate, .. }) => rate.0,
            other => panic!("no flicker: {other:?}"),
        };
        assert_eq!(rate(&light), 8.0, "a shallow pulse may be quick");

        let drawn = pass(&ctx, &mut light, Vec::new());
        let deep = pass(&ctx, &mut light, set(drawn.named("Depth").id, 1.0));
        assert!(deep.dirty);
        assert_eq!(
            rate(&light),
            limits::MAX_LIGHT_FLASHES_HZ,
            "slowed in the panel"
        );
        let held = light.clone().expect("a light");
        assert_eq!(sanitized(&held), held, "the sanitiser slows it no further");

        // A deep flicker's shimmer flashes too: its cap is lower.
        let flicker = pass(&ctx, &mut light, Vec::new()).named("Flicker").id;
        pass(&ctx, &mut light, click(flicker));
        let drawn = pass(&ctx, &mut light, Vec::new());
        let deep = pass(&ctx, &mut light, set(drawn.named("Depth").id, 1.0));
        let cap = max_flicker_rate_hz(false, 1.0);
        assert_eq!(top(&deep), cap);
        assert_eq!(rate(&light), cap.min(4.0));
        let again = pass(&ctx, &mut light, Vec::new());
        assert!(again.painted(&format!("At most {cap} Hz at this depth")));
        let held = light.expect("a light");
        assert_eq!(sanitized(&held), held);
    }

    /// A flicker's shimmer may take a rate of its own (#1950): ticked, it
    /// starts at the coupled 2.7 times the Rate and goes up to 8 Hz beside
    /// a waver held to three a second; a depth past 0.3, where the shimmer
    /// flashes, drops it - the box disabled, the reason painted - and a
    /// depth brought back gives it back; unticked, it is coupled again.
    #[test]
    fn a_flickers_shimmer_takes_its_own_rate_while_it_is_shallow() {
        let ctx = context(false);
        let mut light = Some(LightSource {
            dusk_to_dawn: false,
            flicker: LightFlicker::fresh("Flicker"),
            ..LightSource::default()
        });
        let shimmer = |light: &Option<LightSource>| match held(light).flicker {
            Some(LightFlicker::Flicker { shimmer_rate, .. }) => shimmer_rate.map(|s| s.0),
            ref other => panic!("no flicker: {other:?}"),
        };
        let drawn = pass(&ctx, &mut light, Vec::new());
        assert!(drawn.painted("The shimmer moves 2.7 times the Rate"));
        edit(&ctx, &mut light, "Depth", 0.3);
        edit(&ctx, &mut light, "Rate", 1.0);
        let own = pass(&ctx, &mut light, Vec::new())
            .named("Own shimmer rate")
            .id;
        assert!(pass(&ctx, &mut light, click(own)).dirty);
        assert_eq!(shimmer(&light), Some(2.7), "starts where it was");
        assert!(edit(&ctx, &mut light, "Shimmer", 8.0));
        assert!(!edit(&ctx, &mut light, "Shimmer", 12.0), "at most 8 Hz");
        edit(&ctx, &mut light, "Rate", 5.0);
        assert_eq!(motion(&light), (0.3, limits::MAX_LIGHT_FLASHES_HZ));
        assert_eq!(shimmer(&light), Some(8.0), "the flutter over the waver");
        let held_light = held(&light).clone();
        assert_eq!(sanitized(&held_light), held_light);

        edit(&ctx, &mut light, "Depth", 0.5);
        assert_eq!(shimmer(&light), None, "a shimmer that flashes is coupled");
        let deep = pass(&ctx, &mut light, Vec::new());
        assert!(deep.painted("Deeper than 0.3 the shimmer flashes"));
        assert!(!pass(&ctx, &mut light, click(deep.named("Own shimmer rate").id)).dirty);
        assert_eq!(shimmer(&light), None, "the box is disabled");

        edit(&ctx, &mut light, "Depth", 0.3);
        assert_eq!(shimmer(&light), Some(8.0), "the shimmer rate asked");
        let own = pass(&ctx, &mut light, Vec::new())
            .named("Own shimmer rate")
            .id;
        assert!(pass(&ctx, &mut light, click(own)).dirty);
        assert_eq!(shimmer(&light), None, "unticked, coupled again");
    }

    /// Drawn with no input, the section writes nothing and marks nothing
    /// edited - every folded section and popup open, the colour picker's
    /// round trip through its own colour space among them - for a point,
    /// a spot, each motion and the kinds a newer version may write.
    #[test]
    fn drawing_the_section_writes_nothing() {
        let lights = [
            LightSource::default(),
            LightSource {
                color: Fp3([0.07, 0.4321, 0.9]),
                offset: Fp3([0.1234, -2.5, 3.0]),
                flicker: LightFlicker::fresh("Flicker"),
                ..LightSource::spot()
            },
            LightSource {
                dusk_to_dawn: false,
                shape: LightShape::Spot {
                    aim: Fp3([0.3333, -0.6667, 0.1235]),
                    inner: Fp(12.3456),
                    outer: Fp(33.3333),
                },
                flicker: LightFlicker::fresh("Pulse"),
                ..LightSource::default()
            },
            LightSource {
                flicker: Some(LightFlicker::Flicker {
                    amount: Fp(0.3),
                    rate: Fp(3.0),
                    shimmer_rate: Some(Fp(8.0)),
                }),
                ..LightSource::default()
            },
            LightSource {
                shape: LightShape::Unknown,
                flicker: Some(LightFlicker::Unknown),
                ..LightSource::default()
            },
        ];
        for open in [false, true] {
            for start in &lights {
                let ctx = context(open);
                let mut light = Some(start.clone());
                for _ in 0..3 {
                    let drawn = pass(&ctx, &mut light, Vec::new());
                    drawn.named("Light this part");
                    assert!(!drawn.dirty, "open {open}: {start:?}");
                }
                assert_eq!(light.as_ref(), Some(start), "open {open}");
            }
        }
    }

    /// A live edit does not drift: what the panel writes is what the
    /// editor's flush settles to (`settle_edit`, the room as its peers hold
    /// it), bit for bit, and drawing it again writes nothing.
    #[test]
    fn a_live_edit_round_trip_does_not_drift() {
        let mut record = crate::pds::RoomRecord::default();
        let mut lamp = crate::pds::Generator::default_cuboid();
        lamp.light = Some(LightSource::spot());
        record.generators.insert("lamp".into(), lamp);
        let ctx = context(true);
        let light = |record: &mut crate::pds::RoomRecord| {
            record
                .generators
                .get_mut("lamp")
                .map(|g| &mut g.light)
                .expect("the lamp")
                .clone()
        };
        let mut held = light(&mut record);
        for (name, value) in [
            ("Brightness", 2.468_135_7),
            ("Reach", 7.777_777),
            ("Size", 0.123_456),
            ("Outer cone", 33.333_33),
            ("Inner cone", 12.345_67),
        ] {
            let id = pass(&ctx, &mut held, Vec::new()).named(name).id;
            pass(&ctx, &mut held, set(id, value));
        }
        let drawn = pass(&ctx, &mut held, Vec::new());
        let offsets = drawn.boxes(
            -f64::from(limits::MAX_LIGHT_OFFSET_M),
            f64::from(limits::MAX_LIGHT_OFFSET_M),
        );
        for (id, value) in offsets.into_iter().zip([1.111_11, -2.222_22, 3.333_33]) {
            pass(&ctx, &mut held, set(id, value));
        }
        let written = held.clone();
        assert_ne!(written, Some(LightSource::spot()), "control: it was edited");
        record.generators.get_mut("lamp").expect("lamp").light = written.clone();

        crate::ui::room::settle_edit(&mut record);
        assert_eq!(light(&mut record), written, "the flush moved the light");
        let mut again = light(&mut record);
        assert!(!pass(&ctx, &mut again, Vec::new()).dirty);
        assert_eq!(again, written);
        crate::ui::room::settle_edit(&mut record);
        assert_eq!(light(&mut record), written);
    }

    /// Undo puts back the light as it stood before an edit, and redo the
    /// edit: the history holds the whole record, the light with it.
    #[test]
    fn undo_restores_the_previous_light() {
        use crate::ui::undo::{Observation, UndoHistory};
        let mut record = crate::pds::RoomRecord::default();
        record
            .generators
            .insert("lamp".into(), crate::pds::Generator::default_cuboid());
        let mut history = UndoHistory::<crate::pds::RoomRecord, ()>::default();
        history.reset(Some("did:test:room"), record.clone(), ());
        let ctx = context(false);
        let mut edit = |record: &mut crate::pds::RoomRecord,
                        name: &str,
                        act: &dyn Fn(NodeId) -> Vec<egui::Event>| {
            let light = &mut record.generators.get_mut("lamp").expect("lamp").light;
            let id = pass(&ctx, light, Vec::new()).named(name).id;
            assert!(pass(&ctx, light, act(id)).dirty, "{name}");
            crate::ui::room::settle_edit(record);
            history.observe(
                Some("did:test:room"),
                Observation::Edit(name.into()),
                || record.clone(),
                || (),
            );
        };
        edit(&mut record, "Light this part", &click);
        edit(&mut record, "Brightness", &|id| set(id, 4.5));
        let lit = record.generators["lamp"].light.clone();
        assert_eq!(lit.as_ref().map(|l| l.brightness), Some(Fp(4.5)));

        let (entry, _) = history.undo().expect("an edit to undo");
        assert_eq!(
            entry.record.generators["lamp"].light,
            Some(LightSource::default()),
            "the light before the brightness edit"
        );
        let (entry, _) = history.undo().expect("the add to undo");
        assert_eq!(entry.record.generators["lamp"].light, None);
        history.redo();
        let (entry, _) = history.redo().expect("the edit to redo");
        assert_eq!(entry.record.generators["lamp"].light, lit);
    }

    /// Set the slider `name` to `value` (a fresh pass finds it), as a typed
    /// entry does; whether the record was marked edited.
    fn edit(ctx: &egui::Context, light: &mut Option<LightSource>, name: &str, value: f64) -> bool {
        let id = pass(ctx, light, Vec::new()).named(name).id;
        pass(ctx, light, set(id, value)).dirty
    }

    fn held(light: &Option<LightSource>) -> &LightSource {
        light.as_ref().expect("a light")
    }

    fn motion(light: &Option<LightSource>) -> (f32, f32) {
        match held(light).flicker {
            Some(
                LightFlicker::Flicker { amount, rate, .. } | LightFlicker::Pulse { amount, rate },
            ) => (amount.0, rate.0),
            ref other => panic!("no flicker: {other:?}"),
        }
    }

    /// A number set a hair off the grid - the light as a peer holds it
    /// unchanged - writes nothing and marks nothing edited, as a colour's
    /// round trip through the picker does not (the module docs). Set
    /// through the Offset fields, whose typed value egui hands on unrounded
    /// (a slider rounds it to the decimals it shows first).
    #[test]
    fn a_value_a_hair_off_the_grid_writes_nothing() {
        let ctx = context(false);
        let start = LightSource {
            offset: Fp3([0.1, -2.5, 3.0]),
            ..LightSource::default()
        };
        let mut light = Some(start.clone());
        let boxes = |light: &mut Option<LightSource>| {
            pass(&ctx, light, Vec::new()).boxes(
                -f64::from(limits::MAX_LIGHT_OFFSET_M),
                f64::from(limits::MAX_LIGHT_OFFSET_M),
            )
        };
        for (k, value) in [0.100_04, -2.499_97, 3.000_02].into_iter().enumerate() {
            let field = boxes(&mut light)[k];
            assert!(
                !pass(&ctx, &mut light, set(field, value)).dirty,
                "{k}: {value}"
            );
            assert_eq!(light.as_ref(), Some(&start), "{k}: {value}");
        }
        let field = boxes(&mut light)[0];
        assert!(pass(&ctx, &mut light, set(field, 0.1001)).dirty, "control");
        assert_eq!(held(&light).offset.0[0], 0.1001);
    }

    /// A number field shows every decimal the wire keeps: focused and left
    /// without a key typed - a click elsewhere - it writes back what it
    /// showed, as egui does on losing focus, and that is the value it
    /// holds, not one rounded.
    #[test]
    fn a_number_field_left_untouched_writes_back_the_value_it_holds() {
        let ctx = context(false);
        let start = LightSource {
            offset: Fp3([12.3457, -0.0001, 3.0]),
            ..LightSource::default()
        };
        let mut light = Some(start.clone());
        let boxes = |pass: &Pass| {
            pass.boxes(
                -f64::from(limits::MAX_LIGHT_OFFSET_M),
                f64::from(limits::MAX_LIGHT_OFFSET_M),
            )
        };
        // A click of the pointer at `at`, a frame each to move, press and
        // let go: on nothing, it takes the focus away. Whether any of the
        // three marked the record edited.
        let click_at = |light: &mut Option<LightSource>, at: egui::Pos2| {
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            [egui::Event::PointerMoved(at), button(true), button(false)]
                .into_iter()
                .map(|event| pass(&ctx, light, vec![event]).dirty)
                .fold(false, |a, b| a | b)
        };
        for k in 0..3 {
            let field = boxes(&pass(&ctx, &mut light, Vec::new()))[k];
            let focused = pass(&ctx, &mut light, request(field, Action::Focus, None));
            assert!(!focused.dirty);
            let typing = pass(&ctx, &mut light, Vec::new());
            assert_eq!(typing.focus, field, "control: the field holds the keyboard");
            let left = click_at(&mut light, egui::pos2(900.0, 1800.0));
            let after = pass(&ctx, &mut light, Vec::new());
            assert_ne!(after.focus, field, "control: the field has let it go");
            assert!(!left && !after.dirty, "field {k}");
            assert_eq!(light.as_ref(), Some(&start), "field {k}");
        }
    }

    /// A colour dragged across the open picker's square lands on the wire's
    /// grid at every step, and the light holds still with the pointer: held
    /// still mid-drag, moved a thousandth of a pixel, or let go, nothing is
    /// written or marked edited - the picker's own round trip of the colour
    /// it was handed back is no edit.
    #[test]
    fn a_colour_dragged_in_the_picker_holds_still_with_the_pointer() {
        let ctx = context(false);
        let mut light = Some(LightSource {
            color: Fp3([0.8, 0.55, 0.3]),
            ..LightSource::default()
        });
        let drawn = pass(&ctx, &mut light, Vec::new());
        let well = drawn
            .shown
            .iter()
            .find(|s| s.role == Role::ColorWell)
            .expect("a colour well")
            .id;
        pass(&ctx, &mut light, click(well));
        let open = pass(&ctx, &mut light, Vec::new());
        let square = open
            .shown
            .iter()
            .filter(|s| s.role == Role::Unknown)
            .filter_map(|s| s.bounds)
            .find(|b| b.width() > 200.0 && (b.width() - b.height()).abs() < 1.0)
            .expect("the picker's square");
        let moved = |at: egui::Pos2| egui::Event::PointerMoved(at);
        let button = |at: egui::Pos2, pressed: bool| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let mut at = square.center();
        pass(&ctx, &mut light, vec![moved(at)]);
        let mut written = 0;
        let pressed = pass(&ctx, &mut light, vec![button(at, true)]);
        written += usize::from(pressed.dirty);
        for _ in 0..6 {
            at += egui::vec2(9.0, -7.0);
            let step = pass(&ctx, &mut light, vec![moved(at)]);
            written += usize::from(step.dirty);
            let color = held(&light).color;
            assert_eq!(color, color.on_wire(), "on the grid: {color:?}");
        }
        assert!(
            written >= 5,
            "control: the drag moved the colour: {written}"
        );
        let dragged = held(&light).clone();
        for frame in 0..6 {
            let events = match frame {
                0..=2 => Vec::new(),
                _ => {
                    at.x += 0.001;
                    vec![moved(at)]
                }
            };
            let still = pass(&ctx, &mut light, events);
            assert!(!still.dirty, "held still, frame {frame}");
            assert_eq!(held(&light), &dragged, "held still, frame {frame}");
        }
        pass(&ctx, &mut light, vec![button(at, false)]);
        for frame in 0..3 {
            assert!(!pass(&ctx, &mut light, Vec::new()).dirty, "let go, {frame}");
        }
        assert_eq!(held(&light).color, dragged.color);
    }

    /// The Brightness slider's top follows the reach, as the Rate's follows
    /// the depth, and the panel says what it is: no light is stronger than
    /// the brightest of the longest reach, so a tall lamp of a modest reach
    /// may be bright and a long one may not.
    #[test]
    fn the_brightness_shows_the_cap_for_its_reach_and_never_writes_past_it() {
        let ctx = context(false);
        let mut light = Some(LightSource::spot());
        let top = |pass: &Pass| pass.named("Brightness").range.expect("a slider").2 as f32;
        let drawn = pass(&ctx, &mut light, Vec::new());
        assert_eq!(top(&drawn), limits::MAX_LIGHT_BRIGHTNESS, "a short reach");
        assert!(drawn.painted("At most 32 at this reach."));

        edit(&ctx, &mut light, "Reach", 12.0);
        let drawn = pass(&ctx, &mut light, Vec::new());
        assert_eq!(top(&drawn), 22.2222);
        assert!(
            drawn.painted("At most 22.2222 at this reach: a longer reach"),
            "{:#?}",
            drawn.shown
        );
        edit(&ctx, &mut light, "Brightness", 30.0);
        assert_eq!(held(&light).brightness, Fp(22.2222), "the most at 12 m");

        edit(&ctx, &mut light, "Reach", 20.0);
        assert_eq!(
            held(&light).brightness,
            Fp(limits::MAX_LIGHT_BRIGHTNESS_AT_MAX_RANGE),
            "held by its reach in the panel"
        );
        assert_eq!(&sanitized(held(&light)), held(&light));
    }

    /// What the owner asked comes back when the limit that held it is
    /// crossed back (#1599's lesson): a Reach dragged out and back gives
    /// back the brightness, a Depth past the flash line and back the rate,
    /// an Outer cone narrowed and widened the inner cone - each a run of
    /// absolute values, as a drag is - and a light switched from Pulse to
    /// Flicker and back, from Steady back, or from Spot to Point and back,
    /// keeps what it had. A change from outside the panel - an undo -
    /// forgets what was asked.
    #[test]
    fn a_limit_crossed_and_crossed_back_gives_the_owner_back_what_they_had() {
        let ctx = context(false);
        let mut light = Some(LightSource {
            brightness: Fp(20.0),
            range: Fp(10.0),
            dusk_to_dawn: false,
            ..LightSource::spot()
        });
        for reach in [12.0, 16.0, 20.0, 14.0, 10.0] {
            edit(&ctx, &mut light, "Reach", reach);
        }
        assert_eq!(held(&light).range, Fp(10.0));
        assert_eq!(held(&light).brightness, Fp(20.0), "the brightness asked");

        edit(&ctx, &mut light, "Outer cone", 50.0);
        edit(&ctx, &mut light, "Inner cone", 33.0);
        for outer in [30.0, 10.0, 25.0, 50.0] {
            edit(&ctx, &mut light, "Outer cone", outer);
        }
        let LightShape::Spot { inner, outer, .. } = held(&light).shape else {
            panic!("a spot");
        };
        assert_eq!((inner, outer), (Fp(33.0), Fp(50.0)), "the inner cone asked");

        let pulse = pass(&ctx, &mut light, Vec::new()).named("Pulse").id;
        pass(&ctx, &mut light, click(pulse));
        edit(&ctx, &mut light, "Depth", 0.05);
        edit(&ctx, &mut light, "Rate", 6.0);
        for depth in [0.2, 0.6, 1.0, 0.4, 0.05] {
            edit(&ctx, &mut light, "Depth", depth);
        }
        assert_eq!(motion(&light), (0.05, 6.0), "the rate asked");

        // Pulse to Flicker, an edit, and back; Steady and back.
        let flicker = pass(&ctx, &mut light, Vec::new()).named("Flicker").id;
        pass(&ctx, &mut light, click(flicker));
        edit(&ctx, &mut light, "Depth", 0.07);
        let flickering = motion(&light);
        let pulse = pass(&ctx, &mut light, Vec::new()).named("Pulse").id;
        pass(&ctx, &mut light, click(pulse));
        assert!(
            matches!(held(&light).flicker, Some(LightFlicker::Pulse { .. })),
            "a pulse again"
        );
        assert_eq!(motion(&light), (0.05, 6.0), "the pulse it had");
        let flicker = pass(&ctx, &mut light, Vec::new()).named("Flicker").id;
        pass(&ctx, &mut light, click(flicker));
        assert_eq!(motion(&light), flickering, "the flicker it had");
        let steady = pass(&ctx, &mut light, Vec::new()).named("Steady").id;
        pass(&ctx, &mut light, click(steady));
        assert_eq!(held(&light).flicker, None);
        let flicker = pass(&ctx, &mut light, Vec::new()).named("Flicker").id;
        pass(&ctx, &mut light, click(flicker));
        assert_eq!(motion(&light), flickering, "back from steady");

        // Spot to Point and back.
        let spot = held(&light).shape.clone();
        let point = pass(&ctx, &mut light, Vec::new()).named("Point").id;
        pass(&ctx, &mut light, click(point));
        assert_eq!(held(&light).shape, LightShape::Point);
        let again = pass(&ctx, &mut light, Vec::new()).named("Spot").id;
        pass(&ctx, &mut light, click(again));
        assert_eq!(held(&light).shape, spot, "the spot it had");

        // An undo puts back another light: what was asked of this one goes.
        edit(&ctx, &mut light, "Reach", 20.0);
        assert_eq!(
            held(&light).brightness,
            Fp(limits::MAX_LIGHT_BRIGHTNESS_AT_MAX_RANGE)
        );
        let undone = LightSource {
            dusk_to_dawn: true,
            ..held(&light).clone()
        };
        light = Some(undone);
        edit(&ctx, &mut light, "Reach", 10.0);
        assert_eq!(
            held(&light).brightness,
            Fp(limits::MAX_LIGHT_BRIGHTNESS_AT_MAX_RANGE),
            "nothing asked of the light the undo put back"
        );
    }

    /// A road network is drawn by its terrain and has nowhere to light
    /// from: the section says so and offers no light, while every other
    /// kind - water, terrain and the rest - may carry one.
    #[test]
    fn a_road_network_is_offered_no_light() {
        let ctx = context(false);
        let road = GeneratorKind::RoadNetwork(Default::default());
        let mut light = None;
        let drawn = pass_kind(&ctx, &mut light, &road, Vec::new());
        assert!(drawn.painted("A road network is drawn by its terrain"));
        assert!(
            !drawn.shown.iter().any(|s| s.label == "Light this part"),
            "{:#?}",
            drawn.shown
        );
        let water = GeneratorKind::Water {
            surface: Default::default(),
        };
        let drawn = pass_kind(&ctx, &mut light, &water, Vec::new());
        drawn.named("Light this part");
    }
}
