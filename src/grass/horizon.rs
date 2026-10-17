//! Grass tiles the ground hides (#1693): a tile behind a hill or a ridge
//! from the camera is not drawn.
//!
//! The ground is a heightmap, so whether it hides a tile is a question
//! along the ground between them: whether, somewhere on the way, the ground
//! stands above every sight line from the camera to the tile's blades. The
//! test is a horizon march over a height pyramid - what web and heightmap
//! terrain engines use for vegetation where a depth pyramid needs compute
//! WebGL2 does not have - made conservative for a tile: at each point it
//! looks at, it takes the *lowest* ground across the whole width the tile's
//! sight lines spread to there, against the *highest* of those lines - so a
//! gap in a ridge that any line to any blade passes through keeps the tile
//! drawn. A blocker it does not look at costs only a tile drawn for
//! nothing, never a tile missing. Sight lines meet at the eye only through
//! a perspective lens; the grass asks only for such an eye
//! (`grass::lay_grass_tiles`).

use bevy::prelude::*;
use bevy_symbios_ground::HeightMap;

use super::tiles::{TALLEST_BLADE, WIDEST_LEAN};
use crate::config::grass as cfg;

/// The most heights one look at the ground reads: past it the look reads a
/// coarser level, whose lowest is lower or the same.
const MOST_READS: usize = 64;

/// The ground's lowest heights over ever larger squares, for the horizon
/// test: level `k` (from 1) holds, for each square of `2^k` cells, the
/// least height of the heightmap vertices on it - its corners and edges
/// included, so a square's least is never above the ground drawn over it,
/// which runs straight between vertices.
pub struct GroundFloor {
    /// Metres between vertices, the ground's half width, vertices a side.
    cell: f32,
    half: f32,
    vertices: usize,
    /// Level `k` at `k - 1`: texels a side, and their heights row by row.
    levels: Vec<(usize, Vec<f32>)>,
}

impl GroundFloor {
    /// The pyramid of heightmap `hm`, centred on the origin as the terrain
    /// is.
    pub fn of(hm: &HeightMap) -> Self {
        let vertices = hm.width().min(hm.height());
        let cell = hm.scale();
        let half = (vertices.saturating_sub(1)) as f32 * cell * 0.5;
        let mut levels = Vec::new();
        // Level 1: each texel the least of the 3 x 3 vertices of its 2 x 2
        // cells.
        let mut side = vertices.saturating_sub(1).div_ceil(2);
        if side > 0 {
            let last = vertices - 1;
            let mut data = vec![f32::INFINITY; side * side];
            for j in 0..side {
                for i in 0..side {
                    let mut least = f32::INFINITY;
                    for z in 2 * j..=(2 * j + 2).min(last) {
                        for x in 2 * i..=(2 * i + 2).min(last) {
                            least = least.min(hm.get(x, z));
                        }
                    }
                    data[j * side + i] = least;
                }
            }
            levels.push((side, data));
        }
        // Each level above: the least of the four below.
        while side > 1 {
            let (below_side, below) = levels.last().expect("a level below");
            let below_side = *below_side;
            side = below_side.div_ceil(2);
            let mut data = vec![f32::INFINITY; side * side];
            for j in 0..side {
                for i in 0..side {
                    let mut least = f32::INFINITY;
                    for z in 2 * j..(2 * j + 2).min(below_side) {
                        for x in 2 * i..(2 * i + 2).min(below_side) {
                            least = least.min(below[z * below_side + x]);
                        }
                    }
                    data[j * side + i] = least;
                }
            }
            levels.push((side, data));
        }
        Self {
            cell,
            half,
            vertices,
            levels,
        }
    }

    /// The least height of the ground over the world square `lo..hi`:
    /// never above the ground drawn anywhere in it. `None` when the square
    /// leaves the ground, where nothing stands to hide anything.
    fn lowest(&self, hm: &HeightMap, lo: Vec2, hi: Vec2) -> Option<f32> {
        if self.vertices < 2
            || lo.x < -self.half
            || lo.y < -self.half
            || hi.x > self.half
            || hi.y > self.half
        {
            return None;
        }
        // The vertices of every cell the square touches.
        let last = (self.vertices - 1) as f32;
        let grid = |v: f32| (v + self.half) / self.cell;
        let (x0, z0) = (
            grid(lo.x).floor().clamp(0.0, last) as usize,
            grid(lo.y).floor().clamp(0.0, last) as usize,
        );
        let (x1, z1) = (
            grid(hi.x).ceil().clamp(0.0, last) as usize,
            grid(hi.y).ceil().clamp(0.0, last) as usize,
        );
        // The finest level whose texels the square's shorter side crosses
        // at most two of, every texel it covers read there - level 0 being
        // the vertices themselves. A square thin one way, as the horizon
        // test asks for, is read finely along its length rather than in
        // texels as wide as it is long, which would blur a thin ridge into
        // the ground beside it. Texel `t` of level `k` holds vertices
        // `t 2^k ..= (t + 1) 2^k`.
        if (x1 - x0).min(z1 - z0) <= 2 && (x1 - x0 + 1) * (z1 - z0 + 1) <= MOST_READS {
            let mut least = f32::INFINITY;
            for z in z0..=z1 {
                for x in x0..=x1 {
                    least = least.min(hm.get(x, z));
                }
            }
            return Some(least);
        }
        for (k, (side, data)) in self.levels.iter().enumerate() {
            let shift = k + 1;
            let texel = |v: usize| (v >> shift).min(side - 1);
            let (i0, i1) = (texel(x0), texel(x1.saturating_sub(1).max(x0)));
            let (j0, j1) = (texel(z0), texel(z1.saturating_sub(1).max(z0)));
            if (i1 - i0).min(j1 - j0) <= 1 && (i1 - i0 + 1) * (j1 - j0 + 1) <= MOST_READS {
                let mut least = f32::INFINITY;
                for j in j0..=j1 {
                    for i in i0..=i1 {
                        least = least.min(data[j * side + i]);
                    }
                }
                return Some(least);
            }
        }
        None
    }

    /// Whether the ground of heightmap `hm` hides every blade of grass tile
    /// `coord` - its ground from `heights.0` to `heights.1`, blades up to
    /// [`TALLEST_BLADE`] tall and leaning up to [`WIDEST_LEAN`] - from
    /// `eye`: whether, at one of [`cfg::HORIZON_SAMPLES`] points between
    /// them, the lowest ground across the width the tile's sight lines
    /// spread to there stands above the highest of them. A tile nearer than
    /// [`cfg::HORIZON_NEAR`] is never hidden, nor any from an eye under the
    /// ground, which sees the grass from below.
    pub fn hides(&self, hm: &HeightMap, eye: Vec3, coord: IVec2, heights: (f32, f32)) -> bool {
        let centre = (coord.as_vec2() + 0.5) * cfg::TILE_M;
        let from = Vec2::new(eye.x, eye.z);
        let below = Vec2::splat(self.cell);
        if self
            .lowest(hm, from - below, from + below)
            .is_some_and(|ground| eye.y < ground)
        {
            return false;
        }
        let toward = centre - from;
        let distance = toward.length();
        // Every blade of the tile stands within this of its centre.
        let spread = cfg::TILE_M * std::f32::consts::FRAC_1_SQRT_2 + WIDEST_LEAN;
        if distance < cfg::HORIZON_NEAR.max(2.0 * spread) {
            return false;
        }
        let along = toward / distance;
        let (near, far) = (distance - spread, distance + spread);
        // The sight lines run from the eye to the tips, at most `top` high:
        // the highest of them at `s` metres out is the one to the nearest
        // tips when they stand above the eye, the farthest when below.
        let top = heights.1 + TALLEST_BLADE;
        let rise = top - eye.y;
        let run = if rise >= 0.0 { near } else { far };
        // Seen from the eye the tile spans this angle each way. At `s`
        // metres out its sight lines cross the ground along an arc that
        // far from the eye: across the way by `s` times the angle each side,
        // and bowed back toward the eye by `s (1 - cos)`. The ground over
        // that arc's bounds is what must stand above them - every cell the
        // bounds touch, whose drawn ground never dips under its corners - in
        // a box thin along the way, so a ridge needs to be only a sample's
        // spacing thick to be found.
        let angle = (spread / distance).min(1.0).asin();
        let across = Vec2::new(-along.y, along.x).abs() * angle;
        let back = along.abs() * (1.0 - angle.cos());
        let samples = cfg::HORIZON_SAMPLES;
        (1..=samples).any(|k| {
            let s = near * k as f32 / (samples + 1) as f32;
            let sight = eye.y + rise * s / run;
            let at = from + along * s;
            let reach = (across + back) * s;
            self.lowest(hm, at - reach, at + reach)
                .is_some_and(|ground| ground > sight)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1 m heightmap 257 vertices across (-128..128 m), flat at 0, with
    /// a wall `height` high along X from `x0` to `x1` wherever `gap` does
    /// not open it.
    fn ground(height: f32, x0: f32, x1: f32, gap: Option<(f32, f32)>) -> HeightMap {
        let mut hm = HeightMap::new(257, 257, 1.0);
        for z in 0..257 {
            for x in 0..257 {
                let (wx, wz) = (x as f32 - 128.0, z as f32 - 128.0);
                let open = gap.is_some_and(|(z0, z1)| wz >= z0 && wz <= z1);
                if wx >= x0 && wx <= x1 && !open {
                    hm.set(x, z, height);
                }
            }
        }
        hm
    }

    /// A tile behind a ridge is hidden from an eye below its top - one
    /// thicker than the samples' spacing, square to the way or across it at
    /// a slant; over the ridge, or with none, it is not; a gap in the ridge
    /// wide enough to see any of the tile through keeps it drawn.
    #[test]
    fn a_ridge_hides_the_grass_behind_it_and_a_gap_shows_it() {
        let eye = Vec3::new(-60.0, 1.7, 4.0);
        // Tile (8, 0) runs 64..72 m on X, 0..8 on Z; the ridge stands at
        // 16..28 m on X, the samples along the way 7 m apart.
        let tile = IVec2::new(8, 0);
        let flat = (0.0, 0.0);
        let ridge = ground(10.0, 16.0, 28.0, None);
        let floor = GroundFloor::of(&ridge);
        assert!(floor.hides(&ridge, eye, tile, flat));
        // Seen at a slant across it, from down the Z axis.
        assert!(floor.hides(&ridge, Vec3::new(-40.0, 1.7, -60.0), tile, flat));
        // Over the ridge the tile is seen.
        assert!(!floor.hides(&ridge, Vec3::new(-60.0, 30.0, 4.0), tile, flat));
        // A low ridge the tips stand clear of hides nothing.
        let low = ground(0.3, 16.0, 28.0, None);
        assert!(!GroundFloor::of(&low).hides(&low, eye, tile, flat));
        // Flat ground hides nothing.
        let none = ground(0.0, 0.0, 0.0, None);
        assert!(!GroundFloor::of(&none).hides(&none, eye, tile, flat));
        // A gap in the ridge in line with the tile shows it.
        let gapped = ground(10.0, 16.0, 28.0, Some((2.0, 6.0)));
        assert!(!GroundFloor::of(&gapped).hides(&gapped, eye, tile, flat));
        // A near tile is never hidden, even behind a wall: tile (-7, 0)'s
        // centre is 8 m from the eye, the wall between them.
        let near = ground(10.0, -57.0, -56.0, None);
        assert!(!GroundFloor::of(&near).hides(&near, eye, IVec2::new(-7, 0), flat));
    }

    /// A generator of numbers in [0, 1), the same every run.
    fn numbers(seed: u32) -> impl FnMut() -> f32 {
        let mut state = seed.max(1);
        move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state >> 8) as f32 / (1u32 << 24) as f32
        }
    }

    /// The pyramid's lowest is never above the ground drawn anywhere in the
    /// squares it answers for - the mesher's triangles between the vertices,
    /// sampled finely, cells only partly in the square included - and off
    /// the ground it answers nothing.
    #[test]
    fn the_floor_is_never_above_the_ground() {
        let mut hm = HeightMap::new(65, 65, 2.0);
        let mut next = numbers(0x1234_5678);
        for z in 0..65 {
            for x in 0..65 {
                hm.set(x, z, next() * 40.0);
            }
        }
        let floor = GroundFloor::of(&hm);
        for _ in 0..200 {
            let lo = Vec2::new(next() * 120.0 - 64.0, next() * 120.0 - 64.0);
            let size = Vec2::new(next() * 40.0 + 0.1, next() * 6.0 + 0.1);
            let hi = (lo + size).min(Vec2::splat(64.0));
            let least = floor.lowest(&hm, lo, hi).expect("on the ground");
            for _ in 0..200 {
                let at = lo + (hi - lo) * Vec2::new(next(), next());
                let drawn = crate::ground_cover::mesh_height(&hm, at);
                assert!(
                    least <= drawn + 1e-4,
                    "{lo}..{hi} at {at}: {least} > {drawn}"
                );
            }
        }
        assert_eq!(
            floor.lowest(&hm, Vec2::new(60.0, 0.0), Vec2::new(70.0, 4.0)),
            None
        );
    }

    /// No tile is hidden that a sight line reaches: over rolling hills, from
    /// eyes standing where a camera would, every tile the test hides has
    /// every line from the eye to every tip it can hold - over its square
    /// and as far as a blade leans past it, at the tallest blade over its
    /// highest ground - meet the drawn ground on the way. (A lower point of
    /// the tile lies under a line already blocked.) And hills hide some.
    #[test]
    fn a_hidden_tile_has_no_open_sight_line() {
        let mut hm = HeightMap::new(257, 257, 1.0);
        for z in 0..257 {
            for x in 0..257 {
                let (wx, wz) = (x as f32 - 128.0, z as f32 - 128.0);
                let rolling = 9.0 * (wx / 23.0).sin() * (wz / 31.0).cos()
                    + 5.0 * ((wx + wz) / 17.0).sin()
                    + 3.0 * (wx / 7.0).cos() * (wz / 11.0).sin();
                hm.set(x, z, rolling + 20.0);
            }
        }
        let floor = GroundFloor::of(&hm);
        let mut next = numbers(0x00C0_FFEE);
        let mut hidden = 0;
        for _ in 0..12 {
            let at = Vec2::new(next() * 160.0 - 80.0, next() * 160.0 - 80.0);
            let eye = at.extend(0.0).xzy()
                + Vec3::Y * (crate::ground_cover::mesh_height(&hm, at) + 1.5 + next() * 6.0);
            for tz in -15..15 {
                for tx in -15..15 {
                    let coord = IVec2::new(tx, tz);
                    let heights = super::super::tiles::tile_heights(&hm, coord);
                    if !floor.hides(&hm, eye, coord, heights) {
                        continue;
                    }
                    hidden += 1;
                    let top = heights.1 + TALLEST_BLADE;
                    let lo = coord.as_vec2() * cfg::TILE_M - WIDEST_LEAN;
                    let span = cfg::TILE_M + 2.0 * WIDEST_LEAN;
                    for j in 0..=4 {
                        for i in 0..=4 {
                            let tip = lo + Vec2::new(i as f32, j as f32) * span / 4.0;
                            let to = tip.extend(top).xzy();
                            let steps = ((to - eye).length() / 0.1).ceil() as usize;
                            let met = (1..steps).any(|k| {
                                let p = eye.lerp(to, k as f32 / steps as f32);
                                crate::ground_cover::mesh_height(&hm, Vec2::new(p.x, p.z)) > p.y
                            });
                            assert!(met, "eye {eye} sees tile {coord} at {tip} over the ground");
                        }
                    }
                }
            }
        }
        assert!(
            hidden > 50,
            "only {hidden} tiles hidden: the test proves little"
        );
    }
}
