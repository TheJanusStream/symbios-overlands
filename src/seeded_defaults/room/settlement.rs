//! Seeded mini-settlement spec — every home region grows a themed
//! cluster of catalogue structures near spawn: one landmark, a few
//! secondary buildings ringed around it, and scatter props.
//!
//! Members are resolved by querying the catalogue
//! ([`crate::catalogue::entries_for`]) for entries tagged with the room's
//! [`ThemeArchetype`] and the matching [`StructureRole`], rather than a
//! hardcoded slug pool — so adding a themed catalogue entry grows the
//! settlements automatically. A theme with no landmark entry yet falls
//! back wholesale to [`FALLBACK_THEME`], so every room gets a coherent
//! settlement while the catalogue fills out.
//!
//! Placement: the landmark sits at a footprint-appropriate distance band
//! facing spawn; secondaries fan out on the *far* side of the landmark
//! (so they never crowd the spawn square) facing inward; props scatter
//! across the settlement's far hemisphere. The wiring layer
//! ([`RoomRecord::default_for_did`](crate::pds::RoomRecord::default_for_did))
//! turns each member into a named generator (restamping Shape-grammar
//! seeds) plus a terrain-snapped `Placement::Absolute` carrying the
//! member's water clearance.

use std::f32::consts::TAU;

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use crate::catalogue::{StructureRole, entries_for, entries_for_room};
use crate::seeded_defaults::scene::{
    EscalationTier, ProsperityTier, SceneCharacter, ThemeArchetype, pick, range_f32, unit_f32,
};

/// Sub-stream salt distinct from every sibling room deriver.
const SETTLEMENT_STREAM_SALT: u64 = 0x1A4D_3A2C_1A4D_3A2C;

/// Theme used when the room's own theme has no landmark-role catalogue
/// entry yet. AncientClassical is the most universally-readable kit and
/// is guaranteed non-empty, so every room still gets a settlement during
/// the content build-out.
const FALLBACK_THEME: ThemeArchetype = ThemeArchetype::AncientClassical;

/// Upper bound on secondary buildings in a settlement.
pub const MAX_SECONDARIES: usize = 3;
/// Upper bound on scatter props in a settlement.
pub const MAX_PROPS: usize = 6;

/// One placed structure within a settlement: which catalogue entry,
/// where, and how it stands.
#[derive(Clone, Copy, Debug)]
pub struct SettlementMember {
    /// Catalogue slug (resolved through [`crate::catalogue::by_slug`]).
    pub slug: &'static str,
    /// World XZ of the structure origin.
    pub offset: [f32; 2],
    /// Yaw (radians around Y).
    pub yaw_rad: f32,
    /// Uniform scale multiplier.
    pub scale: f32,
    /// Replacement seed for Shape-grammar entries' stochastic rules.
    pub grammar_seed: u64,
    /// Dry-land clearance radius (m) for the compiler's water-avoidance
    /// walk — the member's [`crate::catalogue::Footprint::clearance`].
    pub clearance: f32,
}

/// The full themed cluster for a room: exactly one landmark plus any
/// available secondaries and props for the (effective) theme.
#[derive(Clone, Debug)]
pub struct Settlement {
    pub landmark: SettlementMember,
    pub secondaries: Vec<SettlementMember>,
    pub props: Vec<SettlementMember>,
}

impl Settlement {
    pub fn from_scene(scene: &SceneCharacter, room_seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(room_seed ^ SETTLEMENT_STREAM_SALT);

        // Fall back to a populated theme if the room's own theme has no
        // landmark yet, so the whole cluster stays internally coherent
        // (no AncientClassical landmark ringed by another theme's props).
        let theme = effective_theme(scene.theme);

        // Socio-political tiers drive how dense / large the settlement is
        // (prosperity) and which cross-theme tier props join the pool
        // (prosperity + escalation).
        let prosperity = scene.prosperity_tier();
        let escalation = scene.escalation_tier();

        let landmark = place_landmark(theme, prosperity, escalation, &mut rng);
        let secondaries = place_secondaries(theme, prosperity, escalation, &landmark, &mut rng);
        let props = place_props(theme, prosperity, escalation, &landmark, &mut rng);

        Self {
            landmark,
            secondaries,
            props,
        }
    }
}

/// The theme actually used for member selection: the room's own theme if
/// it has at least one landmark entry, otherwise [`FALLBACK_THEME`].
fn effective_theme(theme: ThemeArchetype) -> ThemeArchetype {
    if entries_for(theme, StructureRole::Landmark).next().is_some() {
        theme
    } else {
        FALLBACK_THEME
    }
}

/// Theme+role entries narrowed to the room's socio tiers when any match,
/// else the full theme pool. So a theme that authored a tier-specific
/// variant (e.g. Cyberpunk's poor scrap shanty) uses it in matching rooms,
/// while a theme without one still yields a coherent member rather than an
/// empty pool. Props don't use this — their cross-theme tier props ride the
/// always-present civic kit, so [`entries_for_room`] suffices there.
fn tiered_pool(
    theme: ThemeArchetype,
    role: StructureRole,
    prosperity: ProsperityTier,
    escalation: EscalationTier,
) -> Vec<&'static dyn crate::catalogue::CatalogueEntry> {
    let tiered: Vec<_> = entries_for_room(theme, role, prosperity, escalation).collect();
    if tiered.is_empty() {
        entries_for(theme, role).collect()
    } else {
        tiered
    }
}

/// Inclusive `(min, max)` secondary-building count by prosperity: richer
/// settlements are denser. Clamped to the pool size and [`MAX_SECONDARIES`].
fn secondary_count_band(tier: ProsperityTier) -> (usize, usize) {
    match tier {
        ProsperityTier::Poor => (0, 1),
        ProsperityTier::Modest => (1, 2),
        ProsperityTier::Rich => (2, 3),
    }
}

/// Inclusive `(min, max)` scatter-prop count by prosperity. Clamped to
/// [`MAX_PROPS`].
fn prop_count_band(tier: ProsperityTier) -> (usize, usize) {
    match tier {
        ProsperityTier::Poor => (1, 3),
        ProsperityTier::Modest => (2, 5),
        ProsperityTier::Rich => (4, 6),
    }
}

/// Uniform-scale band for the landmark by prosperity: poorer settlements'
/// hero structure is smaller, richer ones grander.
fn landmark_scale_band(tier: ProsperityTier) -> (f32, f32) {
    match tier {
        ProsperityTier::Poor => (0.75, 1.05),
        ProsperityTier::Modest => (0.85, 1.20),
        ProsperityTier::Rich => (1.05, 1.45),
    }
}

/// One uniform integer draw in the inclusive range `[lo, hi]` (one
/// `unit_f32` from `rng`). `hi <= lo` yields `lo`.
fn sample_count(rng: &mut ChaCha8Rng, lo: usize, hi: usize) -> usize {
    if hi <= lo {
        return lo;
    }
    (lo + (unit_f32(rng) * (hi - lo + 1) as f32) as usize).min(hi)
}

fn place_landmark(
    theme: ThemeArchetype,
    prosperity: ProsperityTier,
    escalation: EscalationTier,
    rng: &mut ChaCha8Rng,
) -> SettlementMember {
    // `effective_theme` guarantees the theme has a landmark, and
    // `tiered_pool` falls back to it, so this pool is non-empty.
    let pool = tiered_pool(theme, StructureRole::Landmark, prosperity, escalation);
    let entry = pick(&pool, rng);
    let fp = entry.footprint();

    let angle = unit_f32(rng) * TAU;
    let dist = range_f32(rng, fp.min_spawn_dist, fp.min_spawn_dist + 30.0);
    let offset = [angle.sin() * dist, angle.cos() * dist];
    // Face the spawn origin (±0.35 rad jitter).
    let yaw_rad = offset[0].atan2(offset[1]) + range_f32(rng, -0.35, 0.35);

    let (scale_lo, scale_hi) = landmark_scale_band(prosperity);
    SettlementMember {
        slug: entry.slug(),
        offset,
        yaw_rad,
        scale: range_f32(rng, scale_lo, scale_hi),
        grammar_seed: rng.next_u64(),
        clearance: fp.clearance,
    }
}

fn place_secondaries(
    theme: ThemeArchetype,
    prosperity: ProsperityTier,
    escalation: EscalationTier,
    landmark: &SettlementMember,
    rng: &mut ChaCha8Rng,
) -> Vec<SettlementMember> {
    let mut remaining = tiered_pool(theme, StructureRole::Secondary, prosperity, escalation);
    if remaining.is_empty() {
        return Vec::new();
    }

    let (lo, hi) = secondary_count_band(prosperity);
    let hi = hi.min(remaining.len()).min(MAX_SECONDARIES);
    let count = sample_count(rng, lo.min(hi), hi);
    if count == 0 {
        return Vec::new();
    }

    // Bearing from the spawn origin out to the landmark; secondaries fan
    // out around it so they always sit *beyond* the landmark.
    let base = landmark.offset[0].atan2(landmark.offset[1]);

    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        // Pick without replacement so secondaries are distinct.
        let idx = ((unit_f32(rng) * remaining.len() as f32) as usize).min(remaining.len() - 1);
        let entry = remaining.remove(idx);
        let fp = entry.footprint();

        let spread = if count == 1 {
            range_f32(rng, -0.6, 0.6)
        } else {
            -1.2 + 2.4 * (i as f32) / ((count - 1) as f32) + range_f32(rng, -0.25, 0.25)
        };
        let dir = base + spread;
        let r = landmark.clearance + fp.clearance + range_f32(rng, 4.0, 12.0);
        let offset = [
            landmark.offset[0] + dir.sin() * r,
            landmark.offset[1] + dir.cos() * r,
        ];
        // Face the landmark centre (±0.25 rad jitter).
        let yaw_rad = (landmark.offset[0] - offset[0]).atan2(landmark.offset[1] - offset[1])
            + range_f32(rng, -0.25, 0.25);

        out.push(SettlementMember {
            slug: entry.slug(),
            offset,
            yaw_rad,
            scale: range_f32(rng, 0.80, 1.10),
            grammar_seed: rng.next_u64(),
            clearance: fp.clearance,
        });
    }
    out
}

fn place_props(
    theme: ThemeArchetype,
    prosperity: ProsperityTier,
    escalation: EscalationTier,
    landmark: &SettlementMember,
    rng: &mut ChaCha8Rng,
) -> Vec<SettlementMember> {
    // The room-aware query folds in the cross-theme tier props (civic kit)
    // whose prosperity / escalation band matches this room, on top of the
    // theme's own props.
    let pool: Vec<&'static dyn crate::catalogue::CatalogueEntry> =
        entries_for_room(theme, StructureRole::Prop, prosperity, escalation).collect();
    if pool.is_empty() {
        return Vec::new();
    }

    let (lo, hi) = prop_count_band(prosperity);
    let count = sample_count(rng, lo, hi.min(MAX_PROPS));
    let base = landmark.offset[0].atan2(landmark.offset[1]);
    let radius = landmark.clearance + 25.0;

    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        // Props are clutter — sampled with replacement.
        let entry = pick(&pool, rng);
        let fp = entry.footprint();
        // Keep props on the settlement (far) hemisphere too.
        let dir = base + range_f32(rng, -1.4, 1.4);
        let r = range_f32(rng, landmark.clearance + 2.0, radius);
        let offset = [
            landmark.offset[0] + dir.sin() * r,
            landmark.offset[1] + dir.cos() * r,
        ];

        out.push(SettlementMember {
            slug: entry.slug(),
            offset,
            yaw_rad: unit_f32(rng) * TAU,
            scale: range_f32(rng, 0.70, 1.05),
            grammar_seed: rng.next_u64(),
            clearance: fp.clearance,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::by_slug;

    #[test]
    fn deterministic() {
        let scene = SceneCharacter::for_seed(11);
        let a = Settlement::from_scene(&scene, 11);
        let b = Settlement::from_scene(&scene, 11);
        assert_eq!(a.landmark.slug, b.landmark.slug);
        assert_eq!(a.landmark.offset, b.landmark.offset);
        assert_eq!(a.secondaries.len(), b.secondaries.len());
        for (x, y) in a.secondaries.iter().zip(&b.secondaries) {
            assert_eq!(x.slug, y.slug);
            assert_eq!(x.offset, y.offset);
        }
    }

    #[test]
    fn every_theme_yields_a_resolvable_settlement() {
        for theme in ThemeArchetype::ALL {
            for s in 0u64..6 {
                let mut scene = SceneCharacter::for_seed(s);
                scene.theme = theme;
                let st = Settlement::from_scene(&scene, s);
                assert!(
                    by_slug(st.landmark.slug).is_some(),
                    "landmark {} (theme {theme:?}) not in catalogue",
                    st.landmark.slug
                );
                for m in st.secondaries.iter().chain(&st.props) {
                    assert!(
                        by_slug(m.slug).is_some(),
                        "member {} not in catalogue",
                        m.slug
                    );
                }
            }
        }
    }

    #[test]
    fn landmark_clears_spawn_square() {
        for theme in ThemeArchetype::ALL {
            for s in 0u64..16 {
                let mut scene = SceneCharacter::for_seed(s);
                scene.theme = theme;
                let st = Settlement::from_scene(&scene, s);
                let d = (st.landmark.offset[0].powi(2) + st.landmark.offset[1].powi(2)).sqrt();
                assert!(
                    d >= 30.0,
                    "landmark too close to spawn: {d} m (theme {theme:?})"
                );
                // Scale now varies by prosperity tier; the union of all tier
                // bands is [0.75, 1.45].
                assert!((0.75..=1.45).contains(&st.landmark.scale));
            }
        }
    }

    #[test]
    fn secondaries_bounded_distinct_and_clear_spawn() {
        for s in 0u64..64 {
            let mut scene = SceneCharacter::for_seed(s);
            scene.theme = ThemeArchetype::AncientClassical;
            let st = Settlement::from_scene(&scene, s);
            assert!(st.secondaries.len() <= MAX_SECONDARIES);

            let mut slugs: Vec<&str> = st.secondaries.iter().map(|m| m.slug).collect();
            let n = slugs.len();
            slugs.sort();
            slugs.dedup();
            assert_eq!(n, slugs.len(), "secondaries should be distinct");

            for m in &st.secondaries {
                let d = (m.offset[0].powi(2) + m.offset[1].powi(2)).sqrt();
                assert!(d >= 25.0, "secondary too close to spawn: {d} m");
            }
        }
    }

    #[test]
    fn cyberpunk_settlement_uses_its_own_kit_by_prosperity() {
        // The per-theme poor/rich pattern (#433): a rich cyberpunk room
        // grows the glossy neon kit, a poor one grows the scrap-shanty
        // undercity — never the other theme's buildings nor the fallback.
        const RICH_SECONDARIES: [&str; 4] = [
            "data_spire",
            "arcade_block",
            "holo_billboard",
            "parking_stack",
        ];
        const POOR_SECONDARIES: [&str; 2] = ["container_stack", "tarp_shelter"];

        let cyber_prop = |slug: &str| {
            // Cyberpunk-tagged or an all-theme civic prop — either is a
            // legitimate member of a cyberpunk room's pool.
            by_slug(slug)
                .expect("prop resolves")
                .themes()
                .contains(&ThemeArchetype::Cyberpunk)
        };

        let mut rich_placed_secondary = false;
        let mut poor_placed_secondary = false;
        for s in 0u64..32 {
            let mut rich = SceneCharacter::for_seed(s);
            rich.theme = ThemeArchetype::Cyberpunk;
            rich.prosperity = 0.95;
            let r = Settlement::from_scene(&rich, s);
            assert_eq!(r.landmark.slug, "neon_megatower", "rich cyberpunk landmark");
            for sec in &r.secondaries {
                assert!(
                    RICH_SECONDARIES.contains(&sec.slug),
                    "rich cyber secondary {}",
                    sec.slug
                );
            }
            assert!(r.props.iter().all(|p| cyber_prop(p.slug)));
            rich_placed_secondary |= !r.secondaries.is_empty();

            let mut poor = SceneCharacter::for_seed(s);
            poor.theme = ThemeArchetype::Cyberpunk;
            poor.prosperity = 0.05;
            let p = Settlement::from_scene(&poor, s);
            assert_eq!(p.landmark.slug, "scrap_shanty", "poor cyberpunk landmark");
            for sec in &p.secondaries {
                assert!(
                    POOR_SECONDARIES.contains(&sec.slug),
                    "poor cyber secondary {}",
                    sec.slug
                );
            }
            assert!(p.props.iter().all(|p| cyber_prop(p.slug)));
            poor_placed_secondary |= !p.secondaries.is_empty();
        }
        assert!(
            rich_placed_secondary,
            "some rich cyberpunk room places a secondary"
        );
        assert!(
            poor_placed_secondary,
            "some poor cyberpunk room places a secondary"
        );
    }

    #[test]
    fn nordic_settlement_uses_its_own_kit_by_prosperity() {
        // The per-theme poor/rich pattern (#433/#394): an affluent Nordic
        // room grows the carved-timber steading, a destitute one grows the
        // turf croft — the two registers never cross. (The shared, band-
        // agnostic `stone_circle` is a legitimate Nordic landmark in either,
        // so we assert by register exclusion rather than an exact slug.)
        const POOR_KIT: [&str; 3] = ["turf_house", "sod_shelter", "wood_pile"];
        const RICH_KIT: [&str; 8] = [
            "mead_hall",
            "boathouse",
            "signal_beacon",
            "rune_stones",
            "longship",
            "shield_rack",
            "drying_rack",
            "totem_pole",
        ];

        let nordic_member = |slug: &str| {
            by_slug(slug)
                .expect("member resolves")
                .themes()
                .contains(&ThemeArchetype::Nordic)
        };

        let mut rich_placed_secondary = false;
        let mut poor_placed_sod_shelter = false;
        for s in 0u64..32 {
            let mut rich = SceneCharacter::for_seed(s);
            rich.theme = ThemeArchetype::Nordic;
            rich.prosperity = 0.95;
            let r = Settlement::from_scene(&rich, s);
            for m in std::iter::once(&r.landmark)
                .chain(&r.secondaries)
                .chain(&r.props)
            {
                assert!(nordic_member(m.slug), "rich nordic member {}", m.slug);
                assert!(
                    !POOR_KIT.contains(&m.slug),
                    "rich nordic room grew the poor kit: {}",
                    m.slug
                );
            }
            rich_placed_secondary |= r.secondaries.iter().any(|sec| RICH_KIT.contains(&sec.slug));

            let mut poor = SceneCharacter::for_seed(s);
            poor.theme = ThemeArchetype::Nordic;
            poor.prosperity = 0.05;
            let p = Settlement::from_scene(&poor, s);
            for m in std::iter::once(&p.landmark)
                .chain(&p.secondaries)
                .chain(&p.props)
            {
                assert!(nordic_member(m.slug), "poor nordic member {}", m.slug);
                assert!(
                    !RICH_KIT.contains(&m.slug),
                    "poor nordic room grew the established kit: {}",
                    m.slug
                );
            }
            poor_placed_sod_shelter |= p.secondaries.iter().any(|sec| sec.slug == "sod_shelter");
        }
        assert!(
            rich_placed_secondary,
            "some rich nordic room places an established secondary"
        );
        assert!(
            poor_placed_sod_shelter,
            "some poor nordic room places the sod shelter"
        );
    }

    #[test]
    fn feudal_japan_settlement_uses_its_own_kit_by_prosperity() {
        // The per-theme poor/rich pattern (#433/#395): an affluent room grows
        // the lacquered temple kit, a destitute one the farmstead — the two
        // registers never cross.
        const POOR_KIT: [&str; 3] = ["minka", "rice_shed", "straw_bales"];
        const RICH_KIT: [&str; 8] = [
            "pagoda",
            "torii_gate",
            "tea_house",
            "dojo",
            "stone_lantern",
            "koi_pond",
            "bamboo_fence",
            "bonsai",
        ];

        let jp_member = |slug: &str| {
            by_slug(slug)
                .expect("member resolves")
                .themes()
                .contains(&ThemeArchetype::FeudalJapan)
        };

        let mut rich_placed_secondary = false;
        let mut poor_placed_rice_shed = false;
        for s in 0u64..32 {
            let mut rich = SceneCharacter::for_seed(s);
            rich.theme = ThemeArchetype::FeudalJapan;
            rich.prosperity = 0.95;
            let r = Settlement::from_scene(&rich, s);
            assert_eq!(r.landmark.slug, "pagoda", "rich feudal-japan landmark");
            for m in std::iter::once(&r.landmark)
                .chain(&r.secondaries)
                .chain(&r.props)
            {
                assert!(jp_member(m.slug), "rich feudal-japan member {}", m.slug);
                assert!(
                    !POOR_KIT.contains(&m.slug),
                    "rich room grew the poor kit: {}",
                    m.slug
                );
            }
            rich_placed_secondary |= r.secondaries.iter().any(|sec| RICH_KIT.contains(&sec.slug));

            let mut poor = SceneCharacter::for_seed(s);
            poor.theme = ThemeArchetype::FeudalJapan;
            poor.prosperity = 0.05;
            let p = Settlement::from_scene(&poor, s);
            assert_eq!(p.landmark.slug, "minka", "poor feudal-japan landmark");
            for m in std::iter::once(&p.landmark)
                .chain(&p.secondaries)
                .chain(&p.props)
            {
                assert!(jp_member(m.slug), "poor feudal-japan member {}", m.slug);
                assert!(
                    !RICH_KIT.contains(&m.slug),
                    "poor room grew the established kit: {}",
                    m.slug
                );
            }
            poor_placed_rice_shed |= p.secondaries.iter().any(|sec| sec.slug == "rice_shed");
        }
        assert!(
            rich_placed_secondary,
            "some rich feudal-japan room places an established secondary"
        );
        assert!(
            poor_placed_rice_shed,
            "some poor feudal-japan room places the rice shed"
        );
    }

    #[test]
    fn mesoamerican_settlement_uses_its_own_kit_by_prosperity() {
        // The per-theme poor/rich pattern (#433/#396): an affluent room grows
        // the monumental temple kit, a destitute one the commoner kit — the
        // two registers never cross. (The shared, band-agnostic `ziggurat` is
        // a legitimate Mesoamerican landmark in either, so assert by register
        // exclusion rather than an exact slug.)
        const POOR_KIT: [&str; 3] = ["adobe_hut", "maize_granary", "clay_pots"];
        const RICH_KIT: [&str; 8] = [
            "step_pyramid",
            "ball_court",
            "shrine",
            "stela",
            "skull_rack",
            "idol",
            "fire_bowl",
            "calendar_stone",
        ];

        let meso_member = |slug: &str| {
            by_slug(slug)
                .expect("member resolves")
                .themes()
                .contains(&ThemeArchetype::Mesoamerican)
        };

        let mut rich_placed_secondary = false;
        let mut poor_placed_granary = false;
        for s in 0u64..32 {
            let mut rich = SceneCharacter::for_seed(s);
            rich.theme = ThemeArchetype::Mesoamerican;
            rich.prosperity = 0.95;
            let r = Settlement::from_scene(&rich, s);
            for m in std::iter::once(&r.landmark)
                .chain(&r.secondaries)
                .chain(&r.props)
            {
                assert!(meso_member(m.slug), "rich mesoamerican member {}", m.slug);
                assert!(
                    !POOR_KIT.contains(&m.slug),
                    "rich room grew the poor kit: {}",
                    m.slug
                );
            }
            rich_placed_secondary |= r.secondaries.iter().any(|sec| RICH_KIT.contains(&sec.slug));

            let mut poor = SceneCharacter::for_seed(s);
            poor.theme = ThemeArchetype::Mesoamerican;
            poor.prosperity = 0.05;
            let p = Settlement::from_scene(&poor, s);
            for m in std::iter::once(&p.landmark)
                .chain(&p.secondaries)
                .chain(&p.props)
            {
                assert!(meso_member(m.slug), "poor mesoamerican member {}", m.slug);
                assert!(
                    !RICH_KIT.contains(&m.slug),
                    "poor room grew the established kit: {}",
                    m.slug
                );
            }
            poor_placed_granary |= p.secondaries.iter().any(|sec| sec.slug == "maize_granary");
        }
        assert!(
            rich_placed_secondary,
            "some rich mesoamerican room places an established secondary"
        );
        assert!(
            poor_placed_granary,
            "some poor mesoamerican room places the maize granary"
        );
    }

    #[test]
    fn modern_city_settlement_uses_its_own_kit_by_prosperity() {
        // The per-theme poor/rich pattern (#433/#397): an affluent room grows
        // the glass-and-concrete downtown, a destitute one the inner-city kit
        // — the two registers never cross.
        const POOR_KIT: [&str; 3] = ["tenement", "corner_store", "trash_bags"];
        const RICH_KIT: [&str; 8] = [
            "glass_skyscraper",
            "office_block",
            "parking_garage",
            "transit_stop",
            "street_lamp",
            "traffic_light",
            "parked_car",
            "dumpster",
        ];

        let city_member = |slug: &str| {
            by_slug(slug)
                .expect("member resolves")
                .themes()
                .contains(&ThemeArchetype::ModernCity)
        };

        let mut rich_placed_secondary = false;
        let mut poor_placed_store = false;
        for s in 0u64..32 {
            let mut rich = SceneCharacter::for_seed(s);
            rich.theme = ThemeArchetype::ModernCity;
            rich.prosperity = 0.95;
            let r = Settlement::from_scene(&rich, s);
            assert_eq!(
                r.landmark.slug, "glass_skyscraper",
                "rich modern-city landmark"
            );
            for m in std::iter::once(&r.landmark)
                .chain(&r.secondaries)
                .chain(&r.props)
            {
                assert!(city_member(m.slug), "rich modern-city member {}", m.slug);
                assert!(
                    !POOR_KIT.contains(&m.slug),
                    "rich room grew the poor kit: {}",
                    m.slug
                );
            }
            rich_placed_secondary |= r.secondaries.iter().any(|sec| RICH_KIT.contains(&sec.slug));

            let mut poor = SceneCharacter::for_seed(s);
            poor.theme = ThemeArchetype::ModernCity;
            poor.prosperity = 0.05;
            let p = Settlement::from_scene(&poor, s);
            assert_eq!(p.landmark.slug, "tenement", "poor modern-city landmark");
            for m in std::iter::once(&p.landmark)
                .chain(&p.secondaries)
                .chain(&p.props)
            {
                assert!(city_member(m.slug), "poor modern-city member {}", m.slug);
                assert!(
                    !RICH_KIT.contains(&m.slug),
                    "poor room grew the established kit: {}",
                    m.slug
                );
            }
            poor_placed_store |= p.secondaries.iter().any(|sec| sec.slug == "corner_store");
        }
        assert!(
            rich_placed_secondary,
            "some rich modern-city room places an established secondary"
        );
        assert!(
            poor_placed_store,
            "some poor modern-city room places the corner store"
        );
    }

    #[test]
    fn suburban_settlement_uses_its_own_kit_by_prosperity() {
        // The per-theme poor/rich pattern (#433/#398): an affluent room grows
        // the tidy neighbourhood, a destitute one the trailer lot — the two
        // registers never cross.
        const POOR_KIT: [&str; 3] = ["trailer_home", "carport", "yard_junk"];
        const RICH_KIT: [&str; 8] = [
            "community_center",
            "suburban_house",
            "detached_garage",
            "mini_mart",
            "picket_fence",
            "mailbox",
            "minivan",
            "swing_set",
        ];

        let sub_member = |slug: &str| {
            by_slug(slug)
                .expect("member resolves")
                .themes()
                .contains(&ThemeArchetype::Suburban)
        };

        let mut rich_placed_secondary = false;
        let mut poor_placed_carport = false;
        for s in 0u64..32 {
            let mut rich = SceneCharacter::for_seed(s);
            rich.theme = ThemeArchetype::Suburban;
            rich.prosperity = 0.95;
            let r = Settlement::from_scene(&rich, s);
            assert_eq!(
                r.landmark.slug, "community_center",
                "rich suburban landmark"
            );
            for m in std::iter::once(&r.landmark)
                .chain(&r.secondaries)
                .chain(&r.props)
            {
                assert!(sub_member(m.slug), "rich suburban member {}", m.slug);
                assert!(
                    !POOR_KIT.contains(&m.slug),
                    "rich room grew the poor kit: {}",
                    m.slug
                );
            }
            rich_placed_secondary |= r.secondaries.iter().any(|sec| RICH_KIT.contains(&sec.slug));

            let mut poor = SceneCharacter::for_seed(s);
            poor.theme = ThemeArchetype::Suburban;
            poor.prosperity = 0.05;
            let p = Settlement::from_scene(&poor, s);
            assert_eq!(p.landmark.slug, "trailer_home", "poor suburban landmark");
            for m in std::iter::once(&p.landmark)
                .chain(&p.secondaries)
                .chain(&p.props)
            {
                assert!(sub_member(m.slug), "poor suburban member {}", m.slug);
                assert!(
                    !RICH_KIT.contains(&m.slug),
                    "poor room grew the established kit: {}",
                    m.slug
                );
            }
            poor_placed_carport |= p.secondaries.iter().any(|sec| sec.slug == "carport");
        }
        assert!(
            rich_placed_secondary,
            "some rich suburban room places an established secondary"
        );
        assert!(
            poor_placed_carport,
            "some poor suburban room places the carport"
        );
    }

    #[test]
    fn ancient_theme_sometimes_places_secondaries() {
        let any = (0u64..64).any(|s| {
            let mut scene = SceneCharacter::for_seed(s);
            scene.theme = ThemeArchetype::AncientClassical;
            !Settlement::from_scene(&scene, s).secondaries.is_empty()
        });
        assert!(
            any,
            "AncientClassical has secondary entries; some room should place them"
        );
    }

    #[test]
    fn richer_settlements_are_denser() {
        // Same room seed and theme, only prosperity differs: the prop count
        // bands don't overlap (poor 1–3, rich 4–6), so a rich room always
        // out-densities its poor twin, and never has fewer secondaries.
        for s in 0u64..24 {
            let mut poor = SceneCharacter::for_seed(s);
            poor.theme = ThemeArchetype::AncientClassical;
            poor.prosperity = 0.05;
            poor.escalation = 0.5;
            let mut rich = poor;
            rich.prosperity = 0.95;

            let p = Settlement::from_scene(&poor, s);
            let r = Settlement::from_scene(&rich, s);
            assert!(
                r.props.len() > p.props.len(),
                "rich should have more props (seed {s}): {} vs {}",
                r.props.len(),
                p.props.len()
            );
            assert!(
                r.secondaries.len() >= p.secondaries.len(),
                "rich should not have fewer secondaries (seed {s})"
            );
        }
    }

    #[test]
    fn conflict_rooms_place_conflict_props() {
        // A conflict room draws from the escalation-Conflict civic pool, so
        // across seeds at least one places a barricade / sandbag / etc.
        let conflict = ["barricade", "sandbag_wall", "watch_post", "wreckage"];
        let any = (0u64..40).any(|s| {
            let mut scene = SceneCharacter::for_seed(s);
            scene.theme = ThemeArchetype::Medieval;
            scene.prosperity = 0.5;
            scene.escalation = 0.95;
            Settlement::from_scene(&scene, s)
                .props
                .iter()
                .any(|m| conflict.contains(&m.slug))
        });
        assert!(any, "some conflict room should place a conflict prop");
    }
}
