//! Small text helpers that belong to neither layer.
//!
//! One function so far, and it earns a module because the alternative was
//! worse: [`plural`] is a pure English-noun rule with six callers in `ui`
//! and one in [`crate::diagnostics::anomaly`], and it sat in
//! `ui::toolbar` — so a rule about anomaly counts made the anomaly rules
//! import the toolbar (#1297).
//!
//! English-only, and that is fine: there is no i18n framework in the tree
//! (no fluent, gettext or rust-i18n dependency), so this is not a
//! translation layer — it is the seam to route through if one is ever
//! added.

/// Pick the singular or plural noun for a count (#1264 f374).
///
/// The app already branches on the singular nearly everywhere — the
/// People window's pending offers, the toolbar's anomaly badge, the
/// Inventory header, the audio panel's per-noun suffixes — which is
/// exactly what made "1 entries" in the catalogue, "Downloaded 1 events"
/// and "· 1 props" read as unfinished rather than as a house style.
///
/// Takes both words rather than appending an "s", because the counts this
/// app prints are of anomalies, entries and people, none of which
/// pluralise that way.
pub fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule, and the reason it takes two words: the three counts that
    /// made #1264 f374 worth filing all pluralise irregularly.
    #[test]
    fn one_is_singular_and_everything_else_is_not() {
        assert_eq!(plural(1, "entry", "entries"), "entry");
        assert_eq!(plural(0, "entry", "entries"), "entries");
        assert_eq!(plural(2, "anomaly", "anomalies"), "anomalies");
        assert_eq!(plural(1, "person", "people"), "person");
        assert_eq!(plural(3, "person", "people"), "people");
    }
}
