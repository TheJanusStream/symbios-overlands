//! Terrain probe — flat-region segmentation over the derive-time proxy
//! heightmap (#905).
//!
//! The settlement deriver used to place structures blind, by RNG angles
//! and distances alone; the compile-time snap then draped them over
//! whatever the terrain happened to be — including 45° rock faces. This
//! module gives the deriver eyes: a low-resolution proxy of the room's
//! heightmap (see `gen_jobs::run_heightmap_proxy`) is segmented into
//! **buildable regions** — connected spans of ground that are both
//! gentler than a slope limit and above the water line — and the
//! settlement deriver sites its clusters inside them.
//!
//! Coordinates: the terrain mesh is spawned centred on the room origin
//! (the spawn point), so every world-facing API here uses centred XZ —
//! `[-extent/2, +extent/2]` — matching settlement member offsets.
//!
//! The proxy is an approximation of the full-resolution map, so
//! consumers pair a conservative [slope limit](TerrainProbe::new) with
//! the compile-time slope nudge in the world builder (the safety net
//! that catches proxy-vs-full divergence).

use gen_jobs::HeightmapData;

/// Cells this close to the water line (in metres of height) count as
/// shoreline and are not buildable — keeps settlements off the beach
/// splash zone even when the slope there is gentle.
const WATER_MARGIN: f32 = 0.75;

/// Minimum cell count for a connected span to count as a region at all.
/// Smaller specks are proxy noise, not building ground.
const MIN_REGION_CELLS: usize = 3;

/// Share of a scatter's disc that must satisfy a microbiome band for the
/// band to be kept (#913). Below this the band is treated as unsatisfiable
/// and dropped, because the ground it leaves also has to survive the biome
/// allow-list and the slope cutoff — so a band clinging to a few percent of
/// the disc empties the scatter rather than zoning it.
const MIN_BAND_COVERAGE: f32 = 0.08;

/// One connected buildable region of the proxy grid.
#[derive(Clone, Debug)]
pub struct BuildableRegion {
    /// Usable area in square metres (cell count × cell area).
    pub area_m2: f32,
    /// Number of proxy cells in the region.
    pub cell_count: usize,
    /// Area-centroid in centred world XZ.
    pub centroid: [f32; 2],
    /// Mean terrain height of the region (world Y).
    pub mean_height: f32,
    /// Distance from the room origin (spawn) to the region's nearest cell.
    pub min_spawn_dist: f32,
    /// Flat indices (into the proxy grid) of the region's cells.
    cells: Vec<u32>,
}

/// Segmented derive-time view of a room's terrain.
pub struct TerrainProbe {
    grid: usize,
    /// Metres between adjacent proxy cells.
    cell: f32,
    /// Half the world extent — world XZ = grid index × cell − half.
    half: f32,
    heights: Vec<f32>,
    /// Per-cell slope as rise/run to the steepest 4-neighbour.
    slopes: Vec<f32>,
    water_y: f32,
    regions: Vec<BuildableRegion>,
}

impl TerrainProbe {
    /// Segment `map` into buildable regions: cells with slope at most
    /// `slope_limit` (rise/run) sitting at least [`WATER_MARGIN`] above
    /// `water_y`, grouped by 4-connectivity, smallest specks dropped.
    /// Regions are sorted largest-first.
    pub fn new(map: &HeightmapData, water_y: f32, slope_limit: f32) -> Self {
        let grid = map.width.max(2) as usize;
        let cell = map.scale.max(0.01);
        let half = (grid - 1) as f32 * cell * 0.5;
        let heights = map.data.clone();

        let at = |x: usize, z: usize| heights[z * grid + x];
        let mut slopes = vec![0.0_f32; grid * grid];
        for z in 0..grid {
            for x in 0..grid {
                let h = at(x, z);
                let mut steepest = 0.0_f32;
                if x > 0 {
                    steepest = steepest.max((at(x - 1, z) - h).abs());
                }
                if x + 1 < grid {
                    steepest = steepest.max((at(x + 1, z) - h).abs());
                }
                if z > 0 {
                    steepest = steepest.max((at(x, z - 1) - h).abs());
                }
                if z + 1 < grid {
                    steepest = steepest.max((at(x, z + 1) - h).abs());
                }
                slopes[z * grid + x] = steepest / cell;
            }
        }

        let buildable = |i: usize| slopes[i] <= slope_limit && heights[i] > water_y + WATER_MARGIN;

        // Connected components over the buildable mask (4-connectivity,
        // explicit stack — the proxy is small but recursion depth is
        // unbounded in the worst case).
        let mut region_of = vec![u32::MAX; grid * grid];
        let mut regions: Vec<BuildableRegion> = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        for start in 0..grid * grid {
            if region_of[start] != u32::MAX || !buildable(start) {
                continue;
            }
            let id = regions.len() as u32;
            let mut cells: Vec<u32> = Vec::new();
            stack.push(start);
            region_of[start] = id;
            while let Some(i) = stack.pop() {
                cells.push(i as u32);
                let (x, z) = (i % grid, i / grid);
                let mut push = |n: usize| {
                    if region_of[n] == u32::MAX && buildable(n) {
                        region_of[n] = id;
                        stack.push(n);
                    }
                };
                if x > 0 {
                    push(i - 1);
                }
                if x + 1 < grid {
                    push(i + 1);
                }
                if z > 0 {
                    push(i - grid);
                }
                if z + 1 < grid {
                    push(i + grid);
                }
            }

            if cells.len() < MIN_REGION_CELLS {
                // Too small to build on; unmark so the speck stays
                // region-less rather than occupying an id.
                for &c in &cells {
                    region_of[c as usize] = u32::MAX - 1;
                }
                continue;
            }

            let mut cx = 0.0_f64;
            let mut cz = 0.0_f64;
            let mut ch = 0.0_f64;
            let mut min_d2 = f32::MAX;
            for &c in &cells {
                let (x, z) = ((c as usize) % grid, (c as usize) / grid);
                let wx = x as f32 * cell - half;
                let wz = z as f32 * cell - half;
                cx += wx as f64;
                cz += wz as f64;
                ch += heights[c as usize] as f64;
                min_d2 = min_d2.min(wx * wx + wz * wz);
            }
            let n = cells.len() as f64;
            regions.push(BuildableRegion {
                area_m2: cells.len() as f32 * cell * cell,
                cell_count: cells.len(),
                centroid: [(cx / n) as f32, (cz / n) as f32],
                mean_height: (ch / n) as f32,
                min_spawn_dist: min_d2.sqrt(),
                cells,
            });
        }

        regions.sort_by_key(|b| std::cmp::Reverse(b.cell_count));

        Self {
            grid,
            cell,
            half,
            heights,
            slopes,
            water_y,
            regions,
        }
    }

    /// The buildable regions, largest first.
    pub fn regions(&self) -> &[BuildableRegion] {
        &self.regions
    }

    /// Metres between adjacent proxy cells.
    pub fn cell_size(&self) -> f32 {
        self.cell
    }

    /// Slope (rise/run) at a centred world XZ, from the nearest cell.
    pub fn slope_at(&self, world: [f32; 2]) -> f32 {
        let (x, z) = self.cell_index(world);
        self.slopes[z * self.grid + x]
    }

    /// Terrain height at a centred world XZ, from the nearest cell.
    pub fn height_at(&self, world: [f32; 2]) -> f32 {
        let (x, z) = self.cell_index(world);
        self.heights[z * self.grid + x]
    }

    /// Fraction of the proxy cells inside the disc whose height satisfies
    /// `band` (offset into the band's frame — pass the water line for an
    /// above-water band, `0.0` for an absolute altitude one). `None` when
    /// the disc covers no cell at all.
    fn band_coverage(
        &self,
        center: [f32; 2],
        radius: f32,
        band: [f32; 2],
        offset: f32,
    ) -> Option<f32> {
        let r2 = radius * radius;
        let (mut inside, mut ok) = (0u32, 0u32);
        for z in 0..self.grid {
            for x in 0..self.grid {
                let wx = x as f32 * self.cell - self.half;
                let wz = z as f32 * self.cell - self.half;
                let (dx, dz) = (wx - center[0], wz - center[1]);
                if dx * dx + dz * dz > r2 {
                    continue;
                }
                inside += 1;
                let h = self.heights[z * self.grid + x] - offset;
                if h >= band[0] && h <= band[1] {
                    ok += 1;
                }
            }
        }
        (inside > 0).then(|| ok as f32 / inside as f32)
    }

    /// Drop any height band on `naturalness` that too little of the
    /// scatter's disc could satisfy (#913).
    ///
    /// A microbiome band is a preference expressed over ground the scatter
    /// can actually reach. When the deriver rolls a patch centre onto
    /// terrain that sits almost entirely below an altitude floor — or above
    /// a riparian ceiling — the band stops zoning the patch and simply
    /// deletes it: measured across 14 seeds, one lichen scatter went to
    /// 0/230 placed. Relaxing to "unbanded" there keeps the patch present,
    /// which is the lesser wrong — a slightly mis-zoned patch reads as
    /// vegetation, an absent one reads as a bug.
    ///
    /// The test is a COVERAGE FRACTION, not "does any cell qualify".
    /// Requiring a single qualifying cell is far too weak: a flat room
    /// whose ground just grazes an altitude floor passes that test and
    /// still places nothing, because the surviving ground also has to clear
    /// the biome allow-list and the slope cutoff. [`MIN_BAND_COVERAGE`] is
    /// the margin that makes the test mean "there is somewhere to grow".
    ///
    /// Only bands that fail that test are dropped. A band that merely thins
    /// a scatter is the feature working, and is left alone.
    ///
    /// Deterministic: a pure function of the shared proxy heightmap and the
    /// band, so peers agree without consuming any RNG.
    pub fn relax_unsatisfiable_bands(
        &self,
        naturalness: &mut crate::pds::ScatterNaturalness,
        center: [f32; 2],
        radius: f32,
    ) {
        if let Some(crate::pds::Fp2(band)) = naturalness.altitude_band
            && self
                .band_coverage(center, radius, band, 0.0)
                .is_some_and(|f| f < MIN_BAND_COVERAGE)
        {
            naturalness.altitude_band = None;
        }
        // A water band that reaches BELOW the waterline is not zoning, it is
        // an aquatic species' habitat (#914): lilies over the shallow bed,
        // reeds wading the margin. Relaxing it would not mis-zone a patch —
        // it would move the species out of the water entirely (pads tiling a
        // deep lake, reeds across dry upland). A patch whose disc missed the
        // water placing nothing is the correct outcome there.
        if let Some(crate::pds::Fp2(band)) = naturalness.above_water_band
            && band[0] >= 0.0
            && self
                .band_coverage(center, radius, band, self.water_y)
                .is_some_and(|f| f < MIN_BAND_COVERAGE)
        {
            naturalness.above_water_band = None;
        }
    }

    /// The buildable cell of `region` closest to `desired` that also
    /// keeps at least the paired radius away from every entry of
    /// `keep_clear` — or `None` if the region has no such cell. The
    /// returned position is the cell's centred world XZ.
    pub fn snap_to_region(
        &self,
        region: &BuildableRegion,
        desired: [f32; 2],
        keep_clear: &[([f32; 2], f32)],
    ) -> Option<[f32; 2]> {
        let mut best: Option<([f32; 2], f32)> = None;
        for &c in &region.cells {
            let (x, z) = ((c as usize) % self.grid, (c as usize) / self.grid);
            let wx = x as f32 * self.cell - self.half;
            let wz = z as f32 * self.cell - self.half;
            let clear = keep_clear.iter().all(|&(p, r)| {
                let dx = wx - p[0];
                let dz = wz - p[1];
                dx * dx + dz * dz >= r * r
            });
            if !clear {
                continue;
            }
            let dx = wx - desired[0];
            let dz = wz - desired[1];
            let d2 = dx * dx + dz * dz;
            if best.is_none_or(|(_, bd2)| d2 < bd2) {
                best = Some(([wx, wz], d2));
            }
        }
        best.map(|(p, _)| p)
    }

    /// Honest-adaptation fallback for rooms with no buildable region at
    /// all: the least-steep above-water cell (nearest the origin on
    /// ties), or the least-steep cell overall if the room is entirely
    /// submerged. Always returns a position.
    pub fn least_bad_site(&self) -> [f32; 2] {
        let mut best: Option<(usize, f32, f32)> = None; // (index, slope, d2)
        let mut best_any: Option<(usize, f32)> = None;
        for i in 0..self.grid * self.grid {
            let (x, z) = (i % self.grid, i / self.grid);
            let wx = x as f32 * self.cell - self.half;
            let wz = z as f32 * self.cell - self.half;
            let d2 = wx * wx + wz * wz;
            let s = self.slopes[i];
            if best_any.is_none_or(|(_, bs)| s < bs) {
                best_any = Some((i, s));
            }
            if self.heights[i] > self.water_y + WATER_MARGIN {
                let better = match best {
                    None => true,
                    Some((_, bs, bd2)) => s < bs - 1e-6 || (s < bs + 1e-6 && d2 < bd2),
                };
                if better {
                    best = Some((i, s, d2));
                }
            }
        }
        let i = best.map(|(i, _, _)| i).unwrap_or_else(|| {
            best_any.map(|(i, _)| i).unwrap_or(0) // grid ≥ 2, so cell 0 exists
        });
        let (x, z) = (i % self.grid, i / self.grid);
        [
            x as f32 * self.cell - self.half,
            z as f32 * self.cell - self.half,
        ]
    }

    fn cell_index(&self, world: [f32; 2]) -> (usize, usize) {
        let x = ((world[0] + self.half) / self.cell).round() as i32;
        let z = ((world[1] + self.half) / self.cell).round() as i32;
        (
            x.clamp(0, self.grid as i32 - 1) as usize,
            z.clamp(0, self.grid as i32 - 1) as usize,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a probe from a closure over grid coordinates.
    fn probe_from(
        grid: usize,
        cell: f32,
        water_y: f32,
        slope_limit: f32,
        f: impl Fn(usize, usize) -> f32,
    ) -> TerrainProbe {
        let mut data = vec![0.0_f32; grid * grid];
        for z in 0..grid {
            for x in 0..grid {
                data[z * grid + x] = f(x, z);
            }
        }
        let map = HeightmapData {
            width: grid as u32,
            height: grid as u32,
            scale: cell,
            data,
        };
        TerrainProbe::new(&map, water_y, slope_limit)
    }

    #[test]
    fn flat_map_is_one_room_wide_region() {
        let p = probe_from(16, 4.0, 0.0, 0.3, |_, _| 10.0);
        assert_eq!(p.regions().len(), 1);
        let r = &p.regions()[0];
        assert_eq!(r.cell_count, 256);
        assert!((r.area_m2 - 256.0 * 16.0).abs() < 1e-3);
        // Centroid at the room origin, which the region contains.
        assert!(r.centroid[0].abs() < 1e-3 && r.centroid[1].abs() < 1e-3);
        assert!(r.min_spawn_dist < 4.0);
    }

    #[test]
    fn ravine_splits_two_plateaus() {
        // Left and right plateaus at h=20 with a deep ravine column in
        // the middle; the ravine walls exceed the slope limit.
        let p = probe_from(17, 4.0, 0.0, 0.3, |x, _| {
            if (7..=9).contains(&x) { 2.0 } else { 20.0 }
        });
        // Two plateau regions; the ravine floor is its own flat strip but
        // low — still above water 0, so it also segments. Expect exactly:
        // left plateau, right plateau, ravine floor (x==8 column only,
        // 17 cells).
        assert_eq!(p.regions().len(), 3, "{:?}", p.regions());
        // Largest two are the plateaus (the columns bordering the ravine
        // wall are rightly excluded — their steepest neighbour is the drop).
        assert!(p.regions()[0].cell_count >= 17 * 6);
        assert!(p.regions()[1].cell_count >= 17 * 6);
        // One plateau centroid is left of origin, the other right.
        let cx0 = p.regions()[0].centroid[0];
        let cx1 = p.regions()[1].centroid[0];
        assert!(cx0 * cx1 < 0.0, "plateaus on opposite sides: {cx0} {cx1}");
    }

    #[test]
    fn water_drowns_regions_but_least_bad_site_survives() {
        let p = probe_from(8, 4.0, 50.0, 0.3, |_, _| 10.0);
        assert!(p.regions().is_empty(), "everything is under water");
        let s = p.least_bad_site();
        assert!(s[0].is_finite() && s[1].is_finite());
    }

    #[test]
    fn steep_ramp_is_not_buildable() {
        // Slope 1.0 everywhere (rise 4 per 4 m cell) with limit 0.3.
        let p = probe_from(12, 4.0, -100.0, 0.3, |x, _| x as f32 * 4.0);
        assert!(p.regions().is_empty(), "{:?}", p.regions());
        // The fallback still yields a position on the map.
        let s = p.least_bad_site();
        assert!(s[0].abs() <= 24.0 && s[1].abs() <= 24.0);
    }

    #[test]
    fn snap_respects_keep_clear_and_region_bounds() {
        let p = probe_from(16, 4.0, 0.0, 0.3, |_, _| 10.0);
        let r = &p.regions()[0];
        // Snapping to a far-outside point clamps to the region.
        let s = p.snap_to_region(r, [500.0, 0.0], &[]).unwrap();
        assert!(s[0] <= 32.0, "snapped inside the map: {s:?}");
        // A keep-clear disc over the desired point pushes the snap out.
        let s2 = p
            .snap_to_region(r, [0.0, 0.0], &[([0.0, 0.0], 12.0)])
            .unwrap();
        let d = (s2[0].powi(2) + s2[1].powi(2)).sqrt();
        assert!(d >= 12.0, "keep-clear violated: {s2:?} (d {d})");
        // A keep-clear disc covering the whole map yields None.
        assert!(
            p.snap_to_region(r, [0.0, 0.0], &[([0.0, 0.0], 1000.0)])
                .is_none()
        );
    }

    #[test]
    fn slope_and_height_queries_use_centred_world_coords() {
        let p = probe_from(16, 4.0, 0.0, 0.3, |x, _| x as f32);
        // Height rises with grid x; the centre column sits at x=7..8.
        assert!(p.height_at([-30.0, 0.0]) < p.height_at([30.0, 0.0]));
        // Constant gradient 1/4 per metre → slope 0.25 everywhere.
        assert!((p.slope_at([0.0, 0.0]) - 0.25).abs() < 1e-3);
    }
}

#[cfg(test)]
mod proxy_fidelity {
    //! [`TerrainProbe::relax_unsatisfiable_bands`] decides whether a
    //! microbiome band (#913) is worth keeping by measuring coverage on the
    //! **proxy** heightmap, while the sampler that honours the band reads
    //! the **full-resolution** map. That is only sound while the two agree
    //! about the terrain's height distribution, so guard it.
    //!
    //! This was written as a throwaway diagnostic while chasing a scatter
    //! that placed nothing, to rule the proxy in or out. It ruled it out —
    //! the two track each other to within a couple of percent — and the
    //! measurement is worth keeping, because if that ever stops being true
    //! the relaxation starts trusting the wrong map and the symptom is a
    //! silently empty patch of ground rather than a failure.
    use crate::pds::RoomRecord;

    #[test]
    fn proxy_tracks_the_full_map_closely_enough_to_relax_bands_against() {
        for seed in [1u64, 3, 9, 12] {
            let record = RoomRecord::default_for_seed(seed, "did:plc:probe");
            let cfg = crate::pds::find_terrain_config(&record)
                .cloned()
                .unwrap_or_default();
            let proxy = gen_jobs::run_heightmap_proxy(&crate::terrain::heightmap_params(&cfg), 96);
            let full = crate::terrain::rebuild_heightmap_for_record(&record);

            let pmax = proxy.data.iter().cloned().fold(f32::MIN, f32::max);
            let fmax = full.data().iter().cloned().fold(f32::MIN, f32::max);
            let pmean = proxy.data.iter().sum::<f32>() / proxy.data.len() as f32;
            let fmean = full.data().iter().sum::<f32>() / full.data().len() as f32;

            // Generous bounds — this guards against the proxy drifting into
            // a different terrain, not against ordinary resampling error.
            let rel = |a: f32, b: f32| (a - b).abs() / b.abs().max(1.0);
            assert!(
                rel(pmax, fmax) < 0.10,
                "seed {seed}: proxy max {pmax:.1} vs full {fmax:.1}"
            );
            assert!(
                rel(pmean, fmean) < 0.10,
                "seed {seed}: proxy mean {pmean:.1} vs full {fmean:.1}"
            );
        }
    }
}
