//! CPU-side vertex torture: the resolved [`Torture`] parameter block, the
//! per-vertex deformation kernel, and the whole-mesh post-pass that mutates
//! positions and re-derives normals/tangents from the deformation Jacobian.

use bevy::math::Mat3;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;

use crate::pds::GeneratorKind;

/// Torture parameters resolved from a primitive generator's
/// [`TortureParams`](crate::pds::TortureParams) into compute-friendly types.
/// Each is applied to the mesh's vertex positions along the shape's Y extent
/// (`t = normalised height ∈ [0, 1]`):
///
/// * `twist` — radians of rotation around Y, linear in `t`.
/// * `taper` — per-axis scale `1 - taper[axis] * t` (`.x` → X, `.y` → Z).
///   Equal components taper uniformly (cone / frustum); unequal ones give a
///   wedge / fin.
/// * `bend` — quadratic top displacement `bend * t²` on all three axes (the
///   `.y` component now lengthens / shortens the shape's top).
/// * `s_bend` — a `sin(2π t)` lateral wave of amplitude `(x, z)` layered on
///   top of `bend`, so a column can snake into an S rather than only arc.
/// * `shear` — a linear lateral slide of the top relative to the base
///   (`shear * t` on X / Z), so edges stay straight but lean — a
///   parallelepiped rather than a curve.
#[derive(Clone, Copy)]
pub(super) struct Torture {
    pub twist: f32,
    pub taper: Vec2,
    pub bend: Vec3,
    pub s_bend: Vec2,
    pub shear: Vec2,
}

impl Torture {
    pub fn is_identity(&self) -> bool {
        self.twist.abs() < 1e-6
            && self.taper.length_squared() < 1e-12
            && self.bend.length_squared() < 1e-12
            && self.s_bend.length_squared() < 1e-12
            && self.shear.length_squared() < 1e-12
    }
}

pub(super) fn torture_of(kind: &GeneratorKind) -> Torture {
    match kind.torture() {
        Some(t) => Torture {
            twist: t.twist.0,
            taper: Vec2::from_array(t.taper.0),
            bend: Vec3::from_array(t.bend.0),
            s_bend: Vec2::from_array(t.s_bend.0),
            shear: Vec2::from_array(t.shear.0),
        },
        None => Torture {
            twist: 0.0,
            taper: Vec2::ZERO,
            bend: Vec3::ZERO,
            s_bend: Vec2::ZERO,
            shear: Vec2::ZERO,
        },
    }
}

/// The pure torture deformation `D(p)`: twist + taper + bend applied to a
/// vertex, parametrised by the vertex's normalised Y height `t = (y - y_min) /
/// y_range`. Factored out of [`apply_vertex_torture`] so the same map drives
/// both the position warp and the normal transform (via its Jacobian), and so
/// future multi-axis torture only has to extend one function.
///
/// `t` is clamped to `[0, 1]`, pinning `t = 0` at the lowest vertex and
/// `t = 1` at the highest — well-defined for any primitive whether its origin
/// sits at the base or the centre.
pub(super) fn deform_vertex(p: Vec3, y_min: f32, y_range: f32, torture: Torture) -> Vec3 {
    let t = ((p.y - y_min) / y_range).clamp(0.0, 1.0);

    // Per-axis taper: scale X/Z independently around the central axis.
    // Bounded away from zero by the sanitizer (|taper| ≤ 0.99) so a face never
    // collapses to a point. Equal components = uniform taper (cone/frustum);
    // unequal = wedge / fin.
    let mut x = p.x * (1.0 - torture.taper.x * t);
    let mut z = p.z * (1.0 - torture.taper.y * t);

    // Twist: rotate around Y by an angle linear in normalised height.
    if torture.twist.abs() > 1e-6 {
        let (s, c) = (torture.twist * t).sin_cos();
        let (ox, oz) = (x, z);
        x = c * ox - s * oz;
        z = s * ox + c * oz;
    }

    // Bend: quadratic displacement, tangent to vertical at the base and
    // peaking at the top — now on all three axes (`.y` lengthens the top).
    // S-bend: a sin(2π t) lateral wave layered on top for a serpentine column.
    // Shear: a linear lateral slide of the top relative to the base, so edges
    // stay straight but lean (a parallelepiped) rather than curving like bend.
    let t2 = t * t;
    let wave = (std::f32::consts::TAU * t).sin();
    Vec3::new(
        x + torture.bend.x * t2 + torture.s_bend.x * wave + torture.shear.x * t,
        p.y + torture.bend.y * t2,
        z + torture.bend.z * t2 + torture.s_bend.y * wave + torture.shear.y * t,
    )
}

/// Mutate `mesh`'s vertex positions in-place through [`deform_vertex`], then
/// transform each vertex normal by the **inverse-transpose Jacobian** of that
/// deformation so the original shading character survives the warp.
///
/// This replaces the old flat-normal recompute, which faceted every tortured
/// shape (a twisted sphere went low-poly) — and a naive smooth recompute would
/// instead round a cuboid's hard edges. Transforming the *existing* per-vertex
/// normals by the local Jacobian preserves both: a cuboid's per-face normals
/// stay per-face (sharp), a sphere's stay smooth, and both tilt correctly with
/// the taper/twist/bend. The Jacobian is taken numerically (the map is smooth
/// and cheap to sample), with a fallback to the untransformed normal where the
/// local Jacobian is near-singular. Tangents are regenerated afterwards.
pub(super) fn apply_vertex_torture(mesh: &mut Mesh, torture: Torture) {
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return;
    };
    if positions.is_empty() {
        return;
    }
    let orig: Vec<Vec3> = positions.iter().map(|p| Vec3::from_array(*p)).collect();

    let (mut y_min, mut y_max) = (f32::INFINITY, f32::NEG_INFINITY);
    for p in &orig {
        y_min = y_min.min(p.y);
        y_max = y_max.max(p.y);
    }
    let y_range = (y_max - y_min).max(1e-6);
    let deform = |p: Vec3| deform_vertex(p, y_min, y_range, torture);

    let new_pos: Vec<[f32; 3]> = orig.iter().map(|p| deform(*p).to_array()).collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, new_pos);

    // Normals follow the deformation's inverse-transpose Jacobian, sampled
    // numerically per vertex. `eps` is small relative to any primitive extent
    // (sanitiser floors dimensions at 0.01), so the central map stays linear
    // across the stencil.
    if let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
    {
        const EPS: f32 = 1e-3;
        let new_norms: Vec<[f32; 3]> = orig
            .iter()
            .zip(normals.iter())
            .map(|(p, n)| {
                let n0 = Vec3::from_array(*n);
                let d = deform(*p);
                let jacobian = Mat3::from_cols(
                    (deform(*p + Vec3::X * EPS) - d) / EPS,
                    (deform(*p + Vec3::Y * EPS) - d) / EPS,
                    (deform(*p + Vec3::Z * EPS) - d) / EPS,
                );
                if jacobian.determinant().abs() < 1e-9 {
                    return n0.normalize_or_zero().to_array();
                }
                jacobian
                    .inverse()
                    .transpose()
                    .mul_vec3(n0)
                    .normalize_or_zero()
                    .to_array()
            })
            .collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, new_norms);
    }

    let _ = mesh.generate_tangents();
}
