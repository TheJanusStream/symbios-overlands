//! Style-mood taxonomy over [`ThemeArchetype`] - the named theme groups the
//! avatar pipeline keys flavour off.
//!
//! Each of the 24 archetypes belongs to at least one broad group, so no
//! population is a "desert" with nothing tagged for it (#792). A theme may sit
//! in several (a grimy neon craft is both `NEON` and `GRUBBY`); a consumer
//! draws the group whose *read* it wants. The narrow audiences at the bottom
//! are finer than the broad groups, for flavour whose read only fits a couple
//! of themes (#793).
//!
//! Two consumers today, which is why this lives here beside
//! [`ThemeArchetype`] rather than inside either of them: the styled vehicle
//! part catalogue ([`crate::pds::avatar::parts::vehicle`]) tags each part with
//! the groups it belongs on, and the craft-type affinities ([`super::craft`])
//! weight which real-world craft a theme's avatar is likely to be (#1362).

use crate::seeded_defaults::scene::ThemeArchetype::{
    self, AlienMonolithic, AlienOrganic, AncientClassical, CivicCampus, CoastalResort, Cyberpunk,
    Fantasy, FeudalJapan, GothicHorror, IndustrialPark, Medieval, Mesoamerican, ModernCity, Nordic,
    Pirate, PostApoc, Roadside, RuralFarmland, Solarpunk, SpaceOutpost, SportsRec, Steampunk,
    Suburban, WildWest,
};

// The broad groups. NEON/STEAM/MARTIAL/REGAL/
// GRUBBY/HISTORIC are the originals, widened to fold the desert themes that fit
// them: FeudalJapan / Mesoamerican / GothicHorror → HISTORIC (old-world / ritual);
// AlienOrganic → NEON (bioluminescent); Roadside / RuralFarmland / Suburban →
// GRUBBY (worn, workaday, off-road ground craft). COASTAL is a new home for the
// seaside / sporting moods that fit none of the originals.
pub const NEON: &[ThemeArchetype] = &[
    Cyberpunk,
    SpaceOutpost,
    AlienMonolithic,
    Solarpunk,
    AlienOrganic,
];
pub const STEAM: &[ThemeArchetype] = &[Steampunk, IndustrialPark, ModernCity];
pub const MARTIAL: &[ThemeArchetype] = &[Medieval, Nordic, WildWest, PostApoc, Pirate];
pub const REGAL: &[ThemeArchetype] = &[Fantasy, AncientClassical, CivicCampus];
// GRUBBY = grimy / worn / workaday ground craft: industrial soot, frontier
// scrap, and the ordinary agrarian / roadside / suburban beaters (buggies,
// knobby tyres, cargo decks). Widened past the original five so the farm /
// roadside / suburban desert themes get bespoke parts without losing the
// steampunk / industrial ones (a straight fold, no re-tag regression).
pub const GRUBBY: &[ThemeArchetype] = &[
    Steampunk,
    IndustrialPark,
    WildWest,
    PostApoc,
    Cyberpunk,
    Roadside,
    RuralFarmland,
    Suburban,
];
pub const HISTORIC: &[ThemeArchetype] = &[
    Medieval,
    Nordic,
    WildWest,
    PostApoc,
    Fantasy,
    AncientClassical,
    FeudalJapan,
    Mesoamerican,
    GothicHorror,
    Pirate,
];
/// Seaside / leisure / sporting moods - resort cruisers, sport skiffs. Homes
/// CoastalResort / SportsRec, which fit none of the ground-craft groups.
pub const COASTAL: &[ThemeArchetype] = &[CoastalResort, SportsRec, Solarpunk];

// Narrow audiences (#793 mood-group depth) - finer than the broad groups
// above, for flavour whose read only fits a couple of themes.
/// Longship / dragon-prow craft - a Spine serpent figurehead's home.
pub const NORSE_FEY: &[ThemeArchetype] = &[Nordic, Fantasy];
/// Working / labouring craft - rope coils, cleats, capstans read on these.
pub const WORKING: &[ThemeArchetype] = &[
    Nordic,
    Medieval,
    WildWest,
    PostApoc,
    Steampunk,
    IndustrialPark,
    // The narrowest audience that is also the most obviously right: rope
    // coils, cleats and a capstan are not merely *allowed* on a buccaneer's
    // craft, they are what its deck is for.
    Pirate,
];
/// Funereal / temple / old-world craft - a hanging stern lantern's home.
pub const SEPULCHRAL: &[ThemeArchetype] = &[GothicHorror, FeudalJapan, Medieval];
/// Buccaneer craft - the black colours, a carved billet-head, a pierced gun
/// deck. A group of one, and deliberately: these are period dress rather than
/// a mood, and a jolly roger on a Nordic longship is a costume error. Pirate
/// also sits in MARTIAL / HISTORIC / WORKING, where the parts genuinely are
/// shared, so this narrows rather than replaces.
pub const BUCCANEER: &[ThemeArchetype] = &[Pirate];

/// Agrarian / roadside / ordinary-ground craft - the wooden buckboard read
/// (the #793 issue's "RUSTIC", folded into GRUBBY in #792 but kept as a narrow
/// audience here so the buckboard doesn't land on a cyberpunk skiff).
pub const AGRARIAN: &[ThemeArchetype] = &[RuralFarmland, Roadside, Suburban, WildWest];

/// Whether `style` belongs to the mood `group`.
///
/// The one way to read a group: the lists are small and unsorted, and a
/// linear scan over at most ten archetypes keeps them readable as prose in
/// the source rather than sorted for a binary search nobody would profit
/// from.
pub fn holds(group: &[ThemeArchetype], style: ThemeArchetype) -> bool {
    group.contains(&style)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The promise the module docstring makes: no archetype is a desert.
    /// Every theme has to sit in at least one BROAD group, or the flavour
    /// keyed off these groups would skip that theme's whole population.
    #[test]
    fn every_theme_sits_in_a_broad_group() {
        const BROAD: [&[ThemeArchetype]; 7] =
            [NEON, STEAM, MARTIAL, REGAL, GRUBBY, HISTORIC, COASTAL];
        for style in ThemeArchetype::ALL {
            assert!(
                BROAD.iter().any(|g| holds(g, style)),
                "{style:?} belongs to no broad mood group"
            );
        }
    }

    /// A narrow audience is meant to be narrower than the broad groups it
    /// refines - if one grew past them it should have been a broad group.
    #[test]
    fn the_narrow_audiences_stay_narrow() {
        for group in [NORSE_FEY, WORKING, SEPULCHRAL, BUCCANEER, AGRARIAN] {
            assert!(
                !group.is_empty() && group.len() <= HISTORIC.len(),
                "a narrow audience of {} is no longer narrow",
                group.len()
            );
        }
    }
}
