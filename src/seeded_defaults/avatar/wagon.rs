//! The horseless wagon's one discrete pick: which body it is built as (#1377).
//!
//! The wagon is the land craft of a world without engines, and it takes every
//! historic theme - which is why it is a third of the skiff family. One body
//! would put a Japanese court and a Victorian undertaker in the same farm
//! cart, so the body is keyed to the THEME: each historic culture gets the
//! wheeled vehicle it is actually known by, and every other wagon theme gets
//! the cart. Stance does not pick a body here (unlike the roadster's, which
//! follow the stance): a stance moves a wagon's proportions, and a chariot is
//! a chariot whether it is long and low or short and tall.
//!
//! No salted draw: the pick is a pure function of the theme, so there is no
//! stream to keep apart from the others. The owner agreed this table on the
//! phase-1 renders of #1377 (session 799).

use super::character::AvatarCharacter;
use crate::seeded_defaults::scene::ThemeArchetype;

/// The wagon's body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WagonBody {
    /// A plank box on four wheels, the front pair smaller, with a sprung
    /// bench - and a canvas tilt on swept bows from Adorned up. The floor
    /// every other wagon theme draws.
    Cart,
    /// A low flat bed between high wheels and a sprung seat on a riser - the
    /// WildWest's light wagon.
    Buckboard,
    /// A tall glazed body on the cart's running gear, lanterns lit on every
    /// tier - GothicHorror's.
    Hearse,
    /// Two wheels, a breastwork open at the back and a pole -
    /// AncientClassical's.
    Chariot,
    /// Two big wheels under a lacquered cabin with a swept roof, and two
    /// shafts - FeudalJapan's gissha.
    OxCart,
}

impl WagonBody {
    pub const ALL: [Self; 5] = [
        Self::Cart,
        Self::Buckboard,
        Self::Hearse,
        Self::Chariot,
        Self::OxCart,
    ];

    /// Human-readable display name. Public, like
    /// [`RoadsterBody::label`](super::RoadsterBody::label), because the
    /// readouts that print it are native-only.
    pub fn label(self) -> &'static str {
        match self {
            Self::Cart => "Cart",
            Self::Buckboard => "Buckboard",
            Self::Hearse => "Hearse",
            Self::Chariot => "Chariot",
            Self::OxCart => "Ox-cart",
        }
    }

    /// The body a wagon of `style` is built as.
    pub fn for_style(style: ThemeArchetype) -> Self {
        match style {
            ThemeArchetype::GothicHorror => Self::Hearse,
            ThemeArchetype::AncientClassical => Self::Chariot,
            ThemeArchetype::FeudalJapan => Self::OxCart,
            ThemeArchetype::WildWest => Self::Buckboard,
            _ => Self::Cart,
        }
    }

    /// The body for a seed.
    pub fn for_seed(seed: u64) -> Self {
        Self::for_style(AvatarCharacter::for_seed(seed).style)
    }

    /// Whether this body stands on two wheels rather than four.
    pub fn two_wheeled(self) -> bool {
        matches!(self, Self::Chariot | Self::OxCart)
    }
}

#[cfg(test)]
mod tests {
    use super::super::craft::SkiffType;
    use super::*;

    /// Every body is reachable from a theme the Wagon is actually picked on -
    /// a body no wagon seed can draw is a body nobody agreed to.
    #[test]
    fn every_wagon_body_is_drawn_by_some_wagon_theme() {
        for body in WagonBody::ALL {
            assert!(
                ThemeArchetype::ALL
                    .iter()
                    .any(|&s| SkiffType::Wagon.weight(s) > 0 && WagonBody::for_style(s) == body),
                "{body:?} is not the body of any theme the Wagon is at home on"
            );
        }
    }
}
