//! The dune buggy's one discrete pick: which buggy she is (#1374).
//!
//! A buggy is read by what she carries over her open frame: nothing but the
//! frame itself on a sand rail, a striped canopy on the beach buggy, and on
//! a desert raider a spare wheel hung on her roll bar and jerrycans on her
//! nerf bar. So the variant is keyed to the THEME, as the tug's work and the
//! scow's load are (#1370, #1373), and not to the stance or the tiers: the
//! tiers dress her, the theme decides what she is for.
//!
//! No salted draw: the pick is a pure function of the theme, so there is no
//! stream to keep apart from the others. The owner agreed this table on the
//! phase-1 renders of #1374 (session 813), Suburban on the beach buggy - the
//! weekend toy on the driveway - which is what makes the canopy a variant
//! twenty-seven seeds under 3000 wear rather than six.

use super::character::AvatarCharacter;
use crate::seeded_defaults::scene::ThemeArchetype;

/// Which dune buggy a seed draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuggyVariant {
    /// The bare tube frame: the sporting and roadside themes'.
    Rail,
    /// The rail under a striped canopy on every tier: the two leisure
    /// themes'.
    Beach,
    /// The rail with a spare on the roll bar, two jerrycans on the near
    /// nerf bar and a diagonal across the main hoop on every tier: the two
    /// frontier themes', whose buggy crosses a desert.
    Raider,
}

impl BuggyVariant {
    pub const ALL: [Self; 3] = [Self::Rail, Self::Beach, Self::Raider];

    /// Human-readable display name. Public, like
    /// [`TugVariant::label`](super::TugVariant::label), because the readouts
    /// that print it are native-only.
    pub fn label(self) -> &'static str {
        match self {
            Self::Rail => "Sand rail",
            Self::Beach => "Beach buggy",
            Self::Raider => "Desert raider",
        }
    }

    /// The variant a buggy of `style` is drawn as.
    pub fn for_style(style: ThemeArchetype) -> Self {
        match style {
            ThemeArchetype::CoastalResort | ThemeArchetype::Suburban => Self::Beach,
            ThemeArchetype::PostApoc | ThemeArchetype::WildWest => Self::Raider,
            _ => Self::Rail,
        }
    }

    /// The variant for a seed.
    pub fn for_seed(seed: u64) -> Self {
        Self::for_style(AvatarCharacter::for_seed(seed).style)
    }
}

#[cfg(test)]
mod tests {
    use super::super::craft::SkiffType;
    use super::*;

    /// Every variant is reachable from a theme the dune buggy is actually
    /// picked on - a variant no buggy seed can draw is one nobody agreed.
    #[test]
    fn every_buggy_variant_is_drawn_by_some_buggy_theme() {
        for v in BuggyVariant::ALL {
            assert!(
                // The buggy is not the family floor, so its weight is
                // non-zero exactly on the themes it is at home on.
                ThemeArchetype::ALL
                    .iter()
                    .any(|&s| SkiffType::DuneBuggy.weight(s) > 0 && BuggyVariant::for_style(s) == v),
                "{v:?} is not the variant of any theme the dune buggy is at home on"
            );
        }
    }

    /// The agreed table, pinned: CoastalResort and Suburban the beach buggy,
    /// PostApoc and WildWest the raider, Roadside and SportsRec the rail.
    #[test]
    fn the_agreed_themes_draw_the_agreed_variants() {
        use ThemeArchetype::*;
        for (s, v) in [
            (CoastalResort, BuggyVariant::Beach),
            (Suburban, BuggyVariant::Beach),
            (PostApoc, BuggyVariant::Raider),
            (WildWest, BuggyVariant::Raider),
            (Roadside, BuggyVariant::Rail),
            (SportsRec, BuggyVariant::Rail),
        ] {
            assert_eq!(BuggyVariant::for_style(s), v, "{s:?}");
        }
        // And those six are exactly the buggy's themes: a theme added to her
        // affinity has to be given a variant here on purpose.
        let homes = ThemeArchetype::ALL
            .iter()
            .filter(|&&s| SkiffType::DuneBuggy.weight(s) > 0)
            .count();
        assert_eq!(
            homes, 6,
            "the buggy's themes changed under the variant table"
        );
    }
}
