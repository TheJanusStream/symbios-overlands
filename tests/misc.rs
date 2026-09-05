//! Miscellaneous helpers that don't belong to a specific domain module:
//! `format_elapsed_ts`, the `Generator` primitive tag round-trip, and the
//! DID document URL builder.

use symbios_overlands::format_elapsed_ts;
use symbios_overlands::pds::PrimCommon;
use symbios_overlands::pds::{
    Fp, Fp2, Fp3, Generator, GeneratorKind, TortureParams, sanitize_generator,
};

// ---------------------------------------------------------------------------
// format_elapsed_ts — the Diagnostics event log and the anomaly hovers.
// ---------------------------------------------------------------------------

#[test]
fn format_elapsed_under_one_minute() {
    assert_eq!(format_elapsed_ts(0.0), "+00:00");
    assert_eq!(format_elapsed_ts(5.3), "+00:05");
    assert_eq!(format_elapsed_ts(59.999), "+00:59");
}

#[test]
fn format_elapsed_minutes_and_seconds() {
    assert_eq!(format_elapsed_ts(60.0), "+01:00");
    assert_eq!(format_elapsed_ts(125.0), "+02:05");
    assert_eq!(format_elapsed_ts(599.0), "+09:59");
    assert_eq!(format_elapsed_ts(3599.0), "+59:59");
}

#[test]
fn format_elapsed_promotes_to_hours_after_an_hour() {
    // Zero-padded M/S, non-padded hours.
    assert_eq!(format_elapsed_ts(3600.0), "+1:00:00");
    assert_eq!(format_elapsed_ts(3661.0), "+1:01:01");
    assert_eq!(format_elapsed_ts(12345.0), "+3:25:45");
}

#[test]
fn format_elapsed_handles_very_long_sessions() {
    // 10-hour session — still renders sensibly.
    assert_eq!(format_elapsed_ts(36_000.0), "+10:00:00");
}

/// An elapsed stamp cannot be mistaken for a wall clock (#1264 f231).
///
/// The two formats sat side by side in one app — "14:32" meaning fourteen
/// minutes into the session in the Diagnostics log, and twenty past two in
/// chat — and nothing on either said which it was. The `+` is the whole
/// distinction, so it is asserted as such rather than left implicit in the
/// four format tests above.
#[test]
fn an_elapsed_stamp_is_marked_as_elapsed() {
    for secs in [0.0, 59.0, 90.0, 3600.0, 36_000.0] {
        let stamp = format_elapsed_ts(secs);
        assert!(
            stamp.starts_with('+'),
            "{stamp} would read as a time of day"
        );
        // And the rest is still the HH:MM:SS shape a reader can scan.
        assert!(stamp[1..].chars().all(|c| c.is_ascii_digit() || c == ':'));
    }
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
            common: PrimCommon {
                solid: true,
                material: Default::default(),
                torture: TortureParams {
                    twist: Fp(f32::NAN),
                    taper: Fp2([f32::INFINITY, f32::NAN]),
                    taper_bottom: Fp2([f32::NAN, -10_000.0]),
                    bulge: Fp2([f32::NEG_INFINITY, 10_000.0]),
                    bend: Fp3([f32::NAN, f32::NEG_INFINITY, 10_000.0]),
                    s_bend: Fp2([f32::NAN, f32::INFINITY]),
                    shear: Fp2([f32::INFINITY, 10_000.0]),
                    path_cut: Fp2([-1.0, 5.0]),
                    profile_cut: Fp2([f32::NAN, 2.0]),
                    hollow: Fp(10.0),
                },
                ..Default::default()
            },
            size: Fp3([f32::NAN, -1.0, f32::INFINITY]),
        }),
        Generator::from_kind(GeneratorKind::Sphere {
            radius: Fp(f32::NAN),
            resolution: u32::MAX,
            common: PrimCommon {
                solid: true,
                material: Default::default(),
                ..Default::default()
            },
        }),
        Generator::from_kind(GeneratorKind::Cylinder {
            radius: Fp(-10.0),
            height: Fp(f32::INFINITY),
            resolution: 10_000,
            common: PrimCommon {
                solid: true,
                material: Default::default(),
                ..Default::default()
            },
        }),
        Generator::from_kind(GeneratorKind::Capsule {
            radius: Fp(-1.0),
            length: Fp(f32::NAN),
            latitudes: 10_000,
            longitudes: 10_000,
            common: PrimCommon {
                solid: true,
                material: Default::default(),
                ..Default::default()
            },
        }),
        Generator::from_kind(GeneratorKind::Cone {
            radius: Fp(f32::NEG_INFINITY),
            height: Fp(-5.0),
            resolution: 10_000,
            common: PrimCommon {
                solid: true,
                material: Default::default(),
                ..Default::default()
            },
        }),
        Generator::from_kind(GeneratorKind::Torus {
            minor_radius: Fp(f32::NAN),
            major_radius: Fp(-2.0),
            minor_resolution: 10_000,
            major_resolution: 10_000,
            common: PrimCommon {
                solid: true,
                material: Default::default(),
                ..Default::default()
            },
        }),
        Generator::from_kind(GeneratorKind::Plane {
            common: PrimCommon {
                solid: true,
                material: Default::default(),
                ..Default::default()
            },
            size: Fp2([f32::INFINITY, -1.0]),
            subdivisions: 10_000,
        }),
        Generator::from_kind(GeneratorKind::Tetrahedron {
            common: PrimCommon {
                solid: true,
                material: Default::default(),
                ..Default::default()
            },
            size: Fp(f32::NAN),
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
