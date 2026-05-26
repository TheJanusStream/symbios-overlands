//! Tests for [`SovereignAudioConfig`] — wire-format round-trip per
//! variant, forward-compat for the `Unknown` seam, native ↔ JSON-stash
//! conversion via [`SovereignAudioConfig::from_patch`] /
//! [`parse_patch`], and the legacy-decode contract that records
//! authored before the audio field existed still decode cleanly.

use symbios_overlands::pds::{SovereignAssetReference, SovereignAudioConfig};

// ---------------------------------------------------------------------------
// Default + label()
// ---------------------------------------------------------------------------

#[test]
fn default_is_none_variant() {
    let a = SovereignAudioConfig::default();
    assert!(matches!(a, SovereignAudioConfig::None));
    assert_eq!(a.label(), "None");
}

#[test]
fn label_returns_distinct_strings_per_variant() {
    let none = SovereignAudioConfig::None;
    let r = SovereignAudioConfig::Referenced {
        source: SovereignAssetReference::default(),
    };
    let p = SovereignAudioConfig::Patch {
        patch_json: String::new(),
    };
    let s = SovereignAudioConfig::Sequence {
        recipe_json: String::new(),
    };
    let u = SovereignAudioConfig::Unknown;
    let labels = [none.label(), r.label(), p.label(), s.label(), u.label()];
    let unique: std::collections::HashSet<_> = labels.iter().collect();
    assert_eq!(
        unique.len(),
        5,
        "every variant must produce a distinct label; got {labels:?}"
    );
}

// ---------------------------------------------------------------------------
// Wire-format round-trip per variant.
// ---------------------------------------------------------------------------

#[test]
fn none_variant_round_trips() {
    let original = SovereignAudioConfig::None;
    let json = serde_json::to_string(&original).expect("serialise");
    let back: SovereignAudioConfig = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(back, original);
}

#[test]
fn referenced_variant_round_trips() {
    let original = SovereignAudioConfig::Referenced {
        source: SovereignAssetReference::Url {
            url: "https://example.org/ambient.ogg".into(),
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
    let back: SovereignAudioConfig = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(back, original);
}

#[test]
fn patch_variant_round_trips_as_opaque_string() {
    let blob = r#"{"seed":42,"graph":{"nodes":[],"output":0}}"#.to_string();
    let original = SovereignAudioConfig::Patch {
        patch_json: blob.clone(),
    };
    let json = serde_json::to_string(&original).expect("serialise");
    // The patch JSON is encapsulated as a string at the DAG-CBOR
    // boundary — the outer document sees one big escaped string, no
    // inner numbers leak through.
    let outer: serde_json::Value = serde_json::from_str(&json).expect("outer value");
    let patch_json_field = outer
        .get("patch_json")
        .and_then(|v| v.as_str())
        .expect("patch_json field is a string");
    assert_eq!(patch_json_field, blob.as_str());
    let back: SovereignAudioConfig = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(back, original);
}

#[test]
fn sequence_variant_round_trips_as_opaque_string() {
    let blob = r#"{"bpm":120.0,"sample_rate":44100,"duration_beats":4.0,
                  "loop_start_beats":null,"loop_crossfade_beats":0.0,
                  "instruments":[],"tracks":[]}"#
        .to_string();
    let original = SovereignAudioConfig::Sequence {
        recipe_json: blob.clone(),
    };
    let json = serde_json::to_string(&original).expect("serialise");
    let back: SovereignAudioConfig = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(back, original);
}

// ---------------------------------------------------------------------------
// Forward-compat: unrecognised $type decodes to Unknown.
// ---------------------------------------------------------------------------

#[test]
fn unknown_type_decodes_to_unknown() {
    let json = r#"{ "$type": "FutureAudioKind", "whatever": 42 }"#;
    let a: SovereignAudioConfig =
        serde_json::from_str(json).expect("unrecognised $type must decode to Unknown");
    assert!(matches!(a, SovereignAudioConfig::Unknown));
}

// ---------------------------------------------------------------------------
// Native ↔ JSON-stash conversion. `from_patch` + `parse_patch` is the
// public contract baking consumers will use.
// ---------------------------------------------------------------------------

#[test]
fn from_patch_round_trips_through_native_audio_patch() {
    let native = bevy_symbios_audio::AudioPatch {
        seed: 7,
        graph: bevy_symbios_audio::NodeGraph::default(),
    };
    let stash = SovereignAudioConfig::from_patch(&native).expect("stash");
    let recovered = stash
        .parse_patch()
        .expect("Patch variant returns Some")
        .expect("parsed cleanly");
    assert_eq!(recovered, native);
}

#[test]
fn parse_patch_returns_none_for_other_variants() {
    assert!(SovereignAudioConfig::None.parse_patch().is_none());
    assert!(
        SovereignAudioConfig::Referenced {
            source: SovereignAssetReference::default(),
        }
        .parse_patch()
        .is_none()
    );
    assert!(SovereignAudioConfig::Unknown.parse_patch().is_none());
}

#[test]
fn from_sequence_round_trips_through_native_recipe() {
    let native = bevy_symbios_audio::SequenceRecipe::default();
    let stash = SovereignAudioConfig::from_sequence(&native).expect("stash");
    let recovered = stash
        .parse_sequence()
        .expect("Sequence variant returns Some")
        .expect("parsed cleanly");
    assert_eq!(recovered, native);
}

// ---------------------------------------------------------------------------
// Legacy-decode contract. Records authored before the ambient_audio
// field existed (i.e. every published room record today) must still
// deserialise. The field on `Environment` uses `#[serde(default)]` on
// the parent struct, so missing field → default = None.
// ---------------------------------------------------------------------------

#[test]
fn environment_without_ambient_audio_field_decodes() {
    use symbios_overlands::pds::Environment;
    // An empty object is the worst-case "every field elided" record —
    // legacy decoders for older clients on newer-server records hit
    // this same path.
    let env: Environment = serde_json::from_str("{}").expect("legacy decode");
    assert!(matches!(env.ambient_audio, SovereignAudioConfig::None));
}
