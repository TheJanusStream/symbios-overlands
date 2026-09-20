//! The longship's one discrete pick: which machine she is (#1369).
//!
//! She is at home on four themes, and three of them build the same boat. A
//! Norse, a medieval or a fae seed draws a LONGSHIP; a classical one draws a
//! GALLEY - the same hull with a bronze beak at her forefoot and a bank of
//! oars shipped along her gunwale. That is what the classical world put on
//! the water, and at 12 m the BANK is what says so: a ram and a beak are
//! nearly the same silhouette at that range, but eighteen oars out over the
//! water are not.
//!
//! So the variant is keyed to the THEME, as the tug's work, the scow's load,
//! the buggy's kit, the armoured car's up-armour and the rover's mass are
//! (#1370, #1373, #1374, #1375, #1378), and not to the stance or the tiers:
//! the tiers dress her, the theme decides what she is.
//!
//! No salted draw: the pick is a pure function of the theme, so there is no
//! stream to keep apart from the others. One machine drawn on all four
//! themes was rendered beside it and rejected, and so was a third Fantasy
//! variant.

use super::character::AvatarCharacter;
use crate::seeded_defaults::scene::ThemeArchetype;

/// Which longship a seed draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LongshipVariant {
    /// The Norse boat: shields along the gunwale, no oars drawn, and on a
    /// NORSE_FEY theme a serpent at her stem. Nordic, Medieval and Fantasy's
    /// - 59 of the 75 seeds under 3000.
    Longship,
    /// The classical one: a bronze beak off her forefoot and a bank of oars
    /// shipped along her gunwale. AncientClassical's - 16 of the 75.
    Galley,
}

impl LongshipVariant {
    pub const ALL: [Self; 2] = [Self::Longship, Self::Galley];

    /// Human-readable display name. Public, like
    /// [`RoverVariant::label`](super::RoverVariant::label), because the
    /// readouts that print it are native-only.
    pub fn label(self) -> &'static str {
        match self {
            Self::Longship => "Longship",
            Self::Galley => "Galley",
        }
    }

    /// The variant a longship of `style` is drawn as.
    ///
    /// The classical theme names the galley and her other three name the
    /// longship, so the wildcard is reached from a longship seed only
    /// through those three; it is there because the function is total over
    /// every theme, and the longship is the machine the galley is a
    /// variation of.
    pub fn for_style(style: ThemeArchetype) -> Self {
        match style {
            ThemeArchetype::AncientClassical => Self::Galley,
            _ => Self::Longship,
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

    /// Every variant is reachable from a theme the longship is actually
    /// picked on - a variant no longship seed can draw is one nobody agreed.
    #[test]
    fn every_longship_variant_is_drawn_by_some_longship_theme() {
        for v in LongshipVariant::ALL {
            assert!(
                // The longship is not the family floor, so her weight is
                // non-zero exactly on the themes she is at home on.
                ThemeArchetype::ALL.iter().any(|&s| {
                    BoatType::Longship.weight(s) > 0 && LongshipVariant::for_style(s) == v
                }),
                "{v:?} is not the variant of any theme the longship is at home on"
            );
        }
    }

    /// The agreed table, pinned: AncientClassical the galley, and the other
    /// three of her themes the longship - so a theme added to her affinity
    /// has to be given a variant here on purpose.
    #[test]
    fn the_agreed_themes_draw_the_agreed_variants() {
        use ThemeArchetype::*;
        for (s, v) in [
            (Nordic, LongshipVariant::Longship),
            (Medieval, LongshipVariant::Longship),
            (Fantasy, LongshipVariant::Longship),
            (AncientClassical, LongshipVariant::Galley),
        ] {
            assert_eq!(LongshipVariant::for_style(s), v, "{s:?}");
        }
        let homes = ThemeArchetype::ALL
            .iter()
            .filter(|&&s| BoatType::Longship.weight(s) > 0)
            .count();
        assert_eq!(
            homes, 4,
            "the longship's themes changed under the variant table"
        );
    }
}
