//! Scene-character anchor: the per-room seed-derived tuple that every
//! downstream room deriver reads to coordinate its output.
//!
//! Sampling colours, terrain, water, etc. all independently from the
//! room seed gives clashing combinations (verdant grass + arid sky +
//! alpine water). Sampling them from a shared [`SceneCharacter`]
//! produces coherent rooms ("warm tundra at dawn") because each
//! downstream deriver biases its samples around the same anchor.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use super::hash::fnv1a_64;

/// Discrete landform family. Picked first; continuous terrain knobs
/// (algorithm, erosion intensity, height scale) then sample within
/// archetype-appropriate ranges so "rolling hills with crazy erosion"
/// or "flat archipelago with mesa terraces" never occur.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LandformArchetype {
    /// Smooth hills, low amplitude, light erosion.
    Rolling,
    /// Sharp peaks, high amplitude, heavy thermal erosion.
    Craggy,
    /// Voronoi-terraced flat-tops with sheer cliff edges.
    Mesa,
    /// Water-dominant with scattered island peaks.
    Archipelago,
    /// Heavily-eroded river valleys cut into hilly terrain.
    Valleys,
}

impl LandformArchetype {
    pub const ALL: [Self; 5] = [
        Self::Rolling,
        Self::Craggy,
        Self::Mesa,
        Self::Archipelago,
        Self::Valleys,
    ];
}

/// Discrete biome family. Drives palette anchors and biome thresholds
/// (snow line, vegetation, water hue) toward archetype-appropriate
/// regions of colour space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BiomeArchetype {
    /// Deep greens, brown soil, abundant water.
    Lush,
    /// Browns, ochres, sparse vegetation, low water.
    Arid,
    /// High snow line, cool greys, sharp contrast.
    Alpine,
    /// Dark, reddish, dramatic — volcanic blacks and lava reds.
    Volcanic,
    /// Sandy/warm, water-dominant, mid-altitude.
    Coastal,
    /// Pale blues and whites, low chroma everywhere.
    Tundra,
}

impl BiomeArchetype {
    pub const ALL: [Self; 6] = [
        Self::Lush,
        Self::Arid,
        Self::Alpine,
        Self::Volcanic,
        Self::Coastal,
        Self::Tundra,
    ];
}

/// Discrete theme family — the *artificial* axis, parallel and fully
/// orthogonal to [`BiomeArchetype`] (the natural axis). Drives which
/// themed mini-settlement of catalogue structures a room grows (a
/// landmark plus secondary buildings and scatter props) and, optionally,
/// a light accent the theme nudges back onto the natural derivers (fog
/// tint, ambient audio, particle mood).
///
/// Picked uniformly per room and independently of biome, so surreal
/// collisions — a cyberpunk volcano, a medieval glacier — are intentional.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeArchetype {
    // --- Historical ---
    /// Greco-Roman / bronze-age: temples, villas, observatories.
    AncientClassical,
    /// Castles, keeps, chapels, market stalls.
    Medieval,
    /// Norse: mead halls, rune stones, longships.
    Nordic,
    /// Pagodas, torii gates, tea houses, stone lanterns.
    FeudalJapan,
    /// Step pyramids, ball courts, stelae.
    Mesoamerican,
    // --- Contemporary / realistic ---
    /// Glass skyscrapers, transit stops, street furniture.
    ModernCity,
    /// Houses, garages, corner stores, fences.
    Suburban,
    /// Barns, silos, greenhouses, windmills.
    RuralFarmland,
    /// Warehouses, cooling towers, tank farms.
    IndustrialPark,
    /// Hotels, piers, boardwalk shops, lifeguard towers.
    CoastalResort,
    /// Gas stations, diners, motels, billboards.
    Roadside,
    /// Town halls, libraries, lecture halls, clock towers.
    CivicCampus,
    /// Stadiums, gyms, bleachers, scoreboards.
    SportsRec,
    // --- Speculative / future ---
    /// Neon megatowers, holo-signage, data spires.
    Cyberpunk,
    /// Cog towers, airship docks, foundries, pipework.
    Steampunk,
    /// Biodomes, wind turbines, vertical farms.
    Solarpunk,
    /// Habitat domes, comms dishes, landing pads.
    SpaceOutpost,
    // --- Fantastical ---
    /// Wizard towers, fae rings, crystal shrines.
    Fantasy,
    /// Cathedrals, mausoleums, cemeteries, bell towers.
    GothicHorror,
    /// Chitinous hives, pods, fleshy spires.
    AlienOrganic,
    /// Black monoliths, levitating platforms, glyph arches.
    AlienMonolithic,
    // --- Frontier / collapse ---
    /// Fortified ruins, scrap shanties, radio masts.
    PostApoc,
    /// Saloons, water towers, general stores.
    WildWest,
}

impl ThemeArchetype {
    pub const ALL: [Self; 23] = [
        Self::AncientClassical,
        Self::Medieval,
        Self::Nordic,
        Self::FeudalJapan,
        Self::Mesoamerican,
        Self::ModernCity,
        Self::Suburban,
        Self::RuralFarmland,
        Self::IndustrialPark,
        Self::CoastalResort,
        Self::Roadside,
        Self::CivicCampus,
        Self::SportsRec,
        Self::Cyberpunk,
        Self::Steampunk,
        Self::Solarpunk,
        Self::SpaceOutpost,
        Self::Fantasy,
        Self::GothicHorror,
        Self::AlienOrganic,
        Self::AlienMonolithic,
        Self::PostApoc,
        Self::WildWest,
    ];
}

/// Per-room anchor read by every downstream deriver (palette, terrain,
/// water, sky). Cheap to recompute from the DID; typically derived once
/// when the room loads and threaded through the deriver call graph.
#[derive(Clone, Copy, Debug)]
pub struct SceneCharacter {
    /// Anchor hue (degrees `[0, 360)`) for the OkLCH palette deriver.
    pub base_hue_deg: f32,
    /// `[-1, 1]` cool → warm bias. Shifts sun, fog, palette toward
    /// blue/cyan (`-1`) or amber/orange (`+1`).
    pub temperature: f32,
    /// `[-1, 1]` time-of-day bias. `0` is high noon; `±1` is near the
    /// horizon (dawn/dusk). Drives sun altitude and reddening of
    /// directional light.
    pub time_of_day_bias: f32,
    pub landform: LandformArchetype,
    pub biome: BiomeArchetype,
    /// Artificial-structure theme, picked independently of [`Self::biome`].
    /// Drives the seeded mini-settlement (which catalogue structures grow
    /// near spawn) and an optional light accent on the natural derivers.
    pub theme: ThemeArchetype,
}

impl SceneCharacter {
    /// Derive the character anchor from a room-owner DID. Stable across
    /// peers because [`fnv1a_64`] is bit-exact and [`ChaCha8Rng`] is
    /// deterministic.
    pub fn for_did(did: &str) -> Self {
        Self::for_seed(fnv1a_64(did))
    }

    /// Derive the character anchor from a pre-computed seed. Pulled out
    /// of [`Self::for_did`] so tests can sample a known seed without
    /// picking a DID string that happens to hash to it.
    pub fn for_seed(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let base_hue_deg = unit_f32(&mut rng) * 360.0;
        let temperature = signed_unit_f32(&mut rng);
        let time_of_day_bias = signed_unit_f32(&mut rng);
        let landform = pick(&LandformArchetype::ALL, &mut rng);
        let biome = pick(&BiomeArchetype::ALL, &mut rng);
        // Theme is the last draw, orthogonal to biome: appending it here
        // leaves every prior archetype/knob bit-identical to before.
        let theme = pick(&ThemeArchetype::ALL, &mut rng);
        Self {
            base_hue_deg,
            temperature,
            time_of_day_bias,
            landform,
            biome,
            theme,
        }
    }
}

/// `[0, 1)` uniform sample. Top 24 bits of `next_u32` give full f32
/// mantissa precision without bias.
pub fn unit_f32(rng: &mut impl RngCore) -> f32 {
    (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32
}

/// `[-1, 1)` uniform sample.
pub fn signed_unit_f32(rng: &mut impl RngCore) -> f32 {
    unit_f32(rng) * 2.0 - 1.0
}

/// `[lo, hi)` uniform sample.
pub fn range_f32(rng: &mut impl RngCore, lo: f32, hi: f32) -> f32 {
    lo + unit_f32(rng) * (hi - lo)
}

/// Uniform pick from a non-empty slice.
pub fn pick<T: Copy>(items: &[T], rng: &mut impl RngCore) -> T {
    let i = (unit_f32(rng) * items.len() as f32) as usize;
    items[i.min(items.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn determinism_across_calls() {
        let a = SceneCharacter::for_did("did:plc:abc");
        let b = SceneCharacter::for_did("did:plc:abc");
        assert_eq!(a.base_hue_deg, b.base_hue_deg);
        assert_eq!(a.temperature, b.temperature);
        assert_eq!(a.time_of_day_bias, b.time_of_day_bias);
        assert_eq!(a.landform, b.landform);
        assert_eq!(a.biome, b.biome);
        assert_eq!(a.theme, b.theme);
    }

    #[test]
    fn theme_varies_across_seeds() {
        // Sanity that the theme draw is wired and not stuck on one
        // variant — at least a handful of distinct themes over 64 seeds.
        let mut seen: Vec<ThemeArchetype> = Vec::new();
        for s in 0u64..64 {
            let t = SceneCharacter::for_seed(s).theme;
            if !seen.contains(&t) {
                seen.push(t);
            }
        }
        assert!(seen.len() >= 5, "theme pick looks degenerate: {seen:?}");
    }

    #[test]
    fn distinct_dids_vary() {
        let a = SceneCharacter::for_did("did:plc:abc");
        let b = SceneCharacter::for_did("did:plc:def");
        // At least one field differs; hue is the most sensitive.
        assert!((a.base_hue_deg - b.base_hue_deg).abs() > 1e-6);
    }

    #[test]
    fn fields_in_range() {
        for s in 0u64..32 {
            let c = SceneCharacter::for_seed(s);
            assert!((0.0..360.0).contains(&c.base_hue_deg));
            assert!((-1.0..1.0).contains(&c.temperature));
            assert!((-1.0..1.0).contains(&c.time_of_day_bias));
        }
    }

    #[test]
    fn range_helper_respects_bounds() {
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        for _ in 0..32 {
            let x = range_f32(&mut rng, -5.0, 5.0);
            assert!((-5.0..5.0).contains(&x));
        }
    }

    #[test]
    fn theme_all_has_no_duplicates() {
        // A duplicated variant in ALL would silently skew the uniform
        // theme pick toward it; catch the most likely list-editing slip.
        for (i, a) in ThemeArchetype::ALL.iter().enumerate() {
            let count = ThemeArchetype::ALL.iter().filter(|b| *b == a).count();
            assert_eq!(count, 1, "ThemeArchetype::ALL repeats {a:?} (index {i})");
        }
    }
}
