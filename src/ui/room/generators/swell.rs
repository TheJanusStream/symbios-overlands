//! The Swell section of the generator detail panel (#1936): whether a
//! node's size breathes, how its axes share the breath, whether it keeps
//! its volume, and by which terms. Like the Spin section beside it, it
//! serves a world's items, a body's visuals and a worn prop's parts.
//!
//! A swell is animation each client plays for itself; the transform above
//! it stays the size on record. Every number is written on the wire's grid
//! as the Slide section writes its own (`super::slide::grid_number`), and
//! the panel says what the controls cannot: equal weights with a kept
//! volume change nothing, everything is paused while anything is selected,
//! a swelling part has no collision, and Effects Reduced and Off calm it.

use bevy_egui::egui;

use crate::pds::sanitize::limits;
use crate::pds::{Fp, Fp3, GeneratorKind, Swell, SwellTerm};

use super::super::widgets::unrecognised_value_line;
use super::slide::{PHASE_HELP, grid_number, weak};

/// The weightings the picker names, in the node's own frame.
const SHARES: [(&str, [f32; 3], &str); 3] = [
    (
        "Evenly",
        [1.0, 1.0, 1.0],
        "Every axis grows alike: the part breathes as a whole.",
    ),
    (
        "Up",
        [0.0, 1.0, 0.0],
        "Only its height grows: with its volume kept, it stretches up as it \
         squeezes in.",
    ),
    (
        "Across",
        [1.0, 0.0, 1.0],
        "Only its width and depth grow: with its volume kept, it squats as it \
         spreads.",
    ),
];

/// Draw the Swell section for one node. `refusal` is the tree's reason its
/// root may not move, passed for the root only.
pub(super) fn draw_swell_section(
    ui: &mut egui::Ui,
    swell: &mut Option<Swell>,
    kind: &GeneratorKind,
    refusal: Option<&str>,
    salt: &str,
    dirty: &mut bool,
) {
    // Its own id scope (#1632), as the Slide section has.
    ui.push_id((salt, "swell_section"), |ui| {
        draw_section(ui, swell, kind, refusal, dirty);
    });
}

fn draw_section(
    ui: &mut egui::Ui,
    swell: &mut Option<Swell>,
    kind: &GeneratorKind,
    refusal: Option<&str>,
    dirty: &mut bool,
) {
    let theme = crate::ui::theme::current(ui.ctx());
    ui.label(
        egui::RichText::new("Swell")
            .strong()
            .color(theme.text_strong),
    );
    let reason = if !kind.may_spin() {
        Some("Terrain, water and roads are laid in the world's own terms, so they cannot swell.")
    } else {
        refusal
    };
    if let Some(reason) = reason {
        weak(ui, reason);
        return;
    }

    let mut on = swell.is_some();
    if ui
        .checkbox(&mut on, "Swell this part")
        .on_hover_text(
            "Every visitor's client breathes its size, and everything attached \
             below it grows with it. The transform above stays the size on \
             record.",
        )
        .changed()
    {
        *swell =
            on.then(|| Swell::even(SwellTerm::fresh("Swing").expect("a kind the panel offers")));
        *dirty = true;
    }
    let Some(current) = swell.as_mut() else {
        weak(
            ui,
            "A beating heart, a pulsing mushroom cap, a sail filling in the wind.",
        );
        return;
    };

    draw_weights(ui, current, dirty);

    let mut remove = None;
    for (i, term) in current.terms.iter_mut().enumerate() {
        ui.push_id(("swell_term", i), |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(term.label()).strong());
                if ui
                    .small_button("Remove")
                    .on_hover_text("Take this term off the swell.")
                    .clicked()
                {
                    remove = Some(i);
                }
            });
            ui.indent("swell_term_fields", |ui| draw_term(ui, term, dirty));
        });
    }
    if let Some(i) = remove {
        current.terms.remove(i);
        *dirty = true;
    }

    let full = current.terms.len() >= limits::MAX_SWELL_TERMS;
    let full_reason = format!("{} terms at most.", limits::MAX_SWELL_TERMS);
    ui.horizontal(|ui| {
        let mut add = ui.add_enabled_ui(!full, |ui| {
            ui.menu_button("+ Add term", |ui| {
                for kind in SwellTerm::KINDS {
                    if ui.button(kind).on_hover_text(term_help(kind)).clicked() {
                        current.terms.extend(SwellTerm::fresh(kind));
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
    // The last term gone is no swell: the checkbox reads off, and the
    // record carries none.
    if current.terms.is_empty() {
        *swell = None;
    }

    weak(
        ui,
        "Paused while anything is selected - deselect to watch it. A swelling \
         part, and everything below it, has no collision, and the copies of \
         an instance set do not swell. Effects Reduced breathes it half as \
         much, Off not at all.",
    );
}

/// How the axes share the breath: a named weighting or each axis's own
/// share, and whether the part keeps its volume.
fn draw_weights(ui: &mut egui::Ui, swell: &mut Swell, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label("Grows")
            .on_hover_text("How much of each breath each of the part's own axes takes.");
        for (name, weights, help) in SHARES {
            let selected = swell.weights.wire() == Fp3(weights).wire();
            if ui
                .selectable_label(selected, name)
                .on_hover_text(help)
                .clicked()
                && !selected
            {
                swell.weights = Fp3(weights);
                *dirty = true;
            }
        }
    });
    ui.horizontal(|ui| {
        let help = "This axis's share of each breath, from none (0) to all of \
                    it (1), in the part's own frame.";
        for (axis, weight) in ["X", "Y", "Z"].into_iter().zip(0..3) {
            let mut share = Fp(swell.weights.0[weight]);
            let mut moved = false;
            grid_number(ui, axis, &mut share, 0.0..=1.0, 0.01, "", help, &mut moved);
            if moved {
                swell.weights.0[weight] = share.0;
                *dirty = true;
            }
        }
    });
    if ui
        .checkbox(&mut swell.keep_volume, "Keep its volume")
        .on_hover_text(
            "The other axes give back what one takes, so the part never grows \
             or shrinks as a whole: grown Up, it squeezes in as it stretches - \
             squash and stretch.",
        )
        .changed()
    {
        *dirty = true;
    }
    let w = swell.weights.0;
    if swell.keep_volume && w[0] == w[1] && w[1] == w[2] {
        weak(
            ui,
            "With every axis taking the same share, a kept volume changes \
             nothing: let one axis take more than the others.",
        );
    }
}

/// One term's fields.
fn draw_term(ui: &mut egui::Ui, term: &mut SwellTerm, dirty: &mut bool) {
    let most = limits::MAX_SWELL_AMOUNT;
    let periods = limits::MIN_SWING_PERIOD_S..=limits::MAX_SWING_PERIOD_S;
    let turn = limits::MAX_SWING_PHASE_DEG;
    match term {
        SwellTerm::Swing {
            amount,
            period,
            phase,
        } => {
            grid_number(
                ui,
                "Either side (share of size)",
                amount,
                -most..=most,
                0.005,
                "",
                "How much it grows and shrinks, as a share of its size: 0.1 \
                 breathes a tenth either way. Negative shrinks first.",
                dirty,
            );
            grid_number(
                ui,
                "One breath every",
                period,
                periods,
                0.05,
                " s",
                "Seconds for a whole breath, in and out.",
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
        SwellTerm::Wobble { amount, period } => {
            grid_number(
                ui,
                "Either side (share of size)",
                amount,
                -most..=most,
                0.005,
                "",
                "The most it grows or shrinks, as a share of its size.",
                dirty,
            );
            grid_number(
                ui,
                "About every",
                period,
                periods,
                0.05,
                " s",
                "Roughly how long one breath takes; it never settles into a beat.",
                dirty,
            );
        }
        SwellTerm::Wind { amount } => {
            grid_number(
                ui,
                "Up to (share of size)",
                amount,
                -most..=most,
                0.005,
                "",
                "How much a gust fills it, as a share of its size. Negative \
                 shrinks it in the wind instead.",
                dirty,
            );
            weak(
                ui,
                "In the world's wind, the one its trees sway in (Environment), \
                 whichever way it blows; more in a stronger wind.",
            );
        }
        SwellTerm::Unknown => {
            unrecognised_value_line(ui, "swell term", Some("it changes nothing here"));
        }
    }
}

/// What each kind of term does, for the add menu.
fn term_help(kind: &str) -> &'static str {
    match kind {
        "Swing" => "Breathe in and out: a beating heart, a pulsing cap.",
        "Wobble" => "Breathe without a beat: a jelly, a glowing ember.",
        "Wind" => "Fill in the world's wind: a sail, a windsock.",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::super::slide::probe::{Pass, click, context, pass, set};
    use super::*;

    fn draw(
        ctx: &egui::Context,
        swell: &mut Option<Swell>,
        events: Vec<egui::Event>,
    ) -> (Pass, bool) {
        let mut dirty = false;
        let drawn = pass(ctx, events, |ui| {
            draw_swell_section(
                ui,
                swell,
                &GeneratorKind::default_cuboid(),
                None,
                "cap",
                &mut dirty,
            );
        });
        (drawn, dirty)
    }

    /// Drawn over and over with nothing touched - every folded section and
    /// the add menu open - the section leaves the swell exactly as it was,
    /// a hand-written weighting no picker names and an unknown term
    /// included.
    #[test]
    fn drawing_the_swell_section_leaves_the_record_untouched() {
        let ctx = context();
        let mut terms: Vec<SwellTerm> = SwellTerm::KINDS
            .iter()
            .map(|k| SwellTerm::fresh(k).expect("offered"))
            .collect();
        terms.push(SwellTerm::Unknown);
        let held = Swell {
            weights: Fp3([0.333_333, 1.0, 0.0]),
            keep_volume: true,
            terms,
        };
        let mut swell = Some(held.clone());
        for _ in 0..4 {
            let (_, dirty) = draw(&ctx, &mut swell, Vec::new());
            assert!(!dirty);
            assert_eq!(swell.as_ref(), Some(&held));
        }
        let mut none = None;
        let (_, dirty) = draw(&ctx, &mut none, Vec::new());
        assert!(!dirty && none.is_none(), "drawing adds nothing");
    }

    /// The section adds a swell, weights it up with a kept volume, takes a
    /// typed amount onto the wire's grid and adds a wind term from the menu.
    #[test]
    fn the_swell_section_adds_terms_on_the_wire_grid() {
        let ctx = context();
        let mut swell = None;
        let (drawn, _) = draw(&ctx, &mut swell, Vec::new());
        let (_, dirty) = draw(&ctx, &mut swell, click(drawn.named("Swell this part")));
        assert!(dirty);
        assert_eq!(
            swell,
            Some(Swell::even(SwellTerm::fresh("Swing").expect("offered")))
        );

        let (drawn, _) = draw(&ctx, &mut swell, Vec::new());
        assert!(draw(&ctx, &mut swell, click(drawn.named("Up"))).1);
        let (drawn, _) = draw(&ctx, &mut swell, Vec::new());
        assert!(draw(&ctx, &mut swell, click(drawn.named("Keep its volume"))).1);
        let held = swell.clone().expect("a swell");
        assert_eq!(held.weights, Fp3([0.0, 1.0, 0.0]));
        assert!(held.keep_volume);

        let most = f64::from(limits::MAX_SWELL_AMOUNT);
        let (drawn, _) = draw(&ctx, &mut swell, Vec::new());
        let amounts = drawn.boxes(-most, most);
        assert_eq!(amounts.len(), 1, "{:#?}", drawn.shown);
        assert!(draw(&ctx, &mut swell, set(amounts[0], 0.217_777)).1);
        let shares = drawn.boxes(0.0, 1.0);
        assert_eq!(shares.len(), 3, "{:#?}", drawn.shown);
        assert!(draw(&ctx, &mut swell, set(shares[2], 0.456_789)).1);

        let (drawn, _) = draw(&ctx, &mut swell, Vec::new());
        assert!(draw(&ctx, &mut swell, click(drawn.named("Wind"))).1);
        let held = swell.expect("a swell");
        assert_eq!(held.weights, Fp3([0.0, 1.0, 0.4568]), "on the wire's grid");
        assert_eq!(
            held.terms,
            vec![
                SwellTerm::Swing {
                    amount: Fp(0.2178),
                    period: Fp(2.0),
                    phase: Fp::ZERO,
                },
                SwellTerm::fresh("Wind").expect("offered"),
            ]
        );
        let saved = serde_json::to_string(&held).expect("writes");
        assert_eq!(
            serde_json::from_str::<Swell>(&saved).expect("reads"),
            held,
            "what the panel wrote reloads unchanged"
        );
    }

    /// Equal shares with a kept volume change nothing, and the panel says so
    /// rather than leaving a swell that never moves to be puzzled over.
    #[test]
    fn the_swell_section_says_when_a_kept_volume_changes_nothing() {
        let ctx = context();
        let mut swell = Some(Swell {
            keep_volume: true,
            ..Swell::even(SwellTerm::fresh("Swing").expect("offered"))
        });
        let (drawn, _) = draw(&ctx, &mut swell, Vec::new());
        assert!(drawn.painted("With every axis taking the same share"));
        let mut terrain = None;
        let drawn = pass(&ctx, Vec::new(), |ui| {
            draw_swell_section(
                ui,
                &mut terrain,
                &GeneratorKind::Terrain(Default::default()),
                None,
                "ground",
                &mut false,
            );
        });
        assert!(drawn.painted("Terrain, water and roads"));
        assert!(!drawn.painted("Swell this part"));
    }
}
