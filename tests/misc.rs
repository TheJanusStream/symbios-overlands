//! Miscellaneous helpers that don't belong to a specific domain module:
//! `format_elapsed_ts`, the `Generator` primitive tag round-trip, and the
//! DID document URL builder.

use symbios_overlands::format_elapsed_ts;
use symbios_overlands::pds::{Fp, Fp2, Fp3, Generator, GeneratorKind, sanitize_generator};

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
// Generator primitive tag — round-trip between `kind_tag` and the
// default-builder. The UI's shape-type dropdown uses the tag as the
// round-trip key, so drift in either direction would break in-editor kind
// switches.
// ---------------------------------------------------------------------------

#[test]
fn primitive_tag_round_trips() {
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
        let prim = Generator::default_primitive_for_tag(tag)
            .unwrap_or_else(|| panic!("no default primitive for `{tag}`"));
        assert_eq!(
            prim.kind_tag(),
            *tag,
            "kind_tag round-trip failed for {tag}"
        );
    }
}

#[test]
fn primitive_unknown_tag_returns_none() {
    assert!(Generator::default_primitive_for_tag("not-a-real-tag").is_none());
}

#[test]
fn primitive_sanitize_clamps_non_finite_dimensions() {
    // Every parametric primitive variant carries its own dimensional knobs.
    // Sanitize must clamp NaN / infinity / negative values before they hit
    // Bevy's mesh / Avian's collider constructors.
    let cases: Vec<Generator> = vec![
        Generator::from_kind(GeneratorKind::Cuboid {
            size: Fp3([f32::NAN, -1.0, f32::INFINITY]),
            solid: true,
            material: Default::default(),
            twist: Fp(f32::NAN),
            taper: Fp(f32::INFINITY),
            bend: Fp3([f32::NAN, f32::NEG_INFINITY, 10_000.0]),
        }),
        Generator::from_kind(GeneratorKind::Sphere {
            radius: Fp(f32::NAN),
            resolution: u32::MAX,
            solid: true,
            material: Default::default(),
            twist: Fp(0.0),
            taper: Fp(0.0),
            bend: Fp3([0.0, 0.0, 0.0]),
        }),
        Generator::from_kind(GeneratorKind::Cylinder {
            radius: Fp(-10.0),
            height: Fp(f32::INFINITY),
            resolution: 10_000,
            solid: true,
            material: Default::default(),
            twist: Fp(0.0),
            taper: Fp(0.0),
            bend: Fp3([0.0, 0.0, 0.0]),
        }),
        Generator::from_kind(GeneratorKind::Capsule {
            radius: Fp(-1.0),
            length: Fp(f32::NAN),
            latitudes: 10_000,
            longitudes: 10_000,
            solid: true,
            material: Default::default(),
            twist: Fp(0.0),
            taper: Fp(0.0),
            bend: Fp3([0.0, 0.0, 0.0]),
        }),
        Generator::from_kind(GeneratorKind::Cone {
            radius: Fp(f32::NEG_INFINITY),
            height: Fp(-5.0),
            resolution: 10_000,
            solid: true,
            material: Default::default(),
            twist: Fp(0.0),
            taper: Fp(0.0),
            bend: Fp3([0.0, 0.0, 0.0]),
        }),
        Generator::from_kind(GeneratorKind::Torus {
            minor_radius: Fp(f32::NAN),
            major_radius: Fp(-2.0),
            minor_resolution: 10_000,
            major_resolution: 10_000,
            solid: true,
            material: Default::default(),
            twist: Fp(0.0),
            taper: Fp(0.0),
            bend: Fp3([0.0, 0.0, 0.0]),
        }),
        Generator::from_kind(GeneratorKind::Plane {
            size: Fp2([f32::INFINITY, -1.0]),
            subdivisions: 10_000,
            solid: true,
            material: Default::default(),
            twist: Fp(0.0),
            taper: Fp(0.0),
            bend: Fp3([0.0, 0.0, 0.0]),
        }),
        Generator::from_kind(GeneratorKind::Tetrahedron {
            size: Fp(f32::NAN),
            solid: true,
            material: Default::default(),
            twist: Fp(0.0),
            taper: Fp(0.0),
            bend: Fp3([0.0, 0.0, 0.0]),
        }),
    ];

    for case in cases {
        let mut prim = case;
        sanitize_generator(&mut prim);
        // Re-encode/decode to verify the sanitized generator is valid — an
        // intermediate panic here would surface immediately, and a decode
        // failure would mean sanitize left the record malformed.
        let json = serde_json::to_string(&prim).expect("sanitised generator must serialise");
        let _: Generator =
            serde_json::from_str(&json).expect("sanitised generator must round-trip");
    }
}
