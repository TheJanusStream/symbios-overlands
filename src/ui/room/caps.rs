//! The room record's structural caps, each with its number and its ONE
//! user-facing sentence (#1210).
//!
//! Every cap here is enforced by [`crate::pds::sanitize`] — on fetch, and
//! on the editor's own quarter-second debounce flush over the LIVE record.
//! Until this module the editor never read a cap: the 257th generator was
//! inserted and, a quarter second later, whichever generator sorted LAST
//! alphabetically was deleted; the 1025th placement was pushed, selected,
//! and truncated; a 65th recipe re-sorted the whole list and dropped one;
//! nesting past sixteen levels was amputated — all silently, with no count
//! anywhere on screen. #841 established the fix for the inventory cap:
//! refuse at every insert, disable the control with the reason, show
//! `N/cap`. This is that pattern for the room, with the number and the
//! sentence in one place so the tree, the Placements tab, the scene menu,
//! the drop handler and the footer readout cannot disagree.

use bevy_egui::egui;

use crate::pds::sanitize::limits;

/// One bounded thing in the room record.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Cap {
    /// Top-level generators (`RoomRecord::generators`).
    Generators,
    /// Placements (`RoomRecord::placements`).
    Placements,
    /// Contact-effect recipes.
    Recipes,
    /// Nodes in one generator tree, root included.
    NodesPerGenerator,
    /// Nesting depth of one generator tree — the root is depth 0, and a
    /// node at the cap keeps no children.
    Depth,
    /// Material slots on a Shape generator.
    MaterialSlots,
    /// Points on a Spine / stations on a Lathe.
    SweepPoints,
    /// Elements in a BlobGroup.
    BlobElements,
}

/// How close a count sits to its cap, for the readout's colour.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum CapTone {
    Quiet,
    /// At or past 80 % of the cap.
    Warn,
    /// At the cap: the next add is refused.
    Full,
}

impl Cap {
    /// The sanitiser's number.
    pub(crate) fn max(self) -> usize {
        match self {
            Self::Generators => limits::MAX_GENERATORS,
            Self::Placements => limits::MAX_PLACEMENTS,
            Self::Recipes => limits::MAX_CONTACT_RECIPES,
            Self::NodesPerGenerator => limits::MAX_GENERATOR_NODES as usize,
            Self::Depth => limits::MAX_GENERATOR_DEPTH as usize,
            Self::MaterialSlots => limits::MAX_SHAPE_MATERIAL_SLOTS,
            Self::SweepPoints => limits::MAX_SWEEP_POINTS,
            Self::BlobElements => limits::MAX_BLOB_ELEMENTS,
        }
    }

    /// Whether `used` leaves no room for one more.
    pub(crate) fn is_full(self, used: usize) -> bool {
        used >= self.max()
    }

    /// Why an add is refused — the disabled control's hover, and the toast
    /// for the doors that cannot be disabled (a drop, a drag, a menu that
    /// buffered its action).
    pub(crate) fn full_reason(self) -> String {
        let max = self.max();
        match self {
            Self::Generators => format!("World full ({max}/{max} generators) — delete one first"),
            Self::Placements => format!("World full ({max}/{max} placements) — delete one first"),
            Self::Recipes => format!("Recipe limit reached ({max}/{max}) — delete one first"),
            Self::NodesPerGenerator => {
                format!("This generator is full ({max} nodes) — delete a node first")
            }
            Self::Depth => format!("Nesting limit reached ({max} levels)"),
            Self::MaterialSlots => format!("All {max} material slots are used"),
            Self::SweepPoints => format!("Maximum {max} points"),
            Self::BlobElements => format!("Maximum {max} elements"),
        }
    }

    /// The `Label N/cap` readout and its tone.
    pub(crate) fn readout(self, used: usize) -> (String, CapTone) {
        let label = match self {
            Self::Generators => "Generators",
            Self::Placements => "Placements",
            Self::Recipes => "Recipes",
            Self::NodesPerGenerator => "Nodes",
            Self::Depth => "Depth",
            Self::MaterialSlots => "Material slots",
            Self::SweepPoints => "Points",
            Self::BlobElements => "Elements",
        };
        (format!("{label} {used}/{}", self.max()), self.tone(used))
    }

    /// Colour tier for `used`.
    pub(crate) fn tone(self, used: usize) -> CapTone {
        let max = self.max();
        if used >= max {
            CapTone::Full
        } else if used * 5 >= max * 4 {
            CapTone::Warn
        } else {
            CapTone::Quiet
        }
    }
}

/// Why a remove control at a list's minimum is disabled — a control that
/// is present and does nothing reads as a bug.
pub(crate) const SPINE_MIN_REASON: &str = "A spine needs at least 2 points";
pub(crate) const LATHE_MIN_REASON: &str = "A lathe needs at least 2 stations";
pub(crate) const BLOB_MIN_REASON: &str = "A blob group needs at least one element";

/// Number of nodes in `generator`'s tree, root included — what
/// [`Cap::NodesPerGenerator`] counts.
pub(crate) fn node_count(generator: &crate::pds::Generator) -> usize {
    1 + generator.children.iter().map(node_count).sum::<usize>()
}

/// Depth of the deepest node in `generator`'s tree relative to the
/// generator itself (a leaf is 0). A subtree landing at depth `d` puts its
/// deepest node at `d + subtree_depth`, which [`Cap::Depth`] bounds.
pub(crate) fn subtree_depth(generator: &crate::pds::Generator) -> usize {
    generator
        .children
        .iter()
        .map(|child| 1 + subtree_depth(child))
        .max()
        .unwrap_or(0)
}

/// Whether a subtree of `depth` (see [`subtree_depth`]) may hang under a
/// parent at `parent_depth` — the root of a generator is depth 0.
pub(crate) fn fits_under(parent_depth: usize, depth: usize) -> bool {
    parent_depth + 1 + depth <= Cap::Depth.max()
}

/// The room-wide colour for a [`CapTone`].
pub(crate) fn tone_color(ui: &egui::Ui, tone: CapTone) -> egui::Color32 {
    let theme = crate::ui::theme::current(ui.ctx());
    match tone {
        CapTone::Quiet => theme.text_weak,
        CapTone::Warn => theme.status.warn,
        CapTone::Full => theme.status.error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::Generator;

    /// #1210, finding 410 / 412 / 307. Every cap answers "is one more
    /// allowed?" from the sanitiser's own number, so a control disabled
    /// here can never disagree with the flush that would have deleted.
    #[test]
    fn a_cap_is_full_exactly_at_the_sanitisers_number() {
        for cap in [
            Cap::Generators,
            Cap::Placements,
            Cap::Recipes,
            Cap::NodesPerGenerator,
            Cap::MaterialSlots,
            Cap::SweepPoints,
            Cap::BlobElements,
        ] {
            assert!(!cap.is_full(cap.max() - 1), "{cap:?}");
            assert!(cap.is_full(cap.max()), "{cap:?}");
            assert!(
                cap.full_reason().contains(&cap.max().to_string()),
                "{cap:?}"
            );
        }
        assert_eq!(Cap::Generators.max(), limits::MAX_GENERATORS);
        assert_eq!(Cap::Placements.max(), limits::MAX_PLACEMENTS);
    }

    /// #1210, finding 413. The readout turns warn at 80 % and error at
    /// the cap, the inventory's treatment.
    #[test]
    fn the_readout_warns_before_the_cap_and_names_the_count() {
        let (text, tone) = Cap::Placements.readout(10);
        assert_eq!(text, format!("Placements 10/{}", limits::MAX_PLACEMENTS));
        assert_eq!(tone, CapTone::Quiet);
        assert_eq!(
            Cap::Placements.tone(limits::MAX_PLACEMENTS / 2),
            CapTone::Quiet
        );
        assert_eq!(
            Cap::Placements.tone(limits::MAX_PLACEMENTS.div_ceil(5) * 4),
            CapTone::Warn
        );
        assert_eq!(
            Cap::Placements.tone(limits::MAX_PLACEMENTS - 1),
            CapTone::Warn
        );
        assert_eq!(Cap::Placements.tone(limits::MAX_PLACEMENTS), CapTone::Full);
    }

    /// #1210, finding 83 / #411. Depth is measured the way the sanitiser
    /// measures it: a root is depth 0, a node at depth 16 keeps no
    /// children, so a leaf may hang under a parent at depth 15 and not 16,
    /// and a two-level subtree may hang under depth 13 and not 14.
    #[test]
    fn depth_fits_are_the_sanitisers_arithmetic() {
        let max = limits::MAX_GENERATOR_DEPTH as usize;
        assert!(fits_under(max - 1, 0));
        assert!(!fits_under(max, 0));
        let mut two_deep = Generator::default_cuboid();
        two_deep.children = vec![Generator::default_cuboid()];
        two_deep.children[0].children = vec![Generator::default_cuboid()];
        assert_eq!(subtree_depth(&two_deep), 2);
        assert_eq!(node_count(&two_deep), 3);
        assert!(fits_under(max - 3, subtree_depth(&two_deep)));
        assert!(!fits_under(max - 2, subtree_depth(&two_deep)));

        // The control: the sanitiser really does cut exactly there.
        let mut chain = Generator::default_cuboid();
        let mut cursor = &mut chain;
        for _ in 0..(max + 2) {
            cursor.children = vec![Generator::default_cuboid()];
            cursor = &mut cursor.children[0];
        }
        assert_eq!(subtree_depth(&chain), max + 2);
        crate::pds::sanitize::sanitize_generator(&mut chain);
        assert_eq!(
            subtree_depth(&chain),
            max,
            "depth {max} survives, {} does not",
            max + 1
        );
    }
}
