//! Integration tests for the [`Sign`](symbios_overlands::pds::GeneratorKind::Sign)
//! generator. Cover wire-format round-trips for every [`SignSource`]
//! variant, forward-compat for `Unknown` payloads, sanitiser clamps for
//! every numeric / string field, and the `kind_tag` uniqueness invariant
//! that the editor's variant picker depends on.

use symbios_overlands::pds::{
    AlphaModeKind, Fp, Fp2, Generator, GeneratorKind, SignSource, SovereignMaterialSettings,
    TextureFilter, limits, sanitize_generator,
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
        texture_filter: TextureFilter::Linear,
    })
}

/// #663 record compat: a pre-filter Sign record (no `texture_filter`
/// key) must deserialize with the Linear default.
#[test]
fn sign_without_filter_field_defaults_to_linear() {
    // Build the wire form of a pre-#663 record: serialize a current Sign
    // and strip the new key — everything else (including the full
    // `material` object every legacy record carries) stays authentic.
    let g = sample_sign(
        SignSource::Url {
            url: "https://example.org/a.png".into(),
        },
        AlphaModeKind::Opaque,
    );
    let mut v = serde_json::to_value(&g.kind).expect("serialize");
    v.as_object_mut()
        .expect("sign serialises to an object")
        .remove("texture_filter")
        .expect("current records carry the filter key");
    let kind: GeneratorKind = serde_json::from_value(v).expect("legacy sign decodes");
    match kind {
        GeneratorKind::Sign { texture_filter, .. } => {
            assert!(matches!(texture_filter, TextureFilter::Linear));
        }
        other => panic!("expected Sign, got {other:?}"),
    }
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

/// #964: a pre-unification Sign's mesh-baked UV window folds into the
/// material's UV transform, exactly. The old spawner sampled
/// `offset + repeat · t` from the vertex UVs; the new one samples
/// `scale · t + scale · material_offset` from a plain `0..1` quad, so a
/// repeat of 2 with an offset of 0.5 must land as scale 2, offset 0.25.
#[test]
fn sanitiser_folds_the_legacy_uv_window_into_the_material() {
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
        *uv_repeat = Fp2([2.0, 2.0]);
        *uv_offset = Fp2([0.5, -0.5]);
    }
    sanitize_generator(&mut g);
    let GeneratorKind::Sign {
        uv_repeat,
        uv_offset,
        material,
        ..
    } = &g.kind
    else {
        panic!("expected Sign after sanitise");
    };
    assert!(
        (material.uv_scale.0 - 2.0).abs() < 1e-3,
        "repeat → uv_scale"
    );
    assert!(
        (material.uv_offset.0[0] - 0.25).abs() < 1e-3,
        "offset / scale"
    );
    assert!((material.uv_offset.0[1] + 0.25).abs() < 1e-3);
    assert_eq!(uv_repeat.0, [1.0, 1.0], "legacy field must reset");
    assert_eq!(uv_offset.0, [0.0, 0.0], "legacy field must reset");
}

/// The fold must be a fixpoint: sanitising twice cannot keep multiplying
/// the scale, or every peer broadcast would shrink the image again.
#[test]
fn folding_the_legacy_uv_window_is_idempotent() {
    let mut g = sample_sign(
        SignSource::Url {
            url: "https://example.org/x.png".into(),
        },
        AlphaModeKind::Opaque,
    );
    if let GeneratorKind::Sign { uv_repeat, .. } = &mut g.kind {
        *uv_repeat = Fp2([3.0, 3.0]);
    }
    sanitize_generator(&mut g);
    let once = g.clone();
    sanitize_generator(&mut g);
    assert_eq!(g, once, "a second sanitise must change nothing");
}

/// A record written after the unification has no `uv_repeat` key at all.
/// Its serde default is the identity, so the fold is a no-op rather than a
/// scale of zero — the failure mode that would blank every new sign.
#[test]
fn a_sign_without_the_legacy_keys_is_untouched_by_the_fold() {
    let json = serde_json::json!({
        "$type": "network.symbios.gen.sign",
        "source": { "$type": "network.symbios.sign.url", "url": "https://example.org/x.png" },
        "size": [20000, 15000],
        "double_sided": false,
        "alpha_mode": { "$type": "network.symbios.alpha.opaque" },
        "unlit": true,
    });
    let mut g = Generator::from_kind(serde_json::from_value(json).expect("decode"));
    sanitize_generator(&mut g);
    let GeneratorKind::Sign { material, .. } = &g.kind else {
        panic!("expected Sign");
    };
    assert!(
        (material.uv_scale.0 - 1.0).abs() < 1e-3,
        "an absent legacy window must leave the scale at 1, got {}",
        material.uv_scale.0
    );
    assert_eq!(material.uv_offset.0, [0.0, 0.0]);
}

/// The legacy window is clamped on the way in, not trusted: a hostile
/// record's infinities must not reach the material as NaN.
#[test]
fn sanitiser_clamps_the_legacy_uv_window_before_folding() {
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
    let GeneratorKind::Sign { material, .. } = &g.kind else {
        panic!("expected Sign after sanitise");
    };
    assert!(material.uv_scale.0.is_finite());
    assert!(material.uv_scale.0 >= limits::MIN_SIGN_UV_REPEAT);
    assert!(material.uv_scale.0 <= limits::MAX_SIGN_UV_REPEAT);
    assert!(material.uv_offset.0.iter().all(|c| c.is_finite()));
    assert!(
        material.uv_offset.0.iter().all(|c| c.abs() <= 1_000.0),
        "material offset clamp"
    );
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

/// #964 wire compat in the other direction: a record we write must keep
/// carrying the legacy keys (at the identity), because a client built
/// before the unification requires them and would fail to decode the whole
/// generator without them.
#[test]
fn a_saved_sign_still_carries_the_legacy_keys_for_old_clients() {
    let mut g = sample_sign(
        SignSource::Url {
            url: "https://example.org/x.png".into(),
        },
        AlphaModeKind::Opaque,
    );
    if let GeneratorKind::Sign { uv_repeat, .. } = &mut g.kind {
        *uv_repeat = Fp2([4.0, 4.0]);
    }
    sanitize_generator(&mut g);
    let wire = serde_json::to_value(&g.kind).expect("encode");
    assert_eq!(
        wire.get("uv_repeat")
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(2),
        "uv_repeat must still be written: {wire}"
    );
    assert!(
        wire.get("uv_offset").is_some(),
        "uv_offset must still be written"
    );
    // At the identity — the fold moved the meaning into the material, and an
    // old client reading this renders the image spanning the panel once.
    assert_eq!(wire["uv_repeat"], serde_json::json!([10000, 10000]));
    assert_eq!(wire["uv_offset"], serde_json::json!([0, 0]));
}
