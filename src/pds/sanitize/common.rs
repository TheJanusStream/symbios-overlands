//! Shared sanitiser primitives: scalar clamps and the per-primitive
//! `(twist, taper, bend)` torture clamp used by every primitive
//! `GeneratorKind`.

use super::limits;
use crate::pds::types::{Fp, Fp3};

/// Clamp a single numeric value to a finite range, replacing NaN/Inf with
/// `default`.
pub(super) fn clamp_finite(v: f32, lo: f32, hi: f32, default: f32) -> f32 {
    if v.is_finite() {
        v.clamp(lo, hi)
    } else {
        default
    }
}

/// Clamp the `(twist, taper, bend)` torture triple attached to every
/// primitive. Values drive the CPU-side vertex mutation pass in
/// `world_builder::prim::apply_vertex_torture`; out-of-range inputs produce
/// degenerate meshes (NaN vertex positions, zero-volume colliders) so we
/// clamp them on ingest rather than in the spawn loop.
pub(super) fn sanitize_torture(twist: &mut Fp, taper: &mut Fp, bend: &mut Fp3) {
    let t = limits::MAX_TORTURE_TWIST;
    let tp = limits::MAX_TORTURE_TAPER;
    let b = limits::MAX_TORTURE_BEND;
    twist.0 = clamp_finite(twist.0, -t, t, 0.0);
    taper.0 = clamp_finite(taper.0, -tp, tp, 0.0);
    for i in 0..3 {
        bend.0[i] = clamp_finite(bend.0[i], -b, b, 0.0);
    }
}
