//! Byte-level wire guard for the Sovereign audio mirror (#1160).
//!
//! The audio mirrors write every field, in declaration order, with no
//! default elision — unlike the texture mirrors (#695). Generators carrying
//! an audio patch are content-addressed over those bytes (room child
//! rkeys), so a mirror that starts eliding, reorders a field, or renames one
//! rewrites every such child record on the next publish.
//!
//! This test pins the bytes of a corpus built through the native
//! `bevy_symbios_audio` types and `from_patch` / `from_sequence`, compared
//! line-for-line against `tests/fixtures/audio_wire.jsonl`. Regenerate the
//! fixture only when the wire form is *meant* to move, with
//! `PRIM_WIRE_BLESS=1`, and say so in the commit.

use std::collections::BTreeMap;
use std::path::PathBuf;

use bevy_symbios_audio::{
    AdsrEnvelope, AudioPatch, BiquadBandpass, BiquadHighpass, BiquadLowpass, BrownNoise, Chorus,
    Connection, Event, Gain, Gate, GraphNode, Instrument, Lfo, Mix, NodeGraph, NodeId, NodeKind,
    PinkNoise, Reverb, SawtoothOsc, SequenceRecipe, SineOsc, SquareOsc, Track, TriangleOsc,
    WhiteNoise,
};
use symbios_overlands::pds::SovereignAudioConfig;
use symbios_overlands::pds::audio::SovereignNodeKind;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/audio_wire.jsonl")
}

/// Every node kind at its upstream default — the roster the mirror must
/// carry one arm for.
fn every_kind() -> Vec<NodeKind> {
    vec![
        NodeKind::Silence,
        NodeKind::Sine(SineOsc::default()),
        NodeKind::Square(SquareOsc::default()),
        NodeKind::Sawtooth(SawtoothOsc::default()),
        NodeKind::Triangle(TriangleOsc::default()),
        NodeKind::WhiteNoise(WhiteNoise::default()),
        NodeKind::PinkNoise(PinkNoise::default()),
        NodeKind::BrownNoise(BrownNoise::default()),
        NodeKind::Adsr(AdsrEnvelope::default()),
        NodeKind::BiquadLowpass(BiquadLowpass::default()),
        NodeKind::BiquadHighpass(BiquadHighpass::default()),
        NodeKind::BiquadBandpass(BiquadBandpass::default()),
        NodeKind::Lfo(Lfo::default()),
        NodeKind::Mix(Mix::default()),
        NodeKind::Gain(Gain::default()),
        NodeKind::Gate(Gate::default()),
        NodeKind::Chorus(Chorus::default()),
        NodeKind::Reverb(Reverb::default()),
    ]
}

/// A patch wiring every kind into one graph, with both connection forms
/// and non-default values on every numeric field of the oscillators, so
/// the pinned bytes cover the whole vocabulary rather than the defaults.
fn everything_patch() -> AudioPatch {
    let mut nodes = Vec::new();
    for (i, kind) in every_kind().into_iter().enumerate() {
        let kind = match kind {
            NodeKind::Sine(_) => NodeKind::Sine(SineOsc {
                freq_hz: 220.5,
                phase_offset: 0.25,
                amplitude: 0.8,
            }),
            NodeKind::Square(mut s) => {
                s.duty = 0.3;
                s.anti_alias = bevy_symbios_audio::AntiAlias::PolyBlep;
                NodeKind::Square(s)
            }
            NodeKind::Sawtooth(mut s) => {
                s.polarity = bevy_symbios_audio::SawPolarity::Down;
                NodeKind::Sawtooth(s)
            }
            NodeKind::Adsr(mut a) => {
                a.curve = bevy_symbios_audio::AdsrCurve::Exponential;
                NodeKind::Adsr(a)
            }
            NodeKind::Lfo(mut l) => {
                l.shape = bevy_symbios_audio::LfoShape::Random;
                NodeKind::Lfo(l)
            }
            NodeKind::Gate(_) => NodeKind::Gate(Gate { invert: true }),
            other => other,
        };
        let mut inputs = BTreeMap::new();
        if i > 0 {
            inputs.insert(
                "in".to_string(),
                vec![
                    Connection::Node {
                        id: NodeId((i - 1) as u32),
                        amount: 0.6,
                    },
                    Connection::Constant { value: 0.4 },
                ],
            );
        }
        nodes.push(GraphNode {
            id: NodeId(i as u32),
            kind,
            inputs,
        });
    }
    let output = NodeId((nodes.len() - 1) as u32);
    AudioPatch {
        graph: NodeGraph { nodes, output },
        ..Default::default()
    }
}

fn corpus() -> Vec<String> {
    let mut out = Vec::new();
    let line = |label: &str, cfg: &SovereignAudioConfig| {
        format!(
            "{label}\t{}",
            serde_json::to_string(cfg).expect("audio config serialises")
        )
    };
    out.push(line("none", &SovereignAudioConfig::None));
    out.push(line(
        "patch/default",
        &SovereignAudioConfig::from_patch(&AudioPatch::default()),
    ));
    for kind in every_kind() {
        let mirror = SovereignNodeKind::from_native(&kind);
        out.push(format!(
            "node/{}\t{}",
            serde_json::to_value(&kind).unwrap()["kind"]
                .as_str()
                .unwrap_or("?"),
            serde_json::to_string(&mirror).expect("node serialises")
        ));
    }
    out.push(line(
        "patch/everything",
        &SovereignAudioConfig::from_patch(&everything_patch()),
    ));
    let recipe = SequenceRecipe {
        bpm: 98.5,
        duration_beats: 8.0,
        loop_start_beats: Some(2.0),
        loop_crossfade_beats: 0.25,
        instruments: vec![Instrument {
            id: "lead".to_string(),
            patch: everything_patch(),
        }],
        tracks: vec![Track {
            events: vec![
                Event {
                    time_beats: 0.0,
                    instrument_id: "lead".to_string(),
                    pitch_multiplier: 1.5,
                    volume: 0.7,
                    gate_beats: 0.5,
                    release_beats: 0.25,
                    pitch_mode: bevy_symbios_audio::PitchMode::TimePreserving,
                },
                Event::default(),
            ],
        }],
        ..Default::default()
    };
    out.push(line(
        "sequence/everything",
        &SovereignAudioConfig::from_sequence(&recipe),
    ));
    out
}

/// The corpus bytes match the blessed fixture line for line.
#[test]
fn audio_wire_bytes_are_pinned() {
    let got = corpus();
    let path = fixture_path();
    if std::env::var_os("PRIM_WIRE_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, got.join("\n") + "\n").expect("write fixture");
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} — bless with PRIM_WIRE_BLESS=1", path.display()));
    let want: Vec<&str> = want.lines().collect();
    let mut diffs = Vec::new();
    for (i, g) in got.iter().enumerate() {
        match want.get(i) {
            Some(w) if *w == g => {}
            Some(w) => diffs.push(format!("line {}:\n  want {w}\n  got  {g}", i + 1)),
            None => diffs.push(format!("line {}: not in fixture\n  got  {g}", i + 1)),
        }
    }
    if want.len() > got.len() {
        diffs.push(format!(
            "fixture has {} lines, corpus {}",
            want.len(),
            got.len()
        ));
    }
    assert!(
        diffs.is_empty(),
        "the audio wire form moved ({} difference(s)); generators carrying audio would \
         re-address on the next publish:\n{}",
        diffs.len(),
        diffs.join("\n")
    );
}

/// Every pinned line decodes back and re-encodes to the same bytes.
#[test]
fn audio_wire_fixture_is_a_fixed_point() {
    for l in corpus() {
        let (label, bytes) = l.split_once('\t').unwrap();
        let again = if label.starts_with("node/") {
            let k: SovereignNodeKind = serde_json::from_str(bytes).expect("node line decodes");
            serde_json::to_string(&k).unwrap()
        } else {
            let c: SovereignAudioConfig = serde_json::from_str(bytes).expect("line decodes");
            serde_json::to_string(&c).unwrap()
        };
        assert_eq!(again, bytes, "{label}: decode→encode is not the identity");
    }
}
