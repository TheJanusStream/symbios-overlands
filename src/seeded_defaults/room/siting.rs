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
