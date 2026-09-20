//! The rover's one discrete pick: which machine she is (#1378).
//!
//! A rover is read by the MASS she carries on her equipment deck, and the
//! three themes she is at home on want three different masses: an outpost
//! machine carries a tilted solar panel and a dish, an AlienOrganic one a
//! chitin carapace over a dark running deck, and an AlienMonolithic one an
//! upright lit slab. So the variant is keyed to the THEME, as the tug's
//! work, the scow's load, the buggy's kit and the armoured car's up-armour
//! are (#1370, #1373, #1374, #1375), and not to the stance or the tiers: the
//! tiers dress her, the theme decides what she is for.
//!
//! No salted draw: the pick is a pure function of the theme, so there is no
//! stream to keep apart from the others. The owner agreed the table on the
//! phase-1 renders of #1378 (session 826). One machine drawn on all three
//! themes was rendered beside it and rejected: it is the same rover in a
//! different paint, and an alien theme's scheme alone does not say alien.

use super::character::AvatarCharacter;
use crate::seeded_defaults::scene::ThemeArchetype;

/// Which rover a seed draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoverVariant {
    /// The exploration machine: a tilted solar panel aft on a pedestal and a
    /// dish on her far side. SpaceOutpost's - 25 of the 54 seeds under 3000.
    Surveyor,
    /// A chitin shell over a dark running deck, her identity trim along its
    /// crown as a lit dorsal ridge. AlienOrganic's - 15 of the 54.
    Carapace,
    /// An upright slab standing on the deck with three lit seams, and an
    /// obelisk in place of the instrument mast. AlienMonolithic's - 14 of
    /// the 54.
    Monolith,
}

impl RoverVariant {
    pub const ALL: [Self; 3] = [Self::Surveyor, Self::Carapace, Self::Monolith];

    /// Human-readable display name. Public, like
    /// [`ArmouredVariant::label`](super::ArmouredVariant::label), because the
    /// readouts that print it are native-only.
    pub fn label(self) -> &'static str {
        match self {
            Self::Surveyor => "Surveyor",
            Self::Carapace => "Carapace",
            Self::Monolith => "Monolith",
        }
    }

    /// The variant a rover of `style` is drawn as.
    ///
    /// Her three themes each name one, so the wildcard is unreachable from a
    /// rover seed; it is there because the function is total over every
    /// theme, and the surveyor is the machine the other two are variations
    /// of.
    pub fn for_style(style: ThemeArchetype) -> Self {
        match style {
            ThemeArchetype::AlienOrganic => Self::Carapace,
            ThemeArchetype::AlienMonolithic => Self::Monolith,
            _ => Self::Surveyor,
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

    /// Every variant is reachable from a theme the rover is actually picked
    /// on - a variant no rover seed can draw is one nobody agreed.
    #[test]
    fn every_rover_variant_is_drawn_by_some_rover_theme() {
        for v in RoverVariant::ALL {
            assert!(
                // The rover is not the family floor, so her weight is
                // non-zero exactly on the themes she is at home on.
                ThemeArchetype::ALL.iter().any(|&s| {
                    SkiffType::Rover.weight(s) > 0 && RoverVariant::for_style(s) == v
                }),
                "{v:?} is not the variant of any theme the rover is at home on"
            );
        }
    }

    /// The agreed table, pinned: SpaceOutpost the surveyor, AlienOrganic the
    /// carapace, AlienMonolithic the monolith - and those three are exactly
    /// her themes, so a theme added to her affinity has to be given a variant
    /// here on purpose.
    #[test]
    fn the_agreed_themes_draw_the_agreed_variants() {
        use ThemeArchetype::*;
        for (s, v) in [
            (SpaceOutpost, RoverVariant::Surveyor),
            (AlienOrganic, RoverVariant::Carapace),
            (AlienMonolithic, RoverVariant::Monolith),
        ] {
            assert_eq!(RoverVariant::for_style(s), v, "{s:?}");
        }
        let homes = ThemeArchetype::ALL
            .iter()
            .filter(|&&s| SkiffType::Rover.weight(s) > 0)
            .count();
        assert_eq!(
            homes, 3,
            "the rover's themes changed under the variant table"
        );
    }
}
