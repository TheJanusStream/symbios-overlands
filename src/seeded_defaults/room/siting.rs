//! Terrain probe - flat-region segmentation over the derive-time proxy
//! heightmap (#905).
//!
//! The settlement deriver used to place structures blind, by RNG angles
//! and distances alone; the compile-time snap then draped them over
//! whatever the terrain happened to be - including 45° rock faces. This
//! module gives the deriver eyes: a low-resolution proxy of the room's
//! heightmap (see `gen_jobs::run_heightmap_proxy`) is segmented into
//! **buildable regions** - connected spans of ground that are both
//! gentler than a slope limit and above the water line - and the
//! settlement deriver sites its clusters inside them. Within a region it
//! reads each candidate site's ground under the member's whole footprint
//! ([`TerrainProbe::footprint`], #1773), not at one cell.
//!
//! Coordinates: the terrain mesh is spawned centred on the room origin
//! (the spawn point), so every world-facing API here uses centred XZ -
//! `[-extent/2, +extent/2]` - matching settlement member offsets.
//!
//! The proxy is an approximation of the full-resolution map, so
//! consumers pair a conservative [slope limit](TerrainProbe::new) with
//! the compile-time slope nudge in the world builder (the safety net
//! that catches proxy-vs-full divergence).

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use gen_jobs::HeightmapData;

/// Cells this close to the water line (in metres of height) count as
/// shoreline and are not buildable - keeps settlements off the beach
/// splash zone even when the slope there is gentle.
const WATER_MARGIN: f32 = 0.75;

/// Minimum cell count for a connected span to count as a region at all.
/// Smaller specks are proxy noise, not building ground.
const MIN_REGION_CELLS: usize = 3;

/// Share of a scatter's disc that must satisfy a microbiome band for the
/// band to be kept (#913). Below this the band is treated as unsatisfiable
/// and dropped, because the ground it leaves also has to survive the biome
/// allow-list and the slope cutoff - so a band clinging to a few percent of
/// the disc empties the scatter rather than zoning it.
const MIN_BAND_COVERAGE: f32 = 0.08;

/// How far out the ring a site's [prominence](TerrainProbe::prominence) is
/// read against lies (m): a few proxy cells, the scale of a crest a castle
/// crowns or a valley a farm sits in (#1775).
const PROMINENCE_RING: f32 = 60.0;

/// The farthest from the water a shore building still feels its pull (m):
/// beyond it every site is as far from the shore as any other (#1775).
const SHORE_REACH: f32 = 200.0;

/// How far a site stands over its ring to be a crest a settlement is drawn
/// to lead from (m, #1783) - and under it, a valley floor.
const OFFERED_RISE: f32 = 6.0;

/// How near the water a site lies to be a shore a settlement is drawn to
/// lead from (m, #1783).
const OFFERED_SHORE: f32 = 60.0;

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
    /// Half the world extent - world XZ = grid index × cell − half.
    half: f32,
    heights: Vec<f32>,
    /// Per-cell slope as rise/run to the steepest 4-neighbour.
    slopes: Vec<f32>,
    water_y: f32,
    regions: Vec<BuildableRegion>,
    /// Per cell, metres to the nearest cell the water covers - infinite
    /// on a map with none (#1775).
    shore: Vec<f32>,
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
        // explicit stack - the proxy is small but recursion depth is
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
        let shore = shore_distances(&heights, grid, cell, water_y);

        Self {
            grid,
            cell,
            half,
            heights,
            slopes,
            water_y,
            regions,
            shore,
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

    /// Half the map's side, metres: centred world XZ runs from minus this
    /// to this.
    pub fn half_extent(&self) -> f32 {
        self.half
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
    /// `band` (offset into the band's frame - pass the water line for an
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
    /// terrain that sits almost entirely below an altitude floor - or above
    /// a riparian ceiling - the band stops zoning the patch and simply
    /// deletes it: measured across 14 seeds, one lichen scatter went to
    /// 0/230 placed. Relaxing to "unbanded" there keeps the patch present,
    /// which is the lesser wrong - a slightly mis-zoned patch reads as
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
        // reeds wading the margin. Relaxing it would not mis-zone a patch -
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

    /// The buildable cells of `region` within `within` of `desired`, nearest
    /// first, that keep at least the paired radius from every entry of
    /// `keep_clear`: the sites a member is tried on (#1773). Each is its
    /// cell's centred world XZ. Ties keep the region's own cell order, as
    /// the nearest-cell snap before them took them (a stable sort), so the
    /// nearest site is the cell that snap chose - and peers agree.
    pub fn sites_near(
        &self,
        region: &BuildableRegion,
        desired: [f32; 2],
        within: f32,
        keep_clear: &[([f32; 2], f32)],
    ) -> Vec<[f32; 2]> {
        let mut sites: Vec<(f32, [f32; 2])> = region
            .cells
            .iter()
            .filter_map(|&c| {
                let at = self.cell_world(c);
                let (dx, dz) = (at[0] - desired[0], at[1] - desired[1]);
                let d2 = dx * dx + dz * dz;
                (d2 <= within * within && clear_of(at, keep_clear)).then_some((d2, at))
            })
            .collect();
        sites.sort_by(|a, b| a.0.total_cmp(&b.0));
        sites.into_iter().map(|(_, at)| at).collect()
    }

    /// Cell `c`'s centred world XZ.
    fn cell_world(&self, c: u32) -> [f32; 2] {
        let (x, z) = ((c as usize) % self.grid, (c as usize) / self.grid);
        [
            x as f32 * self.cell - self.half,
            z as f32 * self.cell - self.half,
        ]
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

    /// The proxy's height at a centred world XZ, read between its cells
    /// (bilinear), clamped at the map's edge.
    pub fn height_between(&self, world: [f32; 2]) -> f32 {
        let last = (self.grid - 1) as f32;
        let gx = ((world[0] + self.half) / self.cell).clamp(0.0, last);
        let gz = ((world[1] + self.half) / self.cell).clamp(0.0, last);
        let (x0, z0) = (gx.floor() as usize, gz.floor() as usize);
        let (x1, z1) = ((x0 + 1).min(self.grid - 1), (z0 + 1).min(self.grid - 1));
        let (tx, tz) = (gx - x0 as f32, gz - z0 as f32);
        let at = |x: usize, z: usize| self.heights[z * self.grid + x];
        let near = at(x0, z0) + (at(x1, z0) - at(x0, z0)) * tx;
        let far = at(x0, z1) + (at(x1, z1) - at(x0, z1)) * tx;
        near + (far - near) * tz
    }

    /// The slope (rise/run) at a centred world XZ, as the compile's walk
    /// reads one: central differences either side of it, here half a proxy
    /// cell apart, between the cells.
    fn slope_between(&self, world: [f32; 2]) -> f32 {
        let s = self.cell * 0.5;
        let h = |dx: f32, dz: f32| self.height_between([world[0] + dx, world[1] + dz]);
        let gx = (h(s, 0.0) - h(-s, 0.0)) / (2.0 * s);
        let gz = (h(0.0, s) - h(0.0, -s)) / (2.0 * s);
        (gx * gx + gz * gz).sqrt()
    }

    /// How the ground under a footprint disc of `radius` round `center`
    /// lies on the proxy (#1773): read between the cells over the disc -
    /// rings a half cell apart, at most, out to its rim.
    pub fn footprint(&self, center: [f32; 2], radius: f32) -> FootprintGround {
        self.footprint_unless(center, radius, |_| false)
            .expect("a reading nothing refuses is whole")
    }

    /// [`footprint`](Self::footprint), refused - `None` - at the first
    /// partial reading `refuses`: the middle and the rim, then the middle's
    /// and rim's slopes, then the disc ring by ring inward (#1773). A site
    /// that fails costs a few points rather than the whole disc, and the
    /// verdict is the whole reading's wherever `refuses` refuses a partial
    /// reading only when it would the whole one: as points are added a
    /// relief only grows, the ground's lowest only falls and the steepest
    /// only rises - nought before the slopes are read - all exactly, as a
    /// min and a max take no rounding.
    pub fn footprint_unless(
        &self,
        center: [f32; 2],
        radius: f32,
        refuses: impl Fn(&FootprintGround) -> bool,
    ) -> Option<FootprintGround> {
        let radius = radius.max(0.0);
        let rings = (radius / (self.cell * 0.5)).ceil().clamp(1.0, 24.0) as usize;
        let mut low = self.height_between(center);
        let mut high = low;
        let ring_in = |ring: usize, low: &mut f32, high: &mut f32| {
            let r = radius * ring as f32 / rings as f32;
            for &[dx, dz] in &RIM {
                let h = self.height_between([center[0] + dx * r, center[1] + dz * r]);
                *low = low.min(h);
                *high = high.max(h);
            }
        };
        let reading = |low: f32, high: f32, steepest: f32| FootprintGround {
            relief: high - low,
            above_water: low - self.water_y,
            steepest,
        };
        ring_in(rings, &mut low, &mut high);
        if refuses(&reading(low, high, 0.0)) {
            return None;
        }
        // The compile walk's own test geometry: the middle and eight rim
        // points at the footprint's radius (`compile::slope`).
        let steepest = std::iter::once(center)
            .chain(
                RIM.iter()
                    .step_by(RIM.len() / 8)
                    .map(|&[dx, dz]| [center[0] + dx * radius, center[1] + dz * radius]),
            )
            .map(|p| self.slope_between(p))
            .fold(0.0_f32, f32::max);
        if refuses(&reading(low, high, steepest)) {
            return None;
        }
        for ring in (1..rings).rev() {
            ring_in(ring, &mut low, &mut high);
            if refuses(&reading(low, high, steepest)) {
                return None;
            }
        }
        Some(reading(low, high, steepest))
    }

    /// The water line the probe was segmented against.
    pub fn water_y(&self) -> f32 {
        self.water_y
    }

    /// How far the ground at `at` stands above the ground round it (m):
    /// its height less the mean height on a ring [`PROMINENCE_RING`] out -
    /// above nought on a crest or spur, below it in a valley (#1775).
    pub fn prominence(&self, at: [f32; 2]) -> f32 {
        let ring = RIM
            .iter()
            .map(|&[dx, dz]| {
                self.height_between([at[0] + dx * PROMINENCE_RING, at[1] + dz * PROMINENCE_RING])
            })
            .sum::<f32>()
            / RIM.len() as f32;
        self.height_between(at) - ring
    }

    /// Metres from `at` to the nearest water, read at its cell; infinite on
    /// a map the water covers nowhere (#1775).
    pub fn shore_distance(&self, at: [f32; 2]) -> f32 {
        let (x, z) = self.cell_index(at);
        self.shore[z * self.grid + x]
    }

    /// Which way the nearest water lies from `at`, `within` metres at the
    /// most: a unit vector to the middle of the nearest cell at or under the
    /// water line, the first of equals by cell - `None` where none lies that
    /// near (#1780). The way a building on the shore looks out.
    pub fn toward_water(&self, at: [f32; 2], within: f32) -> Option<[f32; 2]> {
        let (cx, cz) = self.cell_index(at);
        let span = (within / self.cell).ceil() as usize;
        let mut best: Option<(f32, [f32; 2])> = None;
        for z in cz.saturating_sub(span)..=(cz + span).min(self.grid - 1) {
            for x in cx.saturating_sub(span)..=(cx + span).min(self.grid - 1) {
                let c = z * self.grid + x;
                if self.heights[c] > self.water_y {
                    continue;
                }
                let water = self.cell_world(c as u32);
                let (dx, dz) = (water[0] - at[0], water[1] - at[1]);
                let d = dx * dx + dz * dz;
                if d > 0.0 && d <= within * within && best.is_none_or(|(kept, _)| d < kept) {
                    best = Some((d, [dx, dz]));
                }
            }
        }
        // Square root, not `hypot` (#1132): the way decides the facing.
        best.map(|(d, [dx, dz])| {
            let length = d.sqrt();
            [dx / length, dz / length]
        })
    }

    /// How well `at` suits a building of `prefer`, in metres of the ground's
    /// own measure (#1775): its [prominence](Self::prominence) for a crest,
    /// that turned over for a valley floor, its nearness to the water within
    /// [`SHORE_REACH`] for the shore, and nought for anywhere - the more the
    /// better.
    pub fn preference(&self, prefer: crate::catalogue::SitePreference, at: [f32; 2]) -> f32 {
        use crate::catalogue::SitePreference;
        match prefer {
            SitePreference::Anywhere => 0.0,
            SitePreference::Crest => self.prominence(at),
            SitePreference::ValleyFloor => -self.prominence(at),
            SitePreference::Shore => -self.shore_distance(at).min(SHORE_REACH),
        }
    }

    /// Whether `region` offers a site of `prefer` within `reach` of `from`
    /// (#1783): a crest standing [`OFFERED_RISE`] over its ring, a valley
    /// floor as far under it, the shore within [`OFFERED_SHORE`] of the
    /// water - anywhere always. What a settlement's landmark is drawn by.
    pub fn offers(
        &self,
        region: &BuildableRegion,
        prefer: crate::catalogue::SitePreference,
        from: [f32; 2],
        reach: f32,
    ) -> bool {
        use crate::catalogue::SitePreference;
        if prefer == SitePreference::Anywhere {
            return true;
        }
        region
            .cells
            .iter()
            .map(|&c| self.cell_world(c))
            .filter(|at| {
                let (dx, dz) = (at[0] - from[0], at[1] - from[1]);
                dx * dx + dz * dz <= reach * reach
            })
            .any(|at| self.suits(prefer, at))
    }

    /// Whether `at` is a site of `prefer` as [`Self::offers`] reads one
    /// (#1783): a crest, a valley floor or a shore by its measure, and any
    /// site for a building that goes anywhere.
    pub fn suits(&self, prefer: crate::catalogue::SitePreference, at: [f32; 2]) -> bool {
        use crate::catalogue::SitePreference;
        match prefer {
            SitePreference::Crest => self.prominence(at) >= OFFERED_RISE,
            SitePreference::ValleyFloor => self.prominence(at) <= -OFFERED_RISE,
            SitePreference::Shore => self.shore_distance(at) <= OFFERED_SHORE,
            SitePreference::Anywhere => true,
        }
    }

    /// Which way the ground at `at` falls, a unit vector - `None` where it
    /// falls less than `steeper_than` (rise/run): where a building's front
    /// looks out over its view, along the contour (#1775).
    pub fn downhill(&self, at: [f32; 2], steeper_than: f32) -> Option<[f32; 2]> {
        let s = self.cell * 0.5;
        let h = |dx: f32, dz: f32| self.height_between([at[0] + dx, at[1] + dz]);
        let gx = (h(s, 0.0) - h(-s, 0.0)) / (2.0 * s);
        let gz = (h(0.0, s) - h(0.0, -s)) / (2.0 * s);
        // Square root, not `hypot` (#1132): the slope decides the facing.
        let slope = (gx * gx + gz * gz).sqrt();
        (slope >= steeper_than && slope > 0.0).then(|| [-gx / slope, -gz / slope])
    }

    /// The buildable cell of `region`, clear of `keep_clear`, under whose
    /// footprint disc of `radius` the ground is flattest - dry ground before
    /// wet, ties to the cell's index: where a member that fits nowhere
    /// stands least badly (#1773). `None` when no cell is clear.
    pub fn flattest(
        &self,
        region: &BuildableRegion,
        keep_clear: &[([f32; 2], f32)],
        radius: f32,
    ) -> Option<[f32; 2]> {
        region
            .cells
            .iter()
            .map(|&c| (c, self.cell_world(c)))
            .filter(|&(_, at)| clear_of(at, keep_clear))
            .map(|(c, at)| {
                let ground = self.footprint(at, radius);
                (!ground.is_dry(), ground.relief, c, at)
            })
            .min_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)))
            .map(|(.., at)| at)
    }

    /// The basins whose floors lie between `near` and `far` of `from`
    /// (#1793), `over` metres or more over the water: each cell lower than
    /// its four neighbours there, filled as [`Self::basin`] fills it, those
    /// that hold water before `most` cells. In the order of their floors'
    /// cells.
    pub fn basins_near(
        &self,
        from: [f32; 2],
        near: f32,
        far: f32,
        over: f32,
        most: usize,
    ) -> Vec<Basin> {
        (0..self.heights.len() as u32)
            .filter(|&c| {
                let far_off = {
                    let [x, z] = self.cell_world(c);
                    ((x - from[0]) * (x - from[0]) + (z - from[1]) * (z - from[1])).sqrt()
                };
                (near..=far).contains(&far_off) && self.heights[c as usize] >= self.water_y + over
            })
            .filter_map(|c| self.basin(c as usize, most))
            .collect()
    }

    /// The basin round cell `start` (#1793), where it lies lower than its
    /// four neighbours (than those of a lower index, where they are level
    /// with it): filled from it as water fills it - the lowest cell
    /// round what it holds first, 4-connected as a lake's flood is
    /// (`water::bodies::lake_cells`), ties to the lower index - until the
    /// next is lower than the water already stands, where it runs over its
    /// rim. `None` where it is no dip, or reaches the map's edge or `most`
    /// cells first.
    fn basin(&self, start: usize, most: usize) -> Option<Basin> {
        let grid = self.grid;
        let around = |i: usize| {
            let (x, z) = (i % grid, i / grid);
            (x > 0 && z > 0 && x + 1 < grid && z + 1 < grid)
                .then(|| [i - 1, i + 1, i - grid, i + grid])
        };
        let floor = self.heights[start];
        // A floor of equal cells is the lowest index's.
        if around(start)?
            .iter()
            .any(|&n| self.heights[n] < floor || self.heights[n] == floor && n < start)
        {
            return None;
        }
        let mut seen = vec![false; self.heights.len()];
        seen[start] = true;
        let mut queue = BinaryHeap::new();
        queue.push(Reverse((Height(floor), start)));
        let mut level = floor;
        let mut cells = Vec::new();
        while let Some(Reverse((Height(height), i))) = queue.pop() {
            if height < level {
                return Some(Basin {
                    floor_at: self.cell_world(start as u32),
                    floor,
                    spill: level,
                    cells,
                });
            }
            let next = around(i)?;
            if cells.len() == most {
                return None;
            }
            level = height;
            cells.push((self.cell_world(i as u32), height));
            for n in next {
                if !seen[n] {
                    seen[n] = true;
                    queue.push(Reverse((Height(self.heights[n]), n)));
                }
            }
        }
        None
    }
}

/// A basin of the probe's ground (#1793): the dip round a cell lower than
/// its four neighbours, as [`TerrainProbe::basin`] fills it.
#[derive(Clone, Debug, PartialEq)]
pub struct Basin {
    /// Its lowest cell's middle, world XZ, and that cell's height.
    pub floor_at: [f32; 2],
    pub floor: f32,
    /// The height its water runs over its rim at.
    pub spill: f32,
    /// Its cells the water covers before it runs over, each `(middle,
    /// height)`, in the order it reaches them: their heights never fall.
    cells: Vec<([f32; 2], f32)>,
}

impl Basin {
    /// The middles of its cells water standing at `level` covers, in the
    /// order it reaches them: those lower than `level`, which come first.
    pub fn under(&self, level: f32) -> impl Iterator<Item = [f32; 2]> + '_ {
        self.cells
            .iter()
            .take_while(move |&&(_, height)| height < level)
            .map(|&(at, _)| at)
    }

    /// The height of its `n`th cell from the floor, counting from nought:
    /// the level under which water covers its first `n` cells - `None` past
    /// its last.
    pub fn level_for(&self, n: usize) -> Option<f32> {
        self.cells.get(n).map(|&(_, height)| height)
    }
}

/// A height ordered as `f32::total_cmp` orders it, for a heap: one order on
/// every peer.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Height(f32);

impl Eq for Height {}

impl PartialOrd for Height {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Height {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// Whether `at` keeps at least the paired radius from every entry of
/// `keep_clear`.
fn clear_of(at: [f32; 2], keep_clear: &[([f32; 2], f32)]) -> bool {
    keep_clear.iter().all(|&(p, r)| {
        let (dx, dz) = (at[0] - p[0], at[1] - p[1]);
        dx * dx + dz * dz >= r * r
    })
}

/// Per cell of a `grid` x `grid` map of `cell`-metre cells, the distance to
/// the nearest cell at or under `water_y` (#1775): a two-pass chamfer, a
/// side step a cell and a diagonal one a cell times the square root of
/// two, within a few percent of the straight line and the same on every
/// platform. Infinite everywhere on a map with no water.
fn shore_distances(heights: &[f32], grid: usize, cell: f32, water_y: f32) -> Vec<f32> {
    let diagonal = cell * std::f32::consts::SQRT_2;
    let mut d: Vec<f32> = heights
        .iter()
        .map(|&h| if h <= water_y { 0.0 } else { f32::INFINITY })
        .collect();
    let at = |x: usize, z: usize| z * grid + x;
    for z in 0..grid {
        for x in 0..grid {
            let mut best = d[at(x, z)];
            if x > 0 {
                best = best.min(d[at(x - 1, z)] + cell);
            }
            if z > 0 {
                best = best.min(d[at(x, z - 1)] + cell);
                if x > 0 {
                    best = best.min(d[at(x - 1, z - 1)] + diagonal);
                }
                if x + 1 < grid {
                    best = best.min(d[at(x + 1, z - 1)] + diagonal);
                }
            }
            d[at(x, z)] = best;
        }
    }
    for z in (0..grid).rev() {
        for x in (0..grid).rev() {
            let mut best = d[at(x, z)];
            if x + 1 < grid {
                best = best.min(d[at(x + 1, z)] + cell);
            }
            if z + 1 < grid {
                best = best.min(d[at(x, z + 1)] + cell);
                if x + 1 < grid {
                    best = best.min(d[at(x + 1, z + 1)] + diagonal);
                }
                if x > 0 {
                    best = best.min(d[at(x - 1, z + 1)] + diagonal);
                }
            }
            d[at(x, z)] = best;
        }
    }
    d
}

/// Twenty-four directions round a circle, fifteen degrees apart - every
/// third one the compile walk's eight. Constant, so every platform reads
/// the same points (#1132).
pub(super) const RIM: [[f32; 2]; 24] = {
    // sin and cos of k x 15 degrees, k = 0..24.
    const S: [f32; 24] = [
        0.0,
        0.258_819,
        0.5,
        0.707_106_77,
        0.866_025_4,
        0.965_925_8,
        1.0,
        0.965_925_8,
        0.866_025_4,
        0.707_106_77,
        0.5,
        0.258_819,
        0.0,
        -0.258_819,
        -0.5,
        -0.707_106_77,
        -0.866_025_4,
        -0.965_925_8,
        -1.0,
        -0.965_925_8,
        -0.866_025_4,
        -0.707_106_77,
        -0.5,
        -0.258_819,
    ];
    let mut rim = [[0.0_f32; 2]; 24];
    let mut k = 0;
    while k < 24 {
        // cos(a) = sin(a + 90 degrees): six steps on.
        rim[k] = [S[k], S[(k + 6) % 24]];
        k += 1;
    }
    rim
};

/// How the ground under a footprint lies on the proxy
/// ([`TerrainProbe::footprint`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FootprintGround {
    /// Its highest ground less its lowest, metres: what a plinth spans.
    pub relief: f32,
    /// Its lowest ground over the water line, metres.
    pub above_water: f32,
    /// The steepest slope (rise/run) at its middle and its eight rim points.
    pub steepest: f32,
}

impl FootprintGround {
    /// Whether all of it stands clear of the water, by the margin a
    /// buildable cell keeps ([`WATER_MARGIN`]).
    pub fn is_dry(&self) -> bool {
        self.above_water > WATER_MARGIN
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
        // low - still above water 0, so it also segments. Expect exactly:
        // left plateau, right plateau, ravine floor (x==8 column only,
        // 17 cells).
        assert_eq!(p.regions().len(), 3, "{:?}", p.regions());
        // Largest two are the plateaus (the columns bordering the ravine
        // wall are rightly excluded - their steepest neighbour is the drop).
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
    fn sites_respect_keep_clear_reach_and_region_bounds() {
        let p = probe_from(16, 4.0, 0.0, 0.3, |_, _| 10.0);
        let r = &p.regions()[0];
        // The sites nearest a far-outside point are the region's own.
        let s = p.sites_near(r, [500.0, 0.0], f32::INFINITY, &[])[0];
        assert!(s[0] <= 32.0, "a site inside the map: {s:?}");
        // ...and none of them is within reach of it.
        assert!(p.sites_near(r, [500.0, 0.0], 100.0, &[]).is_empty());
        // Nearest first.
        let sites = p.sites_near(r, [0.0, 0.0], 20.0, &[]);
        let d = |s: &[f32; 2]| s[0].hypot(s[1]);
        assert!(sites.windows(2).all(|w| d(&w[0]) <= d(&w[1])));
        assert!(sites.iter().all(|s| d(s) <= 20.0));
        // A keep-clear disc over the desired point pushes every site out.
        let s2 = p.sites_near(r, [0.0, 0.0], f32::INFINITY, &[([0.0, 0.0], 12.0)]);
        assert!(s2.iter().all(|s| d(s) >= 12.0), "keep-clear violated");
        // A keep-clear disc covering the whole map leaves no site.
        assert!(
            p.sites_near(r, [0.0, 0.0], f32::INFINITY, &[([0.0, 0.0], 1000.0)])
                .is_empty()
        );
    }

    #[test]
    fn a_footprint_reads_the_ground_between_the_cells() {
        // A ramp rising 1 m per metre along x past x = 0, flat before it.
        let p = probe_from(32, 4.0, -100.0, 0.3, |x, _| {
            (x as f32 * 4.0 - 62.0).max(0.0)
        });
        // On the flat, well clear of the ramp: no relief, no slope.
        let flat = p.footprint([-30.0, 0.0], 8.0);
        assert!(flat.relief < 1e-4 && flat.steepest < 1e-4, "{flat:?}");
        // Straddling the foot of the ramp: the disc's far rim is 8 m up it.
        let foot = p.footprint([0.0, 0.0], 8.0);
        assert!((foot.relief - 8.0).abs() < 0.5, "{foot:?}");
        assert!(foot.steepest > 0.9, "{foot:?}");
        assert!(foot.is_dry());
        // Under water everywhere: not dry.
        let wet = probe_from(8, 4.0, 50.0, 0.3, |_, _| 10.0);
        assert!(!wet.footprint([0.0, 0.0], 4.0).is_dry());
    }

    #[test]
    fn the_flattest_site_is_dry_and_least_in_relief() {
        // Dry ground everywhere; a ramp east of x = 0, flat to the west.
        let p = probe_from(32, 4.0, -100.0, 10.0, |x, _| {
            (x as f32 * 4.0 - 62.0).max(0.0) * 0.2
        });
        let r = &p.regions()[0];
        let at = p.flattest(r, &[], 8.0).unwrap();
        assert!(at[0] < -8.0, "the flat side: {at:?}");
        assert!(p.flattest(r, &[([0.0, 0.0], 1000.0)], 8.0).is_none());
    }

    /// #1775: a hilltop stands proud of its ring, the foot of the hill below
    /// it, and flat ground far from both neither.
    #[test]
    fn a_crest_stands_proud_and_its_foot_sinks() {
        // A cone 30 m tall and 80 m round in the middle of a flat map.
        let p = probe_from(48, 8.0, -100.0, 10.0, |x, z| {
            let (wx, wz) = (x as f32 * 8.0 - 188.0, z as f32 * 8.0 - 188.0);
            30.0 * (1.0 - wx.hypot(wz) / 80.0).max(0.0)
        });
        assert!(
            p.prominence([0.0, 0.0]) > 15.0,
            "{}",
            p.prominence([0.0, 0.0])
        );
        assert!(
            p.prominence([90.0, 0.0]) < 0.0,
            "{}",
            p.prominence([90.0, 0.0])
        );
        assert!(p.prominence([150.0, 0.0]).abs() < 1e-3);
    }

    /// #1775: the shore is measured from the nearest water - nought on it,
    /// the straight line inland along a straight coast, and no shore at all
    /// on a map without water.
    #[test]
    fn the_shore_is_measured_from_the_water() {
        // The sea west of x = 0, the land rising east of it.
        let p = probe_from(32, 8.0, 0.0, 10.0, |x, _| x as f32 * 8.0 - 124.0);
        assert_eq!(p.shore_distance([-50.0, 0.0]), 0.0);
        let near = p.shore_distance([20.0, 0.0]);
        let far = p.shore_distance([100.0, 0.0]);
        assert!((near - 24.0).abs() < 1e-3, "{near}");
        assert!((far - near - 80.0).abs() < 1e-3, "{near} {far}");
        let dry = probe_from(16, 8.0, -100.0, 10.0, |_, _| 10.0);
        assert!(dry.shore_distance([0.0, 0.0]).is_infinite());
    }

    /// #1780: the water lies the way of its nearest cell - straight out
    /// across a straight coast - and nowhere past the reach asked.
    #[test]
    fn the_way_to_the_water_is_toward_its_nearest_cell() {
        // The sea west of x = 0, the land rising east of it.
        let p = probe_from(32, 8.0, 0.0, 10.0, |x, _| x as f32 * 8.0 - 124.0);
        assert_eq!(p.toward_water([20.0, 4.0], 30.0), Some([-1.0, 0.0]));
        assert_eq!(p.toward_water([100.0, 4.0], 30.0), None);
        let dry = probe_from(16, 8.0, -100.0, 10.0, |_, _| 10.0);
        assert_eq!(dry.toward_water([0.0, 0.0], 1000.0), None);
    }

    /// #1775: downhill points down the slope, and nowhere on gentler ground
    /// than asked.
    #[test]
    fn downhill_points_down_the_slope() {
        let p = probe_from(32, 8.0, -100.0, 10.0, |x, _| x as f32 * 8.0 * 0.1);
        let d = p.downhill([0.0, 0.0], 0.06).expect("a 1-in-10 slope");
        assert!((d[0] + 1.0).abs() < 1e-4 && d[1].abs() < 1e-4, "{d:?}");
        assert!(p.downhill([0.0, 0.0], 0.2).is_none());
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
    //! that placed nothing, to rule the proxy in or out. It ruled it out -
    //! the two track each other to within a couple of percent - and the
    //! measurement is worth keeping, because if that ever stops being true
    //! the relaxation starts trusting the wrong map and the symptom is a
    //! silently empty patch of ground rather than a failure.
    #[test]
    fn proxy_tracks_the_full_map_closely_enough_to_relax_bands_against() {
        for seed in [1u64, 3, 9, 12] {
            // The procedural terrain the proxy stands for, whatever source
            // the seed draws (#1589): seed 3 draws Berlin.
            let record = crate::seeded_defaults::room::build::build_room_with_source(
                seed,
                "did:plc:probe",
                None,
            );
            let cfg = crate::pds::find_terrain_config(&record)
                .cloned()
                .unwrap_or_default();
            let proxy = gen_jobs::run_heightmap_proxy(&crate::terrain::heightmap_params(&cfg), 96);
            let full = crate::terrain::rebuild_heightmap_for_record(&record);

            let pmax = proxy.data.iter().cloned().fold(f32::MIN, f32::max);
            let fmax = full.data().iter().cloned().fold(f32::MIN, f32::max);
            let pmean = proxy.data.iter().sum::<f32>() / proxy.data.len() as f32;
            let fmean = full.data().iter().sum::<f32>() / full.data().len() as f32;

            // Generous bounds - this guards against the proxy drifting into
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
