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
}

/// Inclusive prosperity-tier affinity band a catalogue entry advertises:
/// the contiguous span of [`ProsperityTier`]s a room may have for the
/// entry to be eligible. [`Self::ANY`] (the default) spans every tier, so
/// untagged entries are always eligible. Relies on [`ProsperityTier`]'s
/// poorest-first [`Ord`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProsperityBand {
    lo: ProsperityTier,
    hi: ProsperityTier,
}

impl ProsperityBand {
    /// Every tier — an untagged, always-eligible entry.
    pub const ANY: Self = Self {
        lo: ProsperityTier::Poor,
        hi: ProsperityTier::Rich,
    };

    /// Eligible only at exactly `tier`.
    pub const fn only(tier: ProsperityTier) -> Self {
        Self { lo: tier, hi: tier }
    }

    /// Eligible across the inclusive `lo..=hi` span (caller passes them in
    /// ascending order).
    pub const fn range(lo: ProsperityTier, hi: ProsperityTier) -> Self {
        Self { lo, hi }
    }

    /// Whether a room at `tier` may place an entry advertising this band.
    pub fn accepts(self, tier: ProsperityTier) -> bool {
        self.lo <= tier && tier <= self.hi
    }
}

/// Inclusive escalation-tier affinity band — the [`EscalationTier`]
/// analogue of [`ProsperityBand`]. [`Self::ANY`] is the default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EscalationBand {
    lo: EscalationTier,
    hi: EscalationTier,
}

impl EscalationBand {
    /// Every tier — an untagged, always-eligible entry.
    pub const ANY: Self = Self {
        lo: EscalationTier::Calm,
        hi: EscalationTier::Conflict,
    };

    /// Eligible only at exactly `tier`.
    pub const fn only(tier: EscalationTier) -> Self {
        Self { lo: tier, hi: tier }
    }

    /// Eligible across the inclusive `lo..=hi` span (caller passes them in
    /// ascending order).
    pub const fn range(lo: EscalationTier, hi: EscalationTier) -> Self {
        Self { lo, hi }
    }

    /// Whether a room at `tier` may place an entry advertising this band.
    pub fn accepts(self, tier: EscalationTier) -> bool {
        self.lo <= tier && tier <= self.hi
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
        match self.prosperity {
            p if p < 1.0 / 3.0 => ProsperityTier::Poor,
            p if p < 2.0 / 3.0 => ProsperityTier::Modest,
            _ => ProsperityTier::Rich,
        }
    }

    /// Discrete conflict reading of [`Self::escalation`], thresholded into
    /// equal thirds of `[0, 1]`.
    pub fn escalation_tier(&self) -> EscalationTier {
        match self.escalation {
            e if e < 1.0 / 3.0 => EscalationTier::Calm,
            e if e < 2.0 / 3.0 => EscalationTier::Tense,
            _ => EscalationTier::Conflict,
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
    fn theme_all_has_no_duplicates() {
        // A duplicated variant in ALL would silently skew the uniform
        // theme pick toward it; catch the most likely list-editing slip.
        for (i, a) in ThemeArchetype::ALL.iter().enumerate() {
            let count = ThemeArchetype::ALL.iter().filter(|b| *b == a).count();
            assert_eq!(count, 1, "ThemeArchetype::ALL repeats {a:?} (index {i})");
        }
    }
}
