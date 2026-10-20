//! What shelters the ground from what falls (#1685): the highest static
//! shape over each texel of the cover's grid, drawn on the CPU from the
//! same colliders as the cover ([`super::footprint::tops`]) whenever the
//! cover is. The rain and the snow fold away under it
//! (`crate::weather::precipitation`), and the ground and the grass stay dry
//! where it stands well clear of the ground (`super::sheltered_layer`).
//!
//! A texel is sheltered by a shape when its centre lies under the shape's
//! outline, as the cover mask is blocked; its height there is the shape's
//! top - a triangle's plane, so the rain falls on a pitched roof up to its
//! slope, and everywhere else the highest point of the piece.

use bevy::asset::RenderAssetUsages;
use bevy::math::Vec2;
use bevy::prelude::Image;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use super::mask::convex_spans;

/// The tops over the cover's grid: a square of texels, row-major with rows
/// along +Z, as the cover mask's - minus infinity where nothing stands over
/// a texel.
#[derive(Clone, Debug, PartialEq)]
pub struct ShelterMap {
    /// Texels per side.
    size: usize,
    /// Metres per texel.
    texel: f32,
    /// World XZ of the grid's low corner.
    origin: Vec2,
    top: Vec<f32>,
}

impl ShelterMap {
    /// A map `size` texels square, each `texel` metres, its low corner at
    /// world `origin`, open to the sky everywhere.
    pub fn open(size: usize, texel: f32, origin: Vec2) -> Self {
        Self {
            size,
            texel,
            origin,
            top: vec![f32::NEG_INFINITY; size * size],
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

    /// Raise each texel whose centre lies inside the convex polygon
    /// `outline` to `top` at its centre, where that is higher than what the
    /// texel holds.
    pub fn raise(&mut self, outline: &[Vec2], top: &dyn Fn(Vec2) -> f32) {
        let (size, texel, origin) = (self.size, self.texel, self.origin);
        let tops = &mut self.top;
        convex_spans(size, texel, origin, outline, |j, i0, i1| {
            for i in i0..=i1 {
                let at = top(origin + (Vec2::new(i as f32, j as f32) + 0.5) * texel);
                let held = &mut tops[j * size + i];
                if at > *held {
                    *held = at;
                }
            }
        });
    }

    /// The highest top over world `xz`, its texel's: minus infinity under
    /// the open sky and off the grid.
    pub fn top_at(&self, xz: Vec2) -> f32 {
        let g = ((xz - self.origin) / self.texel).floor();
        let last = self.size as f32;
        if !(0.0..last).contains(&g.x) || !(0.0..last).contains(&g.y) {
            return f32::NEG_INFINITY;
        }
        self.top[g.y as usize * self.size + g.x as usize]
    }

    /// The texels something stands over: each one's centre in world XZ and
    /// the top there. Most of a world is open sky, so a reader that wants
    /// only these passes the rest at the cost of a scan.
    pub fn covered(&self) -> impl Iterator<Item = (Vec2, f32)> + '_ {
        self.top
            .iter()
            .enumerate()
            .filter(|(_, top)| top.is_finite())
            .map(|(k, &top)| {
                let (i, j) = (k % self.size, k / self.size);
                (
                    self.origin + (Vec2::new(i as f32, j as f32) + 0.5) * self.texel,
                    top,
                )
            })
    }

    /// The share of the texels something stands over.
    pub fn covered_share(&self) -> f32 {
        let covered = self.top.iter().filter(|t| t.is_finite()).count();
        covered as f32 / self.top.len().max(1) as f32
    }

    /// The map as the GPU reads it: one `R16Float` texel each, read texel by
    /// texel (`textureLoad`), minus infinity under the open sky, GPU only.
    /// Half a float keeps a height to a quarter metre at 500 m.
    pub fn image(&self) -> Image {
        let size = self.size as u32;
        let bytes = self
            .top
            .iter()
            .flat_map(|&t| half::f16::from_f32(t).to_le_bytes())
            .collect();
        Image::new(
            Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            bytes,
            TextureFormat::R16Float,
            RenderAssetUsages::RENDER_WORLD,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> ShelterMap {
        ShelterMap::open(8, 1.0, Vec2::splat(-4.0))
    }

    fn square(lo: f32, hi: f32) -> [Vec2; 4] {
        [
            Vec2::new(lo, lo),
            Vec2::new(hi, lo),
            Vec2::new(hi, hi),
            Vec2::new(lo, hi),
        ]
    }

    /// A roof raises the texels under it to its top, a higher one over part
    /// of it raises those further, and a lower one leaves them as they are;
    /// the open sky stays open.
    #[test]
    fn the_highest_shape_over_a_texel_is_its_top() {
        let mut m = map();
        assert_eq!(m.covered_share(), 0.0);
        m.raise(&square(-2.0, 2.0), &|_| 3.0);
        assert_eq!(m.top_at(Vec2::new(0.5, 0.5)), 3.0);
        assert_eq!(m.top_at(Vec2::new(3.5, 3.5)), f32::NEG_INFINITY, "open sky");
        m.raise(&square(0.0, 2.0), &|_| 5.0);
        m.raise(&square(-2.0, 2.0), &|_| 1.0);
        assert_eq!(m.top_at(Vec2::new(0.5, 0.5)), 5.0, "the higher one");
        assert_eq!(
            m.top_at(Vec2::new(-1.5, -1.5)),
            3.0,
            "the lower one is under it"
        );
        assert_eq!(m.covered_share(), 16.0 / 64.0);
        assert_eq!(
            m.top_at(Vec2::new(40.0, 0.0)),
            f32::NEG_INFINITY,
            "off the grid"
        );
    }

    /// A slope is read at each texel's centre.
    #[test]
    fn a_slope_is_read_at_each_centre() {
        let mut m = map();
        m.raise(&square(-4.0, 4.0), &|xz| 10.0 + xz.x);
        assert_eq!(m.top_at(Vec2::new(-3.9, 0.0)), 10.0 - 3.5);
        assert_eq!(m.top_at(Vec2::new(3.9, 0.0)), 10.0 + 3.5);
    }

    /// The GPU's texels: two bytes each, the sky's minus infinity kept.
    #[test]
    fn the_image_holds_each_top_in_half_a_float() {
        let mut m = map();
        m.raise(&square(-2.0, 2.0), &|_| 3.25);
        let image = m.image();
        assert_eq!(image.texture_descriptor.format, TextureFormat::R16Float);
        let bytes = image.data.expect("the texels");
        assert_eq!(bytes.len(), 64 * 2);
        let at = |i: usize, j: usize| {
            let k = (j * 8 + i) * 2;
            half::f16::from_le_bytes([bytes[k], bytes[k + 1]]).to_f32()
        };
        assert_eq!(at(4, 4), 3.25);
        assert_eq!(at(0, 0), f32::NEG_INFINITY);
    }
}
