//! Unit tests for [`SovereignAssetReference`] — the canonical "URL or DID"
//! asset pointer that replaces the original `SignSource` enum. These tests
//! pin the wire-format compatibility (the `$type` tags must remain
//! `network.symbios.sign.*` so already-published room records keep
//! deserialising), the forward-compat `Unknown` seam, and the type-alias
//! contract that lets existing call sites keep using the `SignSource`
//! name.

use symbios_overlands::pds::{SignSource, SovereignAssetReference};

// ---------------------------------------------------------------------------
// Default + label() — exercised by the dropdown bridge UI.
// ---------------------------------------------------------------------------

#[test]
fn default_is_empty_url() {
    let r = SovereignAssetReference::default();
    assert!(
        matches!(&r, SovereignAssetReference::Url { url } if url.is_empty()),
        "default must be an empty Url so the bridge UI can render an editor for the new reference"
    );
}

#[test]
fn label_returns_distinct_strings_per_variant() {
    let url = SovereignAssetReference::Url {
        url: String::from("https://example.org/x.png"),
    };
    let blob = SovereignAssetReference::AtprotoBlob {
        did: "did:plc:abc".into(),
        cid: "bafy...".into(),
    };
    let pfp = SovereignAssetReference::DidPfp {
        did: "did:plc:abc".into(),
    };
    let unknown = SovereignAssetReference::Unknown;
    let labels = [url.label(), blob.label(), pfp.label(), unknown.label()];
    let unique: std::collections::HashSet<_> = labels.iter().collect();
    assert_eq!(
        unique.len(),
        4,
        "every variant must produce a distinct label; got {labels:?}"
    );
}

// ---------------------------------------------------------------------------
// Wire-format round-trip per variant. The $type tags are stable.
// ---------------------------------------------------------------------------

#[test]
fn url_variant_round_trips() {
    let original = SovereignAssetReference::Url {
        url: "https://example.org/banner.png".into(),
    };
    let json = serde_json::to_string(&original).expect("serialise");
    assert!(
        json.contains("\"network.symbios.sign.url\""),
        "wire tag must remain `network.symbios.sign.url` for backwards compatibility; got {json}"
    );
    let back: SovereignAssetReference = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(back, original);
}

#[test]
fn atproto_blob_variant_round_trips() {
    let original = SovereignAssetReference::AtprotoBlob {
        did: "did:plc:abc".into(),
        cid: "bafyreigh2akiscaildc5ssia2y3yqomyrnf2c2v3uoxvw7xj3xq5nz4ucy".into(),
    };
    let json = serde_json::to_string(&original).expect("serialise");
    assert!(
        json.contains("\"network.symbios.sign.atproto_blob\""),
        "wire tag must remain `network.symbios.sign.atproto_blob`; got {json}"
    );
    let back: SovereignAssetReference = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(back, original);
}

#[test]
fn did_pfp_variant_round_trips() {
    let original = SovereignAssetReference::DidPfp {
        did: "did:plc:abc".into(),
    };
    let json = serde_json::to_string(&original).expect("serialise");
    assert!(
        json.contains("\"network.symbios.sign.did_pfp\""),
        "wire tag must remain `network.symbios.sign.did_pfp`; got {json}"
    );
    let back: SovereignAssetReference = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(back, original);
}

// ---------------------------------------------------------------------------
// Forward-compat: unrecognised $type decodes to Unknown rather than failing.
// ---------------------------------------------------------------------------

#[test]
fn unknown_type_decodes_to_unknown() {
    let json = r#"{ "$type": "network.symbios.sign.future_kind_2030", "weird_field": 42 }"#;
    let r: SovereignAssetReference =
        serde_json::from_str(json).expect("unrecognised $type must decode to Unknown, not error");
    assert!(matches!(r, SovereignAssetReference::Unknown));
}

// ---------------------------------------------------------------------------
// Type-alias contract: `SignSource` and `SovereignAssetReference` are
// interchangeable so existing call sites keep working without a touch.
// ---------------------------------------------------------------------------

#[test]
fn sign_source_alias_is_the_same_type() {
    // If SignSource were a distinct type rather than a `pub use` re-export,
    // this assignment would fail to compile. The test is therefore a
    // build-time contract; the runtime asserts are just belt-and-braces.
    let via_alias: SignSource = SignSource::Url {
        url: "https://example.org/x".into(),
    };
    let via_canonical: SovereignAssetReference = via_alias.clone();
    assert_eq!(via_alias, via_canonical);
}

// ---------------------------------------------------------------------------
// Records authored before the rename must still deserialise. The $type tags
// are the only wire-format guarantee we need — every published room record
// carrying a Sign panel uses one of these three tags.
// ---------------------------------------------------------------------------

#[test]
fn pre_rename_url_record_still_decodes() {
    let json =
        r#"{ "$type": "network.symbios.sign.url", "url": "https://legacy.example/banner.jpg" }"#;
    let r: SovereignAssetReference =
        serde_json::from_str(json).expect("legacy wire format must decode");
    assert_eq!(
        r,
        SovereignAssetReference::Url {
            url: "https://legacy.example/banner.jpg".into(),
        }
    );
}

#[test]
fn pre_rename_atproto_blob_record_still_decodes() {
    let json = r#"{
        "$type": "network.symbios.sign.atproto_blob",
        "did": "did:plc:legacy",
        "cid": "bafyrei..."
    }"#;
    let r: SovereignAssetReference =
        serde_json::from_str(json).expect("legacy wire format must decode");
    assert_eq!(
        r,
        SovereignAssetReference::AtprotoBlob {
            did: "did:plc:legacy".into(),
            cid: "bafyrei...".into(),
        }
    );
}
