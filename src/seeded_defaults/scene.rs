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

    /// Human-readable display name — used by the pinned re-roll readout.
    pub fn label(self) -> &'static str {
        match self {
            Self::Rolling => "Rolling",
            Self::Craggy => "Craggy",
            Self::Mesa => "Mesa",
            Self::Archipelago => "Archipelago",
            Self::Valleys => "Valleys",
        }
    }
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
    /// Saturated layered greens, humid haze; very high tropical
    /// vegetation. Denser and wetter than [`Self::Lush`] (which now
    /// reads temperate).
    Jungle,
    /// Mixed broadleaf woodland: dappled light, leaf litter, high
    /// vegetation over a woodland floor.
    TemperateForest,
    /// Dark conifer taiga: cold-green, high coniferous vegetation —
    /// green-but-cold, below the tree line ([`Self::Tundra`]/
    /// [`Self::Alpine`] sit above it).
    Boreal,
    /// Dark still water, fog and peat; reeds, mangroves, lily pads —
    /// high water-bound vegetation.
    Wetland,
    /// Rolling grass with wildflowers; medium flowering vegetation.
    Meadow,
    /// Golden dry grass under a big sky, scattered acacia; low-to-medium
    /// vegetation.
    Savanna,
    /// Stratified reds and heavy erosion; very low vegetation. Pairs
    /// naturally with the Mesa landform.
    Badlands,
    /// Blue ice and crevasses; no vegetation. Distinct from
    /// [`Self::Tundra`], which keeps low scrub.
    Glacial,
}

impl BiomeArchetype {
    pub const ALL: [Self; 14] = [
        Self::Lush,
        Self::Arid,
        Self::Alpine,
        Self::Volcanic,
        Self::Coastal,
        Self::Tundra,
        Self::Jungle,
        Self::TemperateForest,
        Self::Boreal,
        Self::Wetland,
        Self::Meadow,
        Self::Savanna,
        Self::Badlands,
        Self::Glacial,
    ];

    /// Human-readable display name — used by the pinned re-roll readout.
    pub fn label(self) -> &'static str {
        match self {
            Self::Lush => "Lush",
            Self::Arid => "Arid",
            Self::Alpine => "Alpine",
            Self::Volcanic => "Volcanic",
            Self::Coastal => "Coastal",
            Self::Tundra => "Tundra",
            Self::Jungle => "Jungle",
            Self::TemperateForest => "Temperate Forest",
            Self::Boreal => "Boreal",
            Self::Wetland => "Wetland",
            Self::Meadow => "Meadow",
            Self::Savanna => "Savanna",
            Self::Badlands => "Badlands",
            Self::Glacial => "Glacial",
        }
    }
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
    /// Harbour batteries, careening slips, prize warehouses, rum tuns —
    /// a Golden-Age buccaneer port. Its destitute register turns eerie
    /// rather than merely poor (gibbets, rotting hulks, tide-line bones),
    /// which is the theme's second read rather than a second identity.
    Pirate,
}

impl ThemeArchetype {
    pub const ALL: [Self; 24] = [
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
        Self::Pirate,
    ];

    /// Human-readable display name — used by the catalogue browser and any
    /// UI that lists themes.
    pub fn label(self) -> &'static str {
        match self {
            Self::AncientClassical => "Ancient Classical",
            Self::Medieval => "Medieval",
            Self::Nordic => "Nordic",
            Self::FeudalJapan => "Feudal Japan",
            Self::Mesoamerican => "Mesoamerican",
            Self::ModernCity => "Modern City",
            Self::Suburban => "Suburban",
            Self::RuralFarmland => "Rural Farmland",
            Self::IndustrialPark => "Industrial Park",
            Self::CoastalResort => "Coastal Resort",
            Self::Roadside => "Roadside",
            Self::CivicCampus => "Civic Campus",
            Self::SportsRec => "Sports & Rec",
            Self::Cyberpunk => "Cyberpunk",
            Self::Steampunk => "Steampunk",
            Self::Solarpunk => "Solarpunk",
            Self::SpaceOutpost => "Space Outpost",
            Self::Fantasy => "Fantasy",
            Self::GothicHorror => "Gothic Horror",
            Self::AlienOrganic => "Alien Organic",
            Self::AlienMonolithic => "Alien Monolithic",
            Self::PostApoc => "Post-Apocalyptic",
            Self::WildWest => "Wild West",
            Self::Pirate => "Pirate",
        }
    }
}

/// Socio-economic tier — the discrete reading of the continuous
/// [`SceneCharacter::prosperity`] axis (poor → rich). Thresholded into
/// thirds. Drives material finish (grime ↔ polish), settlement density,
/// and which cross-theme prop pool a room draws from (shanties/scrap at
/// [`Self::Poor`], fountains/statuary at [`Self::Rich`]).
///
/// Variants are declared poorest-first so the derived [`Ord`] matches the
/// axis direction — [`ProsperityBand`] relies on that ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProsperityTier {
    /// Bottom third — bare, makeshift, weathered.
    Poor,
    /// Middle third — ordinary, unremarkable upkeep.
    Modest,
    /// Top third — polished, ornamented, prosperous.
    Rich,
}

impl ProsperityTier {
    pub const ALL: [Self; 3] = [Self::Poor, Self::Modest, Self::Rich];

    /// Threshold a `[0, 1]` prosperity value into equal thirds.
    pub fn from_unit(prosperity: f32) -> Self {
        match prosperity {
            p if p < 1.0 / 3.0 => Self::Poor,
            p if p < 2.0 / 3.0 => Self::Modest,
            _ => Self::Rich,
        }
    }

    /// Human-readable display name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Poor => "Poor",
            Self::Modest => "Modest",
            Self::Rich => "Rich",
        }
    }
}

/// Conflict tier — the discrete reading of the continuous
/// [`SceneCharacter::escalation`] axis (peaceful → conflict). Thresholded
/// into thirds. Drives mood (smoke/tension audio), defensive props
/// (barricades, wreckage), and escalation-driven geometric damage.
///
/// Variants are declared calmest-first so the derived [`Ord`] matches the
/// axis direction — [`EscalationBand`] relies on that ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EscalationTier {
    /// Bottom third — peaceful: open stalls, benches, no defenses.
    Calm,
    /// Middle third — uneasy: shuttered, lightly fortified.
    Tense,
    /// Top third — open conflict: barricades, wreckage, scorch.
    Conflict,
}

impl EscalationTier {
    pub const ALL: [Self; 3] = [Self::Calm, Self::Tense, Self::Conflict];

    /// Threshold a `[0, 1]` escalation value into equal thirds.
    pub fn from_unit(escalation: f32) -> Self {
        match escalation {
            e if e < 1.0 / 3.0 => Self::Calm,
            e if e < 2.0 / 3.0 => Self::Tense,
            _ => Self::Conflict,
        }
    }

    /// Human-readable display name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Calm => "Calm",
            Self::Tense => "Tense",
            Self::Conflict => "Conflict",
        }
    }
}

/// Inclusive prosperity-tier affinity band a catalogue entry advertises:
/// the contiguous span of [`ProsperityTier`]s a room may have for the
/// entry to be eligible. `ANY` (the default) spans every tier, so
/// untagged entries are always eligible. Relies on [`ProsperityTier`]'s
/// poorest-first [`Ord`]. One instantiation of the shared
/// [`Band`](crate::seeded_defaults::band::Band) (#654).
pub type ProsperityBand = crate::seeded_defaults::band::Band<ProsperityTier>;

impl crate::seeded_defaults::band::BandTier for ProsperityTier {
    const MIN: Self = ProsperityTier::Poor;
    const MAX: Self = ProsperityTier::Rich;
    fn label(self) -> &'static str {
        ProsperityTier::label(self)
    }
}

/// Inclusive escalation-tier affinity band — the [`EscalationTier`]
/// analogue of [`ProsperityBand`]. `ANY` is the default.
pub type EscalationBand = crate::seeded_defaults::band::Band<EscalationTier>;

impl crate::seeded_defaults::band::BandTier for EscalationTier {
    const MIN: Self = EscalationTier::Calm;
    const MAX: Self = EscalationTier::Conflict;
    fn label(self) -> &'static str {
        EscalationTier::label(self)
    }
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
    /// `[0, 1]` socio-economic axis: `0` is destitute, `1` is affluent.
    /// Orthogonal to every other field. Read via [`Self::prosperity_tier`];
    /// drives material finish, settlement density, and prop pools.
    pub prosperity: f32,
    /// `[0, 1]` conflict axis: `0` is peaceful, `1` is open conflict.
    /// Orthogonal to every other field. Read via [`Self::escalation_tier`];
    /// drives mood, defensive props, and geometric damage.
    pub escalation: f32,
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
        let theme = pick(&ThemeArchetype::ALL, &mut rng);
        // The socio-political axes are the last two draws, orthogonal to
        // everything above: appending them leaves every prior archetype /
        // knob (theme included) bit-identical to before they existed.
        let prosperity = unit_f32(&mut rng);
        let escalation = unit_f32(&mut rng);
        Self {
            base_hue_deg,
            temperature,
            time_of_day_bias,
            landform,
            biome,
            theme,
            prosperity,
            escalation,
        }
    }

    /// Discrete socio-economic reading of [`Self::prosperity`], thresholded
    /// into equal thirds of `[0, 1]`.
    pub fn prosperity_tier(&self) -> ProsperityTier {
        ProsperityTier::from_unit(self.prosperity)
    }

    /// Discrete conflict reading of [`Self::escalation`], thresholded into
    /// equal thirds of `[0, 1]`.
    pub fn escalation_tier(&self) -> EscalationTier {
        EscalationTier::from_unit(self.escalation)
    }
}

/// Ceiling on the pinned-re-roll seed hunt (#1005). The hardest legal
/// room pin-set (all five axes locked) matches ~1 seed in 14,490, so two
/// million trials miss with probability ~e⁻¹³⁸ — the cap exists to bound
/// the loop if a future draw stops being uniform, not because a miss is
/// ever expected.
const PIN_HUNT_CAP: u64 = 2_000_000;

/// Deterministic pinned-re-roll seed hunt (#1005): the first seed at or
/// after `start` (wrapping) whose derivation satisfies `accepts`, or
/// `None` after [`PIN_HUNT_CAP`] trials. Deterministic in `start`, so
/// clicking "Re-roll" twice on the same seed lands on the same world —
/// exactly the contract the un-pinned re-roll has.
pub fn find_matching_seed(start: u64, accepts: impl Fn(u64) -> bool) -> Option<u64> {
    (0..PIN_HUNT_CAP)
        .map(|i| start.wrapping_add(i))
        .find(|&s| accepts(s))
}

/// Transient per-axis locks for the World editor's pinned re-roll (#1005).
///
/// A pinned re-roll is a deterministic seed *hunt*, not a parameter
/// override: [`Self::find_seed`] walks forward from the clicked seed to
/// the first one whose [`SceneCharacter`] naturally rolls every pinned
/// value, and the room is then built by the unchanged
/// `RoomRecord::default_for_seed` path. By construction the result is
/// indistinguishable from any other seeded room — no out-of-distribution
/// combination can exist, and peers re-derive it bit-identically from the
/// seed alone. Pins are editor UI state only; nothing is stored in the
/// record.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct ScenePins {
    pub landform: Option<LandformArchetype>,
    pub biome: Option<BiomeArchetype>,
    pub theme: Option<ThemeArchetype>,
    /// Pinned as the discrete tier; the hunt accepts any seed whose
    /// continuous prosperity falls in the tier's third, so the record
    /// keeps a natural in-distribution value rather than a midpoint.
    pub prosperity: Option<ProsperityTier>,
    /// Pinned as the discrete tier, like [`Self::prosperity`].
    pub escalation: Option<EscalationTier>,
}

impl ScenePins {
    /// Whether `c` satisfies every pinned axis (unpinned axes accept
    /// anything).
    pub fn matches(&self, c: &SceneCharacter) -> bool {
        self.landform.is_none_or(|p| p == c.landform)
            && self.biome.is_none_or(|p| p == c.biome)
            && self.theme.is_none_or(|p| p == c.theme)
            && self.prosperity.is_none_or(|p| p == c.prosperity_tier())
            && self.escalation.is_none_or(|p| p == c.escalation_tier())
    }

    /// The first seed at or after `start` whose [`SceneCharacter`]
    /// satisfies every pin. With no pins this is `start` itself, so the
    /// un-pinned path is bit-identical to the pre-#1005 re-roll.
    pub fn find_seed(&self, start: u64) -> Option<u64> {
        if *self == Self::default() {
            return Some(start);
        }
        find_matching_seed(start, |s| self.matches(&SceneCharacter::for_seed(s)))
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
        assert_eq!(a.prosperity, b.prosperity);
        assert_eq!(a.escalation, b.escalation);
    }

    #[test]
    fn socio_axes_in_range_and_orthogonal() {
        // Both axes stay in [0, 1] and neither is stuck on one tier across
        // seeds (a degenerate draw would collapse to a single tier).
        let mut prosperity_tiers: Vec<ProsperityTier> = Vec::new();
        let mut escalation_tiers: Vec<EscalationTier> = Vec::new();
        for s in 0u64..96 {
            let c = SceneCharacter::for_seed(s);
            assert!(
                (0.0..=1.0).contains(&c.prosperity),
                "prosperity OOB: {}",
                c.prosperity
            );
            assert!(
                (0.0..=1.0).contains(&c.escalation),
                "escalation OOB: {}",
                c.escalation
            );
            if !prosperity_tiers.contains(&c.prosperity_tier()) {
                prosperity_tiers.push(c.prosperity_tier());
            }
            if !escalation_tiers.contains(&c.escalation_tier()) {
                escalation_tiers.push(c.escalation_tier());
            }
        }
        assert_eq!(prosperity_tiers.len(), 3, "prosperity tiers degenerate");
        assert_eq!(escalation_tiers.len(), 3, "escalation tiers degenerate");
    }

    #[test]
    fn tier_thresholds_split_into_thirds() {
        let tier_at = |p: f32| {
            let mut c = SceneCharacter::for_seed(0);
            c.prosperity = p;
            c.escalation = p;
            (c.prosperity_tier(), c.escalation_tier())
        };
        assert_eq!(tier_at(0.0), (ProsperityTier::Poor, EscalationTier::Calm));
        assert_eq!(tier_at(0.33), (ProsperityTier::Poor, EscalationTier::Calm));
        assert_eq!(
            tier_at(0.34),
            (ProsperityTier::Modest, EscalationTier::Tense)
        );
        assert_eq!(
            tier_at(0.66),
            (ProsperityTier::Modest, EscalationTier::Tense)
        );
        assert_eq!(
            tier_at(0.67),
            (ProsperityTier::Rich, EscalationTier::Conflict)
        );
        assert_eq!(
            tier_at(1.0),
            (ProsperityTier::Rich, EscalationTier::Conflict)
        );
    }

    #[test]
    fn band_any_accepts_every_tier() {
        for t in ProsperityTier::ALL {
            assert!(ProsperityBand::ANY.accepts(t));
        }
        for t in EscalationTier::ALL {
            assert!(EscalationBand::ANY.accepts(t));
        }
    }

    #[test]
    fn band_only_and_range_gate_correctly() {
        let rich = ProsperityBand::only(ProsperityTier::Rich);
        assert!(rich.accepts(ProsperityTier::Rich));
        assert!(!rich.accepts(ProsperityTier::Poor));
        assert!(!rich.accepts(ProsperityTier::Modest));

        // Poor..=Modest excludes only the top tier.
        let low = ProsperityBand::range(ProsperityTier::Poor, ProsperityTier::Modest);
        assert!(low.accepts(ProsperityTier::Poor));
        assert!(low.accepts(ProsperityTier::Modest));
        assert!(!low.accepts(ProsperityTier::Rich));

        let conflict = EscalationBand::only(EscalationTier::Conflict);
        assert!(conflict.accepts(EscalationTier::Conflict));
        assert!(!conflict.accepts(EscalationTier::Calm));
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
    fn empty_pins_hunt_returns_the_start_seed() {
        // The un-pinned re-roll must stay bit-identical to pre-#1005:
        // no pins → the clicked seed is used verbatim.
        assert_eq!(ScenePins::default().find_seed(42), Some(42));
    }

    #[test]
    fn pinned_hunt_is_deterministic_and_satisfies_the_pins() {
        let pins = ScenePins {
            biome: Some(BiomeArchetype::Glacial),
            theme: Some(ThemeArchetype::WildWest),
            prosperity: Some(ProsperityTier::Rich),
            ..Default::default()
        };
        let found = pins.find_seed(0).expect("hunt failed");
        let c = SceneCharacter::for_seed(found);
        assert_eq!(c.biome, BiomeArchetype::Glacial);
        assert_eq!(c.theme, ThemeArchetype::WildWest);
        assert_eq!(c.prosperity_tier(), ProsperityTier::Rich);
        // Unpinned axes stay whatever the found seed rolls — but the hunt
        // itself is deterministic: same start, same pins, same seed.
        assert_eq!(pins.find_seed(0), Some(found));
    }

    #[test]
    fn a_seed_that_already_matches_is_kept() {
        // Re-rolling again from a found seed must be a fixpoint: the row
        // shows the seed the record was built from, and clicking Re-roll
        // on it (same pins) must not walk away from it.
        let pins = ScenePins {
            landform: Some(LandformArchetype::Mesa),
            ..Default::default()
        };
        let found = pins.find_seed(7).expect("hunt failed");
        assert_eq!(pins.find_seed(found), Some(found));
    }

    #[test]
    fn every_variant_of_every_axis_is_huntable() {
        // Each axis variant pinned alone must be reachable from a fixed
        // start — a variant the hunt can never satisfy would make its
        // combo option a dead button.
        for lf in LandformArchetype::ALL {
            let pins = ScenePins {
                landform: Some(lf),
                ..Default::default()
            };
            let s = pins.find_seed(0).expect("landform unreachable");
            assert_eq!(SceneCharacter::for_seed(s).landform, lf);
        }
        for b in BiomeArchetype::ALL {
            let pins = ScenePins {
                biome: Some(b),
                ..Default::default()
            };
            let s = pins.find_seed(0).expect("biome unreachable");
            assert_eq!(SceneCharacter::for_seed(s).biome, b);
        }
        for t in ThemeArchetype::ALL {
            let pins = ScenePins {
                theme: Some(t),
                ..Default::default()
            };
            let s = pins.find_seed(0).expect("theme unreachable");
            assert_eq!(SceneCharacter::for_seed(s).theme, t);
        }
        for p in ProsperityTier::ALL {
            let pins = ScenePins {
                prosperity: Some(p),
                ..Default::default()
            };
            let s = pins.find_seed(0).expect("prosperity unreachable");
            assert_eq!(SceneCharacter::for_seed(s).prosperity_tier(), p);
        }
        for e in EscalationTier::ALL {
            let pins = ScenePins {
                escalation: Some(e),
                ..Default::default()
            };
            let s = pins.find_seed(0).expect("escalation unreachable");
            assert_eq!(SceneCharacter::for_seed(s).escalation_tier(), e);
        }
    }

    #[test]
    fn fully_pinned_hunt_succeeds() {
        // The hardest legal pin-set: all five axes at once (~1 in 14,490
        // seeds). Must land inside the hunt cap with a satisfying seed.
        let pins = ScenePins {
            landform: Some(LandformArchetype::Archipelago),
            biome: Some(BiomeArchetype::Volcanic),
            theme: Some(ThemeArchetype::GothicHorror),
            prosperity: Some(ProsperityTier::Poor),
            escalation: Some(EscalationTier::Conflict),
        };
        let s = pins
            .find_seed(0xDEAD_BEEF)
            .expect("full pin-set unreachable");
        let c = SceneCharacter::for_seed(s);
        assert!(pins.matches(&c), "hunt returned a non-matching seed");
    }

    #[test]
    fn labels_cover_every_variant_distinctly() {
        // The readout / combo labels must be unique per axis, or two
        // options become indistinguishable in the picker.
        let landforms: Vec<_> = LandformArchetype::ALL.iter().map(|l| l.label()).collect();
        let biomes: Vec<_> = BiomeArchetype::ALL.iter().map(|b| b.label()).collect();
        for (i, l) in landforms.iter().enumerate() {
            assert_eq!(landforms.iter().position(|x| x == l), Some(i));
        }
        for (i, b) in biomes.iter().enumerate() {
            assert_eq!(biomes.iter().position(|x| x == b), Some(i));
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
