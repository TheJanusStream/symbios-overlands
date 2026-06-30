//! Small vector helpers shared across the urban road builders.

/// Normalize a 2D (XZ) vector; a near-zero vector falls back to `+X` so callers
/// never propagate a NaN direction.
pub(crate) fn norm2(v: [f32; 2]) -> [f32; 2] {
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if l < 1.0e-6 {
        [1.0, 0.0]
    } else {
        [v[0] / l, v[1] / l]
    }
}

/// Upward-facing flat normal of triangle `(c, a, b)`.
pub(crate) fn tri_up_normal(c: [f32; 3], a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    let e1 = [a[0] - c[0], a[1] - c[1], a[2] - c[2]];
    let e2 = [b[0] - c[0], b[1] - c[1], b[2] - c[2]];
    let mut nn = cross(e1, e2);
    if nn[1] < 0.0 {
        nn = [-nn[0], -nn[1], -nn[2]];
    }
    normalize(nn)
}

pub(crate) fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn normalize(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt();
    if l < 1.0e-6 {
        [0.0, 1.0, 0.0]
    } else {
        [a[0] / l, a[1] / l, a[2] / l]
    }
}
