//! Seeded mini-settlement spec - every home region grows a themed
//! cluster of catalogue structures near spawn: one landmark, a few
//! secondary buildings ringed around it, and scatter props.
//!
//! Members are resolved by querying the catalogue
//! ([`crate::catalogue::entries_for`]) for entries tagged with the room's
//! [`ThemeArchetype`] and the matching [`StructureRole`], rather than a
//! hardcoded slug pool - so adding a themed catalogue entry grows the
//! settlements automatically. A theme with no landmark entry yet falls
//! back wholesale to [`FALLBACK_THEME`], so every room gets a coherent
//! settlement while the catalogue fills out.
//!
//! Placement on no terrain ([`Settlement::from_scene`], the flat ring the
//! sited plan grew from): the landmark sits at a footprint-appropriate
//! distance band facing spawn; secondaries fan out on the *far* side of the
//! landmark (so they never crowd the spawn square), their fronts turned
//! away from it; props scatter across the settlement's far hemisphere. The
//! wiring layer
//! ([`RoomRecord::default_for_did`](crate::pds::RoomRecord::default_for_did))
//! turns each member into a named generator plus a terrain-snapped
//! `Placement::Absolute` carrying the member's water clearance and, for a
//! Shape-grammar building, its grammar seed.
//!
//! Sited on a terrain probe ([`SettlementPlan::from_scene_sited`]), each
//! member stands only where its ground fits it (#1773): the relief under
//! its footprint, read on the proxy with a margin for what the proxy
//! cannot see, within its plinth's reach; its middle and rim gentle enough
//! that the compile's slope walk leaves it there; and dry. A member that
//! fits nowhere near its place is drawn a quarter octave smaller each step,
//! down to half size, then gives way to the next smaller entry of its role
//! (#1774), and is dropped after that - all but the primary landmark, which
//! always stands: the smallest at half size on its region's flattest
//! ground. A landmark takes at most half its region's room, leaving as much
//! again for its kit, and the plan keeps the forecourt before it clear.
//!
//! Where a building belongs, not only whether it fits (#1775): one whose
//! entry seeks a crest, the shore or a valley floor
//! ([`CatalogueEntry::site`](crate::catalogue::CatalogueEntry::site)) takes,
//! of the sites it fits near its place, the one that ground suits best less
//! the walk. A landmark faces the arrival; a house fronts downhill on a
//! slope - along the contour, out over its view - and toward the arrival
//! on level ground.

use std::f32::consts::TAU;

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use super::siting::{BuildableRegion, FootprintGround, TerrainProbe};
use super::viewpoint::Viewpoint;
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
/// Uniform-scale band of a secondary building (a hamlet's anchor too).
const SECONDARY_SCALE_BAND: (f32, f32) = (0.80, 1.10);
/// The band a prop's scale is still drawn from and then left unapplied (see
/// [`SettlementMember::scale`]): the draw stays so that every draw after it -
/// and so every seeded world - keeps its place in the stream.
const PROP_SCALE_ROLL: (f32, f32) = (0.70, 1.05);

/// A secondary's keep-clear radius before its scale is drawn - at the top of
/// [`SECONDARY_SCALE_BAND`], so it is spaced for the largest it can be drawn
/// (its sites are listed before its scale is drawn; drawing it first would
/// move every seeded world's later draws). Whatever stands in for it is
/// spaced the same.
fn secondary_spacing_clearance(clearance: f32) -> f32 {
    clearance * SECONDARY_SCALE_BAND.1
}

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
    /// Uniform scale the member is drawn at. The build puts it on the
    /// member generator's root prim, scaled about the ground point (#1454:
    /// an item is scaled by its root, not by its placement). Always 1.0 for
    /// a prop: every copy of a prop slug shares one generator, so a per-copy
    /// scale could never be drawn.
    pub scale: f32,
    /// Replacement seed for Shape-grammar entries' stochastic rules,
    /// written on the member's placement (#1514), and the seed of its ruin.
    pub grammar_seed: u64,
    /// Keep-clear radius (m) at the size the member is drawn - its
    /// [`crate::catalogue::Footprint::clearance`] times [`Self::scale`] -
    /// for the compiler's water-avoidance walk and for spacing everything
    /// placed round it (the gate, the secondaries, the props). Spaced by the
    /// unscaled radius, a 1.45x landmark stood up to 17 m over its own gate
    /// once it was drawn at its scale (#1463).
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
/// empty pool. Props don't use this - their cross-theme tier props ride the
/// always-present civic kit, so [`entries_for_room`] suffices there.
///
/// A theme's street buildings are never members (#1598): they are Berlin's
/// street types, built to stand flush in a row along a street, and a
/// settlement stands its members apart on open ground.
fn tiered_pool(
    theme: ThemeArchetype,
    role: StructureRole,
    prosperity: ProsperityTier,
    escalation: EscalationTier,
) -> Vec<&'static dyn crate::catalogue::CatalogueEntry> {
    let unstreeted = |e: &&'static dyn crate::catalogue::CatalogueEntry| e.street().is_none();
    let tiered: Vec<_> = entries_for_room(theme, role, prosperity, escalation)
        .filter(unstreeted)
        .collect();
    if tiered.is_empty() {
        entries_for(theme, role).filter(unstreeted).collect()
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
    let scale = range_f32(rng, scale_lo, scale_hi);
    SettlementMember {
        slug: entry.slug(),
        offset,
        yaw_rad,
        scale,
        grammar_seed: rng.next_u64(),
        clearance: fp.clearance * scale,
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
        let r = landmark.clearance
            + secondary_spacing_clearance(fp.clearance)
            + range_f32(rng, 4.0, 12.0);
        let offset = [
            landmark.offset[0] + dir.sin() * r,
            landmark.offset[1] + dir.cos() * r,
        ];
        // Face the landmark centre (±0.25 rad jitter).
        let yaw_rad = (landmark.offset[0] - offset[0]).atan2(landmark.offset[1] - offset[1])
            + range_f32(rng, -0.25, 0.25);

        let scale = range_f32(rng, SECONDARY_SCALE_BAND.0, SECONDARY_SCALE_BAND.1);
        out.push(SettlementMember {
            slug: entry.slug(),
            offset,
            yaw_rad,
            scale,
            grammar_seed: rng.next_u64(),
            clearance: fp.clearance * scale,
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
        // Props are clutter - sampled with replacement.
        let entry = pick(&pool, rng);
        let fp = entry.footprint();
        // Keep props on the settlement (far) hemisphere too.
        let dir = base + range_f32(rng, -1.4, 1.4);
        let r = range_f32(rng, landmark.clearance + 2.0, radius);
        let offset = [
            landmark.offset[0] + dir.sin() * r,
            landmark.offset[1] + dir.cos() * r,
        ];

        let yaw_rad = unit_f32(rng) * TAU;
        let _unapplied = range_f32(rng, PROP_SCALE_ROLL.0, PROP_SCALE_ROLL.1);
        out.push(SettlementMember {
            slug: entry.slug(),
            offset,
            yaw_rad,
            scale: 1.0,
            grammar_seed: rng.next_u64(),
            clearance: fp.clearance,
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Terrain-aware settlement plan (#905)
// ---------------------------------------------------------------------------

/// Slope limit (rise/run) for buildable ground - the threshold the
/// wiring layer hands to [`TerrainProbe::new`] when segmenting the
/// derive-time proxy. ~16°: gentle hillside, comfortably under what a
/// snapped foundation skirt can absorb. Deliberately conservative
/// because the proxy under-reads fine-scale steepness (box-averaged).
pub const BUILD_SLOPE_LIMIT: f32 = 0.28;

/// Fraction of a region's raw area treated as actually fillable with
/// structures - the rest is breathing room between footprints.
const FILL_FRACTION: f32 = 0.35;

/// A member's desired position may move at most this far (m) to reach a
/// buildable cell its ground fits; anything farther means the layout
/// doesn't fit the region there and the member is dropped instead of
/// smeared.
const SNAP_MAX_DIST: f32 = 30.0;

/// Minimum region area (m²) for a hamlet (secondary + props, no
/// landmark) to grow in a leftover region.
const HAMLET_MIN_AREA: f32 = 400.0;

/// At most this many hamlets beyond the clusters.
const MAX_HAMLETS: usize = 2;

/// Room-wide ceiling on settlement members across every cluster.
const MAX_TOTAL_MEMBERS: usize = 20;

/// Sub-stream tweak so the sited plan draws from its own RNG stream,
/// decoupled from the legacy flat-ring deriver.
const SITED_STREAM_TWEAK: u64 = 0x517E_D000;

/// One placed cluster: landmark-led (`landmark: Some`) or a hamlet.
#[derive(Clone, Debug)]
pub struct SettlementCluster {
    pub landmark: Option<SettlementMember>,
    pub secondaries: Vec<SettlementMember>,
    pub props: Vec<SettlementMember>,
}

impl SettlementCluster {
    fn member_count(&self) -> usize {
        usize::from(self.landmark.is_some()) + self.secondaries.len() + self.props.len()
    }
}

/// The terrain-aware settlement for a room: one primary landmark-led
/// cluster (always present), optionally a second landmark cluster on
/// naturally-partitioned landforms (archipelago islands, valley
/// basins), plus small hamlets in leftover buildable regions.
#[derive(Clone, Debug)]
pub struct SettlementPlan {
    pub clusters: Vec<SettlementCluster>,
    /// Where the primary landmark stood before its ground was read (#1773):
    /// its region's cell nearest spawn clear of its spawn square. A Berlin
    /// square's gate keeps that bearing (#1589), its fixture pinned: the fit
    /// moves the procedural landmark, never the square's gate.
    pub unfitted_landmark: [f32; 2],
    /// Where a visitor arrives (#1777): a viewpoint out at the settlement's
    /// edge in view of the primary landmark, which the build stands a world's
    /// own landing, gate and monument at - `None` where none fits, and the
    /// forecourt stands in front of the landmark as it always did. A Berlin
    /// square's build reads none of it.
    pub viewpoint: Option<Viewpoint>,
    /// How tall the primary landmark stands as the build draws it, metres
    /// over its ground ([`landmark_top`]) - where its viewpoint was sought,
    /// else 0 - which the plan's merit reads ([`Self::best_sited`], #1778).
    landmark_top: f32,
}

impl SettlementPlan {
    /// The primary cluster's landmark - present by construction (the
    /// primary cluster always carries one, even on the honest-adaptation
    /// fallback path).
    pub fn primary_landmark(&self) -> &SettlementMember {
        self.clusters[0]
            .landmark
            .as_ref()
            .expect("primary cluster always carries a landmark")
    }

    /// Derive the plan against a segmented terrain probe. Deterministic
    /// from `(scene, room_seed, probe)`; the probe itself is
    /// deterministic from the seeded terrain config, so peers agree.
    ///
    /// Placement policy (#905, user-picked): prefer buildable regions
    /// near spawn but follow the flat land out when the near ground is
    /// hostile; shrink honestly when flat land is scarce (worst case a
    /// lone landmark on the least-bad site - terrain is never modified);
    /// Archipelago / Valleys rooms may grow a second full landmark
    /// cluster, every landform may grow hamlets, all under
    /// [`MAX_TOTAL_MEMBERS`].
    pub fn from_scene_sited(scene: &SceneCharacter, room_seed: u64, probe: &TerrainProbe) -> Self {
        Self::sited_on_stream(scene, room_seed, probe, 0)
    }

    /// The best of [`CANDIDATES`] plans for a world on its own ground
    /// (#1778): each drawn as [`Self::from_scene_sited`] draws its plan, from
    /// its own stream of `room_seed`'s draws - the first is that plan - and
    /// the one of most [merit](Self::merit) kept, the first of equals, so a
    /// world whose first plan is its best keeps it. Deterministic from
    /// `(scene, room_seed, probe)`, as each is. A Berlin square's build
    /// reads only the first plan's landmark bearing, and asks for that
    /// alone.
    pub fn best_sited(scene: &SceneCharacter, room_seed: u64, probe: &TerrainProbe) -> Self {
        let mut best: Option<(f32, Self)> = None;
        for stream in 0..CANDIDATES {
            let plan = Self::sited_on_stream(scene, room_seed, probe, stream);
            let merit = plan.merit(probe);
            if best.as_ref().is_none_or(|(kept, _)| merit > *kept) {
                best = Some((merit, plan));
            }
        }
        best.map(|(_, plan)| plan)
            .expect("one plan is drawn at least")
    }

    /// What a plan is worth to its world's first view and to its ground
    /// (#1778): its viewpoint's own score ([`Viewpoint::score`]); how much of
    /// the frame its landmark fills, up to the third of its height the
    /// viewpoint's distance aims at; [`BUILDING_FRAMED`] for each building
    /// and [`PROP_FRAMED`] for each prop the first frame shows besides
    /// ([`Viewpoint::frames`]), up to [`FRAMED_MAX`] of each; less the share
    /// of their plinths' reach its buildings' ground takes on average
    /// ([`member_fits`]). The light is not scored: the world's hour turns
    /// its sun to whichever view is kept (#1769). `f32::MIN` for a plan with
    /// no viewpoint.
    fn merit(&self, probe: &TerrainProbe) -> f32 {
        let Some(view) = self.viewpoint else {
            return f32::MIN;
        };
        let landmark = self.primary_landmark().offset;
        let (dx, dz) = (landmark[0] - view.landing[0], landmark[1] - view.landing[1]);
        let presence = (self.landmark_top * super::viewpoint::VIEW_PER_HEIGHT
            / (dx * dx + dz * dz).sqrt().max(1.0))
        .min(1.0);
        let (mut built, mut props, mut taken, mut buildings) = (0usize, 0usize, 0.0, 0usize);
        for (ci, cluster) in self.clusters.iter().enumerate() {
            let anchor = cluster.landmark.iter().map(|member| (member, ci == 0));
            for (member, looked_at) in anchor.chain(cluster.secondaries.iter().map(|m| (m, false)))
            {
                let Some(entry) = crate::catalogue::by_slug(member.slug) else {
                    continue;
                };
                let radius = ground_radius(entry, member.scale);
                if !looked_at && view.frames(probe, member.offset, radius) {
                    built += 1;
                }
                let ground = probe.footprint(member.offset, radius);
                taken += (ground.relief * RELIEF_MARGIN / reach(entry, member.scale)).min(1.0);
                buildings += 1;
            }
            props += cluster
                .props
                .iter()
                .filter(|prop| view.frames(probe, prop.offset, prop.clearance))
                .count();
        }
        view.score
            + presence
            + BUILDING_FRAMED * built.min(FRAMED_MAX) as f32
            + PROP_FRAMED * props.min(FRAMED_MAX) as f32
            - taken / buildings.max(1) as f32
    }

    /// [`Self::from_scene_sited`] drawn from stream `stream` of the plan's
    /// draws (#1778): stream 0 is the plan's own.
    fn sited_on_stream(
        scene: &SceneCharacter,
        room_seed: u64,
        probe: &TerrainProbe,
        stream: u64,
    ) -> Self {
        let mut rng =
            ChaCha8Rng::seed_from_u64(room_seed ^ SETTLEMENT_STREAM_SALT ^ SITED_STREAM_TWEAK);
        rng.set_stream(stream);
        let theme = effective_theme(scene.theme);
        let prosperity = scene.prosperity_tier();
        let escalation = scene.escalation_tier();

        let regions = probe.regions();

        // Honest-adaptation floor: no buildable region at all → the
        // landmark alone on the least-steep site the room has.
        if regions.is_empty() {
            let pool = tiered_pool(theme, StructureRole::Landmark, prosperity, escalation);
            let entry = pick(&pool, &mut rng);
            let fp = entry.footprint();
            let mut site = probe.least_bad_site();
            // Keep it out of the ±5 m spawn-scatter square; the bearing
            // is preserved so it stays on (or near) its flat cell.
            let d = (site[0].powi(2) + site[1].powi(2)).sqrt();
            if d < 10.0 {
                let scale = if d < 1e-3 { 0.0 } else { 10.0 / d };
                site = if scale == 0.0 {
                    [10.0, 0.0]
                } else {
                    [site[0] * scale, site[1] * scale]
                };
            }
            let yaw_rad = libm::atan2f(site[0], site[1]) + range_f32(&mut rng, -0.35, 0.35);
            let (scale_lo, scale_hi) = landmark_scale_band(prosperity);
            let scale = range_f32(&mut rng, scale_lo, scale_hi);
            return Self {
                clusters: vec![SettlementCluster {
                    landmark: Some(SettlementMember {
                        slug: entry.slug(),
                        offset: site,
                        yaw_rad,
                        scale,
                        grammar_seed: rng.next_u64(),
                        clearance: fp.clearance * scale,
                    }),
                    secondaries: Vec::new(),
                    props: Vec::new(),
                }],
                unfitted_landmark: site,
                viewpoint: None,
                landmark_top: 0.0,
            };
        }

        // Primary region: flat capacity × spawn proximity, so near land
        // wins when adequate and a distant plateau wins over a cramped
        // near ledge.
        let primary_idx = {
            let mut best = 0;
            let mut best_score = f32::MIN;
            for (i, r) in regions.iter().enumerate() {
                let capacity = (r.area_m2).min(8_000.0) / 8_000.0;
                let proximity = 1.0 / (1.0 + r.min_spawn_dist / 80.0);
                let score = capacity * proximity;
                if score > best_score {
                    best_score = score;
                    best = i;
                }
            }
            best
        };

        let mut clusters = Vec::new();
        let mut total = 0usize;
        let Grown {
            cluster: primary,
            unfitted: unfitted_landmark,
            viewpoint,
            top: landmark_top,
        } = place_cluster(
            theme,
            prosperity,
            escalation,
            &regions[primary_idx],
            probe,
            ClusterKind::Primary,
            Forecourt::of(scene.theme),
            None,
            &[],
            MAX_TOTAL_MEMBERS,
            &mut rng,
        );
        let unfitted_landmark = unfitted_landmark
            .expect("the primary cluster always sites its landmark (the fallback theme has one)");
        // Where a visitor arrives, which every other cluster's buildings face
        // (#1775): the primary landmark's viewpoint (#1777), else its
        // forecourt landing - and what it keeps clear, which the other
        // clusters keep clear too, as a viewpoint can stand out by their
        // regions.
        let arrival = match (viewpoint, primary.landmark.as_ref()) {
            (Some(viewpoint), _) => viewpoint.landing,
            (None, Some(landmark)) => Forecourt::of(scene.theme).map_or([0.0, 0.0], |forecourt| {
                forecourt.spot(landmark.offset, landmark.clearance).landing
            }),
            (None, None) => [0.0, 0.0],
        };
        let reserved = match (viewpoint, primary.landmark.as_ref()) {
            (Some(viewpoint), Some(landmark)) => {
                viewpoint.keep_outs(landmark.offset, landmark.clearance)
            }
            _ => Vec::new(),
        };
        total += primary.member_count();
        clusters.push(primary);

        // Extra clusters in leftover regions, largest first (`regions`
        // is already sorted largest-first).
        let allow_second_landmark = matches!(
            scene.landform,
            crate::seeded_defaults::scene::LandformArchetype::Archipelago
                | crate::seeded_defaults::scene::LandformArchetype::Valleys
        );
        let mut second_landmark_placed = false;
        let mut hamlets = 0usize;
        for (i, region) in regions.iter().enumerate() {
            if i == primary_idx || total >= MAX_TOTAL_MEMBERS {
                continue;
            }
            // A second landmark where its own room and ground fit it
            // (#1774) - no longer an 8 m stand-in's - and a hamlet where
            // none does. A region too small for any landmark is not tried,
            // so its hamlet draws as it always did.
            if allow_second_landmark && !second_landmark_placed && landmark_has_room(theme, region)
            {
                let Grown { cluster: c, .. } = place_cluster(
                    theme,
                    prosperity,
                    escalation,
                    region,
                    probe,
                    ClusterKind::Outpost,
                    None,
                    Some(arrival),
                    &reserved,
                    MAX_TOTAL_MEMBERS - total,
                    &mut rng,
                );
                if c.member_count() > 0 {
                    total += c.member_count();
                    second_landmark_placed = true;
                    clusters.push(c);
                    continue;
                }
            }
            if hamlets < MAX_HAMLETS && region.area_m2 >= HAMLET_MIN_AREA {
                let Grown { cluster: c, .. } = place_cluster(
                    theme,
                    prosperity,
                    escalation,
                    region,
                    probe,
                    ClusterKind::Hamlet,
                    None,
                    Some(arrival),
                    &reserved,
                    MAX_TOTAL_MEMBERS - total,
                    &mut rng,
                );
                if c.member_count() > 0 {
                    total += c.member_count();
                    hamlets += 1;
                    clusters.push(c);
                }
            }
        }

        Self {
            clusters,
            unfitted_landmark,
            viewpoint,
            landmark_top,
        }
    }
}

/// Whether `region` has room for any landmark of `theme`: its smallest at
/// half size within [`LANDMARK_ROOM_SHARE`] of the region's room (#1774).
/// Draw-free, so a region that cannot hold one is never tried as an outpost
/// and grows its hamlet on the stream it always did.
fn landmark_has_room(theme: ThemeArchetype, region: &BuildableRegion) -> bool {
    let smallest = entries_for(theme, StructureRole::Landmark)
        .filter(|entry| entry.street().is_none())
        .map(|entry| entry.footprint().clearance)
        .fold(f32::INFINITY, f32::min);
    member_area_cost(smallest * SCALE_MIN) <= region.area_m2 * FILL_FRACTION * LANDMARK_ROOM_SHARE
}

/// Approximate ground area (m²) one member occupies, from its dry-land
/// clearance radius plus breathing room.
fn member_area_cost(clearance: f32) -> f32 {
    let r = clearance + 2.0;
    std::f32::consts::PI * r * r
}

/// The share of its region's room ([`FILL_FRACTION`] of its area) a
/// landmark may take: it leaves as much again for its kit (#1774). The
/// second landmark's old gate asked the same of a fixed 8 m member; the
/// primary landmark was never asked at all.
const LANDMARK_ROOM_SHARE: f32 = 0.5;

/// The smallest a member is drawn to fit its ground, as a share of its
/// catalogue size - Berlin's fit's floor (#1588, `terrain::derived::fit`).
const SCALE_MIN: f32 = 0.5;

/// Each step down in size a member that does not fit takes: a quarter
/// octave, as Berlin's fit steps (#1774).
const SCALE_STEP: f32 = 0.840_896_4;

/// How far a member stands below the ground its footprint's highest point
/// sets it on (`room::build::member_placement`'s -0.35 m): part of what
/// covers the ground falling away under it.
const MEMBER_SINK: f32 = 0.35;

/// How many times the proxy's relief under a footprint the full map is
/// taken to have there (#1773). The proxy samples the ground ten metres
/// apart and erodes its own gullies, so it reads a site flatter than it
/// is: over 72 000 random sites in 200 seeds the full map's drop was 1.1x
/// the proxy's relief at the median and 2.2x at p90. At 2.5, 97.7 % of the
/// seeded footprints in seeds 0-299 show no daylight (91.6 % before), for
/// 5 % fewer members than at 1.5. Craggy ground keeps most of the rest:
/// its fine relief lies under the proxy's cells, and no margin the proxy
/// can read sorts its sites without emptying them.
const RELIEF_MARGIN: f32 = 2.5;

/// The steepest the ground may be at a member's middle and its rim, read on
/// the proxy as the compile's walk reads it on the full map
/// (`world_builder::compile::slope`, 0.45): a margin under the walk's
/// limit, so the walk leaves a site the plan chose where it is.
const SITE_SLOPE: f32 = 0.35;

/// How much further than the nearest site it fits a building with a site
/// preference looks for one it prefers (m, #1775).
const PREFERENCE_REACH: f32 = 150.0;

/// What a metre's further walk from where a building was wanted costs its
/// site preference, in the preference's own metres (#1775): a crest 5 m
/// higher is worth a site 100 m further out.
const WALK_COST: f32 = 0.05;

/// The slope (rise/run) from which a building faces downhill, its front
/// along the contour and out over its view, rather than toward the
/// arrival (#1775).
const CONTOUR_SLOPE: f32 = 0.06;

/// The relief a prop's footprint may have: a prop carries no plinth
/// (`foundation::is_settlement_structure`), only the sink and a little
/// more.
const PROP_REACH: f32 = 0.6;

/// How far over the water the lowest ground under the primary landmark
/// stands on the proxy (m, #1777) - every other member keeps the proxy's
/// own margin. The full map's ground lies up to about 3.5 m under the
/// proxy's near the water (its p10), and a landmark the compile walks off
/// water moves out of the arrival's view: over seeds 0-299 the landmarks
/// whose lowest ground stood under 4 m over the water were walked 13 times
/// in 37, those standing 4 to 8 m over it none in 21.
const LANDMARK_FREEBOARD: f32 = 4.0;

/// How far from spawn every member's middle stands at the least (m), as a
/// non-primary anchor's always did: out of the spawn scatter's square, 5 m
/// each way (#905), though a big footprint may reach into it. A primary
/// landmark keeps its own entry's further distance; secondaries and props
/// keep this one since a world's later candidate plans (#1778) found spots
/// a first plan had missed.
const SPAWN_SQUARE_KEEP: f32 = 10.0;

/// How many plans a world on its own ground is drawn as before the best is
/// kept ([`SettlementPlan::best_sited`], #1778).
const CANDIDATES: u64 = 4;

/// What each building the first frame shows besides the landmark adds to a
/// plan's [merit](SettlementPlan::merit) (#1778): the settlement round its
/// landmark, a reason to walk down.
const BUILDING_FRAMED: f32 = 0.25;

/// What each prop the first frame shows adds to a plan's merit (#1778).
const PROP_FRAMED: f32 = 0.05;

/// The most buildings, and the most props, a plan's merit counts in the
/// first frame (#1778).
const FRAMED_MAX: usize = 6;

/// The scales a member is tried at, largest first: `drawn`, then a quarter
/// octave smaller each step, down to [`SCALE_MIN`] (#1774). A member drawn
/// smaller than that is tried as drawn alone.
fn sizes(drawn: f32) -> impl Iterator<Item = f32> {
    std::iter::successors(Some(drawn), |scale| Some(scale * SCALE_STEP))
        .enumerate()
        .take_while(move |&(i, scale)| i == 0 || scale >= SCALE_MIN - 1e-4)
        .map(|(_, scale)| scale)
}

/// The radius of the ground `entry` stands on at `scale` - its own
/// declared ground where it has one (#1559), else its clearance - as the
/// record's placement carries it (`room::build::member_ground_radius`).
fn ground_radius(entry: &dyn crate::catalogue::CatalogueEntry, scale: f32) -> f32 {
    entry
        .ground_radius()
        .unwrap_or_else(|| entry.footprint().clearance)
        * scale
}

/// How tall `entry` stands drawn at `scale` with `grammar_seed` in a world
/// at `escalation`, metres over the ground its placement stands it on
/// (#1777): its coarse top ([`crate::catalogue::items::measure::coarse_top`]),
/// built for no one in particular - an entry's height is not its owner's -
/// and ruined as the build ruins it (`room::build`), since a fought-over
/// world can bring a stone circle down from 4.7 m to 0.6 m, a stadium from
/// 13 m to 3 m. To the decimetre: the grammar and the ruin turn their parts
/// with the platform's trigonometry, so two peers can measure a height a
/// last bit apart, which would stand their viewpoints apart (#1132);
/// rounded, they part only where it falls within a hair of a rounding step.
fn landmark_top(
    entry: &dyn crate::catalogue::CatalogueEntry,
    scale: f32,
    grammar_seed: u64,
    escalation: EscalationTier,
) -> f32 {
    let mut built = entry.build(VIEW_DID);
    crate::pds::ruin::apply_ruin_level_at(
        &mut built,
        escalation,
        grammar_seed,
        entry.ruin_max_lean(),
    );
    let top = crate::catalogue::items::measure::coarse_top(&built.with_shape_seed(grammar_seed))
        .unwrap_or(0.0)
        .max(0.0)
        * scale;
    (top * 10.0).round() / 10.0
}

/// The DID a landmark is built for to read its height (#1777).
const VIEW_DID: &str = "did:plc:viewpoint";

/// The relief `entry`'s footprint may have at `scale` before daylight
/// shows under it: how far below the ground it stands on its plinth
/// reaches, the sink included (#1009) - or, for a prop, which carries
/// none, [`PROP_REACH`].
fn reach(entry: &dyn crate::catalogue::CatalogueEntry, scale: f32) -> f32 {
    use crate::catalogue::items::foundation;
    if foundation::is_settlement_structure(entry) {
        foundation::required_depth(entry.footprint().clearance) * scale + MEMBER_SINK
    } else {
        PROP_REACH
    }
}

/// Whether `entry` drawn at `scale` fits the ground at `at` (#1773): the
/// relief under its footprint, as the full map is taken to have it, within
/// its [`reach`]; the ground at its middle and rim gentle enough that the
/// compile's walk leaves it there; and dry - its lowest ground `freeboard`
/// metres over the water at least, where that asks more than the proxy's
/// own margin ([`LANDMARK_FREEBOARD`], #1777).
fn member_fits(
    probe: &TerrainProbe,
    at: [f32; 2],
    entry: &dyn crate::catalogue::CatalogueEntry,
    scale: f32,
    freeboard: f32,
) -> bool {
    let reach = reach(entry, scale);
    let fits = |ground: &FootprintGround| {
        ground.relief * RELIEF_MARGIN <= reach
            && ground.steepest <= SITE_SLOPE
            && ground.is_dry()
            && ground.above_water >= freeboard
    };
    // Read no further than the first points that refuse it: the slopes, the
    // relief and the dry ground each only fail the more as the disc fills.
    probe
        .footprint_unless(at, ground_radius(entry, scale), |partial| !fits(partial))
        .is_some_and(|ground| fits(&ground))
}

/// The first of `sites` where `entry` fits over `freeboard`, at the largest
/// of its [`sizes`] whose area `room` (m²) still holds (#1774) that fits on
/// any of them (#1773), and that size.
fn fit_among(
    probe: &TerrainProbe,
    sites: &[[f32; 2]],
    from: [f32; 2],
    entry: &dyn crate::catalogue::CatalogueEntry,
    drawn: f32,
    room: f32,
    freeboard: f32,
) -> Option<([f32; 2], f32)> {
    let clearance = entry.footprint().clearance;
    sizes(drawn)
        .filter(|&scale| member_area_cost(clearance * scale) <= room)
        .find_map(|scale| {
            site_for(probe, sites, from, entry, scale, freeboard).map(|at| (at, scale))
        })
}

/// Where among `sites` - nearest `from` first - `entry` drawn at `scale`
/// stands: the nearest site whose ground fits it over `freeboard`
/// ([`member_fits`], #1773), or, for an entry
/// with a [site preference](crate::catalogue::SitePreference), the one it
/// fits within [`PREFERENCE_REACH`] beyond that which its preference scores
/// best, [`WALK_COST`] a metre from `from` taken off - the nearest of equals
/// (#1775).
fn site_for(
    probe: &TerrainProbe,
    sites: &[[f32; 2]],
    from: [f32; 2],
    entry: &dyn crate::catalogue::CatalogueEntry,
    scale: f32,
    freeboard: f32,
) -> Option<[f32; 2]> {
    let fits = |at: [f32; 2]| member_fits(probe, at, entry, scale, freeboard);
    let first = sites.iter().position(|&at| fits(at))?;
    let prefer = entry.site();
    if prefer == crate::catalogue::SitePreference::Anywhere {
        return Some(sites[first]);
    }
    // Square root, not `hypot` (#1132): the walk ranks the sites, and a
    // square root rounds the same on every platform.
    let walk = |at: [f32; 2]| {
        let (dx, dz) = (at[0] - from[0], at[1] - from[1]);
        (dx * dx + dz * dz).sqrt()
    };
    let reach = walk(sites[first]) + PREFERENCE_REACH;
    let score = |at: [f32; 2]| probe.preference(prefer, at) - WALK_COST * walk(at);
    let mut best = (sites[first], score(sites[first]));
    for &at in sites[first + 1..]
        .iter()
        .take_while(|&&at| walk(at) <= reach)
    {
        if fits(at) {
            let scored = score(at);
            if scored > best.1 {
                best = (at, scored);
            }
        }
    }
    Some(best.0)
}

/// The yaw that turns a building's front (its catalogue -Z) to face
/// `toward`, a direction on the ground - libm's (#1132), so every peer
/// writes the same bits.
fn facing(toward: [f32; 2]) -> f32 {
    libm::atan2f(-toward[0], -toward[1])
}

/// Which way a house at `at` faces (#1775): downhill where the ground falls
/// at [`CONTOUR_SLOPE`] or more - its front along the contour, out over its
/// view - else toward the world's `arrival`, its front to the visitor.
fn house_facing(probe: &TerrainProbe, at: [f32; 2], arrival: [f32; 2]) -> [f32; 2] {
    probe
        .downhill(at, CONTOUR_SLOPE)
        .unwrap_or([arrival[0] - at[0], arrival[1] - at[1]])
}

/// The first of `candidates` that [fits](fit_among) one of its sites
/// (`sites_for` it, nearest `from` first) within `room` (m²) and over
/// `freeboard`, with where it stands and at what size: the building drawn,
/// else what stands in for it ([`stand_ins`], #1774).
fn choose(
    probe: &TerrainProbe,
    candidates: &[&'static dyn crate::catalogue::CatalogueEntry],
    sites_for: impl Fn(&'static dyn crate::catalogue::CatalogueEntry) -> Vec<[f32; 2]>,
    from: [f32; 2],
    drawn: f32,
    room: f32,
    freeboard: f32,
) -> Option<(&'static dyn crate::catalogue::CatalogueEntry, [f32; 2], f32)> {
    candidates.iter().find_map(|&entry| {
        fit_among(
            probe,
            &sites_for(entry),
            from,
            entry,
            drawn,
            room,
            freeboard,
        )
        .map(|(at, scale)| (entry, at, scale))
    })
}

/// Catalogue entries by size - clearance, then slug - as Berlin's fit
/// orders its pools (`terrain::derived::fit::sized_pool`).
fn by_size(
    a: &dyn crate::catalogue::CatalogueEntry,
    b: &dyn crate::catalogue::CatalogueEntry,
) -> std::cmp::Ordering {
    a.footprint()
        .clearance
        .total_cmp(&b.footprint().clearance)
        .then(a.slug().cmp(b.slug()))
}

/// `drawn`, then `pool`'s entries smaller than it, biggest first, each once:
/// what a member tries in turn where the building drawn fits nowhere near
/// its place even at half size - the next smaller entry of its role, as
/// Berlin's fit gives way (#1774).
fn stand_ins(
    drawn: &'static dyn crate::catalogue::CatalogueEntry,
    pool: impl IntoIterator<Item = &'static dyn crate::catalogue::CatalogueEntry>,
) -> Vec<&'static dyn crate::catalogue::CatalogueEntry> {
    let mut smaller: Vec<_> = pool
        .into_iter()
        .filter(|entry| by_size(*entry, drawn).is_lt())
        .collect();
    smaller.sort_by(|a, b| by_size(*b, *a));
    smaller.dedup_by(|a, b| a.slug() == b.slug());
    std::iter::once(drawn).chain(smaller).collect()
}

/// The room an arrival stands in at the landing (m), kept clear of members.
const LANDING_KEEP: f32 = 2.0;

/// The forecourt the build stands before the primary landmark (#747,
/// #975): its gate and monument, by their footprints. The plan keeps it
/// clear as it keeps its members (#1774): a building drawn smaller - or
/// smaller standing in for it - brings the gate in closer, onto ground a
/// secondary could take.
#[derive(Clone, Copy)]
struct Forecourt {
    gate: f32,
    monument: Option<f32>,
}

impl Forecourt {
    /// The forecourt a world of `theme` stands, by the entries the build
    /// places ([`gateway_entry`](super::gateway::gateway_entry),
    /// [`monument_entry`](super::monument::monument_entry)); `None` for a
    /// world with no gate.
    fn of(theme: ThemeArchetype) -> Option<Self> {
        super::gateway::gateway_entry(theme).map(|gate| Forecourt {
            gate: gate.footprint().clearance,
            monument: super::monument::monument_entry(theme)
                .map(|monument| monument.footprint().clearance),
        })
    }

    /// Its gate and landing before a landmark at `landmark` keeping
    /// `clearance`, as the build stands them.
    fn spot(self, landmark: [f32; 2], clearance: f32) -> super::GatewaySpot {
        super::GatewaySpot::for_landmark(landmark, clearance, self.gate)
    }

    /// Where it stands before a landmark at `landmark` keeping `clearance`,
    /// as (position, keep-clear radius): the gate, its landing, and the
    /// monument on either side of the approach - its DID picks one, and the
    /// plan does not know its DID.
    fn keep_outs(self, landmark: [f32; 2], clearance: f32) -> Vec<([f32; 2], f32)> {
        let gate = self.spot(landmark, clearance);
        let mut keep = vec![(gate.offset, self.gate), (gate.landing, LANDING_KEEP)];
        if let Some(monument) = self.monument {
            for side in [false, true] {
                let at = super::MonumentSpot::on_side(&gate, self.gate, monument, side);
                keep.push((at.offset, monument));
            }
        }
        keep
    }
}

/// What kind of cluster [`place_cluster`] grows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ClusterKind {
    /// The room's main settlement: landmark + full prosperity bands.
    Primary,
    /// A second landmark-led cluster (archipelago island / second
    /// valley): landmark + reduced counts.
    Outpost,
    /// No landmark; a secondary with a prop or two.
    Hamlet,
}

/// Grow one cluster inside `region`. Members with no buildable cell near
/// their desired ring position whose ground fits them (#1773), even a
/// quarter octave smaller each step down to [`SCALE_MIN`], are dropped, as
/// are those that would blow the region's fill budget / `member_budget` -
/// that *is* the capacity adaptation.
///
/// Returned beside the cluster ([`Grown`]): the cell its anchor would have
/// taken were its ground not read, and the primary cluster's viewpoint.
#[allow(clippy::too_many_arguments)] // internal: a param object would just rename the call site
fn place_cluster(
    theme: ThemeArchetype,
    prosperity: ProsperityTier,
    escalation: EscalationTier,
    region: &BuildableRegion,
    probe: &TerrainProbe,
    kind: ClusterKind,
    forecourt: Option<Forecourt>,
    arrival: Option<[f32; 2]>,
    reserved: &[([f32; 2], f32)],
    member_budget: usize,
    rng: &mut ChaCha8Rng,
) -> Grown {
    let mut cluster = SettlementCluster {
        landmark: None,
        secondaries: Vec::new(),
        props: Vec::new(),
    };
    if member_budget == 0 {
        return Grown::bare(cluster);
    }
    let usable_area = region.area_m2 * FILL_FRACTION;
    let mut consumed = 0.0_f32;
    // (position, keep-clear radius) of everything placed so far - from
    // the first, what the clusters before kept clear (#1777).
    let mut placed: Vec<([f32; 2], f32)> = reserved.to_vec();

    // Anchor: landmark for landmark-led clusters, a secondary for
    // hamlets. Sits on the region cell nearest spawn, clear of the spawn
    // square, whose ground fits it.
    let anchor_role = if kind == ClusterKind::Hamlet {
        StructureRole::Secondary
    } else {
        StructureRole::Landmark
    };
    let anchor_pool = tiered_pool(theme, anchor_role, prosperity, escalation);
    let anchor_pool = if anchor_pool.is_empty() && kind == ClusterKind::Hamlet {
        // A theme with no secondaries yet: fall back to props so the
        // hamlet is at least a marked spot rather than empty.
        tiered_pool(theme, StructureRole::Prop, prosperity, escalation)
    } else {
        anchor_pool
    };
    if anchor_pool.is_empty() {
        return Grown::bare(cluster);
    }
    let drawn_entry = pick(&anchor_pool, rng);
    let (scale_lo, scale_hi) = if kind == ClusterKind::Hamlet {
        SECONDARY_SCALE_BAND
    } else {
        landmark_scale_band(prosperity)
    };
    // Clear of the spawn square, and of what the clusters before kept clear
    // (#1777) by the anchor's footprint at the largest it is drawn.
    let spawn_keep = |entry: &dyn crate::catalogue::CatalogueEntry| {
        let clear = if kind == ClusterKind::Primary {
            entry.footprint().min_spawn_dist
        } else {
            // Non-primary clusters live in other regions; only the basic
            // spawn-square clearance applies.
            SPAWN_SQUARE_KEEP
        };
        let reach = entry.footprint().clearance * scale_hi;
        std::iter::once(([0.0, 0.0], clear))
            .chain(reserved.iter().map(|&(at, radius)| (at, radius + reach)))
            .collect::<Vec<_>>()
    };
    let yaw_jitter = range_f32(rng, -0.35, 0.35);
    let drawn_scale = range_f32(rng, scale_lo, scale_hi);
    // The region's cell nearest spawn, clear of the spawn square, where the
    // anchor's ground fits it (#1773) - at the scale drawn, else a quarter
    // octave smaller each step, its area within its share of the region's
    // room; else the next smaller entry of its role, each clear of spawn by
    // its own distance, of the world's escalation but any prosperity: the
    // ground decides how grand it is, never how fought-over its world
    // (#1774). The primary landmark always stands: on no fitting site, the
    // smallest at half size on the flattest; any other cluster's anchor
    // fitting nowhere grows nothing.
    let sites_for = |entry: &dyn crate::catalogue::CatalogueEntry| {
        probe.sites_near(region, [0.0, 0.0], f32::INFINITY, &spawn_keep(entry))
    };
    let unfitted = sites_for(drawn_entry).first().copied().or_else(|| {
        probe
            .sites_near(region, region.centroid, f32::INFINITY, &[])
            .first()
            .copied()
    });
    let candidates = stand_ins(
        drawn_entry,
        entries_for(theme, drawn_entry.role()).filter(|entry| {
            entry.street().is_none() && entry.escalation_band().accepts(escalation)
        }),
    );
    let room = match kind {
        ClusterKind::Hamlet => usable_area,
        ClusterKind::Primary | ClusterKind::Outpost => usable_area * LANDMARK_ROOM_SHARE,
    };
    let (anchor_entry, anchor_pos, anchor_scale) = match choose(
        probe,
        &candidates,
        sites_for,
        [0.0, 0.0],
        drawn_scale,
        room,
        match kind {
            // The arrival's view is of it (#1777).
            ClusterKind::Primary => LANDMARK_FREEBOARD,
            ClusterKind::Outpost | ClusterKind::Hamlet => 0.0,
        },
    ) {
        Some(chosen) => chosen,
        None if kind == ClusterKind::Primary => {
            let entry = candidates[candidates.len() - 1];
            let scale = drawn_scale.min(SCALE_MIN);
            let radius = ground_radius(entry, scale);
            match probe
                .flattest(region, &spawn_keep(entry), radius)
                .or_else(|| probe.flattest(region, &[], radius))
            {
                Some(at) => (entry, at, scale),
                None => return Grown::bare(cluster),
            }
        }
        None => return Grown::bare(cluster),
    };
    let anchor_clearance = anchor_entry.footprint().clearance * anchor_scale;
    let grammar_seed = rng.next_u64();
    // The primary landmark's arrival (#1777): a viewpoint out at the
    // settlement's edge, in view of it, chosen before anything else is
    // sited so that nothing else stands in the way.
    let top = match (kind, forecourt) {
        (ClusterKind::Primary, Some(_)) => {
            landmark_top(anchor_entry, anchor_scale, grammar_seed, escalation)
        }
        _ => 0.0,
    };
    let viewpoint = match (kind, forecourt) {
        (ClusterKind::Primary, Some(_)) => Viewpoint::choose(
            probe,
            anchor_pos,
            ground_radius(anchor_entry, anchor_scale),
            top,
            anchor_clearance,
        ),
        _ => None,
    };
    // Where a visitor arrives (#1775): the primary cluster's own viewpoint,
    // else its forecourt in front of its landmark, else the one the caller
    // knows, else spawn.
    let arrival = viewpoint
        .map(|viewpoint| viewpoint.landing)
        .or_else(|| forecourt.map(|forecourt| forecourt.spot(anchor_pos, anchor_clearance).landing))
        .or(arrival)
        .unwrap_or([0.0, 0.0]);
    // A landmark faces its arrival - the primary's own viewpoint, or the
    // forecourt on the ray from spawn its landing stands on, so spawn, as
    // it always faced - and a hamlet's house faces as a house does (#1775).
    let toward_arrival = [arrival[0] - anchor_pos[0], arrival[1] - anchor_pos[1]];
    let anchor_yaw = match (kind, viewpoint) {
        (ClusterKind::Primary, Some(_)) | (ClusterKind::Outpost, _) => facing(toward_arrival),
        (ClusterKind::Primary, None) => libm::atan2f(anchor_pos[0], anchor_pos[1]),
        (ClusterKind::Hamlet, _) => facing(house_facing(probe, anchor_pos, arrival)),
    } + yaw_jitter;
    let anchor_member = SettlementMember {
        slug: anchor_entry.slug(),
        offset: anchor_pos,
        yaw_rad: anchor_yaw,
        scale: anchor_scale,
        grammar_seed,
        clearance: anchor_clearance,
    };
    consumed += member_area_cost(anchor_clearance);
    placed.push((anchor_pos, anchor_clearance));
    match (viewpoint, forecourt) {
        (Some(viewpoint), _) => placed.extend(viewpoint.keep_outs(anchor_pos, anchor_clearance)),
        (None, Some(forecourt)) => placed.extend(forecourt.keep_outs(anchor_pos, anchor_clearance)),
        (None, None) => {}
    }
    if kind == ClusterKind::Hamlet {
        cluster.secondaries.push(anchor_member);
    } else {
        cluster.landmark = Some(anchor_member);
    }

    // Secondaries fan out on the far side of the anchor (as the flat
    // deriver always did), each on buildable ground that fits it.
    let (sec_lo, sec_hi) = match kind {
        ClusterKind::Primary => secondary_count_band(prosperity),
        ClusterKind::Outpost => (0, 2),
        ClusterKind::Hamlet => (0, 0), // the anchor *is* the secondary
    };
    let mut remaining = tiered_pool(theme, StructureRole::Secondary, prosperity, escalation);
    let sec_hi = sec_hi.min(remaining.len()).min(MAX_SECONDARIES);
    let sec_count = sample_count(rng, sec_lo.min(sec_hi), sec_hi);
    // Away from the viewpoint the visitor looks from, round the landmark it
    // looks at (#1777) - else away from spawn, as always. libm here and in
    // the directions below (#1132): they decide which site each member takes.
    let base = match viewpoint {
        Some(viewpoint) => libm::atan2f(
            anchor_pos[0] - viewpoint.landing[0],
            anchor_pos[1] - viewpoint.landing[1],
        ),
        None => libm::atan2f(anchor_pos[0], anchor_pos[1]),
    };
    for i in 0..sec_count {
        // A stand-in spends the pool's entries faster than the count drew.
        if cluster.member_count() >= member_budget || remaining.is_empty() {
            break;
        }
        let idx = ((unit_f32(rng) * remaining.len() as f32) as usize).min(remaining.len() - 1);
        let entry = remaining.remove(idx);
        let fp = entry.footprint();
        let spacing = secondary_spacing_clearance(fp.clearance);
        if consumed + member_area_cost(spacing) > usable_area {
            continue;
        }
        let spread = if sec_count == 1 {
            range_f32(rng, -0.6, 0.6)
        } else {
            -1.2 + 2.4 * (i as f32) / ((sec_count - 1) as f32) + range_f32(rng, -0.25, 0.25)
        };
        let dir = base + spread;
        let r = anchor_clearance + spacing + range_f32(rng, 4.0, 12.0);
        let desired = [
            anchor_pos[0] + libm::sinf(dir) * r,
            anchor_pos[1] + libm::cosf(dir) * r,
        ];
        let keep: Vec<([f32; 2], f32)> = placed
            .iter()
            .map(|&(p, c)| (p, c + spacing))
            .chain(std::iter::once(([0.0, 0.0], SPAWN_SQUARE_KEEP)))
            .collect();
        // The nearest buildable cell within reach of where it was wanted
        // where its ground fits it, as drawn or smaller (#1773, #1774).
        let sites = probe.sites_near(region, desired, SNAP_MAX_DIST, &keep);
        if sites.is_empty() {
            continue;
        }
        let yaw_jitter = range_f32(rng, -0.25, 0.25);
        let drawn = range_f32(rng, SECONDARY_SCALE_BAND.0, SECONDARY_SCALE_BAND.1);
        // Else the next smaller of the pool's buildings left (#1774), spent.
        let candidates = stand_ins(entry, remaining.iter().copied());
        let Some((entry, pos, scale)) = choose(
            probe,
            &candidates,
            |_| sites.clone(),
            desired,
            drawn,
            usable_area - consumed,
            0.0,
        ) else {
            continue;
        };
        remaining.retain(|left| left.slug() != entry.slug());
        let fp = entry.footprint();
        let yaw_rad = facing(house_facing(probe, pos, arrival)) + yaw_jitter;
        let clearance = fp.clearance * scale;
        cluster.secondaries.push(SettlementMember {
            slug: entry.slug(),
            offset: pos,
            yaw_rad,
            scale,
            grammar_seed: rng.next_u64(),
            clearance,
        });
        consumed += member_area_cost(clearance);
        placed.push((pos, clearance));
    }

    // Props scatter around the anchor, sited the same way on ground they
    // stand on - they carry no plinth - each clear of every footprint
    // placed (#1776: at half the keep-clear a prop stood inside a castle's
    // walls).
    let prop_pool: Vec<&'static dyn crate::catalogue::CatalogueEntry> =
        entries_for_room(theme, StructureRole::Prop, prosperity, escalation).collect();
    if !prop_pool.is_empty() {
        let (prop_lo, prop_hi) = match kind {
            ClusterKind::Primary => prop_count_band(prosperity),
            ClusterKind::Outpost => (1, 3),
            ClusterKind::Hamlet => (1, 2),
        };
        let prop_count = sample_count(rng, prop_lo, prop_hi.min(MAX_PROPS));
        for _ in 0..prop_count {
            if cluster.member_count() >= member_budget {
                break;
            }
            let entry = pick(&prop_pool, rng);
            let fp = entry.footprint();
            if consumed + member_area_cost(fp.clearance) > usable_area {
                break;
            }
            let dir = base + range_f32(rng, -1.4, 1.4);
            let r = range_f32(rng, anchor_clearance + 2.0, anchor_clearance + 25.0);
            let desired = [
                anchor_pos[0] + libm::sinf(dir) * r,
                anchor_pos[1] + libm::cosf(dir) * r,
            ];
            let keep: Vec<([f32; 2], f32)> = placed
                .iter()
                .map(|&(p, c)| (p, c + fp.clearance))
                .chain(std::iter::once(([0.0, 0.0], SPAWN_SQUARE_KEEP)))
                .collect();
            let Some(pos) = probe
                .sites_near(region, desired, SNAP_MAX_DIST, &keep)
                .into_iter()
                .find(|&at| member_fits(probe, at, entry, 1.0, 0.0))
            else {
                continue;
            };
            let yaw_rad = unit_f32(rng) * TAU;
            let _unapplied = range_f32(rng, PROP_SCALE_ROLL.0, PROP_SCALE_ROLL.1);
            cluster.props.push(SettlementMember {
                slug: entry.slug(),
                offset: pos,
                yaw_rad,
                scale: 1.0,
                grammar_seed: rng.next_u64(),
                clearance: fp.clearance,
            });
            consumed += member_area_cost(fp.clearance);
            placed.push((pos, fp.clearance));
        }
    }

    Grown {
        cluster,
        unfitted,
        viewpoint,
        top,
    }
}

/// What [`place_cluster`] grows (#1777).
struct Grown {
    cluster: SettlementCluster,
    /// The cell its anchor would have taken were its ground not read - the
    /// nearest spawn clear of the spawn square, else the nearest the
    /// region's centroid ([`SettlementPlan::unfitted_landmark`]) - `None`
    /// where it grew no anchor.
    unfitted: Option<[f32; 2]>,
    /// The primary cluster's viewpoint ([`SettlementPlan::viewpoint`]).
    viewpoint: Option<Viewpoint>,
    /// How tall its landmark stands where its viewpoint was sought
    /// ([`SettlementPlan::landmark_top`]), else 0.
    top: f32,
}

impl Grown {
    /// `cluster`, grown no anchor.
    fn bare(cluster: SettlementCluster) -> Self {
        Grown {
            cluster,
            unfitted: None,
            viewpoint: None,
            top: 0.0,
        }
    }
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
        /// Established (Modest–Rich) theme-exclusive slugs - a poor room
        /// never grows these.
        rich_kit: &'static [&'static str],
        /// Destitute (Poor) theme-exclusive slugs - an affluent room never
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
    /// destitute entries - a band-agnostic shared landmark sits in neither.
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
                "pelican_bicycle",
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
        // - the two registers never cross, and where a theme pins a landmark it
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

    // -- Terrain-aware plan (#905) -----------------------------------------

    /// Probe over a synthetic heightmap defined by a closure.
    fn probe_from(
        grid: usize,
        cell: f32,
        water_y: f32,
        f: impl Fn(usize, usize) -> f32,
    ) -> TerrainProbe {
        let mut data = vec![0.0_f32; grid * grid];
        for z in 0..grid {
            for x in 0..grid {
                data[z * grid + x] = f(x, z);
            }
        }
        let map = gen_jobs::HeightmapData {
            width: grid as u32,
            height: grid as u32,
            scale: cell,
            data,
        };
        TerrainProbe::new(&map, water_y, BUILD_SLOPE_LIMIT)
    }

    fn sited_scene(s: u64, landform: crate::seeded_defaults::LandformArchetype) -> SceneCharacter {
        let mut scene = SceneCharacter::for_seed(s);
        scene.theme = ThemeArchetype::AncientClassical;
        scene.landform = landform;
        scene
    }

    /// Two flat plateaus split by an over-steep ravine - the canonical
    /// multi-region terrain.
    fn two_plateau_probe() -> TerrainProbe {
        probe_from(
            17,
            8.0,
            0.0,
            |x, _| {
                if (7..=9).contains(&x) { 2.0 } else { 30.0 }
            },
        )
    }

    /// #1463: a member's keep-clear radius is its footprint at the size it
    /// is drawn, and what is placed round it is spaced by that. Spaced by the
    /// unscaled radius, a 1.45x landmark stood 17 m over its own gate once
    /// the build drew it at its scale. A prop is drawn at its authored size -
    /// every copy of a prop slug shares one generator - so its scale is 1.
    /// Rich rooms are forced here so landmarks roll 1.05-1.45.
    #[test]
    fn members_keep_clear_at_the_size_they_are_drawn() {
        use crate::seeded_defaults::LandformArchetype::Archipelago;
        let probe = two_plateau_probe();
        let footprint = |slug: &str| {
            crate::catalogue::by_slug(slug)
                .expect("a settlement slug resolves")
                .footprint()
                .clearance
        };
        let mut grown = 0;
        for s in 0u64..96 {
            let mut scene = sited_scene(s, Archipelago);
            scene.prosperity = 0.95;
            let sited = SettlementPlan::from_scene_sited(&scene, s, &probe);
            let flat = Settlement::from_scene(&scene, s);
            let clusters: Vec<(
                Option<&SettlementMember>,
                &[SettlementMember],
                &[SettlementMember],
            )> = sited
                .clusters
                .iter()
                .map(|c| (c.landmark.as_ref(), &c.secondaries[..], &c.props[..]))
                .chain(std::iter::once((
                    Some(&flat.landmark),
                    &flat.secondaries[..],
                    &flat.props[..],
                )))
                .collect();
            for (landmark, secondaries, props) in clusters {
                for m in landmark.into_iter().chain(secondaries) {
                    let drawn = footprint(m.slug) * m.scale;
                    assert!(
                        (m.clearance - drawn).abs() < 1e-4,
                        "seed {s}: {} keeps {} clear, drawn at {} x{}",
                        m.slug,
                        m.clearance,
                        drawn,
                        m.scale
                    );
                    grown += usize::from(m.scale > 1.2);
                }
                for p in props {
                    assert_eq!(
                        p.scale, 1.0,
                        "seed {s}: prop {} is drawn at its own size",
                        p.slug
                    );
                    assert_eq!(p.clearance, footprint(p.slug), "seed {s}: prop {}", p.slug);
                }
                let built: Vec<&SettlementMember> =
                    landmark.into_iter().chain(secondaries).collect();
                for (i, a) in built.iter().enumerate() {
                    for b in &built[i + 1..] {
                        let d = (a.offset[0] - b.offset[0]).hypot(a.offset[1] - b.offset[1]);
                        assert!(
                            d >= a.clearance + b.clearance - 1e-3,
                            "seed {s}: {} and {} stand {d} m apart, inside {} + {}",
                            a.slug,
                            b.slug,
                            a.clearance,
                            b.clearance
                        );
                    }
                }
            }
        }
        assert!(
            grown > 0,
            "no member was drawn past 1.2x: the rule was never tested"
        );
    }

    /// #1774's ladder, as Berlin's fit steps: the size drawn, then a
    /// quarter octave smaller each step, down to half size.
    #[test]
    fn a_member_is_tried_a_quarter_octave_smaller_each_step_down_to_half_size() {
        let ladder: Vec<f32> = sizes(1.2).collect();
        assert_eq!(ladder[0], 1.2);
        for step in ladder.windows(2) {
            assert!(
                (step[1] / step[0] - 2f32.powf(-0.25)).abs() < 1e-5,
                "{ladder:?}"
            );
        }
        let last = ladder[ladder.len() - 1];
        assert!(
            last >= SCALE_MIN - 1e-4 && last * SCALE_STEP < SCALE_MIN,
            "{ladder:?}"
        );
        // Half size itself is a rung.
        assert!((sizes(1.0).last().unwrap() - SCALE_MIN).abs() < 1e-5);
        // A member drawn under the floor is tried as drawn alone.
        assert_eq!(sizes(0.4).collect::<Vec<_>>(), vec![0.4]);
    }

    /// #1773: a member fits ground its plinth reaches, the proxy's margin
    /// taken - on one slope, so, drawn smaller where it does not drawn whole
    /// - and never ground the water covers.
    #[test]
    fn a_member_fits_only_ground_its_plinth_reaches() {
        let entry = *crate::catalogue::ENTRIES
            .iter()
            .find(|e| {
                e.role() == StructureRole::Secondary
                    && e.ground_radius().is_none()
                    && (4.0..=12.0).contains(&e.footprint().clearance)
            })
            .expect("a mid-sized secondary");
        let c = entry.footprint().clearance;
        // Flat at 10 m west of x = 0 and rising at `k` east of it: a slope
        // between what the entry spans drawn whole (5 k c against its reach
        // 0.35 c + 0.4) and at half size (2.5 k c against 0.175 c + 0.375).
        let k = (0.35 * c + 0.575) / (5.0 * c);
        let probe = probe_from(64, 8.0, 0.0, |x, _| {
            10.0 + (x as f32 * 8.0 - 252.0).max(0.0) * k
        });
        assert!(member_fits(&probe, [-150.0, 0.0], entry, 1.0, 0.0));
        assert!(!member_fits(&probe, [150.0, 0.0], entry, 1.0, 0.0));
        assert!(member_fits(&probe, [150.0, 0.0], entry, 0.5, 0.0));
        let drowned = probe_from(16, 8.0, 50.0, |_, _| 10.0);
        assert!(!member_fits(&drowned, [0.0, 0.0], entry, 0.5, 0.0));
        // A landmark's freeboard (#1777): ground 10 m over water at 7 fits
        // over 3 m of it, not over 4.
        let shore = probe_from(16, 8.0, 7.0, |_, _| 10.0);
        assert!(member_fits(&shore, [0.0, 0.0], entry, 0.5, 0.0));
        assert!(!member_fits(
            &shore,
            [0.0, 0.0],
            entry,
            0.5,
            LANDMARK_FREEBOARD
        ));
    }

    /// #1774: what stands in for a building that fits nowhere is the next
    /// smaller of its pool, biggest first, each once - the one drawn first.
    #[test]
    fn stand_ins_are_the_smaller_entries_biggest_first() {
        let pool: Vec<&'static dyn crate::catalogue::CatalogueEntry> =
            entries_for(ThemeArchetype::Medieval, StructureRole::Secondary).collect();
        let drawn = *pool
            .iter()
            .max_by(|a, b| by_size(**a, **b))
            .expect("a medieval secondary");
        let doubled = pool.iter().chain(&pool).copied();
        let ins = stand_ins(drawn, doubled);
        assert_eq!(ins[0].slug(), drawn.slug());
        assert_eq!(ins.len(), pool.len(), "each entry once");
        for pair in ins.windows(2) {
            assert!(by_size(pair[1], pair[0]).is_lt(), "biggest first");
        }
        // The smallest drawn has none to give way to.
        let smallest = *pool.iter().min_by(|a, b| by_size(**a, **b)).unwrap();
        assert_eq!(stand_ins(smallest, pool.iter().copied()).len(), 1);
    }

    /// #1774: a landmark takes no more than half its region's room, at the
    /// size drawn or smaller, else gives way to the next smaller landmark of
    /// its theme - a rich Medieval world's castle, on an island too small
    /// for it at any size, gives way to a hovel standing on the island.
    #[test]
    fn a_landmark_too_big_for_its_region_gives_way() {
        use crate::seeded_defaults::LandformArchetype::Rolling;
        let scene_for = |s: u64| {
            let mut scene = sited_scene(s, Rolling);
            scene.theme = ThemeArchetype::Medieval;
            scene.prosperity = 0.95;
            scene.escalation = 0.1;
            scene
        };
        let open = probe_from(64, 12.0, -5.0, |_, _| 10.0);
        // An island 72 x 56 m east of spawn, its sea 20 m below its top.
        let island = probe_from(32, 8.0, 0.0, |x, z| {
            if (18..=27).contains(&x) && (12..=19).contains(&z) {
                10.0
            } else {
                -10.0
            }
        });
        let room = island.regions()[0].area_m2 * FILL_FRACTION * LANDMARK_ROOM_SHARE;
        for s in 0u64..6 {
            let scene = scene_for(s);
            let drawn = SettlementPlan::from_scene_sited(&scene, s, &open);
            let lm = drawn.primary_landmark();
            assert_eq!(lm.slug, "medieval_castle", "seed {s}: drawn on open ground");
            let fitted = SettlementPlan::from_scene_sited(&scene, s, &island);
            let lm = fitted.primary_landmark();
            assert_eq!(lm.slug, "wattle_hovel", "seed {s}: the castle gives way");
            assert!(
                member_area_cost(lm.clearance) <= room,
                "seed {s}: {} takes {} of its {room} m2",
                lm.slug,
                member_area_cost(lm.clearance)
            );
            let entry = by_slug(lm.slug).unwrap();
            assert!(
                member_fits(&island, lm.offset, entry, lm.scale, 0.0),
                "seed {s}"
            );
            assert!(
                lm.offset[0].hypot(lm.offset[1]) >= entry.footprint().min_spawn_dist,
                "seed {s}: the hovel keeps its own distance from spawn"
            );
        }
    }

    /// #1775: a temple seeks the crest - it climbs to a hill's flat top
    /// past the plain nearer spawn, which a building with no preference
    /// takes - and a lighthouse the shore.
    #[test]
    fn a_building_with_a_site_preference_seeks_its_ground() {
        use crate::catalogue::SitePreference;
        // A plain, and a hill 12.5 m tall 150 m east of spawn: flat on top
        // for 25 m round its middle, falling at 1 in 4 to the plain.
        let hill = probe_from(64, 8.0, -100.0, |x, z| {
            let (wx, wz) = (x as f32 * 8.0 - 252.0, z as f32 * 8.0 - 252.0);
            (18.75 - 0.25 * (wx - 150.0).hypot(wz)).clamp(0.0, 12.5)
        });
        let region = &hill.regions()[0];
        let temple = by_slug("ruined_temple").expect("the ruined temple");
        assert_eq!(temple.site(), SitePreference::Crest);
        let keep = [([0.0, 0.0], temple.footprint().min_spawn_dist)];
        let sites = hill.sites_near(region, [0.0, 0.0], f32::INFINITY, &keep);
        let at = site_for(&hill, &sites, [0.0, 0.0], temple, 1.0, 0.0).expect("a site");
        assert!(
            hill.height_between(at) > 12.0 && hill.prominence(at) > 5.0,
            "the temple stands on the hilltop: {at:?}"
        );
        let hut = by_slug("mudbrick_hut").expect("the mudbrick hut");
        assert_eq!(hut.site(), SitePreference::Anywhere);
        let at = site_for(&hill, &sites, [0.0, 0.0], hut, 1.0, 0.0).expect("a site");
        assert_eq!(at, sites[0], "a hut takes the nearest site");

        // The sea west of x = -100; dry land rising 1 in 20 east of it.
        let coast = probe_from(64, 8.0, 0.0, |x, _| {
            ((x as f32 * 8.0 - 252.0) + 100.0) * 0.05
        });
        let region = &coast.regions()[0];
        let lighthouse = by_slug("lighthouse").expect("the lighthouse");
        assert_eq!(lighthouse.site(), SitePreference::Shore);
        let keep = [([0.0, 0.0], lighthouse.footprint().min_spawn_dist)];
        let sites = coast.sites_near(region, [0.0, 0.0], f32::INFINITY, &keep);
        let at = site_for(&coast, &sites, [0.0, 0.0], lighthouse, 1.0, 0.0).expect("a site");
        assert!(
            coast.shore_distance(at) < coast.shore_distance(sites[0]) - 20.0,
            "the lighthouse goes to the water: {at:?} against {:?}",
            sites[0]
        );
    }

    /// #1775: a house fronts downhill on a slope - along the contour, out
    /// over its view - and toward the arrival on level ground; the yaw
    /// turns its catalogue -Z front that way.
    #[test]
    fn houses_front_downhill_on_a_slope_and_the_arrival_on_the_level() {
        let ramp = probe_from(32, 8.0, -100.0, |x, _| x as f32 * 8.0 * 0.1);
        let f = house_facing(&ramp, [0.0, 0.0], [50.0, 50.0]);
        assert!(f[0] < -0.99 && f[1].abs() < 1e-3, "downhill: {f:?}");
        let level = probe_from(32, 8.0, -100.0, |_, _| 10.0);
        let f = house_facing(&level, [10.0, 0.0], [10.0, -30.0]);
        assert!(f[0].abs() < 1e-4 && f[1] < 0.0, "toward the arrival: {f:?}");
        assert!(facing([0.0, -1.0]).abs() < 1e-6, "-Z unturned");
        let yaw = facing([1.0, 0.0]);
        // The placement's yaw turns -Z to (-sin, -cos).
        assert!(
            (-yaw.sin() - 1.0).abs() < 1e-6 && yaw.cos().abs() < 1e-6,
            "{yaw}"
        );
    }

    /// #1773 across the fleet: every member a seeded world sites stands on
    /// ground that fits it, read on the probe its derive read - all but a
    /// primary landmark that fit nowhere, which stands at half size at most.
    #[test]
    fn every_sited_member_fits_its_ground() {
        let mut members = 0;
        for seed in 0u64..24 {
            let record = crate::seeded_defaults::room::build::build_room_with_source(
                seed,
                "did:plc:fits",
                None,
            );
            let scene = SceneCharacter::for_seed(seed);
            let shape = crate::seeded_defaults::TerrainShape::from_scene(&scene, seed);
            let cfg = crate::pds::find_terrain_config(&record).expect("a seeded terrain");
            let probe = crate::seeded_defaults::room::build::settlement_probe(
                cfg,
                shape.water_level_fraction * shape.height_scale,
            );
            let plan = SettlementPlan::best_sited(&scene, seed, &probe);
            for (ci, cluster) in plan.clusters.iter().enumerate() {
                let all = cluster
                    .landmark
                    .iter()
                    .chain(&cluster.secondaries)
                    .chain(&cluster.props);
                for (i, m) in all.enumerate() {
                    let entry = by_slug(m.slug).expect("a settlement slug resolves");
                    // The primary landmark over its freeboard (#1777).
                    let freeboard = if ci == 0 && i == 0 {
                        LANDMARK_FREEBOARD
                    } else {
                        0.0
                    };
                    let fits = member_fits(&probe, m.offset, entry, m.scale, freeboard);
                    let fallback = ci == 0 && i == 0 && m.scale <= SCALE_MIN + 1e-5;
                    assert!(
                        fits || fallback,
                        "seed {seed}: {} x{} at {:?} stands on {:?}",
                        m.slug,
                        m.scale,
                        m.offset,
                        probe.footprint(m.offset, ground_radius(entry, m.scale))
                    );
                    members += 1;
                }
            }
        }
        assert!(members > 100, "only {members} members sited");
    }

    /// #1777: where a plan's arrival stands at a viewpoint, no member of any
    /// cluster - but the primary landmark it looks at - stands its footprint
    /// in the forecourt or across the view down to that landmark, the other
    /// clusters' as well as the primary's - in the plan the build keeps, the
    /// best of its candidates (#1778).
    #[test]
    fn no_member_stands_in_the_forecourt_or_across_the_view() {
        let (mut viewpoints, mut members) = (0, 0);
        for seed in 0u64..24 {
            let record = crate::seeded_defaults::room::build::build_room_with_source(
                seed,
                "did:plc:fits",
                None,
            );
            let scene = SceneCharacter::for_seed(seed);
            let shape = crate::seeded_defaults::TerrainShape::from_scene(&scene, seed);
            let cfg = crate::pds::find_terrain_config(&record).expect("a seeded terrain");
            let probe = crate::seeded_defaults::room::build::settlement_probe(
                cfg,
                shape.water_level_fraction * shape.height_scale,
            );
            let plan = SettlementPlan::best_sited(&scene, seed, &probe);
            let Some(viewpoint) = plan.viewpoint else {
                continue;
            };
            viewpoints += 1;
            let landmark = plan.primary_landmark();
            let keep = viewpoint.keep_outs(landmark.offset, landmark.clearance);
            for (ci, cluster) in plan.clusters.iter().enumerate() {
                let others = cluster
                    .landmark
                    .iter()
                    .filter(|_| ci > 0)
                    .chain(&cluster.secondaries)
                    .chain(&cluster.props);
                for m in others {
                    for &(at, radius) in &keep {
                        let apart = (m.offset[0] - at[0]).hypot(m.offset[1] - at[1]);
                        assert!(
                            apart >= radius + m.clearance - 1e-3,
                            "seed {seed}: {} {} m round stands {apart} m from {at:?}, kept {radius} m round",
                            m.slug,
                            m.clearance
                        );
                    }
                    members += 1;
                }
            }
        }
        assert!(
            viewpoints >= 12,
            "only {viewpoints} of 24 plans arrive at a viewpoint"
        );
        assert!(members > 50, "only {members} members sited");
    }

    #[test]
    fn sited_flat_room_grows_a_full_primary_cluster() {
        use crate::seeded_defaults::LandformArchetype::Rolling;
        let probe = probe_from(64, 12.0, -5.0, |_, _| 10.0);
        for s in 0u64..8 {
            let plan = SettlementPlan::from_scene_sited(&sited_scene(s, Rolling), s, &probe);
            assert!(!plan.clusters.is_empty());
            let lm = plan.primary_landmark();
            let d = (lm.offset[0].powi(2) + lm.offset[1].powi(2)).sqrt();
            assert!(d >= 10.0, "landmark on the spawn square: {d}");
            assert!(by_slug(lm.slug).is_some());
        }
    }

    #[test]
    fn sited_members_sit_on_buildable_ground() {
        use crate::seeded_defaults::LandformArchetype::Archipelago;
        let probe = two_plateau_probe();
        for s in 0u64..12 {
            let plan = SettlementPlan::from_scene_sited(&sited_scene(s, Archipelago), s, &probe);
            for c in &plan.clusters {
                for m in c
                    .landmark
                    .iter()
                    .chain(c.secondaries.iter())
                    .chain(c.props.iter())
                {
                    let slope = probe.slope_at(m.offset);
                    assert!(
                        slope <= BUILD_SLOPE_LIMIT + 1e-3,
                        "seed {s}: member {} on slope {slope} at {:?}",
                        m.slug,
                        m.offset
                    );
                    assert!(
                        probe.height_at(m.offset) > 0.0,
                        "seed {s}: member {} under water",
                        m.slug
                    );
                }
            }
        }
    }

    /// Archipelago / Valleys rooms may grow a second landmark-led
    /// cluster on a second buildable region; other landforms only grow
    /// hamlets there.
    #[test]
    fn second_landmark_is_landform_gated() {
        use crate::seeded_defaults::LandformArchetype::{Archipelago, Rolling};
        let probe = two_plateau_probe();

        let mut arch_second_landmark = false;
        for s in 0u64..24 {
            let plan = SettlementPlan::from_scene_sited(&sited_scene(s, Archipelago), s, &probe);
            arch_second_landmark |= plan.clusters[1..].iter().any(|c| c.landmark.is_some());

            let rolling = SettlementPlan::from_scene_sited(&sited_scene(s, Rolling), s, &probe);
            assert!(
                rolling.clusters[1..].iter().all(|c| c.landmark.is_none()),
                "seed {s}: rolling room grew a second landmark"
            );
        }
        assert!(
            arch_second_landmark,
            "no archipelago seed grew a second landmark cluster on a clear two-island room"
        );
    }

    /// Honest adaptation: a room with no buildable region at all still
    /// gets its landmark - alone, on the least-bad site.
    #[test]
    fn sited_hostile_room_gets_a_lone_landmark() {
        use crate::seeded_defaults::LandformArchetype::Craggy;
        // Uniform over-steep ramp: no region anywhere.
        let probe = probe_from(24, 6.0, -100.0, |x, _| x as f32 * 6.0);
        for s in 0u64..8 {
            let plan = SettlementPlan::from_scene_sited(&sited_scene(s, Craggy), s, &probe);
            assert_eq!(plan.clusters.len(), 1);
            let c = &plan.clusters[0];
            assert!(c.landmark.is_some());
            assert!(c.secondaries.is_empty() && c.props.is_empty());
            let lm = plan.primary_landmark();
            let d = (lm.offset[0].powi(2) + lm.offset[1].powi(2)).sqrt();
            assert!(d >= 10.0 - 1e-3, "lone landmark on the spawn square: {d}");
        }
    }

    #[test]
    fn sited_plan_is_deterministic_and_bounded() {
        use crate::seeded_defaults::LandformArchetype::Archipelago;
        let probe = two_plateau_probe();
        for s in 0u64..12 {
            let scene = sited_scene(s, Archipelago);
            let a = SettlementPlan::from_scene_sited(&scene, s, &probe);
            let b = SettlementPlan::from_scene_sited(&scene, s, &probe);
            assert_eq!(a.clusters.len(), b.clusters.len());
            let count =
                |p: &SettlementPlan| -> usize { p.clusters.iter().map(|c| c.member_count()).sum() };
            assert_eq!(count(&a), count(&b));
            assert!(count(&a) <= MAX_TOTAL_MEMBERS);
            assert_eq!(
                a.primary_landmark().offset,
                b.primary_landmark().offset,
                "seed {s} nondeterministic"
            );
        }
    }

    /// #1778: a world on its own ground keeps the best of its candidate
    /// plans - deterministically, the first of equals; no candidate it passed
    /// over is worth more; the later candidates are other plans, drawn from
    /// other streams; and the first is drawn from the stream every plan drew
    /// from before there were others.
    #[test]
    fn the_best_of_its_plans_is_kept_and_the_first_is_the_sited_one() {
        let (mut later, mut other) = (0, 0);
        for seed in 0u64..16 {
            let record = crate::seeded_defaults::room::build::build_room_with_source(
                seed,
                "did:plc:best",
                None,
            );
            let scene = SceneCharacter::for_seed(seed);
            let shape = crate::seeded_defaults::TerrainShape::from_scene(&scene, seed);
            let cfg = crate::pds::find_terrain_config(&record).expect("a seeded terrain");
            let probe = crate::seeded_defaults::room::build::settlement_probe(
                cfg,
                shape.water_level_fraction * shape.height_scale,
            );
            let key = |plan: &SettlementPlan| format!("{:?}", plan);
            let first = SettlementPlan::sited_on_stream(&scene, seed, &probe, 0);
            let best = SettlementPlan::best_sited(&scene, seed, &probe);
            assert_eq!(
                key(&best),
                key(&SettlementPlan::best_sited(&scene, seed, &probe))
            );
            let merit = best.merit(&probe);
            let mut kept = None;
            for stream in 0..CANDIDATES {
                let candidate = SettlementPlan::sited_on_stream(&scene, seed, &probe, stream);
                let worth = candidate.merit(&probe);
                assert!(worth <= merit, "seed {seed}: stream {stream} is worth more");
                if kept.is_none() && worth == merit {
                    kept = Some(stream);
                    assert_eq!(
                        key(&candidate),
                        key(&best),
                        "seed {seed}: the first of equals"
                    );
                }
                if stream > 0 && key(&candidate) != key(&first) {
                    other += 1;
                }
            }
            later += usize::from(kept != Some(0));
        }
        assert!(other > 16, "the later streams draw the same plans: {other}");
        assert!(
            later > 0,
            "no world kept a later plan: the merit decides nothing"
        );
        // Stream 0 is the stream a plan drew before there were others: a
        // fresh generator moved to it draws what it always drew.
        let mut was = ChaCha8Rng::seed_from_u64(0x1778);
        let mut is = ChaCha8Rng::seed_from_u64(0x1778);
        is.set_stream(0);
        assert!((0..256).all(|_| was.next_u64() == is.next_u64()));
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
