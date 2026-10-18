//! The terrain's height as a GPU texture (#1674), and the height the drawn
//! ground has between the texels.
//!
//! One `Rg32Uint` texel per heightmap vertex, row-major with rows along +Z:
//! the vertex's world height, its `f32` bits in `r`, and the normal the
//! terrain's mesh gives the vertex (#1695) - the mesher's area-weighted
//! one, [`vertex_normals`] - its X and Z as two 16-bit snorms in `g`, X in
//! the low half (WGSL's `unpack2x16snorm`), Y the rest of a unit vector, as
//! a heightfield's normal always points up. The height at full precision
//! rather than half: a Berlin square stands on ground 30-60 m up, where a
//! half float steps 3-6 cm. An integer texture cannot be filtered, and need
//! not be: the drawn ground is not bilinear between vertices but two flat
//! triangles a cell, so a shader that wants what is drawn loads the cell's
//! corners (`textureLoad`) and interpolates as [`mesh_height`] does - the
//! normals too, as the rasterizer interpolates them across the terrain's
//! triangles, which gives the ground's own smooth light.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageSampler, ImageSamplerDescriptor};
use bevy::math::Vec2;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_symbios_ground::HeightMap;

/// The heightmap as the texture the GPU reads: its vertices' heights and
/// normals, one `Rg32Uint` texel each, held only on the GPU.
pub(super) fn height_image(hm: &HeightMap) -> Image {
    let (w, h) = (hm.width(), hm.height());
    let normals = vertex_normals(hm);
    let mut bytes = Vec::with_capacity(w * h * 8);
    for z in 0..h {
        for x in 0..w {
            let normal = normals[z * w + x];
            bytes.extend_from_slice(&hm.get(x, z).to_bits().to_le_bytes());
            bytes.extend_from_slice(&pack_snorm(normal.x, normal.z).to_le_bytes());
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
        TextureFormat::Rg32Uint,
        RenderAssetUsages::RENDER_WORLD,
    );
    // Read by `textureLoad`, which takes no sampler; the nearest one keeps
    // any sampled read honest about what an integer texel is.
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::nearest());
    image
}

/// Two numbers in `[-1, 1]` as WGSL's `unpack2x16snorm` reads them: each to
/// the nearest of 32767 steps a side, `low` in the low 16 bits.
fn pack_snorm(low: f32, high: f32) -> u32 {
    let half = |v: f32| u32::from((v.clamp(-1.0, 1.0) * 32767.0).round() as i16 as u16);
    half(low) | (half(high) << 16)
}

/// The normal at each of heightmap `hm`'s vertices, row by row, as the
/// terrain's mesh carries it: `bevy_symbios_ground`'s mesher with its
/// area-weighted normals (`terrain::heightmap`), the unnormalised normals
/// of the triangles meeting at a vertex summed and made unit - its two
/// triangles a cell, split from the (x + 1, z) corner to the (x, z + 1)
/// one, wound counter-clockwise from above.
fn vertex_normals(hm: &HeightMap) -> Vec<Vec3> {
    let (w, h, s) = (hm.width(), hm.height(), hm.scale());
    let at = |(x, z): (usize, usize)| Vec3::new(x as f32 * s, hm.get(x, z), z as f32 * s);
    let mut sums = vec![Vec3::ZERO; w * h];
    for z in 0..h.saturating_sub(1) {
        for x in 0..w.saturating_sub(1) {
            let (tl, tr, bl, br) = ((x, z), (x + 1, z), (x, z + 1), (x + 1, z + 1));
            for [a, b, c] in [[tl, bl, tr], [tr, bl, br]] {
                let face = (at(b) - at(a)).cross(at(c) - at(a));
                for (vx, vz) in [a, b, c] {
                    sums[vz * w + vx] += face;
                }
            }
        }
    }
    sums.into_iter()
        .map(|sum| {
            let length = sum.length();
            if length > f32::EPSILON {
                sum / length
            } else {
                Vec3::Y
            }
        })
        .collect()
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

    /// The texture holds every vertex's height and normal, row by row
    /// along +Z: the height's bits, and the normal's X and Z as snorms to
    /// within a step, its Y what is left of a unit vector.
    #[test]
    fn the_texture_holds_each_vertex() {
        let hm = map([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]);
        let image = height_image(&hm);
        assert_eq!(image.texture_descriptor.format, TextureFormat::Rg32Uint);
        let data = image.data.as_ref().expect("the bytes before upload");
        let word = |i: usize| u32::from_le_bytes(data[i * 4..i * 4 + 4].try_into().unwrap());
        let height = |i: usize| f32::from_bits(word(2 * i));
        assert_eq!(height(0), 1.0);
        assert_eq!(height(2), 3.0);
        assert_eq!(height(3), 4.0);
        assert_eq!(height(8), 9.0);
        let snorm = |bits: u32| f32::from(bits as u16 as i16).max(-32767.0) / 32767.0;
        let normals = vertex_normals(&hm);
        for (i, normal) in normals.iter().enumerate() {
            let packed = word(2 * i + 1);
            let (x, z) = (snorm(packed & 0xFFFF), snorm(packed >> 16));
            let y = (1.0 - x * x - z * z).max(0.0).sqrt();
            assert!(
                (Vec3::new(x, y, z) - *normal).length() < 1e-4,
                "{i}: {normal}"
            );
        }
    }

    /// The normals are the terrain mesh's own: the mesher's, with the
    /// area-weighted normals the terrain asks it for - on rough ground,
    /// vertex for vertex.
    #[test]
    fn the_normals_are_the_terrain_meshs() {
        let mut hm = HeightMap::new(9, 7, 2.0);
        let mut state = 0x9E37_79B9_u32;
        for z in 0..7 {
            for x in 0..9 {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                hm.set(x, z, (state >> 8) as f32 / (1u32 << 24) as f32 * 12.0);
            }
        }
        let mesh = bevy_symbios_ground::HeightMapMeshBuilder::new()
            .with_normal_method(bevy_symbios_ground::NormalMethod::AreaWeighted)
            .build(&hm);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(meshed)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("the mesh's normals");
        };
        let ours = vertex_normals(&hm);
        assert_eq!(ours.len(), meshed.len());
        for (i, (ours, meshed)) in ours.iter().zip(meshed).enumerate() {
            assert!(
                (*ours - Vec3::from_array(*meshed)).length() < 1e-6,
                "{i}: {ours} against {meshed:?}"
            );
        }
    }
}
