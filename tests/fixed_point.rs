//! Integration tests for the `Fp*` DAG-CBOR-safe fixed-point wrappers.
//!
//! Every `f32` in a record is encoded as an `i32` scaled by [`FP_SCALE`].
//! These tests guard the wire format (integer on the wire, no float
//! literals) and the round-trip quantisation bound.

use symbios_overlands::pds::{FP_SCALE, Fp, Fp2, Fp3, Fp4, Fp64, TransformData};

/// Regression guard: every `f32` we put in must come back equal within
/// the quantisation error of `FP_SCALE`.
#[test]
fn fixed_point_round_trip_preserves_values() {
    let original = TransformData {
        translation: Fp3([1.5, -2.25, 3.125]),
        rotation: Fp4([0.0, 0.6, 0.0, 0.8]),
        scale: Fp3([1.0, 2.0, 0.5]),
    };
    let json = serde_json::to_string(&original).unwrap();
    let decoded: TransformData = serde_json::from_str(&json).unwrap();
    let eps = 1.0 / FP_SCALE;
    for (a, b) in original
        .translation
        .0
        .iter()
        .zip(decoded.translation.0.iter())
    {
        assert!((a - b).abs() < eps, "translation drift: {a} vs {b}");
    }
    for (a, b) in original.rotation.0.iter().zip(decoded.rotation.0.iter()) {
        assert!((a - b).abs() < eps, "rotation drift: {a} vs {b}");
    }
    for (a, b) in original.scale.0.iter().zip(decoded.scale.0.iter()) {
        assert!((a - b).abs() < eps, "scale drift: {a} vs {b}");
    }
}

/// `Fp` encodes to a plain JSON integer (no `.0` suffix, no quotes).
#[test]
fn fp_wire_form_is_integer() {
    let json = serde_json::to_string(&Fp(1.0)).unwrap();
    assert_eq!(json, "10000");
}

#[test]
fn fp_negative_rounds_correctly() {
    let json = serde_json::to_string(&Fp(-0.5)).unwrap();
    assert_eq!(json, "-5000");
}

#[test]
fn fp2_fp3_fp4_encode_as_integer_arrays() {
    assert_eq!(
        serde_json::to_string(&Fp2([1.0, 2.0])).unwrap(),
        "[10000,20000]"
    );
    assert_eq!(
        serde_json::to_string(&Fp3([1.0, 2.0, -0.5])).unwrap(),
        "[10000,20000,-5000]"
    );
    assert_eq!(
        serde_json::to_string(&Fp4([0.0, 0.6, 0.0, 0.8])).unwrap(),
        "[0,6000,0,8000]"
    );
}

/// `Fp64` has a larger scale (to carry wider coordinates for terrain /
/// texture generators) but must still stay integer on the wire.
#[test]
fn fp64_wire_form_is_integer() {
    let json = serde_json::to_string(&Fp64(2.5)).unwrap();
    // Don't hard-code the scale; just assert it parses as a base-10 int.
    let val: i64 = json
        .trim_start_matches('-')
        .parse()
        .unwrap_or_else(|e| panic!("Fp64 must serialise as an integer, got {json}: {e}"));
    assert!(val != 0, "encoded Fp64(2.5) must be non-zero");
}

/// Quantisation ceiling: a value well below half a quantum cannot survive
/// the round trip. Expected behaviour — asserted so a future change to
/// [`FP_SCALE`] is a deliberate choice, not an accidental loss of
/// precision.
#[test]
fn fp_values_below_quantum_round_to_zero() {
    let tiny = 0.1 / FP_SCALE; // ~10% of a quantum
    let json = serde_json::to_string(&Fp(tiny)).unwrap();
    let back: Fp = serde_json::from_str(&json).unwrap();
    assert_eq!(back.0, 0.0);
}

/// Wire form is stable across platforms — an i32 integer with no locale
/// separators and no scientific notation. Guard against someone swapping
/// in a different serializer that tries to prettify large numbers.
#[test]
fn fp_wire_form_has_no_scientific_notation() {
    let json = serde_json::to_string(&Fp(12345.0)).unwrap();
    assert!(!json.contains('e') && !json.contains('E'));
    assert!(!json.contains(','));
}

/// `NaN` and `±inf` cannot survive the cast to `i32` cleanly — the
/// wrappers must **not** panic on malicious input; they just go to 0.
/// This is a behavioural guard, not a correctness claim.
#[test]
fn fp_handles_nonfinite_without_panic() {
    // We don't care what the wire form of NaN is — only that we didn't
    // abort. An `as i32` cast of NaN is `0` in Rust (saturating cast).
    let _ = serde_json::to_string(&Fp(f32::NAN)).unwrap();
    let _ = serde_json::to_string(&Fp(f32::INFINITY)).unwrap();
    let _ = serde_json::to_string(&Fp(f32::NEG_INFINITY)).unwrap();
}

/// Deserialising the wire form of a typical rotation quaternion leaves the
/// magnitude close to unit — one unit-normal round-trip fits within the
/// quantisation budget across all four components.
#[test]
fn fp4_unit_quaternion_round_trip_stays_unit() {
    let q = Fp4([0.0, 0.707_106_78, 0.0, 0.707_106_78]);
    let json = serde_json::to_string(&q).unwrap();
    let back: Fp4 = serde_json::from_str(&json).unwrap();
    let m = back.0.iter().map(|c| c * c).sum::<f32>().sqrt();
    // Allow two quanta of slack per component (4 × 2 / FP_SCALE).
    let eps = 8.0 / FP_SCALE;
    assert!((m - 1.0).abs() < eps, "|q| drifted to {m}");
}
