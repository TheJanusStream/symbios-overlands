//! Avatars in the Catalogue (#1840, epic #1837): ready-made avatars of
//! every kind, which a person can wear at once or copy to their Inventory.
//!
//! An entry is a **recipe**, not a stored record. The seeded builders
//! already draw every kind of avatar from a seed
//! ([`AvatarRecord::default_for_seed`]), and the Avatar editor's pinned
//! re-roll already hunts the first seed at or after a start that draws a
//! pinned family, craft type, style, ornateness and wear
//! ([`AvatarPins::find_seed`], through [`SeedPins`]). So an entry is a slug,
//! a name, a one-line description, its pins and a fixed start seed, and
//! building it is that hunt and then that builder - nothing here draws a
//! body of its own, and no body is built differently for being in the
//! Catalogue.
//!
//! Built on demand - when the entry is selected, worn or copied - and
//! cached for the life of the process in the entry itself
//! ([`AvatarEntry::record`]), never per frame. The cache is a pure function
//! of the entry, so it holds nothing a test or a later call could disagree
//! with.
//!
//! **Coverage.** Every [`CraftType`] is its own entry, named for its type.
//! A person, a beast and a winged body are rigged engine bodies, and the
//! seeded builder hands them to the engine's own roll
//! (`engine_default_for_seed`, `engine_beast_for_seed`,
//! `engine_winged_for_seed`): the style, ornateness and wear axes feed only
//! the vehicle builders, so pinning them would choose nothing a person can
//! see. What the engine roll varies is a person's frame (masculine to
//! feminine), age and build, a beast's family - dog, cat or horse - and a
//! winged body's - bird, bat or sugar glider (#1882) - and whether it is a
//! cross of two. So each rigged entry carries a [`Look`], the part of the
//! roll its name promises, and its start seed is one whose roll shows it.
//!
//! **Seeds may draw differently in a later version** - cross-version
//! determinism is not a constraint on the seeded builders. The test
//! `every_entry_builds_the_avatar_it_is_named_for` is what catches an entry
//! that drifted: each must find its seed inside [`HUNT_BUDGET`] and build
//! the family, craft type and look it is named for.

use std::sync::OnceLock;

use crate::pds::AvatarRecord;
use crate::pds::avatar::EngineAvatarRecord;
use crate::pds::inventory::AvatarItem;
use crate::seeded_defaults::{
    AirplaneType, AirshipType, AvatarPins, BoatType, ChassisFamily, CraftType, HelicopterType,
    SeedPins, SkiffType,
};

/// How many trials past its start an entry's hunt may take. The pinned
/// re-roll's own cap is two million trials, a safety net; an entry is
/// built on the UI thread the moment it is selected, so its start is
/// chosen to land at once, and this is the bound the drift test holds it
/// to.
pub const HUNT_BUDGET: u64 = 4_096;

/// The part of the engine's roll a rigged entry's name promises.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// A vehicle: its craft type is pinned, and that is its whole name.
    Craft,
    /// A person of this frame, age and build.
    Person {
        frame: Frame,
        age: Age,
        build: Build,
    },
    /// A beast of this family, crossed with `cross` when it is one.
    Beast {
        family: symbios_avatar::plan::Family,
        cross: Option<symbios_avatar::plan::Family>,
    },
    /// A winged body of this family, crossed with `cross` when it is one,
    /// and led by `family`, as its name puts it first (#1882): a cross past
    /// an even blend is led by its second family and stands as that one does.
    Winged {
        family: symbios_avatar::WingedFamily,
        cross: Option<symbios_avatar::WingedFamily>,
    },
    /// A bird and no cross whose head the engine draws as an owl's (#1882),
    /// asked of the engine itself (`WingedParams::plumage_for_head`, an owl's
    /// plumage for an owl's head alone), so an owl here is an owl in the
    /// picture whatever line the engine draws one by.
    Owl,
}

/// Where a person's body sits on the engine's masculine-feminine axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frame {
    Feminine,
    Masculine,
}

/// A person's age, as a band of the engine's whole years.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Age {
    /// Thirty or under.
    Young,
    /// Fifty-five or over.
    Older,
    /// Any age.
    Any,
}

/// A person's build, read off the engine's mass axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Build {
    /// Mass well above the middle.
    Heavy,
    /// Mass well below the middle.
    Slight,
    /// Any build.
    Any,
}

/// Femininity past which a frame reads as one or the other.
const FRAME_SPLIT: f32 = 0.25;
/// The young band's top, in years.
const YOUNG_MAX: u32 = 30;
/// The older band's floor, in years.
const OLDER_MIN: u32 = 55;
/// Mass past which a build reads as heavy or slight.
const BUILD_SPLIT: f32 = 0.4;

impl Look {
    /// Whether the engine body `body` shows this look. A vehicle's look is
    /// its craft type, which the hunt itself holds, so it has nothing to
    /// ask of a body.
    pub fn holds(&self, body: Option<&EngineAvatarRecord>) -> bool {
        match *self {
            Self::Craft => body.is_none(),
            Self::Person { frame, age, build } => {
                let Some(body) = body else {
                    return false;
                };
                if !matches!(body.archetype, symbios_avatar::Archetype::Humanoid(_)) {
                    return false;
                }
                let c = &body.composites;
                let frame_ok = match frame {
                    Frame::Feminine => c.femininity > FRAME_SPLIT,
                    Frame::Masculine => c.femininity < -FRAME_SPLIT,
                };
                let age_ok = match age {
                    Age::Young => c.age <= YOUNG_MAX,
                    Age::Older => c.age >= OLDER_MIN,
                    Age::Any => true,
                };
                let build_ok = match build {
                    Build::Heavy => c.mass > BUILD_SPLIT,
                    Build::Slight => c.mass < -BUILD_SPLIT,
                    Build::Any => true,
                };
                frame_ok && age_ok && build_ok
            }
            Self::Beast { family, cross } => match body.map(|body| &body.archetype) {
                Some(symbios_avatar::Archetype::Quadruped(params)) => {
                    params.family == family && params.hybrid == cross
                }
                _ => false,
            },
            Self::Winged { family, cross } => match body.map(|body| &body.archetype) {
                Some(symbios_avatar::Archetype::Winged(params)) => {
                    params.family == family
                        && params.hybrid == cross
                        && symbios_avatar::Kinship::of(params).leading == family
                }
                _ => false,
            },
            Self::Owl => match body.map(|body| &body.archetype) {
                Some(symbios_avatar::Archetype::Winged(params)) => {
                    params.family == symbios_avatar::WingedFamily::Bird
                        && params.hybrid.is_none()
                        && params.plumage_for_head() == symbios_avatar::Plumage::OWL
                }
                _ => false,
            },
        }
    }
}

/// One avatar the Catalogue offers.
pub struct AvatarEntry {
    /// Stable identifier, unique across the whole Catalogue - every slug
    /// starts `avatar_`, which no item's does.
    pub slug: &'static str,
    /// The name the Catalogue lists it under.
    pub name: &'static str,
    /// What it is, in one line.
    pub description: &'static str,
    /// What the hunt pins: the family always, the craft type for a vehicle.
    pub pins: AvatarPins,
    /// Where the hunt starts.
    pub start: u64,
    /// What a rigged entry's roll must show.
    pub look: Look,
    /// The built avatar, once something asked for it.
    built: OnceLock<Option<Built>>,
}

/// A built entry: the avatar record the seeded builder drew, and the
/// Inventory item it copies to ([`AvatarItem::from_record`]), kept beside
/// it so a panel that asks every frame whether it can be worn reads the
/// item rather than copying the record again.
pub struct Built {
    /// The seed the hunt found.
    pub seed: u64,
    /// What [`AvatarRecord::default_for_seed`] built from it.
    pub record: AvatarRecord,
    /// The record as an avatar item, or why it cannot be one.
    pub item: Result<AvatarItem, String>,
    /// How many parts a vehicle's tree has; `None` for a rigged body.
    pub parts: Option<usize>,
}

/// The family a craft type belongs to, as a `const fn` so the registry can
/// be a `static` ([`CraftType::family`] is the runtime answer, and the
/// drift test holds the two together).
const fn family_of(craft: CraftType) -> ChassisFamily {
    match craft {
        CraftType::Boat(_) => ChassisFamily::Boat,
        CraftType::Skiff(_) => ChassisFamily::Skiff,
        CraftType::Helicopter(_) => ChassisFamily::Helicopter,
        CraftType::Airplane(_) => ChassisFamily::Airplane,
        CraftType::Airship(_) => ChassisFamily::Airship,
    }
}

const fn pins(chassis: ChassisFamily, craft: Option<CraftType>) -> AvatarPins {
    AvatarPins {
        chassis: Some(chassis),
        style: None,
        ornateness: None,
        wear: None,
        craft,
    }
}

impl AvatarEntry {
    const fn craft(
        slug: &'static str,
        name: &'static str,
        description: &'static str,
        craft: CraftType,
        start: u64,
    ) -> Self {
        Self {
            slug,
            name,
            description,
            pins: pins(family_of(craft), Some(craft)),
            start,
            look: Look::Craft,
            built: OnceLock::new(),
        }
    }

    const fn person(
        slug: &'static str,
        name: &'static str,
        description: &'static str,
        (frame, age, build): (Frame, Age, Build),
        start: u64,
    ) -> Self {
        Self {
            slug,
            name,
            description,
            pins: pins(ChassisFamily::Humanoid, None),
            start,
            look: Look::Person { frame, age, build },
            built: OnceLock::new(),
        }
    }

    const fn beast(
        slug: &'static str,
        name: &'static str,
        description: &'static str,
        family: symbios_avatar::plan::Family,
        cross: Option<symbios_avatar::plan::Family>,
        start: u64,
    ) -> Self {
        Self {
            slug,
            name,
            description,
            pins: pins(ChassisFamily::Beast, None),
            start,
            look: Look::Beast { family, cross },
            built: OnceLock::new(),
        }
    }

    const fn winged(
        slug: &'static str,
        name: &'static str,
        description: &'static str,
        look: Look,
        start: u64,
    ) -> Self {
        Self {
            slug,
            name,
            description,
            pins: pins(ChassisFamily::Winged, None),
            start,
            look,
            built: OnceLock::new(),
        }
    }

    /// The family the entry is pinned to.
    pub fn family(&self) -> ChassisFamily {
        self.pins.chassis.unwrap_or(ChassisFamily::Humanoid)
    }

    /// The craft type a vehicle entry is pinned to.
    pub fn craft_type(&self) -> Option<CraftType> {
        self.pins.craft
    }

    /// Whether the entry is a rigged body - a person, a beast or a winged
    /// body - rather than a vehicle's tree.
    pub fn is_rigged(&self) -> bool {
        self.family().is_rigged()
    }

    /// What the family is called on a detail row.
    pub fn family_label(&self) -> &'static str {
        family_label(self.family())
    }

    /// The seed the entry builds from: the first at or after its start
    /// that draws its pins.
    pub fn seed(&self) -> Option<u64> {
        SeedPins::find_seed(&self.pins, self.start)
    }

    /// The avatar this entry builds, built the first time it is asked for
    /// and kept. `None` only when the hunt fails, which the drift test
    /// forbids.
    pub fn built(&self) -> Option<&Built> {
        self.built
            .get_or_init(|| {
                #[cfg(test)]
                BUILDS.with(|n| n.set(n.get() + 1));
                self.seed().map(|seed| {
                    let record = AvatarRecord::default_for_seed(seed);
                    let item = AvatarItem::from_record(&record);
                    let parts = record.body.visuals().map(crate::ui::room::caps::node_count);
                    Built {
                        seed,
                        record,
                        item,
                        parts,
                    }
                })
            })
            .as_ref()
    }

    /// The avatar record this entry builds - [`Self::built`]'s record.
    pub fn record(&self) -> Option<&AvatarRecord> {
        self.built().map(|built| &built.record)
    }

    /// The visuals tree a vehicle entry is pictured from; `None` for a
    /// rigged body, whose picture is the body itself, built by the game's own
    /// job (#1842). Builds the entry.
    pub fn visuals(&self) -> Option<&crate::pds::Generator> {
        self.record().and_then(|record| record.body.visuals())
    }

    /// Whether the entry has been built in this process - test-only, for
    /// the work count.
    #[cfg(test)]
    pub(crate) fn is_built(&self) -> bool {
        self.built.get().is_some()
    }

    /// Whether a search query (already lower-cased) finds this entry: its
    /// name, its slug, its family, its craft type, or the word "avatar".
    pub fn matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        self.name.to_lowercase().contains(query)
            || self.slug.contains(query)
            || self.family_label().to_lowercase().contains(query)
            || family_group(self.family()).to_lowercase().contains(query)
            || self
                .craft_type()
                .is_some_and(|craft| craft.label().to_lowercase().contains(query))
            || CATEGORY.to_lowercase().contains(query)
    }
}

// How many entry builds ran on this thread, for the work count in
// `ui::catalogue`'s tests. Thread-local for the reason `item_preview`'s
// restage count is (#1147, #1189).
#[cfg(test)]
thread_local! {
    static BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Builds counted on this thread since the last call. Test-only.
#[cfg(test)]
pub(crate) fn take_build_count() -> usize {
    BUILDS.with(|n| n.replace(0))
}

/// The Catalogue section the avatars list under.
pub const CATEGORY: &str = "Avatars";

/// What a family is called on an entry's detail row.
pub fn family_label(family: ChassisFamily) -> &'static str {
    match family {
        ChassisFamily::Humanoid => "Person",
        ChassisFamily::Beast => "Beast",
        ChassisFamily::Winged => "Winged",
        ChassisFamily::Boat => "Boat",
        ChassisFamily::Skiff => "Land-skiff",
        ChassisFamily::Helicopter => "Helicopter",
        ChassisFamily::Airplane => "Airplane",
        ChassisFamily::Airship => "Airship",
    }
}

/// What a family's group is called in the browse tree.
pub fn family_group(family: ChassisFamily) -> &'static str {
    match family {
        ChassisFamily::Humanoid => "People",
        ChassisFamily::Beast => "Beasts",
        ChassisFamily::Winged => "Winged",
        ChassisFamily::Boat => "Boats",
        ChassisFamily::Skiff => "Land-skiffs",
        ChassisFamily::Helicopter => "Helicopters",
        ChassisFamily::Airplane => "Airplanes",
        ChassisFamily::Airship => "Airships",
    }
}

/// The order the browse tree lists the families in: the rigged bodies
/// first, then the vehicles by medium.
pub const FAMILY_ORDER: [ChassisFamily; 8] = [
    ChassisFamily::Humanoid,
    ChassisFamily::Beast,
    ChassisFamily::Winged,
    ChassisFamily::Boat,
    ChassisFamily::Skiff,
    ChassisFamily::Helicopter,
    ChassisFamily::Airplane,
    ChassisFamily::Airship,
];

use symbios_avatar::WingedFamily as Wings;
use symbios_avatar::plan::Family as Beast;

/// Every avatar the Catalogue offers, in browse order.
pub static AVATARS: [AvatarEntry; 45] = [
    // ---- People: the engine's frame, age and build ----
    AvatarEntry::person(
        "avatar_person_young_woman",
        "Young woman",
        "A young woman on foot, with a face, hair and clothes of her own.",
        (Frame::Feminine, Age::Young, Build::Any),
        1_840_068,
    ),
    AvatarEntry::person(
        "avatar_person_young_man",
        "Young man",
        "A young man on foot, with a face, hair and clothes of his own.",
        (Frame::Masculine, Age::Young, Build::Any),
        1_840_031,
    ),
    AvatarEntry::person(
        "avatar_person_older_woman",
        "Older woman",
        "A woman in her later years, on foot.",
        (Frame::Feminine, Age::Older, Build::Any),
        1_840_101,
    ),
    AvatarEntry::person(
        "avatar_person_older_man",
        "Older man",
        "A man in his later years, on foot.",
        (Frame::Masculine, Age::Older, Build::Any),
        1_840_466,
    ),
    AvatarEntry::person(
        "avatar_person_heavy_set_woman",
        "Heavy-set woman",
        "A broad, heavy-set woman, on foot.",
        (Frame::Feminine, Age::Any, Build::Heavy),
        1_840_074,
    ),
    AvatarEntry::person(
        "avatar_person_slight_man",
        "Slight man",
        "A slight, slender man, on foot.",
        (Frame::Masculine, Age::Any, Build::Slight),
        1_840_073,
    ),
    // ---- Beasts: the engine's family, and its crosses ----
    AvatarEntry::beast(
        "avatar_beast_dog",
        "Dog",
        "A dog on four legs.",
        Beast::Dog,
        None,
        1_840_132,
    ),
    AvatarEntry::beast(
        "avatar_beast_cat",
        "Cat",
        "A cat on four legs.",
        Beast::Cat,
        None,
        1_840_029,
    ),
    AvatarEntry::beast(
        "avatar_beast_horse",
        "Horse",
        "A horse on four legs.",
        Beast::Horse,
        None,
        1_840_059,
    ),
    AvatarEntry::beast(
        "avatar_beast_dog_cat",
        "Dog and cat cross",
        "A dog with a cat in its blood, on four legs.",
        Beast::Dog,
        Some(Beast::Cat),
        1_840_057,
    ),
    AvatarEntry::beast(
        "avatar_beast_cat_horse",
        "Cat and horse cross",
        "A cat with a horse in its blood, on four legs.",
        Beast::Cat,
        Some(Beast::Horse),
        1_840_238,
    ),
    AvatarEntry::beast(
        "avatar_beast_horse_dog",
        "Horse and dog cross",
        "A horse with a dog in its blood, on four legs.",
        Beast::Horse,
        Some(Beast::Dog),
        1_846_494,
    ),
    // ---- Winged: the engine's family, and its crosses (#1882) ----
    AvatarEntry::winged(
        "avatar_winged_owl",
        "Owl",
        "An owl, its face a feathered disc, on two feet and on the wing.",
        Look::Owl,
        1_840_433,
    ),
    AvatarEntry::winged(
        "avatar_winged_bat",
        "Fruit bat",
        "A fox-faced fruit bat, on all fours and on the wing.",
        Look::Winged {
            family: Wings::Bat,
            cross: None,
        },
        1_840_231,
    ),
    AvatarEntry::winged(
        "avatar_winged_glider",
        "Sugar glider",
        "A sugar glider, on all fours and gliding.",
        Look::Winged {
            family: Wings::Glider,
            cross: None,
        },
        1_840_168,
    ),
    AvatarEntry::winged(
        "avatar_winged_bird_bat",
        "Bird and bat cross",
        "A bird with a bat in its blood, on the wing.",
        Look::Winged {
            family: Wings::Bird,
            cross: Some(Wings::Bat),
        },
        1_842_068,
    ),
    AvatarEntry::winged(
        "avatar_winged_glider_bird",
        "Glider and bird cross",
        "A sugar glider with a bird in its blood.",
        Look::Winged {
            family: Wings::Glider,
            cross: Some(Wings::Bird),
        },
        1_844_146,
    ),
    // ---- Boats ----
    AvatarEntry::craft(
        "avatar_boat_sloop",
        "Sloop",
        "A fore-and-aft rigged sailing yacht, as a hover-boat.",
        CraftType::Boat(BoatType::Sloop),
        1_840_025,
    ),
    AvatarEntry::craft(
        "avatar_boat_longship",
        "Longship",
        "A clinker-built double-ender under a square sail, shields along her sides.",
        CraftType::Boat(BoatType::Longship),
        1_840_051,
    ),
    AvatarEntry::craft(
        "avatar_boat_steamtug",
        "Steam tug",
        "A working steam tug with a tall raked funnel and tyre fenders.",
        CraftType::Boat(BoatType::SteamTug),
        1_840_069,
    ),
    AvatarEntry::craft(
        "avatar_boat_junk",
        "Junk",
        "A junk under battened lug sails, with a high stern.",
        CraftType::Boat(BoatType::Junk),
        1_840_112,
    ),
    AvatarEntry::craft(
        "avatar_boat_runabout",
        "Runabout",
        "A varnished planing runabout - a fast open launch.",
        CraftType::Boat(BoatType::Runabout),
        1_840_026,
    ),
    AvatarEntry::craft(
        "avatar_boat_scow",
        "Scow",
        "A blunt working scow with a flat cargo deck.",
        CraftType::Boat(BoatType::Scow),
        1_840_017,
    ),
    // ---- Land-skiffs ----
    AvatarEntry::craft(
        "avatar_skiff_roadster",
        "Roadster",
        "An open two-seat sports car on a boat-tailed body.",
        CraftType::Skiff(SkiffType::Roadster),
        1_840_001,
    ),
    AvatarEntry::craft(
        "avatar_skiff_dunebuggy",
        "Dune buggy",
        "A stripped, tube-framed buggy on balloon tyres.",
        CraftType::Skiff(SkiffType::DuneBuggy),
        1_840_014,
    ),
    AvatarEntry::craft(
        "avatar_skiff_armouredcar",
        "Armoured car",
        "A riveted armoured car with vision slits and a turret ring.",
        CraftType::Skiff(SkiffType::ArmouredCar),
        1_840_202,
    ),
    AvatarEntry::craft(
        "avatar_skiff_cyclecar",
        "Cyclecar",
        "A narrow three-wheeled cyclecar.",
        CraftType::Skiff(SkiffType::Cyclecar),
        1_840_061,
    ),
    AvatarEntry::craft(
        "avatar_skiff_wagon",
        "Wagon",
        "A horseless carriage - a wagon body on spoked wheels.",
        CraftType::Skiff(SkiffType::Wagon),
        1_840_004,
    ),
    AvatarEntry::craft(
        "avatar_skiff_rover",
        "Rover",
        "A six-wheeled exploration rover with a dish and a cargo rack.",
        CraftType::Skiff(SkiffType::Rover),
        1_840_006,
    ),
    // ---- Helicopters ----
    AvatarEntry::craft(
        "avatar_helicopter_utility",
        "Utility helicopter",
        "A light helicopter: a glazed pod on skids under a two-blade rotor.",
        CraftType::Helicopter(HelicopterType::Utility),
        1_840_034,
    ),
    AvatarEntry::craft(
        "avatar_helicopter_trainer",
        "Bubble-canopy trainer",
        "A light helicopter with a glazed bubble over an open engine bay.",
        CraftType::Helicopter(HelicopterType::Trainer),
        1_840_016,
    ),
    AvatarEntry::craft(
        "avatar_helicopter_autogyro",
        "Autogyro",
        "Two open cockpits under a free-turning rotor, with a propeller at the nose.",
        CraftType::Helicopter(HelicopterType::Autogyro),
        1_840_052,
    ),
    AvatarEntry::craft(
        "avatar_helicopter_coaxial",
        "Coaxial helicopter",
        "Two rotors turning opposite ways on one mast, and a pusher behind.",
        CraftType::Helicopter(HelicopterType::Coaxial),
        1_840_009,
    ),
    AvatarEntry::craft(
        "avatar_helicopter_tandem",
        "Tandem-rotor transport",
        "A long box cabin under two big rotors, one at each end.",
        CraftType::Helicopter(HelicopterType::Tandem),
        1_840_018,
    ),
    // ---- Airplanes ----
    AvatarEntry::craft(
        "avatar_airplane_high_wing",
        "High-wing monoplane",
        "A light taildragger with a strut-braced wing over its cabin.",
        CraftType::Airplane(AirplaneType::HighWing),
        1_840_027,
    ),
    AvatarEntry::craft(
        "avatar_airplane_biplane",
        "Biplane",
        "Two staggered wings over two open cockpits and a radial engine.",
        CraftType::Airplane(AirplaneType::Biplane),
        1_840_044,
    ),
    AvatarEntry::craft(
        "avatar_airplane_twin",
        "Twin-engine transport",
        "A light transport with a row of lit windows and an engine on each wing.",
        CraftType::Airplane(AirplaneType::Twin),
        1_840_085,
    ),
    AvatarEntry::craft(
        "avatar_airplane_racer",
        "Air racer",
        "A single-seat racer with a bubble canopy and a short low wing.",
        CraftType::Airplane(AirplaneType::Racer),
        1_840_000,
    ),
    // ---- Airships ----
    AvatarEntry::craft(
        "avatar_airship_zeppelin",
        "Zeppelin",
        "A rigid airship: one long hull, a control car under the bow and four engine cars.",
        CraftType::Airship(AirshipType::Zeppelin),
        1_840_012,
    ),
    AvatarEntry::craft(
        "avatar_airship_blimp",
        "Blimp",
        "A short, fat soft airship with a glazed car under its belly.",
        CraftType::Airship(AirshipType::Blimp),
        1_840_010,
    ),
    AvatarEntry::craft(
        "avatar_airship_steam_dirigible",
        "Steam dirigible",
        "A striped envelope over an open boat gondola with its boiler and funnel.",
        CraftType::Airship(AirshipType::SteamDirigible),
        1_840_133,
    ),
    AvatarEntry::craft(
        "avatar_airship_sky_galleon",
        "Sky galleon",
        "A wooden ship's hull slung under a long gasbag, a wing sail at each side.",
        CraftType::Airship(AirshipType::SkyGalleon),
        1_840_078,
    ),
    AvatarEntry::craft(
        "avatar_airship_balloon",
        "Hot-air balloon",
        "A wicker basket under a balloon, its burner lit.",
        CraftType::Airship(AirshipType::HotAirBalloon),
        1_840_002,
    ),
    AvatarEntry::craft(
        "avatar_airship_hybrid",
        "Hybrid airship",
        "A wide twin-lobed hull with a glass-nosed cabin and four ducted fans.",
        CraftType::Airship(AirshipType::Hybrid),
        1_840_008,
    ),
    AvatarEntry::craft(
        "avatar_airship_floater",
        "Organic floater",
        "A living gasbag like a jellyfish bell, trailing long tentacles.",
        CraftType::Airship(AirshipType::Floater),
        1_840_165,
    ),
];

/// The avatar entry with this slug.
pub fn avatar_by_slug(slug: &str) -> Option<&'static AvatarEntry> {
    AVATARS.iter().find(|entry| entry.slug == slug)
}

/// The engine body a built record wears, for a rigged entry.
pub fn engine_body(record: &AvatarRecord) -> Option<&EngineAvatarRecord> {
    record
        .body
        .rigged_ref()
        .and_then(|rig| rig.resolved.as_ref())
        .map(|resolved| &resolved.body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::inventory::AvatarKind;
    use std::collections::HashSet;

    /// What an avatar item of `family` reads as - the Inventory's own kind
    /// tag, which cannot tell an airship from a helicopter.
    fn kind_of(family: ChassisFamily) -> AvatarKind {
        match family {
            ChassisFamily::Humanoid => AvatarKind::Person,
            ChassisFamily::Beast => AvatarKind::Beast,
            ChassisFamily::Winged => AvatarKind::Winged,
            ChassisFamily::Boat => AvatarKind::Boat,
            ChassisFamily::Skiff => AvatarKind::Skiff,
            ChassisFamily::Airplane => AvatarKind::Airplane,
            ChassisFamily::Helicopter | ChassisFamily::Airship => AvatarKind::Rotorcraft,
        }
    }

    /// The drift test (#1840): every entry finds its seed inside its
    /// budget, and the record it builds is the family, the craft type and
    /// the look it is named for. Seeds may draw differently in a later
    /// version; this is what says which entry moved.
    #[test]
    fn every_entry_builds_the_avatar_it_is_named_for() {
        for entry in &AVATARS {
            let slug = entry.slug;
            let seed = entry
                .seed()
                .unwrap_or_else(|| panic!("{slug}: the hunt found no seed"));
            assert!(
                seed.wrapping_sub(entry.start) < HUNT_BUDGET,
                "{slug}: found {seed}, {} trials past its start",
                seed.wrapping_sub(entry.start)
            );
            assert_eq!(ChassisFamily::for_seed(seed), entry.family(), "{slug}");
            assert_eq!(CraftType::for_seed(seed), entry.craft_type(), "{slug}");
            if let Some(craft) = entry.craft_type() {
                assert_eq!(entry.name, craft.label(), "{slug} is named for its type");
                assert_eq!(craft.family(), entry.family(), "{slug}");
            }

            let built = entry.built().expect("builds");
            assert_eq!(built.seed, seed);
            assert_eq!(built.record, AvatarRecord::default_for_seed(seed), "{slug}");
            let item = built.item.as_ref().expect("copies to an item");
            assert_eq!(item.kind(), kind_of(entry.family()), "{slug}");
            assert_eq!(entry.is_rigged(), built.record.body.visuals().is_none());
            assert!(
                entry.look.holds(engine_body(&built.record)),
                "{slug}: the roll is not the {:?} its name promises",
                entry.look
            );
        }
    }

    /// Every craft type is its own entry, exactly once; the rigged
    /// families carry a spread, every entry a distinct seed - forty-five
    /// different avatars, not one avatar forty-five times.
    #[test]
    fn every_kind_is_covered_and_no_two_entries_are_one_avatar() {
        for family in FAMILY_ORDER {
            for craft in CraftType::all_of(family) {
                let n = AVATARS
                    .iter()
                    .filter(|e| e.craft_type() == Some(*craft))
                    .count();
                assert_eq!(n, 1, "{} has {n} entries", craft.label());
            }
        }
        for (family, entries) in [
            (ChassisFamily::Humanoid, 6),
            (ChassisFamily::Beast, 6),
            (ChassisFamily::Winged, 5),
        ] {
            let n = AVATARS.iter().filter(|e| e.family() == family).count();
            assert_eq!(n, entries, "{} has {n} entries", family_group(family));
        }
        let seeds: HashSet<u64> = AVATARS.iter().filter_map(AvatarEntry::seed).collect();
        assert_eq!(seeds.len(), AVATARS.len(), "two entries share a seed");
        for entry in &AVATARS {
            assert!(
                !entry.description.trim().is_empty() && entry.description.ends_with('.'),
                "{}: a one-line description",
                entry.slug
            );
        }
    }

    /// Slugs are unique across the whole Catalogue and stable: one is
    /// a selection that outlives a build, so a rename must not move it.
    #[test]
    fn every_slug_is_unique_and_stable() {
        let slugs: Vec<&str> = AVATARS.iter().map(|e| e.slug).collect();
        let unique: HashSet<&str> = slugs.iter().copied().collect();
        assert_eq!(unique.len(), slugs.len(), "a duplicate slug");
        for slug in &slugs {
            assert!(slug.starts_with("avatar_"), "{slug}");
            assert!(
                crate::catalogue::by_slug(slug).is_none(),
                "{slug} collides with an item"
            );
            assert_eq!(avatar_by_slug(slug).map(|e| e.slug), Some(*slug));
        }
        assert!(
            crate::catalogue::ENTRIES
                .iter()
                .all(|e| !e.slug().starts_with("avatar_")),
            "an item took the avatars' prefix"
        );
        assert_eq!(
            slugs,
            [
                "avatar_person_young_woman",
                "avatar_person_young_man",
                "avatar_person_older_woman",
                "avatar_person_older_man",
                "avatar_person_heavy_set_woman",
                "avatar_person_slight_man",
                "avatar_beast_dog",
                "avatar_beast_cat",
                "avatar_beast_horse",
                "avatar_beast_dog_cat",
                "avatar_beast_cat_horse",
                "avatar_beast_horse_dog",
                "avatar_winged_owl",
                "avatar_winged_bat",
                "avatar_winged_glider",
                "avatar_winged_bird_bat",
                "avatar_winged_glider_bird",
                "avatar_boat_sloop",
                "avatar_boat_longship",
                "avatar_boat_steamtug",
                "avatar_boat_junk",
                "avatar_boat_runabout",
                "avatar_boat_scow",
                "avatar_skiff_roadster",
                "avatar_skiff_dunebuggy",
                "avatar_skiff_armouredcar",
                "avatar_skiff_cyclecar",
                "avatar_skiff_wagon",
                "avatar_skiff_rover",
                "avatar_helicopter_utility",
                "avatar_helicopter_trainer",
                "avatar_helicopter_autogyro",
                "avatar_helicopter_coaxial",
                "avatar_helicopter_tandem",
                "avatar_airplane_high_wing",
                "avatar_airplane_biplane",
                "avatar_airplane_twin",
                "avatar_airplane_racer",
                "avatar_airship_zeppelin",
                "avatar_airship_blimp",
                "avatar_airship_steam_dirigible",
                "avatar_airship_sky_galleon",
                "avatar_airship_balloon",
                "avatar_airship_hybrid",
                "avatar_airship_floater",
            ],
            "a slug moved"
        );
    }

    /// Built once and kept: a second ask is the same record, and builds
    /// nothing.
    #[test]
    fn an_entry_is_built_once() {
        let entry = avatar_by_slug("avatar_boat_scow").expect("shipped");
        let first: *const AvatarRecord = entry.record().expect("builds");
        take_build_count();
        let second: *const AvatarRecord = entry.record().expect("builds");
        assert_eq!(first, second);
        assert_eq!(take_build_count(), 0, "the second ask built again");
    }

    /// Each look's own control: a roll that is not the look is refused.
    #[test]
    fn a_look_refuses_a_body_that_is_not_it() {
        let dog = Look::Beast {
            family: Beast::Dog,
            cross: None,
        };
        let cat = crate::pds::avatar::wardrobe::engine_beast_for_seed(
            avatar_by_slug("avatar_beast_cat").unwrap().seed().unwrap(),
        );
        assert!(!dog.holds(Some(&cat)));
        assert!(!dog.holds(None), "a vehicle is no dog");
        assert!(!Look::Craft.holds(Some(&cat)), "a beast is no craft");
        let person = Look::Person {
            frame: Frame::Feminine,
            age: Age::Any,
            build: Build::Any,
        };
        assert!(!person.holds(Some(&cat)), "a beast is no person");

        // And the winged looks (#1882): a bat is no owl, an owl no bat, a
        // beast neither, and a cross led past an even blend by its second
        // family is not the cross its first family leads.
        let winged = |slug: &str| {
            crate::pds::avatar::wardrobe::engine_winged_for_seed(
                avatar_by_slug(slug).unwrap().seed().unwrap(),
            )
        };
        let (owl, bat) = (winged("avatar_winged_owl"), winged("avatar_winged_bat"));
        let bat_look = Look::Winged {
            family: Wings::Bat,
            cross: None,
        };
        assert!(Look::Owl.holds(Some(&owl)) && bat_look.holds(Some(&bat)));
        assert!(!Look::Owl.holds(Some(&bat)), "a bat is no owl");
        assert!(!bat_look.holds(Some(&owl)), "an owl is no bat");
        assert!(!Look::Owl.holds(Some(&cat)) && !bat_look.holds(Some(&cat)));
        assert!(!dog.holds(Some(&owl)), "an owl is no dog");
        let mut discless = owl.clone();
        if let symbios_avatar::Archetype::Winged(params) = &mut discless.archetype {
            params.facial_disc = -1.0;
        }
        assert!(
            !Look::Owl.holds(Some(&discless)),
            "a bird with no disc is no owl"
        );
        let cross = Look::Winged {
            family: Wings::Bird,
            cross: Some(Wings::Bat),
        };
        let mut bat_led = winged("avatar_winged_bird_bat");
        assert!(cross.holds(Some(&bat_led)));
        if let symbios_avatar::Archetype::Winged(params) = &mut bat_led.archetype {
            params.blend = 0.8;
        }
        assert!(!cross.holds(Some(&bat_led)), "a bat-led cross is no bird's");
    }

    /// **The Catalogue's winged entries are winged bodies that fly** (#1882):
    /// each is pinned to the winged family, builds a winged engine body on the
    /// preset fitted to it - its flight section with it, upright under a bird
    /// and lying under a bat or a glider - copies to a winged item, and
    /// meshes as a client builds it, at a small atlas: the shape is the same,
    /// and a full one for each entry cost the suite seconds.
    #[test]
    fn the_winged_entries_build_winged_bodies_that_fly() {
        use crate::pds::LocomotionConfig;
        use crate::pds::avatar::RiggedKind;
        let winged: Vec<&AvatarEntry> = AVATARS
            .iter()
            .filter(|e| e.slug.starts_with("avatar_winged_"))
            .collect();
        assert_eq!(winged.len(), 5, "an owl, a bat, a glider and two crosses");
        let small = symbios_avatar::AvatarConfig {
            atlas: 16,
            ..Default::default()
        };
        for entry in winged {
            let slug = entry.slug;
            assert_eq!(entry.family(), ChassisFamily::Winged, "{slug}");
            assert!(entry.is_rigged() && entry.visuals().is_none(), "{slug}");
            let built = entry.built().expect("builds");
            assert_eq!(
                ChassisFamily::for_seed(built.seed),
                ChassisFamily::Winged,
                "{slug}"
            );
            let body = engine_body(&built.record).expect("a rigged body");
            let kind = RiggedKind::of_record(body);
            assert!(kind.is_winged(), "{slug}: a {kind:?}");
            let LocomotionConfig::Humanoid(p) = &built.record.locomotion else {
                panic!("{slug}: on {}", built.record.locomotion.kind_tag());
            };
            assert!(p.flies(), "{slug} cannot fly");
            assert_eq!(
                p.is_lying(),
                kind.on_four(),
                "{slug}: a {kind:?}'s collider"
            );
            let item = built.item.as_ref().expect("copies to an item");
            assert_eq!(item.kind(), AvatarKind::Winged, "{slug}");
            assert!(
                symbios_avatar::Avatar::build_with(body, &small).is_some(),
                "{slug} does not mesh"
            );
        }
    }
}
