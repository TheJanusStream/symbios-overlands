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

/// Surface point + analytic normal at latitude `eta`, longitude `omega`.
/// The normal swaps each exponent for `2 − e` and divides by the axis scale
/// — exact everywhere except the poles/creases, where the radial direction
/// is a stable stand-in.
fn surface(half_extents: [f32; 3], e1: f32, e2: f32, eta: f32, omega: f32) -> (Vec3, Vec3) {
    let [ax, ay, az] = half_extents;
    let (se, ce) = eta.sin_cos();
    let (so, co) = omega.sin_cos();
    let p = Vec3::new(
        ax * spow(ce, e1) * spow(co, e2),
        ay * spow(se, e1),
        az * spow(ce, e1) * spow(so, e2),
    );
    let n = Vec3::new(
        spow(ce, 2.0 - e1) * spow(co, 2.0 - e2) / ax,
        spow(se, 2.0 - e1) / ay,
        spow(ce, 2.0 - e1) * spow(so, 2.0 - e2) / az,
    )
    .normalize_or_zero();
    let n = if n == Vec3::ZERO {
        Vec3::new(ce * co, se, ce * so)
    } else {
        n
    };
    (p, n)
}

/// Build a superellipsoid mesh: `half_extents` scale the three axes,
/// `exponent_ns` shapes the north–south (latitude) profile, `exponent_ew`
/// the east–west (longitude) cross-section. `latitudes` / `longitudes` are
/// ring / segment counts (the UV-sphere convention).
///
/// Surface (Y up): for latitude `η ∈ [-π/2, π/2]` and longitude
/// `ω ∈ [0, 2π)`,
/// `p = (ax·c(η,e1)·c(ω,e2), ay·s(η,e1), az·c(η,e1)·s(ω,e2))`
/// with `c(θ,e) = sign(cos θ)|cos θ|^e` (and `s` likewise). SL-style cuts
/// mirror the banded sphere's: `t0..t1` keeps a **latitude band** (each
/// ring is planar — constant `y` — so open ends close with flat discs, or
/// rim bands to the inner shell when **hollow**), `lon0..lon1` keeps a
/// **longitude wedge** (closed by two meridional cut faces), and `hollow`
/// adds a uniformly-scaled inner shell (a scaled superellipsoid keeps its
/// normal directions). Pole rings collapse to points exactly like the UV
/// sphere's, which the winding pass tolerates.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_superellipsoid(
    half_extents: [f32; 3],
    exponent_ns: f32,
    exponent_ew: f32,
    latitudes: u32,
    longitudes: u32,
    lon0: f32,
    lon1: f32,
    t0: f32,
    t1: f32,
    inner_frac: f32,
) -> Mesh {
    use std::f32::consts::{FRAC_PI_2, PI, TAU};
    let (e1, e2) = (exponent_ns, exponent_ew);
    let nlat = latitudes.max(4);
    let nlon = longitudes.max(4);
    let lon_full = (lon1 - lon0).abs() >= TAU - 1e-3;
    let k = inner_frac.clamp(0.0, 0.99);
    let hollow = k > 1e-4;
    let (t0, t1) = (t0.clamp(0.0, 1.0), t1.clamp(0.0, 1.0).max(t0 + 1e-3));
    let bottom_pole = t0 <= 1e-4;
    let top_pole = t1 >= 1.0 - 1e-4;
    let eta_of = |j: u32| -FRAC_PI_2 + (t0 + (t1 - t0) * (j as f32 / nlat as f32)) * PI;
    let lon_of = |i: u32| lon0 + (lon1 - lon0) * (i as f32 / nlon as f32);

    let mut pos: Vec<[f32; 3]> = Vec::with_capacity(((nlat + 1) * (nlon + 1)) as usize);
    let mut nor: Vec<[f32; 3]> = Vec::with_capacity(pos.capacity());
    let mut uv: Vec<[f32; 2]> = Vec::with_capacity(pos.capacity());
    let mut idx: Vec<u32> = Vec::new();

    // Outer (+ inner, when hollow) surface grid.
    let mut shells = vec![(1.0f32, false)];
    if hollow {
        shells.push((k, true));
    }
    for (scale, inward) in shells {
        let base = pos.len() as u32;
        let sgn = if inward { -1.0 } else { 1.0 };
        for j in 0..=nlat {
            let eta = eta_of(j);
            for i in 0..=nlon {
                let (p, n) = surface(half_extents, e1, e2, eta, lon_of(i));
                pos.push((p * scale).to_array());
                nor.push((n * sgn).to_array());
                uv.push([i as f32 / nlon as f32, 1.0 - j as f32 / nlat as f32]);
            }
        }
        let row = nlon + 1;
        for j in 0..nlat {
            for i in 0..nlon {
                let a = base + j * row + i;
                idx.extend_from_slice(&[a, a + row, a + row + 1, a, a + row + 1, a + 1]);
            }
        }
    }

    // Latitude caps at any open, non-pole edge. Each latitude ring lies in
    // a horizontal plane, so a solid end closes with a flat disc fan; a
    // hollow end with a rim band whose normal is the meridional tangent
    // (numeric — the analytic tangent has the same crease caveats as the
    // normal).
    for (t_edge, ny, pole) in [(t0, -1.0f32, bottom_pole), (t1, 1.0f32, top_pole)] {
        if pole {
            continue;
        }
        let eta = -FRAC_PI_2 + t_edge * PI;
        if hollow {
            let base = pos.len() as u32;
            const EPS: f32 = 1e-3;
            for i in 0..=nlon {
                let om = lon_of(i);
                let (p, _) = surface(half_extents, e1, e2, eta, om);
                let (p_d, _) = surface(half_extents, e1, e2, eta + ny * EPS, om);
                let tang = ((p_d - p) * ny).normalize_or_zero() * ny;
                let tang = if tang == Vec3::ZERO {
                    Vec3::new(0.0, ny, 0.0)
                } else {
                    tang
                };
                pos.push(p.to_array());
                nor.push(tang.to_array());
                uv.push([i as f32 / nlon as f32, 0.0]);
                pos.push((p * k).to_array());
                nor.push(tang.to_array());
                uv.push([i as f32 / nlon as f32, 1.0]);
            }
            for i in 0..nlon {
                let b = base + i * 2;
                idx.extend_from_slice(&[b, b + 1, b + 3, b, b + 3, b + 2]);
            }
        } else {
            let base = pos.len() as u32;
            let (p_first, _) = surface(half_extents, e1, e2, eta, lon_of(0));
            let nrm = [0.0, ny, 0.0];
            pos.push([0.0, p_first.y, 0.0]);
            nor.push(nrm);
            uv.push([0.5, 0.5]);
            for i in 0..=nlon {
                let (p, _) = surface(half_extents, e1, e2, eta, lon_of(i));
                pos.push(p.to_array());
                nor.push(nrm);
                uv.push([0.5 + 0.5 * (i as f32 / nlon as f32) - 0.25, 0.5]);
            }
            for i in 0..nlon {
                idx.extend_from_slice(&[base, base + 1 + i, base + 2 + i]);
            }
        }
    }

    // Meridional cut faces when the longitude sweep is open (path-cut):
    // strips from the outer surface to the axis (solid) or the inner shell
    // (hollow).
    if !lon_full {
        for (i_edge, sgn) in [(0u32, -1.0f32), (nlon, 1.0f32)] {
            let om = lon_of(i_edge);
            let (so, co) = om.sin_cos();
            let nrm = [sgn * -so, 0.0, sgn * co];
            let base = pos.len() as u32;
            for j in 0..=nlat {
                let (p, _) = surface(half_extents, e1, e2, eta_of(j), om);
                pos.push(p.to_array());
                nor.push(nrm);
                uv.push([0.0, j as f32 / nlat as f32]);
                if hollow {
                    pos.push((p * k).to_array());
                } else {
                    pos.push([0.0, p.y, 0.0]);
                }
                nor.push(nrm);
                uv.push([1.0, j as f32 / nlat as f32]);
            }
            for j in 0..nlat {
                let b = base + j * 2;
                idx.extend_from_slice(&[b, b + 1, b + 3, b, b + 3, b + 2]);
            }
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
        for i in 0..HULL_LON {
            let omega = (i as f32 / HULL_LON as f32) * TAU;
            points.push(surface([ax, ay, az], e1, e2, eta, omega).0);
        }
    }
    points
}
