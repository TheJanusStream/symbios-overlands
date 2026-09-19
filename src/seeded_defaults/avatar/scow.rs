//! The scow's one discrete pick: what she carries, and so her stern gear
//! (#1373).
//!
//! A working scow is read by her load: bales on a farm barge, a heap of drums
//! and tyres on a scrapper, crates and casks everywhere else, and on the
//! frontier a small stern wheel where every other scow steers with a sweep.
//! So the load is keyed to the THEME, as the runabout's variant and the
//! wagon's body are (#1372, #1377), and not to the stance or the tiers: the
//! tiers decide how HIGH the load is piled, the theme decides what it is.
//!
//! No salted draw: the pick is a pure function of the theme, so there is no
//! stream to keep apart from the others. The owner agreed this table on the
//! phase-1 renders of #1373 (session 805).

use super::character::AvatarCharacter;
use crate::seeded_defaults::scene::ThemeArchetype;

/// What a scow carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScowLoad {
    /// Crates and casks, steered with a sweep: every grimy theme's load, and
    /// the floor every scow theme without a load of its own draws.
    Freight,
    /// Hay bales in a stepped stack, the farm theme's.
    Hay,
    /// Drums, tyres and sheet iron, the wasteland's - with a fire drum
    /// burning on the foredeck.
    Scrap,
    /// The freight load under a small stern wheel instead of the sweep, the
    /// frontier's - with a fire drum burning on the foredeck.
    Sternwheel,
}

impl ScowLoad {
    pub const ALL: [Self; 4] = [Self::Freight, Self::Hay, Self::Scrap, Self::Sternwheel];

    /// Human-readable display name. Public, like
    /// [`RunaboutVariant::label`](super::RunaboutVariant::label), because the
    /// readouts that print it are native-only.
    pub fn label(self) -> &'static str {
        match self {
            Self::Freight => "Freight scow",
            Self::Hay => "Hay barge",
            Self::Scrap => "Scrap barge",
            Self::Sternwheel => "Sternwheel scow",
        }
    }

    /// The load a scow of `style` carries.
    pub fn for_style(style: ThemeArchetype) -> Self {
        match style {
            ThemeArchetype::RuralFarmland => Self::Hay,
            ThemeArchetype::PostApoc => Self::Scrap,
            ThemeArchetype::WildWest => Self::Sternwheel,
            _ => Self::Freight,
        }
    }

    /// The load for a seed.
    pub fn for_seed(seed: u64) -> Self {
        Self::for_style(AvatarCharacter::for_seed(seed).style)
    }

    /// Whether she steers with a stern wheel rather than a sweep.
    pub fn stern_wheel(self) -> bool {
        self == Self::Sternwheel
    }

    /// Whether a fire drum burns on her foredeck - the scrap and frontier
    /// scows, which are exactly the themes whose aura is Embers.
    pub fn fire_drum(self) -> bool {
        matches!(self, Self::Scrap | Self::Sternwheel)
    }
}

#[cfg(test)]
mod tests {
    use super::super::craft::BoatType;
    use super::*;

    /// Every load is reachable from a theme the Scow is actually picked on -
    /// a load no scow seed can draw is one nobody agreed.
    #[test]
    fn every_scow_load_is_drawn_by_some_scow_theme() {
        for load in ScowLoad::ALL {
            assert!(
                // The scow is not the family floor, so its weight is non-zero
                // exactly on the themes it is at home on.
                ThemeArchetype::ALL
                    .iter()
                    .any(|&s| BoatType::Scow.weight(s) > 0 && ScowLoad::for_style(s) == load),
                "{load:?} is not the load of any theme the Scow is at home on"
            );
        }
    }

    /// The agreed table, pinned: RuralFarmland hay, PostApoc scrap, WildWest
    /// the stern wheel, and the five grimy themes freight.
    #[test]
    fn the_agreed_themes_carry_the_agreed_loads() {
        use ThemeArchetype::*;
        assert_eq!(ScowLoad::for_style(RuralFarmland), ScowLoad::Hay);
        assert_eq!(ScowLoad::for_style(PostApoc), ScowLoad::Scrap);
        assert_eq!(ScowLoad::for_style(WildWest), ScowLoad::Sternwheel);
        for s in [Steampunk, IndustrialPark, Roadside, Suburban, Cyberpunk] {
            assert_eq!(ScowLoad::for_style(s), ScowLoad::Freight, "{s:?}");
        }
        // And those eight are exactly the scow's themes: a theme added to her
        // affinity has to be given a load here on purpose.
        let homes = ThemeArchetype::ALL
            .iter()
            .filter(|&&s| BoatType::Scow.weight(s) > 0)
            .count();
        assert_eq!(homes, 8, "the scow's themes changed under the load table");
    }
}
