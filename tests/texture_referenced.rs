//! Tests for the [`SovereignTextureConfig::Referenced`] variant — wire
//! round-trip, the dropdown label, default behaviour, and the fallback
//! contract that `to_texture_config()` collapses Referenced to
//! `TextureConfig::None` (the actual image is painted in by a separate
//! resolver after fetch).
//!
//! The inner [`SovereignAssetReference`] sanitiser is covered end-to-end
//! by `tests/asset_reference.rs`; this file focuses on the texture-side
//! plumbing only.

use symbios_overlands::pds::{SovereignAssetReference, SovereignTextureConfig};

// ---------------------------------------------------------------------------
// Wire-format round-trip per inner-source variant.
// ---------------------------------------------------------------------------

#[test]
fn referenced_variant_round_trips_with_url_source() {
    let original = SovereignTextureConfig::Referenced {
        source: SovereignAssetReference::Url {
            url: "https://example.org/textures/cobble.png".into(),
        },
    };
    let json = serde_json::to_string(&original).expect("serialise");
    assert!(
        json.contains("\"Referenced\""),
        "$type tag must mention `Referenced`; got {json}"
    );
    assert!(
        json.contains("\"network.symbios.sign.url\""),
        "inner asset-reference wire tag must be preserved; got {json}"
    );
    let back: SovereignTextureConfig = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(back, original);
}

#[test]
fn referenced_variant_round_trips_with_atproto_blob_source() {
    let original = SovereignTextureConfig::Referenced {
        source: SovereignAssetReference::AtprotoBlob {
            did: "did:plc:abc".into(),
            cid: "bafyreigh2akiscaildc5ssia2y3yqomyrnf2c2v3uoxvw7xj3xq5nz4ucy".into(),
        },
    };
    let json = serde_json::to_string(&original).expect("serialise");
    let back: SovereignTextureConfig = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(back, original);
}

#[test]
fn referenced_variant_round_trips_with_did_pfp_source() {
    let original = SovereignTextureConfig::Referenced {
        source: SovereignAssetReference::DidPfp {
            did: "did:plc:abc".into(),
        },
    };
    let json = serde_json::to_string(&original).expect("serialise");
    let back: SovereignTextureConfig = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(back, original);
}

// ---------------------------------------------------------------------------
// Label — exercised by the texture-bridge dropdown.
// ---------------------------------------------------------------------------

#[test]
fn referenced_label_is_distinct() {
    let r = SovereignTextureConfig::Referenced {
        source: SovereignAssetReference::default(),
    };
    assert_eq!(r.label(), "Referenced");
    // Sanity-check against neighbouring variants so the new arm doesn't
    // accidentally re-use an existing label.
    assert_ne!(r.label(), SovereignTextureConfig::None.label());
    assert_ne!(r.label(), SovereignTextureConfig::Unknown.label());
}

// ---------------------------------------------------------------------------
// Default still returns None — Referenced is opt-in. A user choosing it
// from the dropdown gets a fresh empty SovereignAssetReference::Url which
// the UI can then edit (asset-reference UI ticket).
// ---------------------------------------------------------------------------

#[test]
fn default_still_returns_none_variant() {
    let d = SovereignTextureConfig::default();
    assert!(matches!(d, SovereignTextureConfig::None));
}

// ---------------------------------------------------------------------------
// to_texture_config: Referenced collapses to TextureConfig::None for the
// procedural-bake bridge. The actual handle is supplied by the resolver
// path (BlobImageCache), not by the procedural builder.
// ---------------------------------------------------------------------------

#[test]
fn referenced_to_texture_config_collapses_to_none() {
    let r = SovereignTextureConfig::Referenced {
        source: SovereignAssetReference::Url {
            url: "https://example.org/x.png".into(),
        },
    };
    let bridged = r.to_texture_config();
    assert!(matches!(bridged, bevy_symbios_texture::TextureConfig::None));
}

// ---------------------------------------------------------------------------
// Forward-compat: serialised records authored before the Referenced variant
// existed (i.e. every existing room record) still deserialise. The new
// variant is purely additive.
// ---------------------------------------------------------------------------

#[test]
fn legacy_ground_record_still_decodes() {
    // Ground is a representative existing variant; if it round-trips we
    // can be confident the Referenced addition didn't shift the wire
    // format of any sibling. Numeric fields use the Fp/Fp64 fixed-point
    // wire format (integer ticks of FP_SCALE = 10_000), matching what
    // every published room record actually contains.
    let json = r#"{
        "$type": "Ground",
        "seed": 13,
        "macro_scale": 20000,
        "macro_octaves": 5,
        "micro_scale": 80000,
        "micro_octaves": 4,
        "micro_weight": 3500,
        "color_dry": [5000, 4000, 3000],
        "color_moist": [3000, 2000, 1000],
        "normal_strength": 20000
    }"#;
    let r: SovereignTextureConfig = serde_json::from_str(json).expect("legacy record decode");
    assert!(matches!(r, SovereignTextureConfig::Ground(_)));
}
