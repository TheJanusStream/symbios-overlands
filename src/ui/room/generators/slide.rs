//! The Slide section of the generator detail panel (#1936): whether a node
//! moves along an axis, which axis, and by which terms. Like the Spin
//! section beside it, it serves a world's items, a body's visuals and a
//! worn prop's parts.
//!
//! A slide is animation each client plays for itself; the transform above
//! it stays the place on record. Every number is written on the wire's
//! 1/10 000 grid as a peer reads it back ([`grid_number`]), so the editor's
//! flush (`settle_edit`) finds nothing left to move, and a number that comes
//! back unchanged - a box that loses focus writes back what it showed - is
//! not written at all. Distances are offered as far as a part of the tree
//! may be long, which is what its sanitiser holds them to: a room's
//! ceiling, or a body's tighter one. The panel says what the controls
//! cannot: everything is paused while anything is selected, a sliding part
//! has no collision, an instance set's copies draw it still, and Effects
//! Reduced and Off calm it.

use bevy_egui::egui;

use crate::pds::sanitize::limits;
use crate::pds::{Fp, GeneratorKind, Slide, SlideTerm};

use super::super::widgets::unrecognised_value_line;

/// Draw the Slide section for one node. `refusal` is the tree's reason its
/// root may not move, passed for the root only; `max_dim` is the longest a
/// part of the tree may be (m), the furthest a slide may take it.
pub(super) fn draw_slide_section(
    ui: &mut egui::Ui,
    slide: &mut Option<Slide>,
    kind: &GeneratorKind,
    refusal: Option<&str>,
    max_dim: f32,
    salt: &str,
    dirty: &mut bool,
) {
    // Its own id scope (#1632): the axis row's folded header shares its
    // title with the Spin section's.
    ui.push_id((salt, "slide_section"), |ui| {
        draw_section(ui, slide, kind, refusal, max_dim, dirty);
    });
}

fn draw_section(
    ui: &mut egui::Ui,
    slide: &mut Option<Slide>,
    kind: &GeneratorKind,
    refusal: Option<&str>,
    max_dim: f32,
    dirty: &mut bool,
) {
    let theme = crate::ui::theme::current(ui.ctx());
    ui.label(
        egui::RichText::new("Slide")
            .strong()
            .color(theme.text_strong),
    );
    let reason = if !kind.may_spin() {
        Some("Terrain, water and roads are laid in the world's own terms, so they cannot slide.")
    } else {
        refusal
    };
    if let Some(reason) = reason {
        weak(ui, reason);
        return;
    }

    let mut on = slide.is_some();
    if ui
        .checkbox(&mut on, "Slide this part")
        .on_hover_text(
            "Every visitor's client moves it along an axis, and everything \
             attached below it with it - its light and its sound too. The \
             transform above stays the place on record.",
        )
        .changed()
    {
        *slide = on.then(|| {
            Slide::along(
                [0.0, 1.0, 0.0],
                SlideTerm::fresh("Swing").expect("a kind the panel offers"),
            )
        });
        *dirty = true;
    }
    let Some(current) = slide.as_mut() else {
        weak(
            ui,
            "A bobbing buoy, a bouncing ball, crates riding a conveyor.",
        );
        return;
    };

    super::spin::draw_direction(
        ui,
        "Axis",
        "Move the other way along the same line.",
        &mut current.axis,
        "slide",
        dirty,
    );

    let mut remove = None;
    for (i, term) in current.terms.iter_mut().enumerate() {
        ui.push_id(("slide_term", i), |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(term.label()).strong());
                if ui
                    .small_button("Remove")
                    .on_hover_text("Take this term off the slide.")
                    .clicked()
                {
                    remove = Some(i);
                }
            });
            ui.indent("slide_term_fields", |ui| {
                draw_term(ui, term, max_dim, dirty);
            });
        });
    }
    if let Some(i) = remove {
        current.terms.remove(i);
        *dirty = true;
    }

    let full = current.terms.len() >= limits::MAX_SLIDE_TERMS;
    let full_reason = format!("{} terms at most.", limits::MAX_SLIDE_TERMS);
    ui.horizontal(|ui| {
        let mut add = ui.add_enabled_ui(!full, |ui| {
            ui.menu_button("+ Add term", |ui| {
                for kind in SlideTerm::KINDS {
                    if ui.button(kind).on_hover_text(term_help(kind)).clicked() {
                        current.terms.extend(SlideTerm::fresh(kind));
                        *dirty = true;
                        ui.close();
                    }
                }
            });
        });
        add.response = add.response.on_disabled_hover_text(&full_reason);
        if full {
            // Painted as well: a disabled control's hover never fires
            // (#1289).
            weak(ui, &full_reason);
        }
    });
    // The last term gone is no slide: the checkbox reads off, and the
    // record carries none.
    if current.terms.is_empty() {
        *slide = None;
    }

    weak(
        ui,
        "Paused while anything is selected - deselect to watch it. A sliding \
         part, and everything below it, has no collision, and the copies of \
         an instance set do not slide. Effects Reduced moves it half as far \
         (a belt at half speed), Off not at all.",
    );
}

/// One term's fields.
fn draw_term(ui: &mut egui::Ui, term: &mut SlideTerm, max_dim: f32, dirty: &mut bool) {
    let periods = limits::MIN_SWING_PERIOD_S..=limits::MAX_SWING_PERIOD_S;
    let turn = limits::MAX_SWING_PHASE_DEG;
    let reach = -max_dim..=max_dim;
    match term {
        SlideTerm::Swing {
            amplitude,
            period,
            phase,
        } => {
            grid_number(
                ui,
                "Either side",
                amplitude,
                reach,
                0.01,
                " m",
                "How far it moves each way from its place along the axis. \
                 Negative starts the other way.",
                dirty,
            );
            grid_number(
                ui,
                "One swing every",
                period,
                periods,
                0.05,
                " s",
                "Seconds for a whole swing, there and back.",
                dirty,
            );
            grid_number(
                ui,
                "Starting at",
                phase,
                -turn..=turn,
                1.0,
                "\u{b0}",
                PHASE_HELP,
                dirty,
            );
        }
        SlideTerm::Wobble { amplitude, period } => {
            grid_number(
                ui,
                "Either side",
                amplitude,
                reach,
                0.01,
                " m",
                "The furthest it wanders each way from its place.",
                dirty,
            );
            grid_number(
                ui,
                "About every",
                period,
                periods,
                0.05,
                " s",
                "Roughly how long one wander takes; it never settles into a beat.",
                dirty,
            );
        }
        SlideTerm::Wind { amplitude } => {
            grid_number(
                ui,
                "Up to",
                amplitude,
                reach,
                0.01,
                " m",
                "How far the wind pushes it when it blows straight along the \
                 axis. Negative moves it upwind instead.",
                dirty,
            );
            weak(
                ui,
                "In the world's wind, the one its trees sway in (Environment), \
                 downwind along the axis; further in a stronger wind.",
            );
        }
        SlideTerm::Bounce {
            height,
            period,
            phase,
        } => {
            grid_number(
                ui,
                "Hop height",
                height,
                reach,
                0.01,
                " m",
                "How high each hop goes along the axis; it comes back to its \
                 place between hops. Negative hops the other way.",
                dirty,
            );
            grid_number(
                ui,
                "One hop every",
                period,
                periods,
                0.05,
                " s",
                "Seconds from one landing to the next.",
                dirty,
            );
            grid_number(
                ui,
                "Starting at",
                phase,
                -turn..=turn,
                1.0,
                "\u{b0}",
                "Degrees into the hop it starts at: 90 starts at the top, 180 \
                 is a whole hop.",
                dirty,
            );
        }
        SlideTerm::Belt {
            speed,
            length,
            start,
        } => {
            let fastest = limits::MAX_BELT_SPEED_M_S;
            grid_number(
                ui,
                "Speed",
                speed,
                -fastest..=fastest,
                0.01,
                " m/s",
                "How fast it rides along the axis. Negative runs the other way.",
                dirty,
            );
            grid_number(
                ui,
                "Length",
                length,
                0.0..=max_dim,
                0.01,
                " m",
                "How far it rides before it jumps back to its place.",
                dirty,
            );
            grid_number(
                ui,
                "Starting",
                start,
                0.0..=max_dim,
                0.01,
                " m",
                "How far along the run it starts. Give each part on one belt \
                 a start of its own, spaced evenly over the length.",
                dirty,
            );
            weak(
                ui,
                "A conveyor: put several parts in one place, each with the same \
                 belt and starts spaced evenly over the length (four crates on \
                 a 4 m run: 0, 1, 2 and 3 m), and hide both ends of the run \
                 inside something so the jump back is never seen.",
            );
        }
        SlideTerm::Unknown => {
            unrecognised_value_line(ui, "slide term", Some("it moves nothing here"));
        }
    }
}

/// What a phase does, for every swing's Starting at.
pub(super) const PHASE_HELP: &str = "Degrees into the swing it starts at: two parts a quarter turn (90) apart \
     never stand still together.";

/// A small weak note.
pub(super) fn weak(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .small()
            .color(crate::ui::theme::current(ui.ctx()).text_weak),
    );
}

/// A labelled number for one of a slide's or a swell's fields, written on
/// the wire's grid as a peer reads it back, and only when that changes it.
/// A `DragValue` keeps its own running total through a drag (#1599), so
/// rounding what it writes never holds a drag back.
#[allow(clippy::too_many_arguments)]
pub(super) fn grid_number(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Fp,
    range: std::ops::RangeInclusive<f32>,
    speed: f64,
    suffix: &str,
    help: &str,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label(label).on_hover_text(help);
        let mut v = value.0;
        let response = ui
            .add(
                crate::ui::num::drag(&mut v)
                    .speed(speed)
                    .range(range)
                    .suffix(suffix)
                    .max_decimals(super::light::GRID_DECIMALS),
            )
            .on_hover_text(help);
        let asked = Fp(v).on_wire();
        if response.changed() && asked != *value {
            *value = asked;
            *dirty = true;
        }
    });
}

/// What each kind of term does, for the add menu.
fn term_help(kind: &str) -> &'static str {
    match kind {
        "Swing" => "Move to and fro: a bobbing buoy, a piston, a lift.",
        "Wobble" => "Wander to and fro without a beat: a hovering drone, a floating mote.",
        "Wind" => "Move in the world's wind: a hanging sign drifting, a swaying lantern.",
        "Bounce" => "Hop and land again: a bouncing ball, a hopping frog.",
        "Belt" => "Ride along and start again: crates on a conveyor.",
        _ => "",
    }
}

/// A headless probe of a section, shared with the Swell section's tests: one
/// pass drawn with the given input, and what it drew as AccessKit reads it.
#[cfg(test)]
pub(super) mod probe {
    use bevy_egui::egui;
    use egui::accesskit::{Action, ActionData, ActionRequest, Node, NodeId, Role, TreeId};

    use crate::ui::headless::Headless as _;

    /// One widget a pass drew.
    #[derive(Debug)]
    pub struct Shown {
        pub id: NodeId,
        pub role: Role,
        pub label: String,
        pub value: String,
        /// `(min, max)` for a numeric widget.
        pub range: Option<(f64, f64)>,
    }

    /// What one pass drew.
    pub struct Pass {
        pub shown: Vec<Shown>,
    }

    impl Pass {
        /// The interactive widget named `name`, whatever its role.
        pub fn named(&self, name: &str) -> NodeId {
            self.shown
                .iter()
                .find(|s| s.label == name && !matches!(s.role, Role::Label | Role::TextRun))
                .unwrap_or_else(|| panic!("no widget {name:?} in {:#?}", self.shown))
                .id
        }

        /// The number boxes whose range is `lo..=hi`, in document order.
        pub fn boxes(&self, lo: f64, hi: f64) -> Vec<NodeId> {
            self.shown
                .iter()
                .filter(|s| s.role == Role::SpinButton && s.range == Some((lo, hi)))
                .map(|s| s.id)
                .collect()
        }

        pub fn painted(&self, start: &str) -> bool {
            self.shown
                .iter()
                .any(|s| s.label.starts_with(start) || s.value.starts_with(start))
        }
    }

    /// A context with AccessKit on and every folded section and popup drawn.
    pub fn context() -> egui::Context {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.memory_mut(|m| m.set_everything_is_visible(true));
        ctx
    }

    pub fn click(id: NodeId) -> Vec<egui::Event> {
        request(id, Action::Click, None)
    }

    /// Set the numeric widget `id` to `value`, as a typed entry does.
    pub fn set(id: NodeId, value: f64) -> Vec<egui::Event> {
        request(id, Action::SetValue, Some(ActionData::NumericValue(value)))
    }

    fn request(id: NodeId, action: Action, data: Option<ActionData>) -> Vec<egui::Event> {
        vec![egui::Event::AccessKitActionRequest(ActionRequest {
            action,
            target_tree: TreeId::ROOT,
            target_node: id,
            data,
        })]
    }

    /// Draw `draw` once with `events` as the input.
    pub fn pass(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        mut draw: impl FnMut(&mut egui::Ui),
    ) -> Pass {
        let output = ctx.run_headless(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 2400.0),
                )),
                events,
                ..Default::default()
            },
            |ui| draw(ui),
        );
        let update = output
            .platform_output
            .accesskit_update
            .expect("AccessKit is on");
        let by_id: std::collections::HashMap<NodeId, &Node> =
            update.nodes.iter().map(|(id, n)| (*id, n)).collect();
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
                    range: node.min_numeric_value().zip(node.max_numeric_value()),
                });
            }
            stack.extend(node.children().iter().rev().copied());
        }
        Pass { shown }
    }
}

#[cfg(test)]
mod tests {
    use super::probe::{click, context, pass, set};
    use super::*;

    fn draw(
        ctx: &egui::Context,
        slide: &mut Option<Slide>,
        kind: &GeneratorKind,
        events: Vec<egui::Event>,
    ) -> (super::probe::Pass, bool) {
        let mut dirty = false;
        let drawn = pass(ctx, events, |ui| {
            draw_slide_section(
                ui,
                slide,
                kind,
                None,
                limits::MAX_PRIM_DIM_M,
                "buoy",
                &mut dirty,
            );
        });
        (drawn, dirty)
    }

    fn cuboid() -> GeneratorKind {
        GeneratorKind::default_cuboid()
    }

    /// Drawn over and over with nothing touched - every folded section and
    /// the add menu open - the section leaves the slide exactly as it was,
    /// off-grid hand-written values and an unknown term included.
    #[test]
    fn drawing_the_slide_section_leaves_the_record_untouched() {
        let ctx = context();
        let mut terms: Vec<SlideTerm> = SlideTerm::KINDS[..3]
            .iter()
            .map(|k| SlideTerm::fresh(k).expect("offered"))
            .collect();
        terms.push(SlideTerm::Unknown);
        let held = Slide {
            axis: crate::pds::Fp3([0.123_456_7, -0.7, 0.0]),
            terms,
        };
        let mut slide = Some(held.clone());
        for _ in 0..4 {
            let (_, dirty) = draw(&ctx, &mut slide, &cuboid(), Vec::new());
            assert!(!dirty);
            assert_eq!(slide.as_ref(), Some(&held));
        }
        let mut none = None;
        let (_, dirty) = draw(&ctx, &mut none, &cuboid(), Vec::new());
        assert!(!dirty && none.is_none(), "drawing adds nothing");
    }

    /// The section adds a slide, takes a typed number onto the wire's grid,
    /// adds a belt from the menu and takes the last term off again.
    #[test]
    fn the_slide_section_adds_terms_on_the_wire_grid() {
        let ctx = context();
        let mut slide = None;
        let (drawn, _) = draw(&ctx, &mut slide, &cuboid(), Vec::new());
        let (_, dirty) = draw(
            &ctx,
            &mut slide,
            &cuboid(),
            click(drawn.named("Slide this part")),
        );
        assert!(dirty);
        assert_eq!(
            slide,
            Some(Slide::along(
                [0.0, 1.0, 0.0],
                SlideTerm::fresh("Swing").expect("offered")
            ))
        );

        let room = f64::from(limits::MAX_PRIM_DIM_M);
        let (drawn, _) = draw(&ctx, &mut slide, &cuboid(), Vec::new());
        let reach = drawn.boxes(-room, room);
        assert_eq!(reach.len(), 1, "{:#?}", drawn.shown);
        let (_, dirty) = draw(&ctx, &mut slide, &cuboid(), set(reach[0], 0.123_456_78));
        assert!(dirty);
        let typed = slide.as_ref().expect("a slide").terms[0].clone();
        let SlideTerm::Swing { amplitude, .. } = typed else {
            panic!("a swing: {typed:?}");
        };
        assert_eq!(amplitude, Fp(0.1235), "on the wire's grid");

        let (drawn, _) = draw(&ctx, &mut slide, &cuboid(), Vec::new());
        let (_, dirty) = draw(&ctx, &mut slide, &cuboid(), click(drawn.named("Belt")));
        assert!(dirty);
        let held = slide.clone().expect("a slide");
        assert_eq!(held.terms.len(), 2);
        assert_eq!(held.terms[1], SlideTerm::fresh("Belt").expect("offered"));
        // What the panel wrote reloads unchanged: it was on the grid.
        let saved = serde_json::to_string(&held).expect("writes");
        assert_eq!(serde_json::from_str::<Slide>(&saved).expect("reads"), held);

        // Both terms off: no slide.
        for _ in 0..2 {
            let (drawn, _) = draw(&ctx, &mut slide, &cuboid(), Vec::new());
            draw(&ctx, &mut slide, &cuboid(), click(drawn.named("Remove")));
        }
        assert_eq!(slide, None);
    }

    /// Terrain, water and roads may not move: the reason is painted where
    /// the controls would be, and an avatar's root refusal likewise.
    #[test]
    fn the_slide_section_paints_why_a_node_cannot_slide() {
        let ctx = context();
        let mut slide = None;
        let (drawn, _) = draw(
            &ctx,
            &mut slide,
            &GeneratorKind::Terrain(Default::default()),
            Vec::new(),
        );
        assert!(drawn.painted("Terrain, water and roads"));
        assert!(!drawn.painted("Slide this part"));
        let mut dirty = false;
        let drawn = pass(&ctx, Vec::new(), |ui| {
            draw_slide_section(
                ui,
                &mut slide,
                &cuboid(),
                Some("The root of a body's visuals cannot turn, slide or swell."),
                limits::MAX_AVATAR_PRIM_DIM_M,
                "body",
                &mut dirty,
            );
        });
        assert!(drawn.painted("The root of a body's visuals"));
        assert!(!dirty);
    }
}
