//! Craft-type pick for a seeded boat, land-skiff, helicopter, airplane or
//! airship avatar.
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
//! and ALL TWELVE TYPES ARE NOW BUILT - the longship closed the boat half
//! (#1369) and the rover the skiff half (#1378) - so every seed is drawn as
//! the craft it rolled. The `implemented()` predicate that used to say which
//! way a seed went, and the `Option` the builder tables returned, went with
//! the last unbuilt type (#1382).
//!
//! Seeds may change type as types land; that is allowed by design (a
//! never-saved seeded default is free to change look, and a saved avatar keeps
//! its own generator tree).
//!
//! # The helicopter family has five types (#1614, #1621, #1624, #1628, #1630)
//!
//! [`HelicopterType`] opened with its hero alone, the light utility
//! helicopter, which is the family's universal floor and so every style's
//! answer. It was a type rather than nothing - the airship's answer -
//! because the fan-out was coming and its seam was cut ahead of it: the
//! builder table, the pinned re-roll and the survey readouts all keyed on
//! it. The fan-out's first type (#1618) is the bubble-canopy trainer, at
//! home on the farm, the roadside, the suburbs, the frontier, the coast and
//! the north, where the utility keeps every home she had. Its second is the
//! autogyro, at home on the old worlds the trainer does not fly and in the
//! boiler age - the nine themes no rotorcraft was at home on. Its third is
//! the coaxial, at home on the five speculative and alien themes that last
//! reached a helicopter only through the utility's floor. Its fourth is the
//! tandem-rotor transport, the heavy lifter, at home on the works, the north
//! and the wasteland, where the utility, the trainer and the autogyro are:
//! the first fan-out type to share every home she has.
//!
//! # So has the airplane family (#1617, #1623, #1625, #1629)
//!
//! [`AirplaneType`] opened the way the helicopter's did: its hero alone, the
//! high-wing light monoplane taildragger the owner agreed on the phase-5
//! renders (#1616), the family's universal floor. The fan-out's first
//! airplane (#1618) is the biplane, at home on the old worlds and the
//! frontier (HISTORIC) and in Steampunk - the flying machine of a world
//! before the modern one, as the wagon is the land craft of one before
//! engines - where the high-wing keeps every home she had. Its second is
//! the twin-engine transport, at home on the city, the works and the campus,
//! which reached an airplane only through the high-wing's floor. Its third is
//! the air racer, at home on the five speculative and alien themes, the last
//! that reached an airplane only through it - the coaxial's five.
//!
//! # And the airship family, last (#1657)
//!
//! [`AirshipType`] opened the same way, when the airship - the one family
//! still assembled from a tagged part catalogue, five envelopes, three
//! gondolas and three engine pods drawn with no regard for the avatar's
//! theme - became a craft-type family (epic #1656): its hero alone, the
//! rigid Zeppelin the owner agreed on the phase-1 renders, every style's
//! floor. The fan-out (#1658-#1663) added a blimp, a steam dirigible, a sky
//! galleon, a hot-air balloon, a modern hybrid and an organic floater, each
//! at home in the themes that would rather fly her.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use super::character::AvatarCharacter;
use super::chassis::ChassisFamily;
use super::mood::{AGRARIAN, COASTAL, GRUBBY, HISTORIC, NEON, REGAL, STEAM, holds};
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

/// The craft type of a seeded vehicle avatar, for the five families that
/// have one. The rigged humanoid family has no type pick; the airship has had
/// one since #1657.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CraftType {
    Boat(BoatType),
    Skiff(SkiffType),
    Helicopter(HelicopterType),
    Airplane(AirplaneType),
    Airship(AirshipType),
}

impl CraftType {
    /// Every boat type as a craft type, in [`BoatType::ALL`]'s order - the
    /// pinned re-roll's combo list under a Boat chassis (#1380).
    pub const BOATS: [Self; 6] = [
        Self::Boat(BoatType::Sloop),
        Self::Boat(BoatType::Longship),
        Self::Boat(BoatType::SteamTug),
        Self::Boat(BoatType::Junk),
        Self::Boat(BoatType::Runabout),
        Self::Boat(BoatType::Scow),
    ];

    /// Every skiff type as a craft type, in [`SkiffType::ALL`]'s order.
    pub const SKIFFS: [Self; 6] = [
        Self::Skiff(SkiffType::Roadster),
        Self::Skiff(SkiffType::DuneBuggy),
        Self::Skiff(SkiffType::ArmouredCar),
        Self::Skiff(SkiffType::Cyclecar),
        Self::Skiff(SkiffType::Wagon),
        Self::Skiff(SkiffType::Rover),
    ];

    /// Every helicopter type as a craft type, in [`HelicopterType::ALL`]'s
    /// order.
    pub const HELICOPTERS: [Self; 5] = [
        Self::Helicopter(HelicopterType::Utility),
        Self::Helicopter(HelicopterType::Trainer),
        Self::Helicopter(HelicopterType::Autogyro),
        Self::Helicopter(HelicopterType::Coaxial),
        Self::Helicopter(HelicopterType::Tandem),
    ];

    /// Every airplane type as a craft type, in [`AirplaneType::ALL`]'s
    /// order.
    pub const AIRPLANES: [Self; 4] = [
        Self::Airplane(AirplaneType::HighWing),
        Self::Airplane(AirplaneType::Biplane),
        Self::Airplane(AirplaneType::Twin),
        Self::Airplane(AirplaneType::Racer),
    ];

    /// Every airship type as a craft type, in [`AirshipType::ALL`]'s order
    /// (#1657, the fan-out's six since #1658-#1663).
    pub const AIRSHIPS: [Self; 7] = [
        Self::Airship(AirshipType::Zeppelin),
        Self::Airship(AirshipType::Blimp),
        Self::Airship(AirshipType::SteamDirigible),
        Self::Airship(AirshipType::SkyGalleon),
        Self::Airship(AirshipType::HotAirBalloon),
        Self::Airship(AirshipType::Hybrid),
        Self::Airship(AirshipType::Floater),
    ];

    /// The craft type for a seed, or `None` for the one family without one,
    /// the rigged humanoid (the airship has had one since #1657).
    pub fn for_seed(seed: u64) -> Option<Self> {
        Self::for_character(&AvatarCharacter::for_seed(seed))
    }

    /// The craft type of an **already-derived** anchor - the path that
    /// derives the character exactly once.
    ///
    /// [`Self::for_seed`] is this over [`AvatarCharacter::for_seed`], so
    /// there is one derivation in the crate rather than two that could
    /// drift. The pinned re-roll's hunt (#1380) is why the distinction
    /// earns a function: its predicate has just derived the anchor to test
    /// the four independent axes, and calling `for_seed` beside that would
    /// derive it a second time for every trial - on the UI thread, for a
    /// hunt whose measured worst case is 52 571 trials.
    ///
    /// The type is a second draw on its own salt, so it cannot be *read*
    /// off the anchor the way `chassis` and `style` can; but it is derived
    /// from nothing else, which is what makes one derivation enough.
    pub fn for_character(c: &AvatarCharacter) -> Option<Self> {
        craft_axis(c).ok()
    }

    /// The craft type for a DID. `for_did(did)` is exactly
    /// `for_seed(fnv1a_64(did))`.
    pub fn for_did(did: &str) -> Option<Self> {
        Self::for_seed(fnv1a_64(did))
    }

    /// The chassis family this type belongs to.
    ///
    /// A craft type implies its family absolutely, which is why pinning one
    /// also pins the chassis (`AvatarPins::lock_craft`): a pinned longship
    /// is a pinned Boat, and a chassis row reading "unlocked" beside it
    /// would be a lie.
    pub fn family(self) -> ChassisFamily {
        match self {
            Self::Boat(_) => ChassisFamily::Boat,
            Self::Skiff(_) => ChassisFamily::Skiff,
            Self::Helicopter(_) => ChassisFamily::Helicopter,
            Self::Airplane(_) => ChassisFamily::Airplane,
            Self::Airship(_) => ChassisFamily::Airship,
        }
    }

    /// The types of `family`, or an empty slice for the two families that
    /// have no type pick.
    pub fn all_of(family: ChassisFamily) -> &'static [Self] {
        match family {
            ChassisFamily::Boat => &Self::BOATS,
            ChassisFamily::Skiff => &Self::SKIFFS,
            ChassisFamily::Helicopter => &Self::HELICOPTERS,
            ChassisFamily::Airplane => &Self::AIRPLANES,
            ChassisFamily::Airship => &Self::AIRSHIPS,
            ChassisFamily::Humanoid => &[],
        }
    }

    /// Sampling weight of this type for an avatar of `style` - see
    /// [`BoatType::weight`].
    ///
    /// Zero means the draw can *never* pick this type on that style, and
    /// that is exactly the pinned re-roll's gate (#1380): 310 of the 504
    /// (type, style) pairs are unreachable (the census
    /// `character::tests` pins, #1630), so an ungated combo would offer
    /// mostly pin sets no seed will ever satisfy.
    pub fn weight(self, style: ThemeArchetype) -> u32 {
        match self {
            Self::Boat(t) => t.weight(style),
            Self::Skiff(t) => t.weight(style),
            Self::Helicopter(t) => t.weight(style),
            Self::Airplane(t) => t.weight(style),
            Self::Airship(t) => t.weight(style),
        }
    }

    /// Human-readable display name - the readouts and the pinned re-roll.
    pub fn label(self) -> &'static str {
        match self {
            Self::Boat(t) => t.label(),
            Self::Skiff(t) => t.label(),
            Self::Helicopter(t) => t.label(),
            Self::Airplane(t) => t.label(),
            Self::Airship(t) => t.label(),
        }
    }
}

/// The craft axis of an already-derived anchor: the type it rolled, or why
/// its family has none.
///
/// One match over the six families, so the value and the explanation can
/// never disagree and neither arm is dead. [`CraftType::for_character`] is
/// this with the words thrown away, and the pinned re-roll draws its craft
/// row straight from it: `Ok` as a lockable value, `Err` as the weak
/// `none - {why}` cell with the lock toggle disabled, because locking
/// captures the ROLLED value and here there is none to capture (#1380).
pub fn craft_axis(c: &AvatarCharacter) -> Result<CraftType, &'static str> {
    match c.chassis {
        ChassisFamily::Boat => Ok(CraftType::Boat(BoatType::for_style(c.style, c.seed))),
        ChassisFamily::Skiff => Ok(CraftType::Skiff(SkiffType::for_style(c.style, c.seed))),
        ChassisFamily::Helicopter => Ok(CraftType::Helicopter(HelicopterType::for_style(
            c.style, c.seed,
        ))),
        ChassisFamily::Airplane => Ok(CraftType::Airplane(AirplaneType::for_style(
            c.style, c.seed,
        ))),
        ChassisFamily::Airship => Ok(CraftType::Airship(AirshipType::for_style(c.style, c.seed))),
        ChassisFamily::Humanoid => Err("a rigged avatar has no craft types"),
    }
}

/// The recognisable kind of boat a seeded hover-boat avatar is.
///
/// All six are built, one slice each, in the order the owner set by share of
/// the family (#1368): the SLOOP as the hero (#1363), then the runabout
/// (#1372, 19 %), the scow (#1373, 18 %), the steam tug (#1370, 15 %), the
/// junk (#1371, 10 %) and the longship (#1369, 7 %), which closed the family.
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
///
/// All six are built, one slice each, in the same share order as the boats
/// (#1368): the ROADSTER as the hero (#1364, 30 % of the family), then the
/// wagon (#1377) - a horseless carriage takes all ten historic themes, so she
/// alone is 31 %, the largest share in either family - the dune buggy (#1374,
/// 13 %), the cyclecar (#1376, 10 %), the armoured car (#1375, 8 %) and the
/// rover (#1378, 8 %), which closed the family.
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

/// The recognisable kind of rotorcraft a seeded helicopter avatar is (#1614).
///
/// The hero the owner agreed on the phase-2 renders (#1613) is the family's
/// universal floor, so every style reaches her; the fan-out (#1618) adds the
/// types a theme would rather fly, weighed against her the way the boats and
/// skiffs are against theirs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelicopterType {
    /// Light utility helicopter: a glazed pod on skids, a two-blade main
    /// rotor and a tail rotor on a slim boom - the JetRanger's layout, and
    /// the family's universal floor.
    Utility,
    /// Bubble-canopy trainer: a glazed bubble over an open engine bay, twin
    /// saddle tanks, an open truss boom and tall skids - the light
    /// helicopter of the 1950s (#1621).
    Trainer,
    /// Autogyro: a slim fuselage with two open cockpits, a radial turning
    /// an airscrew at her nose, a free three-blade rotor on a tripod pylon,
    /// and wide main gear and a tail wheel she stands tail-down on - the
    /// Cierva C.30's layout (#1624).
    Autogyro,
    /// Coaxial compound: two rotors turning opposite ways on one mast and no
    /// tail rotor, a sleek fuselage tapering to a shrouded pusher behind an
    /// H-tail, on skids - the near-future machine of the Sikorsky X2 and
    /// S-97 line (#1628).
    Coaxial,
    /// Tandem-rotor transport: a long box cabin under two big rotors turning
    /// opposite ways, one over her cockpit and one on a tall aft pylon with
    /// her engines on its flanks, on four wheels in low sponsons - the heavy
    /// lifter's layout of the CH-47 Chinook (#1630).
    Tandem,
}

/// The recognisable kind of lighter-than-air craft a seeded airship avatar is
/// (#1657, epic #1656).
///
/// The hero the owner agreed on the phase-1 renders is the family's universal
/// floor, so every style reaches her; the fan-out (#1658-#1663) adds the types
/// a theme would rather fly, weighed against her the way the helicopters are
/// against theirs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AirshipType {
    /// Rigid airship: one long doped hull with a cruciform tail, a control
    /// car under the bow and four engine cars on struts - the 1930s liner of
    /// the air, in the line of LZ 127 Graf Zeppelin and LZ 129.
    Zeppelin,
    /// Non-rigid blimp: a short fat envelope, a broad band down each flank
    /// and the night sign in it, a glazed car snug under the belly with an
    /// outrigger engine either side, and an X tail - the Goodyear GZ-20's
    /// line (#1658).
    Blimp,
    /// Steam dirigible: a striped spindle envelope over a long wooden spar
    /// slung on fans of rope, an open boat gondola hung from the spar with
    /// its boiler and funnel, a big wooden pusher and a sail rudder - the
    /// line of Giffard's 1852 airship and La France (#1659).
    SteamDirigible,
    /// Sky galleon: a wooden ship's hull with a sterncastle, slung on
    /// rigging under a long gasbag, a wing sail spread from each gunwale -
    /// the fantasy ship of the air (#1660).
    SkyGalleon,
    /// Hot-air balloon: a gored envelope stood on end over a wicker basket,
    /// its burner on a frame over the basket and its flame lit (#1661).
    HotAirBalloon,
    /// Modern hybrid: a wide twin-lobed hull, a glass-nosed cabin, four
    /// ducted fans and canted twin tails - the Airlander's line, solar film
    /// on her crown on her two solar schemes (#1662).
    Hybrid,
    /// Organic floater: a living gasbag of the alien worlds - a jellyfish
    /// bell with glowing canals and spots, a scalloped margin and long
    /// tentacles that ripple and trail (#1663).
    Floater,
}

/// The recognisable kind of fixed-wing a seeded airplane avatar is (#1617).
///
/// The hero the owner agreed on the phase-5 renders (#1616) is the family's
/// universal floor, so every style reaches her; the fan-out (#1618) adds the
/// types a theme would rather fly, weighed against her the way the
/// helicopters are against theirs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AirplaneType {
    /// High-wing light monoplane taildragger: a round-cowled cabin under a
    /// strut-braced wing, on fat tyres and a tail wheel - the Cub's layout,
    /// and the family's universal floor.
    HighWing,
    /// Radial-engined biplane taildragger: two staggered wings on N struts
    /// and wires, two open cockpits under the upper wing, a radial in an
    /// open cowl ring - the 1930s trainer's layout (#1623).
    Biplane,
    /// Twin-engine light transport taildragger: a cabin with a row of lit
    /// windows, a low wing, a radial in a closed cowl on a nacelle either
    /// side and a twin-fin tail - the Beech 18's layout (#1625).
    Twin,
    /// Single-seat air racer taildragger: a slim fuselage from a pointed
    /// electric cowl to a swept fin, a bubble canopy, a short low wing and
    /// spatted mains - the Formula One pylon racer's layout, drawn for the
    /// near future (#1629).
    Racer,
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

impl HelicopterType {
    pub const ALL: [Self; 5] = [
        Self::Utility,
        Self::Trainer,
        Self::Autogyro,
        Self::Coaxial,
        Self::Tandem,
    ];

    /// The type every helicopter theme can reach - see [`FLOOR`].
    pub const UNIVERSAL: Self = Self::Utility;

    /// Human-readable display name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Utility => "Utility helicopter",
            Self::Trainer => "Bubble-canopy trainer",
            Self::Autogyro => "Autogyro",
            Self::Coaxial => "Coaxial helicopter",
            Self::Tandem => "Tandem-rotor transport",
        }
    }

    /// Lower-case, hyphen-free name for the `--craft` survey filter.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Utility => "utility",
            Self::Trainer => "trainer",
            Self::Autogyro => "autogyro",
            Self::Coaxial => "coaxial",
            Self::Tandem => "tandem",
        }
    }

    /// Whether this type is at home on `style` - the affinity table.
    fn at_home(self, style: ThemeArchetype) -> bool {
        match self {
            // The universal hero. Strongest where a civil light helicopter
            // is what the sky over the street would hold: the eight
            // contemporary themes - the news, police, air-ambulance and
            // crop-spraying machine of a modern world.
            Self::Utility => matches!(
                style,
                ThemeArchetype::ModernCity
                    | ThemeArchetype::Suburban
                    | ThemeArchetype::RuralFarmland
                    | ThemeArchetype::IndustrialPark
                    | ThemeArchetype::CoastalResort
                    | ThemeArchetype::Roadside
                    | ThemeArchetype::CivicCampus
                    | ThemeArchetype::SportsRec
            ),
            // The light machine of the farm strip, the roadside and the
            // suburbs, and the frontier ranch's (AGRARIAN) - with the coast's
            // sightseeing hop and the north's bush machine (#1621 Q-D). The
            // utility keeps all eight of her homes, so where both are at
            // home a trainer is three seeds in seven, and on the frontier
            // and in the north, where only she is, three in four.
            Self::Trainer => {
                holds(AGRARIAN, style)
                    || matches!(
                        style,
                        ThemeArchetype::CoastalResort | ThemeArchetype::Nordic
                    )
            }
            // The rotorcraft of a world before the modern one (#1624 Q-E):
            // the old-world and ritual themes (HISTORIC) but the north and
            // the frontier, where the trainer is at home, and the boiler
            // age's - the nine themes no rotorcraft was at home on. There,
            // where only the utility's floor reached a helicopter, an
            // autogyro is three seeds in four.
            Self::Autogyro => {
                (holds(HISTORIC, style)
                    && !matches!(style, ThemeArchetype::Nordic | ThemeArchetype::WildWest))
                    || style == ThemeArchetype::Steampunk
            }
            // The machine of the worlds still to come (#1628 Q-D): the
            // corporate tower's runabout, the solar town's air taxi, the
            // outpost's shuttle and whatever the aliens fly - the five
            // themes no rotorcraft was at home on. There, where only the
            // utility's floor reached a helicopter, a coaxial is three seeds
            // in four.
            Self::Coaxial => matches!(
                style,
                ThemeArchetype::Cyberpunk
                    | ThemeArchetype::Solarpunk
                    | ThemeArchetype::SpaceOutpost
                    | ThemeArchetype::AlienOrganic
                    | ThemeArchetype::AlienMonolithic
            ),
            // The heavy lifter (#1630 Q-D): the works' crane in the sky, the
            // north's logging and Arctic supply machine, and the wasteland's
            // salvaged army transport. She shares every home she has - the
            // utility's works, the trainer's north and the autogyro's
            // wasteland - so there she is three seeds in seven, and the
            // types already at home there share the other four.
            Self::Tandem => matches!(
                style,
                ThemeArchetype::IndustrialPark | ThemeArchetype::Nordic | ThemeArchetype::PostApoc
            ),
        }
    }

    /// Sampling weight of this type for an avatar of `style`.
    pub fn weight(self, style: ThemeArchetype) -> u32 {
        let floor = if self == Self::UNIVERSAL { FLOOR } else { 0 };
        floor + if self.at_home(style) { AT_HOME } else { 0 }
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

impl AirplaneType {
    pub const ALL: [Self; 4] = [Self::HighWing, Self::Biplane, Self::Twin, Self::Racer];

    /// The type every airplane theme can reach - see [`FLOOR`].
    pub const UNIVERSAL: Self = Self::HighWing;

    /// Human-readable display name.
    pub fn label(self) -> &'static str {
        match self {
            Self::HighWing => "High-wing monoplane",
            Self::Biplane => "Biplane",
            Self::Twin => "Twin-engine transport",
            Self::Racer => "Air racer",
        }
    }

    /// Lower-case, hyphen-free name for the `--craft` survey filter.
    pub fn slug(self) -> &'static str {
        match self {
            Self::HighWing => "high_wing",
            Self::Biplane => "biplane",
            Self::Twin => "twin",
            Self::Racer => "racer",
        }
    }

    /// Whether this type is at home on `style` - the affinity table.
    fn at_home(self, style: ThemeArchetype) -> bool {
        match self {
            // The universal hero. Strongest where a light airplane is what a
            // field beside the town keeps: the farm strip, the roadside
            // airfield and the flying club, the coast and the suburbs - and
            // the bush, where she is the only way in.
            Self::HighWing => matches!(
                style,
                ThemeArchetype::RuralFarmland
                    | ThemeArchetype::Roadside
                    | ThemeArchetype::Suburban
                    | ThemeArchetype::CoastalResort
                    | ThemeArchetype::SportsRec
                    | ThemeArchetype::Nordic
                    | ThemeArchetype::WildWest
            ),
            // The flying machine of a world before the modern one (#1623
            // Q-D): the old-world, ritual and frontier themes (HISTORIC) and
            // the boiler age's. The high-wing keeps all seven of her homes,
            // so in the north and on the frontier, where both are at home, a
            // biplane is three seeds in seven, and on the nine themes that
            // reached an airplane only through the high-wing's floor, three
            // in four.
            Self::Biplane => holds(HISTORIC, style) || style == ThemeArchetype::Steampunk,
            // The light transport of the city, the works and the campus
            // (#1625 Q-D): the commuter, the freighter and the survey twin
            // of the three themes that reached an airplane only through the
            // high-wing's floor. There, a twin is three seeds in four.
            Self::Twin => matches!(
                style,
                ThemeArchetype::ModernCity
                    | ThemeArchetype::IndustrialPark
                    | ThemeArchetype::CivicCampus
            ),
            // The racer of the five speculative and alien themes (#1629
            // Q-D): the neon city's, the solar town's, the outpost's and
            // whatever the aliens race - the last themes that reached an
            // airplane only through the high-wing's floor, as they were the
            // last a helicopter reached only through the utility's. There, a
            // racer is three seeds in four.
            Self::Racer => matches!(
                style,
                ThemeArchetype::Cyberpunk
                    | ThemeArchetype::Solarpunk
                    | ThemeArchetype::SpaceOutpost
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

impl AirshipType {
    pub const ALL: [Self; 7] = [
        Self::Zeppelin,
        Self::Blimp,
        Self::SteamDirigible,
        Self::SkyGalleon,
        Self::HotAirBalloon,
        Self::Hybrid,
        Self::Floater,
    ];

    /// The type every airship theme can reach - see [`FLOOR`].
    pub const UNIVERSAL: Self = Self::Zeppelin;

    /// Human-readable name, for survey readouts.
    pub fn label(self) -> &'static str {
        match self {
            Self::Zeppelin => "Zeppelin",
            Self::Blimp => "Blimp",
            Self::SteamDirigible => "Steam dirigible",
            Self::SkyGalleon => "Sky galleon",
            Self::HotAirBalloon => "Hot-air balloon",
            Self::Hybrid => "Hybrid airship",
            Self::Floater => "Organic floater",
        }
    }

    /// Stable lower-case name, for `render --craft`.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Zeppelin => "zeppelin",
            Self::Blimp => "blimp",
            Self::SteamDirigible => "steam_dirigible",
            Self::SkyGalleon => "sky_galleon",
            Self::HotAirBalloon => "balloon",
            Self::Hybrid => "hybrid",
            Self::Floater => "floater",
        }
    }

    /// Whether this type is at home in `style`.
    fn at_home(self, style: ThemeArchetype) -> bool {
        match self {
            // The universal hero (#1656 decision 4's sketch). Strongest
            // where the great liner of the air belongs: the stately classical
            // and civic worlds, the city she moored over, the gothic night
            // she raided through, and the north the Norge flew to the Pole.
            Self::Zeppelin => matches!(
                style,
                ThemeArchetype::AncientClassical
                    | ThemeArchetype::CivicCampus
                    | ThemeArchetype::ModernCity
                    | ThemeArchetype::GothicHorror
                    | ThemeArchetype::Nordic
            ),
            // The stadium's and the seaside's advertising blimp, the
            // suburb's and the roadside's, the neon city's lit ad blimp and
            // the wasteland's patched one (#1658).
            Self::Blimp => matches!(
                style,
                ThemeArchetype::SportsRec
                    | ThemeArchetype::CoastalResort
                    | ThemeArchetype::Suburban
                    | ThemeArchetype::Roadside
                    | ThemeArchetype::Cyberpunk
                    | ThemeArchetype::PostApoc
            ),
            // The boiler age's airship and the frontier's (#1659): Verne's
            // and Giffard's, over the works and the gothic night too.
            Self::SteamDirigible => matches!(
                style,
                ThemeArchetype::Steampunk
                    | ThemeArchetype::WildWest
                    | ThemeArchetype::IndustrialPark
                    | ThemeArchetype::GothicHorror
            ),
            // The worlds of sailing ships and legend (#1660): the pirate's
            // and the fantasy realm's, the castle's, the north's and the
            // shogunate's.
            Self::SkyGalleon => matches!(
                style,
                ThemeArchetype::Pirate
                    | ThemeArchetype::Fantasy
                    | ThemeArchetype::Medieval
                    | ThemeArchetype::Nordic
                    | ThemeArchetype::FeudalJapan
            ),
            // The country fair's, the roadside's and the suburb's balloon,
            // and the festival balloon of the old Americas (#1661).
            Self::HotAirBalloon => matches!(
                style,
                ThemeArchetype::RuralFarmland
                    | ThemeArchetype::Roadside
                    | ThemeArchetype::Suburban
                    | ThemeArchetype::Mesoamerican
            ),
            // The worlds still to come (#1662): the outpost's and the solar
            // town's airship, the works' heavy lifter, the neon city's and
            // the monolith's.
            Self::Hybrid => matches!(
                style,
                ThemeArchetype::SpaceOutpost
                    | ThemeArchetype::Solarpunk
                    | ThemeArchetype::IndustrialPark
                    | ThemeArchetype::Cyberpunk
                    | ThemeArchetype::AlienMonolithic
            ),
            // The living gasbag of the organic alien world (#1663) - the
            // airships' biggest theme, which reached nothing of its own.
            Self::Floater => style == ThemeArchetype::AlienOrganic,
        }
    }

    /// Sampling weight of this type for an avatar of `style`.
    pub fn weight(self, style: ThemeArchetype) -> u32 {
        let floor = if self == Self::UNIVERSAL { FLOOR } else { 0 };
        floor + if self.at_home(style) { AT_HOME } else { 0 }
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
            HelicopterType::ALL.map(HelicopterType::slug).to_vec(),
            AirplaneType::ALL.map(AirplaneType::slug).to_vec(),
            AirshipType::ALL.map(AirshipType::slug).to_vec(),
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
            .chain(HelicopterType::ALL.iter().map(|t| t.label()))
            .chain(AirplaneType::ALL.iter().map(|t| t.label()))
            .chain(AirshipType::ALL.iter().map(|t| t.label()))
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
            assert!(
                HelicopterType::UNIVERSAL.weight(style) > 0,
                "{style:?} cannot reach the utility helicopter"
            );
            assert!(
                AirplaneType::UNIVERSAL.weight(style) > 0,
                "{style:?} cannot reach the high-wing monoplane"
            );
        }
    }

    /// Every theme must have a real choice - the floor and at least one type
    /// that theme actually suggests. A theme with only the floor would look
    /// identical across its whole population once the types land, which is
    /// the defect the type pick exists to remove.
    ///
    /// ONE named exception, for boats only: PIRATE, whose boats have all been
    /// sloops since the steam tug left the labouring themes (#1370) - a funnel
    /// on a buccaneer being the costume error that narrowing took out.
    ///
    /// The sloop is not the bare FLOOR there: Pirate is at home on her, and
    /// since #1379 a Pirate sloop is the one boat in the fleet that wears a
    /// KIT - a black ensign, gun ports with lids, a taffrail staff - which no
    /// other theme can draw. That kit, not the count of types, is what keeps a
    /// Pirate population from reading as everyone else's; the five rigs vary
    /// her on top of it. The kit's own half of this bargain is pinned by
    /// `default_visuals::boats::tests::the_pirate_kit_is_pirate_only`, which
    /// is where the assertion that it is Pirate-only lives - deliberately NOT
    /// repeated here, because the kit is geometry and this module derives
    /// picks.
    ///
    /// Named, not relaxed: "the floor counts where the theme is at home on
    /// it" would unpin every COASTAL and REGAL theme with it.
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
        for t in HelicopterType::ALL {
            assert!(
                ThemeArchetype::ALL.iter().any(|&s| t.at_home(s)),
                "{t:?} is at home in no theme"
            );
        }
        for t in AirplaneType::ALL {
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

    /// Every vehicle family carries a type - the airship too, since #1657 -
    /// and the rigged humanoid does not.
    #[test]
    fn the_type_follows_the_family() {
        let mut families = 0;
        for s in 0u64..400 {
            match (ChassisFamily::for_seed(s), CraftType::for_seed(s)) {
                (ChassisFamily::Boat, Some(CraftType::Boat(_)))
                | (ChassisFamily::Skiff, Some(CraftType::Skiff(_)))
                | (ChassisFamily::Helicopter, Some(CraftType::Helicopter(_)))
                | (ChassisFamily::Airplane, Some(CraftType::Airplane(_)))
                | (ChassisFamily::Airship, Some(CraftType::Airship(_))) => families += 1,
                (ChassisFamily::Humanoid, None) => families += 1,
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
        let mut helicopters: Vec<HelicopterType> = Vec::new();
        let mut airplanes: Vec<AirplaneType> = Vec::new();
        for s in 0u64..4000 {
            match CraftType::for_seed(s) {
                Some(CraftType::Boat(t)) if !boats.contains(&t) => boats.push(t),
                Some(CraftType::Skiff(t)) if !skiffs.contains(&t) => skiffs.push(t),
                Some(CraftType::Helicopter(t)) if !helicopters.contains(&t) => helicopters.push(t),
                Some(CraftType::Airplane(t)) if !airplanes.contains(&t) => airplanes.push(t),
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
        assert_eq!(
            helicopters.len(),
            HelicopterType::ALL.len(),
            "unsampled helicopter types: got {helicopters:?}"
        );
        assert_eq!(
            airplanes.len(),
            AirplaneType::ALL.len(),
            "unsampled airplane types: got {airplanes:?}"
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
        // And the trainer's (#1621): AGRARIAN's four with the coast and the
        // north, and none of the utility's town homes past them.
        for style in [
            RuralFarmland,
            Roadside,
            Suburban,
            WildWest,
            CoastalResort,
            Nordic,
        ] {
            assert!(HelicopterType::Trainer.at_home(style), "{style:?}");
        }
        for style in [ModernCity, IndustrialPark, CivicCampus, SportsRec] {
            assert!(!HelicopterType::Trainer.at_home(style), "{style:?}");
        }
        // And the autogyro's (#1624): HISTORIC but the trainer's north and
        // frontier, with Steampunk - and none of the other two types' homes,
        // nor the speculative themes.
        for style in [
            Medieval,
            PostApoc,
            Fantasy,
            AncientClassical,
            FeudalJapan,
            Mesoamerican,
            GothicHorror,
            Pirate,
            Steampunk,
        ] {
            assert!(HelicopterType::Autogyro.at_home(style), "{style:?}");
            assert!(!HelicopterType::Utility.at_home(style), "{style:?}");
            assert!(!HelicopterType::Trainer.at_home(style), "{style:?}");
        }
        for style in [Nordic, WildWest, ModernCity, CoastalResort, Cyberpunk] {
            assert!(!HelicopterType::Autogyro.at_home(style), "{style:?}");
        }
        // And the coaxial's (#1628): the five speculative and alien themes,
        // none of which another helicopter type is at home on - and nothing
        // past them.
        for style in ThemeArchetype::ALL {
            let future = matches!(
                style,
                Cyberpunk | Solarpunk | SpaceOutpost | AlienOrganic | AlienMonolithic
            );
            assert_eq!(HelicopterType::Coaxial.at_home(style), future, "{style:?}");
            if future {
                for other in [
                    HelicopterType::Utility,
                    HelicopterType::Trainer,
                    HelicopterType::Autogyro,
                ] {
                    assert!(!other.at_home(style), "{other:?} on {style:?}");
                }
            }
        }
        // And the tandem's (#1630): the works, the north and the wasteland,
        // each already another helicopter type's home - and nothing past
        // them.
        for style in ThemeArchetype::ALL {
            let home = matches!(style, IndustrialPark | Nordic | PostApoc);
            assert_eq!(HelicopterType::Tandem.at_home(style), home, "{style:?}");
            if home {
                assert!(
                    [
                        HelicopterType::Utility,
                        HelicopterType::Trainer,
                        HelicopterType::Autogyro,
                    ]
                    .iter()
                    .any(|t| t.at_home(style)),
                    "{style:?} is hers alone"
                );
            }
        }
        // And the biplane's (#1623): all ten of HISTORIC with Steampunk, and
        // none of the town, coast, sport or speculative themes.
        for style in [
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
            Steampunk,
        ] {
            assert!(AirplaneType::Biplane.at_home(style), "{style:?}");
        }
        for style in [
            ModernCity,
            Suburban,
            CoastalResort,
            SportsRec,
            Cyberpunk,
            AlienOrganic,
        ] {
            assert!(!AirplaneType::Biplane.at_home(style), "{style:?}");
        }
        // And the twin's (#1625): the city, the works and the campus, none
        // of which either other airplane is at home on - and nothing past
        // them.
        for style in ThemeArchetype::ALL {
            let city = matches!(style, ModernCity | IndustrialPark | CivicCampus);
            assert_eq!(AirplaneType::Twin.at_home(style), city, "{style:?}");
            if city {
                assert!(!AirplaneType::HighWing.at_home(style), "{style:?}");
                assert!(!AirplaneType::Biplane.at_home(style), "{style:?}");
            }
        }
        // And the racer's (#1629): the five speculative and alien themes,
        // none of which any other airplane is at home on - and with hers
        // every theme is some airplane's home.
        for style in ThemeArchetype::ALL {
            let speculative = matches!(
                style,
                Cyberpunk | Solarpunk | SpaceOutpost | AlienOrganic | AlienMonolithic
            );
            assert_eq!(AirplaneType::Racer.at_home(style), speculative, "{style:?}");
            let others = [
                AirplaneType::HighWing,
                AirplaneType::Biplane,
                AirplaneType::Twin,
            ]
            .into_iter()
            .filter(|t| t.at_home(style))
            .count();
            assert_eq!(others == 0, speculative, "{style:?}");
        }
    }
}
