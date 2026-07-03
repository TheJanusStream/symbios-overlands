//! SL-style cut meshers: the swept (path-cut / hollow) cylinder family, the
//! banded UV sphere, and the doubly-cut torus — everything a non-identity
//! `path_cut` / `profile_cut` / `hollow` routes to instead of a Bevy built-in.

use bevy::prelude::*;

use super::base::mesh_from_parts;

/// Convert a `[begin, end]` path-cut (turns) to a `(start, end)` angle pair.
pub(super) fn path_cut_angles(t: &crate::pds::TortureParams) -> (f32, f32) {
    use std::f32::consts::TAU;
    (t.path_cut.0[0] * TAU, t.path_cut.0[1] * TAU)
}

/// Unified swept-ring mesher — a straight cylinder that may be **hollow**
/// (`inner_frac > 0` → pipe / tube) and/or **angularly path-cut** (`a0..a1` <
/// full turn → trough / half-pipe / pie wedge, closed by two radial cut faces).
/// One generator backs `Cylinder` + `Tube` and all their SL-style cuts; taper /
/// twist / bend / shear ride on top via the vertex-torture post-pass. Winding is
/// reconciled to the supplied normals by [`mesh_from_parts`].
pub(super) fn build_swept_cylinder(
    r: f32,
    height: f32,
    resolution: u32,
    inner_frac: f32,
    a0: f32,
    a1: f32,
) -> Mesh {
    use std::f32::consts::TAU;
    let segs = resolution.max(3);
    let full = (a1 - a0).abs() >= TAU - 1e-3;
    let ri = (r * inner_frac).clamp(0.0, r * 0.999);
    let hollow = ri > 1e-4;
    let (yb, yt) = (-0.5 * height, 0.5 * height);
    let n = segs + 1;
    let ang = |i: u32| a0 + (a1 - a0) * (i as f32 / segs as f32);

    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut nor: Vec<[f32; 3]> = Vec::new();
    let mut uv: Vec<[f32; 2]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();

    // Outer wall, plus an inner wall when hollow (inward-facing normals).
    let mut walls = vec![(r, false)];
    if hollow {
        walls.push((ri, true));
    }
    for (radius, inward) in walls {
        let base = pos.len() as u32;
        let sgn = if inward { -1.0 } else { 1.0 };
        for i in 0..n {
            let a = ang(i);
            let (s, c) = a.sin_cos();
            let nrm = [sgn * c, 0.0, sgn * s];
            let u = i as f32 / segs as f32;
            pos.push([radius * c, yb, radius * s]);
            nor.push(nrm);
            uv.push([u, 1.0]);
            pos.push([radius * c, yt, radius * s]);
            nor.push(nrm);
            uv.push([u, 0.0]);
        }
        for i in 0..segs {
            let b = base + i * 2;
            idx.extend_from_slice(&[b, b + 2, b + 3, b, b + 3, b + 1]);
        }
    }

    // Top + bottom caps: annular when hollow, a triangle fan to the centre when
    // solid.
    for (y, ny) in [(yt, 1.0f32), (yb, -1.0f32)] {
        let nrm = [0.0, ny, 0.0];
        if hollow {
            let base = pos.len() as u32;
            let k = ri / r;
            for i in 0..n {
                let a = ang(i);
                let (s, c) = a.sin_cos();
                pos.push([r * c, y, r * s]);
                nor.push(nrm);
                uv.push([0.5 + 0.5 * c, 0.5 + 0.5 * s]);
                pos.push([ri * c, y, ri * s]);
                nor.push(nrm);
                uv.push([0.5 + 0.5 * k * c, 0.5 + 0.5 * k * s]);
            }
            for i in 0..segs {
                let b = base + i * 2;
                idx.extend_from_slice(&[b, b + 1, b + 3, b, b + 3, b + 2]);
            }
        } else {
            let base = pos.len() as u32;
            pos.push([0.0, y, 0.0]);
            nor.push(nrm);
            uv.push([0.5, 0.5]);
            for i in 0..n {
                let a = ang(i);
                let (s, c) = a.sin_cos();
                pos.push([r * c, y, r * s]);
                nor.push(nrm);
                uv.push([0.5 + 0.5 * c, 0.5 + 0.5 * s]);
            }
            for i in 0..segs {
                idx.extend_from_slice(&[base, base + 1 + i, base + 2 + i]);
            }
        }
    }

    // Radial cut faces close the wedge opening (only when path-cut).
    if !full {
        for (i, sgn) in [(0u32, -1.0f32), (segs, 1.0f32)] {
            let a = ang(i);
            let (s, c) = a.sin_cos();
            let nrm = [-sgn * s, 0.0, sgn * c];
            let rin = if hollow { ri } else { 0.0 };
            let base = pos.len() as u32;
            pos.push([rin * c, yb, rin * s]);
            pos.push([r * c, yb, r * s]);
            pos.push([r * c, yt, r * s]);
            pos.push([rin * c, yt, rin * s]);
            for _ in 0..4 {
                nor.push(nrm);
            }
            uv.extend_from_slice(&[[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
            idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    mesh_from_parts(pos, nor, uv, idx)
}

/// Unified revolved-sphere mesher — a UV sphere swept over a **latitude band**
/// (`lat_t0..lat_t1` in 0..1 → domes, bowls, dishes via profile-cut) and a
/// **longitude band** (`lon0..lon1` → orange slices / half-domes via path-cut),
/// optionally **hollow** (`inner_frac > 0` → a shell). Open latitude edges are
/// closed by horizontal disc / annulus caps; an open longitude wedge by two
/// meridional cut faces. Used for the cut Sphere; the plain icosphere stays on
/// Bevy. Winding is reconciled by [`mesh_from_parts`].
#[allow(clippy::too_many_arguments)]
pub(super) fn build_uv_sphere(
    radius: f32,
    resolution: u32,
    lon0: f32,
    lon1: f32,
    lat_t0: f32,
    lat_t1: f32,
    inner_frac: f32,
) -> Mesh {
    use std::f32::consts::{FRAC_PI_2, PI, TAU};
    let nlon = (resolution.max(2) * 6).max(8);
    let nlat = (resolution.max(2) * 4).max(6);
    let lon_full = (lon1 - lon0).abs() >= TAU - 1e-3;
    let ri_frac = inner_frac.clamp(0.0, 0.99);
    let hollow = ri_frac > 1e-4;
    let phi = |t: f32| -FRAC_PI_2 + t * PI;
    let latt = |j: u32| lat_t0 + (lat_t1 - lat_t0) * (j as f32 / nlat as f32);
    let lonf = |i: u32| lon0 + (lon1 - lon0) * (i as f32 / nlon as f32);
    let bottom_pole = lat_t0 <= 1e-4;
    let top_pole = lat_t1 >= 1.0 - 1e-4;

    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut nor: Vec<[f32; 3]> = Vec::new();
    let mut uv: Vec<[f32; 2]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();

    // Outer (+ inner, when hollow) revolved surface grid.
    let mut shells = vec![(radius, false)];
    if hollow {
        shells.push((radius * ri_frac, true));
    }
    for (rad, inward) in shells {
        let base = pos.len() as u32;
        let sgn = if inward { -1.0 } else { 1.0 };
        for j in 0..=nlat {
            let p = phi(latt(j));
            let (sp, cp) = p.sin_cos();
            for i in 0..=nlon {
                let l = lonf(i);
                let (sl, cl) = l.sin_cos();
                let d = [cp * cl, sp, cp * sl];
                pos.push([rad * d[0], rad * d[1], rad * d[2]]);
                nor.push([sgn * d[0], sgn * d[1], sgn * d[2]]);
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

    // Latitude caps (horizontal disc / annulus) at any open, non-pole edge.
    for (t, ny, pole) in [(lat_t0, -1.0f32, bottom_pole), (lat_t1, 1.0f32, top_pole)] {
        if pole {
            continue;
        }
        let p = phi(t);
        let (sp, cp) = p.sin_cos();
        let (y, rc) = (radius * sp, radius * cp);
        let nrm = [0.0, ny, 0.0];
        if hollow {
            // The rim joins the outer-shell edge to the *inner-shell* edge — both
            // at this latitude, so the inner edge sits at `radius * ri_frac * dir`
            // (its own Y), not at the outer Y. A flat annulus would leave the
            // inner edge floating; this conical band closes it. Normal is the
            // meridional tangent, facing out of the kept band.
            let base = pos.len() as u32;
            for i in 0..=nlon {
                let l = lonf(i);
                let (sl, cl) = l.sin_cos();
                let tang = [ny * -sp * cl, ny * cp, ny * -sp * sl];
                pos.push([radius * cp * cl, y, radius * cp * sl]);
                nor.push(tang);
                uv.push([0.5 + 0.5 * cl, 0.5 + 0.5 * sl]);
                pos.push([
                    radius * ri_frac * cp * cl,
                    radius * ri_frac * sp,
                    radius * ri_frac * cp * sl,
                ]);
                nor.push(tang);
                uv.push([0.5 + 0.5 * ri_frac * cl, 0.5 + 0.5 * ri_frac * sl]);
            }
            for i in 0..nlon {
                let b = base + i * 2;
                idx.extend_from_slice(&[b, b + 1, b + 3, b, b + 3, b + 2]);
            }
        } else {
            let base = pos.len() as u32;
            pos.push([0.0, y, 0.0]);
            nor.push(nrm);
            uv.push([0.5, 0.5]);
            for i in 0..=nlon {
                let l = lonf(i);
                let (sl, cl) = l.sin_cos();
                pos.push([rc * cl, y, rc * sl]);
                nor.push(nrm);
                uv.push([0.5 + 0.5 * cl, 0.5 + 0.5 * sl]);
            }
            for i in 0..nlon {
                idx.extend_from_slice(&[base, base + 1 + i, base + 2 + i]);
            }
        }
    }

    // Meridional cut faces when the longitude sweep is open (path-cut).
    if !lon_full {
        for (i_edge, sgn) in [(0u32, -1.0f32), (nlon, 1.0f32)] {
            let l = lonf(i_edge);
            let (sl, cl) = l.sin_cos();
            let nrm = [sgn * -sl, 0.0, sgn * cl];
            let base = pos.len() as u32;
            for j in 0..=nlat {
                let p = phi(latt(j));
                let (sp, cp) = p.sin_cos();
                let d = [cp * cl, sp, cp * sl];
                pos.push([radius * d[0], radius * d[1], radius * d[2]]);
                nor.push(nrm);
                uv.push([0.0, j as f32 / nlat as f32]);
                if hollow {
                    pos.push([
                        radius * ri_frac * d[0],
                        radius * ri_frac * d[1],
                        radius * ri_frac * d[2],
                    ]);
                } else {
                    pos.push([0.0, radius * sp, 0.0]);
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

/// Unified swept-torus mesher — a circular profile revolved along a major
/// circle, over a **major arc** (`maj0..maj1`, path-cut → arch / horseshoe /
/// open ring), a **minor arc** (`min0..min1`, profile-cut → C-channel / gutter),
/// optionally **hollow** (`inner_frac > 0` → a tubular shell). Open major ends
/// get cross-section caps (disc / annulus / fan); open minor edges get bands
/// running along the sweep. Used for the cut Torus; the plain torus stays on
/// Bevy. Winding is reconciled by [`mesh_from_parts`].
#[allow(clippy::too_many_arguments)]
pub(super) fn build_torus(
    major_r: f32,
    minor_r: f32,
    major_res: u32,
    minor_res: u32,
    maj0: f32,
    maj1: f32,
    min0: f32,
    min1: f32,
    inner_frac: f32,
) -> Mesh {
    use std::f32::consts::TAU;
    let nmaj = major_res.max(3);
    let nmin = minor_res.max(3);
    let maj_full = (maj1 - maj0).abs() >= TAU - 1e-3;
    let min_full = (min1 - min0).abs() >= TAU - 1e-3;
    let ri_frac = inner_frac.clamp(0.0, 0.99);
    let hollow = ri_frac > 1e-4;
    let majf = |i: u32| maj0 + (maj1 - maj0) * (i as f32 / nmaj as f32);
    let minf = |j: u32| min0 + (min1 - min0) * (j as f32 / nmin as f32);
    // Tube-surface point + outward normal for a given (major θ, minor radius, minor φ).
    let point = |th: f32, rad: f32, ph: f32| -> ([f32; 3], [f32; 3]) {
        let (st, ct) = th.sin_cos();
        let (sp, cp) = ph.sin_cos();
        let rr = major_r + rad * cp;
        ([rr * ct, rad * sp, rr * st], [cp * ct, sp, cp * st])
    };

    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut nor: Vec<[f32; 3]> = Vec::new();
    let mut uv: Vec<[f32; 2]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();

    // Outer (+ inner, when hollow) tube surface.
    let mut shells = vec![(minor_r, false)];
    if hollow {
        shells.push((minor_r * ri_frac, true));
    }
    for (rad, inward) in shells {
        let base = pos.len() as u32;
        let sgn = if inward { -1.0 } else { 1.0 };
        for i in 0..=nmaj {
            let th = majf(i);
            for j in 0..=nmin {
                let (p, no) = point(th, rad, minf(j));
                pos.push(p);
                nor.push([sgn * no[0], sgn * no[1], sgn * no[2]]);
                uv.push([i as f32 / nmaj as f32, j as f32 / nmin as f32]);
            }
        }
        let row = nmin + 1;
        for i in 0..nmaj {
            for j in 0..nmin {
                let a = base + i * row + j;
                idx.extend_from_slice(&[a, a + row, a + row + 1, a, a + row + 1, a + 1]);
            }
        }
    }

    // Major-arc end caps (the tube cross-section) when path-cut.
    if !maj_full {
        for (i_edge, sgn) in [(0u32, -1.0f32), (nmaj, 1.0f32)] {
            let th = majf(i_edge);
            let (st, ct) = th.sin_cos();
            let nrm = [sgn * -st, 0.0, sgn * ct];
            if hollow {
                let base = pos.len() as u32;
                for j in 0..=nmin {
                    let ph = minf(j);
                    pos.push(point(th, minor_r, ph).0);
                    nor.push(nrm);
                    uv.push([0.0, j as f32 / nmin as f32]);
                    pos.push(point(th, minor_r * ri_frac, ph).0);
                    nor.push(nrm);
                    uv.push([1.0, j as f32 / nmin as f32]);
                }
                for j in 0..nmin {
                    let b = base + j * 2;
                    idx.extend_from_slice(&[b, b + 1, b + 3, b, b + 3, b + 2]);
                }
            } else {
                let base = pos.len() as u32;
                pos.push([major_r * ct, 0.0, major_r * st]);
                nor.push(nrm);
                uv.push([0.5, 0.5]);
                for j in 0..=nmin {
                    pos.push(point(th, minor_r, minf(j)).0);
                    nor.push(nrm);
                    uv.push([0.5, j as f32 / nmin as f32]);
                }
                for j in 0..nmin {
                    idx.extend_from_slice(&[base, base + 1 + j, base + 2 + j]);
                }
            }
        }
    }

    // Minor-arc edge bands (the open lips of a C-channel) when profile-cut.
    if !min_full {
        for (j_edge, sgn) in [(0u32, -1.0f32), (nmin, 1.0f32)] {
            let ph = minf(j_edge);
            let (sp, cp) = ph.sin_cos();
            let base = pos.len() as u32;
            for i in 0..=nmaj {
                let th = majf(i);
                let (st, ct) = th.sin_cos();
                let nrm = [sgn * -sp * ct, sgn * cp, sgn * -sp * st];
                pos.push(point(th, minor_r, ph).0);
                nor.push(nrm);
                uv.push([i as f32 / nmaj as f32, 0.0]);
                if hollow {
                    pos.push(point(th, minor_r * ri_frac, ph).0);
                } else {
                    pos.push([major_r * ct, 0.0, major_r * st]);
                }
                nor.push(nrm);
                uv.push([i as f32 / nmaj as f32, 1.0]);
            }
            for i in 0..nmaj {
                let b = base + i * 2;
                idx.extend_from_slice(&[b, b + 1, b + 3, b, b + 3, b + 2]);
            }
        }
    }

    mesh_from_parts(pos, nor, uv, idx)
}
