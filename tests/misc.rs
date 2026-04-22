//! Miscellaneous helpers that don't belong to a specific domain module:
//! `format_elapsed_ts`, `PrimShape::default_for_tag` + sanitize, and the
//! DID document URL builder.

use symbios_overlands::format_elapsed_ts;
use symbios_overlands::pds::{Fp, Fp2, Fp3, PrimShape};

// ---------------------------------------------------------------------------
// format_elapsed_ts — used to label chat messages and the diagnostics log.
// ---------------------------------------------------------------------------

#[test]
fn format_elapsed_under_one_minute() {
    assert_eq!(format_elapsed_ts(0.0), "00:00");
    assert_eq!(format_elapsed_ts(5.3), "00:05");
    assert_eq!(format_elapsed_ts(59.999), "00:59");
}

#[test]
fn format_elapsed_minutes_and_seconds() {
    assert_eq!(format_elapsed_ts(60.0), "01:00");
    assert_eq!(format_elapsed_ts(125.0), "02:05");
    assert_eq!(format_elapsed_ts(599.0), "09:59");
    assert_eq!(format_elapsed_ts(3599.0), "59:59");
}

#[test]
fn format_elapsed_promotes_to_hours_after_an_hour() {
    // Zero-padded M/S, non-padded hours.
    assert_eq!(format_elapsed_ts(3600.0), "1:00:00");
    assert_eq!(format_elapsed_ts(3661.0), "1:01:01");
    assert_eq!(format_elapsed_ts(12345.0), "3:25:45");
}

#[test]
fn format_elapsed_handles_very_long_sessions() {
    // 10-hour session — still renders sensibly.
    assert_eq!(format_elapsed_ts(36_000.0), "10:00:00");
}

// ---------------------------------------------------------------------------
// PrimShape — tag helpers + sanitize.
// ---------------------------------------------------------------------------

#[test]
fn prim_shape_tag_round_trips() {
    // Every `kind_tag` value must be parseable back to the same shape class
    // by `default_for_tag`. Drift in either direction would break the UI's
    // shape-type dropdown, which uses the tag as the round-trip key.
    for tag in &[
        "Cuboid",
        "Sphere",
        "Cylinder",
        "Capsule",
        "Cone",
        "Torus",
        "Plane",
        "Tetrahedron",
    ] {
        let shape = PrimShape::default_for_tag(tag);
        assert_eq!(
            shape.kind_tag(),
            *tag,
            "kind_tag round-trip failed for {tag}"
        );
    }
}

#[test]
fn prim_shape_unknown_tag_falls_back_to_default() {
    let shape = PrimShape::default_for_tag("not-a-real-tag");
    assert_eq!(shape.kind_tag(), PrimShape::default().kind_tag());
}

#[test]
fn prim_shape_sanitize_clamps_non_finite_dimensions() {
    // Every shape variant carries its own dimensional knobs. Sanitize
    // must clamp NaN and negative values before they hit Bevy's mesh /
    // Avian's collider constructors.
    let cases: Vec<PrimShape> = vec![
        PrimShape::Cuboid {
            size: Fp3([f32::NAN, -1.0, f32::INFINITY]),
        },
        PrimShape::Sphere {
            radius: Fp(f32::NAN),
            resolution: u32::MAX,
        },
        PrimShape::Cylinder {
            radius: Fp(-10.0),
            height: Fp(f32::INFINITY),
            resolution: 10_000,
        },
        PrimShape::Capsule {
            radius: Fp(-1.0),
            length: Fp(f32::NAN),
            latitudes: 10_000,
            longitudes: 10_000,
        },
        PrimShape::Cone {
            radius: Fp(f32::NEG_INFINITY),
            height: Fp(-5.0),
            resolution: 10_000,
        },
        PrimShape::Torus {
            minor_radius: Fp(f32::NAN),
            major_radius: Fp(-2.0),
            minor_resolution: 10_000,
            major_resolution: 10_000,
        },
        PrimShape::Plane {
            size: Fp2([f32::INFINITY, -1.0]),
            subdivisions: 10_000,
        },
        PrimShape::Tetrahedron { size: Fp(f32::NAN) },
    ];

    for case in cases {
        let mut shape = case;
        shape.sanitize();
        // Re-encode/decode to verify the shape is valid after sanitize —
        // an intermediate panic here would surface immediately.
        let json = serde_json::to_string(&shape).expect("sanitised shape must serialise");
        let _: PrimShape = serde_json::from_str(&json).expect("sanitised shape must round-trip");
    }
}
