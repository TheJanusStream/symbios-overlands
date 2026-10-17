//! The terrain's height as a GPU texture (#1674), and the height the drawn
//! ground has between the texels.
//!
//! One `R32Float` texel per heightmap vertex, row-major with rows along +Z,
//! holding the vertex's world height. Full precision rather than half: a
//! Berlin square stands on ground 30-60 m up, where a half float steps 3-6
//! cm. A 32-bit float cannot be filtered on WebGL2, and need not be: the
//! drawn ground is not bilinear between vertices but two flat triangles a
//! cell, so a shader that wants what is drawn loads the cell's corners
//! (`textureLoad`) and interpolates as [`mesh_height`] does.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageSampler, ImageSamplerDescriptor};
use bevy::math::Vec2;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_symbios_ground::HeightMap;

/// The heightmap as the texture the GPU reads: its vertices' heights, one
/// `R32Float` texel each, held only on the GPU.
pub(super) fn height_image(hm: &HeightMap) -> Image {
    let (w, h) = (hm.width(), hm.height());
    let mut bytes = Vec::with_capacity(w * h * 4);
    for z in 0..h {
        for x in 0..w {
            bytes.extend_from_slice(&hm.get(x, z).to_le_bytes());
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: w as u32,
            height: h as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        bytes,
        TextureFormat::R32Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    // Read by `textureLoad`, which takes no sampler; the nearest one keeps
    // any sampled read honest about what a float texel is.
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::nearest());
    image
}

/// The drawn ground's height at world `xz` on heightmap `hm`, centred on
/// the origin as the terrain is: the cell's two triangles as the mesher
/// builds them - split from its (x + 1, z) corner to its (x, z + 1) one -
/// rather than the bilinear blend the CPU's other height queries take.
/// Past the edge, the edge's.
pub(crate) fn mesh_height(hm: &HeightMap, xz: Vec2) -> f32 {
    let (w, h) = (hm.width(), hm.height());
    let half = Vec2::new((w - 1) as f32, (h - 1) as f32) * hm.scale() * 0.5;
    let g = ((xz + half) / hm.scale()).clamp(Vec2::ZERO, Vec2::new((w - 1) as f32, (h - 1) as f32));
    let (x0, z0) = (
        (g.x.floor() as usize).min(w - 2),
        (g.y.floor() as usize).min(h - 2),
    );
    let (fx, fz) = (g.x - x0 as f32, g.y - z0 as f32);
    let (tl, tr) = (hm.get(x0, z0), hm.get(x0 + 1, z0));
    let (bl, br) = (hm.get(x0, z0 + 1), hm.get(x0 + 1, z0 + 1));
    if fx + fz <= 1.0 {
        tl + (tr - tl) * fx + (bl - tl) * fz
    } else {
        br + (bl - br) * (1.0 - fx) + (tr - br) * (1.0 - fz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 3 x 3 heightmap, 2 m cells, centred on the origin (so -2..2 m).
    fn map(heights: [[f32; 3]; 3]) -> HeightMap {
        let mut hm = HeightMap::new(3, 3, 2.0);
        for (z, row) in heights.iter().enumerate() {
            for (x, &y) in row.iter().enumerate() {
                hm.set(x, z, y);
            }
        }
        hm
    }

    /// At a vertex the height is the vertex's; across a cell it follows
    /// the mesher's two triangles, not a bilinear blend.
    #[test]
    fn the_height_follows_the_drawn_triangles() {
        // One cell raised at its far corner only: bilinear would read a
        // quarter of it at the cell's middle, the triangles nothing on the
        // near triangle's side of the split and half on the far side.
        let hm = map([[0.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 0.0]]);
        assert_eq!(mesh_height(&hm, Vec2::new(0.0, 0.0)), 4.0);
        assert_eq!(mesh_height(&hm, Vec2::new(-2.0, -2.0)), 0.0);
        // In the first cell (-2..0), its (x + 1, z + 1) corner is the raised
        // vertex: on the split itself the height is nought, past it rising.
        assert!(mesh_height(&hm, Vec2::new(-1.0, -1.0)).abs() < 1e-6);
        let far = mesh_height(&hm, Vec2::new(-0.5, -0.5));
        assert!((far - 2.0).abs() < 1e-5, "{far}");
        // Past the edge, the edge.
        assert_eq!(mesh_height(&hm, Vec2::new(-50.0, 0.0)), 0.0);
    }

    /// The texture holds every vertex's height, row by row along +Z.
    #[test]
    fn the_texture_holds_each_vertex() {
        let hm = map([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]);
        let image = height_image(&hm);
        assert_eq!(image.texture_descriptor.format, TextureFormat::R32Float);
        let data = image.data.as_ref().expect("the bytes before upload");
        let texel = |i: usize| f32::from_le_bytes(data[i * 4..i * 4 + 4].try_into().unwrap());
        assert_eq!(texel(0), 1.0);
        assert_eq!(texel(2), 3.0);
        assert_eq!(texel(3), 4.0);
        assert_eq!(texel(8), 9.0);
    }
}
