//! BlobGroup mesher (#690): evaluate the element list as one signed
//! distance field (polynomial smooth-min in list order, Dreams-style) and
//! polygonize it once with surface nets. The SL-style topology cuts (#725)
//! apply as hard CSG on the final field just before meshing: `profile_cut`
//! keeps a Y-band of the element bounds, `path_cut` keeps a pie wedge
//! around the prim-local Y axis, and `hollow` erodes an inner shell. The
//! SDF layer is hand-rolled — the ecosystem crates for this (sdfu, saft)
//! are unmaintained, and the whole layer is smaller than their integration
//! glue. Formulas follow Inigo Quilez's distance-function reference.

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
        BlobShape::Box => e.radii.length(),
        // Radius + half-height corner for the capped shapes; ring + tube
        // for the torus.
        BlobShape::Cylinder | BlobShape::Cone => Vec2::new(e.radii.x, e.radii.y).length(),
        BlobShape::Torus => e.radii.x + e.radii.y,
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
        BlobShape::Box => {
            let d = q.abs() - e.radii;
            d.max(Vec3::ZERO).length() + d.max_element().min(0.0)
        }
        BlobShape::Cylinder => {
            let d = Vec2::new(
                (q.x * q.x + q.z * q.z).sqrt() - e.radii.x,
                q.y.abs() - e.radii.y,
            );
            d.x.max(d.y).min(0.0) + d.max(Vec2::ZERO).length()
        }
        BlobShape::Torus => {
            let ring = Vec2::new((q.x * q.x + q.z * q.z).sqrt() - e.radii.x, q.y);
            ring.length() - e.radii.y
        }
        BlobShape::Cone => {
            // IQ's exact capped cone with the top radius pinched to a tip:
            // base radius `radii.x` at −radii.y, apex at +radii.y.
            let (r1, h) = (e.radii.x, e.radii.y);
            let w = Vec2::new((q.x * q.x + q.z * q.z).sqrt(), q.y);
            let k1 = Vec2::new(0.0, h);
            let k2 = Vec2::new(-r1, 2.0 * h);
            let ca = Vec2::new(
                w.x - w.x.min(if w.y < 0.0 { r1 } else { 0.0 }),
                w.y.abs() - h,
            );
            let cb = w - k1 + k2 * ((k1 - w).dot(k2) / k2.length_squared()).clamp(0.0, 1.0);
            let s = if cb.x < 0.0 && ca.y < 0.0 { -1.0 } else { 1.0 };
            s * ca.length_squared().min(cb.length_squared()).sqrt()
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

/// Signed distance (in the XZ plane, infinite along Y) of the **kept**
/// angular sector `[mid − half, mid + half]` around the prim-local Y axis —
/// the blob analogue of the swept prims' path-cut. Negative inside the
/// wedge; the two flat faces land exactly on the cut angles, and the axis
/// itself is on the boundary (distance `r` when the nearest rim is behind
/// the apex).
fn wedge_sdf(p: Vec3, mid: f32, half: f32) -> f32 {
    use std::f32::consts::{FRAC_PI_2, PI, TAU};
    let r = (p.x * p.x + p.z * p.z).sqrt();
    let mut ang = (p.z.atan2(p.x) - mid).rem_euclid(TAU);
    if ang > PI {
        ang -= TAU;
    }
    // Angular offset past the nearer rim: negative inside the kept sector.
    let beta = ang.abs() - half;
    let rim = |b: f32| if b >= FRAC_PI_2 { r } else { r * b.sin() };
    if beta <= 0.0 { -rim(-beta) } else { rim(beta) }
}

/// Build the BlobGroup mesh: sample the group SDF over an auto-sized grid,
/// apply the SL-style topology cuts as hard CSG on the field (`t0..t1`
/// keeps a Y-band of the element bounds, `a0..a1` keeps a pie wedge around
/// local Y, `hollow_frac` erodes an inner shell), and run surface nets. A
/// group whose field never crosses zero (e.g. every element subtracts, or
/// the cuts removed everything) meshes as a small marker sphere so the prim
/// stays selectable in the editor instead of silently vanishing.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_blob_mesh(
    elements: &[BlobElement],
    resolution: u32,
    a0: f32,
    a1: f32,
    t0: f32,
    t1: f32,
    hollow_frac: f32,
) -> Mesh {
    use std::f32::consts::TAU;
    let resolved = resolve(elements);
    if resolved.is_empty() {
        return marker_mesh();
    }

    // Tight element AABB (no blend pad): the reference frame for the cut
    // semantics, so the slab band tracks the authored mass rather than the
    // blend head-room.
    let mut tight_lo = Vec3::splat(f32::INFINITY);
    let mut tight_hi = Vec3::splat(f32::NEG_INFINITY);
    for e in &resolved {
        let r = bound_radius(e);
        tight_lo = tight_lo.min(e.position - Vec3::splat(r));
        tight_hi = tight_hi.max(e.position + Vec3::splat(r));
    }

    // Conservative world AABB: element bounds + the largest blend bulge.
    let max_blend = resolved.iter().map(|e| e.blend).fold(0.0f32, f32::max);
    let lo = tight_lo - Vec3::splat(max_blend);
    let hi = tight_hi + Vec3::splat(max_blend);
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

    // Resolve the cuts (identity fast-paths keep the common case free).
    let full_sweep = (a1 - a0).abs() >= TAU - 1e-3;
    let (mid, half) = ((a0 + a1) * 0.5, (a1 - a0).abs() * 0.5);
    let (t0, t1) = (t0.clamp(0.0, 1.0), t1.clamp(0.0, 1.0));
    let slab = t0 > 1e-4 || t1 < 1.0 - 1e-4;
    let (y0, y1) = (
        tight_lo.y + (tight_hi.y - tight_lo.y) * t0,
        tight_lo.y + (tight_hi.y - tight_lo.y) * t1.max(t0 + 1e-3),
    );
    let k = hollow_frac.clamp(0.0, 0.95);
    // Shell wall: the un-bored fraction of the thinnest half-extent, floored
    // so surface nets can still resolve both faces at this grid's cell size.
    let wall = ((1.0 - k) * 0.5 * (tight_hi - tight_lo).min_element()).max(cell * 1.5);
    let hollow = k > 1e-4;

    // Sample the field. RuntimeShape linearizes in x-fastest order. The
    // shell erodes the *pre-cut* field so the slab / wedge faces slice
    // through it and expose the wall, rather than growing walls of their
    // own along the cut planes.
    let shape = RuntimeShape::<u32, 3>::new([dims.x, dims.y, dims.z]);
    let mut samples = vec![1.0f32; shape.size() as usize];
    for i in 0..shape.size() {
        let [x, y, z] = shape.delinearize(i);
        let p = origin + Vec3::new(x as f32, y as f32, z as f32) * cell;
        let d = group_sdf(&resolved, p);
        let mut v = d;
        if hollow {
            v = v.max(-(d + wall));
        }
        if slab {
            v = v.max((y0 - p.y).max(p.y - y1));
        }
        if !full_sweep {
            v = v.max(wedge_sdf(p, mid, half));
        }
        samples[i as usize] = v;
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
    use std::f32::consts::TAU;
    const HULL_RING: u32 = 8;
    let ring = |j: u32| {
        let (s, c) = (j as f32 / HULL_RING as f32 * TAU).sin_cos();
        (c, s)
    };
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
            // The box's corners ARE its hull.
            BlobShape::Box => {
                for d in DIRS[6..].iter() {
                    out.push(e.position + rot * (Vec3::from_array(*d).signum() * e.radii));
                }
            }
            BlobShape::Cylinder => {
                for end in [-1.0f32, 1.0] {
                    for j in 0..HULL_RING {
                        let (c, s) = ring(j);
                        out.push(
                            e.position
                                + rot * Vec3::new(c * e.radii.x, end * e.radii.y, s * e.radii.x),
                        );
                    }
                }
            }
            BlobShape::Cone => {
                out.push(e.position + rot * Vec3::new(0.0, e.radii.y, 0.0));
                for j in 0..HULL_RING {
                    let (c, s) = ring(j);
                    out.push(
                        e.position + rot * Vec3::new(c * e.radii.x, -e.radii.y, s * e.radii.x),
                    );
                }
            }
            // Outer equator + tube top/bottom rings bound the solid of
            // revolution's hull.
            BlobShape::Torus => {
                let (rr, tr) = (e.radii.x, e.radii.y);
                for j in 0..HULL_RING {
                    let (c, s) = ring(j);
                    out.push(e.position + rot * Vec3::new(c * (rr + tr), 0.0, s * (rr + tr)));
                    out.push(e.position + rot * Vec3::new(c * rr, tr, s * rr));
                    out.push(e.position + rot * Vec3::new(c * rr, -tr, s * rr));
                }
            }
        }
    }
    out
}
