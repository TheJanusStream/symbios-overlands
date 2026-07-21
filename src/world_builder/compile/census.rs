//! Offline scatter-placement census (#912) — what the compiler will
//! *actually* place, measured without a browser or a GPU in the loop.
//!
//! The analytic `--room-census` (#810) multiplies each placement's `count`
//! by its generator's node count. That is the right tool for the entity
//! budget, but it is blind to everything this work stream is about: it
//! cannot see the biome filter, the slope cutoff or the road districts
//! rejecting samples, and it has nothing to say about how the survivors are
//! *arranged*. A stand of 200 trees and a stand of 200 trees in four
//! thickets are the same number.
//!
//! So this replays the real sampling loop — [`super::scatter::try_sample`],
//! the same function the executor calls — against a heightmap rebuilt from
//! the record, and reports both the yield and the arrangement.
//!
//! # Reading the clustering number
//!
//! The arrangement statistic is the **Clark–Evans nearest-neighbour index**:
//! the mean distance from each instance to its nearest neighbour, divided by
//! the `0.5 / √density` that a uniform random (Poisson) arrangement of the
//! same density over the same area would give.
//!
//! * `R ≈ 1` — indistinguishable from a random sprinkle.
//! * `R < 1` — clustered. This is what a grown stand looks like.
//! * `R > 1` — over-dispersed, more evenly spaced than random (an orchard).
//!
//! One caveat worth stating rather than hiding: the density denominator uses
//! the authored bounds area, but the biome filter and slope cutoff carve
//! that area down, so the absolute `R` reads low even for a genuinely
//! uniform scatter. The number that means something is therefore the
//! **ratio between the tuned scatter and the same scatter with its
//! naturalness zeroed**, which is why every row reports both.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::pds::{Placement, RoomRecord, ScatterBounds, ScatterNaturalness};
use crate::terrain::FinishedHeightMap;

use super::scatter::{
    SampleFilters, cluster_centers, instance_jitter, slope_cutoff, try_sample, urban_exclusions,
};

/// One scatter placement's measured outcome.
pub(crate) struct ScatterCensusRow {
    pub generator_ref: String,
    /// `Placement::Scatter::count` — instances asked for.
    pub requested: u32,
    /// Instances the sampler actually placed.
    pub placed: u32,
    /// Clark–Evans index of the placed instances (see the module docs).
    pub clark_evans: f32,
    /// Same scatter with naturalness zeroed: the control this row's
    /// `clark_evans` should be read against.
    pub clark_evans_uniform: f32,
    /// Per-instance scale range actually produced, `(min, max)`.
    pub scale_range: (f32, f32),
    /// The configured cutoff in degrees, for reporting alongside the
    /// steepness it produced.
    pub max_slope_deg: Option<f32>,
    /// The scatter's microbiome bands (#913), for display.
    pub above_water_band: Option<[f32; 2]>,
    pub altitude_band: Option<[f32; 2]>,
    /// Height above the water line at the placed instances, as
    /// `(median, p95, max)` metres — and the same with the bands removed.
    ///
    /// Compared as DISTRIBUTIONS, never as counts. The sampler retries past
    /// a rejection until it hits the requested count, so a band that
    /// rejects most of a disc still places the full quota and a
    /// count-difference reads as a flat zero. (This is the second time that
    /// trap has been walked into on this census — the slope columns learned
    /// it first.) What a working band changes is WHICH ground is planted,
    /// so the band shows up here or nowhere.
    pub above_water: (f32, f32, f32),
    pub above_water_unbanded: (f32, f32, f32),
    /// Ground steepness in **degrees** under the instances actually
    /// placed, as `(median, 95th percentile, max)`.
    pub slope_deg: (f32, f32, f32),
    /// World XZ of every placed instance, and of the same scatter with its
    /// naturalness zeroed. Kept so `render --scatter-plot` can draw the two
    /// plan views side by side — arrangement is a plan-view question that a
    /// perspective contact sheet of a 400 m stand cannot answer.
    pub points: Vec<(f32, f32)>,
    pub points_uniform: Vec<(f32, f32)>,
    /// Bounds radius (circle) or the larger half-extent (rect), for scaling
    /// the plot.
    pub bounds_radius: f32,
    /// Bounds centre in world XZ.
    pub bounds_center: (f32, f32),
    /// The same measure with the cutoff removed — the terrain the scatter
    /// was *offered*.
    ///
    /// This pair, not the placed count, is what shows a slope cutoff
    /// working. The sampler has a `count * 10` rejection budget, so a
    /// cutoff that rejects a tenth of the candidates still reaches the
    /// requested count; it changes *which* ground gets planted, not how
    /// much. A working cutoff therefore shows up as `slope_deg.max`
    /// sitting at or under the cutoff while `slope_deg_offered.max` runs
    /// well past it.
    pub slope_deg_offered: (f32, f32, f32),
}

/// Every seeded scatter in one room, measured.
pub(crate) struct ScatterCensus {
    pub rows: Vec<ScatterCensusRow>,
}

/// Replay every `Placement::Scatter` in `record` against a heightmap
/// rebuilt from it, and measure the result.
///
/// Rebuilding the heightmap is the expensive part (seconds per seed), so it
/// is done once and shared across the room's scatters — the same map every
/// peer's terrain pass produces for that record.
pub(crate) fn scatter_census(record: &RoomRecord) -> ScatterCensus {
    let heightmap = FinishedHeightMap(crate::terrain::rebuild_heightmap_for_record(record));
    let terrain_cfg = crate::pds::find_terrain_config(record);
    let water_level = super::water::room_water_level(record);

    let rows = record
        .placements
        .iter()
        .filter_map(|p| {
            let Placement::Scatter {
                generator_ref,
                bounds,
                count,
                local_seed,
                biome_filter,
                avoid_urban,
                naturalness,
                ..
            } = p
            else {
                return None;
            };
            let exclusions = urban_exclusions(record, *avoid_urban);
            let run = |naturalness: &ScatterNaturalness| {
                let filters = SampleFilters {
                    biome_filter,
                    terrain_cfg,
                    water_level,
                    urban_exclusions: &exclusions,
                    slope_cutoff: slope_cutoff(naturalness),
                };
                place(
                    bounds,
                    *count,
                    *local_seed,
                    naturalness,
                    &heightmap,
                    &filters,
                )
            };

            let tuned = run(naturalness);
            let uniform = run(&ScatterNaturalness::default());
            let unlimited = run(&ScatterNaturalness {
                max_slope_deg: None,
                ..*naturalness
            });
            let unbanded = run(&ScatterNaturalness {
                above_water_band: None,
                altitude_band: None,
                ..*naturalness
            });

            Some(ScatterCensusRow {
                generator_ref: generator_ref.clone(),
                requested: *count,
                placed: tuned.len() as u32,
                clark_evans: clark_evans(&tuned, bounds),
                clark_evans_uniform: clark_evans(&uniform, bounds),
                scale_range: scale_range(*local_seed, tuned.len(), naturalness),
                max_slope_deg: naturalness.max_slope_deg.map(|d| d.0),
                above_water_band: naturalness.above_water_band.map(|b| b.0),
                altitude_band: naturalness.altitude_band.map(|b| b.0),
                above_water: height_percentiles(&tuned, &heightmap, water_level),
                above_water_unbanded: height_percentiles(&unbanded, &heightmap, water_level),
                slope_deg: slope_percentiles(&tuned, &heightmap),
                slope_deg_offered: slope_percentiles(&unlimited, &heightmap),
                bounds_radius: bounds_radius(bounds),
                bounds_center: bounds_center(bounds),
                points: tuned,
                points_uniform: uniform,
            })
        })
        .collect();

    ScatterCensus { rows }
}

/// Run one scatter's sampling loop to completion, returning the world XZ of
/// every placed instance. Mirrors the executor's loop bounds exactly —
/// same `count * 10` rejection budget — so the yield reported here is the
/// yield the compiler gets.
fn place(
    bounds: &ScatterBounds,
    count: u32,
    local_seed: u64,
    naturalness: &ScatterNaturalness,
    heightmap: &FinishedHeightMap,
    filters: &SampleFilters<'_>,
) -> Vec<(f32, f32)> {
    let clusters = cluster_centers(bounds, count, local_seed, naturalness.edge_falloff.0);
    let mut rng = ChaCha8Rng::seed_from_u64(local_seed);
    let max_attempts = count.saturating_mul(10).max(count);
    let mut placed = Vec::with_capacity(count as usize);
    let mut attempts = 0;
    while (placed.len() as u32) < count && attempts < max_attempts {
        attempts += 1;
        if let Some((x, _, z)) = try_sample(
            bounds,
            naturalness,
            &clusters,
            &mut rng,
            Some(heightmap),
            filters,
        ) {
            placed.push((x, z));
        }
    }
    placed
}

/// Plot-scaling radius: the circle's radius, or a rect's larger
/// half-extent (rotation-agnostic, so the whole rect always fits).
fn bounds_radius(bounds: &ScatterBounds) -> f32 {
    match bounds {
        ScatterBounds::Circle { radius, .. } => radius.0,
        ScatterBounds::Rect { extents, .. } => extents.0[0].max(extents.0[1]),
    }
}

fn bounds_center(bounds: &ScatterBounds) -> (f32, f32) {
    match bounds {
        ScatterBounds::Circle { center, .. } | ScatterBounds::Rect { center, .. } => {
            (center.0[0], center.0[1])
        }
    }
}

/// `(median, p95, max)` ground steepness in degrees over a set of placed
/// points. Converts back out of the `1 - normal.y` measure the sampler
/// works in, because degrees are what the `max_slope_deg` knob is written
/// in and what a reader can reason about.
fn slope_percentiles(points: &[(f32, f32)], heightmap: &FinishedHeightMap) -> (f32, f32, f32) {
    if points.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut degs: Vec<f32> = points
        .iter()
        .map(|&(x, z)| {
            let s = super::scatter::terrain_slope_at(&heightmap.0, x, z);
            (1.0 - s).clamp(-1.0, 1.0).acos().to_degrees()
        })
        .collect();
    degs.sort_by(f32::total_cmp);
    let at = |q: f32| degs[((degs.len() - 1) as f32 * q).round() as usize];
    (at(0.5), at(0.95), degs[degs.len() - 1])
}

/// `(median, p95, max)` height above the water line, in metres, over a set
/// of placed points. `water_level` of `None` measures raw world Y instead,
/// which is what a room with no water generator has to compare against.
fn height_percentiles(
    points: &[(f32, f32)],
    heightmap: &FinishedHeightMap,
    water_level: Option<f32>,
) -> (f32, f32, f32) {
    if points.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let wl = water_level.unwrap_or(0.0);
    let mut hs: Vec<f32> = points
        .iter()
        .map(|&(x, z)| heightmap.world_height_at(x, z) - wl)
        .collect();
    hs.sort_by(f32::total_cmp);
    let at = |q: f32| hs[((hs.len() - 1) as f32 * q).round() as usize];
    (at(0.5), at(0.95), hs[hs.len() - 1])
}

/// Observed per-instance scale range, replayed from the same side stream
/// the executor draws from.
fn scale_range(local_seed: u64, instances: usize, naturalness: &ScatterNaturalness) -> (f32, f32) {
    let mut rng = ChaCha8Rng::seed_from_u64(local_seed ^ super::scatter::JITTER_SEED_SALT);
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for _ in 0..instances {
        let s = instance_jitter(&mut rng, naturalness).scale;
        lo = lo.min(s);
        hi = hi.max(s);
    }
    if instances == 0 { (1.0, 1.0) } else { (lo, hi) }
}

/// Clark–Evans nearest-neighbour index — see the module docs. `NaN` when
/// fewer than two instances were placed, since a nearest neighbour needs a
/// neighbour.
fn clark_evans(points: &[(f32, f32)], bounds: &ScatterBounds) -> f32 {
    if points.len() < 2 {
        return f32::NAN;
    }
    let mean_nn: f32 = points
        .iter()
        .map(|&(x, z)| {
            points
                .iter()
                .filter(|&&(ox, oz)| (ox, oz) != (x, z))
                .map(|&(ox, oz)| ((x - ox).powi(2) + (z - oz).powi(2)).sqrt())
                .fold(f32::MAX, f32::min)
        })
        .sum::<f32>()
        / points.len() as f32;
    let area = match bounds {
        ScatterBounds::Circle { radius, .. } => std::f32::consts::PI * radius.0 * radius.0,
        // `extents` are half-extents on each axis, as `sample_bounds` reads
        // them: a `[-1, 1]` sample scaled by the extent spans `2 * extent`.
        ScatterBounds::Rect { extents, .. } => 4.0 * extents.0[0] * extents.0[1],
    };
    let expected = 0.5 / (points.len() as f32 / area).sqrt();
    mean_nn / expected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{Fp, Fp2};

    /// The statistic has to answer the question it is being asked, so
    /// calibrate it against arrangements whose answer is known: a uniform
    /// random sprinkle sits near 1, and the same points contracted onto a
    /// handful of seeds sit well below it.
    #[test]
    fn clark_evans_separates_clustered_from_uniform() {
        let bounds = ScatterBounds::Circle {
            center: Fp2([0.0, 0.0]),
            radius: Fp(100.0),
        };
        let mut rng = ChaCha8Rng::seed_from_u64(4);
        let uniform: Vec<(f32, f32)> = (0..600)
            .map(|_| super::super::scatter::sample_bounds(&bounds, &mut rng, 0.0))
            .collect();
        let clusters = cluster_centers(&bounds, 600, 4, 0.0);
        let clustered: Vec<(f32, f32)> = uniform
            .iter()
            .map(|&p| super::super::scatter::apply_clumping(p, &clusters, 0.7))
            .collect();

        let r_uniform = clark_evans(&uniform, &bounds);
        let r_clustered = clark_evans(&clustered, &bounds);
        assert!(
            (0.9..1.1).contains(&r_uniform),
            "a uniform sprinkle should score ≈1, got {r_uniform}"
        );
        assert!(
            r_clustered < 0.6,
            "clumping 0.7 should score well under 1, got {r_clustered}"
        );
    }

    #[test]
    fn clark_evans_needs_a_neighbour() {
        let bounds = ScatterBounds::default();
        assert!(clark_evans(&[], &bounds).is_nan());
        assert!(clark_evans(&[(0.0, 0.0)], &bounds).is_nan());
    }
}
