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
        patch: symbios_overlands::pds::audio::SovereignAudioPatch::default(),
    };
    let s = SovereignAudioConfig::Sequence {
        recipe: symbios_overlands::pds::audio::SovereignSequenceRecipe::default(),
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
fn patch_variant_round_trips_as_structured() {
    let original = SovereignAudioConfig::Patch {
        patch: symbios_overlands::pds::audio::SovereignAudioPatch::default(),
    };
    let json = serde_json::to_string(&original).expect("serialise");
    // Structured form — no inner floats at this wire level. The Fp
    // wrapper encodes as a fixed-point integer when its underlying
    // value is non-zero; the empty default produces minimal output.
    let back: SovereignAudioConfig = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(back, original);
}

#[test]
fn sequence_variant_round_trips_as_structured() {
    let original = SovereignAudioConfig::Sequence {
        recipe: symbios_overlands::pds::audio::SovereignSequenceRecipe::default(),
    };
    let json = serde_json::to_string(&original).expect("serialise");
    let back: SovereignAudioConfig = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(back, original);
}

#[test]
fn structured_patch_wire_has_no_inner_floats() {
    // The whole point of #311's structured mirrors over the JSON-stash
    // approach: the wire format carries Fp-encoded integers, never a
    // raw float. Walk the serialised JSON Value and assert that every
    // Number node is integral. (Mirrors the assert_no_floats pattern
    // from tests/pds_records.rs but scoped to the audio config.)
    let mut native = bevy_symbios_audio::AudioPatch::default();
    native.seed = 7;
    // Patch the default Silence node with a SineOsc carrying real
    // floats — exercises the Fp-wrapping path.
    native.graph.nodes[0].kind = bevy_symbios_audio::NodeKind::Sine(bevy_symbios_audio::SineOsc {
        freq_hz: 440.5,
        phase_offset: 0.25,
        amplitude: 0.9,
    });
    let stash = SovereignAudioConfig::from_patch(&native);
    let json: serde_json::Value = serde_json::to_value(&stash).expect("serialise");
    fn walk_no_floats(v: &serde_json::Value, path: &mut Vec<String>) {
        match v {
            serde_json::Value::Number(n) => assert!(
                n.is_i64() || n.is_u64(),
                "Number at /{} is a float ({}); structured wire must be all integers",
                path.join("/"),
                n
            ),
            serde_json::Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    path.push(i.to_string());
                    walk_no_floats(item, path);
                    path.pop();
                }
            }
            serde_json::Value::Object(m) => {
                for (k, v) in m {
                    path.push(k.clone());
                    walk_no_floats(v, path);
                    path.pop();
                }
            }
            _ => {}
        }
    }
    walk_no_floats(&json, &mut Vec::new());
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
    let stash = SovereignAudioConfig::from_patch(&native);
    let recovered = stash.parse_patch().expect("Patch variant returns Some");
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
    let stash = SovereignAudioConfig::from_sequence(&native);
    let recovered = stash
        .parse_sequence()
        .expect("Sequence variant returns Some");
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
