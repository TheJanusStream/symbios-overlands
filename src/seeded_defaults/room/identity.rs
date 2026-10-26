//! A seeded world's name and banner (#1783), from its own stream: what its
//! owner finds it already called, and flying, and may change in the World
//! Editor ([`crate::pds::WorldIdentity`]).
//!
//! The name is a place name of the world's kind. Most worlds join a
//! descriptor of their land or their theme to a place-word of their
//! landform - Frostmere, Saltholm, Brasshaven, Grimdale - as English place
//! names are made; a modern world's is two words (Maple Heights, Canyon
//! Junction), a cyberpunk one a district (Chrome Quarter), a space outpost a
//! station on a star (Vega Station), an alien world a word of no human
//! tongue (Xyrrath). The words are chosen, never generated letter by
//! letter, so no world is named something its owner would not say. A Berlin
//! square is named for the borough it lies in.
//!
//! The banner takes one light colour and one dark, so that its mark always
//! reads (the heraldic rule of tincture), in the theme's own colours where it
//! has them - neon for a cyberpunk world, a starfield's for an outpost; its
//! mark is most often one of its land's - waves by the sea, peaks in the
//! mountains, a tree in a forest, a sun in the desert.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::pds::{BannerDivision, BannerMark, Fp3, WorldBanner, WorldIdentity};
use crate::seeded_defaults::scene::{
    BiomeArchetype, LandformArchetype, SceneCharacter, ThemeArchetype, pick, pick_weighted,
    unit_f32,
};

const IDENTITY_STREAM_SALT: u64 = 0x1783_1DE7_1783_1DE7;

/// The share of compound names whose descriptor is the theme's rather than
/// the land's, and of modern names whose place-word is.
const THEME_WORD_SHARE: f32 = 0.35;

/// The share of banners whose mark is one of their land's.
const LAND_MARK_SHARE: f32 = 0.6;

/// The name and banner the world `scene` draws with seed `seed`, named for
/// `borough` where it is a square of Berlin.
pub fn derive(
    scene: &SceneCharacter,
    seed: u64,
    borough: Option<geodata::berlin::Borough>,
) -> WorldIdentity {
    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ IDENTITY_STREAM_SALT);
    // Drawn whether or not a borough names the world, so its banner is the
    // one its seed draws on its own ground too.
    let drawn = name(scene, &mut rng);
    let name = borough.map_or(drawn, |borough| borough.name().to_string());
    WorldIdentity {
        name: crate::pds::identity::clean_world_name(&name),
        banner: banner(scene, &mut rng),
    }
}

/// How a world of `theme` is named.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Style {
    /// A descriptor joined to a place-word: Frostmere.
    Compound,
    /// Two words, a descriptor and a kind of place: Maple Heights.
    Modern,
    /// A district of a neon city: Chrome Quarter.
    Neon,
    /// A station on a star: Vega Station.
    Stellar,
    /// A word of no human tongue: Xyrrath.
    Alien,
}

fn style(theme: ThemeArchetype) -> Style {
    use ThemeArchetype::*;
    match theme {
        ModernCity | Suburban | IndustrialPark | Roadside | CivicCampus | SportsRec => {
            Style::Modern
        }
        Cyberpunk => Style::Neon,
        SpaceOutpost => Style::Stellar,
        AlienOrganic | AlienMonolithic => Style::Alien,
        AncientClassical | Medieval | Nordic | FeudalJapan | Mesoamerican | RuralFarmland
        | CoastalResort | Steampunk | Solarpunk | Fantasy | GothicHorror | PostApoc | WildWest
        | Pirate => Style::Compound,
    }
}

fn name(scene: &SceneCharacter, rng: &mut ChaCha8Rng) -> String {
    match style(scene.theme) {
        Style::Compound => {
            let theme_words = compound_theme_words(scene.theme);
            let descriptor = if !theme_words.is_empty() && unit_f32(rng) < THEME_WORD_SHARE {
                pick(theme_words, rng)
            } else {
                pick(compound_land_words(scene.biome), rng)
            };
            join(descriptor, place_words(scene.landform), rng)
        }
        Style::Modern => {
            let descriptor = pick(modern_land_words(scene.biome), rng);
            let theme_kinds = modern_theme_kinds(scene.theme);
            let kinds = if !theme_kinds.is_empty() && unit_f32(rng) < THEME_WORD_SHARE {
                theme_kinds
            } else {
                modern_kinds(scene.landform)
            };
            two_words(descriptor, kinds, rng)
        }
        Style::Neon => two_words(pick(NEON_WORDS, rng), NEON_KINDS, rng),
        Style::Stellar => two_words(pick(STARS, rng), STATIONS, rng),
        Style::Alien => join(pick(ALIEN_HEADS, rng), ALIEN_TAILS, rng),
    }
}

/// `head` joined to one of `tails`, drawn: the first from the draw on that
/// neither doubles a letter across the join nor runs two vowels together
/// (Elmmere, Seaisle), which English place names wear down and these would
/// not.
fn join(head: &str, tails: &[&str], rng: &mut ChaCha8Rng) -> String {
    let start = (unit_f32(rng) * tails.len() as f32) as usize;
    let vowel = |c: char| "aeiouy".contains(c.to_ascii_lowercase());
    let last = head.chars().last().unwrap_or(' ');
    let tail = (0..tails.len())
        .map(|i| tails[(start + i) % tails.len()])
        .find(|tail| {
            let first = tail.chars().next().unwrap_or(' ');
            !last.eq_ignore_ascii_case(&first) && (!vowel(last) || !vowel(first))
        })
        .unwrap_or(tails[start.min(tails.len() - 1)]);
    format!("{head}{tail}")
}

/// `head` and one of `kinds`, drawn, as two words: the first from the draw on
/// that is not `head` itself (Mesa Mesa).
fn two_words(head: &str, kinds: &[&str], rng: &mut ChaCha8Rng) -> String {
    let start = (unit_f32(rng) * kinds.len() as f32) as usize;
    let kind = (0..kinds.len())
        .map(|i| kinds[(start + i) % kinds.len()])
        .find(|kind| *kind != head)
        .unwrap_or(kinds[start.min(kinds.len() - 1)]);
    format!("{head} {kind}")
}

fn compound_land_words(biome: BiomeArchetype) -> &'static [&'static str] {
    use BiomeArchetype::*;
    match biome {
        Lush => &["Green", "Fern", "Willow", "Clover", "Hazel", "Moss"],
        Arid => &["Dust", "Sand", "Amber", "Dry", "Thorn", "Sage"],
        Alpine => &["High", "Snow", "White", "Stone", "Eagle", "Cloud"],
        Volcanic => &["Ash", "Ember", "Cinder", "Black", "Flint", "Smoke"],
        Coastal => &["Salt", "Gull", "Tide", "Shell", "Brine", "Sea"],
        Tundra => &["Frost", "Rime", "Pale", "Wolf", "Cold", "Hare"],
        Jungle => &["Vine", "Fern", "Orchid", "Parrot", "Jade", "Rain"],
        TemperateForest => &["Oak", "Elm", "Beech", "Rowan", "Hart", "Acorn"],
        Boreal => &["Pine", "Fir", "Spruce", "Elk", "Birch", "Lynx"],
        Wetland => &["Fen", "Reed", "Heron", "Marsh", "Mire", "Rush"],
        Meadow => &["Lark", "Clover", "Honey", "Bramble", "Barley", "Daisy"],
        Savanna => &["Sun", "Gold", "Acacia", "Ochre", "Tawny", "Lion"],
        Badlands => &["Red", "Rust", "Copper", "Coyote", "Flint", "Scorch"],
        Glacial => &["Ice", "Glass", "Blue", "Winter", "Frost", "Crystal"],
    }
}

fn compound_theme_words(theme: ThemeArchetype) -> &'static [&'static str] {
    use ThemeArchetype::*;
    match theme {
        AncientClassical => &["Marble", "Laurel", "Olive", "Bronze", "Temple"],
        Medieval => &["Crown", "Raven", "Hart", "Thorn", "King"],
        Nordic => &["Wolf", "Raven", "Elk", "Storm", "Rune"],
        FeudalJapan => &["Cedar", "Plum", "Crane", "Lantern", "Mist"],
        Mesoamerican => &["Jade", "Jaguar", "Serpent", "Feather", "Maize"],
        RuralFarmland => &["Barley", "Wheat", "Hay", "Apple", "Mill"],
        CoastalResort => &["Coral", "Sunny", "Shell", "Palm", "Sea"],
        Steampunk => &["Brass", "Copper", "Cog", "Steam", "Gear"],
        Solarpunk => &["Sun", "Bloom", "Seed", "Bright", "Green"],
        Fantasy => &["Moon", "Star", "Silver", "Dream", "Elder"],
        GothicHorror => &["Grim", "Gloam", "Mourn", "Raven", "Bleak"],
        PostApoc => &["Rust", "Scrap", "Ash", "Last", "Ruin"],
        WildWest => &["Dust", "Copper", "Silver", "Bison", "Sage"],
        Pirate => &["Skull", "Gull", "Brine", "Rum", "Corsair"],
        ModernCity | Suburban | IndustrialPark | Roadside | CivicCampus | SportsRec | Cyberpunk
        | SpaceOutpost | AlienOrganic | AlienMonolithic => &[],
    }
}

fn place_words(landform: LandformArchetype) -> &'static [&'static str] {
    use LandformArchetype::*;
    match landform {
        Rolling => &["field", "down", "ley", "wold", "hurst", "worth", "ton"],
        Craggy => &["crag", "tor", "fell", "scar", "cliff", "peak"],
        Mesa => &["rock", "edge", "stead", "top", "ridge"],
        Archipelago => &["holm", "haven", "port", "isle", "strand"],
        Valleys => &["dale", "vale", "combe", "hollow", "ford", "brook", "mere"],
    }
}

fn modern_land_words(biome: BiomeArchetype) -> &'static [&'static str] {
    use BiomeArchetype::*;
    match biome {
        Lush => &["Green", "Willow", "Maple", "Brook"],
        Arid => &["Desert", "Sage", "Amber", "Mesa"],
        Alpine => &["Summit", "Alpine", "Granite", "Eagle"],
        Volcanic => &["Ember", "Cinder", "Basalt", "Obsidian"],
        Coastal => &["Harbor", "Seaside", "Bay", "Gull"],
        Tundra => &["Polar", "North", "Frost", "Caribou"],
        Jungle => &["Palm", "Orchid", "Banyan", "Monsoon"],
        TemperateForest => &["Oak", "Maple", "Elm", "Chestnut"],
        Boreal => &["Pine", "Spruce", "Cedar", "Timber"],
        Wetland => &["Bayou", "Heron", "Cypress", "Marsh"],
        Meadow => &["Meadow", "Clover", "Prairie", "Bluebell"],
        Savanna => &["Acacia", "Sunset", "Golden", "Savanna"],
        Badlands => &["Canyon", "Redrock", "Coyote", "Sandstone"],
        Glacial => &["Glacier", "Crystal", "Iceberg", "Snowcap"],
    }
}

fn modern_kinds(landform: LandformArchetype) -> &'static [&'static str] {
    use LandformArchetype::*;
    match landform {
        Rolling => &["Hills", "Park", "Heights", "Commons", "Downs"],
        Craggy => &["Ridge", "Bluffs", "Heights", "Point", "Rock"],
        Mesa => &["Flats", "Mesa", "Plateau", "Junction", "Uplands"],
        Archipelago => &["Harbor", "Landing", "Point", "Isles", "Keys"],
        Valleys => &["Valley", "Springs", "Crossing", "Hollow", "Glen"],
    }
}

fn modern_theme_kinds(theme: ThemeArchetype) -> &'static [&'static str] {
    use ThemeArchetype::*;
    match theme {
        IndustrialPark => &["Works", "Yards", "Depot"],
        Roadside => &["Junction", "Stop", "Crossing"],
        SportsRec => &["Park", "Fields", "Grounds"],
        CivicCampus => &["Commons", "Square", "Green"],
        ModernCity => &["Heights", "Square", "Plaza"],
        Suburban => &["Acres", "Estates", "Grove"],
        _ => &[],
    }
}

const NEON_WORDS: &[&str] = &[
    "Neon", "Chrome", "Static", "Circuit", "Vector", "Cobalt", "Signal", "Pixel", "Halo", "Glitch",
    "Ion", "Volt",
];
const NEON_KINDS: &[&str] = &[
    "Sprawl", "Quarter", "Grid", "Strip", "Sector", "Row", "Block", "Heights", "Stack",
];
const STARS: &[&str] = &[
    "Vega", "Lyra", "Orion", "Altair", "Rigel", "Deneb", "Sirius", "Polaris", "Tau", "Sigma",
    "Cygnus", "Aquila",
];
const STATIONS: &[&str] = &[
    "Station", "Outpost", "Base", "Landing", "Reach", "Relay", "Colony", "Point", "Prime",
];
const ALIEN_HEADS: &[&str] = &[
    "Xy", "Zo", "Vae", "Ul", "Om", "Ix", "Qua", "Thes", "Kha", "Ny", "Sa", "Ae", "Oru", "Vel",
    "Zhe", "Ish",
];
const ALIEN_TAILS: &[&str] = &[
    "rrath", "lessa", "mor", "xis", "thul", "ri", "nae", "vos", "qir", "lune", "dra", "seth",
    "oon", "vex", "kai",
];

// The heraldic tinctures, sRGB: two metals, the light colours, and the
// dark colours a mark reads against.
const OR: [f32; 3] = [0.83, 0.65, 0.18];
const ARGENT: [f32; 3] = [0.92, 0.91, 0.87];
const GULES: [f32; 3] = [0.66, 0.09, 0.1];
const AZURE: [f32; 3] = [0.1, 0.26, 0.6];
const VERT: [f32; 3] = [0.1, 0.42, 0.2];
const PURPURE: [f32; 3] = [0.42, 0.18, 0.48];
const SABLE: [f32; 3] = [0.07, 0.07, 0.08];
const TENNE: [f32; 3] = [0.6, 0.27, 0.05];
const MURREY: [f32; 3] = [0.45, 0.1, 0.2];
// A neon city's, a starfield's and an alien world's.
const CYAN: [f32; 3] = [0.15, 0.85, 0.9];
const MAGENTA: [f32; 3] = [1.0, 0.3, 0.75];
const ACID: [f32; 3] = [0.7, 0.95, 0.2];
const INDIGO: [f32; 3] = [0.12, 0.08, 0.3];
const SIGNAL: [f32; 3] = [0.95, 0.45, 0.1];
const NAVY: [f32; 3] = [0.06, 0.1, 0.25];
const DEEP_BLUE: [f32; 3] = [0.05, 0.12, 0.4];
const GLOW_TEAL: [f32; 3] = [0.3, 0.9, 0.75];
const PALE_VIOLET: [f32; 3] = [0.75, 0.6, 0.95];
const DEEP_VIOLET: [f32; 3] = [0.2, 0.08, 0.3];
const ABYSS: [f32; 3] = [0.04, 0.1, 0.12];

/// The light colours and the dark ones a banner of `style` takes one of each
/// of.
fn tinctures(style: Style) -> (&'static [[f32; 3]], &'static [[f32; 3]]) {
    match style {
        Style::Compound | Style::Modern => (
            &[OR, ARGENT],
            &[GULES, AZURE, VERT, PURPURE, SABLE, TENNE, MURREY],
        ),
        Style::Neon => (&[CYAN, MAGENTA, ACID], &[SABLE, INDIGO]),
        Style::Stellar => (&[ARGENT, SIGNAL], &[DEEP_BLUE, NAVY, SABLE]),
        Style::Alien => (&[GLOW_TEAL, PALE_VIOLET], &[DEEP_VIOLET, ABYSS]),
    }
}

/// The marks of a world's land: the sea's, the mountains', a forest's.
fn land_marks(scene: &SceneCharacter) -> &'static [BannerMark] {
    use BannerMark::*;
    use BiomeArchetype as B;
    use LandformArchetype as L;
    match (scene.landform, scene.biome) {
        (L::Archipelago, _) | (_, B::Coastal) => &[Waves, Waves, Crescent],
        (L::Craggy, _) | (_, B::Alpine | B::Glacial) => &[Peaks, Peaks, Star],
        (L::Mesa, _) | (_, B::Badlands | B::Arid | B::Savanna) => &[Sun, Sun, Peaks],
        (_, B::TemperateForest | B::Boreal | B::Jungle) => &[Tree, Tree, Roundel],
        (_, B::Wetland) => &[Waves, Crescent, Tree],
        (_, B::Volcanic) => &[Sun, Peaks],
        (_, B::Tundra) => &[Star, Crescent],
        (_, B::Lush | B::Meadow) => &[Roundel, Lozenge, Cross, Tree, Sun],
    }
}

/// The marks any banner of `style` may carry.
fn any_marks(style: Style) -> &'static [BannerMark] {
    use BannerMark::*;
    match style {
        Style::Compound | Style::Modern => {
            &[Star, Crescent, Sun, Roundel, Lozenge, Cross, Saltire, Tower]
        }
        Style::Neon => &[Lozenge, Saltire, Roundel, Star],
        Style::Stellar => &[Star, Star, Crescent, Roundel],
        Style::Alien => &[Crescent, Roundel, Lozenge, Star],
    }
}

fn banner(scene: &SceneCharacter, rng: &mut ChaCha8Rng) -> WorldBanner {
    let style = style(scene.theme);
    let (light, dark) = tinctures(style);
    let light = pick(light, rng);
    let dark = pick(dark, rng);
    // A dark field most often, the mark in its metal.
    let (field, second) = if unit_f32(rng) < 0.6 {
        (dark, light)
    } else {
        (light, dark)
    };
    let division = pick_weighted(
        &BannerDivision::ALL,
        |division| match division {
            BannerDivision::Plain => 30,
            BannerDivision::PerPale | BannerDivision::PerFess | BannerDivision::PerBend => 14,
            BannerDivision::Quarterly => 12,
            BannerDivision::PerChevron | BannerDivision::Unknown => 16,
        },
        rng,
    )
    .unwrap_or_default();
    let mark = if unit_f32(rng) < LAND_MARK_SHARE {
        pick(land_marks(scene), rng)
    } else {
        pick(any_marks(style), rng)
    };
    WorldBanner {
        field: Fp3(field).on_wire(),
        second: Fp3(second).on_wire(),
        division,
        mark,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(theme: ThemeArchetype) -> SceneCharacter {
        SceneCharacter {
            theme,
            ..SceneCharacter::for_seed(7)
        }
    }

    /// Every word joins: names are letters and at most one space, start
    /// capitalised, and fit the record's cap, whatever the draw.
    #[test]
    fn every_name_is_a_clean_place_name() {
        for seed in 0..400u64 {
            let scene = SceneCharacter::for_seed(seed);
            for theme in ThemeArchetype::ALL {
                let identity = derive(&SceneCharacter { theme, ..scene }, seed, None);
                let name = &identity.name;
                assert!(
                    name.chars().next().is_some_and(char::is_uppercase)
                        && name.chars().all(|c| c.is_ascii_alphabetic() || c == ' ')
                        && name.matches(' ').count() <= 1
                        && name.chars().count() <= crate::pds::limits::MAX_WORLD_NAME_CHARS,
                    "seed {seed} {theme:?}: {name:?}"
                );
                assert_eq!(crate::pds::identity::clean_world_name(name), *name);
            }
        }
    }

    /// No join doubles a letter or runs two vowels together (Elmmere,
    /// Seaisle): every head of every pool has a place-word in every
    /// landform's pool, and the alien tails, that it joins cleanly, so the
    /// draw never falls back on one that does not. Nor do two words repeat.
    #[test]
    fn names_avoid_awkward_joins() {
        let vowel = |c: char| "aeiouy".contains(c.to_ascii_lowercase());
        let clean = |head: &str, tail: &str| {
            let (last, first) = (head.chars().last().unwrap(), tail.chars().next().unwrap());
            !last.eq_ignore_ascii_case(&first) && (!vowel(last) || !vowel(first))
        };
        let mut heads: Vec<&str> = BiomeArchetype::ALL
            .iter()
            .flat_map(|&biome| compound_land_words(biome).iter().copied())
            .chain(
                ThemeArchetype::ALL
                    .iter()
                    .flat_map(|&theme| compound_theme_words(theme).iter().copied()),
            )
            .collect();
        heads.sort_unstable();
        heads.dedup();
        for head in &heads {
            for landform in LandformArchetype::ALL {
                assert!(
                    place_words(landform).iter().any(|tail| clean(head, tail)),
                    "{head} joins no {landform:?} place-word cleanly"
                );
            }
        }
        for head in ALIEN_HEADS {
            assert!(ALIEN_TAILS.iter().any(|tail| clean(head, tail)), "{head}");
        }
        for seed in 0..400u64 {
            for theme in ThemeArchetype::ALL {
                let name = derive(&scene(theme), seed, None).name;
                if let [a, b] = name.split(' ').collect::<Vec<_>>()[..] {
                    assert_ne!(a, b, "{name}");
                }
            }
        }
    }

    /// Names vary: over 300 seeds no name stands for more than a few worlds.
    #[test]
    fn names_vary_across_worlds() {
        let mut seen = std::collections::HashMap::<String, usize>::new();
        for seed in 0..300u64 {
            let scene = SceneCharacter::for_seed(seed);
            *seen.entry(derive(&scene, seed, None).name).or_default() += 1;
        }
        let most = seen.values().copied().max().unwrap_or(0);
        assert!(
            seen.len() >= 240 && most <= 3,
            "{} names, most {most}",
            seen.len()
        );
    }

    #[test]
    fn a_berlin_square_is_named_for_its_borough() {
        use geodata::berlin::Borough;
        let identity = derive(
            &SceneCharacter::even(3),
            3,
            Some(Borough::TempelhofSchoeneberg),
        );
        assert_eq!(identity.name, "Tempelhof-Sch\u{f6}neberg");
        // Its banner is the one its seed draws on its own ground.
        assert_eq!(
            identity.banner,
            derive(&SceneCharacter::even(3), 3, None).banner
        );
    }

    /// A banner's two colours are one light and one dark, so its mark
    /// reads on its field (the rule of tincture).
    #[test]
    fn a_banner_mark_always_reads() {
        let luma = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        for seed in 0..300u64 {
            for theme in ThemeArchetype::ALL {
                let banner = derive(&scene(theme), seed, None).banner;
                let gap = (luma(banner.field.0) - luma(banner.second.0)).abs();
                assert!(gap > 0.3, "seed {seed} {theme:?}: {banner:?}");
            }
        }
    }

    /// The land's marks are its own: a seafront world flies the sea more
    /// often than not.
    #[test]
    fn a_banner_flies_its_land() {
        let mut sea = 0;
        let mut coast = 0;
        for seed in 0..2_000u64 {
            let scene = SceneCharacter::for_seed(seed);
            if scene.landform != LandformArchetype::Archipelago {
                continue;
            }
            coast += 1;
            let mark = derive(&scene, seed, None).banner.mark;
            if matches!(mark, BannerMark::Waves | BannerMark::Crescent) {
                sea += 1;
            }
        }
        assert!(coast > 100 && sea * 2 > coast, "{sea} of {coast}");
    }

    /// The identity a world derives is its saved self: a name the sanitiser
    /// keeps, colours on the wire's grid.
    #[test]
    fn a_derived_identity_is_its_saved_self() {
        for seed in 0..200u64 {
            let identity = derive(&SceneCharacter::for_seed(seed), seed, None);
            let mut saved: WorldIdentity =
                serde_json::from_str(&serde_json::to_string(&identity).unwrap()).unwrap();
            saved.sanitize();
            assert_eq!(saved, identity, "seed {seed}");
        }
    }
}
