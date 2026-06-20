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

    /// One theme's prosperity-register expectations for
    /// [`theme_uses_its_own_kit_by_prosperity`].
    struct KitCase {
        theme: ThemeArchetype,
        /// Established (Modest–Rich) theme-exclusive slugs — a poor room
        /// never grows these.
        rich_kit: &'static [&'static str],
        /// Destitute (Poor) theme-exclusive slugs — an affluent room never
        /// grows these.
        poor_kit: &'static [&'static str],
        /// Expected landmark of a rich room. `None` for themes that share a
        /// band-agnostic landmark (e.g. `stone_circle`, `ziggurat`,
        /// `lighthouse`) across both registers, which assert by register
        /// exclusion alone.
        rich_landmark: Option<&'static str>,
        /// Expected landmark of a poor room, likewise.
        poor_landmark: Option<&'static str>,
        /// A specific poor secondary some poor room must place; `None`
        /// asserts only that *some* poor secondary is placed.
        poor_secondary_witness: Option<&'static str>,
    }

    /// The poor/rich kit register for every theme (#433/#394–#413/#460). The
    /// `rich_kit` / `poor_kit` slugs are the theme-exclusive established /
    /// destitute entries — a band-agnostic shared landmark sits in neither.
    const KIT_CASES: &[KitCase] = &[
        KitCase {
            theme: ThemeArchetype::Cyberpunk,
            rich_kit: &[
                "neon_megatower",
                "data_spire",
                "arcade_block",
                "holo_billboard",
                "parking_stack",
                "neon_kiosk",
                "drone_perch",
                "cable_arch",
            ],
            poor_kit: &[
                "scrap_shanty",
                "container_stack",
                "tarp_shelter",
                "ewaste_pile",
                "busted_terminal",
            ],
            rich_landmark: Some("neon_megatower"),
            poor_landmark: Some("scrap_shanty"),
            poor_secondary_witness: None,
        },
        KitCase {
            theme: ThemeArchetype::Nordic,
            rich_kit: &[
                "mead_hall",
                "boathouse",
                "signal_beacon",
                "rune_stones",
                "longship",
                "shield_rack",
                "drying_rack",
                "totem_pole",
            ],
            poor_kit: &["turf_house", "sod_shelter", "wood_pile"],
            rich_landmark: None,
            poor_landmark: None,
            poor_secondary_witness: Some("sod_shelter"),
        },
        KitCase {
            theme: ThemeArchetype::Medieval,
            rich_kit: &[
                "medieval_castle",
                "watchtower",
                "chapel",
                "blacksmith",
                "market_hall",
                "well_house",
                "handcart",
                "barrel_stack",
                "trade_stall",
                "banner_pole",
            ],
            poor_kit: &["wattle_hovel", "lean_to", "kindling_pile"],
            rich_landmark: Some("medieval_castle"),
            poor_landmark: Some("wattle_hovel"),
            poor_secondary_witness: Some("lean_to"),
        },
        KitCase {
            theme: ThemeArchetype::FeudalJapan,
            rich_kit: &[
                "pagoda",
                "torii_gate",
                "tea_house",
                "dojo",
                "stone_lantern",
                "koi_pond",
                "bamboo_fence",
                "bonsai",
            ],
            poor_kit: &["minka", "rice_shed", "straw_bales"],
            rich_landmark: Some("pagoda"),
            poor_landmark: Some("minka"),
            poor_secondary_witness: Some("rice_shed"),
        },
        KitCase {
            theme: ThemeArchetype::Mesoamerican,
            rich_kit: &[
                "step_pyramid",
                "ball_court",
                "shrine",
                "stela",
                "skull_rack",
                "idol",
                "fire_bowl",
                "calendar_stone",
            ],
            poor_kit: &["adobe_hut", "maize_granary", "clay_pots"],
            rich_landmark: None,
            poor_landmark: None,
            poor_secondary_witness: Some("maize_granary"),
        },
        KitCase {
            theme: ThemeArchetype::ModernCity,
            rich_kit: &[
                "glass_skyscraper",
                "office_block",
                "parking_garage",
                "transit_stop",
                "street_lamp",
                "traffic_light",
                "parked_car",
                "dumpster",
            ],
            poor_kit: &["tenement", "corner_store", "trash_bags"],
            rich_landmark: Some("glass_skyscraper"),
            poor_landmark: Some("tenement"),
            poor_secondary_witness: Some("corner_store"),
        },
        KitCase {
            theme: ThemeArchetype::Suburban,
            rich_kit: &[
                "community_center",
                "suburban_house",
                "detached_garage",
                "mini_mart",
                "picket_fence",
                "mailbox",
                "minivan",
                "swing_set",
            ],
            poor_kit: &["trailer_home", "carport", "yard_junk"],
            rich_landmark: Some("community_center"),
            poor_landmark: Some("trailer_home"),
            poor_secondary_witness: Some("carport"),
        },
        KitCase {
            theme: ThemeArchetype::RuralFarmland,
            rich_kit: &[
                "barn",
                "farmhouse",
                "grain_silo",
                "windmill",
                "greenhouse",
                "tractor",
                "hay_bales",
                "scarecrow",
                "rail_fence",
            ],
            poor_kit: &["homestead_shack", "pole_barn", "farm_junk"],
            rich_landmark: Some("barn"),
            poor_landmark: Some("homestead_shack"),
            poor_secondary_witness: Some("pole_barn"),
        },
        KitCase {
            theme: ThemeArchetype::IndustrialPark,
            rich_kit: &[
                "factory",
                "cooling_tower",
                "loading_dock",
                "tank_farm",
                "shipping_containers",
                "pipe_run",
                "pallet_stack",
                "floodlight",
            ],
            poor_kit: &["derelict_shed", "rusted_tank", "scrap_heap"],
            rich_landmark: Some("factory"),
            poor_landmark: Some("derelict_shed"),
            poor_secondary_witness: Some("rusted_tank"),
        },
        KitCase {
            theme: ThemeArchetype::CoastalResort,
            rich_kit: &[
                "grand_hotel",
                "resort_pier",
                "beach_house",
                "boardwalk_shops",
                "lifeguard_tower",
                "beach_umbrella",
                "deck_chair",
                "dinghy",
                "buoy",
            ],
            poor_kit: &["fishing_shack", "bait_stand", "crab_traps"],
            rich_landmark: None,
            poor_landmark: Some("fishing_shack"),
            poor_secondary_witness: Some("bait_stand"),
        },
        KitCase {
            theme: ThemeArchetype::Roadside,
            rich_kit: &[
                "gas_station",
                "roadside_diner",
                "motel",
                "billboard",
                "fuel_pump",
                "road_sign",
                "traffic_cone",
                "vending_machine",
                "guardrail",
            ],
            poor_kit: &["produce_stand", "boarded_shack", "oil_drums"],
            rich_landmark: Some("gas_station"),
            poor_landmark: Some("produce_stand"),
            poor_secondary_witness: Some("boarded_shack"),
        },
        KitCase {
            theme: ThemeArchetype::CivicCampus,
            rich_kit: &[
                "town_hall",
                "library",
                "lecture_hall",
                "dormitory",
                "clock_tower",
                "flagpole",
                "bike_rack",
                "notice_board",
                "campus_lamp",
            ],
            poor_kit: &["portable_classroom", "bus_shelter", "recycling_bins"],
            rich_landmark: Some("town_hall"),
            poor_landmark: Some("portable_classroom"),
            poor_secondary_witness: Some("bus_shelter"),
        },
        KitCase {
            theme: ThemeArchetype::SportsRec,
            rich_kit: &[
                "stadium",
                "gym",
                "bleachers",
                "ticket_booth",
                "clubhouse",
                "goalpost",
                "floodlight_mast",
                "scoreboard",
                "players_bench",
            ],
            poor_kit: &["rec_court", "backstop", "tire_stack"],
            rich_landmark: Some("stadium"),
            poor_landmark: Some("rec_court"),
            poor_secondary_witness: Some("backstop"),
        },
        KitCase {
            theme: ThemeArchetype::Steampunk,
            rich_kit: &[
                "cog_tower",
                "airship_dock",
                "foundry",
                "pump_house",
                "pipework",
                "pressure_tank",
                "gear_pile",
                "gas_lamp",
                "coal_hopper",
            ],
            poor_kit: &["tinkerers_shack", "scrap_boiler", "cog_scrap"],
            rich_landmark: Some("cog_tower"),
            poor_landmark: Some("tinkerers_shack"),
            poor_secondary_witness: Some("scrap_boiler"),
        },
        KitCase {
            theme: ThemeArchetype::Solarpunk,
            rich_kit: &[
                "biodome",
                "green_pavilion",
                "wind_turbine",
                "vertical_farm",
                "solar_panel",
                "veggie_planter",
                "water_channel",
                "solar_lamp",
                "beehive",
            ],
            poor_kit: &["cob_roundhouse", "poly_tunnel", "compost_heap"],
            rich_landmark: Some("biodome"),
            poor_landmark: Some("cob_roundhouse"),
            poor_secondary_witness: Some("poly_tunnel"),
        },
        KitCase {
            theme: ThemeArchetype::SpaceOutpost,
            rich_kit: &[
                "habitat_dome",
                "solar_array",
                "comms_dish",
                "landing_pad",
                "hydroponics",
                "rover",
                "cargo_crate",
                "beacon",
                "airlock",
            ],
            poor_kit: &["crash_shelter", "solar_wreck", "scrap_canister"],
            rich_landmark: Some("habitat_dome"),
            poor_landmark: Some("crash_shelter"),
            poor_secondary_witness: Some("solar_wreck"),
        },
        KitCase {
            theme: ThemeArchetype::Fantasy,
            rich_kit: &[
                "wizard_tower",
                "enchanted_library",
                "fae_ring",
                "crystal_shrine",
                "runestone",
                "glow_mushroom",
                "spell_circle",
                "mana_font",
                "crystal_cluster",
            ],
            poor_kit: &["hedge_hut", "standing_stone", "toadstool_ring"],
            rich_landmark: Some("wizard_tower"),
            poor_landmark: Some("hedge_hut"),
            poor_secondary_witness: Some("standing_stone"),
        },
        KitCase {
            theme: ThemeArchetype::GothicHorror,
            rich_kit: &[
                "cathedral",
                "mausoleum",
                "cemetery",
                "bell_tower",
                "gravestone",
                "gargoyle",
                "dead_tree",
                "iron_fence",
                "stone_cross",
            ],
            poor_kit: &["ruined_chapel", "pauper_graves", "bone_pile"],
            rich_landmark: Some("cathedral"),
            poor_landmark: Some("ruined_chapel"),
            poor_secondary_witness: Some("pauper_graves"),
        },
        KitCase {
            theme: ThemeArchetype::AlienOrganic,
            rich_kit: &[
                "chitinous_hive",
                "pod_cluster",
                "fleshy_spire",
                "membrane_wall",
                "egg_sac",
                "biolume_stalk",
                "tendril",
                "spore_vent",
                "creep_patch",
            ],
            poor_kit: &["withered_hive", "husk_pods", "rot_patch"],
            rich_landmark: Some("chitinous_hive"),
            poor_landmark: Some("withered_hive"),
            poor_secondary_witness: Some("husk_pods"),
        },
        KitCase {
            theme: ThemeArchetype::AlienMonolithic,
            rich_kit: &[
                "black_monolith",
                "levitating_platform",
                "light_pylon",
                "glyph_arch",
                "floating_cube",
                "glyph_stone",
                "energy_node",
                "monolith_shard",
                "light_disc",
            ],
            poor_kit: &["broken_monolith", "dead_pylon", "glyph_rubble"],
            rich_landmark: Some("black_monolith"),
            poor_landmark: Some("broken_monolith"),
            poor_secondary_witness: Some("dead_pylon"),
        },
        KitCase {
            theme: ThemeArchetype::PostApoc,
            rich_kit: &[
                "fortified_ruin",
                "salvage_shack",
                "radio_mast",
                "fuel_depot",
                "wrecked_car",
                "scrap_wall",
                "fuel_barrels",
                "tire_wall",
                "signal_fire",
            ],
            poor_kit: &["survivor_lean_to", "rubble_barricade", "ash_pit"],
            rich_landmark: Some("fortified_ruin"),
            poor_landmark: Some("survivor_lean_to"),
            poor_secondary_witness: Some("rubble_barricade"),
        },
        KitCase {
            theme: ThemeArchetype::WildWest,
            rich_kit: &[
                "saloon",
                "water_tower",
                "church",
                "jail",
                "general_store",
                "hitching_post",
                "wagon",
                "frontier_fence",
                "wind_pump",
            ],
            poor_kit: &["prospector_shack", "boot_hill", "tumbleweed"],
            rich_landmark: Some("saloon"),
            poor_landmark: Some("prospector_shack"),
            poor_secondary_witness: Some("boot_hill"),
        },
    ];

    #[test]
    fn theme_uses_its_own_kit_by_prosperity() {
        // The per-theme poor/rich pattern (#433/#394–#413/#460): an affluent
        // room grows the theme's established kit, a destitute one its poor kit
        // — the two registers never cross, and where a theme pins a landmark it
        // always heads its settlement. (Was 22 near-identical per-theme tests.)
        for case in KIT_CASES {
            let theme_member = |slug: &str| {
                by_slug(slug)
                    .expect("member resolves")
                    .themes()
                    .contains(&case.theme)
            };
            let mut rich_placed_secondary = false;
            let mut poor_witness_placed = false;
            for s in 0u64..32 {
                let mut rich = SceneCharacter::for_seed(s);
                rich.theme = case.theme;
                rich.prosperity = 0.95;
                let r = Settlement::from_scene(&rich, s);
                if let Some(lm) = case.rich_landmark {
                    assert_eq!(r.landmark.slug, lm, "{:?} rich landmark", case.theme);
                }
                for m in std::iter::once(&r.landmark)
                    .chain(&r.secondaries)
                    .chain(&r.props)
                {
                    assert!(
                        theme_member(m.slug),
                        "{:?} rich member {}",
                        case.theme,
                        m.slug
                    );
                    assert!(
                        !case.poor_kit.contains(&m.slug),
                        "{:?} rich room grew the poor kit: {}",
                        case.theme,
                        m.slug
                    );
                }
                rich_placed_secondary |= r
                    .secondaries
                    .iter()
                    .any(|sec| case.rich_kit.contains(&sec.slug));

                let mut poor = SceneCharacter::for_seed(s);
                poor.theme = case.theme;
                poor.prosperity = 0.05;
                let p = Settlement::from_scene(&poor, s);
                if let Some(lm) = case.poor_landmark {
                    assert_eq!(p.landmark.slug, lm, "{:?} poor landmark", case.theme);
                }
                for m in std::iter::once(&p.landmark)
                    .chain(&p.secondaries)
                    .chain(&p.props)
                {
                    assert!(
                        theme_member(m.slug),
                        "{:?} poor member {}",
                        case.theme,
                        m.slug
                    );
                    assert!(
                        !case.rich_kit.contains(&m.slug),
                        "{:?} poor room grew the established kit: {}",
                        case.theme,
                        m.slug
                    );
                }
                poor_witness_placed |= match case.poor_secondary_witness {
                    Some(w) => p.secondaries.iter().any(|sec| sec.slug == w),
                    None => !p.secondaries.is_empty(),
                };
            }
            assert!(
                rich_placed_secondary,
                "{:?}: some rich room places an established secondary",
                case.theme
            );
            assert!(
                poor_witness_placed,
                "{:?}: some poor room places its poor-kit witness",
                case.theme
            );
        }
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
