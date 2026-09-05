//! Numeric entry: the one place a `DragValue` or a `Slider` is built
//! (#1264 f364).
//!
//! **A number field used to reject a decimal comma without saying so.**
//! egui's default parser strips whitespace, folds U+2212 to a hyphen and
//! then calls `f64::from_str`, which fails on `1,5`; on failure the set
//! branch simply does not run, so the field snaps back to its old value
//! with no error, no tooltip and no toast. Comma-decimal locales cover
//! most of Europe and Latin America, and typing a number is the single
//! most repeated action in the World Editor — a field that discards your
//! input without saying so reads as an app that has stopped responding,
//! and the natural conclusion (the value is locked) is wrong.
//!
//! **The fix has to be per-widget, so it has to be per-construction
//! site.** egui offers `custom_parser` on the builder and nothing at the
//! `Style` level — `Style::number_formatter` is output only
//! (`drag_value.rs:534`, `:727`). The review's own proposal was a helper
//! inside `room::widgets`, and its refuter caught why that is not enough:
//! 16 files build these widgets directly, so the avatar, settings, gizmo
//! and most generator panels would still have rejected `1,5`. So the
//! constructors live here instead of at the call sites, all 78 of them
//! go through [`drag`] and [`slider`], and
//! `the_only_numeric_widgets_are_the_locale_aware_ones` is a source scan
//! that fails if a 79th is ever built the old way. The scan is the point:
//! this is a defect that reappears one call site at a time.

use bevy_egui::egui;

/// A locale-aware [`egui::DragValue`]. Use this, never
/// `egui::DragValue::new`.
pub(crate) fn drag<Num: egui::emath::Numeric>(value: &mut Num) -> egui::DragValue<'_> {
    egui::DragValue::new(value).custom_parser(locale_number)
}

/// A locale-aware [`egui::Slider`]. Use this, never `egui::Slider::new`.
pub(crate) fn slider<Num: egui::emath::Numeric>(
    value: &mut Num,
    range: std::ops::RangeInclusive<Num>,
) -> egui::Slider<'_> {
    egui::Slider::new(value, range).custom_parser(locale_number)
}

/// Parse a number written the way the typist's locale writes it.
///
/// egui's leniency first — whitespace anywhere is ignored, so a thousands
/// space works, and U+2212 MINUS SIGN folds to a hyphen — then the
/// separators:
///
/// **Both separators present: the LAST one is the decimal point.** That
/// is true in every locale that uses two, so `1.234,5` and `1,234.5` both
/// come out as the same number without the parser needing to know where
/// the user lives.
///
/// **Only commas, exactly one of them, not followed by exactly three
/// digits: it is a decimal comma.** This is the case the finding is
/// about — `1,5` — and it is unambiguous.
///
/// **Only commas, any other shape: they group digits and are dropped.**
/// `1,234,567` can only be grouping. `1,234` genuinely cannot be
/// resolved — 1234 to one reader and 1.234 to another — and this is the
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
    /// Not an accident and not a deduction — a convention, pinned here so
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
