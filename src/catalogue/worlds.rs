//! Worlds in the Catalogue (#1841, epic #1837): ready-made worlds of every
//! kind, which an owner can load into their own world or copy to their
//! Inventory as a world item.
//!
//! An entry is a **recipe**, not a stored record, as an avatar entry is
//! ([`crate::catalogue::avatars`]). The World Editor's pinned re-roll
//! already builds every kind of world from a seed: it hunts the first seed
//! at or after a start whose scene and source draw hold the pinned
//! landform, biome, theme, ground and borough ([`ScenePins`], through
//! [`SeedPins`]), and builds the world from it
//! ([`build_room_with_source`], on the square
//! [`RegionSource::for_seed`] draws). So an entry is a slug, a name, a
//! one-line description, its pins and a fixed start seed, and building it
//! is that hunt and then that builder, for the signed-in owner's DID - the
//! world's name, banner and monument follow the owner, as a re-roll's do.
//! Nothing here draws a world of its own.
//!
//! **Never built on selection, and never cached.** A build is the whole
//! procedural pipeline - the re-roll's frame stall, some 15 to 30 ms
//! natively under the test profile and more in a browser - and a built
//! world is up to a few hundred kilobytes of record, so the detail panel's
//! text card reads the entry's pins alone, and [`WorldEntry::build`] runs
//! only on a Load or a Copy click, once per click. Each start is the seed
//! its own hunt finds, so the hunt a click runs is one trial.
//!
//! **Coverage.** One procedural entry per [`ThemeArchetype`], each on a
//! landform and a biome its theme suits ([`theme_affinity`]): together they
//! show every [`LandformArchetype`] and every [`BiomeArchetype`]. And one
//! Berlin entry per [`Borough`], on a square of real Berlin in that borough,
//! dressed in a theme a city suits.
//!
//! **Seeds may draw differently in a later version** - cross-version
//! determinism is not a constraint on the seeded builders. The test
//! `every_entry_draws_the_world_it_is_named_for` is what catches an entry
//! that drifted: each must find its seed inside [`HUNT_BUDGET`] and draw
//! the theme, landform and biome - or for Berlin the square and borough -
//! it is named for.

use geodata::berlin::Borough;

use crate::pds::RoomRecord;
use crate::seeded_defaults::room::build::build_room_with_source;
use crate::seeded_defaults::scene::theme_affinity;
use crate::seeded_defaults::{
    BiomeArchetype, LandformArchetype, RegionSource, ScenePins, SeedPins, SourceKind,
    ThemeArchetype,
};

/// How many trials past its start an entry's hunt may take. The pinned
/// re-roll's own cap is two million trials, a safety net; an entry's hunt
/// runs on the UI thread on a click, so its start is chosen to land at
/// once, and this is the bound the drift test holds it to.
pub const HUNT_BUDGET: u64 = 4_096;

/// The Catalogue section the worlds list under.
pub const CATEGORY: &str = "Worlds";

/// The group a procedural entry lists under.
pub const PROCEDURAL_GROUP: &str = "Procedural";

/// The group a Berlin entry lists under.
pub const BERLIN_GROUP: &str = "Berlin";

/// The order the browse tree lists the groups in.
pub const GROUP_ORDER: [&str; 2] = [PROCEDURAL_GROUP, BERLIN_GROUP];

/// The ground a world entry stands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ground {
    /// Its own terrain, of this landform and biome.
    Procedural {
        landform: LandformArchetype,
        biome: BiomeArchetype,
    },
    /// A square of real Berlin whose middle lies in this borough.
    Berlin(Borough),
}

/// One world the Catalogue offers.
pub struct WorldEntry {
    /// Stable identifier, unique across the whole Catalogue - every slug
    /// starts `worlds_`, which no item's or avatar's does.
    pub slug: &'static str,
    /// The name the Catalogue lists it under.
    pub name: &'static str,
    /// What it is, in one line.
    pub description: &'static str,
    /// The theme its settlement - or for Berlin, its buildings - is
    /// dressed in.
    pub theme: ThemeArchetype,
    /// The ground it stands on.
    pub ground: Ground,
    /// Where the hunt starts.
    pub start: u64,
}

impl WorldEntry {
    const fn procedural(
        slug: &'static str,
        name: &'static str,
        description: &'static str,
        theme: ThemeArchetype,
        (landform, biome): (LandformArchetype, BiomeArchetype),
        start: u64,
    ) -> Self {
        Self {
            slug,
            name,
            description,
            theme,
            ground: Ground::Procedural { landform, biome },
            start,
        }
    }

    const fn berlin(
        slug: &'static str,
        name: &'static str,
        description: &'static str,
        borough: Borough,
        theme: ThemeArchetype,
        start: u64,
    ) -> Self {
        Self {
            slug,
            name,
            description,
            theme,
            ground: Ground::Berlin(borough),
            start,
        }
    }

    /// What the hunt pins: the theme always; the ground procedural with its
    /// landform and biome, or Berlin with its borough.
    pub fn pins(&self) -> ScenePins {
        match self.ground {
            Ground::Procedural { landform, biome } => ScenePins {
                landform: Some(landform),
                biome: Some(biome),
                theme: Some(self.theme),
                source: Some(SourceKind::Procedural),
                ..ScenePins::default()
            },
            Ground::Berlin(borough) => ScenePins {
                theme: Some(self.theme),
                source: Some(SourceKind::Berlin),
                borough: Some(borough),
                ..ScenePins::default()
            },
        }
    }

    /// The seed the entry builds from: the first at or after its start
    /// that draws its pins.
    pub fn seed(&self) -> Option<u64> {
        SeedPins::find_seed(&self.pins(), self.start)
    }

    /// The world this entry builds for the owner `did`, with the seed it
    /// was built from: what the World Editor's re-roll builds from that
    /// seed, on the ground the seed draws. The whole procedural pipeline,
    /// so only ever on a click. `None` only when the hunt fails, which the
    /// drift test forbids.
    pub fn build(&self, did: &str) -> Option<(u64, RoomRecord)> {
        #[cfg(test)]
        BUILDS.with(|n| n.set(n.get() + 1));
        let seed = self.seed()?;
        let berlin = RegionSource::for_seed(seed).berlin_square();
        Some((seed, build_room_with_source(seed, did, berlin)))
    }

    /// The group the entry lists under: [`PROCEDURAL_GROUP`] or
    /// [`BERLIN_GROUP`].
    pub fn group(&self) -> &'static str {
        match self.ground {
            Ground::Procedural { .. } => PROCEDURAL_GROUP,
            Ground::Berlin(_) => BERLIN_GROUP,
        }
    }

    /// What kind of world it is, for the detail card.
    pub fn kind_label(&self) -> &'static str {
        match self.ground {
            Ground::Procedural { .. } => "A procedural world, on terrain of its own",
            Ground::Berlin(_) => "A square kilometre of real Berlin",
        }
    }

    /// The borough a Berlin entry stands in.
    pub fn borough(&self) -> Option<Borough> {
        match self.ground {
            Ground::Berlin(borough) => Some(borough),
            Ground::Procedural { .. } => None,
        }
    }

    /// Whether a search query (already lower-cased) finds this entry: its
    /// name, its slug, its theme, its landform, its biome, its borough, its
    /// group, or the word "world". A borough is found with or without its
    /// umlaut - "neukolln" finds Neuk\u{f6}lln.
    pub fn matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let q = fold(query);
        let has = |text: &str| fold(text).contains(&q);
        has(self.name)
            || self.slug.contains(query)
            || has(self.theme.label())
            || has(self.group())
            || has(CATEGORY)
            || match self.ground {
                Ground::Procedural { landform, biome } => {
                    has(landform.label()) || has(biome.label())
                }
                Ground::Berlin(borough) => has(borough.name()),
            }
    }
}

/// `text` lower-cased with the umlauts and the sharp s the borough names
/// carry spelled as plain letters, so a search typed without them finds
/// them.
fn fold(text: &str) -> String {
    text.to_lowercase()
        .replace('\u{e4}', "a")
        .replace('\u{f6}', "o")
        .replace('\u{fc}', "u")
        .replace('\u{df}', "ss")
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

use BiomeArchetype as B;
use LandformArchetype as L;
use ThemeArchetype as T;

/// Every world the Catalogue offers, in browse order: the procedural
/// worlds in theme order, then Berlin's boroughs in code order.
pub static WORLDS: [WorldEntry; 36] = [
    // ---- Procedural: one per theme, on land it suits ----
    WorldEntry::procedural(
        "worlds_ancient_classical",
        "Ancient Classical world",
        "Temples, villas and an observatory on rolling hills above the sea.",
        T::AncientClassical,
        (L::Rolling, B::Coastal),
        1_841_425,
    ),
    WorldEntry::procedural(
        "worlds_medieval",
        "Medieval world",
        "Castles, keeps and chapels on crags among broadleaf woods.",
        T::Medieval,
        (L::Craggy, B::TemperateForest),
        1_841_057,
    ),
    WorldEntry::procedural(
        "worlds_nordic",
        "Nordic world",
        "Mead halls and rune stones on crags in the dark northern pines.",
        T::Nordic,
        (L::Craggy, B::Boreal),
        1_841_299,
    ),
    WorldEntry::procedural(
        "worlds_feudal_japan",
        "Feudal Japan world",
        "Pagodas, tea houses and torii gates down green river valleys.",
        T::FeudalJapan,
        (L::Valleys, B::Lush),
        1_842_146,
    ),
    WorldEntry::procedural(
        "worlds_mesoamerican",
        "Mesoamerican world",
        "Step pyramids, ball courts and stelae in rolling jungle.",
        T::Mesoamerican,
        (L::Rolling, B::Jungle),
        1_842_795,
    ),
    WorldEntry::procedural(
        "worlds_modern_city",
        "Modern City world",
        "Glass towers and transit stops along a river valley by the sea.",
        T::ModernCity,
        (L::Valleys, B::Coastal),
        1_841_990,
    ),
    WorldEntry::procedural(
        "worlds_suburban",
        "Suburban world",
        "Houses, garages and corner stores on gentle meadow hills.",
        T::Suburban,
        (L::Rolling, B::Meadow),
        1_841_941,
    ),
    WorldEntry::procedural(
        "worlds_rural_farmland",
        "Rural Farmland world",
        "Barns, silos and windmills down wide meadow valleys.",
        T::RuralFarmland,
        (L::Valleys, B::Meadow),
        1_841_033,
    ),
    WorldEntry::procedural(
        "worlds_industrial_park",
        "Industrial Park world",
        "Warehouses, cooling towers and tank farms by the marshes of a river valley.",
        T::IndustrialPark,
        (L::Valleys, B::Wetland),
        1_841_813,
    ),
    WorldEntry::procedural(
        "worlds_coastal_resort",
        "Coastal Resort world",
        "Hotels, piers and boardwalk shops on a chain of sandy islands.",
        T::CoastalResort,
        (L::Archipelago, B::Coastal),
        1_841_600,
    ),
    WorldEntry::procedural(
        "worlds_roadside",
        "Roadside world",
        "Gas stations, diners and motels across rolling golden grassland.",
        T::Roadside,
        (L::Rolling, B::Savanna),
        1_843_915,
    ),
    WorldEntry::procedural(
        "worlds_civic_campus",
        "Civic Campus world",
        "Town halls, libraries and clock towers among wooded hills.",
        T::CivicCampus,
        (L::Rolling, B::TemperateForest),
        1_841_207,
    ),
    WorldEntry::procedural(
        "worlds_sports_rec",
        "Sports & Rec world",
        "Stadiums, bleachers and playing fields in a snowy mountain valley.",
        T::SportsRec,
        (L::Valleys, B::Alpine),
        1_841_621,
    ),
    WorldEntry::procedural(
        "worlds_cyberpunk",
        "Cyberpunk world",
        "Neon megatowers and data spires rising from foggy, drowned islands.",
        T::Cyberpunk,
        (L::Archipelago, B::Wetland),
        1_841_633,
    ),
    WorldEntry::procedural(
        "worlds_steampunk",
        "Steampunk world",
        "Cog towers, foundries and airship docks on high alpine crags.",
        T::Steampunk,
        (L::Craggy, B::Alpine),
        1_843_016,
    ),
    WorldEntry::procedural(
        "worlds_solarpunk",
        "Solarpunk world",
        "Biodomes, wind turbines and vertical farms on green rolling hills.",
        T::Solarpunk,
        (L::Rolling, B::Lush),
        1_846_635,
    ),
    WorldEntry::procedural(
        "worlds_space_outpost",
        "Space Outpost world",
        "Habitat domes, comms dishes and landing pads on red mesas.",
        T::SpaceOutpost,
        (L::Mesa, B::Badlands),
        1_843_155,
    ),
    WorldEntry::procedural(
        "worlds_fantasy",
        "Fantasy world",
        "Wizard towers, fae rings and crystal shrines on green islands.",
        T::Fantasy,
        (L::Archipelago, B::Lush),
        1_844_271,
    ),
    WorldEntry::procedural(
        "worlds_gothic_horror",
        "Gothic Horror world",
        "Cathedrals, mausoleums and bell towers on crags above a foggy marsh.",
        T::GothicHorror,
        (L::Craggy, B::Wetland),
        1_841_201,
    ),
    WorldEntry::procedural(
        "worlds_alien_organic",
        "Alien Organic world",
        "Chitinous hives, pods and fleshy spires in volcanic valleys.",
        T::AlienOrganic,
        (L::Valleys, B::Volcanic),
        1_841_785,
    ),
    WorldEntry::procedural(
        "worlds_alien_monolithic",
        "Alien Monolithic world",
        "Black monoliths, levitating platforms and glyph arches on mesas of blue ice.",
        T::AlienMonolithic,
        (L::Mesa, B::Glacial),
        1_841_658,
    ),
    WorldEntry::procedural(
        "worlds_post_apoc",
        "Post-Apocalyptic world",
        "Fortified ruins, scrap shanties and radio masts on cold, bare mesas.",
        T::PostApoc,
        (L::Mesa, B::Tundra),
        1_841_039,
    ),
    WorldEntry::procedural(
        "worlds_wild_west",
        "Wild West world",
        "Saloons, water towers and general stores among desert mesas.",
        T::WildWest,
        (L::Mesa, B::Arid),
        1_842_683,
    ),
    WorldEntry::procedural(
        "worlds_pirate",
        "Pirate world",
        "A buccaneer port of harbour batteries and rum warehouses on jungle islands.",
        T::Pirate,
        (L::Archipelago, B::Jungle),
        1_842_960,
    ),
    // ---- Berlin: one per borough, dressed as a city ----
    WorldEntry::berlin(
        "worlds_berlin_mitte",
        "Berlin: Mitte",
        "A square kilometre of real Berlin in Mitte, its buildings dressed as a modern city.",
        Borough::Mitte,
        T::ModernCity,
        1_841_422,
    ),
    WorldEntry::berlin(
        "worlds_berlin_friedrichshain_kreuzberg",
        "Berlin: Friedrichshain-Kreuzberg",
        "A square kilometre of real Berlin in Friedrichshain-Kreuzberg, dressed as a modern city.",
        Borough::FriedrichshainKreuzberg,
        T::ModernCity,
        1_847_614,
    ),
    WorldEntry::berlin(
        "worlds_berlin_pankow",
        "Berlin: Pankow",
        "A square kilometre of real Berlin in Pankow, dressed as a modern city.",
        Borough::Pankow,
        T::ModernCity,
        1_841_400,
    ),
    WorldEntry::berlin(
        "worlds_berlin_charlottenburg_wilmersdorf",
        "Berlin: Charlottenburg-Wilmersdorf",
        "A square kilometre of real Berlin in Charlottenburg-Wilmersdorf, dressed as a civic campus.",
        Borough::CharlottenburgWilmersdorf,
        T::CivicCampus,
        1_842_938,
    ),
    WorldEntry::berlin(
        "worlds_berlin_spandau",
        "Berlin: Spandau",
        "A square kilometre of real Berlin in Spandau, dressed as a suburb.",
        Borough::Spandau,
        T::Suburban,
        1_841_659,
    ),
    WorldEntry::berlin(
        "worlds_berlin_steglitz_zehlendorf",
        "Berlin: Steglitz-Zehlendorf",
        "A square kilometre of real Berlin in Steglitz-Zehlendorf, dressed as a suburb.",
        Borough::SteglitzZehlendorf,
        T::Suburban,
        1_842_361,
    ),
    WorldEntry::berlin(
        "worlds_berlin_tempelhof_schoeneberg",
        "Berlin: Tempelhof-Sch\u{f6}neberg",
        "A square kilometre of real Berlin in Tempelhof-Sch\u{f6}neberg, dressed as a modern city.",
        Borough::TempelhofSchoeneberg,
        T::ModernCity,
        1_846_424,
    ),
    WorldEntry::berlin(
        "worlds_berlin_neukoelln",
        "Berlin: Neuk\u{f6}lln",
        "A square kilometre of real Berlin in Neuk\u{f6}lln, dressed as a modern city.",
        Borough::Neukoelln,
        T::ModernCity,
        1_841_276,
    ),
    WorldEntry::berlin(
        "worlds_berlin_treptow_koepenick",
        "Berlin: Treptow-K\u{f6}penick",
        "A square kilometre of real Berlin in Treptow-K\u{f6}penick, dressed as a suburb.",
        Borough::TreptowKoepenick,
        T::Suburban,
        1_841_002,
    ),
    WorldEntry::berlin(
        "worlds_berlin_marzahn_hellersdorf",
        "Berlin: Marzahn-Hellersdorf",
        "A square kilometre of real Berlin in Marzahn-Hellersdorf, dressed as a modern city.",
        Borough::MarzahnHellersdorf,
        T::ModernCity,
        1_842_968,
    ),
    WorldEntry::berlin(
        "worlds_berlin_lichtenberg",
        "Berlin: Lichtenberg",
        "A square kilometre of real Berlin in Lichtenberg, dressed as an industrial park.",
        Borough::Lichtenberg,
        T::IndustrialPark,
        1_841_327,
    ),
    WorldEntry::berlin(
        "worlds_berlin_reinickendorf",
        "Berlin: Reinickendorf",
        "A square kilometre of real Berlin in Reinickendorf, dressed as a suburb.",
        Borough::Reinickendorf,
        T::Suburban,
        1_841_180,
    ),
];

/// The world entry with this slug.
pub fn world_by_slug(slug: &str) -> Option<&'static WorldEntry> {
    WORLDS.iter().find(|entry| entry.slug == slug)
}

/// Whether `theme` suits a world of `ground` - for a procedural entry, the
/// seeded draw's own [`theme_affinity`]; a city suits every Berlin theme
/// the Catalogue dresses one in.
pub fn theme_suits(theme: ThemeArchetype, ground: Ground) -> bool {
    match ground {
        Ground::Procedural { landform, biome } => theme_affinity(theme, landform, biome) > 0,
        Ground::Berlin(_) => matches!(
            theme,
            T::ModernCity | T::Suburban | T::IndustrialPark | T::CivicCampus
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeded_defaults::SceneCharacter;
    use std::collections::HashSet;

    const DID: &str = "did:plc:catalogue-worlds";

    /// The drift test (#1841): every entry finds its seed inside its
    /// budget; the seed's scene is the theme it is named for, on the
    /// landform and biome its theme suits, or for Berlin the seed draws a
    /// Berlin square in its borough; and the world built from it stands on
    /// that ground. Seeds may draw differently in a later version; this is
    /// what says which entry moved. Every entry is built: a build costs
    /// about 15 to 30 ms under the test profile, so the whole set is about
    /// a second.
    #[test]
    fn every_entry_draws_the_world_it_is_named_for() {
        for entry in &WORLDS {
            let slug = entry.slug;
            let seed = entry
                .seed()
                .unwrap_or_else(|| panic!("{slug}: the hunt found no seed"));
            assert!(
                seed.wrapping_sub(entry.start) < HUNT_BUDGET,
                "{slug}: found {seed}, {} trials past its start",
                seed.wrapping_sub(entry.start)
            );
            // The module's promise that a click hunts ONE trial on the UI
            // thread (critic of #1841): a drifted entry would otherwise hunt
            // up to the budget per click and this test stay green.
            assert_eq!(
                seed, entry.start,
                "{slug}: its start no longer draws it - set the start to {seed}"
            );
            let scene = SceneCharacter::for_seed(seed);
            assert_eq!(scene.theme, entry.theme, "{slug}");
            assert!(theme_suits(entry.theme, entry.ground), "{slug}");
            let source = RegionSource::for_seed(seed);
            let (_, world) = entry.build(DID).expect("builds");
            assert_eq!(
                serde_json::to_value(&world).expect("serializes"),
                serde_json::to_value(crate::seeded_defaults::room::build::build_room(seed, DID))
                    .expect("serializes"),
                "{slug}: the seed's own seeded world, on the ground the seed draws (what a \
                 re-roll with the square unlocked builds; a locked square is the re-roll's \
                 own choice and no Catalogue entry's)"
            );
            match entry.ground {
                Ground::Procedural { landform, biome } => {
                    assert_eq!(source.kind, SourceKind::Procedural, "{slug}");
                    assert_eq!(scene.landform, landform, "{slug}");
                    assert_eq!(scene.biome, biome, "{slug}");
                    assert_eq!(world.geo_source, None, "{slug}: its own terrain");
                    assert_eq!(entry.name, format!("{} world", entry.theme.label()));
                }
                Ground::Berlin(borough) => {
                    assert_eq!(source.kind, SourceKind::Berlin, "{slug}");
                    assert_eq!(source.borough(), Some(borough), "{slug}");
                    let square = world
                        .geo_source
                        .as_ref()
                        .and_then(crate::pds::GeoSource::berlin_square)
                        .unwrap_or_else(|| panic!("{slug}: a Berlin square"));
                    let (e, n) = square.centre();
                    assert_eq!(
                        geodata::berlin::Coverage::berlin().borough_at(e, n),
                        Some(borough),
                        "{slug}: the square stands in its borough"
                    );
                    assert_eq!(entry.name, format!("Berlin: {}", borough.name()));
                }
            }
        }
    }

    /// Every theme is one procedural entry, every borough one Berlin entry;
    /// the procedural entries stand on every landform and every biome;
    /// every entry is its own seed - thirty-six different worlds, not one
    /// world thirty-six times.
    #[test]
    fn every_kind_is_covered_and_no_two_entries_are_one_world() {
        for theme in ThemeArchetype::ALL {
            let n = WORLDS
                .iter()
                .filter(|e| e.theme == theme && e.group() == PROCEDURAL_GROUP)
                .count();
            assert_eq!(n, 1, "{} has {n} procedural entries", theme.label());
        }
        for borough in Borough::ALL {
            let n = WORLDS
                .iter()
                .filter(|e| e.borough() == Some(borough))
                .count();
            assert_eq!(n, 1, "{} has {n} entries", borough.name());
        }
        let grounds = || {
            WORLDS.iter().filter_map(|e| match e.ground {
                Ground::Procedural { landform, biome } => Some((landform, biome)),
                Ground::Berlin(_) => None,
            })
        };
        for landform in LandformArchetype::ALL {
            assert!(
                grounds().any(|(l, _)| l == landform),
                "no entry on {}",
                landform.label()
            );
        }
        for biome in BiomeArchetype::ALL {
            assert!(
                grounds().any(|(_, b)| b == biome),
                "no entry in {}",
                biome.label()
            );
        }
        assert_eq!(WORLDS.len(), ThemeArchetype::ALL.len() + Borough::ALL.len());
        let seeds: HashSet<u64> = WORLDS.iter().filter_map(WorldEntry::seed).collect();
        assert_eq!(seeds.len(), WORLDS.len(), "two entries share a seed");
        for entry in &WORLDS {
            assert!(
                !entry.description.trim().is_empty() && entry.description.ends_with('.'),
                "{}: a one-line description",
                entry.slug
            );
        }
    }

    /// Slugs are unique across the whole Catalogue and stable: one is a
    /// selection that outlives a build, so a rename must not move it.
    #[test]
    fn every_slug_is_unique_and_stable() {
        let slugs: Vec<&str> = WORLDS.iter().map(|e| e.slug).collect();
        let unique: HashSet<&str> = slugs.iter().copied().collect();
        assert_eq!(unique.len(), slugs.len(), "a duplicate slug");
        for slug in &slugs {
            assert!(slug.starts_with("worlds_"), "{slug}");
            assert!(
                crate::catalogue::by_slug(slug).is_none(),
                "{slug} collides with an item"
            );
            assert!(
                crate::catalogue::avatars::avatar_by_slug(slug).is_none(),
                "{slug} collides with an avatar"
            );
            assert_eq!(world_by_slug(slug).map(|e| e.slug), Some(*slug));
        }
        assert!(
            crate::catalogue::ENTRIES
                .iter()
                .all(|e| !e.slug().starts_with("worlds_")),
            "an item took the worlds' prefix"
        );
        assert!(
            crate::catalogue::avatars::AVATARS
                .iter()
                .all(|e| !e.slug.starts_with("worlds_")),
            "an avatar took the worlds' prefix"
        );
        assert_eq!(
            slugs,
            [
                "worlds_ancient_classical",
                "worlds_medieval",
                "worlds_nordic",
                "worlds_feudal_japan",
                "worlds_mesoamerican",
                "worlds_modern_city",
                "worlds_suburban",
                "worlds_rural_farmland",
                "worlds_industrial_park",
                "worlds_coastal_resort",
                "worlds_roadside",
                "worlds_civic_campus",
                "worlds_sports_rec",
                "worlds_cyberpunk",
                "worlds_steampunk",
                "worlds_solarpunk",
                "worlds_space_outpost",
                "worlds_fantasy",
                "worlds_gothic_horror",
                "worlds_alien_organic",
                "worlds_alien_monolithic",
                "worlds_post_apoc",
                "worlds_wild_west",
                "worlds_pirate",
                "worlds_berlin_mitte",
                "worlds_berlin_friedrichshain_kreuzberg",
                "worlds_berlin_pankow",
                "worlds_berlin_charlottenburg_wilmersdorf",
                "worlds_berlin_spandau",
                "worlds_berlin_steglitz_zehlendorf",
                "worlds_berlin_tempelhof_schoeneberg",
                "worlds_berlin_neukoelln",
                "worlds_berlin_treptow_koepenick",
                "worlds_berlin_marzahn_hellersdorf",
                "worlds_berlin_lichtenberg",
                "worlds_berlin_reinickendorf",
            ],
            "a slug moved"
        );
    }

    /// Each pin's own control: the drift test's checks fail for an entry
    /// whose pins drew some other world.
    #[test]
    fn a_seed_that_draws_another_world_is_refused() {
        let medieval = world_by_slug("worlds_medieval").unwrap();
        let nordic = world_by_slug("worlds_nordic").unwrap();
        let scene = SceneCharacter::for_seed(nordic.seed().unwrap());
        assert!(
            !medieval.pins().matches(&scene),
            "a Nordic seed is not Medieval"
        );
        let mitte = world_by_slug("worlds_berlin_mitte").unwrap();
        let neukoelln = world_by_slug("worlds_berlin_neukoelln").unwrap();
        assert!(!mitte.pins().source_matches(neukoelln.seed().unwrap()));
        assert!(
            !medieval.pins().source_matches(mitte.seed().unwrap()),
            "a Berlin seed is not procedural ground"
        );
        assert!(!theme_suits(ThemeArchetype::Pirate, medieval.ground));
        assert!(!theme_suits(ThemeArchetype::Fantasy, mitte.ground));
    }

    /// Search finds a world by its name, theme, landform, biome, borough -
    /// with or without the umlaut - and its section's words.
    #[test]
    fn search_finds_a_world_by_what_it_is() {
        let neukoelln = world_by_slug("worlds_berlin_neukoelln").unwrap();
        for query in [
            "neuk\u{f6}lln",
            "neukolln",
            "neukoelln",
            "berlin",
            "modern",
            "world",
        ] {
            assert!(neukoelln.matches(query), "{query}");
        }
        let cyberpunk = world_by_slug("worlds_cyberpunk").unwrap();
        for query in [
            "cyberpunk",
            "archipelago",
            "wetland",
            "procedural",
            "worlds",
            "",
        ] {
            assert!(cyberpunk.matches(query), "{query}");
        }
        assert!(!cyberpunk.matches("berlin"));
        assert!(!neukoelln.matches("archipelago"), "Berlin has no landform");
        assert!(!cyberpunk.matches("avatar"));
    }

    /// The build count counts a build, and only a build: asking for the
    /// pins, the seed or a search builds nothing.
    #[test]
    fn only_a_build_counts() {
        let entry = world_by_slug("worlds_pirate").unwrap();
        take_build_count();
        let _ = (entry.pins(), entry.seed(), entry.matches("pirate"));
        assert_eq!(take_build_count(), 0);
        let _ = entry.build(DID);
        assert_eq!(take_build_count(), 1);
    }
}
