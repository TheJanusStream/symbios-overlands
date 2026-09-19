//! The runabout's one discrete pick: which launch it is built as (#1372).
//!
//! The runabout takes three audiences that want three different boats: the
//! seaside and campus themes a varnished gentleman's launch, the neon themes a
//! skimmer with glowing thruster pods, and the sporting theme a power
//! catamaran. So the variant is keyed to the THEME, as the wagon's body is
//! (#1377), and not to the stance: a stance moves a runabout's proportions,
//! and a skimmer is a skimmer whether it is long and fine or short and beamy.
//!
//! No salted draw: the pick is a pure function of the theme, so there is no
//! stream to keep apart from the others. The owner agreed this table on the
//! phase-1 renders of #1372 (session 801).

use super::character::AvatarCharacter;
use super::mood::{NEON, holds};
use crate::seeded_defaults::scene::ThemeArchetype;

/// The runabout's variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunaboutVariant {
    /// A varnished mahogany launch: planked decks, chrome windscreen frame,
    /// bench seats in the scheme's colour, an engine hatch aft. The floor
    /// every non-neon, non-sporting runabout theme draws.
    Coastal,
    /// The same planing hull on a finer entry, painted, with two turned
    /// thruster pods at the transom quarters and lit nozzles.
    Skimmer,
    /// A power catamaran: two demihulls under a bridge deck, a centre
    /// console, twin outboards.
    Catamaran,
}

impl RunaboutVariant {
    pub const ALL: [Self; 3] = [Self::Coastal, Self::Skimmer, Self::Catamaran];

    /// Human-readable display name. Public, like
    /// [`WagonBody::label`](super::WagonBody::label), because the readouts
    /// that print it are native-only.
    pub fn label(self) -> &'static str {
        match self {
            Self::Coastal => "Coastal launch",
            Self::Skimmer => "Neon skimmer",
            Self::Catamaran => "Power catamaran",
        }
    }

    /// The variant a runabout of `style` is built as.
    pub fn for_style(style: ThemeArchetype) -> Self {
        if style == ThemeArchetype::SportsRec {
            Self::Catamaran
        } else if holds(NEON, style) {
            Self::Skimmer
        } else {
            Self::Coastal
        }
    }

    /// The variant for a seed.
    pub fn for_seed(seed: u64) -> Self {
        Self::for_style(AvatarCharacter::for_seed(seed).style)
    }
}

#[cfg(test)]
mod tests {
    use super::super::craft::BoatType;
    use super::*;

    /// Every variant is reachable from a theme the Runabout is actually
    /// picked on - a variant no runabout seed can draw is one nobody agreed.
    #[test]
    fn every_runabout_variant_is_drawn_by_some_runabout_theme() {
        for v in RunaboutVariant::ALL {
            assert!(
                // The runabout is not the family floor, so its weight is
                // non-zero exactly on the themes it is at home on.
                ThemeArchetype::ALL.iter().any(
                    |&s| BoatType::Runabout.weight(s) > 0 && RunaboutVariant::for_style(s) == v
                ),
                "{v:?} is not the variant of any theme the Runabout is at home on"
            );
        }
    }

    /// The agreed table, pinned: SportsRec is the catamaran, the five neon
    /// themes the skimmer, and the seaside and campus themes the launch.
    #[test]
    fn the_agreed_themes_draw_the_agreed_variants() {
        use ThemeArchetype::*;
        assert_eq!(
            RunaboutVariant::for_style(SportsRec),
            RunaboutVariant::Catamaran
        );
        for s in [
            Cyberpunk,
            SpaceOutpost,
            AlienMonolithic,
            AlienOrganic,
            Solarpunk,
        ] {
            assert_eq!(
                RunaboutVariant::for_style(s),
                RunaboutVariant::Skimmer,
                "{s:?}"
            );
        }
        for s in [CoastalResort, CivicCampus] {
            assert_eq!(
                RunaboutVariant::for_style(s),
                RunaboutVariant::Coastal,
                "{s:?}"
            );
        }
    }
}
