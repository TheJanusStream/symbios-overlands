//! The distances and crossings `--floating-report` measures parts by: a
//! triangle's nearest point, two segments' nearest points, a segment
//! through a triangle, and a ray's first hit.

use bevy::prelude::*;

/// One triangle as the world draws it, and the box round it.
pub(super) struct Tri {
    pub(super) v: [Vec3; 3],
    pub(super) min: Vec3,
    pub(super) max: Vec3,
}

impl Tri {
    pub(super) fn new(v: [Vec3; 3]) -> Self {
        Self {
            v,
            min: v[0].min(v[1]).min(v[2]),
            max: v[0].max(v[1]).max(v[2]),
        }
    }

    pub(super) fn centre(&self) -> Vec3 {
        (self.v[0] + self.v[1] + self.v[2]) / 3.0
    }
}

pub(super) fn boxes_meet(a_min: Vec3, a_max: Vec3, b_min: Vec3, b_max: Vec3) -> bool {
    a_min.cmple(b_max).all() && b_min.cmple(a_max).all()
}

/// How far `p` is from the box `min..max` (0 inside it).
pub(super) fn box_distance(p: Vec3, min: Vec3, max: Vec3) -> f32 {
    (min - p).max(p - max).max(Vec3::ZERO).length()
}

/// How far apart two boxes are (0 where they meet).
pub(super) fn boxes_distance(a_min: Vec3, a_max: Vec3, b_min: Vec3, b_max: Vec3) -> f32 {
    (b_min - a_max).max(a_min - b_max).max(Vec3::ZERO).length()
}

/// Whether two triangles come within `tol` of each other: a corner of
/// one near the other, an edge of each near an edge of the other, or an
/// edge of one through the other.
pub(super) fn tris_within(a: &Tri, b: &Tri, tol: f32) -> bool {
    let tol_sq = tol * tol;
    let near_corner = |p: &Vec3, t: &Tri| (*p - closest_on_tri(*p, t)).length_squared() <= tol_sq;
    if a.v.iter().any(|p| near_corner(p, b)) || b.v.iter().any(|p| near_corner(p, a)) {
        return true;
    }
    let edges = |t: &Tri| [(t.v[0], t.v[1]), (t.v[1], t.v[2]), (t.v[2], t.v[0])];
    let (ea, eb) = (edges(a), edges(b));
    if ea.iter().any(|&(p, q)| {
        eb.iter()
            .any(|&(r, s)| segment_distance_sq(p, q, r, s) <= tol_sq)
    }) {
        return true;
    }
    ea.iter().any(|&(p, q)| segment_crosses(p, q, b))
        || eb.iter().any(|&(p, q)| segment_crosses(p, q, a))
}

/// The point of `t` nearest `p` (Ericson, Real-Time Collision Detection
/// 5.1.5).
pub(super) fn closest_on_tri(p: Vec3, t: &Tri) -> Vec3 {
    let [a, b, c] = t.v;
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let sum = va + vb + vc;
    if sum.abs() <= f32::EPSILON {
        return a;
    }
    a + ab * (vb / sum) + ac * (vc / sum)
}

/// The squared distance between the segments `p1..q1` and `p2..q2`
/// (Ericson 5.1.9).
pub(super) fn segment_distance_sq(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> f32 {
    const EPS: f32 = 1.0e-12;
    let d1 = q1 - p1;
    let d2 = q2 - p2;
    let r = p1 - p2;
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);
    let (s, t) = if a <= EPS && e <= EPS {
        (0.0, 0.0)
    } else if a <= EPS {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = d1.dot(r);
        if e <= EPS {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let s = if denom > EPS {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let t = (b * s + f) / e;
            if t < 0.0 {
                ((-c / a).clamp(0.0, 1.0), 0.0)
            } else if t > 1.0 {
                (((b - c) / a).clamp(0.0, 1.0), 1.0)
            } else {
                (s, t)
            }
        }
    };
    ((p1 + d1 * s) - (p2 + d2 * t)).length_squared()
}

/// How far along `dir` from `origin` the ray meets `t`, in lengths of
/// `dir` (Moller-Trumbore), if it meets it ahead of `origin`.
pub(super) fn ray_hit(origin: Vec3, dir: Vec3, t: &Tri) -> Option<f32> {
    let e1 = t.v[1] - t.v[0];
    let e2 = t.v[2] - t.v[0];
    let h = dir.cross(e2);
    let det = e1.dot(h);
    if det.abs() < 1.0e-12 {
        return None;
    }
    let s = origin - t.v[0];
    let u = s.dot(h) / det;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) / det;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let along = e2.dot(q) / det;
    (along > 0.0).then_some(along)
}

/// Whether the segment `p..q` passes through `t`.
fn segment_crosses(p: Vec3, q: Vec3, t: &Tri) -> bool {
    ray_hit(p, q - p, t).is_some_and(|along| along <= 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat() -> Tri {
        Tri::new([
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ])
    }

    #[test]
    fn a_point_over_a_triangle_is_its_height_away() {
        let t = flat();
        let p = Vec3::new(0.2, 0.5, 0.2);
        assert!((closest_on_tri(p, &t) - Vec3::new(0.2, 0.0, 0.2)).length() < 1e-6);
        // Beyond the long edge, the nearest point is on that edge.
        let beyond = Vec3::new(1.0, 0.0, 1.0);
        assert!((closest_on_tri(beyond, &t) - Vec3::new(0.5, 0.0, 0.5)).length() < 1e-6);
    }

    #[test]
    fn crossed_segments_are_their_offset_apart() {
        let d = segment_distance_sq(
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 0.3, -1.0),
            Vec3::new(0.0, 0.3, 1.0),
        );
        assert!((d - 0.09).abs() < 1e-6, "{d}");
    }

    /// Two triangles apart by more than the tolerance do not touch; one
    /// run through the other does, though no corner is near it.
    #[test]
    fn triangles_touch_when_near_or_through() {
        let t = flat();
        let above = |y: f32| {
            Tri::new([
                Vec3::new(0.1, y, 0.1),
                Vec3::new(0.3, y, 0.1),
                Vec3::new(0.1, y, 0.3),
            ])
        };
        assert!(tris_within(&t, &above(0.02), 0.03));
        assert!(!tris_within(&t, &above(0.05), 0.03));
        let through = Tri::new([
            Vec3::new(0.2, -2.0, 0.2),
            Vec3::new(0.25, 2.0, 0.2),
            Vec3::new(0.2, 2.0, 0.25),
        ]);
        assert!(tris_within(&t, &through, 0.001));
    }
}
