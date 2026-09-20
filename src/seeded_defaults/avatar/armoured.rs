//! The armoured car's one discrete pick: which machine she is (#1375).
//!
//! An armoured car is read by what is bolted ON her plate. A works or
//! municipal machine carries nothing but her own stowage; a raider carries
//! up-armour - two applique slabs on her driver's plate, one on her near
//! flank and a pile of scavenged kit lashed on her rear deck. So the variant
//! is keyed to the THEME, as the tug's work, the scow's load and the buggy's
//! kit are (#1370, #1373, #1374), and not to the stance or the tiers: the
//! tiers dress her, the theme decides what she is for.
//!
//! No salted draw: the pick is a pure function of the theme, so there is no
//! stream to keep apart from the others. The owner agreed the table on the
//! phase-1 renders of #1375 (session 822). Without it a PostApoc armoured car
//! is a municipal wagon in sand paint - and what a works machine and a city
//! machine really differ by is the scheme, which the livery already carries.

use super::character::AvatarCharacter;
use crate::seeded_defaults::scene::ThemeArchetype;

/// Which armoured car a seed draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmouredVariant {
    /// The clean machine: her own stowage and nothing bolted on. The two
    /// works themes', IndustrialPark and ModernCity - 58 of the 65 seeds
    /// under 3000.
    Works,
    /// The up-armoured machine: applique slabs on her driver's plate and her
    /// near flank and a pile of scavenged kit on her rear deck, on every
    /// tier. PostApoc's.
    Raider,
}

impl ArmouredVariant {
    pub const ALL: [Self; 2] = [Self::Works, Self::Raider];

    /// Human-readable display name. Public, like
    /// [`BuggyVariant::label`](super::BuggyVariant::label), because the
    /// readouts that print it are native-only.
    pub fn label(self) -> &'static str {
        match self {
            Self::Works => "Works security",
            Self::Raider => "Desert raider",
        }
    }

    /// The variant an armoured car of `style` is drawn as.
    pub fn for_style(style: ThemeArchetype) -> Self {
        match style {
            ThemeArchetype::PostApoc => Self::Raider,
            _ => Self::Works,
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

    /// Every variant is reachable from a theme the armoured car is actually
    /// picked on - a variant no armoured-car seed can draw is one nobody
    /// agreed.
    #[test]
    fn every_armoured_variant_is_drawn_by_some_armoured_theme() {
        for v in ArmouredVariant::ALL {
            assert!(
                // The armoured car is not the family floor, so her weight is
                // non-zero exactly on the themes she is at home on.
                ThemeArchetype::ALL.iter().any(|&s| {
                    SkiffType::ArmouredCar.weight(s) > 0 && ArmouredVariant::for_style(s) == v
                }),
                "{v:?} is not the variant of any theme the armoured car is at home on"
            );
        }
    }

    /// The agreed table, pinned: PostApoc the raider, IndustrialPark and
    /// ModernCity the works machine - and those three are exactly her themes,
    /// so a theme added to her affinity has to be given a variant here on
    /// purpose.
    #[test]
    fn the_agreed_themes_draw_the_agreed_variants() {
        use ThemeArchetype::*;
        for (s, v) in [
            (PostApoc, ArmouredVariant::Raider),
            (IndustrialPark, ArmouredVariant::Works),
            (ModernCity, ArmouredVariant::Works),
        ] {
            assert_eq!(ArmouredVariant::for_style(s), v, "{s:?}");
        }
        let homes = ThemeArchetype::ALL
            .iter()
            .filter(|&&s| SkiffType::ArmouredCar.weight(s) > 0)
            .count();
        assert_eq!(
            homes, 3,
            "the armoured car's themes changed under the variant table"
        );
    }
}
