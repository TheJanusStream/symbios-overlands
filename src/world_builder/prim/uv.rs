//! Post-mesh UV projection for meshers with no analytic parameterisation
//! (#739). [`project_uvs`] maps a [`UvMapping`] mode over raw attribute
//! buffers rather than a `Mesh`, so any hand-built mesher can adopt it just
//! before its `mesh_from_parts` tail.
//!
//! # The metre convention (#933)
//!
//! **Every projection here emits UVs in metres of prim-local surface: a UV
//! delta of `1.0` is one metre across the surface.** `uv_scale` on the
//! material therefore reads as *tiles per metre* — `uv_scale: 5.0` lays a
//! 20 cm brick course whatever it is applied to.
//!
//! This replaces the original `1 / longest-extent` normalisation, under
//! which each prim received exactly one texture tile across its own bounds.
//! That made texel density a function of prim *size*: a 0.8 m pier and a
//! 6.4 m lintel sharing one material could not have matching brickwork, and
//! the only lever was a per-material `uv_scale` scalar hand-tuned per prop.
//! Size-invariance is the whole point of the change.
//!
//! World-*scale*, not world-*space*: the projections still work in the
//! buffers' own prim-local frame, so a mode's result is stable under the
//! prim's transform and the geometry-keyed mesh cache keeps working.
//! Texture continuity *between* adjacent prims is a different problem and
//! is not solved here.
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

use crate::pds::GeneratorKind;
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

/// Which metre-scale projection (if any) a primitive kind should have baked
/// over whatever its mesher produced (#934).
///
/// `Cuboid` is the case that matters and the only one taken so far. Its
/// stock parameterisation — Bevy's for the plain box, the swept rectangular
/// profile once a cut is active — lays exactly one tile across *each face*,
/// so an 8 × 4 × 0.35 wall slab wears one tile on the 8 × 4 face and
/// another crammed into the 0.35 × 4 end. Box projection fixes both at
/// once: every face is projected along its own normal at one metre scale,
/// and the two meshers stop disagreeing with each other as a side effect.
///
/// Deliberately absent:
///
/// * `Plane` — the card carrier. Every use in the catalogue is a foliage
///   billboard or a glazing card, and a card must span its quad exactly
///   once (it uploads clamp-to-edge). It gets an explicit "fit" mode with
///   the rest of the flat-faced family in #937 rather than a silent metre
///   default that would tile every card.
/// * The rotational family (`Sphere`, `Cylinder`, `Cone`, `Capsule`,
///   `Torus`) — each has *two* parameterisations already, Bevy's when
///   untortured and the sweep mesher's once a cut is active, and they mix
///   wall and cap conventions within one mesh. Rescaling that mixture
///   globally would turn every cap disc into an ellipse. Unifying them is
///   #935's job, where the sweep mesher becomes the single source of UV
///   truth.
///
/// UVs must stay a pure function of geometry: [`prim_geometry_fingerprint`]
/// drops the material from the mesh cache key, so anything material-derived
/// here would silently serve one prop's UVs to another.
///
/// [`prim_geometry_fingerprint`]: crate::world_builder::prim_cache::prim_geometry_fingerprint
pub(super) fn metre_projection_for(kind: &GeneratorKind) -> Option<UvMapping> {
    matches!(kind, GeneratorKind::Cuboid { .. }).then_some(UvMapping::Box)
}

/// Re-project a built mesh's UVs through [`project_uvs`], writing back the
/// (possibly grown) position/normal/index buffers and regenerating tangents.
///
/// Bails without touching the mesh if it lacks the float buffers or the
/// indices the projections need — a mesher that produced something exotic
/// keeps whatever UVs it made rather than losing them to a half-applied
/// pass.
pub(super) fn reproject_mesh(mesh: &mut Mesh, mapping: UvMapping) {
    use bevy::mesh::{Indices, VertexAttributeValues};

    let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return;
    };
    let Some(VertexAttributeValues::Float32x3(nor)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else {
        return;
    };
    let (mut pos, mut nor) = (pos.clone(), nor.clone());
    let mut idx: Vec<u32> = match mesh.indices() {
        Some(i) => i.iter().map(|v| v as u32).collect(),
        None => return,
    };

    let uv = project_uvs(mapping, &mut pos, &mut nor, &mut idx);

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_indices(Indices::U32(idx));
    let _ = mesh.generate_tangents();
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

/// Equirectangular projection of each vertex's direction from the surface
/// centroid (the original #739 mapping, and the default until #742), scaled
/// into metres of arc on the mass's mean sphere: `U` spans one equatorial
/// circumference, `V` one pole-to-pole half-circumference, so a texel is
/// square at the equator.
fn spherical(pos: &[[f32; 3]]) -> Vec<[f32; 2]> {
    use std::f32::consts::{PI, TAU};
    let centroid = pos
        .iter()
        .fold(Vec3::ZERO, |acc, p| acc + Vec3::from_array(*p))
        / pos.len().max(1) as f32;
    let mean_r = mean_radius(pos, centroid);
    pos.iter()
        .map(|p| {
            let d = (Vec3::from_array(*p) - centroid).normalize_or_zero();
            [
                (0.5 + d.z.atan2(d.x) / TAU) * TAU * mean_r,
                (0.5 - d.y.clamp(-1.0, 1.0).asin() / PI) * PI * mean_r,
            ]
        })
        .collect()
}

/// Mean distance of the vertices from `centre` — the radius a rotational
/// projection measures its arc lengths against.
fn mean_radius(pos: &[[f32; 3]], centre: Vec3) -> f32 {
    (pos.iter()
        .map(|p| (Vec3::from_array(*p) - centre).length())
        .sum::<f32>()
        / pos.len().max(1) as f32)
        .max(1e-5)
}

/// Flat projection along one local axis, in metres from the bounds centre.
/// Both UV axes share the one scale (metres), so the stretch a per-axis
/// normalisation would reintroduce cannot occur.
fn planar(pos: &[[f32; 3]], mapping: UvMapping) -> Vec<[f32; 2]> {
    let (lo, hi) = bounds(pos);
    let c = (lo + hi) * 0.5;
    pos.iter()
        .map(|p| {
            let q = Vec3::from_array(*p) - c;
            match mapping {
                UvMapping::PlanarX => [q.z, -q.y],
                UvMapping::PlanarY => [q.x, q.z],
                _ => [q.x, -q.y],
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
    let uv_for = |p: [f32; 3], region: u8| -> [f32; 2] {
        let q = Vec3::from_array(p) - c;
        match region {
            0 => [-q.z, -q.y], // +X
            1 => [q.z, -q.y],  // −X
            2 => [q.x, q.z],   // +Y
            3 => [q.x, -q.z],  // −Y
            4 => [q.x, -q.y],  // +Z
            _ => [-q.x, -q.y], // −Z
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

/// Wrap around the vertical axis through the bounds centre: `U` is metres
/// of arc around the mass's mean circumference, `V` metres descending from
/// the top. Both being metres is what keeps a texel square without the
/// reciprocal scaling the pre-#933 version needed.
///
/// Triangles straddling the azimuth wrap re-point their low-`U` corners at
/// duplicates one full circumference along (the repeat sampler tiles them
/// back), killing the one-triangle-wide smear band a shared seam vertex
/// would cause.
fn cylindrical(pos: &mut Vec<[f32; 3]>, nor: &mut Vec<[f32; 3]>, idx: &mut [u32]) -> Vec<[f32; 2]> {
    use std::f32::consts::TAU;
    let (lo, hi) = bounds(pos);
    let c = (lo + hi) * 0.5;
    let mean_r = pos
        .iter()
        .map(|p| Vec2::new(p[0] - c.x, p[2] - c.z).length())
        .sum::<f32>()
        / pos.len().max(1) as f32;
    // One full turn in metres. Also the offset a seam duplicate carries, so
    // the wrapped corner lands exactly one tile-period along.
    let circumference = (TAU * mean_r).max(1e-5);
    let mut uv: Vec<[f32; 2]> = pos
        .iter()
        .map(|p| {
            [
                (0.5 + (p[2] - c.z).atan2(p[0] - c.x) / TAU) * circumference,
                hi.y - p[1],
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
            if max_u - uv[old][0] > circumference * 0.5 {
                use std::collections::hash_map::Entry;
                *i = match shifted.entry(*i) {
                    Entry::Occupied(e) => *e.get(),
                    Entry::Vacant(e) => {
                        let slot = pos.len() as u32;
                        let (p, n, u) = (pos[old], nor[old], uv[old]);
                        pos.push(p);
                        nor.push(n);
                        uv.push([u[0] + circumference, u[1]]);
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
    /// corners at duplicates one full circumference along, so no wall
    /// triangle interpolates across the whole texture, and every duplicate
    /// must share position and normal with an original.
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
        // Unit ring, so the metre period the seam shifts by is one turn.
        let circumference = TAU;
        let uv = cylindrical(&mut pos, &mut nor, &mut idx);

        assert_eq!(uv.len(), pos.len());
        assert_eq!(nor.len(), pos.len());
        assert!(pos.len() > originals, "the seam quad split no vertices");
        for tri in idx.chunks_exact(3) {
            let us: Vec<f32> = tri.iter().map(|&i| uv[i as usize][0]).collect();
            let span = us.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
                - us.iter().cloned().fold(f32::INFINITY, f32::min);
            assert!(
                span < circumference * 0.5,
                "a wall triangle interpolates across the seam"
            );
        }
        for dup in originals..pos.len() {
            let orig = (0..originals)
                .find(|&i| pos[i] == pos[dup])
                .expect("every duplicate shadows an original");
            assert_eq!(nor[orig], nor[dup]);
            assert!((uv[dup][0] - uv[orig][0] - circumference).abs() < 1e-5);
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
        // Metre convention: the octahedron spans 2 m on each axis, so UVs
        // run over ±1 m about the centre rather than the old unit square.
        for (i, u) in uv.iter().enumerate() {
            assert!(
                u.iter()
                    .all(|c| c.is_finite() && (-1.01..=1.01).contains(c)),
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
            // Metres about the bounds centre: the quad is 4 m × 2 m, so no
            // component can exceed half the longest span.
            assert!(
                uv.iter()
                    .flatten()
                    .all(|c| c.is_finite() && (-2.01..=2.01).contains(c))
            );
        }
        // PlanarY ignores height: two verts differing only in Y share a UV.
        let uv = planar(&[[0.3, 0.0, -0.4], [0.3, 9.0, -0.4]], UvMapping::PlanarY);
        assert_eq!(uv[0], uv[1]);
    }

    /// The property the metre convention exists for (#933): texel density
    /// is a function of surface metres, not of prim size. Scaling a mass by
    /// `k` must scale its UV span by exactly `k` — under the old
    /// `1 / longest-extent` normalisation both spans came out `1.0` and a
    /// small prim and a large one wore the same number of tiles.
    #[test]
    fn uv_span_tracks_metres_not_prim_size() {
        let unit: Vec<[f32; 3]> = vec![
            [-1.0, -1.0, -1.0],
            [1.0, -1.0, -1.0],
            [1.0, 1.0, -1.0],
            [-1.0, 1.0, -1.0],
        ];
        let span_of = |pos: &[[f32; 3]]| {
            let uv = planar(pos, UvMapping::PlanarZ);
            let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
            for t in &uv {
                lo = lo.min(t[0]);
                hi = hi.max(t[0]);
            }
            hi - lo
        };
        let small = span_of(&unit);
        for k in [3.0_f32, 12.5] {
            let scaled: Vec<[f32; 3]> = unit.iter().map(|p| p.map(|c| c * k)).collect();
            let got = span_of(&scaled);
            assert!(
                (got - small * k).abs() < 1e-4,
                "a {k}× mass should wear {k}× the tiles, got {got} vs {}",
                small * k
            );
        }
    }
}
