//! The cover mask's raster (#1674): a square grid of texels over the
//! ground, each open or blocked, with the shapes that block it drawn in on
//! the CPU.
//!
//! A texel is blocked when its centre lies inside a shape. That keeps every
//! shape's outline where it is, to within half a texel, and lets a shape
//! thinner than a texel - a lamp post, a railing - through: grass growing
//! against a post's foot is what grass does. The GPU samples the mask
//! linearly between texel centres, so an edge fades across one texel rather
//! than stepping.

use bevy::math::Vec2;

/// An open texel: something may grow here.
pub const OPEN: u8 = u8::MAX;
/// A blocked texel: road, building, a solid placed item or water.
pub const BLOCKED: u8 = 0;

/// A square grid of cover texels over the ground. Row-major, rows along +Z
/// and columns along +X, as the heightmap's are.
#[derive(Clone, Debug, PartialEq)]
pub struct CoverMask {
    /// Texels per side.
    size: usize,
    /// Metres per texel.
    texel: f32,
    /// World XZ of the grid's outer corner, texel (0, 0)'s low corner.
    origin: Vec2,
    data: Vec<u8>,
}

impl CoverMask {
    /// A mask `size` texels square, each `texel` metres, its low corner at
    /// world `origin`, open everywhere.
    pub fn open(size: usize, texel: f32, origin: Vec2) -> Self {
        Self {
            size,
            texel,
            origin,
            data: vec![OPEN; size * size],
        }
    }

    /// Texels per side.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Metres per texel.
    pub fn texel(&self) -> f32 {
        self.texel
    }

    /// World XZ of the grid's low corner.
    pub fn origin(&self) -> Vec2 {
        self.origin
    }

    /// The texels, row-major, rows along +Z: [`OPEN`] or [`BLOCKED`].
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }

    /// The world XZ of texel `(i, j)`'s centre: column `i`, row `j`.
    pub fn centre(&self, i: usize, j: usize) -> Vec2 {
        self.origin + (Vec2::new(i as f32, j as f32) + 0.5) * self.texel
    }

    /// The most open the GPU samples the mask anywhere in the rectangle
    /// `lo..hi` (#1693), 0 to 1: the most of the texels whose centres lie
    /// within two texels of it - the one its linear sampling blends there,
    /// and one more, as a GPU places a sample between texels to a few bits
    /// only. 0 for a rectangle off the grid, where nothing grows.
    pub fn most_open_in(&self, lo: Vec2, hi: Vec2) -> f32 {
        let pad = 2.0 * self.texel;
        let (Some((i0, i1)), Some((j0, j1))) = (
            self.span(lo.x - pad, hi.x + pad, self.origin.x),
            self.span(lo.y - pad, hi.y + pad, self.origin.y),
        ) else {
            return 0.0;
        };
        let mut most = BLOCKED;
        for j in j0..=j1 {
            let row = &self.data[j * self.size..(j + 1) * self.size];
            most = most.max(row[i0..=i1].iter().copied().max().unwrap_or(BLOCKED));
            if most == OPEN {
                break;
            }
        }
        f32::from(most) / f32::from(OPEN)
    }

    /// The columns or rows whose centres can lie in `[lo, hi]` along one
    /// axis whose grid starts at `start`, clamped to the grid; `None` when
    /// the span misses it.
    fn span(&self, lo: f32, hi: f32, start: f32) -> Option<(usize, usize)> {
        if !(lo.is_finite() && hi.is_finite()) || hi < lo {
            return None;
        }
        // Centre k sits at start + (k + 0.5) texel.
        let first = ((lo - start) / self.texel - 0.5).ceil().max(0.0);
        let last = ((hi - start) / self.texel - 0.5).floor();
        if last < 0.0 || first > (self.size - 1) as f32 {
            return None;
        }
        Some((first as usize, (last as usize).min(self.size - 1)))
    }

    /// Block every texel whose centre lies inside the convex polygon `hull`,
    /// wound either way. A polygon of under three points, or of no area,
    /// blocks nothing. Row by row: each row of centres crosses a convex
    /// polygon in one span, found from its edges, so a long diagonal road
    /// triangle costs its rows, not its bounding box.
    pub fn block_convex(&mut self, hull: &[Vec2]) {
        if hull.len() < 3 {
            return;
        }
        let area = signed_area(hull);
        if area.abs() <= f32::EPSILON || !area.is_finite() {
            return;
        }
        let (low, high) = hull.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
            (lo.min(p.y), hi.max(p.y))
        });
        let Some((j0, j1)) = self.span(low, high, self.origin.y) else {
            return;
        };
        for j in j0..=j1 {
            let y = self.origin.y + (j as f32 + 0.5) * self.texel;
            let mut left = f32::MAX;
            let mut right = f32::MIN;
            for (a, b) in hull.iter().zip(hull.iter().cycle().skip(1)) {
                let (lo, hi) = (a.y.min(b.y), a.y.max(b.y));
                if y < lo || y > hi {
                    continue;
                }
                let x = if (b.y - a.y).abs() <= f32::EPSILON {
                    // An edge along the row: both its ends are in.
                    left = left.min(a.x.min(b.x));
                    a.x.max(b.x)
                } else {
                    a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y)
                };
                left = left.min(x);
                right = right.max(x);
            }
            if let Some((i0, i1)) = self.span(left, right, self.origin.x) {
                self.data[j * self.size + i0..=j * self.size + i1].fill(BLOCKED);
            }
        }
    }

    /// Block every texel whose centre `blocked` says yes to: the water's
    /// test, which asks the ground's height there.
    pub fn block_where(&mut self, blocked: impl Fn(Vec2) -> bool) {
        for j in 0..self.size {
            for i in 0..self.size {
                if blocked(self.centre(i, j)) {
                    self.data[j * self.size + i] = BLOCKED;
                }
            }
        }
    }

    /// How open the ground is at world `xz`, 1 open to 0 blocked, linear
    /// between texel centres as the GPU samples the mask; past the grid's
    /// edge, its edge.
    pub fn open_at(&self, xz: Vec2) -> f32 {
        let g = (xz - self.origin) / self.texel - 0.5;
        let last = (self.size - 1) as f32;
        let g = g.clamp(Vec2::ZERO, Vec2::splat(last));
        let (i0, j0) = (g.x.floor() as usize, g.y.floor() as usize);
        let (i1, j1) = ((i0 + 1).min(self.size - 1), (j0 + 1).min(self.size - 1));
        let f = g - Vec2::new(i0 as f32, j0 as f32);
        let at = |i: usize, j: usize| f32::from(self.data[j * self.size + i]) / f32::from(OPEN);
        let low = at(i0, j0) + (at(i1, j0) - at(i0, j0)) * f.x;
        let high = at(i0, j1) + (at(i1, j1) - at(i0, j1)) * f.x;
        low + (high - low) * f.y
    }

    /// The share of the texels that are open.
    pub fn open_share(&self) -> f32 {
        let open = self.data.iter().filter(|&&t| t == OPEN).count();
        open as f32 / self.data.len().max(1) as f32
    }
}

/// Twice the signed area of polygon `p`: positive wound anticlockwise in
/// (x, y), as XZ reads from above with +Z down the page.
fn signed_area(p: &[Vec2]) -> f32 {
    p.iter()
        .zip(p.iter().cycle().skip(1))
        .map(|(a, b)| a.perp_dot(*b))
        .sum()
}

/// The convex hull of `points` (Andrew's monotone chain), wound
/// anticlockwise; fewer than three distinct points give what is there.
pub fn convex_hull(mut points: Vec<Vec2>) -> Vec<Vec2> {
    points.retain(|p| p.is_finite());
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    let mut hull: Vec<Vec2> = Vec::with_capacity(points.len() * 2);
    for pass in 0..2 {
        let start = hull.len();
        let ordered: Box<dyn Iterator<Item = &Vec2>> = if pass == 0 {
            Box::new(points.iter())
        } else {
            Box::new(points.iter().rev())
        };
        for p in ordered {
            while hull.len() >= start + 2 {
                let (a, b) = (hull[hull.len() - 2], hull[hull.len() - 1]);
                if (b - a).perp_dot(*p - a) <= 0.0 {
                    hull.pop();
                } else {
                    break;
                }
            }
            hull.push(*p);
        }
        // Each chain's last point starts the other chain.
        hull.pop();
    }
    hull
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The most open a rectangle reads is the most of the texels a linear
    /// sample in it can blend, and one more (#1693): blocked ground reads 0
    /// only when it stands two texels clear of open ground, and off the grid
    /// nothing grows.
    #[test]
    fn a_rectangle_reads_the_most_open_texel_it_can_sample() {
        let mut m = mask();
        m.block_convex(&[
            Vec2::new(0.0, 0.0),
            Vec2::new(6.0, 0.0),
            Vec2::new(6.0, 10.0),
            Vec2::new(0.0, 10.0),
        ]);
        // Columns 0..=5 are blocked, 6..=9 open.
        assert_eq!(
            m.most_open_in(Vec2::new(1.0, 1.0), Vec2::new(4.0, 4.0)),
            0.0
        );
        // Two texels short of the first open centre (6.5) a rectangle reads
        // blocked only; nearer, it takes the open one in.
        assert_eq!(
            m.most_open_in(Vec2::new(1.0, 1.0), Vec2::new(4.4, 4.0)),
            0.0
        );
        assert_eq!(
            m.most_open_in(Vec2::new(1.0, 1.0), Vec2::new(4.6, 4.0)),
            1.0
        );
        assert_eq!(
            m.most_open_in(Vec2::new(7.0, 1.0), Vec2::new(8.0, 2.0)),
            1.0
        );
        assert_eq!(
            m.most_open_in(Vec2::new(20.0, 1.0), Vec2::new(30.0, 2.0)),
            0.0
        );
    }

    /// A 10 x 10 mask of 1 m texels from (0, 0).
    fn mask() -> CoverMask {
        CoverMask::open(10, 1.0, Vec2::ZERO)
    }

    fn blocked(m: &CoverMask) -> usize {
        m.bytes().iter().filter(|&&t| t == BLOCKED).count()
    }

    /// A square blocks the texels whose centres it holds, wound either way,
    /// and nothing outside it.
    #[test]
    fn a_square_blocks_the_texels_it_covers() {
        for square in [
            vec![
                Vec2::new(2.0, 2.0),
                Vec2::new(5.0, 2.0),
                Vec2::new(5.0, 4.0),
                Vec2::new(2.0, 4.0),
            ],
            vec![
                Vec2::new(2.0, 2.0),
                Vec2::new(2.0, 4.0),
                Vec2::new(5.0, 4.0),
                Vec2::new(5.0, 2.0),
            ],
        ] {
            let mut m = mask();
            m.block_convex(&square);
            assert_eq!(blocked(&m), 3 * 2, "{square:?}");
            assert_eq!(m.open_at(Vec2::new(3.5, 3.0)), 0.0);
            assert_eq!(m.open_at(Vec2::new(7.5, 7.5)), 1.0);
        }
    }

    /// A shape thinner than a texel, between centres, lets the grass
    /// through; one off the grid blocks nothing and does not panic.
    #[test]
    fn a_post_or_a_shape_off_the_grid_blocks_nothing() {
        let mut m = mask();
        m.block_convex(&[
            Vec2::new(3.1, 3.1),
            Vec2::new(3.4, 3.1),
            Vec2::new(3.4, 3.4),
        ]);
        m.block_convex(&[
            Vec2::new(-30.0, -30.0),
            Vec2::new(-20.0, -30.0),
            Vec2::new(-20.0, -20.0),
        ]);
        m.block_convex(&[Vec2::ZERO, Vec2::ONE]);
        m.block_convex(&[Vec2::ZERO, Vec2::ONE, Vec2::splat(2.0)]);
        assert_eq!(blocked(&m), 0);
    }

    /// A shape running off the grid blocks the part on it.
    #[test]
    fn a_shape_over_the_edge_blocks_what_lies_on_the_grid() {
        let mut m = mask();
        m.block_convex(&[
            Vec2::new(-5.0, -5.0),
            Vec2::new(2.0, -5.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(-5.0, 2.0),
        ]);
        assert_eq!(blocked(&m), 4);
    }

    /// The water's test blocks what it answers yes for.
    #[test]
    fn block_where_blocks_by_position() {
        let mut m = mask();
        m.block_where(|p| p.x < 3.0);
        assert_eq!(blocked(&m), 3 * 10);
        assert!((m.open_share() - 0.7).abs() < 1e-6);
    }

    /// Between an open and a blocked centre the mask reads half open, as
    /// a linear sampler does.
    #[test]
    fn the_mask_reads_linearly_between_centres() {
        let mut m = mask();
        m.block_where(|p| p.x < 5.0);
        let edge = m.open_at(Vec2::new(5.0, 5.0));
        assert!((edge - 0.5).abs() < 1e-6, "{edge}");
        assert_eq!(m.open_at(Vec2::new(-100.0, 5.0)), 0.0);
        assert_eq!(m.open_at(Vec2::new(100.0, 5.0)), 1.0);
    }

    /// The hull keeps the outline and drops what lies inside it.
    #[test]
    fn the_hull_keeps_the_outline() {
        let hull = convex_hull(vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(4.0, 0.0),
            Vec2::new(2.0, 1.0),
            Vec2::new(4.0, 4.0),
            Vec2::new(0.0, 4.0),
            Vec2::new(0.0, 4.0),
            Vec2::new(f32::NAN, 1.0),
        ]);
        assert_eq!(hull.len(), 4, "{hull:?}");
        assert!(signed_area(&hull) > 0.0);
        assert!((signed_area(&hull) - 32.0).abs() < 1e-4);
    }
}
