//! Spline-sweep meshers (#689): the Spine tube (circular profile swept along
//! a Catmull-Rom path with per-point radius) and the Lathe revolve (a
//! bottom-to-top silhouette spun around Y, with the SL-style path-cut /
//! hollow closures). Both interpolate with `bevy_math::cubic_splines`, so
//! the serialized recipe is just the control points.

use bevy::math::cubic_splines::{CubicCardinalSpline, CubicGenerator};
use bevy::prelude::*;

use super::base::mesh_from_parts;

/// One sampled station along a spine: centreline point, tube radius, the
/// parallel-transported frame, and the radius slope `dr/ds` that tilts the
/// surface normal on a tapering tube.
pub(super) struct SpineStation {
    pub pos: Vec3,
    pub radius: f32,
    pub tangent: Vec3,
    pub normal: Vec3,
    pub binormal: Vec3,
    pub slope: f32,
}

/// Sample a spine's Catmull-Rom curve (positions and radii ride one `Vec4`
/// spline) into parallel-transport-framed stations. Shared by the mesh
/// builder and the coarse collider-hull sampler so physics can never
/// diverge from the visible tube. Falls back to a straight 2-station rod
/// when the point list is degenerate (the sanitizer prevents that on any
/// networked record).
pub(super) fn spine_stations(
    points: &[(Vec3, f32)],
    samples_per_segment: u32,
) -> Vec<SpineStation> {
    let fallback = [
        (Vec3::new(0.0, -0.5, 0.0), 0.15),
        (Vec3::new(0.0, 0.5, 0.0), 0.15),
    ];
    let points = if points.len() >= 2 { points } else { &fallback };

    let ctrl: Vec<Vec4> = points.iter().map(|(p, r)| p.extend(r.max(0.005))).collect();
    let n_seg = (ctrl.len() - 1) as u32;
    let per = samples_per_segment.clamp(2, 64);
    let n_samples = n_seg * per;

    // The spline passes through every control point (mirrored endpoints);
    // ctrl.len() >= 2 makes to_curve infallible here.
    let curve = CubicCardinalSpline::new_catmull_rom(ctrl)
        .to_curve()
        .expect("spine spline needs 2+ points");

    // Sample positions / radii / tangents along the whole domain.
    let mut raw: Vec<(Vec3, f32, Vec3)> = Vec::with_capacity(n_samples as usize + 1);
    for i in 0..=n_samples {
        let t = i as f32 / per as f32;
        let v = curve.position(t);
        let vel = curve.velocity(t);
        raw.push((v.truncate(), v.w.max(0.005), vel.truncate()));
    }

    // Tangents: normalized velocity, falling back to the chord (then +Y) so
    // a coincident control point can't produce a zero frame.
    let chord = |i: usize| -> Vec3 {
        let next = raw.get(i + 1).map(|s| s.0).unwrap_or(raw[i].0);
        let prev = if i > 0 { raw[i - 1].0 } else { raw[i].0 };
        (next - prev).normalize_or_zero()
    };
    let tangents: Vec<Vec3> = (0..raw.len())
        .map(|i| {
            let t = raw[i].2.normalize_or_zero();
            if t != Vec3::ZERO {
                t
            } else {
                let c = chord(i);
                if c != Vec3::ZERO { c } else { Vec3::Y }
            }
        })
        .collect();

    // Parallel transport: seed a normal perpendicular to the first tangent,
    // then rotate it by the minimal rotation between consecutive tangents —
    // the rotation-minimizing frame that keeps the tube from spiralling.
    let seed_axis = if tangents[0].dot(Vec3::Y).abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let mut normal = seed_axis.cross(tangents[0]).normalize_or_zero();
    if normal == Vec3::ZERO {
        normal = Vec3::X;
    }

    let mut stations = Vec::with_capacity(raw.len());
    for i in 0..raw.len() {
        if i > 0 {
            let q = Quat::from_rotation_arc(tangents[i - 1], tangents[i]);
            normal = q * normal;
        }
        // Re-orthogonalize against drift.
        normal = (normal - tangents[i] * normal.dot(tangents[i])).normalize_or_zero();
        if normal == Vec3::ZERO {
            normal = tangents[i].any_orthonormal_vector();
        }
        let binormal = tangents[i].cross(normal).normalize_or_zero();

        // Radius slope vs arc length, from neighbours (central difference).
        let (p_prev, r_prev) = {
            let j = i.saturating_sub(1);
            (raw[j].0, raw[j].1)
        };
        let (p_next, r_next) = {
            let j = (i + 1).min(raw.len() - 1);
            (raw[j].0, raw[j].1)
        };
        let ds = (p_next - p_prev).length().max(1e-5);
        let slope = (r_next - r_prev) / ds;

        stations.push(SpineStation {
            pos: raw[i].0,
            radius: raw[i].1,
            tangent: tangents[i],
            normal,
            binormal,
            slope,
        });
    }
    stations
}

/// Build the Spine tube mesh: a circular profile of `resolution` segments
/// stitched over the [`spine_stations`] rings. SL-style cuts (#691):
/// `a0..a1` is the kept **angular range** of the ring (an open gutter /
/// half-pipe along the curve, closed by two edge strips), `t0..t1` trims
/// the kept **path range** (end caps move to the trimmed ends), and
/// `inner_frac > 0` hollows the tube into a shell (caps become annular).
/// Vertex torture rides on top via the shared post-pass.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_spine_mesh(
    points: &[(Vec3, f32)],
    resolution: u32,
    samples_per_segment: u32,
    a0: f32,
    a1: f32,
    t0: f32,
    t1: f32,
    inner_frac: f32,
) -> Mesh {
    use std::f32::consts::TAU;
    let res = resolution.clamp(3, 64);
    let full = (a1 - a0).abs() >= TAU - 1e-3;
    let k = inner_frac.clamp(0.0, 0.99);
    let hollow = k > 1e-4;
    let all = spine_stations(points, samples_per_segment);

    // Path trim: keep the stations inside the [t0, t1] arc-length band
    // (stations are dense — 2..64 per segment — so snapping to the nearest
    // station is visually exact).
    let mut full_arc = vec![0.0f32; all.len()];
    for i in 1..all.len() {
        full_arc[i] = full_arc[i - 1] + (all[i].pos - all[i - 1].pos).length();
    }
    let total = full_arc.last().copied().unwrap_or(0.0).max(1e-5);
    let (t0, t1) = (t0.clamp(0.0, 1.0), t1.clamp(0.0, 1.0).max(t0 + 1e-3));
    let i0 = full_arc.partition_point(|a| *a < t0 * total - 1e-6);
    let i1 = full_arc
        .partition_point(|a| *a <= t1 * total + 1e-6)
        .saturating_sub(1);
    let (i0, i1) = if i1 > i0 {
        (i0, i1)
    } else {
        (i0.min(all.len() - 2), i0.min(all.len() - 2) + 1)
    };
    let stations = &all[i0..=i1];
    let n_rings = stations.len() as u32;

    // V follows arc length scaled so a texel stays square against the
    // tube's mean circumference — a plain 0..1 V would compress a long
    // thin tube's texture into fine hoops (the trouser-stripe bug).
    let mut arc = vec![0.0f32; stations.len()];
    for i in 1..stations.len() {
        arc[i] = arc[i - 1] + (stations[i].pos - stations[i - 1].pos).length();
    }
    let mean_r = (stations.iter().map(|s| s.radius).sum::<f32>() / stations.len() as f32).max(1e-3);
    let v_of = |i: usize| arc[i] / (TAU * mean_r);
    let ang = |j: u32| a0 + (a1 - a0) * (j as f32 / res as f32);

    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut nor: Vec<[f32; 3]> = Vec::new();
    let mut uv: Vec<[f32; 2]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();

    // Tube surface grid — outer, plus an inner shell when hollow. The
    // surface normal of a tapering tube tilts along the tangent by the
    // radius slope (`dr/ds`), same as a cone's wall.
    let mut shells = vec![(1.0f32, false)];
    if hollow {
        shells.push((k, true));
    }
    for (scale, inward) in shells {
        let base = pos.len() as u32;
        let sgn = if inward { -1.0 } else { 1.0 };
        for (ri, st) in stations.iter().enumerate() {
            for j in 0..=res {
                let (sn, cs) = ang(j).sin_cos();
                let dir = st.normal * cs + st.binormal * sn;
                let p = st.pos + dir * (st.radius * scale);
                let n = (dir - st.tangent * (st.slope * scale)).normalize_or_zero() * sgn;
                pos.push(p.to_array());
                nor.push(n.to_array());
                uv.push([j as f32 / res as f32, v_of(ri)]);
            }
        }
        let row = res + 1;
        for ri in 0..n_rings - 1 {
            for j in 0..res {
                let a = base + ri * row + j;
                idx.extend_from_slice(&[a, a + row, a + row + 1, a, a + row + 1, a + 1]);
            }
        }
    }

    // End caps, normal along ∓tangent: a disc fan over the kept arc when
    // solid (a pie for an open ring — the centre lies on the cut plane), an
    // annular band outer → inner when hollow.
    for (st, sgn) in [
        (&stations[0], -1.0f32),
        (stations.last().expect("stations non-empty"), 1.0f32),
    ] {
        let nrm = (st.tangent * sgn).to_array();
        let base = pos.len() as u32;
        if hollow {
            for j in 0..=res {
                let (sn, cs) = ang(j).sin_cos();
                let dir = st.normal * cs + st.binormal * sn;
                pos.push((st.pos + dir * st.radius).to_array());
                nor.push(nrm);
                uv.push([0.5 + 0.5 * cs, 0.5 + 0.5 * sn]);
                pos.push((st.pos + dir * (st.radius * k)).to_array());
                nor.push(nrm);
                uv.push([0.5 + 0.5 * k * cs, 0.5 + 0.5 * k * sn]);
            }
            for j in 0..res {
                let b = base + j * 2;
                idx.extend_from_slice(&[b, b + 1, b + 3, b, b + 3, b + 2]);
            }
        } else {
            pos.push(st.pos.to_array());
            nor.push(nrm);
            uv.push([0.5, 0.5]);
            for j in 0..=res {
                let (sn, cs) = ang(j).sin_cos();
                let dir = st.normal * cs + st.binormal * sn;
                pos.push((st.pos + dir * st.radius).to_array());
                nor.push(nrm);
                uv.push([0.5 + 0.5 * cs, 0.5 + 0.5 * sn]);
            }
            for j in 0..res {
                idx.extend_from_slice(&[base, base + 1 + j, base + 2 + j]);
            }
        }
    }

    // Edge strips closing an open angular ring (path-cut): one strip per
    // cut angle, running the whole path from the outer surface to the axis
    // (solid) or the inner shell (hollow). The face normal is the ring
    // tangent at the cut angle.
    if !full {
        for (j_edge, sgn) in [(0u32, -1.0f32), (res, 1.0f32)] {
            let a = ang(j_edge);
            let (sn, cs) = a.sin_cos();
            let base = pos.len() as u32;
            for (ri, st) in stations.iter().enumerate() {
                let dir = st.normal * cs + st.binormal * sn;
                let edge_n = ((st.normal * -sn + st.binormal * cs) * sgn).normalize_or_zero();
                pos.push((st.pos + dir * st.radius).to_array());
                nor.push(edge_n.to_array());
                uv.push([0.0, v_of(ri)]);
                if hollow {
                    pos.push((st.pos + dir * (st.radius * k)).to_array());
                } else {
                    pos.push(st.pos.to_array());
                }
                nor.push(edge_n.to_array());
                uv.push([1.0, v_of(ri)]);
            }
            for ri in 0..n_rings - 1 {
                let b = base + ri * 2;
                idx.extend_from_slice(&[b, b + 1, b + 3, b, b + 3, b + 2]);
            }
        }
    }

    mesh_from_parts(pos, nor, uv, idx)
}

/// Coarse analytic point cloud for the Spine's convex-hull collider: a few
/// ring directions at every station of a low-rate resample. The hull fills
/// the concave side of a bent spine — the usual standoff trade, matching
/// every other tortured prim.
pub(super) fn spine_hull_points(points: &[(Vec3, f32)]) -> Vec<Vec3> {
    const HULL_DIRS: u32 = 6;
    use std::f32::consts::TAU;
    let stations = spine_stations(points, 3);
    let mut out = Vec::with_capacity(stations.len() * HULL_DIRS as usize);
    for st in &stations {
        for j in 0..HULL_DIRS {
            let a = j as f32 / HULL_DIRS as f32 * TAU;
            let (s, c) = a.sin_cos();
            out.push(st.pos + (st.normal * c + st.binormal * s) * st.radius);
        }
    }
    out
}

/// Resolve a lathe's profile points into meshing stations: the raw polyline
/// when `smooth` is off, or a Catmull-Rom resample through every station
/// when it's on. Radii are floored at zero (a spline overshoot below the
/// axis pinches to a pole instead of crossing it). Shared by the mesher and
/// the collider-hull sampler. Falls back to a unit cone profile on a
/// degenerate list (sanitize prevents that on networked records).
pub(super) fn lathe_stations(points: &[(f32, f32)], smooth: bool) -> Vec<Vec2> {
    let fallback = [(0.3, -0.5), (0.0, 0.5)];
    let points = if points.len() >= 2 {
        points
    } else {
        &fallback[..]
    };
    let ctrl: Vec<Vec2> = points.iter().map(|(r, h)| Vec2::new(*r, *h)).collect();

    let mut stations: Vec<Vec2> = if smooth {
        const PER_SEGMENT: u32 = 6;
        let n_samples = (ctrl.len() as u32 - 1) * PER_SEGMENT;
        let curve = CubicCardinalSpline::new_catmull_rom(ctrl)
            .to_curve()
            .expect("lathe spline needs 2+ points");
        (0..=n_samples)
            .map(|i| curve.position(i as f32 / PER_SEGMENT as f32))
            .collect()
    } else {
        ctrl
    };
    for s in stations.iter_mut() {
        s.x = s.x.max(0.0);
    }
    stations
}

/// Build the Lathe mesh: the profile stations revolved around Y over the
/// `a0..a1` angular range (path-cut), optionally **hollow** (`inner_frac >
/// 0` → a proportional inner shell). Open ends with a non-zero radius are
/// closed by disc / annulus caps; an open angular wedge by two flat cut
/// faces spanning profile → bore (or axis).
pub(super) fn build_lathe_mesh(
    points: &[(f32, f32)],
    resolution: u32,
    smooth: bool,
    inner_frac: f32,
    a0: f32,
    a1: f32,
) -> Mesh {
    use std::f32::consts::TAU;
    let segs = resolution.clamp(3, 128);
    let full = (a1 - a0).abs() >= TAU - 1e-3;
    let k = inner_frac.clamp(0.0, 0.99);
    let hollow = k > 1e-4;
    let stations = lathe_stations(points, smooth);
    let n_st = stations.len() as u32;
    let ang = |i: u32| a0 + (a1 - a0) * (i as f32 / segs as f32);

    // Per-station outward 2D profile normal `(radial, y)`: perpendicular to
    // the local profile tangent, averaged across neighbours so a smooth
    // silhouette shades smoothly.
    let prof_normal = |i: usize| -> Vec2 {
        let prev = stations[i.saturating_sub(1)];
        let next = stations[(i + 1).min(stations.len() - 1)];
        let tang = next - prev;
        let n = Vec2::new(tang.y, -tang.x).normalize_or_zero();
        if n == Vec2::ZERO { Vec2::X } else { n }
    };

    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut nor: Vec<[f32; 3]> = Vec::new();
    let mut uv: Vec<[f32; 2]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();

    // Outer (+ inner, when hollow) revolved surface. The inner shell scales
    // radii proportionally (same convention as the frustum bore), keeping
    // the profile normals reusable, flipped inward.
    let mut shells = vec![(1.0f32, false)];
    if hollow {
        shells.push((k, true));
    }
    for (scale, inward) in shells {
        let base = pos.len() as u32;
        let sgn = if inward { -1.0 } else { 1.0 };
        for (si, st) in stations.iter().enumerate() {
            let pn = prof_normal(si);
            for i in 0..=segs {
                let a = ang(i);
                let (s, c) = a.sin_cos();
                pos.push([scale * st.x * c, st.y, scale * st.x * s]);
                nor.push([sgn * pn.x * c, sgn * pn.y, sgn * pn.x * s]);
                uv.push([i as f32 / segs as f32, 1.0 - si as f32 / (n_st - 1) as f32]);
            }
        }
        let row = segs + 1;
        for si in 0..n_st - 1 {
            for i in 0..segs {
                let a = base + si * row + i;
                idx.extend_from_slice(&[a, a + row, a + row + 1, a, a + row + 1, a + 1]);
            }
        }
    }

    // End caps at any non-pole profile end: fan disc (solid) or annulus
    // (hollow).
    let last = stations.len() - 1;
    for (si, ny) in [(0usize, -1.0f32), (last, 1.0f32)] {
        let st = stations[si];
        if st.x <= 1e-4 {
            continue;
        }
        let nrm = [0.0, ny, 0.0];
        let base = pos.len() as u32;
        if hollow {
            for i in 0..=segs {
                let a = ang(i);
                let (s, c) = a.sin_cos();
                pos.push([st.x * c, st.y, st.x * s]);
                nor.push(nrm);
                uv.push([0.5 + 0.5 * c, 0.5 + 0.5 * s]);
                pos.push([k * st.x * c, st.y, k * st.x * s]);
                nor.push(nrm);
                uv.push([0.5 + 0.5 * k * c, 0.5 + 0.5 * k * s]);
            }
            for i in 0..segs {
                let b = base + i * 2;
                idx.extend_from_slice(&[b, b + 1, b + 3, b, b + 3, b + 2]);
            }
        } else {
            pos.push([0.0, st.y, 0.0]);
            nor.push(nrm);
            uv.push([0.5, 0.5]);
            for i in 0..=segs {
                let a = ang(i);
                let (s, c) = a.sin_cos();
                pos.push([st.x * c, st.y, st.x * s]);
                nor.push(nrm);
                uv.push([0.5 + 0.5 * c, 0.5 + 0.5 * s]);
            }
            for i in 0..segs {
                idx.extend_from_slice(&[base, base + 1 + i, base + 2 + i]);
            }
        }
    }

    // Flat cut faces when the sweep is angularly open (path-cut): one strip
    // per edge, spanning outer profile → bore (hollow) or axis (solid).
    if !full {
        for (i_edge, sgn) in [(0u32, -1.0f32), (segs, 1.0f32)] {
            let a = ang(i_edge);
            let (s, c) = a.sin_cos();
            let nrm = [sgn * -s, 0.0, sgn * c];
            let base = pos.len() as u32;
            for (si, st) in stations.iter().enumerate() {
                pos.push([st.x * c, st.y, st.x * s]);
                nor.push(nrm);
                uv.push([0.0, si as f32 / (n_st - 1) as f32]);
                if hollow {
                    pos.push([k * st.x * c, st.y, k * st.x * s]);
                } else {
                    pos.push([0.0, st.y, 0.0]);
                }
                nor.push(nrm);
                uv.push([1.0, si as f32 / (n_st - 1) as f32]);
            }
            for si in 0..n_st - 1 {
                let b = base + si * 2;
                idx.extend_from_slice(&[b, b + 1, b + 3, b, b + 3, b + 2]);
            }
        }
    }

    mesh_from_parts(pos, nor, uv, idx)
}

/// Coarse analytic point cloud for the Lathe's convex-hull collider: every
/// profile station revolved at a few angles. Concave silhouettes (a vase's
/// waist) hull-fill — the standard standoff trade.
pub(super) fn lathe_hull_points(points: &[(f32, f32)], smooth: bool) -> Vec<Vec3> {
    const HULL_DIRS: u32 = 8;
    use std::f32::consts::TAU;
    let stations = lathe_stations(points, smooth);
    let mut out = Vec::with_capacity(stations.len() * HULL_DIRS as usize);
    for st in &stations {
        for i in 0..HULL_DIRS {
            let a = i as f32 / HULL_DIRS as f32 * TAU;
            let (s, c) = a.sin_cos();
            out.push(Vec3::new(st.x * c, st.y, st.x * s));
        }
    }
    out
}
