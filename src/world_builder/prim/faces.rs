//! Per-triangle face identity (#958): the [`FaceTable`] every primitive
//! mesher emits alongside its `Mesh`, the [`FaceSpans`] accumulator the
//! hand-built meshers fill as they append triangle blocks, and the
//! normal-based classifiers that recover the same vocabulary from Bevy's
//! stock builders.
//!
//! Face keys are *semantic* (see [`FaceKey`]): they name what a surface
//! **is**, so the same key survives a cut being toggled on and off, and a
//! [`FaceOverride`](crate::pds::generator::FaceOverride) addressed to it
//! keeps meaning. The two meshers a kind may use — Bevy's stock builder
//! while untortured, our swept mesher once a cut is active — must therefore
//! agree on the vocabulary, which the `faces_survive_cut_toggle` test pins.
//!
//! # Why emission-time spans still name the right triangles at spawn
//!
//! Everything downstream of a mesher preserves triangle order and count:
//! vertex torture mutates positions and normals only; the UV re-projection
//! walks `chunks_exact(3)` and re-points indices in place; `generate_tangents`
//! only adds an attribute; [`orient_to_normals`](super::base::orient_to_normals)
//! swaps two corners *within* a triangle; and `subdivide_flat` expands each
//! triangle into four consecutive ones, which [`FaceTable::subdivide`]
//! mirrors. So a span recorded at emission is still correct after the whole
//! pipeline — no re-derivation, and nothing to keep in sync by hand.

use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;

use crate::pds::generator::FaceKey;

/// A built primitive: its mesh plus the face each triangle belongs to.
///
/// Returned by [`build_primitive_mesh`](super::build_primitive_mesh). The
/// table is what lets the spawner split a prim into per-face material
/// groups (#959) and the editor resolve a picked triangle back to a face
/// (#961).
#[derive(Clone, Debug)]
pub struct PrimMesh {
    pub mesh: Mesh,
    pub faces: FaceTable,
}

/// Which face each triangle of a primitive mesh belongs to, as contiguous
/// spans in emission order.
///
/// Spans tile `[0, triangle_count)` exactly. A face may own more than one
/// span — the box sweep emits its four sides once per wall row — so this is
/// deliberately *not* a map; adjacent spans sharing a key merge on push, so
/// the overwhelmingly common single-face prim costs one entry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FaceTable {
    /// `(face, end)`: the face covers triangles `[previous end, end)`.
    spans: Vec<(FaceKey, u32)>,
}

impl FaceTable {
    /// A table whose whole mesh is one face — the smooth closed prims
    /// (sphere, blob, plane) and every Bevy stock path with no cap split.
    pub fn single(face: FaceKey, triangles: u32) -> Self {
        let mut table = Self::default();
        table.push(face, triangles);
        table
    }

    /// Append `triangles` more triangles belonging to `face`, merging with
    /// the previous span when the key repeats. A zero-length run is dropped
    /// (a cap the mesher skipped emits nothing and must not appear).
    pub fn push(&mut self, face: FaceKey, triangles: u32) {
        if triangles == 0 {
            return;
        }
        match self.spans.last_mut() {
            Some((last, end)) if *last == face => *end += triangles,
            _ => {
                let end = self.triangle_count() + triangles;
                self.spans.push((face, end));
            }
        }
    }

    /// Total triangles covered.
    pub fn triangle_count(&self) -> u32 {
        self.spans.last().map(|(_, end)| *end).unwrap_or(0)
    }

    /// The face triangle `tri` belongs to, or `None` when out of range.
    pub fn face_of(&self, tri: u32) -> Option<FaceKey> {
        let i = self.spans.partition_point(|(_, end)| *end <= tri);
        self.spans.get(i).map(|(face, _)| *face)
    }

    /// The distinct faces present, in emission order.
    pub fn faces(&self) -> Vec<FaceKey> {
        let mut out: Vec<FaceKey> = Vec::with_capacity(self.spans.len());
        for (face, _) in &self.spans {
            if !out.contains(face) {
                out.push(*face);
            }
        }
        out
    }

    /// `(face, start, end)` for every span, in emission order.
    pub fn spans(&self) -> impl Iterator<Item = (FaceKey, u32, u32)> + '_ {
        let mut start = 0;
        self.spans.iter().map(move |(face, end)| {
            let span = (*face, start, *end);
            start = *end;
            span
        })
    }

    /// Mirror `levels` rounds of [`subdivide_flat`](super::base::subdivide_flat),
    /// which replaces every triangle with four consecutive ones — so each
    /// span simply scales by `4^levels`.
    pub fn subdivide(&mut self, levels: u32) {
        let factor = 4u32.saturating_pow(levels);
        for (_, end) in self.spans.iter_mut() {
            *end *= factor;
        }
    }
}

/// Accumulator the hand-built meshers fill as they append triangle blocks:
/// after emitting a block, call [`mark`](Self::mark) with the block's face
/// and the index buffer, and the span from the previous mark is recorded.
///
/// Marking *after* each block (rather than declaring counts up front) is
/// what keeps the table honest when a block is conditionally skipped — a
/// zero-radius cap, an absent bore — because a skipped block marks nothing.
#[derive(Default)]
pub(super) struct FaceSpans {
    table: FaceTable,
    marked: u32,
}

impl FaceSpans {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Assign every triangle appended to `idx` since the previous mark to
    /// `face`.
    pub(super) fn mark(&mut self, idx: &[u32], face: FaceKey) {
        let total = (idx.len() / 3) as u32;
        self.table.push(face, total.saturating_sub(self.marked));
        self.marked = total;
    }

    /// Finish the table for a mesh of `total_tris` triangles.
    ///
    /// Exact coverage is a mesher invariant, so a shortfall is a bug in the
    /// caller's marking — `debug_assert` catches it in every test run, and
    /// release builds absorb the tail into the last face rather than leaving
    /// triangles unaddressable.
    pub(super) fn finish(mut self, total_tris: u32) -> FaceTable {
        debug_assert_eq!(
            self.marked, total_tris,
            "face spans cover {} of {total_tris} triangles — a mesher block is unmarked",
            self.marked
        );
        if self.marked < total_tris {
            let tail = self
                .table
                .spans
                .last()
                .map(|(face, _)| *face)
                .unwrap_or(FaceKey::Surface);
            self.table.push(tail, total_tris - self.marked);
        }
        self.table
    }
}

/// Wrap a mesh whose whole surface is one face — Bevy's stock sphere,
/// capsule, torus and plane, which have no cap/wall split to recover.
pub(super) fn whole(mesh: Mesh, face: FaceKey) -> PrimMesh {
    let triangles = match mesh.indices() {
        Some(i) => (i.len() / 3) as u32,
        None => (mesh.count_vertices() / 3) as u32,
    };
    PrimMesh {
        faces: FaceTable::single(face, triangles),
        mesh,
    }
}

/// Wrap a mesh built by a stock Bevy builder, recovering face identity from
/// triangle normals via [`classify_by_normal`].
pub(super) fn classified(
    mesh: Mesh,
    fallback: FaceKey,
    classify: impl Fn(Vec3) -> FaceKey,
) -> PrimMesh {
    let faces = classify_by_normal(&mesh, fallback, classify);
    PrimMesh { mesh, faces }
}

/// Build a [`FaceTable`] for a mesh whose triangles must be classified after
/// the fact — Bevy's stock builders, which emit no block structure we can
/// mark. `classify` receives each triangle's summed vertex normal.
///
/// Falls back to a single `fallback` span when the mesh lacks the normals or
/// indices to classify, so a stock builder that changes shape upstream
/// degrades to "one face" rather than to an empty table.
pub(super) fn classify_by_normal(
    mesh: &Mesh,
    fallback: FaceKey,
    classify: impl Fn(Vec3) -> FaceKey,
) -> FaceTable {
    let Some(VertexAttributeValues::Float32x3(nor)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else {
        return FaceTable::single(fallback, triangle_count(mesh));
    };
    let Some(indices) = mesh.indices() else {
        return FaceTable::single(fallback, triangle_count(mesh));
    };
    let idx: Vec<usize> = indices.iter().collect();
    let mut table = FaceTable::default();
    for tri in idx.chunks_exact(3) {
        let n = tri.iter().fold(Vec3::ZERO, |acc, &i| {
            acc + nor
                .get(i)
                .map(|n| Vec3::from_array(*n))
                .unwrap_or(Vec3::ZERO)
        });
        table.push(classify(n.normalize_or_zero()), 1);
    }
    table
}

/// Triangle count of a mesh, indexed or not.
fn triangle_count(mesh: &Mesh) -> u32 {
    match mesh.indices() {
        Some(i) => (i.len() / 3) as u32,
        None => (mesh.count_vertices() / 3) as u32,
    }
}

/// Classifier for the axis-aligned box family (the Bevy `Cuboid`): the six
/// sides by the exact axis their normal points along.
pub(super) fn box_face(n: Vec3) -> FaceKey {
    let a = n.abs();
    if a.x >= a.y && a.x >= a.z {
        if n.x >= 0.0 {
            FaceKey::SidePx
        } else {
            FaceKey::SideNx
        }
    } else if a.y >= a.z {
        if n.y >= 0.0 {
            FaceKey::Top
        } else {
            FaceKey::Bottom
        }
    } else if n.z >= 0.0 {
        FaceKey::SidePz
    } else {
        FaceKey::SideNz
    }
}

/// Classifier for the revolved family's stock builders (Bevy `Cylinder` /
/// `Cone`): a cap normal points exactly along the axis, everything else is
/// wall.
///
/// The test is deliberately near-exact rather than the `|n.y| > 0.5`
/// heuristic the UV rescaler uses: a squat cone's wall normal can pass 0.5
/// comfortably (a `r = 1.0`, `h = 0.2` cone reaches ~0.98), and mistaking
/// its wall for its cap would paint the whole prim with the cap's material.
pub(super) fn revolved_face(n: Vec3) -> FaceKey {
    if n.y > 0.999 {
        FaceKey::Top
    } else if n.y < -0.999 {
        FaceKey::Bottom
    } else {
        FaceKey::Wall
    }
}

/// Classifier for the Bevy `Tetrahedron`: a horizontal base plus three
/// lateral faces named by the direction they lean (see
/// [`TetrahedronShape`](super::shapes) for the corner layout — one base
/// corner sits at `-Z`, the other two at `+Z`, so the `+Z` face is the
/// front and the remaining two split left / right by their `X` sign).
pub(super) fn tetra_face(n: Vec3) -> FaceKey {
    if n.y < -0.9 {
        FaceKey::Base
    } else if n.z >= n.x.abs() {
        FaceKey::Front
    } else if n.x >= 0.0 {
        FaceKey::Right
    } else {
        FaceKey::Left
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_tile_the_mesh_and_resolve_triangles() {
        let mut t = FaceTable::default();
        t.push(FaceKey::Wall, 4);
        // A repeated key merges rather than growing the table.
        t.push(FaceKey::Wall, 2);
        t.push(FaceKey::Top, 3);
        // A skipped block contributes nothing.
        t.push(FaceKey::Bottom, 0);
        assert_eq!(t.triangle_count(), 9);
        assert_eq!(t.spans().count(), 2, "same-key spans must merge");
        assert_eq!(t.faces(), vec![FaceKey::Wall, FaceKey::Top]);
        assert_eq!(t.face_of(0), Some(FaceKey::Wall));
        assert_eq!(t.face_of(5), Some(FaceKey::Wall));
        assert_eq!(t.face_of(6), Some(FaceKey::Top));
        assert_eq!(t.face_of(8), Some(FaceKey::Top));
        assert_eq!(t.face_of(9), None, "past the end resolves to nothing");
        // Spans are contiguous and ordered.
        let spans: Vec<_> = t.spans().collect();
        assert_eq!(spans[0], (FaceKey::Wall, 0, 6));
        assert_eq!(spans[1], (FaceKey::Top, 6, 9));
    }

    #[test]
    fn subdivide_scales_every_span() {
        let mut t = FaceTable::default();
        t.push(FaceKey::Slope, 2);
        t.push(FaceKey::Back, 1);
        t.subdivide(2); // 4² = 16 triangles per original
        assert_eq!(t.triangle_count(), 48);
        assert_eq!(t.face_of(31), Some(FaceKey::Slope));
        assert_eq!(t.face_of(32), Some(FaceKey::Back));
    }

    #[test]
    fn spans_mark_only_what_was_emitted() {
        let mut spans = FaceSpans::new();
        let mut idx: Vec<u32> = Vec::new();
        idx.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        spans.mark(&idx, FaceKey::Wall);
        // A skipped block: no triangles appended between marks.
        spans.mark(&idx, FaceKey::Top);
        idx.extend_from_slice(&[4, 5, 6]);
        spans.mark(&idx, FaceKey::Bottom);
        let table = spans.finish(3);
        assert_eq!(table.faces(), vec![FaceKey::Wall, FaceKey::Bottom]);
        assert_eq!(table.face_of(2), Some(FaceKey::Bottom));
    }

    #[test]
    fn classifiers_name_the_axes_they_face() {
        assert_eq!(box_face(Vec3::X), FaceKey::SidePx);
        assert_eq!(box_face(-Vec3::X), FaceKey::SideNx);
        assert_eq!(box_face(Vec3::Z), FaceKey::SidePz);
        assert_eq!(box_face(-Vec3::Z), FaceKey::SideNz);
        assert_eq!(box_face(Vec3::Y), FaceKey::Top);
        assert_eq!(box_face(-Vec3::Y), FaceKey::Bottom);

        assert_eq!(revolved_face(Vec3::Y), FaceKey::Top);
        assert_eq!(revolved_face(-Vec3::Y), FaceKey::Bottom);
        assert_eq!(revolved_face(Vec3::X), FaceKey::Wall);
        // The squat-cone case the 0.5 heuristic would get wrong.
        assert_eq!(
            revolved_face(Vec3::new(0.2, 0.98, 0.0).normalize()),
            FaceKey::Wall
        );

        assert_eq!(tetra_face(-Vec3::Y), FaceKey::Base);
        assert_eq!(tetra_face(Vec3::new(0.0, 0.3, 0.9)), FaceKey::Front);
        assert_eq!(tetra_face(Vec3::new(0.9, 0.3, -0.3)), FaceKey::Right);
        assert_eq!(tetra_face(Vec3::new(-0.9, 0.3, -0.3)), FaceKey::Left);
    }
}
