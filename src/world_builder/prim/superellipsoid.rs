//! Barr superellipsoid mesher — one prim that morphs continuously from box
//! (small exponents) through pillow / sphere (`1.0`) toward a pinched
//! octahedral form (large exponents), per axis pair. The two exponents are
//! the whole shape interface (POV-Ray ships the same surface as
//! `superellipsoid { <e, n> }`); the sanitiser clamps them to `0.2..=2.5`
//! because the signed-power parametrisation degenerates numerically outside
//! that band (normals blow up along the creases as an exponent → 0).

use bevy::prelude::*;

use super::base::mesh_from_parts;

/// Sign-preserving power: `sign(v) * |v|^e`. The superellipsoid's
/// parametrisation and its analytic normal are both built from this.
fn spow(v: f32, e: f32) -> f32 {
    v.signum() * v.abs().powf(e)
}

/// Build a superellipsoid mesh: `half_extents` scale the three axes,
/// `exponent_ns` shapes the north–south (latitude) profile, `exponent_ew`
/// the east–west (longitude) cross-section. `latitudes` / `longitudes` are
/// ring / segment counts (the UV-sphere convention).
///
/// Surface (Y up): for latitude `η ∈ [-π/2, π/2]` and longitude
/// `ω ∈ [0, 2π)`,
/// `p = (ax·c(η,e1)·c(ω,e2), ay·s(η,e1), az·c(η,e1)·s(ω,e2))`
/// with `c(θ,e) = sign(cos θ)|cos θ|^e` (and `s` likewise). The analytic
/// normal swaps each exponent for `2 − e` and divides by the axis scale —
/// exact everywhere except the poles/creases, where the numeric fallback of
/// a mesh recompute would be far worse; the pole rings collapse to points
/// exactly like the UV sphere's, which the winding pass tolerates.
pub(super) fn build_superellipsoid(
    half_extents: [f32; 3],
    exponent_ns: f32,
    exponent_ew: f32,
    latitudes: u32,
    longitudes: u32,
) -> Mesh {
    use std::f32::consts::{FRAC_PI_2, TAU};
    let [ax, ay, az] = half_extents;
    let (e1, e2) = (exponent_ns, exponent_ew);
    let nlat = latitudes.max(4);
    let nlon = longitudes.max(4);

    let mut pos: Vec<[f32; 3]> = Vec::with_capacity(((nlat + 1) * (nlon + 1)) as usize);
    let mut nor: Vec<[f32; 3]> = Vec::with_capacity(pos.capacity());
    let mut uv: Vec<[f32; 2]> = Vec::with_capacity(pos.capacity());
    let mut idx: Vec<u32> = Vec::new();

    for j in 0..=nlat {
        let eta = -FRAC_PI_2 + (j as f32 / nlat as f32) * (FRAC_PI_2 * 2.0);
        let (se, ce) = eta.sin_cos();
        for i in 0..=nlon {
            let omega = (i as f32 / nlon as f32) * TAU;
            let (so, co) = omega.sin_cos();
            pos.push([
                ax * spow(ce, e1) * spow(co, e2),
                ay * spow(se, e1),
                az * spow(ce, e1) * spow(so, e2),
            ]);
            let n = Vec3::new(
                spow(ce, 2.0 - e1) * spow(co, 2.0 - e2) / ax,
                spow(se, 2.0 - e1) / ay,
                spow(ce, 2.0 - e1) * spow(so, 2.0 - e2) / az,
            )
            .normalize_or_zero();
            // A crease vertex (exact pole with e1 > 2, say) can zero the
            // analytic normal; the radial direction is a stable stand-in.
            let n = if n == Vec3::ZERO {
                Vec3::new(ce * co, se, ce * so)
            } else {
                n
            };
            nor.push(n.to_array());
            uv.push([i as f32 / nlon as f32, 1.0 - j as f32 / nlat as f32]);
        }
    }
    let row = nlon + 1;
    for j in 0..nlat {
        for i in 0..nlon {
            let a = j * row + i;
            idx.extend_from_slice(&[a, a + row, a + row + 1, a, a + row + 1, a + 1]);
        }
    }

    mesh_from_parts(pos, nor, uv, idx)
}

/// Analytic surface point cloud for the convex-hull collider — a coarse
/// sampling of the same parametrisation (the shape is convex for exponents
/// `≤ 2`, and the mild over-hull past that matches the tortured-prim
/// precedent of standoff-over-fidelity).
pub(super) fn superellipsoid_hull_points(
    half_extents: [f32; 3],
    exponent_ns: f32,
    exponent_ew: f32,
) -> Vec<Vec3> {
    use std::f32::consts::{FRAC_PI_2, TAU};
    let [ax, ay, az] = half_extents;
    let (e1, e2) = (exponent_ns, exponent_ew);
    const HULL_LAT: u32 = 8;
    const HULL_LON: u32 = 12;
    let mut points = Vec::with_capacity(((HULL_LAT + 1) * HULL_LON) as usize);
    for j in 0..=HULL_LAT {
        let eta = -FRAC_PI_2 + (j as f32 / HULL_LAT as f32) * (FRAC_PI_2 * 2.0);
        let (se, ce) = eta.sin_cos();
        for i in 0..HULL_LON {
            let omega = (i as f32 / HULL_LON as f32) * TAU;
            let (so, co) = omega.sin_cos();
            points.push(Vec3::new(
                ax * spow(ce, e1) * spow(co, e2),
                ay * spow(se, e1),
                az * spow(ce, e1) * spow(so, e2),
            ));
        }
    }
    points
}
