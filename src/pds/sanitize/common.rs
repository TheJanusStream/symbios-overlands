//! Shared sanitiser primitives: scalar clamps and the per-primitive
//! [`TortureParams`] clamp used by every primitive `GeneratorKind`.

use super::limits;
use crate::pds::TortureParams;
use crate::pds::types::FP_SCALE;

/// Clamp a single numeric value to a finite range, replacing NaN/Inf with
/// `default`.
pub(super) fn clamp_finite(v: f32, lo: f32, hi: f32, default: f32) -> f32 {
    if v.is_finite() {
        v.clamp(lo, hi)
    } else {
        default
    }
}

/// How far from 1 the squared length of a rotation's WIRE value may sit and
/// [`settle_unit_quat`] still keep the rotation as it is (#1565).
///
/// The wire stores each component on a 1/10 000 grid. Rounding a unit
/// quaternion onto it moves each component by at most half a step, so its
/// squared length by at most `2 * sum(|q_i|) * 0.5e-4 <= 2e-4`, plus float
/// error: 2.02e-4 in all. That bound is reached - four components near 0.5
/// that all round outward land at 2.0003e-4, just past glam's own
/// `is_normalized` tolerance of 2e-4 - though nowhere near it for most
/// rotations: 1.87e-4 is the most measured over 4 000 000 random ones. The
/// tolerance sits a quarter above the bound, so a renormalised rotation is
/// always kept once the wire has rounded it.
pub(crate) const UNIT_QUAT_TOLERANCE: f32 = 2.5e-4;

/// `q` as the wire will hold it: each component rounded onto the 1/10 000
/// grid the way `Fp4` writes it, and read back the way `Fp4` reads it.
fn wire_value(q: [f32; 4]) -> [f32; 4] {
    q.map(|v| ((v * FP_SCALE).round() as i32) as f32 / FP_SCALE)
}

fn length_squared(q: [f32; 4]) -> f32 {
    q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]
}

/// A finite rotation as the sanitisers store it (#1565): kept as it is when
/// its wire value is unit to [`UNIT_QUAT_TOLERANCE`], and renormalised
/// otherwise, which gives one whose wire value always is. So the wire value
/// of the result is one this function keeps, and a saved rotation reloads,
/// sanitises and saves again bit-identical. Renormalising every rotation, as
/// the transform sanitiser did, made 1.4% of rotations written straight from
/// memory come back a grid step off on their first reload (2 000 000
/// simulated, session 909): Isoline's lots differed from the record saved.
///
/// The kept rotation can be that far off unit length, so code that turns a
/// stored rotation into a Bevy one normalises it (`From<&TransformData> for
/// Transform`). A rotation too short to normalise becomes the identity; the
/// callers decide first what counts as no rotation at all.
pub(crate) fn settle_unit_quat(q: [f32; 4]) -> [f32; 4] {
    if (length_squared(wire_value(q)) - 1.0).abs() <= UNIT_QUAT_TOLERANCE {
        return q;
    }
    // One whose squared length overflows is first scaled by its largest
    // component, so it keeps its direction.
    let q = if length_squared(q).is_finite() {
        q
    } else {
        let largest = q.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
        q.map(|v| v / largest)
    };
    let inv = length_squared(q).sqrt().recip();
    if inv.is_finite() && inv > 0.0 {
        q.map(|v| v * inv)
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

/// The blob-element quaternion sanitiser: clamp components finite, then
/// [`settle_unit_quat`]. Keeping a rotation that is already unit, rather
/// than renormalising it every time, is what makes the function idempotent:
/// an exact-arithmetic renormalisation of an ulp-off unit quaternion
/// oscillates between a slightly-short and slightly-long neighbour (a
/// 2-cycle with NO bit-stable fixpoint), which broke the parts'
/// survive-sanitise-unchanged round-trip contract. The mesher's own
/// `Quat::normalize()` absorbs what is kept off unit length.
pub(crate) fn sanitize_unit_quat(q: [f32; 4]) -> [f32; 4] {
    let q = q.map(|v| clamp_finite(v, -1.0, 1.0, 0.0));
    if length_squared(q) <= 1e-6 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    settle_unit_quat(q)
}

// `unit_quat_fixpoint` - [`sanitize_unit_quat`] re-exported as the avatar part
// builders' authoring guard - lived here until #1363. Its only callers were
// the boat parts, and the redesigned sloop authors no rotated node at all: an
// identity quaternion is a sanitise fixpoint for free. It comes back the day a
// craft type needs a `sin`/`cos`-built rotation to round-trip bit-for-bit.

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

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::pds::types::Fp4;

    /// Rotations as code computes them in memory, before the wire has
    /// rounded them (#1565): random unit ones; the yaw-only ones a grown lot
    /// is turned by; the corner where all four components round outward;
    /// and one in ten of those again, up to 5e-4 off unit length.
    pub(in crate::pds::sanitize) fn rotation_sweep() -> Vec<[f32; 4]> {
        // splitmix64: a fixed stream, so a failure names a reproducible case.
        let mut state = 1565_u64;
        let mut uniform = move || {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 23) as f32 - 1.0
        };
        let mut out = Vec::new();
        while out.len() < 20_000 {
            let v = [uniform(), uniform(), uniform(), uniform()];
            let len_sq = length_squared(v);
            if (0.01..=1.0).contains(&len_sq) {
                out.push(bevy::math::Quat::from_array(v).normalize().to_array());
            }
        }
        out.extend((0..4_000).map(|i| {
            let yaw = i as f32 / 4_000.0 * std::f32::consts::TAU - std::f32::consts::PI;
            bevy::math::Quat::from_rotation_y(yaw).to_array()
        }));
        out.push(OUTWARD_CORNER);
        // Rotations the wire holds as the identity though they are not one:
        // a sliver of yaw, a full turn's float noise, a lone `w` that
        // renormalises to a hair under one, a subnormal.
        out.extend([
            bevy::math::Quat::from_rotation_y(5e-5).to_array(),
            bevy::math::Quat::from_rotation_y(4.0 * std::f32::consts::PI).to_array(),
            [0.0, 0.0, 0.0, std::f32::consts::FRAC_1_SQRT_2],
            [1e-40, 0.0, 0.0, 1.0],
        ]);
        let off_unit: Vec<[f32; 4]> = out
            .iter()
            .step_by(10)
            .enumerate()
            .map(|(i, q)| {
                let stretch = 1.0 + ((i % 21) as f32 - 10.0) * 2.5e-5;
                q.map(|c| c * stretch)
            })
            .collect();
        out.extend(off_unit);
        out
    }

    /// A rotation whose squared length overflows keeps its direction: it
    /// came out as the identity, and from the transform sanitiser before
    /// #1565 as a zero rotation.
    #[test]
    fn a_rotation_too_long_to_square_keeps_its_direction() {
        let settled = settle_unit_quat([0.0, 0.0, 1e30, 1e30]);
        let half = std::f32::consts::FRAC_1_SQRT_2;
        assert!(
            settled
                .iter()
                .zip([0.0, 0.0, half, half])
                .all(|(got, want)| (got - want).abs() < 1e-6),
            "{settled:?}"
        );
    }

    /// Unit length to the last bit, and every component rounds outward on
    /// the wire: 5001, 5001, 5000, 5000.
    const OUTWARD_CORNER: [f32; 4] = [0.50005, 0.50005, 0.49995, 0.49995];

    /// Why the tolerance is wider than glam's 2e-4: a unit rotation's wire
    /// value can land past it, and renormalising that one on reload would
    /// move it a grid step.
    #[test]
    fn a_unit_rotation_the_wire_rounds_past_glams_tolerance_is_kept() {
        assert_eq!(length_squared(OUTWARD_CORNER), 1.0);
        let wire = wire_value(OUTWARD_CORNER);
        assert!(
            length_squared(wire) - 1.0 > 2e-4,
            "no longer the corner: {wire:?}"
        );
        assert_eq!(settle_unit_quat(OUTWARD_CORNER), OUTWARD_CORNER);
        assert_eq!(settle_unit_quat(wire), wire);
    }

    /// A blob element's rotation, sanitised, saved and reloaded, is one the
    /// sanitiser leaves as it is - and `wire_value` is what `Fp4` really
    /// stores.
    #[test]
    fn a_blob_rotation_reloads_and_sanitises_unchanged() {
        for q in rotation_sweep() {
            let kept = sanitize_unit_quat(q);
            assert_eq!(sanitize_unit_quat(kept), kept, "not idempotent for {q:?}");
            let json = serde_json::to_string(&Fp4(kept)).expect("serialise");
            let reloaded: Fp4 = serde_json::from_str(&json).expect("decode");
            assert_eq!(
                reloaded.0,
                wire_value(kept),
                "wire_value is not Fp4 for {kept:?}"
            );
            assert_eq!(
                sanitize_unit_quat(reloaded.0),
                reloaded.0,
                "sanitise moved the reloaded {q:?}"
            );
        }
    }
}
