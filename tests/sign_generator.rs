//! Integration tests for the [`Sign`](symbios_overlands::pds::GeneratorKind::Sign)
//! generator. Cover wire-format round-trips for every [`SignSource`]
//! variant, forward-compat for `Unknown` payloads, sanitiser clamps for
//! every numeric / string field, and the `kind_tag` uniqueness invariant
//! that the editor's variant picker depends on.

use symbios_overlands::pds::{
    AlphaModeKind, Fp, Fp2, Generator, GeneratorKind, SignSource, SovereignMaterialSettings,
    limits, sanitize_generator,
};

fn sample_sign(source: SignSource, alpha_mode: AlphaModeKind) -> Generator {
    Generator::from_kind(GeneratorKind::Sign {
        source,
        size: Fp2([2.0, 1.5]),
        uv_repeat: Fp2([1.0, 1.0]),
        uv_offset: Fp2([0.0, 0.0]),
        material: SovereignMaterialSettings::default(),
        double_sided: false,
        alpha_mode,
        unlit: true,
    })
}

// ---------------------------------------------------------------------------
// Round-trip coverage for every SignSource variant.
// ---------------------------------------------------------------------------

#[test]
fn sign_with_url_source_round_trips() {
    let original = sample_sign(
        SignSource::Url {
            url: "https://example.org/banner.png".into(),
        },
        AlphaModeKind::Opaque,
    );
    let json = serde_json::to_string(&original).expect("serialise");
    let back: Generator = serde_json::from_str(&json).expect("deserialise");
    let original_v: serde_json::Value = serde_json::to_value(&original).unwrap();
    let back_v: serde_json::Value = serde_json::to_value(&back).unwrap();
    assert_eq!(original_v, back_v, "URL Sign must round-trip without drift");
}

#[test]
fn sign_with_atproto_blob_source_round_trips() {
    let original = sample_sign(
        SignSource::AtprotoBlob {
            did: "did:plc:author".into(),
            cid: "bafkreigh2akiscaildc...".into(),
        },
        AlphaModeKind::Mask { cutoff: Fp(0.4) },
    );
    let json = serde_json::to_string(&original).expect("serialise");
    let back: Generator = serde_json::from_str(&json).expect("deserialise");
    let original_v: serde_json::Value = serde_json::to_value(&original).unwrap();
    let back_v: serde_json::Value = serde_json::to_value(&back).unwrap();
    assert_eq!(
        original_v, back_v,
        "AtprotoBlob Sign must round-trip without drift"
    );
}

#[test]
fn sign_with_did_pfp_source_round_trips() {
    let original = sample_sign(
        SignSource::DidPfp {
            did: "did:plc:portrait".into(),
        },
        AlphaModeKind::Blend,
    );
    let json = serde_json::to_string(&original).expect("serialise");
    let back: Generator = serde_json::from_str(&json).expect("deserialise");
    let original_v: serde_json::Value = serde_json::to_value(&original).unwrap();
    let back_v: serde_json::Value = serde_json::to_value(&back).unwrap();
    assert_eq!(
        original_v, back_v,
        "DidPfp Sign must round-trip without drift"
    );
}

// ---------------------------------------------------------------------------
// Forward-compat: unknown source / alpha-mode tags decode to Unknown.
// ---------------------------------------------------------------------------

#[test]
fn unknown_sign_source_decodes_to_unknown() {
    // Synthesise a Sign whose `source` carries a future variant tag.
    // The decoder must surface it as `SignSource::Unknown` rather than
    // failing the whole generator decode — otherwise a record authored
    // by a forward-compat client would render as an opaque error block.
    let json = r#"{
        "$type": "network.symbios.gen.sign",
        "source": { "$type": "network.symbios.sign.future_holo_2027", "id": "abc" },
        "size": [10000, 10000],
        "uv_repeat": [10000, 10000],
        "uv_offset": [0, 0],
        "material": {
            "base_color": [10000, 10000, 10000],
            "emission_color": [0, 0, 0],
            "emission_strength": 0,
            "roughness": 5000,
            "metallic": 0,
            "uv_scale": 10000,
            "texture": { "$type": "network.symbios.tex.none" }
        },
        "double_sided": false,
        "alpha_mode": { "$type": "network.symbios.alpha.opaque" },
        "unlit": true
    }"#;
    let kind: GeneratorKind =
        serde_json::from_str(json).expect("unknown source must not crash decode");
    match kind {
        GeneratorKind::Sign { source, .. } => assert!(matches!(source, SignSource::Unknown)),
        other => panic!("expected Sign, got {other:?}"),
    }
}

#[test]
fn unknown_alpha_mode_decodes_to_unknown() {
    let json = r#"{
        "$type": "network.symbios.gen.sign",
        "source": { "$type": "network.symbios.sign.url", "url": "https://example.org/x.png" },
        "size": [10000, 10000],
        "uv_repeat": [10000, 10000],
        "uv_offset": [0, 0],
        "material": {
            "base_color": [10000, 10000, 10000],
            "emission_color": [0, 0, 0],
            "emission_strength": 0,
            "roughness": 5000,
            "metallic": 0,
            "uv_scale": 10000,
            "texture": { "$type": "network.symbios.tex.none" }
        },
        "double_sided": false,
        "alpha_mode": { "$type": "network.symbios.alpha.future_dither_2030" },
        "unlit": true
    }"#;
    let kind: GeneratorKind =
        serde_json::from_str(json).expect("unknown alpha mode must not crash decode");
    match kind {
        GeneratorKind::Sign { alpha_mode, .. } => {
            assert!(matches!(alpha_mode, AlphaModeKind::Unknown))
        }
        other => panic!("expected Sign, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Sanitiser clamps.
// ---------------------------------------------------------------------------

#[test]
fn sanitiser_clamps_panel_size() {
    let mut g = sample_sign(
        SignSource::Url {
            url: "https://example.org/x.png".into(),
        },
        AlphaModeKind::Opaque,
    );
    if let GeneratorKind::Sign { size, .. } = &mut g.kind {
        *size = Fp2([f32::NAN, 10_000.0]);
    }
    sanitize_generator(&mut g);
    if let GeneratorKind::Sign { size, .. } = &g.kind {
        assert!(size.0[0].is_finite(), "NaN clamps to a finite default");
        assert!(size.0[0] >= 0.01 && size.0[0] <= limits::MAX_SIGN_SIZE);
        assert!(size.0[1] <= limits::MAX_SIGN_SIZE);
    } else {
        panic!("expected Sign after sanitise");
    }
}

#[test]
fn sanitiser_clamps_uv_repeat_and_offset() {
    let mut g = sample_sign(
        SignSource::Url {
            url: "https://example.org/x.png".into(),
        },
        AlphaModeKind::Opaque,
    );
    if let GeneratorKind::Sign {
        uv_repeat,
        uv_offset,
        ..
    } = &mut g.kind
    {
        *uv_repeat = Fp2([f32::INFINITY, 0.0]);
        *uv_offset = Fp2([1_000_000.0, -1_000_000.0]);
    }
    sanitize_generator(&mut g);
    if let GeneratorKind::Sign {
        uv_repeat,
        uv_offset,
        ..
    } = &g.kind
    {
        assert!(uv_repeat.0[0].is_finite());
        assert!(uv_repeat.0[0] >= limits::MIN_SIGN_UV_REPEAT);
        assert!(uv_repeat.0[0] <= limits::MAX_SIGN_UV_REPEAT);
        assert!(uv_repeat.0[1] >= limits::MIN_SIGN_UV_REPEAT);
        assert!(uv_offset.0[0] <= limits::MAX_SIGN_UV_OFFSET);
        assert!(uv_offset.0[1] >= -limits::MAX_SIGN_UV_OFFSET);
    } else {
        panic!("expected Sign after sanitise");
    }
}

#[test]
fn sanitiser_clamps_mask_cutoff() {
    // Mask cutoff outside [0, 1] would propagate to the StandardMaterial
    // shader as a NaN comparison; clamp to the nearest valid bound.
    let mut g = sample_sign(
        SignSource::Url {
            url: "https://example.org/x.png".into(),
        },
        AlphaModeKind::Mask {
            cutoff: Fp(f32::NAN),
        },
    );
    sanitize_generator(&mut g);
    if let GeneratorKind::Sign { alpha_mode, .. } = &g.kind {
        if let AlphaModeKind::Mask { cutoff } = alpha_mode {
            assert!(cutoff.0.is_finite());
            assert!(cutoff.0 >= 0.0 && cutoff.0 <= 1.0);
        } else {
            panic!("expected Mask, got {alpha_mode:?}");
        }
    } else {
        panic!("expected Sign after sanitise");
    }
}

#[test]
fn sanitiser_truncates_oversize_url() {
    let huge_url = format!(
        "https://example.org/{}",
        "a".repeat(limits::MAX_SIGN_URL_BYTES * 2)
    );
    let mut g = sample_sign(
        SignSource::Url {
            url: huge_url.clone(),
        },
        AlphaModeKind::Opaque,
    );
    sanitize_generator(&mut g);
    if let GeneratorKind::Sign { source, .. } = &g.kind {
        if let SignSource::Url { url } = source {
            assert!(url.len() <= limits::MAX_SIGN_URL_BYTES);
        } else {
            panic!("expected Url, got {source:?}");
        }
    } else {
        panic!("expected Sign after sanitise");
    }
}

#[test]
fn sanitiser_truncates_oversize_did_and_cid() {
    let huge_did = "did:plc:".to_string() + &"x".repeat(limits::MAX_SIGN_DID_BYTES * 2);
    let huge_cid = "bafy".to_string() + &"y".repeat(limits::MAX_SIGN_CID_BYTES * 2);
    let mut g = sample_sign(
        SignSource::AtprotoBlob {
            did: huge_did,
            cid: huge_cid,
        },
        AlphaModeKind::Opaque,
    );
    sanitize_generator(&mut g);
    if let GeneratorKind::Sign { source, .. } = &g.kind {
        if let SignSource::AtprotoBlob { did, cid } = source {
            assert!(did.len() <= limits::MAX_SIGN_DID_BYTES);
            assert!(cid.len() <= limits::MAX_SIGN_CID_BYTES);
        } else {
            panic!("expected AtprotoBlob, got {source:?}");
        }
    } else {
        panic!("expected Sign after sanitise");
    }
}

// ---------------------------------------------------------------------------
// Editor invariants.
// ---------------------------------------------------------------------------

#[test]
fn sign_kind_tag_is_unique() {
    // The variant picker keys on `kind_tag`; a duplicate tag would
    // render two distinct kinds as the same row in the combo box.
    let kinds: Vec<&'static str> = vec![
        GeneratorKind::default_cuboid().kind_tag(),
        GeneratorKind::default_sign().kind_tag(),
    ];
    let mut seen = std::collections::HashSet::new();
    for k in &kinds {
        assert!(seen.insert(*k), "duplicate kind_tag: {k}");
    }
    assert!(kinds.contains(&"Sign"));
}

#[test]
fn default_sign_round_trips() {
    // The UI's "+ Sign" entry constructs `default_sign`. It must be a
    // valid record on its own — sanitise leaves it unchanged and JSON
    // round-trip preserves shape.
    let g = Generator::from_kind(GeneratorKind::default_sign());
    let json = serde_json::to_string(&g).expect("serialise");
    let back: Generator = serde_json::from_str(&json).expect("deserialise");
    let original_v: serde_json::Value = serde_json::to_value(&g).unwrap();
    let back_v: serde_json::Value = serde_json::to_value(&back).unwrap();
    assert_eq!(original_v, back_v);

    let mut sanitised = g.clone();
    sanitize_generator(&mut sanitised);
    let sanitised_v: serde_json::Value = serde_json::to_value(&sanitised).unwrap();
    assert_eq!(
        sanitised_v, original_v,
        "default_sign must be sanitiser-stable"
    );
}
