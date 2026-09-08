//! Sanitiser for [`SovereignAudioConfig`] and its structured node /
//! sequence mirror types. Clamps node-count and event-count budgets so
//! a hostile peer can't smuggle a million-element graph through a
//! room recipe; per-config-field numeric clamps live inside the
//! variant arms.
//!
//! The Referenced variant forwards to the asset-reference sanitiser
//! for URL / DID / CID length caps.

use super::Sanitize;
use crate::pds::audio::{
    SovereignAudioConfig, SovereignAudioPatch, SovereignConnection, SovereignEvent,
    SovereignNodeGraph, SovereignNodeKind, SovereignSequenceRecipe, SovereignTrack,
};
use crate::pds::types::{Fp, truncate_on_char_boundary};

/// Soft cap on the total number of nodes a single
/// [`SovereignNodeGraph`] may carry. A graph this size already bakes
/// for tens of seconds at the audio crate's evaluation rate; anything
/// past this is overwhelmingly more likely to be an attack than a
/// legitimate sound design choice.
pub const MAX_AUDIO_NODES: usize = 256;

/// Soft cap on per-instrument-track event count. Events compound in
/// the mixdown baker (one bake per unique `(instrument, gate)`), so an
/// unbounded list amplifies the bake cost quadratically.
pub const MAX_TRACK_EVENTS: usize = 4096;

/// Cap on the number of instruments in a sequence recipe — the inner
/// AudioPatch on each one is already bounded by [`MAX_AUDIO_NODES`].
pub const MAX_SEQUENCE_INSTRUMENTS: usize = 64;

/// Cap on the number of tracks in a sequence recipe.
pub const MAX_SEQUENCE_TRACKS: usize = 64;

/// Cap on the length (bytes) of an [`SovereignEvent::instrument_id`]
/// string. Aligns with the L-system code cap order of magnitude.
pub const MAX_INSTRUMENT_ID_BYTES: usize = 128;

/// Cap on the number of connections wired into a single input port.
/// Ports sum their connections, so a realistic port holds one signal
/// plus a handful of modulators; a million-element array is an attack,
/// not sound design. Aligns with the other per-collection caps above.
pub const MAX_CONNECTIONS_PER_PORT: usize = 64;

/// Clamp `v` to `[lo, hi]`, replacing NaN/Inf with `default`.
fn clamp_finite(v: f32, lo: f32, hi: f32, default: f32) -> f32 {
    if v.is_finite() {
        v.clamp(lo, hi)
    } else {
        default
    }
}

impl Sanitize for SovereignAudioConfig {
    fn sanitize(&mut self) {
        match self {
            SovereignAudioConfig::None | SovereignAudioConfig::Unknown => {}
            SovereignAudioConfig::Referenced { source } => source.sanitize(),
            SovereignAudioConfig::Patch { patch } => patch.sanitize(),
            SovereignAudioConfig::Sequence { recipe } => recipe.sanitize(),
        }
    }
}

impl Sanitize for SovereignAudioPatch {
    fn sanitize(&mut self) {
        self.graph.sanitize();
    }
}

impl Sanitize for SovereignNodeGraph {
    fn sanitize(&mut self) {
        // Cap node count first so the per-node sanitiser doesn't walk
        // an attacker-supplied giant list. Truncates from the tail
        // because the head usually carries the wired output node.
        if self.nodes.len() > MAX_AUDIO_NODES {
            self.nodes.truncate(MAX_AUDIO_NODES);
        }
        for node in &mut self.nodes {
            node.kind.sanitize();
            // Each port now holds a list of connections (summed at bake
            // time); cap the per-port count before walking it so a
            // hostile array can't blow up the bake.
            for connections in node.inputs.values_mut() {
                if connections.len() > MAX_CONNECTIONS_PER_PORT {
                    connections.truncate(MAX_CONNECTIONS_PER_PORT);
                }
                for connection in connections.iter_mut() {
                    connection.sanitize();
                }
            }
        }
    }
}

impl Sanitize for SovereignNodeKind {
    fn sanitize(&mut self) {
        // Per-config numeric clamps. The bounds mirror what the audio
        // crate's own runtime clamps would do (filter.rs::clamp_cutoff
        // is f32::EPSILON..sample_rate/2; we cap more conservatively
        // here to defuse hostile records before the audio worker even
        // sees them).
        match self {
            Self::Silence | Self::Unknown => {}
            Self::Sine(c) => {
                c.freq_hz = Fp(clamp_finite(c.freq_hz.0, 0.0, 22_050.0, 440.0));
                c.phase_offset = Fp(clamp_finite(c.phase_offset.0, -1.0, 1.0, 0.0));
                c.amplitude = Fp(clamp_finite(c.amplitude.0, -8.0, 8.0, 1.0));
            }
            Self::Square(c) => {
                c.freq_hz = Fp(clamp_finite(c.freq_hz.0, 0.0, 22_050.0, 440.0));
                c.duty = Fp(clamp_finite(c.duty.0, 0.0, 1.0, 0.5));
                c.amplitude = Fp(clamp_finite(c.amplitude.0, -8.0, 8.0, 1.0));
            }
            Self::Sawtooth(c) => {
                c.freq_hz = Fp(clamp_finite(c.freq_hz.0, 0.0, 22_050.0, 440.0));
                c.amplitude = Fp(clamp_finite(c.amplitude.0, -8.0, 8.0, 1.0));
            }
            Self::Triangle(c) => {
                c.freq_hz = Fp(clamp_finite(c.freq_hz.0, 0.0, 22_050.0, 440.0));
                c.amplitude = Fp(clamp_finite(c.amplitude.0, -8.0, 8.0, 1.0));
            }
            Self::WhiteNoise(c) => {
                c.amplitude = Fp(clamp_finite(c.amplitude.0, -8.0, 8.0, 0.5));
            }
            Self::PinkNoise(c) => {
                c.amplitude = Fp(clamp_finite(c.amplitude.0, -8.0, 8.0, 0.5));
            }
            Self::BrownNoise(c) => {
                c.amplitude = Fp(clamp_finite(c.amplitude.0, -8.0, 8.0, 0.5));
            }
            Self::Adsr(c) => {
                c.attack_s = Fp(clamp_finite(c.attack_s.0, 0.0, 60.0, 0.01));
                c.decay_s = Fp(clamp_finite(c.decay_s.0, 0.0, 60.0, 0.1));
                c.sustain_level = Fp(clamp_finite(c.sustain_level.0, 0.0, 1.0, 0.7));
                c.release_s = Fp(clamp_finite(c.release_s.0, 0.0, 60.0, 0.2));
            }
            Self::BiquadLowpass(c) => {
                c.cutoff_hz = Fp(clamp_finite(c.cutoff_hz.0, 1.0, 22_050.0, 1_000.0));
                c.q = Fp(clamp_finite(c.q.0, 0.001, 64.0, 0.707));
            }
            Self::BiquadHighpass(c) => {
                c.cutoff_hz = Fp(clamp_finite(c.cutoff_hz.0, 1.0, 22_050.0, 1_000.0));
                c.q = Fp(clamp_finite(c.q.0, 0.001, 64.0, 0.707));
            }
            Self::BiquadBandpass(c) => {
                c.center_hz = Fp(clamp_finite(c.center_hz.0, 1.0, 22_050.0, 1_000.0));
                c.q = Fp(clamp_finite(c.q.0, 0.001, 64.0, 1.0));
            }
            Self::Lfo(c) => {
                c.rate_hz = Fp(clamp_finite(c.rate_hz.0, 0.0, 1_000.0, 1.0));
                c.depth = Fp(clamp_finite(c.depth.0, -10_000.0, 10_000.0, 1.0));
                c.offset = Fp(clamp_finite(c.offset.0, -10_000.0, 10_000.0, 0.0));
            }
            // Combiners — gain is a plain multiplier; bound it well past
            // unity but away from the float rails so a hostile value
            // can't drive the summed bus to NaN/Inf.
            Self::Mix(c) => {
                c.gain = Fp(clamp_finite(c.gain.0, -64.0, 64.0, 1.0));
            }
            Self::Gain(c) => {
                c.gain = Fp(clamp_finite(c.gain.0, -64.0, 64.0, 1.0));
            }
            // Gate carries only a bool — nothing to clamp.
            Self::Gate(_) => {}
            Self::Chorus(c) => {
                c.rate_hz = Fp(clamp_finite(c.rate_hz.0, 0.0, 100.0, 0.8));
                // Delay times drive the ring-buffer size; cap so a giant
                // value can't allocate an absurd buffer at bake time.
                c.depth_ms = Fp(clamp_finite(c.depth_ms.0, 0.0, 100.0, 2.0));
                c.base_delay_ms = Fp(clamp_finite(c.base_delay_ms.0, 0.0, 100.0, 8.0));
                // The crate clamps feedback below 1.0 internally; mirror
                // that ceiling here so the line stays contractive.
                c.feedback = Fp(clamp_finite(c.feedback.0, 0.0, 0.95, 0.0));
                c.mix = Fp(clamp_finite(c.mix.0, 0.0, 1.0, 0.5));
            }
            Self::Reverb(c) => {
                c.room_size = Fp(clamp_finite(c.room_size.0, 0.0, 1.0, 0.5));
                c.damping = Fp(clamp_finite(c.damping.0, 0.0, 1.0, 0.5));
                c.mix = Fp(clamp_finite(c.mix.0, 0.0, 1.0, 0.3));
            }
        }
    }
}

/// Bound on a connection's DC value and on a node connection's `amount` —
/// the same number `symbios_audio::envelope`'s `MAX_CONNECTION_MAGNITUDE`
/// carries, and the drift guard below asserts the two agree.
///
/// It was `1_000_000.0` until #1316, which is 4.6x more than [`Fp`] can
/// represent: the wire form is `(v * FP_SCALE).round() as i32`, a cast that
/// **saturates**, so a value the sanitiser accepted at 500_000 came back from
/// the wire as 214_748.3647 — silently, and leaving `sanitize` non-idempotent
/// across a round trip for this one field. It was also the only bound in the
/// table unrelated to its neighbours: the combiner gains are ±64 and the
/// widest other bound is an LFO depth at ±10_000.
pub const MAX_CONNECTION_MAGNITUDE: f32 = 100_000.0;

// The proof, at compile time, that this sanitiser cannot accept a value the
// writer cannot store. That is the property #1316 was actually about — the
// number itself is only as good as the thing keeping it honest, and a
// comment would not have caught the original.
const _: () = assert!(
    (MAX_CONNECTION_MAGNITUDE as f64 * crate::pds::types::FP_SCALE as f64) <= i32::MAX as f64,
    "MAX_CONNECTION_MAGNITUDE does not survive Fp's i32 wire form — see #1316"
);

impl Sanitize for SovereignConnection {
    fn sanitize(&mut self) {
        match self {
            SovereignConnection::Constant { value } => {
                value.0 = clamp_finite(
                    value.0,
                    -MAX_CONNECTION_MAGNITUDE,
                    MAX_CONNECTION_MAGNITUDE,
                    0.0,
                );
            }
            SovereignConnection::Node { amount, .. } => {
                amount.0 = clamp_finite(
                    amount.0,
                    -MAX_CONNECTION_MAGNITUDE,
                    MAX_CONNECTION_MAGNITUDE,
                    1.0,
                );
            }
            // No native counterpart — `Connection` is not `#[non_exhaustive]`
            // upstream, so this arm is a read-side seam only and there is
            // nothing to bound.
            SovereignConnection::Unknown => {}
        }
    }
}

impl Sanitize for SovereignSequenceRecipe {
    fn sanitize(&mut self) {
        self.bpm = Fp(clamp_finite(self.bpm.0, 1.0, 1_000.0, 120.0));
        // sample_rate is u32 so finiteness is guaranteed; clamp to a
        // reasonable audio range nonetheless.
        self.sample_rate = self.sample_rate.clamp(8_000, 192_000);
        self.duration_beats = Fp(clamp_finite(self.duration_beats.0, 0.0, 100_000.0, 4.0));
        if let Some(ref mut loop_start) = self.loop_start_beats {
            loop_start.0 = clamp_finite(loop_start.0, 0.0, self.duration_beats.0.max(0.0), 0.0);
        }
        self.loop_crossfade_beats = Fp(clamp_finite(
            self.loop_crossfade_beats.0,
            0.0,
            self.duration_beats.0.max(0.0),
            0.0,
        ));
        if self.instruments.len() > MAX_SEQUENCE_INSTRUMENTS {
            self.instruments.truncate(MAX_SEQUENCE_INSTRUMENTS);
        }
        for instr in &mut self.instruments {
            truncate_on_char_boundary(&mut instr.id, MAX_INSTRUMENT_ID_BYTES);
            instr.patch.sanitize();
        }
        if self.tracks.len() > MAX_SEQUENCE_TRACKS {
            self.tracks.truncate(MAX_SEQUENCE_TRACKS);
        }
        for track in &mut self.tracks {
            track.sanitize();
        }
    }
}

impl Sanitize for SovereignTrack {
    fn sanitize(&mut self) {
        if self.events.len() > MAX_TRACK_EVENTS {
            self.events.truncate(MAX_TRACK_EVENTS);
        }
        for event in &mut self.events {
            event.sanitize();
        }
    }
}

impl Sanitize for SovereignEvent {
    fn sanitize(&mut self) {
        self.time_beats = Fp(clamp_finite(self.time_beats.0, 0.0, 100_000.0, 0.0));
        truncate_on_char_boundary(&mut self.instrument_id, MAX_INSTRUMENT_ID_BYTES);
        // Pitch multiplier is continuous (see audio crate's sequence
        // module docstring) — not clamped to semitones. Bound below
        // away from zero so playback speed doesn't degenerate.
        self.pitch_multiplier = Fp(clamp_finite(self.pitch_multiplier.0, 0.001, 64.0, 1.0));
        self.volume = Fp(clamp_finite(self.volume.0, 0.0, 1.0, 1.0));
        self.gate_beats = Fp(clamp_finite(self.gate_beats.0, 0.0, 100_000.0, 1.0));
        // Release tail bakes extra samples after the gate closes; bound
        // it like gate_beats so a huge value can't balloon the bake.
        self.release_beats = Fp(clamp_finite(self.release_beats.0, 0.0, 100_000.0, 0.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::audio::{
        SovereignChorus, SovereignGain, SovereignMix, SovereignNodeKind, SovereignReverb,
    };
    use bevy_symbios_audio::{ClampToEnvelope, Envelope};

    // -----------------------------------------------------------------
    // Drift guard: this sanitiser and symbios-audio's `Envelope` are two
    // implementations of one table (#1305)
    // -----------------------------------------------------------------

    /// Replace every **floating-point** leaf of `value` with `fill`.
    ///
    /// Integers are left alone deliberately: `sample_rate` is a `u32` and
    /// `NodeId` a transparent `u32`, and neither can decode a value chosen to
    /// overflow an `f32`. `serde_json` distinguishes the two, and every field
    /// these clamps touch is a float.
    fn fill_floats(value: &mut serde_json::Value, fill: f64) {
        match value {
            serde_json::Value::Number(n) if n.is_f64() => {
                *value = serde_json::json!(fill);
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(|v| fill_floats(v, fill)),
            serde_json::Value::Object(map) => {
                map.values_mut().for_each(|v| fill_floats(v, fill));
            }
            _ => {}
        }
    }

    /// Every field of `T` set to `fill`, via its own serde representation.
    ///
    /// This is what makes the guard below *total* rather than a spot check:
    /// no per-kind hostile constructor to write, and no list of field names
    /// to keep in step with upstream.
    fn hostile<T>(value: &T, fill: f64) -> T
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
    {
        let mut json = serde_json::to_value(value).expect("config serialises");
        fill_floats(&mut json, fill);
        serde_json::from_value(json).expect("config decodes")
    }

    /// The fills, and what each one is for.
    ///
    /// `±1e300` is the interesting one: JSON cannot carry NaN, but serde
    /// decodes a number into `f32` by casting the parsed `f64`, and a
    /// float-to-float cast **saturates**, so `1e300` arrives as
    /// `f32::INFINITY`. That reaches the same non-finite branch of
    /// `clamp_finite` that NaN does — the branch that resolves to the
    /// *field's default* — so the default column of the table is covered
    /// generically, which is where the `q = 0.707` vs `FRAC_1_SQRT_2` drift
    /// lived (#1160). The finite fills cover the `lo`/`hi` columns.
    const FILLS: [f64; 5] = [1e300, -1e300, 1e9, -1e9, 0.0];

    #[test]
    fn the_infinity_injection_actually_injects_infinity() {
        // The premise of every fill below. If serde ever starts rejecting an
        // out-of-range float instead of saturating, the guard would silently
        // stop testing the default column, so it is asserted rather than
        // assumed.
        let v: f32 = serde_json::from_value(serde_json::json!(1e300)).expect("decodes");
        assert!(v.is_infinite() && v.is_sign_positive());
        let v: f32 = serde_json::from_value(serde_json::json!(-1e300)).expect("decodes");
        assert!(v.is_infinite() && v.is_sign_negative());
        // And the integer fields really are distinguishable, so `fill_floats`
        // can leave them alone.
        assert!(serde_json::to_value(0.0f32).expect("ok").is_f64());
        assert!(!serde_json::to_value(44_100u32).expect("ok").is_f64());
    }

    /// The six collection caps are the same numbers on both sides.
    ///
    /// They are the half of the envelope that is *named* in two places, so
    /// they are the half that can drift by a plain edit.
    #[test]
    fn the_caps_match_the_upstream_envelope() {
        let e = Envelope::default();
        assert_eq!(MAX_AUDIO_NODES, e.max_nodes);
        assert_eq!(MAX_CONNECTIONS_PER_PORT, e.max_connections_per_port);
        assert_eq!(MAX_TRACK_EVENTS, e.max_track_events);
        assert_eq!(MAX_SEQUENCE_INSTRUMENTS, e.max_instruments);
        assert_eq!(MAX_SEQUENCE_TRACKS, e.max_tracks);
        assert_eq!(MAX_INSTRUMENT_ID_BYTES, e.max_instrument_id_bytes);
    }

    /// **The drift guard.** For every node kind upstream ships, and every
    /// field of it, clamping through this sanitiser and clamping through
    /// `symbios-audio`'s `Envelope` land on the same value.
    ///
    /// Two implementations of one table, in two crates, that have to agree
    /// or a record means different things on the two sides of the worker
    /// boundary: the mirror sanitiser runs on the load path (on the `Fp`
    /// grid, before `to_native`), and `clamp_to_envelope` runs inside
    /// `gen-jobs` just before `bake`.
    ///
    /// Compared as **wire values**, not as structs: `Fp` holds a raw `f32`
    /// and quantises only in `Serialize`, so `to_value` is what puts both
    /// sides on the grid the record actually carries — and it side-steps
    /// `NaN != NaN`. Never derive one side's constants from the other by
    /// round-tripping through `f32`; that is how `q` ended up one tick out
    /// (7070 vs 7071, #1160).
    #[test]
    fn every_node_kind_clamps_the_same_on_both_sides() {
        for fill in FILLS {
            for kind in bevy_symbios_audio::NodeKind::defaults() {
                let native = hostile(&kind, fill);

                // This side: mirror the hostile value, then sanitise.
                let mut mirrored = SovereignNodeKind::from_native(&native);
                mirrored.sanitize();

                // Upstream: clamp the hostile value, then mirror.
                let mut clamped = native.clone();
                clamped.clamp_to_envelope(&Envelope::default());
                let expected = SovereignNodeKind::from_native(&clamped);

                assert_eq!(
                    serde_json::to_value(&mirrored).expect("serialises"),
                    serde_json::to_value(&expected).expect("serialises"),
                    "{} disagrees between pds::sanitize and symbios-audio's \
                     Envelope at fill {fill:e}",
                    kind.label()
                );
            }
        }
    }

    /// The same equivalence for a whole recipe — the collection caps, the
    /// instrument-id byte cap, the loop bounds that clamp against an
    /// already-clamped duration, and every event field.
    #[test]
    fn a_sequence_recipe_clamps_the_same_on_both_sides() {
        use bevy_symbios_audio::{AudioPatch, Event, Instrument, NodeKind, SequenceRecipe, Track};

        let seed = SequenceRecipe {
            loop_start_beats: Some(2.0),
            instruments: vec![
                Instrument {
                    // Three bytes per character, so the byte cap lands
                    // mid-character and both sides must walk back to a
                    // boundary rather than panic.
                    id: "☃".repeat(200),
                    patch: AudioPatch {
                        seed: 3,
                        graph: bevy_symbios_audio::NodeGraph {
                            nodes: vec![bevy_symbios_audio::GraphNode {
                                id: bevy_symbios_audio::NodeId(0),
                                kind: NodeKind::Chorus(Default::default()),
                                inputs: Default::default(),
                            }],
                            output: bevy_symbios_audio::NodeId(0),
                        },
                    },
                };
                3
            ],
            tracks: vec![
                Track {
                    events: vec![
                        Event {
                            instrument_id: "☃".repeat(200),
                            ..Event::default()
                        };
                        4
                    ],
                };
                2
            ],
            ..SequenceRecipe::default()
        };

        for fill in FILLS {
            let native = hostile(&seed, fill);

            let mut mirrored = crate::pds::audio::SovereignSequenceRecipe::from_native(&native);
            mirrored.sanitize();

            let mut clamped = native.clone();
            clamped.clamp_to_envelope(&Envelope::default());
            let expected = crate::pds::audio::SovereignSequenceRecipe::from_native(&clamped);

            assert_eq!(
                serde_json::to_value(&mirrored).expect("serialises"),
                serde_json::to_value(&expected).expect("serialises"),
                "SequenceRecipe disagrees at fill {fill:e}"
            );
        }
    }

    /// Connections agree too — the one place an arbitrary float reaches the
    /// summed bus directly.
    ///
    /// `SovereignConnection::Unknown` is deliberately not exercised: native
    /// `Connection` is not `#[non_exhaustive]` and has no such variant, so
    /// there is no upstream value to clamp and nothing to compare against.
    /// The mirror keeps the arm as a read-side seam; it has no image here.
    #[test]
    fn connections_clamp_the_same_on_both_sides() {
        use bevy_symbios_audio::{Connection, NodeId};

        for fill in FILLS {
            for seed in [
                Connection::Constant { value: 1.0 },
                Connection::Node {
                    id: NodeId(7),
                    amount: 1.0,
                },
            ] {
                let native = hostile(&seed, fill);

                let mut mirrored = crate::pds::audio::SovereignConnection::from_native(&native);
                mirrored.sanitize();

                let mut clamped = native.clone();
                clamped.clamp_to_envelope(&Envelope::default());
                let expected = crate::pds::audio::SovereignConnection::from_native(&clamped);

                assert_eq!(
                    serde_json::to_value(&mirrored).expect("serialises"),
                    serde_json::to_value(&expected).expect("serialises"),
                    "{native:?} disagrees at fill {fill:e}"
                );
            }
        }
    }

    #[test]
    fn chorus_clamps_hostile_values() {
        // Feedback past the contractive ceiling, NaN delays, out-of-range
        // mix — all must land back in safe bounds.
        let mut k = SovereignNodeKind::Chorus(SovereignChorus {
            rate_hz: Fp(1e9),
            depth_ms: Fp(f32::NAN),
            base_delay_ms: Fp(1e9),
            feedback: Fp(5.0),
            mix: Fp(50.0),
        });
        k.sanitize();
        let SovereignNodeKind::Chorus(c) = k else {
            panic!("variant changed");
        };
        assert_eq!(c.rate_hz.0, 100.0);
        assert_eq!(c.depth_ms.0, 2.0, "NaN must fall back to the default");
        assert_eq!(c.base_delay_ms.0, 100.0);
        assert_eq!(c.feedback.0, 0.95, "feedback must stay contractive");
        assert_eq!(c.mix.0, 1.0);
    }

    #[test]
    fn reverb_clamps_to_unit_ranges() {
        let mut k = SovereignNodeKind::Reverb(SovereignReverb {
            room_size: Fp(9.0),
            damping: Fp(-9.0),
            mix: Fp(f32::INFINITY),
        });
        k.sanitize();
        let SovereignNodeKind::Reverb(r) = k else {
            panic!("variant changed");
        };
        assert_eq!(r.room_size.0, 1.0);
        assert_eq!(r.damping.0, 0.0);
        assert_eq!(r.mix.0, 0.3, "Inf must fall back to the default");
    }

    #[test]
    fn mix_and_gain_clamp_gain() {
        let mut m = SovereignNodeKind::Mix(SovereignMix { gain: Fp(1e6) });
        m.sanitize();
        let SovereignNodeKind::Mix(m) = m else {
            panic!("variant changed");
        };
        assert_eq!(m.gain.0, 64.0);

        let mut g = SovereignNodeKind::Gain(SovereignGain { gain: Fp(f32::NAN) });
        g.sanitize();
        let SovereignNodeKind::Gain(g) = g else {
            panic!("variant changed");
        };
        assert_eq!(g.gain.0, 1.0, "NaN gain must fall back to unity");
    }
}
