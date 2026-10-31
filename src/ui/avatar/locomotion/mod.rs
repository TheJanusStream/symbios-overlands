//! Locomotion-tab UI: per-preset slider panel plus the central preset
//! picker. Each preset's [`LocomotionPanel`] impl lives in its own
//! submodule so adding a new preset is one new file + one match arm in
//! [`draw_locomotion_tab`]. The On foot panel is the one exception, an
//! inherent method that is also given the body's kind, since a person and
//! a beast on it are offered different rows (#1803).
//!
//! The On foot preset carries a person upright or a four-legged beast lying
//! along its capsule (#1802). A beast's collider is the body's, refitted on
//! every edit of it on the Body tab, so its panel reads it rather than
//! offering it to drag, and picking On foot under a beast fits the preset
//! to it (#1803).

mod airplane;
mod car;
mod gait;
mod helicopter;
mod hover_boat;
mod humanoid;

use bevy_egui::egui;

use crate::pds::avatar::EngineAvatarRecord;
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
            // The body the preset carries, for the one preset sized by it
            // (#1803): disjoint from the two fields the panel edits.
            let record = &mut *ctx.record;
            let body = record
                .body
                .rigged_ref()
                .and_then(|rig| rig.resolved.as_ref())
                .map(|resolved| &resolved.body);
            draw_locomotion_tab(
                ui,
                &mut record.locomotion,
                &mut record.gait,
                body,
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
///
/// A beast's On foot preset is measured against what picking it gives that
/// body - the preset fitted to it (#1803) - since that is the tuning nobody
/// chose, where a person's is the preset's own defaults.
fn discarded_tuning(
    current: &LocomotionConfig,
    body: Option<&EngineAvatarRecord>,
) -> Option<&'static str> {
    let kind = current.kind_tag();
    LocomotionConfig::pickers()
        .iter()
        .find(|(k, _, _)| *k == kind)
        .filter(|(_, _, ctor)| picked(*ctor, body) != *current)
        .map(|(_, label, _)| *label)
}

/// What picking a preset gives the avatar: the preset's own defaults, and
/// for the On foot preset under a beast, those fitted to it (#1803, owner
/// decision D3) - a beast never stands on a person's upright capsule, even
/// for a frame. A person's are the defaults, untouched.
fn picked(ctor: fn() -> LocomotionConfig, body: Option<&EngineAvatarRecord>) -> LocomotionConfig {
    let mut config = ctor();
    if let (LocomotionConfig::Humanoid(p), Some(body)) = (&mut config, body)
        && let Some(fit) = crate::pds::avatar::locomotion::follow(body, None, p, &mut None)
    {
        **p = fit;
    }
    config
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
    // The body the avatar wears, when it is a resolved rigged one: what
    // the On foot preset is sized by under a beast (#1803).
    body: Option<&EngineAvatarRecord>,
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
                let lost = discarded_tuning(locomotion, body);
                *locomotion = picked(*ctor, body);
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
        LocomotionConfig::Humanoid(p) => {
            let beast = humanoid::carries_beast(body, p);
            p.draw_on_foot(ui, dirty, facts, beast);
        }
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
                discarded_tuning(&untouched, None),
                None,
                "an untouched {label} has no tuning to lose"
            );
        }

        // Tune each one, and the switch owes the owner a sentence naming it.
        for (_, label, ctor) in LocomotionConfig::pickers() {
            let mut tuned = ctor();
            tune(&mut tuned);
            assert_eq!(
                discarded_tuning(&tuned, None),
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
        assert_eq!(discarded_tuning(&LocomotionConfig::Unknown, None), None);
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
    /// a humanoid and a beast: twenty-four records, every locomotion preset a
    /// seed can produce - the airplanes' since #1623, the autogyro's since
    /// #1624, the twin's since #1625, the coaxial's since #1628, the racer's
    /// since #1629, the tandem's since #1630, the Zeppelin's since #1657, when
    /// the airship stopped being a non-craft family, and a beast's since
    /// #1808. The two rigged bodies are found by pinning the chassis, which is
    /// the only axis either has.
    fn seeded_fleet() -> Vec<(String, u64, AvatarRecord)> {
        let mut out = Vec::new();
        for craft in CraftType::BOATS
            .into_iter()
            .chain(CraftType::SKIFFS)
            .chain(CraftType::HELICOPTERS)
            .chain(CraftType::AIRPLANES)
            .chain(CraftType::AIRSHIPS)
        {
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
        for family in [ChassisFamily::Humanoid, ChassisFamily::Beast] {
            let mut pins = AvatarPins::default();
            pins.set_chassis(Some(family));
            let seed = pins.find_seed(0).expect("a rigged family is reachable");
            out.push((
                family.label().to_string(),
                seed,
                AvatarRecord::default_for_seed(seed),
            ));
        }
        out
    }

    /// The engine body a record's rigged body resolves, as the tab is given
    /// it.
    fn worn(body: &crate::pds::avatar::AvatarBody) -> Option<&EngineAvatarRecord> {
        body.rigged_ref()
            .and_then(|rig| rig.resolved.as_ref())
            .map(|resolved| &resolved.body)
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
                    worn(&record.body),
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
                worn(&record.body),
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

    /// #1524: the Car panel's "In the air" section draws the air model as it
    /// finds it and writes nothing - a value between its sliders' steps is
    /// not snapped, and one past a slider's range but inside the sanitiser's
    /// is not clamped. The seeded fleet above only ever shows these sliders
    /// their defaults, which sit on the steps.
    #[test]
    fn the_in_the_air_section_writes_nothing() {
        use crate::pds::types::Fp;
        for (linear, angular, control, level) in [
            // Between the steps.
            (0.0137, 0.4321, 3.3333, 7.7777),
            // Past the sliders' ranges, inside the sanitiser's.
            (5.5, 15.5, 33.3, 44.4),
        ] {
            let mut record = AvatarRecord::default_for_did("did:plc:in-the-air-panel");
            let car = crate::pds::CarParams {
                air_linear_damping: Fp(linear),
                air_angular_damping: Fp(angular),
                air_control_accel: Fp(control),
                air_level_accel: Fp(level),
                ..Default::default()
            };
            record.locomotion = LocomotionConfig::Car(Box::new(car));
            let drawn = draw_tab_over(&mut record, 0, 2);
            assert!(
                drawn.controls.iter().any(|c| c == "In the air"),
                "the section is drawn: {:?}",
                drawn.controls
            );
            assert!(
                drawn.moved.is_empty() && !drawn.dirty,
                "drawing the air model wrote to it: {:?}, dirty {}",
                drawn.moved,
                drawn.dirty
            );
        }
    }

    /// THE GUARD. Drawing the Locomotion tab, every section open and
    /// nothing touched, leaves the record bit-identical and the editor
    /// clean - on all twenty-three seeded presets (#1390), the helicopters'
    /// (#1614, #1621, #1624, #1628, #1630) and the airplanes' (#1617, #1623,
    /// #1625, #1629) among them.
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

    /// Every line of text a pass painted, as AccessKit reads it: each
    /// label's text once, the per-row `TextRun` children of a wrapped one
    /// skipped.
    fn painted(output: &egui::FullOutput) -> Vec<String> {
        use bevy_egui::egui::accesskit::Role;
        let update = output
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("accesskit was enabled, so the pass has a tree");
        update
            .nodes
            .iter()
            .filter(|(_, n)| n.role() == Role::Label)
            .filter_map(|(_, n)| n.value().or(n.label()).map(str::to_string))
            .collect()
    }

    /// One frame of the tab over `record`, every section open, with
    /// `events` as its input; what it painted, its controls, and whether it
    /// raised `dirty`.
    fn pass(
        ctx: &egui::Context,
        record: &mut AvatarRecord,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, bool) {
        let mut labels = crate::ui::undo::PendingUndoLabels::default();
        let mut toasts = crate::notify::Toasts::default();
        let movement = crate::player::LocalMovement::default();
        let mut dirty = false;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 1400.0),
            )),
            events,
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            draw_locomotion_tab(
                ui,
                &mut record.locomotion,
                &mut record.gait,
                worn(&record.body),
                0,
                &mut dirty,
                &mut labels.slot(crate::ui::shortcuts::EditorKind::Avatar),
                &movement,
                &mut toasts,
                0.0,
            );
        });
        (output, dirty)
    }

    /// A record wearing `family`'s default beast, resolved, on its fitted
    /// On foot preset.
    fn beast(family: symbios_avatar::plan::Family) -> AvatarRecord {
        use crate::pds::avatar::LocomotionPreset;
        let body = crate::pds::avatar::locomotion::default_beast(family);
        let mut record = AvatarRecord::wearing("3jzfcijpj2z2a");
        record.locomotion =
            crate::pds::avatar::fit_on_foot(&body, &crate::pds::HumanoidParams::default())
                .into_config();
        if let Some(rig) = record.body.rigged_mut() {
            rig.resolved = Some(crate::pds::avatar::ResolvedRig {
                body,
                attachments: Vec::new(),
            });
        }
        record
    }

    /// **A beast's Locomotion tab reads its collider and offers no capsule
    /// to drag** (#1803, owner decisions D3 and D7): the radius, length,
    /// mass and jump sliders are a read-only "Body collider" readout that
    /// says it follows the body - its length, its depth, the height of its
    /// back (never the lying capsule's diameter, which is only the trunk's
    /// depth) and where it is centred - while the run, the derived walk's
    /// sliders and the water rows stay. Drawing it writes nothing. A
    /// person's tab, the control, keeps its capsule sliders and its total
    /// height.
    #[test]
    fn a_beasts_tab_reads_its_collider_and_a_persons_keeps_its_capsule() {
        use symbios_avatar::plan::Family;
        let capsule_rows = [
            "Radius (m)",
            "Cylinder length (m)",
            "Mass (kg)",
            "Jump impulse (N·s)",
        ];
        let kept_rows = [
            "Run speed (m/s)",
            "Linear damping",
            "Acceleration (1/s)",
            "Swim speed (m/s)",
            "Wading speed factor",
        ];
        for family in [Family::Dog, Family::Cat, Family::Horse] {
            let mut record = beast(family);
            let p = match &record.locomotion {
                LocomotionConfig::Humanoid(p) => (**p).clone(),
                _ => unreachable!("on foot"),
            };
            let drawn = draw_tab_over(&mut record, 0, 2);
            assert!(
                drawn.moved.is_empty() && !drawn.dirty,
                "{family:?}: drawing the beast's tab wrote {:?}, dirty {}",
                drawn.moved,
                drawn.dirty
            );
            assert!(
                drawn.controls.iter().any(|c| c == "Body collider")
                    && !drawn.controls.iter().any(|c| c == "Capsule"),
                "{family:?}: sections {:?}",
                drawn.controls
            );
            let rows: Vec<String> = controls_for(&mut record, 0)
                .into_iter()
                .map(|c| c.label)
                .collect();
            for row in capsule_rows {
                assert!(!rows.iter().any(|r| r == row), "{family:?} offers {row:?}");
            }
            for row in kept_rows {
                assert!(
                    rows.iter().any(|r| r == row),
                    "{family:?} lost {row:?}: {rows:?}"
                );
            }
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            ctx.memory_mut(|m| m.set_everything_is_visible(true));
            let (output, _) = pass(&ctx, &mut record, Vec::new());
            let text = painted(&output);
            for line in humanoid::collider_lines(&p)
                .into_iter()
                .chain([humanoid::FOLLOWS_THE_BODY.to_string()])
            {
                assert!(
                    text.contains(&line),
                    "{family:?} paints no {line:?}: {text:?}"
                );
            }
            let back = format!("Back height {:.2} m over the feet", p.stand_height());
            assert!(
                p.stand_height() > p.total_height() + 0.05,
                "{family:?}: the premise, a back above the capsule's top"
            );
            assert!(text.contains(&back), "{family:?}: {text:?}");
            assert!(
                !text.iter().any(|t| t.starts_with("Total height")),
                "{family:?} shows the capsule's diameter as its height"
            );
        }

        let mut person = AvatarRecord::wearing("3jzfcijpj2z2a");
        let rows: Vec<String> = controls_for(&mut person, 0)
            .into_iter()
            .map(|c| c.label)
            .collect();
        for row in capsule_rows.into_iter().chain(kept_rows) {
            assert!(
                rows.iter().any(|r| r == row),
                "a person lost {row:?}: {rows:?}"
            );
        }
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.memory_mut(|m| m.set_everything_is_visible(true));
        let (output, _) = pass(&ctx, &mut person, Vec::new());
        let text = painted(&output);
        assert!(
            text.iter().any(|t| t.starts_with("Total height")),
            "{text:?}"
        );
        assert!(!text.iter().any(|t| t == humanoid::FOLLOWS_THE_BODY));
    }

    /// **Picking On foot under a beast stands it on its own collider**
    /// (#1803, D3): the preset the picker makes is fitted to the body, not
    /// a person's upright capsule for the beast to hang from until its next
    /// edit - and since that is what the beast gets untouched, switching
    /// away from it again claims no lost tuning. A person picking it gets
    /// the preset's defaults, as before.
    #[test]
    fn picking_on_foot_under_a_beast_fits_it() {
        use crate::pds::avatar::LocomotionPreset;
        use bevy_egui::egui::accesskit::{Action, ActionRequest, TreeId};
        let mut record = beast(symbios_avatar::plan::Family::Horse);
        let want = record.locomotion.clone();
        record.locomotion = crate::pds::CarParams::default_config();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let (output, _) = pass(&ctx, &mut record, Vec::new());
        let update = output.platform_output.accesskit_update.expect("a tree");
        let on_foot = update
            .nodes
            .iter()
            .find(|(_, n)| n.label() == Some("On foot") && n.toggled().is_some())
            .map(|(id, _)| *id)
            .expect("the picker offers On foot");
        let click = vec![egui::Event::AccessKitActionRequest(ActionRequest {
            action: Action::Click,
            target_tree: TreeId::ROOT,
            target_node: on_foot,
            data: None,
        })];
        let (_, dirty) = pass(&ctx, &mut record, click);
        assert!(dirty, "the switch reported no change");
        assert_eq!(
            record.locomotion, want,
            "the horse is not on its own collider"
        );
        assert_eq!(
            discarded_tuning(&record.locomotion, worn(&record.body)),
            None,
            "an untouched beast claims lost tuning"
        );

        let mut person = AvatarRecord::wearing("3jzfcijpj2z2a");
        if let Some(rig) = person.body.rigged_mut() {
            rig.resolved = Some(crate::pds::avatar::ResolvedRig {
                body: crate::pds::avatar::wardrobe::engine_default_for_did("did:plc:picker"),
                attachments: Vec::new(),
            });
        }
        assert!(
            worn(&person.body).is_some(),
            "the premise: a resolved person"
        );
        assert_eq!(
            picked(
                crate::pds::HumanoidParams::default_config,
                worn(&person.body)
            ),
            crate::pds::HumanoidParams::default_config()
        );
        person.locomotion = crate::pds::CarParams::default_config();
        assert_eq!(
            discarded_tuning(&person.locomotion, worn(&person.body)),
            None
        );
    }

    /// **The On foot panel reads the body's kind, not the preset's shape**
    /// (#1803): a person under a lying preset - another app re-saved their
    /// wardrobe entry as a person under a beast's collider - is offered a
    /// person's capsule sliders, not a beast's read-only "Body collider"
    /// that says it follows the body; a beast is shown its collider
    /// whatever shape the preset has; and only with no body resolved does
    /// the shape decide.
    #[test]
    fn the_on_foot_panel_reads_the_bodys_kind() {
        use crate::pds::avatar::LocomotionPreset;
        use symbios_avatar::plan::Family;
        let horse = beast(Family::Horse);
        let lying = horse.locomotion.clone();
        let sections = |record: &mut AvatarRecord| draw_tab_over(record, 0, 2).controls;
        let shows = |sections: &[String], header: &str| sections.iter().any(|c| c == header);

        // A person under the horse's lying collider.
        let mut person = AvatarRecord::wearing("3jzfcijpj2z2a");
        if let Some(rig) = person.body.rigged_mut() {
            rig.resolved = Some(crate::pds::avatar::ResolvedRig {
                body: crate::pds::avatar::wardrobe::engine_default_for_did("did:plc:mirror"),
                attachments: Vec::new(),
            });
        }
        person.locomotion = lying.clone();
        let drawn = sections(&mut person);
        assert!(
            shows(&drawn, "Capsule") && !shows(&drawn, "Body collider"),
            "a person under a lying preset: {drawn:?}"
        );
        let rows: Vec<String> = controls_for(&mut person, 0)
            .into_iter()
            .map(|c| c.label)
            .collect();
        assert!(
            rows.iter().any(|r| r == "Radius (m)")
                && rows.iter().any(|r| r == "Jump impulse (N·s)"),
            "a person is not offered their capsule: {rows:?}"
        );

        // A beast on an upright preset.
        let mut upright = horse.clone();
        upright.locomotion = crate::pds::HumanoidParams::default_config();
        let drawn = sections(&mut upright);
        assert!(
            shows(&drawn, "Body collider") && !shows(&drawn, "Capsule"),
            "a beast on an upright preset: {drawn:?}"
        );

        // No body resolved: the shape decides.
        let mut unresolved = AvatarRecord::wearing("3jzfcijpj2z2a");
        unresolved.locomotion = lying;
        assert!(worn(&unresolved.body).is_none(), "the premise");
        assert!(shows(&sections(&mut unresolved), "Body collider"));
        unresolved.locomotion = crate::pds::HumanoidParams::default_config();
        assert!(shows(&sections(&mut unresolved), "Capsule"));
    }
}
