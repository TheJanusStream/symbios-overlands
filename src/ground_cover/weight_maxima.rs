//! The splat weights' most, grass tile by grass tile (#1693).
//!
//! The weight map lives on the GPU only (its CPU bytes are dropped after
//! upload), but the grass decides on the CPU how many of a tile's blades to
//! draw, and needs to know how thickly the tile's ground can grow grass.
//! What it needs is small: for each of its tiles, the most each splat
//! layer's share reaches anywhere in the tile's square. The terrain folds
//! the weight map into that once, as it builds the map.

use bevy::prelude::*;

use crate::config::grass::TILE_M;

/// The most each splat layer's share reaches in each grass tile's square
/// (#1693) - a share being a layer's weight over the four's sum, as the
/// grass's shader reads it - for every point of the square at least what
/// the GPU's linear sampling of the weight map gives there. The grass's
/// tiles bound what can grow in them by it ([`crate::grass::tiles::most_density`]).
#[derive(Clone, Debug, PartialEq)]
pub struct WeightMaxima {
    /// The tile coordinate of the grid's first tile, and tiles per side.
    first: IVec2,
    side: usize,
    /// Per tile, row by row along +Z, each layer's most share out of 255,
    /// rounded up.
    most: Vec<[u8; 4]>,
}

impl WeightMaxima {
    /// From a weight map of `width` x `height` RGBA8 texels, row by row
    /// along +Z, over a square ground `extent` metres across and centred on
    /// the origin, sampled as the ground's shader samples it: linearly at
    /// `(xz + half) / extent`, which blends texels `floor(t)` and the next
    /// along each axis, `t` being that times the texels less a half - and a
    /// texel more each way, as a GPU takes `floor(t)` to a few bits past
    /// the point only, and a sample on a tile's edge can reach the next. A
    /// map that does not hold its texels bounds nothing: every share full.
    pub fn of(texels: &[[u8; 4]], width: usize, height: usize, extent: f32) -> Self {
        let half = extent * 0.5;
        let first = (Vec2::splat(-half) / TILE_M).floor().as_ivec2();
        let last = (Vec2::splat(half) / TILE_M).floor().as_ivec2();
        let side = usize::try_from(last.x - first.x + 1).unwrap_or(0);
        if width == 0 || height == 0 || texels.len() < width * height || extent <= 0.0 {
            return Self {
                first,
                side,
                most: vec![[u8::MAX; 4]; side * side],
            };
        }
        let shares: Vec<[u8; 4]> = texels.iter().map(|&texel| share_bytes(texel)).collect();
        // The texels the sampling blends between world `lo` and `hi` along
        // an axis of `n` texels, a texel more each way.
        let span = |lo: f32, hi: f32, n: usize| {
            let t = |x: f32| (x + half) / extent * n as f32 - 0.5;
            let top = (n - 1) as f32;
            let a = (t(lo).floor() - 1.0).clamp(0.0, top) as usize;
            let b = (t(hi).floor() + 2.0).clamp(0.0, top) as usize;
            (a, b)
        };
        let mut most = vec![[0u8; 4]; side * side];
        for row in 0..side {
            for column in 0..side {
                let coord = first + IVec2::new(column as i32, row as i32);
                let lo = (coord.as_vec2() * TILE_M).max(Vec2::splat(-half));
                let hi = (coord.as_vec2() * TILE_M + TILE_M).min(Vec2::splat(half));
                if lo.x > hi.x || lo.y > hi.y {
                    continue;
                }
                let (i0, i1) = span(lo.x, hi.x, width);
                let (j0, j1) = span(lo.y, hi.y, height);
                let tile = &mut most[row * side + column];
                for j in j0..=j1 {
                    for share in &shares[j * width + i0..=j * width + i1] {
                        for (most, &share) in tile.iter_mut().zip(share) {
                            *most = (*most).max(share);
                        }
                    }
                }
            }
        }
        Self { first, side, most }
    }

    /// The most each layer's share reaches anywhere in grass tile `coord`'s
    /// square, 0 to 1; nothing for a tile off the ground.
    pub fn in_tile(&self, coord: IVec2) -> [f32; 4] {
        let at = coord - self.first;
        match (usize::try_from(at.x), usize::try_from(at.y)) {
            (Ok(column), Ok(row)) if column < self.side && row < self.side => {
                self.most[row * self.side + column].map(|share| f32::from(share) / 255.0)
            }
            _ => [0.0; 4],
        }
    }
}

/// A texel's share of each layer as the grass's shader reads it - its
/// weight over the four's sum, the sum held at 1e-3 at least - out of 255
/// and rounded up, so the most taken over texels is never under the truth.
fn share_bytes(texel: [u8; 4]) -> [u8; 4] {
    let sum = texel
        .iter()
        .map(|&weight| f32::from(weight))
        .sum::<f32>()
        .max(1.0e-3 * 255.0);
    texel.map(|weight| (f32::from(weight) / sum * 255.0).ceil().min(255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whatever the shader samples anywhere in a tile, the tile's most is
    /// at least it: a weight map of random texels, sampled linearly at many
    /// points of many tiles exactly as the shader does, never shows a share
    /// over its tile's most. A tile off the ground grows nothing.
    #[test]
    fn a_tiles_most_bounds_every_share_sampled_in_it() {
        // 33 x 33 texels over a 64 m ground (-32..32 m), as a 2 m heightmap.
        let (n, extent) = (33usize, 64.0f32);
        let mut state = 0x2545_F491_u32;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state >> 24) as u8
        };
        let texels: Vec<[u8; 4]> = (0..n * n)
            .map(|_| {
                [
                    next(),
                    next() / 3,
                    next() / 7,
                    if next() > 200 { 255 } else { 0 },
                ]
            })
            .collect();
        let maxima = WeightMaxima::of(&texels, n, n, extent);
        // The shader's sampling: linear between texels, clamped at the edges.
        let sample = |x: f32, z: f32| {
            let half = extent * 0.5;
            let t = |v: f32| ((v + half) / extent * n as f32 - 0.5).clamp(0.0, (n - 1) as f32);
            let (tx, tz) = (t(x), t(z));
            let (i, j) = (tx.floor() as usize, tz.floor() as usize);
            let (fx, fz) = (tx - i as f32, tz - j as f32);
            let at = |i: usize, j: usize| texels[j.min(n - 1) * n + i.min(n - 1)].map(f32::from);
            let mut w = [0.0f32; 4];
            for (k, w) in w.iter_mut().enumerate() {
                let top = at(i, j)[k] * (1.0 - fx) + at(i + 1, j)[k] * fx;
                let bottom = at(i, j + 1)[k] * (1.0 - fx) + at(i + 1, j + 1)[k] * fx;
                *w = top * (1.0 - fz) + bottom * fz;
            }
            let sum = w.iter().sum::<f32>().max(1.0e-3 * 255.0);
            w.map(|w| w / sum)
        };
        for tz in -4..4 {
            for tx in -4..4 {
                let coord = IVec2::new(tx, tz);
                let most = maxima.in_tile(coord);
                for sz in 0..=16 {
                    for sx in 0..=16 {
                        let x = tx as f32 * TILE_M + sx as f32 * TILE_M / 16.0;
                        let z = tz as f32 * TILE_M + sz as f32 * TILE_M / 16.0;
                        for (k, share) in sample(x, z).into_iter().enumerate() {
                            assert!(
                                share <= most[k] + 1e-6,
                                "tile {coord} at ({x}, {z}) layer {k}: {share} > {}",
                                most[k]
                            );
                        }
                    }
                }
            }
        }
        // Tiles wholly off the ground (-32..32 m) hold nothing. (Tile 4
        // starts on the ground's edge line, which the shader still reads as
        // ground; the field lays no tile there.)
        assert_eq!(maxima.in_tile(IVec2::new(-5, 0)), [0.0; 4]);
        assert_eq!(maxima.in_tile(IVec2::new(5, 0)), [0.0; 4]);
        // A ground of one layer is that layer's, wholly, in every tile.
        let grass = WeightMaxima::of(&vec![[255, 0, 0, 0]; n * n], n, n, extent);
        assert_eq!(grass.in_tile(IVec2::ZERO), [1.0, 0.0, 0.0, 0.0]);
        // No weight at all reads as no share, as the shader's floor makes it.
        let none = WeightMaxima::of(&vec![[0; 4]; n * n], n, n, extent);
        assert_eq!(none.in_tile(IVec2::ZERO), [0.0; 4]);
    }
}
