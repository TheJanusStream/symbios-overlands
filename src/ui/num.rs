//! Numeric entry: the one place a `DragValue` or a `Slider` is built
//! (#1264 f364).
//!
//! **A number field used to reject a decimal comma without saying so.**
//! egui's default parser strips whitespace, folds U+2212 to a hyphen and
//! then calls `f64::from_str`, which fails on `1,5`; on failure the set
//! branch simply does not run, so the field snaps back to its old value
//! with no error, no tooltip and no toast. Comma-decimal locales cover
//! most of Europe and Latin America, and typing a number is the single
//! most repeated action in the World Editor - a field that discards your
//! input without saying so reads as an app that has stopped responding,
//! and the natural conclusion (the value is locked) is wrong.
//!
//! **The fix has to be per-widget, so it has to be per-construction
//! site.** egui offers `custom_parser` on the builder and nothing at the
//! `Style` level - `Style::number_formatter` is output only
//! (`drag_value.rs:534`, `:727`). The review's own proposal was a helper
//! inside `room::widgets`, and its refuter caught why that is not enough:
//! 16 files build these widgets directly, so the avatar, settings, gizmo
//! and most generator panels would still have rejected `1,5`. So the
//! constructors live here instead of at the call sites, all 78 of them
//! go through [`drag`] and [`slider`], and
//! `the_only_numeric_widgets_are_the_locale_aware_ones` is a source scan
//! that fails if a 79th is ever built the old way. The scan is the point:
//! this is a defect that reappears one call site at a time.
//!
//! # A panel must not edit what it shows (#1390)
//!
//! The second default this module owns, and the same shape as the first:
//! something the widget does to your number without being asked, and
//! without saying so.
//!
//! egui 0.35's [`egui::SliderClamping`] default is `Always` - "always
//! clamp values, even existing ones" - and `Slider::add_contents` acts on
//! it before any input is read: `let old_value = self.get_value(); if
//! self.clamping == SliderClamping::Always { self.set_value(old_value); }`.
//! `get_value` applies the range clamp and `set_value` applies BOTH the
//! range clamp and `step_by`'s rounding, so merely drawing a slider
//! rewrites an out-of-range value to its bound and every other value to a
//! multiple of its step. `DragValue` has the same default under the name
//! `clamp_existing_to_range`, for the range half.
//!
//! **That lands in the live record.** The avatar editor's `TabCtx` is
//! "the live record every tab edits in place", so the Locomotion tab
//! opened on a seeded steam tug took her mass from 411.6 kg to the
//! slider's 200.0 ceiling, and her lateral grip from 48 000 to 15 000 -
//! while `changed()` stayed FALSE for the clamp, so the change tick never
//! moved and no peer was ever told. The step snap DOES report changed, so
//! the other half marks an avatar edited and queues a broadcast for being
//! looked at. Measured on fourteen seeded presets - every boat, every
//! skiff, the airship and the humanoid - in
//! `ui::avatar::locomotion::inert_panel_tests`, which is the guard.
//!
//! The fix is here rather than at the 80 call sites for the same reason
//! the parser is: the defect is the DEFAULT, and a per-call-site fix
//! re-appears one panel at a time. Both constructors keep the clamp for
//! EDITS, which is the half anybody wanted: a drag or a typed number
//! still cannot leave the range.

use bevy_egui::egui;

/// A locale-aware [`egui::DragValue`] that does not edit what it shows.
/// Use this, never `egui::DragValue::new`.
///
/// `clamp_existing_to_range(false)` for the reason in the module doc:
/// egui's default is `true`, and a `DragValue` given a `.range(..)` then
/// rewrites an existing out-of-range value the first time it is drawn. An
/// edit - a drag or a typed number - is still clamped, which is the half
/// of the behaviour that was ever wanted.
pub(crate) fn drag<Num: egui::emath::Numeric>(value: &mut Num) -> egui::DragValue<'_> {
    egui::DragValue::new(value)
        .custom_parser(locale_number)
        .clamp_existing_to_range(false)
}

/// A locale-aware [`egui::Slider`] that does not edit what it shows. Use
/// this, never `egui::Slider::new`.
///
/// [`egui::SliderClamping::Edits`] for the reason in the module doc: the
/// default is `Always`, which rewrites the value with no input at all.
/// Dragging and typing still clamp, and `step_by` still snaps what a drag
/// lands on.
pub(crate) fn slider<Num: egui::emath::Numeric>(
    value: &mut Num,
    range: std::ops::RangeInclusive<Num>,
) -> egui::Slider<'_> {
    egui::Slider::new(value, range)
        .custom_parser(locale_number)
        .clamping(egui::SliderClamping::Edits)
}

/// Parse a number written the way the typist's locale writes it.
///
/// egui's leniency first - whitespace anywhere is ignored, so a thousands
/// space works, and U+2212 MINUS SIGN folds to a hyphen - then the
/// separators:
///
/// **Both separators present: the LAST one is the decimal point.** That
/// is true in every locale that uses two, so `1.234,5` and `1,234.5` both
/// come out as the same number without the parser needing to know where
/// the user lives.
///
/// **Only commas, exactly one of them, not followed by exactly three
/// digits: it is a decimal comma.** This is the case the finding is
/// about - `1,5` - and it is unambiguous.
///
/// **Only commas, any other shape: they group digits and are dropped.**
/// `1,234,567` can only be grouping. `1,234` genuinely cannot be
/// resolved - 1234 to one reader and 1.234 to another - and this is the
/// one place the answer is a convention rather than a deduction: the
/// three-digit group wins, so it reads as 1234. Chosen because it is the
/// same answer the unambiguous multi-comma case gives, which keeps one
/// rule rather than two; a comma is a decimal separator unless it is
/// shaped like a group separator. Pinned by a test so it stays a decision.
pub(crate) fn locale_number(text: &str) -> Option<f64> {
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| if c == '\u{2212}' { '-' } else { c })
        .collect();

    let last_comma = cleaned.rfind(',');
    let last_dot = cleaned.rfind('.');
    let normalised = match (last_comma, last_dot) {
        (Some(comma), Some(dot)) => {
            let (decimal, group) = if comma > dot { (',', '.') } else { ('.', ',') };
            cleaned
                .chars()
                .filter(|c| *c != group)
                .map(|c| if c == decimal { '.' } else { c })
                .collect()
        }
        (Some(comma), None) => {
            let tail = &cleaned[comma + 1..];
            let grouped = cleaned.matches(',').count() > 1 || (tail.len() == 3 && all_digits(tail));
            if grouped {
                cleaned.replace(',', "")
            } else {
                cleaned.replace(',', ".")
            }
        }
        _ => cleaned,
    };
    normalised.parse().ok()
}

/// Whether every character is an ASCII digit, and there is at least one.
fn all_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive one widget for three frames: draw it, press the pointer on
    /// it, then drag to `to` while held. Returns the value afterwards.
    ///
    /// Three frames because egui needs a press and a move it can read as a
    /// drag, and the widget's rect is only known once it has been laid
    /// out. The starting value is deliberately OUTSIDE the range in the
    /// tests below: the whole point of #1390's fix is that an untouched
    /// widget leaves it there, so the drag has to be what brings it back.
    fn drag_widget(
        start: f32,
        to: egui::Pos2,
        mut add: impl FnMut(&mut egui::Ui, &mut f32) -> egui::Response,
    ) -> f32 {
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let mut value = start;
        let mut rect = egui::Rect::NOTHING;
        for frame in 0..3 {
            let events = match frame {
                1 => vec![
                    egui::Event::PointerMoved(rect.center()),
                    egui::Event::PointerButton {
                        pos: rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                2 => vec![egui::Event::PointerMoved(to)],
                _ => Vec::new(),
            };
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            let _ = ctx.run_ui(input, |ui| {
                rect = add(ui, &mut value).rect;
            });
        }
        value
    }

    /// #1390 PROOF 3: `Edits` is not `Never` - an interaction still cannot
    /// leave the range.
    ///
    /// The fix stops the widget writing with NO input; it must not also
    /// stop it clamping WITH input, or an owner could drag a mass past the
    /// sanitiser's cap and have the record rewritten under them on the
    /// next round trip. Each case starts far outside the range - which
    /// `an_untouched_locomotion_tab_writes_nothing` proves is left alone -
    /// and drags to a point far past the end of the track.
    #[test]
    fn a_drag_still_cannot_leave_the_range() {
        // Far to the right of any track this screen can hold.
        let far_right = egui::pos2(5_000.0, 40.0);
        let far_left = egui::pos2(-5_000.0, 40.0);

        let high = drag_widget(9_999.0, far_right, |ui, v| {
            ui.add(slider(v, 0.0..=10.0).step_by(1.0))
        });
        assert_eq!(high, 10.0, "a drag to the right pins at the range's top");

        let low = drag_widget(-9_999.0, far_left, |ui, v| {
            ui.add(slider(v, 0.0..=10.0).step_by(1.0))
        });
        assert_eq!(low, 0.0, "a drag to the left pins at the range's bottom");

        // And the `DragValue` half, whose flag has the other name.
        let dragged = drag_widget(9_999.0, far_right, |ui, v| {
            ui.add(drag(v).speed(1.0).range(0.0..=10.0))
        });
        assert_eq!(
            dragged, 10.0,
            "a dragged DragValue is clamped into its range too"
        );
    }

    /// A decimal comma is accepted, and a decimal point still is (#1264
    /// f364).
    ///
    /// The second half is the control: this parser REPLACES egui's, so a
    /// regression here breaks number entry for everyone rather than for
    /// the users the change is for.
    #[test]
    fn both_decimal_separators_parse() {
        assert_eq!(locale_number("1,5"), Some(1.5));
        assert_eq!(locale_number("1.5"), Some(1.5));
        assert_eq!(locale_number("-1,25"), Some(-1.25));
        assert_eq!(locale_number("0,0001"), Some(0.0001));
        assert_eq!(locale_number("42"), Some(42.0));
        assert_eq!(locale_number("-42"), Some(-42.0));

        // egui's own leniency, which this must not lose.
        assert_eq!(locale_number(" 1,5 "), Some(1.5));
        assert_eq!(locale_number("1 000,5"), Some(1000.5));
        assert_eq!(locale_number("\u{2212}3,5"), Some(-3.5));
    }

    /// With two separators the last one is the decimal point, in either
    /// order (#1264 f364).
    #[test]
    fn the_last_separator_is_the_decimal_point() {
        assert_eq!(locale_number("1,234.5"), Some(1234.5));
        assert_eq!(locale_number("1.234,5"), Some(1234.5));
        assert_eq!(locale_number("1.234.567,89"), Some(1234567.89));
        assert_eq!(locale_number("1,234,567.89"), Some(1234567.89));
    }

    /// The one genuinely ambiguous shape resolves the same way the
    /// unambiguous grouped ones do (#1264 f364).
    ///
    /// Not an accident and not a deduction - a convention, pinned here so
    /// that changing it is a decision somebody makes on purpose.
    #[test]
    fn a_lone_three_digit_group_reads_as_grouping() {
        assert_eq!(locale_number("1,234"), Some(1234.0));
        assert_eq!(locale_number("1,234,567"), Some(1234567.0));
        // Two digits or four are not a group in any locale, so they are
        // the decimal case and stay unambiguous.
        assert_eq!(locale_number("1,23"), Some(1.23));
        assert_eq!(locale_number("1,2345"), Some(1.2345));
    }

    /// Nonsense is still rejected, so the field still refuses rather than
    /// inventing a number.
    #[test]
    fn unparseable_input_is_still_refused() {
        assert_eq!(locale_number(""), None);
        assert_eq!(locale_number("abc"), None);
        assert_eq!(locale_number("1,2,3.4.5"), None);
        assert_eq!(locale_number("--1"), None);
    }
}
