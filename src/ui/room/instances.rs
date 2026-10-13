//! The World Editor's instance sets (#1643): what a walker meets at a set's
//! copies, its variants - each a grammar seed and grammar values - and its
//! copies, one row each; and the grammar values an absolute placement gives
//! its item. The set's item, snapping and anchor are drawn by the
//! Placements tab, as every placement's are.
//!
//! A copy's numbers are edited as the record holds them, in whole
//! centimetres, tenths of a degree and thousandths of a size, and only
//! shown as metres, degrees and multiples: an edit never rounds what it
//! did not change.

use bevy_egui::egui;

use crate::pds::instances::{COPY_SCALE_ONE, centimetres};
use crate::pds::sanitize::instances::is_param_name;
use crate::pds::sanitize::limits;
use crate::pds::{Fp, GrammarParams, InstanceCopy, InstanceSolid, InstanceVariant, TransformData};

/// How many copy rows show before the table scrolls.
const COPY_ROWS_SHOWN: f32 = 10.0;

/// Why a variant copies draw cannot be removed, beside its greyed button
/// and on it (#1289).
const VARIANT_STAYS: &str = "copies draw it, so it stays";

/// Why no copy can be added, beside its greyed button and on it.
const SET_FULL: &str = "A set holds no more copies.";

/// A set's own fields, borrowed from its placement.
pub(super) struct SetFields<'a> {
    pub solid: &'a mut InstanceSolid,
    pub variants: &'a mut Vec<InstanceVariant>,
    pub copies: &'a mut Vec<InstanceCopy>,
}

/// A set's solid, variants and copies, for the placement numbered `item`
/// whose item's grammars draw with their own seeds `own` (#1505).
pub(super) fn draw_instance_set(
    ui: &mut egui::Ui,
    item: usize,
    set: SetFields,
    own: &[u64],
    dirty: &mut bool,
) {
    let SetFields {
        solid,
        variants,
        copies,
    } = set;
    draw_solid(ui, item, solid, dirty);
    draw_variants(ui, item, variants, copies, own, dirty);
    draw_copies(ui, item, copies, variants.len().max(1), dirty);
}

/// The words a set's solid may be, with what each is for.
const SOLIDS: [(InstanceSolid, &str); 4] = [
    (InstanceSolid::Shell, "Shell (a building's)"),
    (InstanceSolid::Box, "Box (a prop's)"),
    (InstanceSolid::Trunk, "Trunk (a tree's)"),
    (InstanceSolid::None, "None (ground cover)"),
];

fn draw_solid(ui: &mut egui::Ui, item: usize, solid: &mut InstanceSolid, dirty: &mut bool) {
    let shown = SOLIDS.iter().find(|(word, _)| word == solid).map_or_else(
        || format!("{} (met as a shell)", solid.word()),
        |(_, name)| (*name).to_string(),
    );
    ui.horizontal(|ui| {
        ui.label("Solid").on_hover_text(
            "What a walker meets at each copy: copies are drawn merged, so their \
             parts have no colliders of their own.",
        );
        egui::ComboBox::from_id_salt(("instance_solid", item))
            .selected_text(shown)
            .show_ui(ui, |ui| {
                for (word, name) in SOLIDS {
                    let chosen = *solid == word;
                    if ui.selectable_label(chosen, name).clicked() && !chosen {
                        *solid = word;
                        *dirty = true;
                    }
                }
            });
    });
}

/// The variants: none in the record is one, the item as it is, shown and
/// edited as such and written back as none while it is still that. Their
/// rows are built only while the section is open: a set may hold hundreds.
fn draw_variants(
    ui: &mut egui::Ui,
    item: usize,
    variants: &mut Vec<InstanceVariant>,
    copies: &mut [InstanceCopy],
    own: &[u64],
    dirty: &mut bool,
) {
    egui::CollapsingHeader::new(format!("Variants ({})", variants.len().max(1)))
        .id_salt(("instance_variants", item))
        .default_open(false)
        .show(ui, |ui| {
            if edit_variants(ui, item, variants, copies, own) {
                *dirty = true;
            }
        });
}

/// The variants' rows. Whether any changed.
fn edit_variants(
    ui: &mut egui::Ui,
    item: usize,
    variants: &mut Vec<InstanceVariant>,
    copies: &mut [InstanceCopy],
    own: &[u64],
) -> bool {
    let mut shown = if variants.is_empty() {
        vec![InstanceVariant::default()]
    } else {
        variants.clone()
    };
    // How many copies draw each, counted in one pass over the copies.
    let mut users = vec![0usize; shown.len()];
    for copy in copies.iter() {
        if let Some(n) = users.get_mut(copy.variant as usize) {
            *n += 1;
        }
    }
    let mut changed = false;
    let mut removed = None;
    let weak = crate::ui::theme::current(ui.ctx()).text_weak;
    ui.label(
        egui::RichText::new(
            "What each copy's shape grammars draw with: a seed in place of their \
             own, and values they read in place of their own of those names.",
        )
        .small()
        .color(weak),
    );
    let count = shown.len();
    for (v, variant) in shown.iter_mut().enumerate() {
        ui.push_id(("instance_variant", item, v), |ui| {
            let used = users[v];
            ui.horizontal(|ui| {
                ui.strong(format!("Variant {v}"));
                ui.label(
                    egui::RichText::new(format!("{used} copies"))
                        .small()
                        .color(weak),
                );
                // Only one no copy draws can go, so no copy changes its
                // look for a removal; the last one is the item as it is.
                if count > 1
                    && ui
                        .add_enabled(used == 0, egui::Button::new("Remove").small())
                        .on_disabled_hover_text(VARIANT_STAYS)
                        .clicked()
                {
                    removed = Some(v);
                }
            });
            if count > 1 && used > 0 {
                ui.label(egui::RichText::new(VARIANT_STAYS).small().color(weak));
            }
            changed |= draw_variant_seed(ui, variant, own);
            changed |= draw_grammar_params(ui, &mut variant.params);
        });
        ui.separator();
    }
    if shown.len() < limits::MAX_INSTANCE_VARIANTS
        && ui
            .button("Add a variant")
            .on_hover_text("A copy of the last variant, to change.")
            .clicked()
    {
        let last = shown.last().cloned().unwrap_or_default();
        shown.push(last);
        changed = true;
    }
    if let Some(v) = removed {
        shown.remove(v);
        for copy in copies.iter_mut() {
            if copy.variant as usize > v {
                copy.variant -= 1;
            }
        }
        changed = true;
    }
    if changed {
        *variants = if shown == [InstanceVariant::default()] {
            Vec::new()
        } else {
            shown
        };
    }
    changed
}

/// A variant's seed: on, its number; off, the grammars' own. Whether it
/// changed.
fn draw_variant_seed(ui: &mut egui::Ui, variant: &mut InstanceVariant, own: &[u64]) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let mut on = variant.seed.is_some();
        if ui
            .checkbox(&mut on, "Grammar seed")
            .on_hover_text("Off, each shape grammar draws with its own seed.")
            .changed()
        {
            variant.seed = on.then(|| own.first().copied().unwrap_or_default());
            changed = true;
        }
        if let Some(seed) = &mut variant.seed {
            changed |= super::widgets::u64_text_field(
                ui,
                "variant_seed",
                seed,
                "The seed every shape grammar of these copies derives with. Type a \
                 whole number and press Enter to apply it.",
            );
        }
    });
    changed
}

/// The grammar values `params` gives an item's shape grammars (#1643): each
/// value's number and a way to remove it, and a name to add one under.
/// Whether any changed.
pub(super) fn draw_grammar_params(ui: &mut egui::Ui, params: &mut GrammarParams) -> bool {
    let mut changed = false;
    let mut removed = None;
    let bound = limits::MAX_GRAMMAR_PARAM_VALUE;
    for (name, value) in params.0.iter_mut() {
        ui.horizontal(|ui| {
            ui.label(name.as_str());
            if ui
                .add(
                    crate::ui::num::drag(&mut value.0)
                        .speed(0.1)
                        .range(-bound..=bound),
                )
                .changed()
            {
                changed = true;
            }
            if ui.small_button("Remove").clicked() {
                removed = Some(name.clone());
            }
        });
    }
    if let Some(name) = removed {
        params.0.remove(&name);
        changed = true;
    }
    if params.0.len() >= limits::MAX_GRAMMAR_PARAMS {
        return changed;
    }
    let id = ui.make_persistent_id("grammar_value_name");
    let mut draft: String = ui.data_mut(|data| data.get_temp(id).unwrap_or_default());
    let weak = crate::ui::theme::current(ui.ctx()).text_weak;
    ui.horizontal(|ui| {
        let field = crate::ui::affordances::text_edit(
            ui,
            egui::TextEdit::singleline(&mut draft)
                .hint_text("value name")
                .desired_width(110.0),
        );
        let why = name_refusal(&draft, params);
        let add = ui
            .add_enabled(why.is_empty(), egui::Button::new("Add value"))
            .on_disabled_hover_text(why);
        let entered = field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if why.is_empty() && (add.clicked() || entered) {
            params.0.insert(std::mem::take(&mut draft), Fp(0.0));
            changed = true;
        }
    });
    // Said beside the button as well as on it: a greyed button is easy to
    // pass over (#1289).
    let why = name_refusal(&draft, params);
    if !why.is_empty() {
        ui.label(egui::RichText::new(why).small().color(weak));
    }
    ui.data_mut(|data| data.insert_temp(id, draft));
    changed
}

/// Why a value cannot be added under the name `draft`, or `""` where it
/// can.
fn name_refusal(draft: &str, params: &GrammarParams) -> &'static str {
    if draft.is_empty() {
        "Name a value a shape grammar reads, as its attr or const is named."
    } else if params.0.contains_key(draft) {
        "That value is set already."
    } else if !is_param_name(draft) {
        "A name is letters, digits and _, not starting with a digit."
    } else {
        ""
    }
}

/// The copies, a row each - its numbers as the record holds them - with a
/// row's removal and a copy added at the anchor.
fn draw_copies(
    ui: &mut egui::Ui,
    item: usize,
    copies: &mut Vec<InstanceCopy>,
    variants: usize,
    dirty: &mut bool,
) {
    egui::CollapsingHeader::new(format!("Copies ({})", copies.len()))
        .id_salt(("instance_copies", item))
        .default_open(false)
        .show(ui, |ui| {
            let weak = crate::ui::theme::current(ui.ctx()).text_weak;
            ui.label(
                egui::RichText::new(
                    "Each copy: X and Z from the anchor (m), its turn clockwise (deg), its \
                     variant, its size, and Y above the ground under it (m).",
                )
                .small()
                .color(weak),
            );
            let row_height = ui.spacing().interact_size.y;
            let mut removed = None;
            egui::ScrollArea::vertical()
                .id_salt(("instance_copy_rows", item))
                .max_height(row_height * COPY_ROWS_SHOWN)
                .show_rows(ui, row_height, copies.len(), |ui, rows| {
                    for c in rows {
                        ui.push_id(("instance_copy", c), |ui| {
                            ui.horizontal(|ui| {
                                ui.label(format!("#{c}"));
                                if draw_copy(ui, &mut copies[c], variants) {
                                    *dirty = true;
                                }
                                if ui.small_button("Remove").clicked() {
                                    removed = Some(c);
                                }
                            });
                        });
                    }
                });
            if let Some(c) = removed {
                copies.remove(c);
                *dirty = true;
            }
            let room = copies.len() < limits::MAX_INSTANCE_COPIES;
            if ui
                .add_enabled(room, egui::Button::new("Add a copy at the anchor"))
                .on_disabled_hover_text(SET_FULL)
                .clicked()
            {
                copies.push(InstanceCopy::at(0.0, 0.0, 0.0));
                *dirty = true;
            }
            if !room {
                ui.label(egui::RichText::new(SET_FULL).small().color(weak));
            }
        });
}

/// One copy's numbers. Whether any changed.
fn draw_copy(ui: &mut egui::Ui, copy: &mut InstanceCopy, variants: usize) -> bool {
    let offset = limits::MAX_INSTANCE_OFFSET_CM;
    let mut changed = false;
    changed |= ui
        .add(scaled(&mut copy.x_cm, 100.0, "X", 5.0).range(-offset..=offset))
        .changed();
    changed |= ui
        .add(scaled(&mut copy.z_cm, 100.0, "Z", 5.0).range(-offset..=offset))
        .changed();
    changed |= ui
        .add(scaled(&mut copy.yaw_ddeg, 10.0, "Turn", 10.0).range(0..=3_599))
        .changed();
    let last = u32::try_from(variants.saturating_sub(1)).unwrap_or(u32::MAX);
    changed |= ui
        .add(
            crate::ui::num::drag(&mut copy.variant)
                .range(0..=last)
                .prefix("v"),
        )
        .changed();
    changed |= ui
        .add(
            scaled(&mut copy.scale_pm, f64::from(COPY_SCALE_ONE), "x", 5.0)
                .range(1..=limits::MAX_INSTANCE_SCALE_PM),
        )
        .changed();
    changed |= ui
        .add(scaled(&mut copy.y_cm, 100.0, "Y", 1.0).range(-offset..=offset))
        .changed();
    changed
}

/// A whole number of hundredths, tenths or thousandths, dragged and typed
/// as the number it stands for: `unit` of it make one.
fn scaled<'a, Num: egui::emath::Numeric>(
    value: &'a mut Num,
    unit: f64,
    prefix: &'static str,
    speed: f64,
) -> egui::DragValue<'a> {
    crate::ui::num::drag(value)
        .speed(speed)
        .prefix(format!("{prefix} "))
        .custom_formatter(move |n, _| format!("{}", n / unit))
        .custom_parser(move |text| crate::ui::num::locale_number(text).map(|n| (n * unit).round()))
}

/// Stand a set's copies where they were as it stops or starts snapping
/// (#700's rule, which a grid keeps): the anchor on the ground under it,
/// and each copy's `y` rewritten so the copy stays where it was drawn.
/// `ground` is the height under a point.
pub(super) fn keep_copies_standing(
    transform: &mut TransformData,
    copies: &mut [InstanceCopy],
    snapping_now: bool,
    ground: &dyn Fn(f32, f32) -> f32,
) {
    let anchor = bevy::prelude::Transform::from(&*transform);
    let anchor_ground = ground(anchor.translation.x, anchor.translation.z);
    for copy in copies.iter_mut() {
        let [x, y, z] = copy.position();
        let at = anchor.translation + anchor.rotation * bevy::prelude::Vec3::new(x, 0.0, z);
        let under = ground(at.x, at.z);
        // Where it was drawn: unsnapped, `y` above the anchor; snapped, `y`
        // above the ground under it. Where it is drawn now: snapped, above
        // the ground under it; unsnapped, above the anchor, which is now on
        // the ground under the anchor.
        let new_y = if snapping_now {
            anchor.translation.y + y - under
        } else {
            under + y - anchor_ground
        };
        copy.y_cm = centimetres(new_y);
    }
    transform.translation.0[1] = anchor_ground;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #700: a set that stops snapping keeps every copy where it stood, and
    /// starting again puts each back on the ground with its own lift.
    #[test]
    fn a_set_that_stops_or_starts_snapping_keeps_its_copies_standing() {
        let ground = |x: f32, _z: f32| x * 0.1;
        let mut transform = TransformData::default();
        let mut copies = vec![
            InstanceCopy::at(10.0, 0.0, 0.0),
            InstanceCopy {
                y_cm: 50,
                ..InstanceCopy::at(-20.0, 5.0, 90.0)
            },
        ];
        let snapped = copies.clone();
        // Snapped: drawn at ground + y, so at 1.0 and -2.0 + 0.5.
        keep_copies_standing(&mut transform, &mut copies, false, &ground);
        assert_eq!(transform.translation.0[1], 0.0);
        assert_eq!(copies[0].y_cm, 100);
        assert_eq!(copies[1].y_cm, -150);
        keep_copies_standing(&mut transform, &mut copies, true, &ground);
        assert_eq!(copies, snapped);
    }

    #[test]
    fn a_scaled_field_shows_and_reads_the_number_it_stands_for() {
        // What the formatter and parser do, without a frame to draw in.
        let unit = 100.0;
        let shown = |n: f64| format!("{}", n / unit);
        let read = |text: &str| crate::ui::num::locale_number(text).map(|n| (n * unit).round());
        assert_eq!(shown(1234.0), "12.34");
        assert_eq!(read("12,34"), Some(1234.0));
        assert_eq!(read("-0.5"), Some(-50.0));
    }
}
