//! Craft-type pick for a seeded boat or land-skiff avatar.
//!
//! The discrete pick *inside* a vehicle family. It used to be an arrangement -
//! monohull / catamaran / trimaran / barge, default / dune / trike / armoured -
//! which barely moves the silhouette, and silhouette is all that reads at the
//! chase camera's range (a craft is about 109 px a metre there). It is now a
//! recognisable real-world craft **type**, and the type is keyed to the
//! avatar's [`ThemeArchetype`] style, so a Nordic avatar arrives on a longship
//! and a cyberpunk one in a cyclecar rather than both drawing from one
//! theme-blind pool (owner decision 2 of the vehicle redesign, #1359 / #1362).
//!
//! # How the pick is weighted
//!
//! Each type lists the themes it is *at home* in, drawn from the shared mood
//! taxonomy ([`super::mood`]) plus the odd explicit theme where no group fits.
//! One type per family is also the **universal floor** - the sloop and the
//! roadster - and carries a smaller weight on every theme, so every style can
//! reach it and no theme is ever cornered into a single answer. A floor type on
//! a theme it is also at home in simply scores both.
//!
//! The draw runs on its own salted sub-stream, like [`ChassisFamily`]'s, so it
//! is decorrelated from every sibling deriver and adding it left every other
//! seeded value bit-identical.
//!
//! # A type is a property of the seed, not of what is built from it
//!
//! [`BoatType::for_seed`] and [`SkiffType::for_seed`] answer for every seed of
//! their family, whether or not anything builds the type they name. That is
//! deliberate and is what the survey readouts want: the type slices that
//! follow (#1363 sloop and #1364 roadster, then #1369-#1378) each need to find
//! the seeds their type was picked for *before* that type can be built.
//! `render --outfit` prints the picked type and `render --family-seeds --craft
//! <type>` filters by it. The build seam that routes a seed to its type's own
//! builder landed with the first builder of each family, in #1363 and #1364,
//! and `implemented()` is what says which way a seed goes.
//!
//! Seeds may change type as types land; that is allowed by design (a
//! never-saved seeded default is free to change look, and a saved avatar keeps
//! its own generator tree).

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use super::character::AvatarCharacter;
use super::chassis::ChassisFamily;
use super::mood::{COASTAL, GRUBBY, HISTORIC, NEON, REGAL, STEAM, holds};
use crate::seeded_defaults::hash::fnv1a_64;
use crate::seeded_defaults::scene::{ThemeArchetype, pick_weighted};

/// Sub-stream salt for the craft-type draw - distinct from every sibling
/// avatar deriver salt so the pick never aliases another stream.
const AVATAR_CRAFT_SALT: u64 = 0xC4AF_7C4A_F7C4_AF7C;

/// Weight a type carries on a theme it is at home in.
pub(super) const AT_HOME: u32 = 6;

/// Weight the family's universal floor type carries on *every* theme, so no
/// style is ever cornered and the floor is always a live answer. A floor type
/// at home in the theme scores this on top of [`AT_HOME`].
pub(super) const FLOOR: u32 = 2;

/// The craft type of a seeded vehicle avatar, for the two families that have
/// one. Airships and the rigged humanoid family have no type pick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CraftType {
    Boat(BoatType),
    Skiff(SkiffType),
}

impl CraftType {
    /// The craft type for a seed, or `None` for a family without one (the
    /// airship, whose envelope forms are its variety, and the humanoid).
    pub fn for_seed(seed: u64) -> Option<Self> {
        match ChassisFamily::for_seed(seed) {
            ChassisFamily::Boat => Some(Self::Boat(BoatType::for_seed(seed))),
            ChassisFamily::Skiff => Some(Self::Skiff(SkiffType::for_seed(seed))),
            ChassisFamily::Airship | ChassisFamily::Humanoid => None,
        }
    }

    /// The craft type for a DID. `for_did(did)` is exactly
    /// `for_seed(fnv1a_64(did))`.
    pub fn for_did(did: &str) -> Option<Self> {
        Self::for_seed(fnv1a_64(did))
    }

    /// Human-readable display name - the readouts and the pinned re-roll.
    pub fn label(self) -> &'static str {
        match self {
            Self::Boat(t) => t.label(),
            Self::Skiff(t) => t.label(),
        }
    }
}

/// The recognisable kind of boat a seeded hover-boat avatar is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoatType {
    /// Fore-and-aft rigged sailing yacht - the family's universal floor, and
    /// the hero the rest are measured against.
    Sloop,
    /// Clinker-built double-ender under a square sail, shields on the sheer.
    Longship,
    /// Working steam tug: a tall raked funnel over a wheelhouse forward,
    /// tyre fenders along the sheer and a low towing deck aft - the boat of
    /// the themes with a boiler in them (#1370).
    SteamTug,
    /// Battened-lug junk with a high transom stern.
    Junk,
    /// Varnished planing runabout - a fast open launch.
    Runabout,
    /// Blunt working scow: a flat cargo deck on a punt hull.
    Scow,
}

/// The recognisable kind of land craft a seeded skiff avatar is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkiffType {
    /// Open two-seat sports car on a boat-tailed body - the family's universal
    /// floor, and the hero the rest are measured against.
    Roadster,
    /// Stripped tube-framed buggy on balloon tyres.
    DuneBuggy,
    /// Riveted armoured car with vision slits and a turret ring.
    ArmouredCar,
    /// Narrow three-wheeled cyclecar.
    Cyclecar,
    /// Horseless carriage - a wagon body on spoked wheels.
    Wagon,
    /// Six-wheeled exploration rover with a dish and a cargo rack.
    Rover,
}

impl BoatType {
    pub const ALL: [Self; 6] = [
        Self::Sloop,
        Self::Longship,
        Self::SteamTug,
        Self::Junk,
        Self::Runabout,
        Self::Scow,
    ];

    /// The type every boat theme can reach - see [`FLOOR`].
    pub const UNIVERSAL: Self = Self::Sloop;

    /// Human-readable display name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Sloop => "Sloop",
            Self::Longship => "Longship",
            Self::SteamTug => "Steam tug",
            Self::Junk => "Junk",
            Self::Runabout => "Runabout",
            Self::Scow => "Scow",
        }
    }

    /// Lower-case, hyphen-free name for the `--craft` survey filter.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Sloop => "sloop",
            Self::Longship => "longship",
            Self::SteamTug => "steamtug",
            Self::Junk => "junk",
            Self::Runabout => "runabout",
            Self::Scow => "scow",
        }
    }

    /// Whether this type is at home on `style` - the affinity table.
    ///
    /// Read as "what would this culture actually put on the water": the mood
    /// groups carry most of it, with the odd explicit theme where no group has
    /// the right shape.
    fn at_home(self, style: ThemeArchetype) -> bool {
        match self {
            // The universal hero. Strongest where a pleasure sailing yacht is
            // the obvious craft - the seaside and sporting moods, the stately
            // ones, and the buccaneer (a gaff cutter under black colours is
            // the historically apt pirate craft, not a galleon).
            Self::Sloop => {
                holds(COASTAL, style) || holds(REGAL, style) || style == ThemeArchetype::Pirate
            }
            // Norse and the old-world martial themes; a galley reads as the
            // same hull for the classical one.
            Self::Longship => matches!(
                style,
                ThemeArchetype::Nordic
                    | ThemeArchetype::Medieval
                    | ThemeArchetype::Fantasy
                    | ThemeArchetype::AncientClassical
            ),
            // Anything with a boiler in it, and the two frontier themes that
            // burn wood under one. NOT the rest of the labouring craft
            // (WORKING): a funnel on a medieval, Nordic or pirate boat is the
            // costume error, so those seeds go to the longship and the sloop
            // instead - the owner's narrowing on #1370's phase-1 renders.
            Self::SteamTug => {
                holds(STEAM, style)
                    || matches!(style, ThemeArchetype::WildWest | ThemeArchetype::PostApoc)
            }
            // Battened lug rig - the eastern and ritual old-world themes.
            Self::Junk => matches!(
                style,
                ThemeArchetype::FeudalJapan
                    | ThemeArchetype::GothicHorror
                    | ThemeArchetype::Mesoamerican
            ),
            // A fast varnished launch: the neon moods plus the leisure ones.
            // CivicCampus is mine rather than the brief's - REGAL left it on
            // the Sloop floor and nothing else, and a launch on a campus
            // boating lake is the obvious second answer (#1362).
            Self::Runabout => {
                holds(NEON, style)
                    || matches!(
                        style,
                        ThemeArchetype::SportsRec
                            | ThemeArchetype::CoastalResort
                            | ThemeArchetype::Solarpunk
                            | ThemeArchetype::CivicCampus
                    )
            }
            // A blunt working barge - the grimy and frontier themes.
            Self::Scow => {
                holds(GRUBBY, style)
                    || matches!(
                        style,
                        ThemeArchetype::WildWest
                            | ThemeArchetype::PostApoc
                            | ThemeArchetype::RuralFarmland
                    )
            }
        }
    }

    /// Sampling weight of this type for an avatar of `style`.
    pub fn weight(self, style: ThemeArchetype) -> u32 {
        let floor = if self == Self::UNIVERSAL { FLOOR } else { 0 };
        floor + if self.at_home(style) { AT_HOME } else { 0 }
    }

    /// Whether anything actually BUILDS this type yet (#1363).
    ///
    /// The pick is a property of the seed and answers for every type from the
    /// day the table landed; the geometry arrives one slice at a time. Until a
    /// type's slice lands, a seed that picked it is DRAWN as the family's
    /// [`UNIVERSAL`](Self::UNIVERSAL) floor - the sloop - while still
    /// *reporting* the type it rolled, which is what lets each fan-out slice
    /// find its own seeds before it builds them.
    ///
    /// Kept in step with the builder table by
    /// `default_visuals::boats::tests::a_type_is_implemented_exactly_when_
    /// something_builds_it`; the failure it prevents is a type that claims to
    /// be built and silently draws a sloop.
    pub fn implemented(self) -> bool {
        match self {
            Self::Sloop | Self::Runabout | Self::Scow | Self::SteamTug => true,
            Self::Longship | Self::Junk => false,
        }
    }

    /// The type for a seed, weighted by the avatar's style.
    pub fn for_seed(seed: u64) -> Self {
        Self::for_style(AvatarCharacter::for_seed(seed).style, seed)
    }

    /// The type for an already-resolved style - the path the anchor-holding
    /// callers take, so the anchor is derived once.
    pub fn for_style(style: ThemeArchetype, seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ AVATAR_CRAFT_SALT);
        pick_weighted(&Self::ALL, |t| t.weight(style), &mut rng).unwrap_or(Self::UNIVERSAL)
    }
}

impl SkiffType {
    pub const ALL: [Self; 6] = [
        Self::Roadster,
        Self::DuneBuggy,
        Self::ArmouredCar,
        Self::Cyclecar,
        Self::Wagon,
        Self::Rover,
    ];

    /// The type every skiff theme can reach - see [`FLOOR`].
    pub const UNIVERSAL: Self = Self::Roadster;

    /// Human-readable display name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Roadster => "Roadster",
            Self::DuneBuggy => "Dune buggy",
            Self::ArmouredCar => "Armoured car",
            Self::Cyclecar => "Cyclecar",
            Self::Wagon => "Wagon",
            Self::Rover => "Rover",
        }
    }

    /// Lower-case, hyphen-free name for the `--craft` survey filter.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Roadster => "roadster",
            Self::DuneBuggy => "dunebuggy",
            Self::ArmouredCar => "armouredcar",
            Self::Cyclecar => "cyclecar",
            Self::Wagon => "wagon",
            Self::Rover => "rover",
        }
    }

    /// Whether this type is at home on `style` - the affinity table.
    fn at_home(self, style: ThemeArchetype) -> bool {
        match self {
            // The universal hero. Strongest where a smart open car is what the
            // street would hold.
            Self::Roadster => {
                holds(REGAL, style)
                    || matches!(
                        style,
                        ThemeArchetype::Suburban
                            | ThemeArchetype::CivicCampus
                            | ThemeArchetype::CoastalResort
                    )
            }
            // The weekend toy and the frontier runabout. Suburban is mine
            // rather than the brief's: it needed a second type beyond the
            // floor, and a buggy on a suburban driveway is a real thing.
            Self::DuneBuggy => matches!(
                style,
                ThemeArchetype::PostApoc
                    | ThemeArchetype::WildWest
                    | ThemeArchetype::Roadside
                    | ThemeArchetype::SportsRec
                    | ThemeArchetype::CoastalResort
                    | ThemeArchetype::Suburban
            ),
            Self::ArmouredCar => matches!(
                style,
                ThemeArchetype::PostApoc
                    | ThemeArchetype::IndustrialPark
                    | ThemeArchetype::ModernCity
            ),
            // A narrow cyclecar is the small, odd, light machine: the neon
            // fringes, plus the campus it would actually be parked on.
            Self::Cyclecar => matches!(
                style,
                ThemeArchetype::Cyberpunk
                    | ThemeArchetype::AlienMonolithic
                    | ThemeArchetype::Solarpunk
                    | ThemeArchetype::CivicCampus
            ),
            // A horseless carriage is the land craft of a world without
            // engines, so it takes every historic theme. Steampunk and
            // RuralFarmland are mine rather than the brief's: HISTORIC holds
            // neither, and a steam car and a farm wagon are exactly this body
            // (#1362).
            Self::Wagon => {
                holds(HISTORIC, style)
                    || matches!(
                        style,
                        ThemeArchetype::Steampunk | ThemeArchetype::RuralFarmland
                    )
            }
            Self::Rover => matches!(
                style,
                ThemeArchetype::SpaceOutpost
                    | ThemeArchetype::AlienOrganic
                    | ThemeArchetype::AlienMonolithic
            ),
        }
    }

    /// Sampling weight of this type for an avatar of `style`.
    pub fn weight(self, style: ThemeArchetype) -> u32 {
        let floor = if self == Self::UNIVERSAL { FLOOR } else { 0 };
        floor + if self.at_home(style) { AT_HOME } else { 0 }
    }

    /// Whether anything actually BUILDS this type yet (#1364).
    ///
    /// The skiff half of [`BoatType::implemented`]'s seam, and the same
    /// contract: the pick is a property of the seed and answers for every type
    /// from the day the table landed, while the geometry arrives one slice at
    /// a time. Until a type's slice lands, a seed that picked it is DRAWN as
    /// the family's [`UNIVERSAL`](Self::UNIVERSAL) floor - the roadster -
    /// while still *reporting* the type it rolled.
    ///
    /// The Wagon was built second (#1377): a horseless carriage takes all ten
    /// historic themes, so it alone is 31 % of the family against the
    /// Roadster's 30. The Dune buggy is the third (#1374), on its six leisure
    /// and frontier themes: 13 % of the family. The Cyclecar is the fourth
    /// (#1376), on the neon themes and the campus; after her, 23 of the 151
    /// skiff seeds under 600 still pick a type nothing draws yet - the
    /// armoured car and the rover.
    ///
    /// Kept in step with the builder table by
    /// `default_visuals::skiffs::tests::a_skiff_type_is_implemented_exactly_
    /// when_something_builds_it`.
    pub fn implemented(self) -> bool {
        match self {
            Self::Roadster | Self::Wagon | Self::DuneBuggy | Self::Cyclecar => true,
            Self::ArmouredCar | Self::Rover => false,
        }
    }

    /// The type for a seed, weighted by the avatar's style.
    pub fn for_seed(seed: u64) -> Self {
        Self::for_style(AvatarCharacter::for_seed(seed).style, seed)
    }

    /// The type for an already-resolved style.
    pub fn for_style(style: ThemeArchetype, seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ AVATAR_CRAFT_SALT);
        pick_weighted(&Self::ALL, |t| t.weight(style), &mut rng).unwrap_or(Self::UNIVERSAL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both readouts and the pinned re-roll key off these, so a duplicate
    /// slug would silently make one type unfindable.
    #[test]
    fn slugs_and_labels_are_unique_within_a_family() {
        for slugs in [
            BoatType::ALL.map(BoatType::slug).to_vec(),
            SkiffType::ALL.map(SkiffType::slug).to_vec(),
        ] {
            let mut sorted = slugs.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(
                sorted.len(),
                slugs.len(),
                "duplicate craft slug in {slugs:?}"
            );
        }
        let mut labels: Vec<&str> = BoatType::ALL
            .iter()
            .map(|t| t.label())
            .chain(SkiffType::ALL.iter().map(|t| t.label()))
            .collect();
        labels.sort_unstable();
        let before = labels.len();
        labels.dedup();
        assert_eq!(before, labels.len(), "two craft types share a label");
    }

    /// The floor keeps its promise: every style can reach the family hero, so
    /// no theme is ever cornered into a type nobody has built yet.
    #[test]
    fn the_universal_floor_is_reachable_from_every_theme() {
        for style in ThemeArchetype::ALL {
            assert!(
                BoatType::UNIVERSAL.weight(style) > 0,
                "{style:?} cannot reach the sloop"
            );
            assert!(
                SkiffType::UNIVERSAL.weight(style) > 0,
                "{style:?} cannot reach the roadster"
            );
        }
    }

    /// Every theme must have a real choice - the floor and at least one type
    /// that theme actually suggests. A theme with only the floor would look
    /// identical across its whole population once the types land, which is
    /// the defect the type pick exists to remove.
    ///
    /// ONE named exception, for boats only: PIRATE, whose boats have all been
    /// sloops since the steam tug left the labouring themes (#1370). The
    /// sloop is not the bare floor there - Pirate is at home on her, the gaff
    /// cutter being the apt pirate craft - and her five rigs, the square
    /// topsail among them, are that theme's variety; a funnel on a buccaneer
    /// is the costume error the narrowing took out. Named, not relaxed: "the
    /// floor counts where the theme is at home on it" would unpin every
    /// COASTAL and REGAL theme with it.
    #[test]
    fn every_theme_reaches_at_least_two_types_per_family() {
        for style in ThemeArchetype::ALL {
            let boats = BoatType::ALL.iter().filter(|t| t.weight(style) > 0).count();
            let skiffs = SkiffType::ALL
                .iter()
                .filter(|t| t.weight(style) > 0)
                .count();
            if style == ThemeArchetype::Pirate {
                // Pinned both ways, so the exception cannot outlive its
                // reason: one boat type, and a type Pirate is at home on.
                assert_eq!(
                    boats, 1,
                    "Pirate reaches {boats} boat types - drop its exception"
                );
                assert!(BoatType::Sloop.at_home(style));
            } else {
                assert!(boats >= 2, "{style:?} boats have only {boats} type(s)");
            }
            assert!(skiffs >= 2, "{style:?} skiffs have only {skiffs} type(s)");
        }
    }

    /// Every type has to be someone's answer, or it is a type nobody will ever
    /// see and the slice that builds it is wasted.
    #[test]
    fn every_type_is_at_home_somewhere() {
        for t in BoatType::ALL {
            assert!(
                ThemeArchetype::ALL.iter().any(|&s| t.at_home(s)),
                "{t:?} is at home in no theme"
            );
        }
        for t in SkiffType::ALL {
            assert!(
                ThemeArchetype::ALL.iter().any(|&s| t.at_home(s)),
                "{t:?} is at home in no theme"
            );
        }
    }

    #[test]
    fn deterministic() {
        for s in 0u64..50 {
            assert_eq!(CraftType::for_seed(s), CraftType::for_seed(s));
        }
        assert_eq!(
            CraftType::for_did("did:plc:abc"),
            CraftType::for_seed(fnv1a_64("did:plc:abc"))
        );
    }

    /// Only the two part-assembled vehicle families carry a type.
    #[test]
    fn the_type_follows_the_family() {
        let mut families = 0;
        for s in 0u64..400 {
            match (ChassisFamily::for_seed(s), CraftType::for_seed(s)) {
                (ChassisFamily::Boat, Some(CraftType::Boat(_)))
                | (ChassisFamily::Skiff, Some(CraftType::Skiff(_))) => families += 1,
                (ChassisFamily::Airship | ChassisFamily::Humanoid, None) => families += 1,
                (fam, craft) => panic!("seed {s}: {fam:?} got craft {craft:?}"),
            }
        }
        assert_eq!(families, 400);
    }

    /// The sampler must actually spread across the table: every type is
    /// reached by some seed, and no family collapses onto its floor.
    #[test]
    fn the_sampler_reaches_every_type() {
        let mut boats: Vec<BoatType> = Vec::new();
        let mut skiffs: Vec<SkiffType> = Vec::new();
        for s in 0u64..4000 {
            match CraftType::for_seed(s) {
                Some(CraftType::Boat(t)) if !boats.contains(&t) => boats.push(t),
                Some(CraftType::Skiff(t)) if !skiffs.contains(&t) => skiffs.push(t),
                _ => {}
            }
        }
        assert_eq!(
            boats.len(),
            BoatType::ALL.len(),
            "unsampled boat types: got {boats:?}"
        );
        assert_eq!(
            skiffs.len(),
            SkiffType::ALL.len(),
            "unsampled skiff types: got {skiffs:?}"
        );
    }

    /// The pick is weighted, not uniform: a theme's own types dominate its
    /// population, or the theme keying is decorative. And a type the theme
    /// gives no weight is never drawn at all.
    #[test]
    fn a_themes_own_types_dominate_its_population() {
        // A Steampunk boat is a steam tug or a scow (6 each) far more often
        // than the sloop the floor always leaves open (2) - about 6 in 7. It
        // was written on Nordic until the tug left that theme (#1370).
        let style = ThemeArchetype::Steampunk;
        let mut at_home = 0;
        for s in 0u64..4000 {
            let t = BoatType::for_style(style, s);
            assert!(
                t.weight(style) > 0,
                "seed {s}: drew {t:?}, which {style:?} gives no weight"
            );
            at_home += usize::from(t.at_home(style));
        }
        // 12 of 14 weight is at home; allow a wide sampling margin.
        assert!(
            (3200..=3700).contains(&at_home),
            "{at_home} of 4000 Steampunk boats were at home, expected about 3430"
        );
    }

    /// The affinities the prose claims to take from the mood groups really
    /// do come from them - so a group edit that empties a craft type is caught
    /// here rather than in a survey months later.
    #[test]
    fn the_mood_groups_still_carry_the_affinities_they_are_cited_for() {
        assert!(BoatType::SteamTug.at_home(ThemeArchetype::Steampunk));
        assert!(BoatType::Scow.at_home(ThemeArchetype::RuralFarmland));
        assert!(SkiffType::Wagon.at_home(ThemeArchetype::Medieval));
        assert!(SkiffType::Roadster.at_home(ThemeArchetype::Fantasy));
        assert!(BoatType::Sloop.at_home(ThemeArchetype::CoastalResort));
        // And the four additions this slice made over the brief's table.
        assert!(BoatType::Runabout.at_home(ThemeArchetype::CivicCampus));
        assert!(SkiffType::Wagon.at_home(ThemeArchetype::Steampunk));
        assert!(SkiffType::Wagon.at_home(ThemeArchetype::RuralFarmland));
        assert!(SkiffType::DuneBuggy.at_home(ThemeArchetype::Suburban));
        // And the steam tug's narrowed reach (#1370): the boiler themes and
        // the two frontier ones, and none of the pre-industrial labouring
        // themes that WORKING still gathers for the sloop's rigs.
        use ThemeArchetype::*;
        for style in [Steampunk, WildWest, PostApoc] {
            assert!(BoatType::SteamTug.at_home(style), "{style:?}");
        }
        for style in [Medieval, Nordic, Pirate] {
            assert!(!BoatType::SteamTug.at_home(style), "{style:?}");
        }
    }
}
