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
    let mut native = bevy_symbios_audio::AudioPatch {
        seed: 7,
        ..Default::default()
    };
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

// ---------------------------------------------------------------------------
// bevy_symbios_audio 0.2 nodes + fields (#314): the five new NodeKind
// variants (Mix / Gain / Gate / Chorus / Reverb) and the two new schema
// fields (osc anti_alias, event pitch_mode) must round-trip losslessly
// and stay float-free on the wire.
// ---------------------------------------------------------------------------

/// Build a native patch that exercises every new 0.2 node kind plus the
/// oscillator `anti_alias` field. Topology is irrelevant to the mirror
/// round-trip; it just has to carry one of each config.
fn patch_with_all_new_nodes() -> bevy_symbios_audio::AudioPatch {
    use bevy_symbios_audio::{
        AntiAlias, Chorus, Gain, Gate, GraphNode, Mix, NodeGraph, NodeId, NodeKind, Reverb,
        SquareOsc,
    };
    let node = |id: u32, kind: NodeKind| GraphNode {
        id: NodeId(id),
        kind,
        inputs: Default::default(),
    };
    NodeGraph {
        nodes: vec![
            node(
                0,
                NodeKind::Square(SquareOsc {
                    freq_hz: 220.5,
                    duty: 0.3,
                    amplitude: 0.8,
                    anti_alias: AntiAlias::PolyBlep,
                }),
            ),
            node(1, NodeKind::Mix(Mix { gain: 0.75 })),
            node(2, NodeKind::Gain(Gain { gain: 0.5 })),
            node(3, NodeKind::Gate(Gate { invert: true })),
            node(
                4,
                NodeKind::Chorus(Chorus {
                    rate_hz: 1.2,
                    depth_ms: 3.5,
                    base_delay_ms: 9.0,
                    feedback: 0.4,
                    mix: 0.6,
                }),
            ),
            node(
                5,
                NodeKind::Reverb(Reverb {
                    room_size: 0.7,
                    damping: 0.3,
                    mix: 0.25,
                }),
            ),
        ],
        output: NodeId(5),
    }
    .into_patch()
}

/// Tiny helper trait so the fixture above reads cleanly.
trait IntoPatch {
    fn into_patch(self) -> bevy_symbios_audio::AudioPatch;
}
impl IntoPatch for bevy_symbios_audio::NodeGraph {
    fn into_patch(self) -> bevy_symbios_audio::AudioPatch {
        bevy_symbios_audio::AudioPatch {
            seed: 3,
            graph: self,
        }
    }
}

#[test]
fn new_node_kinds_round_trip_through_native() {
    let native = patch_with_all_new_nodes();
    let stash = SovereignAudioConfig::from_patch(&native);
    let recovered = stash.parse_patch().expect("Patch variant returns Some");
    assert_eq!(recovered, native, "all 0.2 node kinds must round-trip");
}

#[test]
fn new_node_kinds_have_no_inner_floats_on_wire() {
    let native = patch_with_all_new_nodes();
    let stash = SovereignAudioConfig::from_patch(&native);
    let json: serde_json::Value = serde_json::to_value(&stash).expect("serialise");
    fn walk(v: &serde_json::Value, path: &mut Vec<String>) {
        match v {
            serde_json::Value::Number(n) => assert!(
                n.is_i64() || n.is_u64(),
                "Number at /{} is a float ({n}); wire must be all integers",
                path.join("/"),
            ),
            serde_json::Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    path.push(i.to_string());
                    walk(item, path);
                    path.pop();
                }
            }
            serde_json::Value::Object(m) => {
                for (k, v) in m {
                    path.push(k.clone());
                    walk(v, path);
                    path.pop();
                }
            }
            _ => {}
        }
    }
    walk(&json, &mut Vec::new());
}

#[test]
fn event_pitch_mode_round_trips() {
    use bevy_symbios_audio::{AudioPatch, Event, Instrument, PitchMode, SequenceRecipe, Track};
    let native = SequenceRecipe {
        instruments: vec![Instrument {
            id: "v".into(),
            patch: AudioPatch::default(),
        }],
        tracks: vec![Track {
            events: vec![Event {
                instrument_id: "v".into(),
                pitch_multiplier: 1.5,
                pitch_mode: PitchMode::TimePreserving,
                ..Default::default()
            }],
        }],
        ..Default::default()
    };
    let stash = SovereignAudioConfig::from_sequence(&native);
    let recovered = stash.parse_sequence().expect("Sequence returns Some");
    assert_eq!(recovered, native, "pitch_mode must survive round-trip");
}

#[test]
fn osc_without_anti_alias_field_decodes_to_naive() {
    // A square-osc node authored before the anti_alias field existed has
    // no such key; #[serde(default)] must land it on Naive so old room
    // records bake byte-for-byte as before.
    use symbios_overlands::pds::audio::{SovereignAntiAlias, SovereignNodeKind};
    let json = r#"{ "kind": "Square", "freq_hz": 0, "duty": 0, "amplitude": 0 }"#;
    let k: SovereignNodeKind = serde_json::from_str(json).expect("legacy square decode");
    let SovereignNodeKind::Square(c) = k else {
        panic!("expected Square variant");
    };
    assert_eq!(c.anti_alias, SovereignAntiAlias::Naive);
}

#[test]
fn event_without_pitch_mode_field_decodes_to_varispeed() {
    use symbios_overlands::pds::audio::{SovereignEvent, SovereignPitchMode};
    // An event authored before pitch_mode existed (no such key).
    let json = r#"{ "time_beats": 0, "instrument_id": "v", "pitch_multiplier": 0,
                    "volume": 0, "gate_beats": 0 }"#;
    let e: SovereignEvent = serde_json::from_str(json).expect("legacy event decode");
    assert_eq!(e.pitch_mode, SovereignPitchMode::Varispeed);
}

#[test]
fn unknown_node_kind_decodes_to_unknown_variant() {
    // Forward-compat: a node kind from a future crate version that this
    // mirror doesn't know must decode to Unknown (→ Silence on bake),
    // not fail the whole record.
    use symbios_overlands::pds::audio::SovereignNodeKind;
    let json = r#"{ "kind": "FutureFilter", "cutoff": 1 }"#;
    let k: SovereignNodeKind = serde_json::from_str(json).expect("unknown kind → Unknown");
    assert!(matches!(k, SovereignNodeKind::Unknown));
}
