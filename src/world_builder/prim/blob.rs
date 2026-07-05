//! BlobGroup mesher (#690): evaluate the element list as one signed
//! distance field (polynomial smooth-min in list order, Dreams-style) and
//! polygonize it once with surface nets. The SDF layer is hand-rolled — the
//! ecosystem crates for this (sdfu, saft) are unmaintained, and the whole
//! layer is smaller than their integration glue. Formulas follow Inigo
//! Quilez's distance-function reference.

use bevy::prelude::*;
use fast_surface_nets::ndshape::{RuntimeShape, Shape};
use fast_surface_nets::{SurfaceNetsBuffer, surface_nets};

use crate::pds::generator::{BlobElement, BlobShape};

use super::base::mesh_from_parts;

/// One element resolved from wire types into compute-friendly form.
struct ResolvedElement {
    shape: BlobShape,
    /// World→local rotation (inverse of the element's orientation).
    inv_rot: Quat,
    position: Vec3,
    radii: Vec3,
    subtract: bool,
    blend: f32,
}

/// A conservative rotation-independent bounding radius for the element.
fn bound_radius(e: &ResolvedElement) -> f32 {
    match e.shape {
        BlobShape::Ellipsoid => e.radii.x.max(e.radii.y).max(e.radii.z),
        BlobShape::Capsule => e.radii.x + e.radii.y,
        // Sphere and forward-compat Unknown both read radii[0].
        BlobShape::Sphere | BlobShape::Unknown => e.radii.x,
    }
}

fn resolve(elements: &[BlobElement]) -> Vec<ResolvedElement> {
    elements
        .iter()
        .map(|e| ResolvedElement {
            shape: e.shape,
            inv_rot: Quat::from_array(e.rotation.0).normalize().inverse(),
            position: Vec3::from_array(e.position.0),
            radii: Vec3::from_array(e.radii.0).max(Vec3::splat(0.005)),
            subtract: e.subtract,
            blend: e.blend.0.max(0.0),
        })
        .collect()
}

/// Signed distance of one element at world-space `p` (negative inside).
fn element_sdf(e: &ResolvedElement, p: Vec3) -> f32 {
    let q = e.inv_rot * (p - e.position);
    match e.shape {
        BlobShape::Sphere | BlobShape::Unknown => q.length() - e.radii.x,
        BlobShape::Capsule => {
            let mut q = q;
            q.y -= q.y.clamp(-e.radii.y, e.radii.y);
            q.length() - e.radii.x
        }
        BlobShape::Ellipsoid => {
            // IQ's bound-improved approximation: exact enough for meshing,
            // and monotone so smooth-min stays well-behaved.
            let k0 = (q / e.radii).length();
            let k1 = (q / (e.radii * e.radii)).length();
            if k1 < 1e-6 {
                -e.radii.min_element()
            } else {
                k0 * (k0 - 1.0) / k1
            }
        }
    }
}

/// Polynomial smooth minimum (IQ): blends `a` and `b` within distance `k`.
fn smooth_min(a: f32, b: f32, k: f32) -> f32 {
    if k <= 1e-5 {
        return a.min(b);
    }
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    b + (a - b) * h - k * h * (1.0 - h)
}

/// Smooth subtraction: carves `d` (the element) out of `acc`.
fn smooth_sub(acc: f32, d: f32, k: f32) -> f32 {
    if k <= 1e-5 {
        return acc.max(-d);
    }
    let h = (0.5 - 0.5 * (acc + d) / k).clamp(0.0, 1.0);
    acc + (-d - acc) * h + k * h * (1.0 - h)
}

/// Evaluate the whole group's SDF at `p`: elements fold in list order, so a
/// subtract element only carves what was already there (Dreams edit-list
/// semantics).
fn group_sdf(elements: &[ResolvedElement], p: Vec3) -> f32 {
    // The accumulator seeds from the first *additive* element rather than
    // +INF: the polynomial smooth-min computes `(acc - d) * h`, which is
    // NaN for an infinite accumulator even at h = 0.
    let mut acc = f32::INFINITY;
    for e in elements {
        let d = element_sdf(e, p);
        acc = if e.subtract {
            if acc.is_finite() {
                smooth_sub(acc, d, e.blend)
            } else {
                acc
            }
        } else if acc.is_finite() {
            smooth_min(acc, d, e.blend)
        } else {
            d
        };
    }
    acc
}

/// Hard per-axis cap on the sample grid. `resolution` (sanitize-clamped to
/// 48) sets cells along the longest axis; padding can push a dimension a
/// little past it, never past this.
const MAX_GRID_DIM: u32 = 56;
/// Empty cells kept between the surface and the grid boundary — surface
/// nets needs the isosurface strictly inside the sampled volume.
const GRID_PAD: u32 = 2;

/// Build the BlobGroup mesh: sample the group SDF over an auto-sized grid
/// and run surface nets. A group whose SDF never crosses zero (e.g. every
/// element subtracts) meshes as a small marker sphere so the prim stays
/// selectable in the editor instead of silently vanishing.
pub(super) fn build_blob_mesh(elements: &[BlobElement], resolution: u32) -> Mesh {
    let resolved = resolve(elements);
    if resolved.is_empty() {
        return marker_mesh();
    }

    // Conservative world AABB: element bounds + the largest blend bulge.
    let max_blend = resolved.iter().map(|e| e.blend).fold(0.0f32, f32::max);
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for e in &resolved {
        let r = bound_radius(e) + max_blend;
        lo = lo.min(e.position - Vec3::splat(r));
        hi = hi.max(e.position + Vec3::splat(r));
    }
    let size = (hi - lo).max(Vec3::splat(0.01));
    let res = resolution.clamp(8, 48);
    let cell = size.max_element() / res as f32;
    let dims = UVec3::new(
        (size.x / cell).ceil() as u32 + 1 + 2 * GRID_PAD,
        (size.y / cell).ceil() as u32 + 1 + 2 * GRID_PAD,
        (size.z / cell).ceil() as u32 + 1 + 2 * GRID_PAD,
    )
    .min(UVec3::splat(MAX_GRID_DIM));
    let origin = lo - Vec3::splat(GRID_PAD as f32 * cell);

    // Sample the field. RuntimeShape linearizes in x-fastest order.
    let shape = RuntimeShape::<u32, 3>::new([dims.x, dims.y, dims.z]);
    let mut samples = vec![1.0f32; shape.size() as usize];
    for i in 0..shape.size() {
        let [x, y, z] = shape.delinearize(i);
        let p = origin + Vec3::new(x as f32, y as f32, z as f32) * cell;
        samples[i as usize] = group_sdf(&resolved, p);
    }

    let mut buffer = SurfaceNetsBuffer::default();
    surface_nets(
        &samples,
        &shape,
        [0; 3],
        [dims.x - 1, dims.y - 1, dims.z - 1],
        &mut buffer,
    );
    if buffer.positions.is_empty() || buffer.indices.len() < 3 {
        return marker_mesh();
    }

    // Grid space → world space; normals from the buffer's SDF gradient;
    // spherical UVs around the surface centroid (good enough for the
    // procedural-texture pipeline on a blobby mass).
    let centroid = buffer
        .positions
        .iter()
        .fold(Vec3::ZERO, |acc, p| acc + Vec3::from_array(*p))
        / buffer.positions.len() as f32;
    let mut pos: Vec<[f32; 3]> = Vec::with_capacity(buffer.positions.len());
    let mut nor: Vec<[f32; 3]> = Vec::with_capacity(buffer.positions.len());
    let mut uv: Vec<[f32; 2]> = Vec::with_capacity(buffer.positions.len());
    for (gp, gn) in buffer.positions.iter().zip(buffer.normals.iter()) {
        let world = origin + Vec3::from_array(*gp) * cell;
        pos.push(world.to_array());
        let n = Vec3::from_array(*gn).normalize_or_zero();
        let n = if n == Vec3::ZERO {
            (Vec3::from_array(*gp) - centroid).normalize_or_zero()
        } else {
            n
        };
        nor.push(n.to_array());
        let d = (Vec3::from_array(*gp) - centroid).normalize_or_zero();
        uv.push([
            0.5 + d.z.atan2(d.x) / std::f32::consts::TAU,
            0.5 - d.y.clamp(-1.0, 1.0).asin() / std::f32::consts::PI,
        ]);
    }

    mesh_from_parts(pos, nor, uv, buffer.indices)
}

/// Tiny visible stand-in for a group with no surface (all-subtract or
/// empty): keeps the prim selectable in the editor instead of vanishing.
fn marker_mesh() -> Mesh {
    Sphere::new(0.05)
        .mesh()
        .ico(2)
        .unwrap_or_else(|_| Sphere::new(0.05).mesh().build())
}

/// Analytic point cloud for the convex-hull collider: support-ish samples
/// of every *additive* element's surface (subtractive carves are interior
/// detail a standoff hull rightly ignores). Fourteen directions per shape —
/// the 6 axes + 8 diagonals — matches the hull fidelity of the other prims.
pub(super) fn blob_hull_points(elements: &[BlobElement]) -> Vec<Vec3> {
    const DIRS: [[f32; 3]; 14] = [
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
        [0.577, 0.577, 0.577],
        [0.577, 0.577, -0.577],
        [0.577, -0.577, 0.577],
        [0.577, -0.577, -0.577],
        [-0.577, 0.577, 0.577],
        [-0.577, 0.577, -0.577],
        [-0.577, -0.577, 0.577],
        [-0.577, -0.577, -0.577],
    ];
    let resolved = resolve(elements);
    let mut out = Vec::new();
    for e in resolved.iter().filter(|e| !e.subtract) {
        let rot = e.inv_rot.inverse();
        match e.shape {
            BlobShape::Sphere | BlobShape::Unknown => {
                for d in DIRS {
                    out.push(e.position + Vec3::from_array(d) * e.radii.x);
                }
            }
            BlobShape::Ellipsoid => {
                for d in DIRS {
                    out.push(e.position + rot * (Vec3::from_array(d) * e.radii));
                }
            }
            BlobShape::Capsule => {
                for end in [-1.0f32, 1.0] {
                    let c = e.position + rot * Vec3::new(0.0, end * e.radii.y, 0.0);
                    for d in DIRS {
                        out.push(c + Vec3::from_array(d) * e.radii.x);
                    }
                }
            }
        }
    }
    out
}
