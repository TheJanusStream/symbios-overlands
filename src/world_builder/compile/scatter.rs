//! Scatter placement helpers: deterministic sampling inside a
//! [`ScatterBounds`], the placement-naturalness transforms layered on top of
//! it (#912), and the dominant-biome lookup the scatter biome filter
//! consults. The biome lookup delegates to `bevy_symbios_ground::SplatRule`
//! so the splat-rule weight formula stays single-sourced upstream.
//!
//! # The determinism discipline
//!
//! Placement must be bit-stable for a given seed across peers, and — the
//! stronger property this module maintains — **changing a naturalness knob
//! must not move the instances that knob is not about**. That rules out the
//! obvious implementations (draw an extra sample to decide a cluster, draw a
//! rejection roll for edge density), because every extra draw shifts the
//! stream for every later instance.
//!
//! So each knob is built to cost zero draws from the placement RNG:
//!
//! * [`ScatterNaturalness::edge_falloff`] and
//!   [`ScatterNaturalness::clumping`] are pure *remappings* of a sample that
//!   was already drawn — the uniform draw happens either way and is then
//!   warped in place.
//! * The cluster seeds those two need come from a **separate** RNG derived
//!   from the same `local_seed` ([`cluster_centers`]), never from the
//!   placement stream.
//! * `max_slope_deg` only rejects. Because the per-instance decorations moved
//!   off the placement stream (below), a rejection now consumes exactly what
//!   an acceptance would have, so tightening the cutoff *removes* instances
//!   without relocating the survivors.
//! * `scale_jitter` / `tilt_jitter` draw from the per-scatter side stream
//!   ([`JITTER_SEED_SALT`]) in a fixed 4-draw group per placed instance, so
//!   the side stream stays in lockstep with the instance index and toggling
//!   either knob changes nothing else.

use bevy::prelude::*;
use bevy_symbios_ground::SplatRule;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use crate::pds::{ScatterBounds, ScatterNaturalness, SovereignSplatRule, SovereignTerrainConfig};

/// Seed offset for the per-scatter cluster-seed stream. Distinct from
/// [`JITTER_SEED_SALT`] so a scatter's clump layout and its per-instance
/// jitter can't correlate.
const CLUSTER_SEED_SALT: u64 = 0xC105_7E12_C105_7E12;

/// Seed offset for the per-scatter instance-decoration stream (scale, tilt).
pub(crate) const JITTER_SEED_SALT: u64 = 0x1177_E812_1177_E812;

/// Draws taken from the jitter stream per placed instance. Fixed, and taken
/// unconditionally, so the stream index tracks the instance index no matter
/// which knobs are enabled.
pub(crate) const JITTER_DRAWS_PER_INSTANCE: usize = 4;

/// Upper bound on cluster seeds per scatter. Beyond this the clumps are
/// finer than the props themselves and the effect reads as noise.
const MAX_CLUSTERS: usize = 32;

/// Uniform sample inside the scatter region, warped by `edge_falloff`.
/// Circle bounds use rejection sampling so the base distribution stays flat
/// instead of clumping at the centre (which a naïve `radius * random()`
/// would produce).
///
/// `edge_falloff` is an exponent on the normalised radius: `0` leaves the
/// uniform distribution alone, and larger values push samples inward, so the
/// stand thins toward its boundary instead of ending in a mown circular
/// edge. It is applied *after* the draw, so it never changes how many u32s
/// this consumes for a given RNG state.
pub(crate) fn sample_bounds(
    bounds: &ScatterBounds,
    rng: &mut ChaCha8Rng,
    edge_falloff: f32,
) -> (f32, f32) {
    match bounds {
        ScatterBounds::Rect {
            center,
            extents,
            rotation,
        } => {
            // Per-axis falloff: the rect thins toward all four edges.
            let lx = falloff_axis(unit_f32(rng), edge_falloff) * extents.0[0];
            let lz = falloff_axis(unit_f32(rng), edge_falloff) * extents.0[1];
            let rot = rotation.0;
            let rx = lx * rot.cos() - lz * rot.sin();
            let rz = lx * rot.sin() + lz * rot.cos();
            (center.0[0] + rx, center.0[1] + rz)
        }
        ScatterBounds::Circle { center, radius } => loop {
            let x = unit_f32(rng);
            let z = unit_f32(rng);
            if x * x + z * z <= 1.0 {
                // Radial falloff: `r -> r^(1+falloff)` on the unit disc.
                let (x, z) = if edge_falloff > 0.0 {
                    let r = (x * x + z * z).sqrt();
                    if r > f32::EPSILON {
                        let k = r.powf(edge_falloff);
                        (x * k, z * k)
                    } else {
                        (x, z)
                    }
                } else {
                    (x, z)
                };
                return (center.0[0] + x * radius.0, center.0[1] + z * radius.0);
            }
        },
    }
}

/// Signed `|v|^(1+falloff)` for a `[-1, 1]` axis sample — the rect
/// counterpart of the disc's radial remap.
fn falloff_axis(v: f32, falloff: f32) -> f32 {
    if falloff <= 0.0 {
        return v;
    }
    v.signum() * v.abs().powf(1.0 + falloff)
}

/// Deterministic cluster seeds for a scatter, drawn from their own stream so
/// they never perturb the placement RNG. Seed count scales with the square
/// root of the instance count — enough clumps that a dense stand still reads
/// as patchy, few enough that a sparse one doesn't degenerate into one
/// instance per clump.
///
/// The seeds carry the same `edge_falloff` warp as the samples do, so
/// contracting samples onto them preserves rather than flattens the stand's
/// radial density profile.
///
/// Always computed (it is a few dozen samples), so enabling `clumping` never
/// changes anything but the contraction itself.
pub(crate) fn cluster_centers(
    bounds: &ScatterBounds,
    count: u32,
    local_seed: u64,
    edge_falloff: f32,
) -> Vec<(f32, f32)> {
    let n = (((count as f32).sqrt() / 1.5).round() as usize).clamp(2, MAX_CLUSTERS);
    let mut rng = ChaCha8Rng::seed_from_u64(local_seed ^ CLUSTER_SEED_SALT);
    (0..n)
        .map(|_| sample_bounds(bounds, &mut rng, edge_falloff))
        .collect()
}

/// Pull a sample toward its nearest cluster seed by `clumping`. `0` is the
/// identity (flat uniform); `0.5` halves each seed's catchment radius, which
/// leaves thickets with clearings between them.
///
/// Contraction toward a point inside the region can never leave it — both
/// bounds shapes are convex — so this cannot push an instance outside the
/// authored stand.
pub(crate) fn apply_clumping(
    pos: (f32, f32),
    clusters: &[(f32, f32)],
    clumping: f32,
) -> (f32, f32) {
    if clumping <= 0.0 || clusters.is_empty() {
        return pos;
    }
    let mut best = clusters[0];
    let mut best_d2 = f32::MAX;
    for &c in clusters {
        let (dx, dz) = (pos.0 - c.0, pos.1 - c.1);
        let d2 = dx * dx + dz * dz;
        if d2 < best_d2 {
            best_d2 = d2;
            best = c;
        }
    }
    let keep = 1.0 - clumping;
    (
        best.0 + (pos.0 - best.0) * keep,
        best.1 + (pos.1 - best.1) * keep,
    )
}

/// One instance's pose decorations, drawn as a fixed group from the
/// scatter's side stream.
pub(crate) struct InstanceJitter {
    /// Spin about the instance's own Y axis, radians.
    pub yaw: f32,
    /// Compass direction the instance leans toward, radians.
    pub tilt_azimuth: f32,
    /// Lean off vertical, radians. Signed — a negative angle is the same
    /// lean 180° round, which is why one draw covers the whole cone.
    pub tilt_angle: f32,
    /// Uniform scale multiplier, log-uniform about `1.0`.
    pub scale: f32,
}

/// Draw one instance's decorations. Takes exactly
/// [`JITTER_DRAWS_PER_INSTANCE`] samples every call regardless of which
/// knobs are enabled — that fixed group size is what keeps the side stream
/// aligned with the instance index, so toggling `tilt_jitter` cannot change
/// the next instance's scale.
///
/// Yaw lives on this stream rather than the placement stream (where it used
/// to sit) so that the placement stream carries *nothing but positions*.
/// That is what makes `max_slope_deg` purely subtractive: rejecting a sample
/// now consumes exactly what accepting it would have, so the survivors stay
/// put instead of the whole stand reshuffling.
pub(crate) fn instance_jitter(rng: &mut ChaCha8Rng, n: &ScatterNaturalness) -> InstanceJitter {
    // Drawn as one fixed-size array rather than four statements: the array
    // length *is* the group-size contract, checked by the compiler instead
    // of by a comment asking the next editor not to add a fifth draw.
    let [yaw, azimuth, tilt, scale]: [f32; JITTER_DRAWS_PER_INSTANCE] =
        std::array::from_fn(|_| unit_f32(rng));
    InstanceJitter {
        yaw: yaw * std::f32::consts::PI,
        tilt_azimuth: azimuth * std::f32::consts::PI,
        tilt_angle: tilt * n.tilt_jitter.0,
        // Log-uniform: a 0.85× and a 1.18× instance are the same one step
        // away from nominal, which an additive spread would not give.
        scale: (scale * n.scale_jitter.0).exp(),
    }
}

/// Compose one instance's local pose from its position and a jitter draw.
///
/// Shared by the compiler and by the headless render tool's scatter
/// preview, so a contact sheet shows the arrangement the game will
/// actually build rather than a separate approximation of it.
///
/// The scale stays uniform: the generator's own root transform composes on
/// top of this, and a non-uniform parent scale would shear any child that
/// carries a rotation.
pub(crate) fn instance_pose(
    local_pos: Vec3,
    jitter: &InstanceJitter,
    random_yaw: bool,
    n: &ScatterNaturalness,
) -> Transform {
    let mut rotation = if random_yaw {
        Quat::from_rotation_y(jitter.yaw)
    } else {
        Quat::IDENTITY
    };
    if n.tilt_jitter.0 > 0.0 {
        // A *pure* lean: rotate about a horizontal axis pointing at
        // `tilt_azimuth`, which is `Ry(az) · Rx(θ) · Ry(-az)`. Dropping the
        // trailing `Ry(-az)` would be simpler and wrong — it would leave a
        // net yaw of `az` behind, silently spinning instances in a scatter
        // that explicitly asked for `random_yaw: false`.
        let lean = Quat::from_rotation_y(jitter.tilt_azimuth)
            * Quat::from_rotation_x(jitter.tilt_angle)
            * Quat::from_rotation_y(-jitter.tilt_azimuth);
        rotation = lean * rotation;
    }
    let mut tf = Transform::from_translation(local_pos).with_rotation(rotation);
    if n.scale_jitter.0 > 0.0 {
        tf.scale = Vec3::splat(jitter.scale);
    }
    tf
}

/// Replay a scatter's arrangement with every terrain-dependent filter
/// skipped, for the headless render tool.
///
/// The `--room` contact sheet has no heightmap, so the biome allow-list and
/// the slope cutoff have nothing to resolve against and
/// [`try_sample`] would correctly refuse to place anything. This keeps the
/// parts that *are* meaningful without terrain — the distribution warps and
/// the per-instance pose — so the sheet still shows what clumping and
/// jitter do. Instances sit on the ground plane rather than terrain-snapped.
pub(crate) struct ScatterPreview {
    bounds: ScatterBounds,
    naturalness: ScatterNaturalness,
    random_yaw: bool,
    clusters: Vec<(f32, f32)>,
    rng: ChaCha8Rng,
    jitter_rng: ChaCha8Rng,
}

impl ScatterPreview {
    pub(crate) fn new(
        bounds: &ScatterBounds,
        count: u32,
        local_seed: u64,
        naturalness: &ScatterNaturalness,
        random_yaw: bool,
    ) -> Self {
        Self {
            bounds: bounds.clone(),
            naturalness: *naturalness,
            random_yaw,
            clusters: cluster_centers(bounds, count, local_seed, naturalness.edge_falloff.0),
            rng: ChaCha8Rng::seed_from_u64(local_seed),
            jitter_rng: ChaCha8Rng::seed_from_u64(local_seed ^ JITTER_SEED_SALT),
        }
    }

    /// The next instance's pose on the ground plane.
    pub(crate) fn next_pose(&mut self) -> Transform {
        let (x, z) = apply_clumping(
            sample_bounds(&self.bounds, &mut self.rng, self.naturalness.edge_falloff.0),
            &self.clusters,
            self.naturalness.clumping.0,
        );
        let jitter = instance_jitter(&mut self.jitter_rng, &self.naturalness);
        instance_pose(
            Vec3::new(x, 0.0, z),
            &jitter,
            self.random_yaw,
            &self.naturalness,
        )
    }
}

/// Terrain steepness at a world XZ, as `1 - normal.y` — the same measure
/// [`dominant_biome`] scores the splat rules against, so a scatter's slope
/// cutoff and its biome allow-list are talking about the same quantity.
/// `0` is dead flat, `1` is a vertical face.
pub(crate) fn terrain_slope_at(
    hm: &bevy_symbios_ground::HeightMap,
    world_x: f32,
    world_z: f32,
) -> f32 {
    // Normal sampling reads the raw heightmap frame; mirror the world→map
    // shift that `world_height_at` applies to the height.
    let extent = (hm.width() - 1) as f32 * hm.scale();
    let half = extent * 0.5;
    let normal = hm.get_normal_at(
        (world_x + half).clamp(0.0, extent),
        (world_z + half).clamp(0.0, extent),
    );
    (1.0 - normal[1]).max(0.0)
}

/// Convert [`ScatterNaturalness::max_slope_deg`] into the `1 - normal.y`
/// cutoff [`terrain_slope_at`] produces, so the trigonometry is paid once
/// per scatter rather than once per sample. `1 - cos θ`: 15° → 0.034,
/// 30° → 0.134, 45° → 0.293.
pub(crate) fn slope_cutoff(naturalness: &ScatterNaturalness) -> Option<f32> {
    naturalness
        .max_slope_deg
        .map(|deg| 1.0 - deg.0.to_radians().cos())
}

/// Everything a scatter sample is tested against, resolved once per unit.
/// Grouped so the accept/reject decision can live in one place that both
/// the executor and the offline `--scatter-census` call — the two agreeing
/// is the whole point, since the census exists to report what the compiler
/// will actually place.
pub(crate) struct SampleFilters<'a> {
    pub biome_filter: &'a crate::pds::BiomeFilter,
    /// `None` on a record with no terrain generator, which collapses the
    /// biome allow-list to "never matches".
    pub terrain_cfg: Option<&'a SovereignTerrainConfig>,
    pub water_level: Option<f32>,
    /// `(centre_x, centre_z, radius²)` road districts to stay out of.
    pub urban_exclusions: &'a [(f32, f32, f32)],
    /// Pre-resolved [`slope_cutoff`].
    pub slope_cutoff: Option<f32>,
}

/// `(centre_x, centre_z, radius²)` of every road district a scatter that
/// opted into `avoid_urban` must stay out of (#895). Empty unless the
/// scatter opted in *and* the room actually grows an enabled road network.
/// The centre follows the authored district offset (#889); a zero offset is
/// the historical spawn-centred circle.
pub(crate) fn urban_exclusions(
    record: &crate::pds::RoomRecord,
    avoid_urban: bool,
) -> Vec<(f32, f32, f32)> {
    if !avoid_urban {
        return Vec::new();
    }
    crate::pds::find_road_configs(record)
        .into_iter()
        .filter(|c| c.enabled)
        .map(|c| {
            (
                c.center.0[0],
                c.center.0[1],
                c.district_half_extent.0 * c.district_half_extent.0,
            )
        })
        .collect()
}

/// Draw one candidate and decide it: `Some((x, y, z))` in world space when
/// it is kept, `None` when a filter rejected it.
///
/// Exactly one draw group is taken from `rng` per call whatever the outcome,
/// so a rejection costs the stream the same as an acceptance. That is what
/// makes every filter here purely subtractive — tighten one and the
/// instances that survive stay exactly where they were.
pub(crate) fn try_sample(
    bounds: &ScatterBounds,
    naturalness: &ScatterNaturalness,
    clusters: &[(f32, f32)],
    rng: &mut ChaCha8Rng,
    heightmap: Option<&crate::terrain::FinishedHeightMap>,
    filters: &SampleFilters<'_>,
) -> Option<(f32, f32, f32)> {
    let (world_x, world_z) = apply_clumping(
        sample_bounds(bounds, rng, naturalness.edge_falloff.0),
        clusters,
        naturalness.clumping.0,
    );

    // Road-network districts (#895) keep wild scatter out of the built-up
    // area. Purely positional, so it is checked before the terrain lookups.
    if filters.urban_exclusions.iter().any(|&(cx, cz, r2)| {
        let (dx, dz) = (world_x - cx, world_z - cz);
        dx * dx + dz * dz < r2
    }) {
        return None;
    }

    let Some(hm_res) = heightmap else {
        // Nothing to resolve against: the allow-list and the slope cutoff
        // both fail closed rather than silently passing every sample.
        return (filters.biome_filter.is_noop() && filters.slope_cutoff.is_none())
            .then_some((world_x, 0.0, world_z));
    };

    let hm = &hm_res.0;
    let y = hm_res.world_height_at(world_x, world_z);

    // Steepness feeds two consumers — the allow-list (via the dominant
    // splat layer) and the explicit cutoff — so sample it once, and only if
    // one of them is going to read it.
    let slope = (!filters.biome_filter.is_noop() || filters.slope_cutoff.is_some())
        .then(|| terrain_slope_at(hm, world_x, world_z));
    if !filters
        .slope_cutoff
        .is_none_or(|cutoff| slope.is_some_and(|s| s <= cutoff))
    {
        return None;
    }
    if !filters.biome_filter.is_noop() {
        // Without a terrain generator the biome allow-list has no channel
        // to resolve against; treat any non-empty list as "never matches"
        // so accidental biome filters on dry-land records don't silently
        // pass through. The water clause still evaluates.
        let biome = match (filters.terrain_cfg, slope) {
            (Some(tcfg), Some(s)) => dominant_biome(tcfg, y, s),
            _ => 255,
        };
        if !filters.biome_filter.accepts(biome, y, filters.water_level) {
            return None;
        }
    }
    Some((world_x, y, world_z))
}

/// Deterministic `[-1, 1]` sample from a `ChaCha8Rng`.
pub(crate) fn unit_f32(rng: &mut ChaCha8Rng) -> f32 {
    let v = rng.next_u32() as f32 / u32::MAX as f32;
    v * 2.0 - 1.0
}

// ---------------------------------------------------------------------------
// Biome evaluation
// ---------------------------------------------------------------------------

/// Convert a wire-format [`SovereignSplatRule`] into an upstream [`SplatRule`]
/// so the weight formula can be evaluated by [`SplatRule::weight`] directly,
/// without re-implementing the smooth-range logic locally.
fn convert_rule(r: &SovereignSplatRule) -> SplatRule {
    SplatRule::new(
        (r.height_min.0, r.height_max.0),
        (r.slope_min.0, r.slope_max.0),
        r.sharpness.0,
    )
}

/// Return the dominant biome index (0=Grass, 1=Dirt, 2=Rock, 3=Snow) at the
/// given world-space (height, slope) pair, using the terrain generator's
/// splat rules. The splat rules expect *normalised* heights so we divide
/// by `height_scale` first.
pub(crate) fn dominant_biome(cfg: &SovereignTerrainConfig, height_world: f32, slope: f32) -> u8 {
    let height_norm = if cfg.height_scale.0.abs() > f32::EPSILON {
        height_world / cfg.height_scale.0
    } else {
        0.0
    };
    let weights = [
        convert_rule(&cfg.material.rules[0]).weight(height_norm, slope),
        convert_rule(&cfg.material.rules[1]).weight(height_norm, slope),
        convert_rule(&cfg.material.rules[2]).weight(height_norm, slope),
        convert_rule(&cfg.material.rules[3]).weight(height_norm, slope),
    ];
    let mut best = 0;
    let mut max_w = weights[0];
    for (i, &w) in weights.iter().enumerate().skip(1) {
        if w > max_w {
            max_w = w;
            best = i;
        }
    }
    best as u8
}

#[cfg(test)]
mod tests {
    //! The placement-naturalness knobs (#912) carry two kinds of
    //! obligation, and both are tested here:
    //!
    //! * **A determinism contract.** No knob may consume a draw from the
    //!   placement RNG. These are the tests that would fail if someone
    //!   "simplified" a warp into a rejection roll — which would silently
    //!   move every instance in every existing record.
    //! * **An effect.** Each knob has to actually do the thing its name
    //!   claims, which is checked statistically over a few thousand
    //!   samples rather than by pinning exact coordinates.
    use super::*;
    use crate::pds::{Fp, Fp2};

    fn disc(radius: f32) -> ScatterBounds {
        ScatterBounds::Circle {
            center: Fp2([0.0, 0.0]),
            radius: Fp(radius),
        }
    }

    fn rect() -> ScatterBounds {
        ScatterBounds::Rect {
            center: Fp2([0.0, 0.0]),
            extents: Fp2([50.0, 30.0]),
            rotation: Fp(0.0),
        }
    }

    /// Mean distance from the bounds centre over `n` samples.
    fn mean_radius(bounds: &ScatterBounds, falloff: f32, n: usize) -> f32 {
        let mut rng = ChaCha8Rng::seed_from_u64(11);
        let total: f32 = (0..n)
            .map(|_| {
                let (x, z) = sample_bounds(bounds, &mut rng, falloff);
                (x * x + z * z).sqrt()
            })
            .sum();
        total / n as f32
    }

    // --- determinism contract ------------------------------------------

    /// The load-bearing one. `edge_falloff` warps a sample that was drawn
    /// either way, so two runs from the same seed must leave the RNG in
    /// *exactly* the same state — otherwise turning the knob on would
    /// reshuffle every instance after the first.
    #[test]
    fn edge_falloff_consumes_no_extra_draws() {
        for bounds in [disc(100.0), rect()] {
            let mut flat = ChaCha8Rng::seed_from_u64(7);
            let mut warped = ChaCha8Rng::seed_from_u64(7);
            for _ in 0..500 {
                sample_bounds(&bounds, &mut flat, 0.0);
                sample_bounds(&bounds, &mut warped, 2.5);
            }
            assert_eq!(
                flat.next_u32(),
                warped.next_u32(),
                "edge_falloff shifted the placement stream"
            );
        }
    }

    /// `clumping` is applied to an already-drawn sample, so it cannot
    /// touch the stream at all — and its cluster seeds come from their own
    /// RNG rather than the placement one.
    #[test]
    fn clumping_consumes_no_draws() {
        let bounds = disc(100.0);
        let clusters = cluster_centers(&bounds, 400, 99, 0.0);
        let mut plain = ChaCha8Rng::seed_from_u64(7);
        let mut clumped = ChaCha8Rng::seed_from_u64(7);
        for _ in 0..500 {
            sample_bounds(&bounds, &mut plain, 0.0);
            apply_clumping(sample_bounds(&bounds, &mut clumped, 0.0), &clusters, 0.8);
        }
        assert_eq!(plain.next_u32(), clumped.next_u32());
    }

    /// The jitter stream advances by a fixed group per instance no matter
    /// which knobs are on, so toggling `tilt_jitter` cannot change the
    /// next instance's scale.
    #[test]
    fn instance_jitter_takes_a_fixed_draw_group() {
        let off = ScatterNaturalness::default();
        let on = ScatterNaturalness {
            scale_jitter: Fp(0.3),
            tilt_jitter: Fp(0.2),
            ..ScatterNaturalness::default()
        };
        for naturalness in [off, on] {
            let mut used = ChaCha8Rng::seed_from_u64(3);
            let mut counted = ChaCha8Rng::seed_from_u64(3);
            instance_jitter(&mut used, &naturalness);
            for _ in 0..JITTER_DRAWS_PER_INSTANCE {
                counted.next_u32();
            }
            assert_eq!(
                used.next_u32(),
                counted.next_u32(),
                "jitter group is not {JITTER_DRAWS_PER_INSTANCE} draws wide"
            );
        }
    }

    /// Cluster seeds must be a pure function of the scatter's own seed —
    /// two peers compiling the same record have to agree on them, and a
    /// re-derivation within one peer must not drift.
    #[test]
    fn cluster_seeds_are_reproducible_and_seed_specific() {
        let bounds = disc(100.0);
        assert_eq!(
            cluster_centers(&bounds, 300, 42, 1.0),
            cluster_centers(&bounds, 300, 42, 1.0)
        );
        assert_ne!(
            cluster_centers(&bounds, 300, 42, 1.0),
            cluster_centers(&bounds, 300, 43, 1.0),
            "two scatters in one room would share a clump layout"
        );
    }

    // --- effects --------------------------------------------------------

    #[test]
    fn edge_falloff_pulls_samples_toward_the_middle() {
        let bounds = disc(100.0);
        let flat = mean_radius(&bounds, 0.0, 4000);
        let mild = mean_radius(&bounds, 1.0, 4000);
        let hard = mean_radius(&bounds, 3.0, 4000);
        // Uniform on a disc has mean radius 2R/3 ≈ 66.7.
        assert!(
            (60.0..72.0).contains(&flat),
            "flat disc sampling drifted: mean radius {flat}"
        );
        assert!(mild < flat * 0.85, "mild falloff {mild} vs flat {flat}");
        assert!(hard < mild, "hard falloff {hard} should beat mild {mild}");
    }

    /// Rect bounds thin toward all four edges rather than radially, so the
    /// check is per-axis.
    #[test]
    fn edge_falloff_thins_a_rect_toward_its_edges() {
        let bounds = rect();
        let mean_abs = |falloff: f32| {
            let mut rng = ChaCha8Rng::seed_from_u64(5);
            let total: f32 = (0..4000)
                .map(|_| sample_bounds(&bounds, &mut rng, falloff).0.abs())
                .sum();
            total / 4000.0
        };
        assert!(mean_abs(2.0) < mean_abs(0.0) * 0.75);
    }

    #[test]
    fn clumping_tightens_samples_onto_their_cluster_seeds() {
        let bounds = disc(100.0);
        let clusters = cluster_centers(&bounds, 400, 21, 0.0);
        let mean_to_seed = |clumping: f32| {
            let mut rng = ChaCha8Rng::seed_from_u64(13);
            let total: f32 = (0..3000)
                .map(|_| {
                    let p =
                        apply_clumping(sample_bounds(&bounds, &mut rng, 0.0), &clusters, clumping);
                    clusters
                        .iter()
                        .map(|c| ((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt())
                        .fold(f32::MAX, f32::min)
                })
                .sum();
            total / 3000.0
        };
        let loose = mean_to_seed(0.0);
        let tight = mean_to_seed(0.6);
        // Contraction by `1 - clumping` should scale the mean distance to
        // the nearest seed by the same factor. Nearest-seed identity can
        // change under contraction, so allow slack rather than pinning.
        assert!(
            tight < loose * 0.5,
            "clumping 0.6 gave mean {tight} vs {loose} flat"
        );
    }

    /// Contraction toward an interior point of a convex region can never
    /// escape it — no instance may end up outside the authored stand.
    #[test]
    fn clumping_keeps_every_sample_inside_the_bounds() {
        let bounds = disc(100.0);
        let clusters = cluster_centers(&bounds, 500, 4, 1.5);
        let mut rng = ChaCha8Rng::seed_from_u64(77);
        for _ in 0..3000 {
            let (x, z) = apply_clumping(sample_bounds(&bounds, &mut rng, 1.5), &clusters, 0.9);
            assert!(
                (x * x + z * z).sqrt() <= 100.0 + 1e-3,
                "clumping pushed a sample outside the bounds: ({x}, {z})"
            );
        }
    }

    #[test]
    fn scale_jitter_is_log_symmetric_about_one() {
        let n = ScatterNaturalness {
            scale_jitter: Fp(0.18),
            ..ScatterNaturalness::default()
        };
        let mut rng = ChaCha8Rng::seed_from_u64(31);
        let (mut lo, mut hi, mut log_sum) = (f32::MAX, f32::MIN, 0.0f32);
        for _ in 0..4000 {
            let s = instance_jitter(&mut rng, &n).scale;
            lo = lo.min(s);
            hi = hi.max(s);
            log_sum += s.ln();
        }
        // e^±0.18 ≈ 0.835 / 1.197 — the range the doc comment promises.
        assert!((0.83..0.85).contains(&lo), "low end {lo}");
        assert!((1.19..1.21).contains(&hi), "high end {hi}");
        assert!(
            (log_sum / 4000.0).abs() < 0.01,
            "log-mean should sit on 1.0, not {}",
            (log_sum / 4000.0).exp()
        );
    }

    /// With the knob off the decorations must be exact identities, not
    /// merely small — a 1.0000001 scale would defeat the shared-handle
    /// dedup path for no visual gain.
    #[test]
    fn jitter_knobs_off_produce_exact_identities() {
        let n = ScatterNaturalness::default();
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        for _ in 0..200 {
            let j = instance_jitter(&mut rng, &n);
            assert_eq!(j.scale, 1.0);
            assert_eq!(j.tilt_angle, 0.0);
        }
    }

    /// The cutoff has to speak the same language as `terrain_slope_at`,
    /// which reports `1 - normal.y` rather than an angle or a gradient.
    #[test]
    fn slope_cutoff_converts_degrees_to_the_normal_measure() {
        let at = |deg: f32| {
            slope_cutoff(&ScatterNaturalness {
                max_slope_deg: Some(Fp(deg)),
                ..ScatterNaturalness::default()
            })
            .unwrap()
        };
        assert!(at(0.0).abs() < 1e-6, "flat ground is 0");
        assert!((at(60.0) - 0.5).abs() < 1e-5, "1 - cos 60 = 0.5");
        assert!((at(90.0) - 1.0).abs() < 1e-5, "a vertical face is 1");
        // Monotone, so a bigger angle is always a looser filter.
        assert!(at(15.0) < at(30.0) && at(30.0) < at(45.0));
        assert!(slope_cutoff(&ScatterNaturalness::default()).is_none());
    }

    /// Tilt must be a pure lean. The naive composition leaves a net yaw
    /// behind, which would spin instances in a scatter that explicitly
    /// turned `random_yaw` off — a silent, hard-to-attribute regression.
    #[test]
    fn tilt_leans_without_introducing_yaw() {
        let n = ScatterNaturalness {
            tilt_jitter: Fp(0.3),
            ..ScatterNaturalness::default()
        };
        let mut rng = ChaCha8Rng::seed_from_u64(17);
        for _ in 0..200 {
            let j = instance_jitter(&mut rng, &n);
            let tf = instance_pose(Vec3::ZERO, &j, false, &n);
            // A pure lean tips the up axis by |tilt_angle| and leaves the
            // horizontal heading alone, so a forward vector projected back
            // onto the ground plane still points at -Z.
            let up = tf.rotation * Vec3::Y;
            assert!(
                (up.angle_between(Vec3::Y) - j.tilt_angle.abs()).abs() < 1e-3,
                "lean magnitude wrong: {} vs {}",
                up.angle_between(Vec3::Y),
                j.tilt_angle.abs()
            );
            // "No spin" means the rotation *axis* is horizontal — that is
            // the definition of a lean. It is deliberately not "the
            // forward vector's ground shadow is unmoved": any rigid lean
            // tips forward out of the horizontal plane, and its shadow
            // swings by a second-order `(1 - cos θ)` amount. The naive
            // `Ry(az) · Rx(θ)` composition fails this; the conjugated form
            // passes it.
            let (axis, angle) = tf.rotation.to_axis_angle();
            if angle.abs() > 1e-3 {
                assert!(
                    axis.y.abs() < 1e-3,
                    "tilt axis is not horizontal (y = {}) — that is a spin, \
                     not a lean",
                    axis.y
                );
            }
        }
    }

    /// A scatter that asks for a slope limit must place nothing when there
    /// is no heightmap to ask, rather than silently ignoring the limit.
    #[test]
    fn slope_limit_without_a_heightmap_fails_closed() {
        let bounds = disc(50.0);
        let naturalness = ScatterNaturalness {
            max_slope_deg: Some(Fp(30.0)),
            ..ScatterNaturalness::default()
        };
        let filters = SampleFilters {
            biome_filter: &crate::pds::BiomeFilter::default(),
            terrain_cfg: None,
            water_level: None,
            urban_exclusions: &[],
            slope_cutoff: slope_cutoff(&naturalness),
        };
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        for _ in 0..100 {
            assert!(try_sample(&bounds, &naturalness, &[], &mut rng, None, &filters).is_none());
        }
        // …while the same scatter without the limit places freely.
        let open = ScatterNaturalness::default();
        let filters = SampleFilters {
            slope_cutoff: None,
            ..filters
        };
        assert!(try_sample(&bounds, &open, &[], &mut rng, None, &filters).is_some());
    }
}
