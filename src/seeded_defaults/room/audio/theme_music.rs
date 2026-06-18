//! Theme ambient *music* — the tonal melodic voice that gives a
//! settlement its character. Each [`ThemeArchetype`] maps to a
//! [`ThemeVoice`] descriptor (instrument timbre + scale + note pattern); the
//! match is exhaustive, so every theme has an authored voice and a new
//! archetype must add one. The biome still nudges the register and the voice
//! shares the bed's reverb space — so some of the music is "based on biome"
//! while its identity comes from the theme.

use std::collections::BTreeMap;

use bevy_symbios_audio::{
    AdsrCurve, AdsrEnvelope, AudioPatch, Connection, Event, Gain, Gate, GraphNode, Instrument,
    NodeGraph, NodeId, NodeKind, PitchMode, Reverb, SawtoothOsc, SineOsc, Track, TriangleOsc,
};
use rand_chacha::ChaCha8Rng;

use super::WARMUP_BEATS;
use super::bed::AmbientParams;
use super::scales::{DORIAN, HIRAJOSHI, PENTATONIC_MAJOR, PENTATONIC_MINOR, PHRYGIAN};
use crate::seeded_defaults::scene::{
    BiomeArchetype, SceneCharacter, ThemeArchetype, range_f32, unit_f32,
};

#[derive(Clone, Copy)]
enum Wave {
    Sine,
    Triangle,
    Sawtooth,
}

/// A theme's melodic voice: a single synth instrument plus the shape of
/// the pattern it plays.
struct ThemeVoice {
    /// Stable instrument id.
    id: &'static str,
    wave: Wave,
    /// Cents of detune for a second stacked oscillator (synth width);
    /// `0.0` = a single oscillator.
    detune_cents: f32,
    /// Just-intonation ratio table the pattern walks.
    scale: &'static [f32],
    /// Octave multiplier applied on top of the biome register.
    octave: f32,
    attack_s: f32,
    decay_s: f32,
    sustain_level: f32,
    release_s: f32,
    /// Inclusive notes-per-loop band.
    note_count: (u32, u32),
    /// Per-note gate-length band (beats).
    gate: (f32, f32),
    /// Per-note volume band (kept under the bed).
    volume: (f32, f32),
    /// Dense even eighth-note arpeggio (`true`) vs sparse scattered
    /// onsets (`false`).
    arp: bool,
    reverb_mix: f32,
}

/// Biome register multiplier — volcanic tolls an octave down, tundra
/// rings an octave up, alpine a fifth. Keeps the music seated in the
/// biome even when the theme owns the melody.
fn biome_register(biome: BiomeArchetype) -> f32 {
    match biome {
        BiomeArchetype::Volcanic => 0.5,
        BiomeArchetype::Tundra => 2.0,
        BiomeArchetype::Alpine => 1.5,
        _ => 1.0,
    }
}

/// The voice for a room. Authored themes return their signature voice;
/// every other theme gets the biome-anchored neutral default.
fn voice_for(scene: &SceneCharacter) -> ThemeVoice {
    match scene.theme {
        // Driving detuned-saw synth arpeggio in phrygian — the template.
        ThemeArchetype::Cyberpunk => ThemeVoice {
            id: "theme_synth",
            wave: Wave::Sawtooth,
            detune_cents: 11.0,
            scale: PHRYGIAN,
            octave: 1.0,
            attack_s: 0.005,
            decay_s: 0.12,
            sustain_level: 0.25,
            release_s: 0.18,
            note_count: (12, 16),
            gate: (0.18, 0.30),
            volume: (0.06, 0.11),
            arp: true,
            reverb_mix: 0.28,
        },
        // Plucked koto — bright quick attack, long ring, the half-step
        // Japanese pentatonic. Sparse and contemplative.
        ThemeArchetype::FeudalJapan => ThemeVoice {
            id: "theme_koto",
            wave: Wave::Triangle,
            detune_cents: 0.0,
            scale: HIRAJOSHI,
            octave: 1.0,
            attack_s: 0.004,
            decay_s: 0.35,
            sustain_level: 0.0,
            release_s: 1.3,
            note_count: (4, 7),
            gate: (0.2, 0.5),
            volume: (0.09, 0.16),
            arp: false,
            reverb_mix: 0.46,
        },
        // Grinding low drone — a heavily-detuned saw in phrygian an octave
        // down, sparse and sustained, under the machine hum.
        ThemeArchetype::IndustrialPark => ThemeVoice {
            id: "theme_drone",
            wave: Wave::Sawtooth,
            detune_cents: 14.0,
            scale: PHRYGIAN,
            octave: 0.5,
            attack_s: 0.4,
            decay_s: 0.6,
            sustain_level: 0.7,
            release_s: 2.0,
            note_count: (2, 4),
            gate: (1.0, 2.0),
            volume: (0.06, 0.11),
            arp: false,
            reverb_mix: 0.45,
        },
        // Warm reedy fiddle/accordion — a folksy detuned major, mid-register
        // and lilting over the crickets.
        ThemeArchetype::RuralFarmland => ThemeVoice {
            id: "theme_fiddle",
            wave: Wave::Sawtooth,
            detune_cents: 6.0,
            scale: PENTATONIC_MAJOR,
            octave: 1.0,
            attack_s: 0.02,
            decay_s: 0.3,
            sustain_level: 0.4,
            release_s: 0.8,
            note_count: (4, 7),
            gate: (0.3, 0.7),
            volume: (0.08, 0.15),
            arp: false,
            reverb_mix: 0.38,
        },
        // Bright twinkling chimes — a high, gentle major (an ice-cream-van
        // wholesomeness) over the birdsong.
        ThemeArchetype::Suburban => ThemeVoice {
            id: "theme_chimes",
            wave: Wave::Triangle,
            detune_cents: 0.0,
            scale: PENTATONIC_MAJOR,
            octave: 1.5,
            attack_s: 0.005,
            decay_s: 0.4,
            sustain_level: 0.0,
            release_s: 1.0,
            note_count: (4, 7),
            gate: (0.2, 0.5),
            volume: (0.07, 0.13),
            arp: false,
            reverb_mix: 0.35,
        },
        // Slow detuned-saw synth pad — a calm urban drone under the traffic
        // hum (which rides the traffic light's spatial fx).
        ThemeArchetype::ModernCity => ThemeVoice {
            id: "theme_citypad",
            wave: Wave::Sawtooth,
            detune_cents: 9.0,
            scale: PENTATONIC_MAJOR,
            octave: 1.0,
            attack_s: 0.3,
            decay_s: 0.5,
            sustain_level: 0.6,
            release_s: 1.6,
            note_count: (3, 5),
            gate: (0.8, 1.6),
            volume: (0.06, 0.12),
            arp: false,
            reverb_mix: 0.4,
        },
        // Breathy clay ocarina — sparse, plaintive minor over the implied
        // ritual drums (the drum itself rides the step pyramid's spatial fx).
        ThemeArchetype::Mesoamerican => ThemeVoice {
            id: "theme_ocarina",
            wave: Wave::Sine,
            detune_cents: 0.0,
            scale: PENTATONIC_MINOR,
            octave: 1.0,
            attack_s: 0.05,
            decay_s: 0.45,
            sustain_level: 0.2,
            release_s: 1.4,
            note_count: (3, 6),
            gate: (0.4, 0.9),
            volume: (0.09, 0.16),
            arp: false,
            reverb_mix: 0.4,
        },
        // Low droning lur / horn — slow, sparse, heroic minor, an octave
        // down so it tolls over the steading.
        ThemeArchetype::Nordic => ThemeVoice {
            id: "theme_lur",
            wave: Wave::Triangle,
            detune_cents: 7.0,
            scale: PENTATONIC_MINOR,
            octave: 0.5,
            attack_s: 0.05,
            decay_s: 0.4,
            sustain_level: 0.3,
            release_s: 1.6,
            note_count: (3, 6),
            gate: (0.5, 1.1),
            volume: (0.10, 0.17),
            arp: false,
            reverb_mix: 0.5,
        },
        // Plucked dorian lute — modest, sparse.
        ThemeArchetype::Medieval => ThemeVoice {
            id: "theme_lute",
            wave: Wave::Triangle,
            detune_cents: 0.0,
            scale: DORIAN,
            octave: 1.0,
            attack_s: 0.005,
            decay_s: 0.25,
            sustain_level: 0.0,
            release_s: 0.6,
            note_count: (4, 7),
            gate: (0.3, 0.6),
            volume: (0.09, 0.16),
            arp: false,
            reverb_mix: 0.4,
        },
        // Plucky banjo roll — a bright triangle arpeggio rolling up a sunny
        // major, the dance-hall lilt of the saloon.
        ThemeArchetype::WildWest => ThemeVoice {
            id: "theme_banjo",
            wave: Wave::Triangle,
            detune_cents: 0.0,
            scale: PENTATONIC_MAJOR,
            octave: 1.0,
            attack_s: 0.004,
            decay_s: 0.2,
            sustain_level: 0.0,
            release_s: 0.4,
            note_count: (6, 10),
            gate: (0.18, 0.3),
            volume: (0.06, 0.12),
            arp: true,
            reverb_mix: 0.3,
        },
        // Bleak wasteland drone — a heavily-detuned saw groaning low in a
        // minor pentatonic, sparse and forlorn over the desolate wind.
        ThemeArchetype::PostApoc => ThemeVoice {
            id: "theme_wasteland",
            wave: Wave::Sawtooth,
            detune_cents: 14.0,
            scale: PENTATONIC_MINOR,
            octave: 0.5,
            attack_s: 0.3,
            decay_s: 0.6,
            sustain_level: 0.5,
            release_s: 2.2,
            note_count: (2, 4),
            gate: (1.0, 2.0),
            volume: (0.07, 0.13),
            arp: false,
            reverb_mix: 0.5,
        },
        // Deep monolith tone — a pure sine tolling far below in a dark
        // phrygian, sparse and vast under the array's hum.
        ThemeArchetype::AlienMonolithic => ThemeVoice {
            id: "theme_monolith",
            wave: Wave::Sine,
            detune_cents: 0.0,
            scale: PHRYGIAN,
            octave: 0.5,
            attack_s: 0.2,
            decay_s: 0.6,
            sustain_level: 0.6,
            release_s: 2.5,
            note_count: (2, 3),
            gate: (1.5, 2.5),
            volume: (0.08, 0.14),
            arp: false,
            reverb_mix: 0.6,
        },
        // Eerie biolume theremin — a heavily-detuned sine wavering in a dark
        // phrygian, alien and unsettling over the hive's pulse.
        ThemeArchetype::AlienOrganic => ThemeVoice {
            id: "theme_biolume",
            wave: Wave::Sine,
            detune_cents: 16.0,
            scale: PHRYGIAN,
            octave: 1.0,
            attack_s: 0.1,
            decay_s: 0.5,
            sustain_level: 0.4,
            release_s: 1.8,
            note_count: (3, 5),
            gate: (0.5, 1.1),
            volume: (0.07, 0.13),
            arp: false,
            reverb_mix: 0.55,
        },
        // Funereal pipe organ — a heavily-detuned saw swelling in a dark
        // phrygian, slow and dread-laden through the nave.
        ThemeArchetype::GothicHorror => ThemeVoice {
            id: "theme_organ_dirge",
            wave: Wave::Sawtooth,
            detune_cents: 12.0,
            scale: PHRYGIAN,
            octave: 1.0,
            attack_s: 0.3,
            decay_s: 0.5,
            sustain_level: 0.7,
            release_s: 2.0,
            note_count: (2, 4),
            gate: (1.0, 2.0),
            volume: (0.07, 0.13),
            arp: false,
            reverb_mix: 0.6,
        },
        // Twinkling celesta — a high triangle arpeggio sparkling up a sunny
        // major, the shimmer of bound magic.
        ThemeArchetype::Fantasy => ThemeVoice {
            id: "theme_celesta",
            wave: Wave::Triangle,
            detune_cents: 0.0,
            scale: PENTATONIC_MAJOR,
            octave: 1.5,
            attack_s: 0.005,
            decay_s: 0.4,
            sustain_level: 0.0,
            release_s: 1.4,
            note_count: (8, 12),
            gate: (0.18, 0.3),
            volume: (0.06, 0.11),
            arp: true,
            reverb_mix: 0.5,
        },
        // Cold distant beacon — a high pure sine pinging a sparse minor, the
        // lonely signal of the outpost.
        ThemeArchetype::SpaceOutpost => ThemeVoice {
            id: "theme_beacon",
            wave: Wave::Sine,
            detune_cents: 0.0,
            scale: PENTATONIC_MINOR,
            octave: 1.5,
            attack_s: 0.02,
            decay_s: 0.5,
            sustain_level: 0.0,
            release_s: 2.0,
            note_count: (2, 4),
            gate: (0.4, 0.9),
            volume: (0.08, 0.14),
            arp: false,
            reverb_mix: 0.55,
        },
        // Warm wooden marimba — a soft detune-free sine in a sunny major,
        // gentle and hopeful over the birdsong.
        ThemeArchetype::Solarpunk => ThemeVoice {
            id: "theme_marimba",
            wave: Wave::Sine,
            detune_cents: 0.0,
            scale: PENTATONIC_MAJOR,
            octave: 1.0,
            attack_s: 0.01,
            decay_s: 0.4,
            sustain_level: 0.0,
            release_s: 1.0,
            note_count: (4, 7),
            gate: (0.2, 0.5),
            volume: (0.08, 0.14),
            arp: false,
            reverb_mix: 0.46,
        },
        // Clockwork music box — a bright triangle arpeggio ticking up a minor
        // pentatonic, the mechanism of the cog tower.
        ThemeArchetype::Steampunk => ThemeVoice {
            id: "theme_musicbox",
            wave: Wave::Triangle,
            detune_cents: 0.0,
            scale: PENTATONIC_MINOR,
            octave: 1.0,
            attack_s: 0.004,
            decay_s: 0.25,
            sustain_level: 0.0,
            release_s: 0.5,
            note_count: (10, 14),
            gate: (0.18, 0.3),
            volume: (0.06, 0.11),
            arp: true,
            reverb_mix: 0.35,
        },
        // Bright stadium fanfare — a punchy detuned-saw arpeggio in a major
        // pentatonic, the organ-and-crowd energy of a full ground.
        ThemeArchetype::SportsRec => ThemeVoice {
            id: "theme_fanfare",
            wave: Wave::Sawtooth,
            detune_cents: 8.0,
            scale: PENTATONIC_MAJOR,
            octave: 1.0,
            attack_s: 0.01,
            decay_s: 0.18,
            sustain_level: 0.3,
            release_s: 0.4,
            note_count: (8, 12),
            gate: (0.18, 0.34),
            volume: (0.06, 0.11),
            arp: true,
            reverb_mix: 0.3,
        },
        // Stately pipe-organ pad — a detuned saw swelling in a modal dorian,
        // sparse and reverberant under the clock-tower resonance.
        ThemeArchetype::CivicCampus => ThemeVoice {
            id: "theme_organ",
            wave: Wave::Sawtooth,
            detune_cents: 6.0,
            scale: DORIAN,
            octave: 1.0,
            attack_s: 0.25,
            decay_s: 0.4,
            sustain_level: 0.6,
            release_s: 1.8,
            note_count: (3, 5),
            gate: (0.8, 1.6),
            volume: (0.07, 0.13),
            arp: false,
            reverb_mix: 0.5,
        },
        // Lonesome slide guitar — a gently-detuned triangle keening in a
        // bluesy minor, sparse and reverberant over the highway drone.
        ThemeArchetype::Roadside => ThemeVoice {
            id: "theme_slidegtr",
            wave: Wave::Triangle,
            detune_cents: 5.0,
            scale: PENTATONIC_MINOR,
            octave: 1.0,
            attack_s: 0.04,
            decay_s: 0.5,
            sustain_level: 0.2,
            release_s: 1.4,
            note_count: (3, 6),
            gate: (0.4, 0.9),
            volume: (0.08, 0.15),
            arp: false,
            reverb_mix: 0.44,
        },
        // Bright shimmering steel pan — a detuned sine struck high in a
        // sunny major, lilting and carefree over the surf.
        ThemeArchetype::CoastalResort => ThemeVoice {
            id: "theme_steelpan",
            wave: Wave::Sine,
            detune_cents: 8.0,
            scale: PENTATONIC_MAJOR,
            octave: 1.25,
            attack_s: 0.005,
            decay_s: 0.5,
            sustain_level: 0.0,
            release_s: 0.9,
            note_count: (4, 7),
            gate: (0.2, 0.5),
            volume: (0.07, 0.13),
            arp: false,
            reverb_mix: 0.4,
        },
        // Sparse modal lyre/bells, long ring.
        ThemeArchetype::AncientClassical => ThemeVoice {
            id: "theme_lyre",
            wave: Wave::Triangle,
            detune_cents: 0.0,
            scale: PENTATONIC_MAJOR,
            octave: 1.0,
            attack_s: 0.02,
            decay_s: 0.5,
            sustain_level: 0.0,
            release_s: 1.8,
            note_count: (3, 5),
            gate: (0.4, 0.9),
            volume: (0.09, 0.16),
            arp: false,
            reverb_mix: 0.45,
        },
    }
}

/// Layer the socio-political axes onto the chosen voice. Escalation makes
/// the music busier (more notes), more clipped (shorter gates) and more
/// dissonant (added detune beating); prosperity nudges brightness — richer
/// rooms ring more present and reverberant, poorer ones duller and quieter.
///
/// Both are gated and bounded: a mid-prosperity, peaceful room is left at
/// the voice's authored values, and the post-adjust keeps note counts and
/// volumes inside the orchestrator's loop / mixdown limits (≤20 notes,
/// per-note volume ≤0.3).
fn apply_socio(voice: &mut ThemeVoice, scene: &SceneCharacter) {
    // Escalation ramps in above ~0.45 — calm/tense rooms keep the authored
    // pattern; only real conflict agitates it.
    let conflict = ((scene.escalation - 0.45) / 0.55).clamp(0.0, 1.0);
    if conflict > 0.0 {
        let (lo, hi) = voice.note_count;
        voice.note_count = (
            lo + (conflict * 4.0) as u32,
            (hi + (conflict * 6.0) as u32).min(20),
        );
        let tighten = 1.0 - 0.3 * conflict;
        voice.gate = (voice.gate.0 * tighten, voice.gate.1 * tighten);
        // Detuned beating reads as unease; stacks a second oscillator on
        // voices that were a single oscillator.
        voice.detune_cents += 18.0 * conflict;
    }

    // Prosperity brightness: centred at 0.5 (no change), ±1 at the extremes.
    let wealth = (scene.prosperity.clamp(0.0, 1.0) - 0.5) * 2.0;
    voice.reverb_mix = (voice.reverb_mix + 0.12 * wealth).clamp(0.1, 0.6);
    let vol_scale = 1.0 + 0.18 * wealth;
    voice.volume = (
        (voice.volume.0 * vol_scale).max(0.02),
        (voice.volume.1 * vol_scale).min(0.3),
    );
}

const GATE_ID: NodeId = NodeId(0);
const ADSR_ID: NodeId = NodeId(1);
const OSC1_ID: NodeId = NodeId(2);
const OSC2_ID: NodeId = NodeId(3);
const VCA_ID: NodeId = NodeId(4);
const REVERB_ID: NodeId = NodeId(5);

fn osc(id: NodeId, wave: Wave, freq_hz: f32, amplitude: f32) -> GraphNode {
    let kind = match wave {
        Wave::Sine => NodeKind::Sine(SineOsc {
            freq_hz,
            phase_offset: 0.0,
            amplitude,
        }),
        Wave::Triangle => NodeKind::Triangle(TriangleOsc {
            freq_hz,
            amplitude,
            anti_alias: Default::default(),
        }),
        Wave::Sawtooth => NodeKind::Sawtooth(SawtoothOsc {
            freq_hz,
            polarity: Default::default(),
            amplitude,
            anti_alias: Default::default(),
        }),
    };
    GraphNode {
        id,
        kind,
        inputs: BTreeMap::new(),
    }
}

/// `Gate -> ADSR -> osc(es) -> VCA -> reverb`: the per-event gate strikes
/// the envelope, which shapes the (optionally detuned-stacked) oscillator
/// through the VCA, ringing into the bed's shared space.
fn build_patch(voice: &ThemeVoice, root_hz: f32, params: &AmbientParams, seed: u64) -> AudioPatch {
    let gate = GraphNode {
        id: GATE_ID,
        kind: NodeKind::Gate(Gate { invert: false }),
        inputs: BTreeMap::new(),
    };
    let mut adsr_inputs = BTreeMap::new();
    adsr_inputs.insert("gate".to_string(), vec![Connection::from_node(GATE_ID)]);
    let adsr = GraphNode {
        id: ADSR_ID,
        kind: NodeKind::Adsr(AdsrEnvelope {
            attack_s: voice.attack_s,
            decay_s: voice.decay_s,
            sustain_level: voice.sustain_level,
            release_s: voice.release_s,
            curve: AdsrCurve::Exponential,
        }),
        inputs: adsr_inputs,
    };

    let stacked = voice.detune_cents > 0.0;
    let osc_amp = if stacked { 0.6 } else { 1.0 };
    let mut nodes = vec![gate, adsr, osc(OSC1_ID, voice.wave, root_hz, osc_amp)];
    let mut vca_in = vec![Connection::from_node(OSC1_ID)];
    if stacked {
        let detuned = root_hz * 2.0_f32.powf(voice.detune_cents / 1200.0);
        nodes.push(osc(OSC2_ID, voice.wave, detuned, osc_amp));
        vca_in.push(Connection::from_node(OSC2_ID));
    }

    let mut vca_inputs = BTreeMap::new();
    vca_inputs.insert("in".to_string(), vca_in);
    vca_inputs.insert("gain".to_string(), vec![Connection::from_node(ADSR_ID)]);
    nodes.push(GraphNode {
        id: VCA_ID,
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_inputs,
    });

    let mut reverb_inputs = BTreeMap::new();
    reverb_inputs.insert("in".to_string(), vec![Connection::from_node(VCA_ID)]);
    nodes.push(GraphNode {
        id: REVERB_ID,
        kind: NodeKind::Reverb(Reverb {
            room_size: params.reverb_room_size,
            damping: params.reverb_damping,
            mix: voice.reverb_mix,
        }),
        inputs: reverb_inputs,
    });

    AudioPatch {
        seed: (seed.rotate_left(24) & 0xFFFF_FFFF) as u32,
        graph: NodeGraph {
            nodes,
            output: REVERB_ID,
        },
    }
}

/// Lay out the voice's note events across the loop region. Arp voices
/// stride even eighth-notes up the scale (climbing an octave each pass);
/// sparse voices scatter half-beat-quantised onsets with an occasional
/// octave lift. Both keep tails inside the loop-plus-crossfade overhang
/// and sort deterministically so peers serialise identical recipes.
fn build_events(voice: &ThemeVoice, rng: &mut ChaCha8Rng) -> Vec<Event> {
    let span = (voice.note_count.1 - voice.note_count.0 + 1) as f32;
    let count = (voice.note_count.0 + (range_f32(rng, 0.0, span) as u32)).min(voice.note_count.1);
    let id = voice.id.to_string();
    let n = voice.scale.len();
    let mut events = Vec::with_capacity(count as usize);
    for i in 0..count {
        let i = i as usize;
        let (time_beats, deg, octave) = if voice.arp {
            (
                WARMUP_BEATS + i as f32 * 0.5,
                i % n,
                1.0 + ((i / n) % 2) as f32,
            )
        } else {
            let t = WARMUP_BEATS + (range_f32(rng, 0.0, 13.5) * 2.0).floor() * 0.5;
            let deg = (range_f32(rng, 0.0, n as f32) as usize).min(n - 1);
            let octave = if unit_f32(rng) < 0.2 { 2.0 } else { 1.0 };
            (t, deg, octave)
        };
        events.push(Event {
            time_beats,
            instrument_id: id.clone(),
            pitch_multiplier: voice.scale[deg] * octave,
            volume: range_f32(rng, voice.volume.0, voice.volume.1),
            gate_beats: range_f32(rng, voice.gate.0, voice.gate.1),
            release_beats: voice.release_s,
            pitch_mode: PitchMode::Varispeed,
        });
    }
    events.sort_by(|a, b| a.time_beats.total_cmp(&b.time_beats));
    events
}

/// Build the theme music layer — one melodic instrument + its track —
/// for the room's theme, sharing the bed's reverb space via `params`.
pub(super) fn build(
    scene: &SceneCharacter,
    params: &AmbientParams,
    rng: &mut ChaCha8Rng,
    seed: u64,
) -> (Instrument, Track) {
    let mut voice = voice_for(scene);
    apply_socio(&mut voice, scene);
    let root_hz = 220.0
        * 2.0_f32.powf(scene.base_hue_deg / 360.0)
        * voice.octave
        * biome_register(scene.biome);
    let patch = build_patch(&voice, root_hz, params, seed);
    let events = build_events(&voice, rng);
    (
        Instrument {
            id: voice.id.to_string(),
            patch,
        },
        Track { events },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene_with(theme: ThemeArchetype, temp: f32) -> SceneCharacter {
        let mut s = SceneCharacter::for_seed(7);
        s.theme = theme;
        s.temperature = temp;
        s
    }

    #[test]
    fn authored_themes_use_their_signature_wave_and_scale() {
        let cy = voice_for(&scene_with(ThemeArchetype::Cyberpunk, 0.0));
        assert!(matches!(cy.wave, Wave::Sawtooth) && cy.arp && cy.detune_cents > 0.0);
        assert_eq!(cy.scale, PHRYGIAN);
        let med = voice_for(&scene_with(ThemeArchetype::Medieval, 0.0));
        assert_eq!(med.scale, DORIAN);
        let nor = voice_for(&scene_with(ThemeArchetype::Nordic, 0.0));
        assert!(matches!(nor.wave, Wave::Triangle) && !nor.arp && nor.octave < 1.0);
        assert_eq!(nor.scale, PENTATONIC_MINOR);
        let jp = voice_for(&scene_with(ThemeArchetype::FeudalJapan, 0.0));
        assert_eq!(jp.scale, HIRAJOSHI);
        let meso = voice_for(&scene_with(ThemeArchetype::Mesoamerican, 0.0));
        assert!(matches!(meso.wave, Wave::Sine) && !meso.arp);
        assert_eq!(meso.scale, PENTATONIC_MINOR);
        let city = voice_for(&scene_with(ThemeArchetype::ModernCity, 0.0));
        assert!(matches!(city.wave, Wave::Sawtooth) && !city.arp && city.detune_cents > 0.0);
        assert_eq!(city.scale, PENTATONIC_MAJOR);
        let sub = voice_for(&scene_with(ThemeArchetype::Suburban, 0.0));
        assert!(matches!(sub.wave, Wave::Triangle) && sub.octave > 1.0);
        assert_eq!(sub.scale, PENTATONIC_MAJOR);
        let farm = voice_for(&scene_with(ThemeArchetype::RuralFarmland, 0.0));
        assert!(
            matches!(farm.wave, Wave::Sawtooth) && farm.detune_cents > 0.0 && farm.attack_s < 0.1
        );
        assert_eq!(farm.scale, PENTATONIC_MAJOR);
        let ind = voice_for(&scene_with(ThemeArchetype::IndustrialPark, 0.0));
        assert!(matches!(ind.wave, Wave::Sawtooth) && !ind.arp && ind.octave < 1.0);
        assert_eq!(ind.scale, PHRYGIAN);
        let anc = voice_for(&scene_with(ThemeArchetype::AncientClassical, 0.0));
        assert_eq!(anc.scale, PENTATONIC_MAJOR);
        let coast = voice_for(&scene_with(ThemeArchetype::CoastalResort, 0.0));
        assert!(matches!(coast.wave, Wave::Sine) && coast.detune_cents > 0.0 && coast.octave > 1.0);
        assert_eq!(coast.scale, PENTATONIC_MAJOR);
        let road = voice_for(&scene_with(ThemeArchetype::Roadside, 0.0));
        assert!(matches!(road.wave, Wave::Triangle) && road.detune_cents > 0.0 && !road.arp);
        assert_eq!(road.scale, PENTATONIC_MINOR);
        let civ = voice_for(&scene_with(ThemeArchetype::CivicCampus, 0.0));
        assert!(matches!(civ.wave, Wave::Sawtooth) && civ.detune_cents > 0.0 && civ.attack_s > 0.1);
        assert_eq!(civ.scale, DORIAN);
        let spr = voice_for(&scene_with(ThemeArchetype::SportsRec, 0.0));
        assert!(matches!(spr.wave, Wave::Sawtooth) && spr.arp && spr.detune_cents > 0.0);
        assert_eq!(spr.scale, PENTATONIC_MAJOR);
        let stm = voice_for(&scene_with(ThemeArchetype::Steampunk, 0.0));
        assert!(matches!(stm.wave, Wave::Triangle) && stm.arp);
        assert_eq!(stm.scale, PENTATONIC_MINOR);
        let sol = voice_for(&scene_with(ThemeArchetype::Solarpunk, 0.0));
        assert!(matches!(sol.wave, Wave::Sine) && !sol.arp && sol.detune_cents == 0.0);
        assert_eq!(sol.scale, PENTATONIC_MAJOR);
        let spo = voice_for(&scene_with(ThemeArchetype::SpaceOutpost, 0.0));
        assert!(matches!(spo.wave, Wave::Sine) && spo.octave > 1.0);
        assert_eq!(spo.scale, PENTATONIC_MINOR);
        let fan = voice_for(&scene_with(ThemeArchetype::Fantasy, 0.0));
        assert!(matches!(fan.wave, Wave::Triangle) && fan.arp && fan.octave > 1.0);
        assert_eq!(fan.scale, PENTATONIC_MAJOR);
        let got = voice_for(&scene_with(ThemeArchetype::GothicHorror, 0.0));
        assert!(matches!(got.wave, Wave::Sawtooth) && !got.arp && got.attack_s > 0.1);
        assert_eq!(got.scale, PHRYGIAN);
        let alo = voice_for(&scene_with(ThemeArchetype::AlienOrganic, 0.0));
        assert!(matches!(alo.wave, Wave::Sine) && alo.detune_cents > 0.0);
        assert_eq!(alo.scale, PHRYGIAN);
        let alm = voice_for(&scene_with(ThemeArchetype::AlienMonolithic, 0.0));
        assert!(matches!(alm.wave, Wave::Sine) && alm.octave < 1.0 && alm.detune_cents == 0.0);
        assert_eq!(alm.scale, PHRYGIAN);
        let pa = voice_for(&scene_with(ThemeArchetype::PostApoc, 0.0));
        assert!(matches!(pa.wave, Wave::Sawtooth) && pa.octave < 1.0 && !pa.arp);
        assert_eq!(pa.scale, PENTATONIC_MINOR);
        let ww = voice_for(&scene_with(ThemeArchetype::WildWest, 0.0));
        assert!(matches!(ww.wave, Wave::Triangle) && ww.arp && (ww.octave - 1.0).abs() < 1e-6);
        assert_eq!(ww.scale, PENTATONIC_MAJOR);
    }

    #[test]
    fn conflict_makes_the_voice_busier_and_more_dissonant() {
        let mut scene = scene_with(ThemeArchetype::Medieval, 0.0);
        scene.prosperity = 0.5;
        scene.escalation = 0.0;
        let mut calm = voice_for(&scene);
        apply_socio(&mut calm, &scene);

        scene.escalation = 1.0;
        let mut war = voice_for(&scene);
        apply_socio(&mut war, &scene);

        assert!(war.note_count.1 > calm.note_count.1, "conflict adds notes");
        assert!(
            war.detune_cents > calm.detune_cents,
            "conflict adds dissonance"
        );
        assert!(war.gate.1 < calm.gate.1, "conflict clips note lengths");
        // Bounds the orchestrator relies on hold.
        assert!(war.note_count.1 <= 20 && war.volume.1 <= 0.3);
    }

    #[test]
    fn prosperity_brightens_or_dulls_presence() {
        let mut scene = scene_with(ThemeArchetype::AncientClassical, 0.0);
        scene.escalation = 0.0;
        scene.prosperity = 0.95;
        let mut rich = voice_for(&scene);
        apply_socio(&mut rich, &scene);
        scene.prosperity = 0.05;
        let mut poor = voice_for(&scene);
        apply_socio(&mut poor, &scene);
        assert!(rich.reverb_mix > poor.reverb_mix, "rich rings more present");
        assert!(rich.volume.1 > poor.volume.1, "rich is louder");
        assert!(rich.volume.1 <= 0.3, "still under the bed");
    }
}
