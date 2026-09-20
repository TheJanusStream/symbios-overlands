//! Locomotion-tab UI: per-preset slider panel plus the central preset
//! picker. Each preset's [`LocomotionPanel`] impl lives in its own
//! submodule so adding a new preset is one new file + one match arm in
//! [`draw_locomotion_tab`].

mod airplane;
mod car;
mod gait;
mod helicopter;
mod hover_boat;
mod humanoid;

use bevy_egui::egui;

use crate::pds::{Fp, GaitParams, LocomotionConfig};
use crate::player::LocalMovement;
use crate::ui::avatar::TabCtx;
use crate::ui::editable::SeedRowState;

/// The Locomotion tab, wired to the editor (#1161).
///
/// [`draw_locomotion_tab`] below is the panel itself. This is the half that
/// is the *editor's*: the scroll frame the tab body lives in, the standing
/// note about what does and does not hold the body still, and the seed this
/// tab falls back to when the record carries no gait section.
pub(super) fn draw_tab(
    ui: &mut egui::Ui,
    ctx: &mut TabCtx,
    height: f32,
    seed_row: &SeedRowState,
    movement: &LocalMovement,
) {
    egui::ScrollArea::vertical()
        .auto_shrink([true, false])
        .max_height(height)
        .show(ui, |ui| {
            // #1265 f109: this used to teach a collapse-the-window
            // workaround for the #814 full-body freeze. #1103 reversed that
            // freeze - `holds_avatar_still` is exactly `has_gizmo_selection`
            // now, and `release_hidden_selections` clears every avatar-side
            // selection when a tab that cannot show it is picked, so no
            // gizmo can be aimed while this tab is on screen. Name the
            // gizmo, not the window.
            ui.label(
                egui::RichText::new(
                    "⏵ Drive with WASD while this window is open - \
                     your avatar only holds still while a gizmo is \
                     aimed at it.",
                )
                .small()
                .weak(),
            );
            ui.add_space(4.0);
            // Master seed for the Idle-motion section's baseline + ⟲
            // re-derive: the seed row's current value when it parses (the
            // footer synced it to the DID seed on first draw), else the DID
            // derivation every peer falls back to for a record without a
            // gait section.
            let fallback_seed = seed_row
                .current_seed()
                .or_else(|| ctx.did.map(crate::seeded_defaults::fnv1a_64))
                .unwrap_or_default();
            draw_locomotion_tab(
                ui,
                &mut ctx.record.locomotion,
                &mut ctx.record.gait,
                fallback_seed,
                ctx.changed,
                &mut ctx.labels.slot(crate::ui::shortcuts::EditorKind::Avatar),
                movement,
                ctx.toasts,
                ctx.now,
            );
        });
}

/// Egui detail panel for one locomotion preset. Implemented on each
/// `*Params` struct in this module's siblings - `draw_locomotion_tab`
/// dispatches to whichever variant the live `LocomotionConfig` carries.
pub trait LocomotionPanel {
    fn draw(&mut self, ui: &mut egui::Ui, dirty: &mut bool, facts: &LocalMovement);
}

/// The name of the preset whose tuning a switch away from `current` would
/// throw away, or `None` when it would throw away nothing (#1256 f102).
///
/// "Nothing" means the live config still equals its own preset defaults -
/// the exact comparison the retired #838 confirm modal used to gate on. An
/// untuned switch stays silent, because the sentence is about loss, not
/// about having clicked.
fn discarded_tuning(current: &LocomotionConfig) -> Option<&'static str> {
    let kind = current.kind_tag();
    LocomotionConfig::pickers()
        .iter()
        .find(|(k, _, _)| *k == kind)
        .filter(|(_, _, ctor)| ctor() != *current)
        .map(|(_, label, _)| *label)
}

/// What the owner is told at the moment their tuning is replaced.
///
/// Undo alone was not enough: the loss was INVISIBLE when it happened - the
/// panel simply redrew with different sliders - so by the time anyone
/// noticed, the 32-entry ring could have rolled past it. `undo_label` only
/// names a future undo ENTRY; the undo toast fires on Ctrl+Z, which is
/// exactly the gesture someone who does not know they lost anything will
/// never make.
fn switch_discard_line(from: &str, to: &str) -> String {
    format!("Switched to {to} - your {from} tuning was replaced with defaults. Ctrl+Z restores it.")
}

/// Render the picker row (one selectable label per preset, switching
/// preset replaces `*locomotion` with the new variant's default-tuned
/// instance) followed by the per-preset detail panel.
///
/// #838 originally routed a lossy switch through the shared confirm modal;
/// #866 retired that in favour of undo. The undo contract is the one in
/// force: switching preset replaces the whole config with the new variant's
/// defaults, the switch is one entry in the ring, and `undo_label` names it.
///
/// #1256 f102: undo alone was not enough, because the loss was INVISIBLE at
/// the moment it happened - the panel simply redrew with different sliders,
/// so by the time an owner noticed their tuning was gone the 32-entry ring
/// could have rolled past it. The switch now says what it replaced, at the
/// moment it replaces it, and `toasts` is threaded here for that. A switch
/// that discards nothing (the config still IS its own defaults) stays
/// silent - the sentence is about loss, not about clicking.
#[allow(clippy::too_many_arguments)]
pub fn draw_locomotion_tab(
    ui: &mut egui::Ui,
    locomotion: &mut LocomotionConfig,
    gait: &mut Option<GaitParams>,
    fallback_seed: u64,
    dirty: &mut bool,
    undo_label: &mut crate::ui::undo::LabelSlot,
    // What the live body is actually doing (#1241 f168): a panel that
    // tunes movement needs to be able to say when a value it publishes
    // has stopped having an effect on THIS body.
    facts: &LocalMovement,
    toasts: &mut crate::notify::Toasts,
    now: f64,
) {
    let current_kind = locomotion.kind_tag();

    ui.horizontal_wrapped(|ui| {
        ui.label("Preset:");
        for (kind, label, ctor) in LocomotionConfig::pickers() {
            // Fires on the click itself (#866): pre-undo this asked for
            // confirmation when tuning would be discarded, but a switch
            // is now one Ctrl+Z away and the toast names it.
            if ui.selectable_label(current_kind == *kind, *label).clicked() && current_kind != *kind
            {
                // Measured BEFORE the replacement.
                let lost = discarded_tuning(locomotion);
                *locomotion = ctor();
                undo_label.set(format!("preset switch to {label}"));
                *dirty = true;
                if let Some(from) = lost {
                    toasts.warn(switch_discard_line(from, label), now);
                }
            }
        }
    });
    ui.separator();

    match locomotion {
        LocomotionConfig::HoverBoat(p) => p.draw(ui, dirty, facts),
        LocomotionConfig::Humanoid(p) => p.draw(ui, dirty, facts),
        LocomotionConfig::Airplane(p) => p.draw(ui, dirty, facts),
        LocomotionConfig::Helicopter(p) => p.draw(ui, dirty, facts),
        LocomotionConfig::Car(p) => p.draw(ui, dirty, facts),
        LocomotionConfig::Unknown => {
            ui.colored_label(
                crate::ui::theme::current(ui.ctx()).status.warn,
                "This avatar's locomotion preset was authored against a newer schema - \
                 pick a preset above to replace it.",
            );
        }
    }

    ui.separator();
    gait::draw_gait_section(ui, locomotion, gait, fallback_seed, dirty, undo_label);
}

// ---------------------------------------------------------------------------
// Shared widgets - narrower than `ui::room::widgets::fp_slider` (this one
// takes a step size and emits no inline label, leaving the caller to draw
// labels next to a stack of related sliders).
// ---------------------------------------------------------------------------

pub(super) fn fp_slider(
    ui: &mut egui::Ui,
    value: &mut Fp,
    range: std::ops::RangeInclusive<f32>,
    step: f64,
    dirty: &mut bool,
) {
    if ui
        .add(crate::ui::num::slider(&mut value.0, range).step_by(step))
        .changed()
    {
        *dirty = true;
    }
}

/// Three-component drag editor for `Fp3` half-extents (or any other
/// vec3-shaped numeric triple). Edits land in the underlying `[f32; 3]`
/// directly so the caller's `Fp3` wrapper picks up the change without an
/// intermediate copy.
pub(super) fn fp3_extents(ui: &mut egui::Ui, label: &str, value: &mut [f32; 3], dirty: &mut bool) {
    ui.label(label);
    ui.horizontal(|ui| {
        for axis in value.iter_mut() {
            if ui
                .add(crate::ui::num::drag(axis).speed(0.05).range(0.05..=20.0))
                .changed()
            {
                *dirty = true;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Move one authored value on whichever preset this is - the smallest
    /// possible "the owner tuned something".
    fn tune(cfg: &mut LocomotionConfig) {
        match cfg {
            LocomotionConfig::Humanoid(p) => {
                p.capsule_radius = crate::pds::types::Fp(p.capsule_radius.0 + 0.1);
            }
            LocomotionConfig::HoverBoat(p) => p.chassis_half_extents.0[0] += 0.1,
            LocomotionConfig::Car(p) => p.chassis_half_extents.0[0] += 0.1,
            LocomotionConfig::Helicopter(p) => p.chassis_half_extents.0[0] += 0.1,
            LocomotionConfig::Airplane(p) => p.chassis_half_extents.0[0] += 0.1,
            LocomotionConfig::Unknown => panic!("every pickable preset is a known one"),
        }
    }

    /// THE SEQUENCE (#1256 f102): tune the Car preset for ten minutes,
    /// click Helicopter to compare, click Car again - and every value is
    /// back to default, with no warning at any point.
    ///
    /// #838 guarded this with a confirm modal; #866 retired that in favour
    /// of undo, on the grounds that "a switch is now one Ctrl+Z away and the
    /// toast names it". No toast fired here: `undo_label.set` only names a
    /// future undo ENTRY, and the undo toast appears when the user presses
    /// Ctrl+Z - which is exactly the gesture someone who does not know they
    /// lost anything will never make. Undo is only a remedy for someone who
    /// notices within 32 edits.
    #[test]
    fn switching_away_from_tuned_settings_says_what_it_replaced() {
        // An untuned preset is still its own defaults: nothing is lost, so
        // nothing is said. Clicking around must not manufacture warnings.
        for (_, label, ctor) in LocomotionConfig::pickers() {
            let untouched = ctor();
            assert_eq!(
                discarded_tuning(&untouched),
                None,
                "an untouched {label} has no tuning to lose"
            );
        }

        // Tune each one, and the switch owes the owner a sentence naming it.
        for (_, label, ctor) in LocomotionConfig::pickers() {
            let mut tuned = ctor();
            tune(&mut tuned);
            assert_eq!(
                discarded_tuning(&tuned),
                Some(*label),
                "a tuned {label} must name itself as what the switch throws away"
            );
        }

        // And the sentence says what happened and how to undo it - the
        // whole point being that it arrives at the moment of the loss.
        let line = switch_discard_line("Car", "Helicopter");
        assert!(line.contains("Car") && line.contains("Helicopter"));
        assert!(
            line.contains("Ctrl+Z"),
            "the remedy has to be in the sentence: {line}"
        );
    }

    /// A record authored against a newer schema has no preset defaults to
    /// compare against, so a switch away from it claims no loss - the panel
    /// above already tells the owner to replace it.
    #[test]
    fn an_unrecognised_preset_claims_no_lost_tuning() {
        assert_eq!(discarded_tuning(&LocomotionConfig::Unknown), None);
    }
}

/// The Locomotion tab must not edit what it shows (#1390).
///
/// The tab draws the LIVE record - `TabCtx`'s own doc is "the live record
/// every tab edits in place", and `draw_tab` hands `draw_locomotion_tab` a
/// `&mut` to `ctx.record.locomotion` - so a widget that writes on sight
/// writes into the avatar, not into a copy. Two ways it did:
///
/// * **the range clamp.** egui 0.35's `SliderClamping` default is `Always`
///   ("Always clamp values, even existing ones"), and `Slider::add_contents`
///   acts on it with no input at all: `let old_value = self.get_value(); if
///   self.clamping == SliderClamping::Always { self.set_value(old_value); }`.
///   The hover-boat Mass slider was 5..=200 against seeded masses reaching
///   474, so a steam tug lost 212 kg for being looked at - and `changed`
///   stayed FALSE, so the change tick never moved and no peer was told.
/// * **the step snap.** `set_value` is also where `step_by`'s rounding is
///   applied, so every seeded value was quantised on arrival; that one DOES
///   report `changed`, which marks the avatar edited and queues a broadcast
///   for opening a tab.
///
/// Both are the same default, and `ui::num` exists to own defaults, so the
/// fix is one line there rather than one per call site.
#[cfg(test)]
mod inert_panel_tests {
    use super::*;
    use crate::pds::AvatarRecord;
    use crate::seeded_defaults::{AvatarPins, ChassisFamily, CraftType};
    use bevy_egui::egui;

    /// One seed per craft type from the craft pin's own hunt (#1380), plus
    /// an airship and a humanoid: fourteen records, every locomotion preset
    /// a seed can produce. The two non-craft families are found by pinning
    /// the chassis, which is the only axis they have.
    fn seeded_fleet() -> Vec<(String, u64, AvatarRecord)> {
        let mut out = Vec::new();
        for craft in CraftType::BOATS.into_iter().chain(CraftType::SKIFFS) {
            let mut pins = AvatarPins::default();
            pins.lock_craft(Some(craft));
            let seed = pins
                .find_seed(0)
                .unwrap_or_else(|| panic!("{} is reachable", craft.label()));
            out.push((
                craft.label().to_string(),
                seed,
                AvatarRecord::default_for_seed(seed),
            ));
        }
        for family in [ChassisFamily::Airship, ChassisFamily::Humanoid] {
            let mut pins = AvatarPins::default();
            pins.set_chassis(Some(family));
            let seed = pins
                .find_seed(0)
                .unwrap_or_else(|| panic!("{} is reachable", family.label()));
            out.push((
                family.label().to_string(),
                seed,
                AvatarRecord::default_for_seed(seed),
            ));
        }
        out
    }

    /// What one draw of the whole tab did to the record.
    struct Drawn {
        /// Every field whose value moved, as `name: before -> after`.
        moved: Vec<String>,
        /// Whether the panel raised the editor's dirty flag.
        dirty: bool,
        /// Whether the record grew a `gait` section it did not have.
        materialised_gait: bool,
        /// Every control the draw reached, by AccessKit label - the
        /// collapsing-section headers among them.
        controls: Vec<String>,
    }

    /// Draw the whole tab `frames` times over a bare context with no input
    /// at all, and report what it did to `record`.
    ///
    /// **Every collapsing section is open**, through
    /// `Memory::set_everything_is_visible`, which makes
    /// `CollapsingState::openness` 1.0 for every header and so runs every
    /// body. That is coverage by construction rather than by a list: a
    /// section added tomorrow is drawn here without anybody remembering to
    /// add it. It matters because session 834's one-frame probe reached
    /// only the default-open sections, which is exactly how it caught the
    /// boat's mass moving and missed her drive force - and how the
    /// Idle-motion section (`default_open(false)`) hid entirely.
    fn draw_tab_over(record: &mut AvatarRecord, seed: u64, frames: usize) -> Drawn {
        let before_cfg = record.locomotion.clone();
        let before_gait = record.gait.clone();
        let before = serde_json::to_value(&record.locomotion).expect("a locomotion config");
        let before_g = serde_json::to_value(&record.gait).expect("a gait section");
        let had_gait = record.gait.is_some();

        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.memory_mut(|m| m.set_everything_is_visible(true));
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0));

        let mut dirty = false;
        let mut controls: Vec<String> = Vec::new();
        for _ in 0..frames {
            let mut labels = crate::ui::undo::PendingUndoLabels::default();
            let mut toasts = crate::notify::Toasts::default();
            let movement = crate::player::LocalMovement::default();
            let input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                draw_locomotion_tab(
                    ui,
                    &mut record.locomotion,
                    &mut record.gait,
                    seed,
                    &mut dirty,
                    &mut labels.slot(crate::ui::shortcuts::EditorKind::Avatar),
                    &movement,
                    &mut toasts,
                    0.0,
                );
            });
            controls = button_labels(&output);
        }

        let after = serde_json::to_value(&record.locomotion).expect("a locomotion config");
        let after_g = serde_json::to_value(&record.gait).expect("a gait section");
        let mut moved = diff_numbers("", &before, &after);
        // The Idle-motion section is the same shape as the panels above it
        // and was never measured, because it is `default_open(false)` and
        // session 834's probe drew one frame. It draws a CLONE of the gait
        // through `fp_slider` and writes it back under `if changed`, so a
        // slider that reports a snap it invented materialises a section or
        // rewrites one.
        moved.extend(diff_numbers("gait", &before_g, &after_g));
        // The serialised form is the wire form, so the report above is
        // quantised to the record's own 1e-4 (`Fp` is an i32 scaled by
        // `FP_SCALE`). The live config is what the drive systems read, and
        // a change too small to reach the wire still reaches them - so the
        // equality that decides is the config's, and the diff is only the
        // sentence it gets to say.
        if moved.is_empty() && (before_cfg != record.locomotion || before_gait != record.gait) {
            moved.push(
                "the live config moved by less than the record's own 1e-4 resolution".to_string(),
            );
        }
        Drawn {
            moved,
            dirty,
            materialised_gait: !had_gait && record.gait.is_some(),
            controls,
        }
    }

    /// Every clickable control in the last pass's AccessKit tree, by label.
    ///
    /// A `CollapsingHeader` reports `Role::Button` with its header text
    /// (`response.rs` maps `WidgetType::CollapsingHeader` there), and so
    /// does a plain button; a `selectable_label` reports the same role but
    /// also carries `toggled`, which is how the preset picker's row is kept
    /// out. So this list is the sections plus the tab's own buttons, and a
    /// new section changes it.
    fn button_labels(output: &egui::FullOutput) -> Vec<String> {
        let Some(update) = output.platform_output.accesskit_update.as_ref() else {
            panic!("accesskit was enabled, so the pass has a tree");
        };
        let mut out: Vec<String> = update
            .nodes
            .iter()
            .filter(|(_, n)| {
                n.role() == bevy_egui::egui::accesskit::Role::Button && n.toggled().is_none()
            })
            .filter_map(|(_, n)| n.label().map(str::to_string))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Every number that differs between two serialised configs, as
    /// `path: before -> after`.
    ///
    /// The report is the point: "the record moved" is not a finding, and
    /// #1390's whole story is which field moved and by how much.
    fn diff_numbers(path: &str, a: &serde_json::Value, b: &serde_json::Value) -> Vec<String> {
        use serde_json::Value;
        match (a, b) {
            (Value::Object(a), Value::Object(b)) => a
                .iter()
                .flat_map(|(k, av)| {
                    let bv = b.get(k).unwrap_or(&Value::Null);
                    diff_numbers(
                        &format!("{path}{}{k}", if path.is_empty() { "" } else { "." }),
                        av,
                        bv,
                    )
                })
                .collect(),
            (Value::Array(a), Value::Array(b)) => a
                .iter()
                .zip(b)
                .enumerate()
                .flat_map(|(i, (av, bv))| diff_numbers(&format!("{path}[{i}]"), av, bv))
                .collect(),
            (a, b) if a != b => vec![format!("{path}: {} -> {}", unscaled(a), unscaled(b))],
            _ => Vec::new(),
        }
    }

    /// A wire scalar back in the unit the owner reads on the slider: every
    /// numeric field of a locomotion config is an `Fp`, an i32 scaled by
    /// `FP_SCALE`, so the raw JSON integer is metres x 10 000.
    fn unscaled(v: &serde_json::Value) -> String {
        match v.as_i64() {
            Some(n) => format!("{:.4}", n as f32 / crate::pds::types::FP_SCALE),
            None => v.to_string(),
        }
    }

    /// Every numeric control the tab draws, as its own AccessKit node
    /// reports it: the label painted above it, the value and the range.
    ///
    /// The range is read off the WIDGET rather than off a table copied
    /// from the panels, so the audit cannot drift from what is on screen:
    /// a `Slider` publishes `min_numeric_value` / `max_numeric_value`, and
    /// so does a `DragValue` given a `.range(..)`.
    struct Control {
        label: String,
        value: f64,
        min: f64,
        max: f64,
    }

    /// The numeric controls of one draw, in draw order, each paired with
    /// the label painted in front of it.
    ///
    /// `fp_slider` deliberately emits no inline label - the caller draws
    /// `ui.label("Mass (kg)")` above a stack of related sliders - so the
    /// slider's own AccessKit label is empty and the name has to come from
    /// the node before it.
    ///
    /// TWO TRAPS, both paid for once. `TreeUpdate::nodes` is NOT in draw
    /// order - it comes out of an id map, so a straight walk of it pairs
    /// a slider with whatever label hashed next to it - hence the depth
    /// first walk from the tree's own root, which is document order. And
    /// an `egui::Slider` reports TWICE, once as `Slider` and once as the
    /// `SpinButton` of the number beside it, with the same value and the
    /// same range; a bare `DragValue` (`fp3_extents`) reports only the
    /// second, so a `SpinButton` is kept unless the control just before it
    /// was a slider saying exactly the same thing.
    fn numeric_controls(output: &egui::FullOutput) -> Vec<Control> {
        use bevy_egui::egui::accesskit::{Node, NodeId, Role};
        use std::collections::HashMap;
        let Some(update) = output.platform_output.accesskit_update.as_ref() else {
            panic!("accesskit was enabled, so the pass has a tree");
        };
        let by_id: HashMap<NodeId, &Node> = update.nodes.iter().map(|(id, n)| (*id, n)).collect();
        let root = update.tree.as_ref().expect("the pass has a tree").root;

        let mut out: Vec<Control> = Vec::new();
        let mut last_label = String::new();
        let mut last_was_slider = false;
        // Explicit stack rather than recursion: the tree is the panel's,
        // not this test's, and a deep one should not be a stack overflow.
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let Some(node) = by_id.get(&id) else { continue };
            match node.role() {
                Role::Label => {
                    if let Some(v) = node.value() {
                        last_label = v.to_string();
                    }
                    last_was_slider = false;
                }
                role @ (Role::Slider | Role::SpinButton) => {
                    if let (Some(value), Some(min), Some(max)) = (
                        node.numeric_value(),
                        node.min_numeric_value(),
                        node.max_numeric_value(),
                    ) {
                        let companion = role == Role::SpinButton
                            && last_was_slider
                            && out.last().is_some_and(|c: &Control| {
                                c.value == value && c.min == min && c.max == max
                            });
                        if !companion {
                            let label = node
                                .label()
                                .filter(|l| !l.is_empty())
                                .map_or_else(|| last_label.clone(), str::to_string);
                            out.push(Control {
                                label,
                                value,
                                min,
                                max,
                            });
                        }
                        last_was_slider = role == Role::Slider;
                    }
                }
                _ => {}
            }
            // Children are pushed in reverse so the first one is popped
            // first, which makes the walk document order.
            stack.extend(node.children().iter().rev().copied());
        }
        out
    }

    /// The numeric controls the tab draws for one record, every section
    /// open, on a draw that writes nothing.
    fn controls_for(record: &mut AvatarRecord, seed: u64) -> Vec<Control> {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.memory_mut(|m| m.set_everything_is_visible(true));
        let mut labels = crate::ui::undo::PendingUndoLabels::default();
        let mut toasts = crate::notify::Toasts::default();
        let movement = crate::player::LocalMovement::default();
        let mut dirty = false;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 720.0),
            )),
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            draw_locomotion_tab(
                ui,
                &mut record.locomotion,
                &mut record.gait,
                seed,
                &mut dirty,
                &mut labels.slot(crate::ui::shortcuts::EditorKind::Avatar),
                &movement,
                &mut toasts,
                0.0,
            );
        });
        numeric_controls(&output)
    }

    /// PRINT-ONLY. What every slider-backed field of the Locomotion tab
    /// actually derives, over seeds 0..4000, against the range its own
    /// slider offers (#1390 design call 2).
    ///
    /// Under `SliderClamping::Edits` an out-of-range value is no longer
    /// corrupted by being looked at - but it still cannot be DRAGGED,
    /// because the track only spans the range. So the ranges have to hold
    /// the fleet, and the only way to know is to derive the fleet and
    /// look. Re-run it after any retune that moves a derived field
    /// (#1381's tuples move four of them).
    #[test]
    #[ignore = "audit for #1390: what the seeds derive against what the sliders offer"]
    fn audit_every_slider_range_against_what_the_seeds_derive() {
        use std::collections::BTreeMap;
        /// One control's span over the seeds: min and max seen, the range
        /// its own slider offers, and the seed of the worst excursion.
        type Band = (f64, f64, f64, f64, u64);
        let mut bands: BTreeMap<String, BTreeMap<String, Band>> = BTreeMap::new();
        for seed in 0u64..4000 {
            let mut record = AvatarRecord::default_for_seed(seed);
            let preset = record.locomotion.kind_tag().to_string();
            for c in controls_for(&mut record, seed) {
                let e = bands
                    .entry(preset.clone())
                    .or_default()
                    .entry(c.label.clone())
                    .or_insert((c.value, c.value, c.min, c.max, seed));
                if c.value < e.0 {
                    e.0 = c.value;
                    if c.value < c.min {
                        e.4 = seed;
                    }
                }
                if c.value > e.1 {
                    e.1 = c.value;
                    if c.value > c.max {
                        e.4 = seed;
                    }
                }
            }
        }
        for (preset, fields) in &bands {
            println!("\n== {preset} ==");
            println!(
                "{:<38} {:>12} {:>12}   {:>12} {:>12}  verdict",
                "control", "seeds min", "seeds max", "slider lo", "slider hi"
            );
            for (label, (lo, hi, rlo, rhi, worst)) in fields {
                let out = *lo < *rlo || *hi > *rhi;
                println!(
                    "{:<38} {:>12.4} {:>12.4}   {:>12.4} {:>12.4}  {}",
                    label,
                    lo,
                    hi,
                    rlo,
                    rhi,
                    if out {
                        format!("OUT OF RANGE (seed {worst})")
                    } else {
                        "ok".to_string()
                    }
                );
            }
        }
    }

    /// THE GUARD. Drawing the Locomotion tab, every section open and
    /// nothing touched, leaves the record bit-identical and the editor
    /// clean - on all fourteen seeded presets (#1390).
    #[test]
    fn an_untouched_locomotion_tab_writes_nothing() {
        let mut complaints = Vec::new();
        let mut sections: Vec<String> = Vec::new();
        for (name, seed, mut record) in seeded_fleet() {
            // Two frames: the first lays the panel out, the second draws it
            // against a settled layout, and a widget that writes on sight
            // writes on both.
            let drawn = draw_tab_over(&mut record, seed, 2);
            sections.extend(drawn.controls.iter().cloned());
            for moved in &drawn.moved {
                complaints.push(format!("{name} (seed {seed}): {moved}"));
            }
            if drawn.dirty {
                complaints.push(format!(
                    "{name} (seed {seed}): the panel raised `dirty` with no input"
                ));
            }
            if drawn.materialised_gait {
                complaints.push(format!(
                    "{name} (seed {seed}): the panel materialised a gait section on a record \
                     that had none"
                ));
            }
        }
        sections.sort();
        sections.dedup();
        assert!(
            complaints.is_empty(),
            "an untouched Locomotion tab wrote to the live record.\n  sections drawn: {}\n{}",
            sections.join(" | "),
            complaints.join("\n")
        );
    }
}
