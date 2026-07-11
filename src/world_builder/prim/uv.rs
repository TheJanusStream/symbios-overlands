//! Post-mesh UV projection for meshers with no analytic parameterisation
//! (#739). [`project_uvs`] maps a [`UvMapping`] mode over raw attribute
//! buffers rather than a `Mesh`, so any hand-built mesher can adopt it just
//! before its `mesh_from_parts` tail — BlobGroup is the only adopter today.
//! All projections work in the buffers' own (prim-local) frame, so a mode's
//! result is stable under the prim's transform.
//!
//! The discontinuous modes (`Box` between projection axes, `Cylindrical` at
//! the azimuth wrap) split shared vertices along their seams: a shared
//! vertex whose triangles land in different projection regions would
//! otherwise interpolate across the whole texture inside one triangle — the
//! smear band the pre-#739 spherical mapping shows at its own wrap seam.
//! Positions and normals of a split pair are identical, so shading stays
//! seamless; only the UV differs.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::pds::generator::UvMapping;

/// Compute per-vertex UVs for `mapping`, splitting shared vertices where
/// the projection is discontinuous (`pos` / `nor` grow together, `idx` is
/// re-pointed; counts are unchanged for the continuous modes). `Unknown`
/// (a mode from a newer client) meshes as the default — Box since #742 —
/// mirroring how an unknown
/// [`BlobShape`](crate::pds::generator::BlobShape) meshes as a sphere.
pub(super) fn project_uvs(
    mapping: UvMapping,
    pos: &mut Vec<[f32; 3]>,
    nor: &mut Vec<[f32; 3]>,
    idx: &mut [u32],
) -> Vec<[f32; 2]> {
    match mapping {
        UvMapping::Spherical => spherical(pos),
        UvMapping::Box | UvMapping::Unknown => box_mapped(pos, nor, idx),
        UvMapping::Cylindrical => cylindrical(pos, nor, idx),
        UvMapping::PlanarX | UvMapping::PlanarY | UvMapping::PlanarZ => planar(pos, mapping),
    }
}

/// Tight AABB of the vertex positions.
fn bounds(pos: &[[f32; 3]]) -> (Vec3, Vec3) {
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for p in pos {
        let p = Vec3::from_array(*p);
        lo = lo.min(p);
        hi = hi.max(p);
    }
    (lo, hi)
}

/// The original mapping (the default until #742), kept formula-identical
/// to the pre-#739 inline code: equirectangular projection of each
/// vertex's direction from the surface centroid.
fn spherical(pos: &[[f32; 3]]) -> Vec<[f32; 2]> {
    use std::f32::consts::{PI, TAU};
    let centroid = pos
        .iter()
        .fold(Vec3::ZERO, |acc, p| acc + Vec3::from_array(*p))
        / pos.len().max(1) as f32;
    pos.iter()
        .map(|p| {
            let d = (Vec3::from_array(*p) - centroid).normalize_or_zero();
            [
                0.5 + d.z.atan2(d.x) / TAU,
                0.5 - d.y.clamp(-1.0, 1.0).asin() / PI,
            ]
        })
        .collect()
}

/// Flat projection along one local axis. Uniform `1 / longest-extent`
/// scale (not per-axis normalisation, which would just re-introduce the
/// stretch this mode exists to fix), centred so the texture covers the
/// mass once along its longest axis.
fn planar(pos: &[[f32; 3]], mapping: UvMapping) -> Vec<[f32; 2]> {
    let (lo, hi) = bounds(pos);
    let c = (lo + hi) * 0.5;
    let inv = 1.0 / (hi - lo).max_element().max(1e-5);
    pos.iter()
        .map(|p| {
            let q = (Vec3::from_array(*p) - c) * inv;
            match mapping {
                UvMapping::PlanarX => [0.5 + q.z, 0.5 - q.y],
                UvMapping::PlanarY => [0.5 + q.x, 0.5 + q.z],
                _ => [0.5 + q.x, 0.5 - q.y],
            }
        })
        .collect()
}

/// Baked tri-planar box projection: each triangle projects along the axis
/// its summed vertex normals lean into most (the SDF-gradient normals are
/// smoother than per-face geometric normals, so region borders wander
/// less), with the six face orientations chosen so no face mirrors its
/// texture. Shared vertices are duplicated per `(vertex, face)` — interior
/// vertices stay shared, only region borders split.
fn box_mapped(pos: &mut Vec<[f32; 3]>, nor: &mut Vec<[f32; 3]>, idx: &mut [u32]) -> Vec<[f32; 2]> {
    let (lo, hi) = bounds(pos);
    let c = (lo + hi) * 0.5;
    let inv = 1.0 / (hi - lo).max_element().max(1e-5);
    let uv_for = |p: [f32; 3], region: u8| -> [f32; 2] {
        let q = (Vec3::from_array(p) - c) * inv;
        match region {
            0 => [0.5 - q.z, 0.5 - q.y], // +X
            1 => [0.5 + q.z, 0.5 - q.y], // −X
            2 => [0.5 + q.x, 0.5 + q.z], // +Y
            3 => [0.5 + q.x, 0.5 - q.z], // −Y
            4 => [0.5 + q.x, 0.5 - q.y], // +Z
            _ => [0.5 - q.x, 0.5 - q.y], // −Z
        }
    };

    let mut out_pos: Vec<[f32; 3]> = Vec::with_capacity(pos.len() + pos.len() / 4);
    let mut out_nor: Vec<[f32; 3]> = Vec::with_capacity(out_pos.capacity());
    let mut out_uv: Vec<[f32; 2]> = Vec::with_capacity(out_pos.capacity());
    let mut slots: HashMap<(u32, u8), u32> = HashMap::with_capacity(pos.len());
    for tri in idx.chunks_exact_mut(3) {
        let n = tri.iter().fold(Vec3::ZERO, |acc, &i| {
            acc + Vec3::from_array(nor[i as usize])
        });
        let a = n.abs();
        let axis = if a.x >= a.y && a.x >= a.z {
            0
        } else if a.y >= a.z {
            1
        } else {
            2
        };
        let region = (axis * 2) as u8 + (n[axis] < 0.0) as u8;
        for i in tri {
            let old = *i as usize;
            *i = *slots.entry((*i, region)).or_insert_with(|| {
                out_pos.push(pos[old]);
                out_nor.push(nor[old]);
                out_uv.push(uv_for(pos[old], region));
                (out_pos.len() - 1) as u32
            });
        }
    }
    *pos = out_pos;
    *nor = out_nor;
    out_uv
}

/// Wrap around the vertical axis through the bounds centre: U is azimuth,
/// V descends from the top scaled by `1 / (τ · mean radius)` so a texel
/// stays square against the group's mean circumference — the swept prims'
/// convention (`sweeps::v_of`). Triangles straddling the azimuth wrap
/// re-point their low-U corners at `U + 1` duplicates (the repeat sampler
/// tiles them back), killing the one-triangle-wide smear band a shared
/// seam vertex would cause.
fn cylindrical(pos: &mut Vec<[f32; 3]>, nor: &mut Vec<[f32; 3]>, idx: &mut [u32]) -> Vec<[f32; 2]> {
    use std::f32::consts::TAU;
    let (lo, hi) = bounds(pos);
    let c = (lo + hi) * 0.5;
    let mean_r = pos
        .iter()
        .map(|p| Vec2::new(p[0] - c.x, p[2] - c.z).length())
        .sum::<f32>()
        / pos.len().max(1) as f32;
    let inv_v = 1.0 / (TAU * mean_r).max(1e-5);
    let mut uv: Vec<[f32; 2]> = pos
        .iter()
        .map(|p| {
            [
                0.5 + (p[2] - c.z).atan2(p[0] - c.x) / TAU,
                (hi.y - p[1]) * inv_v,
            ]
        })
        .collect();

    let mut shifted: HashMap<u32, u32> = HashMap::new();
    for tri in idx.chunks_exact_mut(3) {
        let max_u = tri
            .iter()
            .map(|&i| uv[i as usize][0])
            .fold(f32::NEG_INFINITY, f32::max);
        for i in tri {
            let old = *i as usize;
            if max_u - uv[old][0] > 0.5 {
                use std::collections::hash_map::Entry;
                *i = match shifted.entry(*i) {
                    Entry::Occupied(e) => *e.get(),
                    Entry::Vacant(e) => {
                        let slot = pos.len() as u32;
                        let (p, n, u) = (pos[old], nor[old], uv[old]);
                        pos.push(p);
                        nor.push(n);
                        uv.push([u[0] + 1.0, u[1]]);
                        *e.insert(slot)
                    }
                };
            }
        }
    }
    uv
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A full ring of wall quads around the axis: the quad straddling the
    /// atan2 discontinuity (the −X meridian) must re-point its low-U
    /// corners at `U + 1` duplicates so no wall triangle interpolates
    /// across the whole texture, and every duplicate must share position
    /// and normal with an original.
    #[test]
    fn cylindrical_splits_the_wrap_seam() {
        use std::f32::consts::TAU;
        const COLS: usize = 8;
        // Two rows (y = 0 / 1) of COLS columns; vertex id = col * 2 + row.
        // The ring is symmetric, so the bounds centre is the axis itself.
        let mut pos: Vec<[f32; 3]> = (0..COLS)
            .flat_map(|k| {
                let a = k as f32 / COLS as f32 * TAU;
                [[a.cos(), 0.0, a.sin()], [a.cos(), 1.0, a.sin()]]
            })
            .collect();
        let mut nor: Vec<[f32; 3]> = pos
            .iter()
            .map(|p| Vec3::new(p[0], 0.0, p[2]).normalize_or_zero().to_array())
            .collect();
        let mut idx: Vec<u32> = (0..COLS as u32)
            .flat_map(|k| {
                let (a, b) = (k * 2, (k + 1) % COLS as u32 * 2);
                [a, a + 1, b, b, a + 1, b + 1]
            })
            .collect();
        let originals = pos.len();
        let uv = cylindrical(&mut pos, &mut nor, &mut idx);

        assert_eq!(uv.len(), pos.len());
        assert_eq!(nor.len(), pos.len());
        assert!(pos.len() > originals, "the seam quad split no vertices");
        for tri in idx.chunks_exact(3) {
            let us: Vec<f32> = tri.iter().map(|&i| uv[i as usize][0]).collect();
            let span = us.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
                - us.iter().cloned().fold(f32::INFINITY, f32::min);
            assert!(span < 0.5, "a wall triangle interpolates across the seam");
        }
        for dup in originals..pos.len() {
            let orig = (0..originals)
                .find(|&i| pos[i] == pos[dup])
                .expect("every duplicate shadows an original");
            assert_eq!(nor[orig], nor[dup]);
            assert!((uv[dup][0] - uv[orig][0] - 1.0).abs() < 1e-6);
            assert_eq!(uv[dup][1], uv[orig][1]);
        }
    }

    /// Box projection on a shared-vertex octahedron: every triangle's
    /// summed normal leans into exactly one axis, so all 8 faces classify
    /// distinctly and the 6 shared tips split per adjacent region while
    /// UVs stay inside the unit square.
    #[test]
    fn box_mapping_splits_regions_and_stays_in_range() {
        let mut pos = vec![
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ];
        let mut nor: Vec<[f32; 3]> = pos.clone();
        let mut idx = vec![
            0, 2, 4, 4, 2, 1, 1, 2, 5, 5, 2, 0, // top four faces
            0, 4, 3, 4, 1, 3, 1, 5, 3, 5, 0, 3, // bottom four faces
        ];
        let uv = box_mapped(&mut pos, &mut nor, &mut idx);
        assert_eq!(uv.len(), pos.len());
        assert_eq!(nor.len(), pos.len());
        assert!(pos.len() > 6, "shared tips split across regions");
        assert_eq!(idx.len(), 24, "triangle count unchanged");
        for (i, u) in uv.iter().enumerate() {
            assert!(
                u.iter()
                    .all(|c| c.is_finite() && (-0.01..=1.01).contains(c)),
                "uv {i} out of range: {u:?}"
            );
        }
    }

    /// Planar modes are pure per-vertex maps: no topology change, unit
    /// range, and the projected axis carries no UV variation.
    #[test]
    fn planar_modes_project_flat() {
        let pos = vec![
            [-2.0, 0.0, -1.0],
            [2.0, 0.5, -1.0],
            [2.0, 1.0, 1.0],
            [-2.0, 0.25, 1.0],
        ];
        for mapping in [UvMapping::PlanarX, UvMapping::PlanarY, UvMapping::PlanarZ] {
            let uv = planar(&pos, mapping);
            assert_eq!(uv.len(), pos.len());
            assert!(
                uv.iter()
                    .flatten()
                    .all(|c| c.is_finite() && (-0.01..=1.01).contains(c))
            );
        }
        // PlanarY ignores height: two verts differing only in Y share a UV.
        let uv = planar(&[[0.3, 0.0, -0.4], [0.3, 9.0, -0.4]], UvMapping::PlanarY);
        assert_eq!(uv[0], uv[1]);
    }
}
