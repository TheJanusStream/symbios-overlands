//! Sanitiser for [`SovereignAudioConfig`] and its structured node /
//! sequence mirror types. Clamps node-count and event-count budgets so
//! a hostile peer can't smuggle a million-element graph through a
//! room recipe; per-config-field numeric clamps live inside the
//! variant arms.
//!
//! The Referenced variant forwards to the asset-reference sanitiser
//! for URL / DID / CID length caps.

use super::Sanitize;
use super::limits;
use crate::pds::audio::{
    SovereignAudioConfig, SovereignAudioPatch, SovereignConnection, SovereignEvent,
    SovereignNodeGraph, SovereignNodeKind, SovereignSequenceRecipe, SovereignTrack,
};
use crate::pds::types::Fp;

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

impl Sanitize for SovereignConnection {
    fn sanitize(&mut self) {
        match self {
            SovereignConnection::Constant { value } => {
                value.0 = clamp_finite(value.0, -1_000_000.0, 1_000_000.0, 0.0);
            }
            SovereignConnection::Node { amount, .. } => {
                amount.0 = clamp_finite(amount.0, -1_000_000.0, 1_000_000.0, 1.0);
            }
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
            if instr.id.len() > MAX_INSTRUMENT_ID_BYTES {
                instr.id.truncate(MAX_INSTRUMENT_ID_BYTES);
            }
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
        if self.instrument_id.len() > MAX_INSTRUMENT_ID_BYTES {
            self.instrument_id.truncate(MAX_INSTRUMENT_ID_BYTES);
        }
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

#[allow(dead_code)]
fn _retain_unused_limits_link() {
    // `limits::` would-be reference left in scope so a future tweak
    // of MAX_AUDIO_PATCH_JSON_BYTES doesn't drop the import warning
    // before the JSON-stash compatibility layer is fully removed
    // (none of the structured sanitiser arms need it).
    let _ = limits::MAX_AUDIO_PATCH_JSON_BYTES;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::audio::{
        SovereignChorus, SovereignGain, SovereignMix, SovereignNodeKind, SovereignReverb,
    };

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
