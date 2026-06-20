//! Shared sanitiser primitives: scalar clamps and the per-primitive
//! [`TortureParams`] clamp used by every primitive `GeneratorKind`.

use super::limits;
use crate::pds::TortureParams;

/// Clamp a single numeric value to a finite range, replacing NaN/Inf with
/// `default`.
pub(super) fn clamp_finite(v: f32, lo: f32, hi: f32, default: f32) -> f32 {
    if v.is_finite() {
        v.clamp(lo, hi)
    } else {
        default
    }
}

/// Clamp the [`TortureParams`] attached to every primitive. Values drive the
/// CPU-side vertex mutation pass in
/// `world_builder::prim::apply_vertex_torture`; out-of-range inputs produce
/// degenerate meshes (NaN vertex positions, zero-volume colliders) so we
/// clamp them on ingest rather than in the spawn loop. Per-axis taper and the
/// S-bend reuse the scalar taper / bend magnitude bounds.
pub(super) fn sanitize_torture(t: &mut TortureParams) {
    let tw = limits::MAX_TORTURE_TWIST;
    let tp = limits::MAX_TORTURE_TAPER;
    let b = limits::MAX_TORTURE_BEND;
    t.twist.0 = clamp_finite(t.twist.0, -tw, tw, 0.0);
    for v in t.taper.0.iter_mut() {
        *v = clamp_finite(*v, -tp, tp, 0.0);
    }
    for v in t.bend.0.iter_mut() {
        *v = clamp_finite(*v, -b, b, 0.0);
    }
    for v in t.s_bend.0.iter_mut() {
        *v = clamp_finite(*v, -b, b, 0.0);
    }
}
