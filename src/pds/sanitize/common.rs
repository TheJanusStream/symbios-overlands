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
    for v in t.taper.0.iter_mut().chain(t.taper_bottom.0.iter_mut()) {
        *v = clamp_finite(*v, -tp, tp, 0.0);
    }
    let bu = limits::MAX_TORTURE_BULGE;
    for v in t.bulge.0.iter_mut() {
        *v = clamp_finite(*v, -bu, bu, 0.0);
    }
    for v in t.bend.0.iter_mut() {
        *v = clamp_finite(*v, -b, b, 0.0);
    }
    for v in t.s_bend.0.iter_mut() {
        *v = clamp_finite(*v, -b, b, 0.0);
    }
    let sh = limits::MAX_TORTURE_SHEAR;
    for v in t.shear.0.iter_mut() {
        *v = clamp_finite(*v, -sh, sh, 0.0);
    }

    // Topology cuts. path_cut / profile_cut are kept ranges in [0, 1] with
    // begin ≤ end (a default-identity [0, 1] when degenerate); hollow is a bore
    // fraction in [0, 0.95] (floored below 1 so a wall always remains).
    sanitize_cut_range(&mut t.path_cut.0);
    sanitize_cut_range(&mut t.profile_cut.0);
    t.hollow.0 = clamp_finite(t.hollow.0, 0.0, limits::MAX_HOLLOW, 0.0);
}

/// Clamp a `[begin, end]` cut range into `[0, 1]` with `begin ≤ end`; collapse a
/// degenerate or inverted range back to the full `[0, 1]` identity so a hostile
/// record can't produce a zero-width (vertex-less) sweep.
fn sanitize_cut_range(r: &mut [f32; 2]) {
    let begin = clamp_finite(r[0], 0.0, 1.0, 0.0);
    let end = clamp_finite(r[1], 0.0, 1.0, 1.0);
    if end - begin < 1e-3 {
        *r = [0.0, 1.0];
    } else {
        *r = [begin, end];
    }
}
