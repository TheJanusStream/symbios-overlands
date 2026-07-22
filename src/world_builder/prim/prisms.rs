//! Hand-built prism / sweep meshers with no Bevy built-in: tube (hollow
//! cylinder), wedge, helix and bevelled cuboid, plus their quad/tri push
//! helpers.

use bevy::prelude::*;

use super::base::mesh_from_parts;

/// Hollow cylinder: outer + inner walls (smooth radial normals) closed by two
/// annular caps. `inner` is clamped just inside `outer` so the bore never
/// inverts.
pub(super) fn build_tube_mesh(outer: f32, inner: f32, height: f32, resolution: u32) -> Mesh {
    use std::f32::consts::TAU;
    let res = resolution.max(3) as usize;
    let inner = inner.clamp(0.0, outer * 0.999);
    let h2 = height * 0.5;

    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut nor: Vec<[f32; 3]> = Vec::new();
    let mut uv: Vec<[f32; 2]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();

    // Walls: outer normal points out (+1), inner points in (-1). UVs are
    // metres (#935) — each shell's U spans its own circumference, so the
    // bore's tighter wrap gets its own (shorter) run of texture rather than
    // the outer wall's stretched over it.
    for &(radius, sign) in &[(outer, 1.0f32), (inner, -1.0f32)] {
        let base = pos.len() as u32;
        let circumference = TAU * radius;
        for i in 0..=res {
            let a = i as f32 / res as f32 * TAU;
            let (s, c) = a.sin_cos();
            let n = [sign * c, 0.0, sign * s];
            let u = i as f32 / res as f32 * circumference;
            pos.push([radius * c, h2, radius * s]);
            nor.push(n);
            uv.push([u, height]);
            pos.push([radius * c, -h2, radius * s]);
            nor.push(n);
            uv.push([u, 0.0]);
        }
        for i in 0..res as u32 {
            let t0 = base + i * 2;
            let (b0, t1, b1) = (t0 + 1, t0 + 2, t0 + 3);
            idx.extend_from_slice(&[b0, b1, t1, b0, t1, t0]);
        }
    }

    // Annular caps: top (+Y) then bottom (-Y).
    for &(y, ny) in &[(h2, 1.0f32), (-h2, -1.0f32)] {
        let base = pos.len() as u32;
        for i in 0..=res {
            let a = i as f32 / res as f32 * TAU;
            let (s, c) = a.sin_cos();
            pos.push([outer * c, y, outer * s]);
            nor.push([0.0, ny, 0.0]);
            uv.push([outer * c, outer * s]);
            pos.push([inner * c, y, inner * s]);
            nor.push([0.0, ny, 0.0]);
            uv.push([inner * c, inner * s]);
        }
        for i in 0..res as u32 {
            let o0 = base + i * 2;
            let (in0, o1, in1) = (o0 + 1, o0 + 2, o0 + 3);
            idx.extend_from_slice(&[o0, o1, in1, o0, in1, in0]);
        }
    }

    mesh_from_parts(pos, nor, uv, idx)
}

/// Push one flat-shaded quad (4 corners, one normal); winding is reconciled by
/// [`mesh_from_parts`].
fn push_quad(
    pos: &mut Vec<[f32; 3]>,
    nor: &mut Vec<[f32; 3]>,
    uv: &mut Vec<[f32; 2]>,
    idx: &mut Vec<u32>,
    v: [[f32; 3]; 4],
    n: [f32; 3],
) {
    let base = pos.len() as u32;
    let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    for k in 0..4 {
        pos.push(v[k]);
        nor.push(n);
        uv.push(uvs[k]);
    }
    idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// Push one flat-shaded triangle (3 corners, one normal).
fn push_tri(
    pos: &mut Vec<[f32; 3]>,
    nor: &mut Vec<[f32; 3]>,
    uv: &mut Vec<[f32; 2]>,
    idx: &mut Vec<u32>,
    v: [[f32; 3]; 3],
    n: [f32; 3],
) {
    let base = pos.len() as u32;
    let uvs = [[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]];
    for k in 0..3 {
        pos.push(v[k]);
        nor.push(n);
        uv.push(uvs[k]);
    }
    idx.extend_from_slice(&[base, base + 1, base + 2]);
}

/// Right-triangular prism (ramp / roof pitch / buttress): a `size` bounding box
/// whose slope rises from the front-bottom (`+Z`, `-Y`) to the back-top (`-Z`,
/// `+Y`) across the full width (X). Five flat faces.
pub(super) fn build_wedge_mesh(size: [f32; 3]) -> Mesh {
    let (w, h, d) = (size[0] * 0.5, size[1] * 0.5, size[2] * 0.5);
    let bbl = [-w, -h, -d];
    let bfl = [-w, -h, d];
    let tbl = [-w, h, -d];
    let bbr = [w, -h, -d];
    let bfr = [w, -h, d];
    let tbr = [w, h, -d];
    let mut pos = Vec::new();
    let mut nor = Vec::new();
    let mut uv = Vec::new();
    let mut idx = Vec::new();
    push_quad(
        &mut pos,
        &mut nor,
        &mut uv,
        &mut idx,
        [bbl, bbr, bfr, bfl],
        [0.0, -1.0, 0.0],
    ); // bottom
    push_quad(
        &mut pos,
        &mut nor,
        &mut uv,
        &mut idx,
        [bbl, tbl, tbr, bbr],
        [0.0, 0.0, -1.0],
    ); // back
    let sl = (d * d + h * h).sqrt().max(1e-6);
    push_quad(
        &mut pos,
        &mut nor,
        &mut uv,
        &mut idx,
        [bfl, bfr, tbr, tbl],
        [0.0, d / sl, h / sl],
    ); // slope
    push_tri(
        &mut pos,
        &mut nor,
        &mut uv,
        &mut idx,
        [bbl, bfl, tbl],
        [-1.0, 0.0, 0.0],
    ); // left
    push_tri(
        &mut pos,
        &mut nor,
        &mut uv,
        &mut idx,
        [bbr, tbr, bfr],
        [1.0, 0.0, 0.0],
    ); // right
    mesh_from_parts(pos, nor, uv, idx)
}

/// Helical tube (spring / screw / spiral rail): a circular cross-section swept
/// along a helix. `pitch` is the rise per turn, `turns` the revolution count,
/// `resolution` the path segments per turn. SL-style cuts (#691): `p0..p1`
/// keeps a sub-range of the sweep (end caps move with it), `min0..min1` an
/// arc of the tube cross-section (a C-channel coiled along the helix, closed
/// by two edge lips), and `inner_frac > 0` hollows the tube into a shell.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_helix_mesh(
    radius: f32,
    tube_radius: f32,
    pitch: f32,
    turns: f32,
    resolution: u32,
    p0: f32,
    p1: f32,
    min0: f32,
    min1: f32,
    inner_frac: f32,
) -> Mesh {
    use std::f32::consts::TAU;
    let res = resolution.max(3);
    let turns = turns.abs().max(0.05);
    let (p0, p1) = (p0.clamp(0.0, 1.0), p1.clamp(0.0, 1.0).max(p0 + 1e-3));
    let min_full = (min1 - min0).abs() >= TAU - 1e-3;
    let k = inner_frac.clamp(0.0, 0.99);
    let hollow = k > 1e-4;
    let path_segs = ((turns * (p1 - p0) * res as f32).ceil() as u32).max(2);
    let tube_segs = 10u32;
    let total_h = turns * pitch;
    let path = |i: u32| -> (Vec3, Vec3) {
        let th = turns * TAU * (p0 + (p1 - p0) * (i as f32 / path_segs as f32));
        let (st, ct) = th.sin_cos();
        let c = Vec3::new(radius * ct, (th / TAU) * pitch - total_h * 0.5, radius * st);
        let mut t = Vec3::new(-radius * st, pitch / TAU, radius * ct).normalize_or_zero();
        if t == Vec3::ZERO {
            t = Vec3::Y;
        }
        (c, t)
    };
    let frame = |t: Vec3| -> (Vec3, Vec3) {
        let refv = if t.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
        let n = refv.cross(t).normalize_or_zero();
        (n, t.cross(n))
    };
    let phi_of = |j: u32| min0 + (min1 - min0) * (j as f32 / tube_segs as f32);
    // Metre convention (#938): U is arc length travelled along the coil, V
    // arc length around the tube's cross-section. One turn of a helix
    // covers `sqrt(circumference² + pitch²)` — the hypotenuse, not the
    // circumference, or a steeply pitched spring would wear its texture
    // squashed along the rise.
    let turn_len = ((TAU * radius).powi(2) + pitch * pitch).sqrt();
    let path_len = turns * (p1 - p0) * turn_len;
    let tube_arc = (min1 - min0).abs() * tube_radius;

    let mut pos = Vec::new();
    let mut nor = Vec::new();
    let mut uv = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    // Outer tube (+ inner shell when hollow).
    let mut shells = vec![(tube_radius, false)];
    if hollow {
        shells.push((tube_radius * k, true));
    }
    for (rad, inward) in shells {
        let base = pos.len() as u32;
        let sgn = if inward { -1.0 } else { 1.0 };
        for i in 0..=path_segs {
            let (c, t) = path(i);
            let (n, b) = frame(t);
            for j in 0..=tube_segs {
                let (sp, cp) = phi_of(j).sin_cos();
                let dir = n * cp + b * sp;
                pos.push((c + dir * rad).to_array());
                nor.push((dir * sgn).to_array());
                uv.push([
                    path_len * (i as f32 / path_segs as f32),
                    tube_arc * (j as f32 / tube_segs as f32),
                ]);
            }
        }
        let row = tube_segs + 1;
        for i in 0..path_segs {
            for j in 0..tube_segs {
                let a = base + i * row + j;
                idx.extend_from_slice(&[a, a + row, a + row + 1, a, a + row + 1, a + 1]);
            }
        }
    }
    // Edge lips closing an open cross-section arc (profile-cut), running the
    // whole path.
    if !min_full {
        for (j_edge, sgn) in [(0u32, -1.0f32), (tube_segs, 1.0f32)] {
            let (sp, cp) = phi_of(j_edge).sin_cos();
            let base = pos.len() as u32;
            for i in 0..=path_segs {
                let (c, t) = path(i);
                let (n, b) = frame(t);
                let dir = n * cp + b * sp;
                let lip_n = ((n * -sp + b * cp) * sgn).normalize_or_zero();
                pos.push((c + dir * tube_radius).to_array());
                nor.push(lip_n.to_array());
                uv.push([path_len * (i as f32 / path_segs as f32), tube_radius]);
                if hollow {
                    pos.push((c + dir * (tube_radius * k)).to_array());
                } else {
                    pos.push(c.to_array());
                }
                nor.push(lip_n.to_array());
                uv.push([
                    path_len * (i as f32 / path_segs as f32),
                    if hollow { tube_radius * k } else { 0.0 },
                ]);
            }
            for i in 0..path_segs {
                let b = base + i * 2;
                idx.extend_from_slice(&[b, b + 1, b + 3, b, b + 3, b + 2]);
            }
        }
    }
    // End caps: a fan over the kept arc when solid, an annular band when
    // hollow.
    for (i_edge, sgn) in [(0u32, -1.0f32), (path_segs, 1.0f32)] {
        let (c, t) = path(i_edge);
        let (n, b) = frame(t);
        let ncap = (t * sgn).to_array();
        let basec = pos.len() as u32;
        if hollow {
            for j in 0..=tube_segs {
                let (sp, cp) = phi_of(j).sin_cos();
                let dir = n * cp + b * sp;
                pos.push((c + dir * tube_radius).to_array());
                nor.push(ncap);
                uv.push([tube_radius * cp, tube_radius * sp]);
                pos.push((c + dir * (tube_radius * k)).to_array());
                nor.push(ncap);
                uv.push([tube_radius * k * cp, tube_radius * k * sp]);
            }
            for j in 0..tube_segs {
                let b2 = basec + j * 2;
                idx.extend_from_slice(&[b2, b2 + 1, b2 + 3, b2, b2 + 3, b2 + 2]);
            }
            continue;
        }
        pos.push(c.to_array());
        nor.push(ncap);
        uv.push([0.0, 0.0]);
        for j in 0..=tube_segs {
            let (sp, cp) = phi_of(j).sin_cos();
            let dir = n * cp + b * sp;
            pos.push((c + dir * tube_radius).to_array());
            nor.push(ncap);
            uv.push([tube_radius * cp, tube_radius * sp]);
        }
        for j in 0..tube_segs {
            idx.extend_from_slice(&[basec, basec + 1 + j, basec + 2 + j]);
        }
    }
    mesh_from_parts(pos, nor, uv, idx)
}

/// Box with chamfered / rounded vertical edges — an extruded rounded-rectangle
/// prism. `bevel` is the corner radius (clamped inside the footprint);
/// `segments` is `1` for a flat chamfer (octagonal prism), higher for a
/// rounded corner. Side normals follow the profile (smooth on arcs, flat on
/// the straight runs); caps are flat fans.
pub(super) fn build_bevel_mesh(size: [f32; 3], bevel: f32, segments: u32) -> Mesh {
    use std::f32::consts::{FRAC_PI_2, PI};
    let [sx, sy, sz] = size;
    let (hx, hy, hz) = (sx * 0.5, sy * 0.5, sz * 0.5);
    let b = bevel.clamp(0.0, (hx.min(hz) - 1e-3).max(0.0));
    let seg = segments.max(1) as usize;

    // Rounded-rectangle profile (XZ), CCW: four corner arcs whose endpoints
    // meet the straight edges. Each entry is (x, z, outward nx, outward nz).
    let centers = [
        (hx - b, hz - b, 0.0f32),
        (-(hx - b), hz - b, FRAC_PI_2),
        (-(hx - b), -(hz - b), PI),
        (hx - b, -(hz - b), 3.0 * FRAC_PI_2),
    ];
    let mut profile: Vec<(f32, f32, f32, f32)> = Vec::new();
    for (cx, cz, a0) in centers {
        for k in 0..=seg {
            let a = a0 + (k as f32 / seg as f32) * FRAC_PI_2;
            let (s, c) = a.sin_cos();
            profile.push((cx + b * c, cz + b * s, c, s));
        }
    }
    let n = profile.len();

    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut nor: Vec<[f32; 3]> = Vec::new();
    let mut uv: Vec<[f32; 2]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();

    // Side wall.
    for (i, &(x, z, nx, nz)) in profile.iter().enumerate() {
        let u = i as f32 / n as f32;
        pos.push([x, hy, z]);
        nor.push([nx, 0.0, nz]);
        uv.push([u, 1.0]);
        pos.push([x, -hy, z]);
        nor.push([nx, 0.0, nz]);
        uv.push([u, 0.0]);
    }
    for i in 0..n {
        let i1 = (i + 1) % n;
        let t0 = (i as u32) * 2;
        let b0 = t0 + 1;
        let t1 = (i1 as u32) * 2;
        let b1 = t1 + 1;
        idx.extend_from_slice(&[b0, b1, t1, b0, t1, t0]);
    }

    // Caps: a centre-fan over the profile, top (+Y) then bottom (-Y).
    let inv_hx = 1.0 / hx.max(1e-3);
    let inv_hz = 1.0 / hz.max(1e-3);
    for &(y, ny) in &[(hy, 1.0f32), (-hy, -1.0f32)] {
        let center = pos.len() as u32;
        pos.push([0.0, y, 0.0]);
        nor.push([0.0, ny, 0.0]);
        uv.push([0.5, 0.5]);
        let rim = pos.len() as u32;
        for &(x, z, _, _) in &profile {
            pos.push([x, y, z]);
            nor.push([0.0, ny, 0.0]);
            uv.push([0.5 + 0.5 * x * inv_hx, 0.5 + 0.5 * z * inv_hz]);
        }
        for i in 0..n as u32 {
            let i1 = (i + 1) % n as u32;
            idx.extend_from_slice(&[center, rim + i, rim + i1]);
        }
    }

    mesh_from_parts(pos, nor, uv, idx)
}
