//! The steam tug's one discrete pick: what she works at (#1370).
//!
//! A harbour tug is read by her after deck: the towing arches and the hook
//! of a tug that tows, or a cargo derrick over the same deck on the harbour
//! tender an industrial port runs. So the variant is keyed to the THEME, as
//! the scow's load and the runabout's variant are (#1373, #1372), and not to
//! the stance or the tiers: the tiers dress her, the theme decides her work.
//!
//! No salted draw: the pick is a pure function of the theme, so there is no
//! stream to keep apart from the others. The owner agreed this table on the
//! phase-1 renders of #1370 (session 809).

use super::character::AvatarCharacter;
use crate::seeded_defaults::scene::ThemeArchetype;

/// What a steam tug works at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TugVariant {
    /// Towing arches over the after deck and the hook on the casing: every
    /// tug theme's but one.
    Towing,
    /// A harbour tender: a cargo derrick over the after deck where the
    /// towing gear stands - the industrial port's.
    Derrick,
}

impl TugVariant {
    pub const ALL: [Self; 2] = [Self::Towing, Self::Derrick];

    /// Human-readable display name. Public, like
    /// [`ScowLoad::label`](super::ScowLoad::label), because the readouts that
    /// print it are native-only.
    pub fn label(self) -> &'static str {
        match self {
            Self::Towing => "Steam tug",
            Self::Derrick => "Harbour tender",
        }
    }

    /// The variant a tug of `style` is drawn as.
    pub fn for_style(style: ThemeArchetype) -> Self {
        match style {
            ThemeArchetype::IndustrialPark => Self::Derrick,
            _ => Self::Towing,
        }
    }

    /// The variant for a seed.
    pub fn for_seed(seed: u64) -> Self {
        Self::for_style(AvatarCharacter::for_seed(seed).style)
    }

    /// Whether she tows - the towing hook and arches, and on an Ornate tug
    /// the hawser coiled between them.
    pub fn tows(self) -> bool {
        self == Self::Towing
    }
}

#[cfg(test)]
mod tests {
    use super::super::craft::BoatType;
    use super::*;

    /// Every variant is reachable from a theme the steam tug is actually
    /// picked on - a variant no tug seed can draw is one nobody agreed.
    #[test]
    fn every_tug_variant_is_drawn_by_some_tug_theme() {
        for v in TugVariant::ALL {
            assert!(
                // The tug is not the family floor, so its weight is non-zero
                // exactly on the themes it is at home on.
                ThemeArchetype::ALL
                    .iter()
                    .any(|&s| BoatType::SteamTug.weight(s) > 0 && TugVariant::for_style(s) == v),
                "{v:?} is not the variant of any theme the steam tug is at home on"
            );
        }
    }

    /// The agreed table, pinned: IndustrialPark the derrick tender, and the
    /// other four tug themes towing.
    #[test]
    fn the_agreed_themes_draw_the_agreed_variants() {
        use ThemeArchetype::*;
        assert_eq!(TugVariant::for_style(IndustrialPark), TugVariant::Derrick);
        for s in [Steampunk, ModernCity, WildWest, PostApoc] {
            assert_eq!(TugVariant::for_style(s), TugVariant::Towing, "{s:?}");
        }
        // And those five are exactly the tug's themes: a theme added to her
        // affinity has to be given a variant here on purpose.
        let homes = ThemeArchetype::ALL
            .iter()
            .filter(|&&s| BoatType::SteamTug.weight(s) > 0)
            .count();
        assert_eq!(homes, 5, "the tug's themes changed under the variant table");
    }
}
