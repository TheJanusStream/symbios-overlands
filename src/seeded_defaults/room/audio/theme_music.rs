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
use rand_chacha::rand_core::SeedableRng;

use super::bed::AmbientParams;
use super::scales::{DORIAN, HIRAJOSHI, PENTATONIC_MAJOR, PENTATONIC_MINOR, PHRYGIAN};
use super::{LOOP_BEATS, WARMUP_BEATS};
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
        // Glassy ice rings as high as the tundra frost.
        BiomeArchetype::Tundra | BiomeArchetype::Glacial => 2.0,
        BiomeArchetype::Alpine => 1.5,
        _ => 1.0,
    }
}

/// Sub-stream salt for the per-room voice variety, distinct from the
/// pattern stream so picking a mode / jittering the timbre never shifts
/// the note-placement rng (which would re-roll every pattern).
const VOICE_VARIETY_SALT: u64 = 0x5EED_1CE5_C0DE_0001;

/// The curated, in-character scale set for a theme. The voice picks one by
/// seed so two settlements of the same theme can sit in different modes
/// while staying inside the theme's harmonic family. Element `[0]` is the
/// signature mode (the one named in the per-theme literal below); themes
/// whose mode *is* their identity (Feudal-Japan's Hirajōshi) or whose
/// brightness only one scale carries (the sunny-major themes) keep a
/// single-element set and get their variety from key / register / voicing
/// / pattern instead.
fn theme_scales(theme: ThemeArchetype) -> &'static [&'static [f32]] {
    match theme {
        ThemeArchetype::Cyberpunk => &[PHRYGIAN, PENTATONIC_MINOR],
        ThemeArchetype::FeudalJapan => &[HIRAJOSHI],
        ThemeArchetype::IndustrialPark => &[PHRYGIAN, PENTATONIC_MINOR],
        ThemeArchetype::RuralFarmland => &[PENTATONIC_MAJOR, DORIAN],
        ThemeArchetype::Suburban => &[PENTATONIC_MAJOR],
        ThemeArchetype::ModernCity => &[PENTATONIC_MAJOR, DORIAN],
        ThemeArchetype::Mesoamerican => &[PENTATONIC_MINOR, PHRYGIAN],
        ThemeArchetype::Nordic => &[PENTATONIC_MINOR, DORIAN],
        ThemeArchetype::Medieval => &[DORIAN, PHRYGIAN],
        ThemeArchetype::WildWest => &[PENTATONIC_MAJOR, PENTATONIC_MINOR],
        ThemeArchetype::PostApoc => &[PENTATONIC_MINOR, PHRYGIAN],
        ThemeArchetype::AlienMonolithic => &[PHRYGIAN, PENTATONIC_MINOR],
        ThemeArchetype::AlienOrganic => &[PHRYGIAN, DORIAN],
        ThemeArchetype::GothicHorror => &[PHRYGIAN, PENTATONIC_MINOR],
        ThemeArchetype::Fantasy => &[PENTATONIC_MAJOR, DORIAN],
        ThemeArchetype::SpaceOutpost => &[PENTATONIC_MINOR, PHRYGIAN],
        ThemeArchetype::Solarpunk => &[PENTATONIC_MAJOR],
        ThemeArchetype::Steampunk => &[PENTATONIC_MINOR, DORIAN],
        ThemeArchetype::SportsRec => &[PENTATONIC_MAJOR],
        ThemeArchetype::CivicCampus => &[DORIAN, PENTATONIC_MAJOR],
        ThemeArchetype::Roadside => &[PENTATONIC_MINOR, PENTATONIC_MAJOR],
        ThemeArchetype::CoastalResort => &[PENTATONIC_MAJOR],
        ThemeArchetype::AncientClassical => &[DORIAN, PHRYGIAN],
    }
}

/// Seeded per-room variety layered on top of the signature voice: choose a
/// mode from the theme's curated family and nudge the timbre (ring, colour,
/// articulation) a little, so the same theme reads fresh across rooms
/// without losing its identity. Deterministic in `seed`; its own rng stream
/// keeps it independent of the pattern generator. Octave / wave / attack are
/// left untouched — those carry the recognisable signature.
fn apply_voice_variety(voice: &mut ThemeVoice, theme: ThemeArchetype, seed: u64) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ VOICE_VARIETY_SALT);
    let scales = theme_scales(theme);
    debug_assert!(
        scales[0] == voice.scale,
        "theme_scales[0] must equal the signature scale in the voice literal"
    );
    let idx = ((unit_f32(&mut rng) * scales.len() as f32) as usize).min(scales.len() - 1);
    voice.scale = scales[idx];

    // Symmetric ±`frac` multiplier around 1.0.
    let jitter = |rng: &mut ChaCha8Rng, frac: f32| 1.0 + (unit_f32(rng) * 2.0 - 1.0) * frac;
    voice.release_s *= jitter(&mut rng, 0.15);
    voice.decay_s *= jitter(&mut rng, 0.12);
    voice.reverb_mix = (voice.reverb_mix + (unit_f32(&mut rng) * 2.0 - 1.0) * 0.05).clamp(0.1, 0.6);
    let g = jitter(&mut rng, 0.12);
    voice.gate = (voice.gate.0 * g, voice.gate.1 * g);
    // Only widen voices that are already stacked — keep the pure-sine themes
    // (their detune is 0 by identity) pure.
    if voice.detune_cents > 0.0 {
        voice.detune_cents = (voice.detune_cents * jitter(&mut rng, 0.25)).max(2.0);
    }
}

/// The voice for a room. Authored themes return their signature voice
/// (with seeded per-room variety on top); every other theme gets the
/// biome-anchored neutral default.
fn voice_for(scene: &SceneCharacter, seed: u64) -> ThemeVoice {
    let mut voice = match scene.theme {
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
        // Gentle warm chimes — a softly-ringing major a touch above the
        // melody register, a little sustain and a long tail so it reads as a
        // calm domestic pad rather than a bright ice-cream-van jingle.
        ThemeArchetype::Suburban => ThemeVoice {
            id: "theme_chimes",
            wave: Wave::Triangle,
            detune_cents: 0.0,
            scale: PENTATONIC_MAJOR,
            octave: 1.25,
            attack_s: 0.01,
            decay_s: 0.5,
            sustain_level: 0.12,
            release_s: 1.4,
            note_count: (4, 7),
            gate: (0.2, 0.5),
            volume: (0.07, 0.13),
            arp: false,
            reverb_mix: 0.42,
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
        // Lonesome harmonica — a reedy, slightly-detuned triangle keening
        // sparse and plaintive up a wide major, a long ring trailing off
        // into the dry wind.
        ThemeArchetype::WildWest => ThemeVoice {
            id: "theme_harmonica",
            wave: Wave::Triangle,
            detune_cents: 5.0,
            scale: PENTATONIC_MAJOR,
            octave: 1.0,
            attack_s: 0.03,
            decay_s: 0.35,
            sustain_level: 0.25,
            release_s: 1.2,
            note_count: (3, 6),
            gate: (0.4, 0.9),
            volume: (0.07, 0.13),
            arp: false,
            reverb_mix: 0.4,
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
        // Funereal pipe organ — a heavily-detuned saw swelling low in a dark
        // phrygian, slow and dread-laden through the nave (a tolling pad over
        // the bass-pad floor).
        ThemeArchetype::GothicHorror => ThemeVoice {
            id: "theme_organ_dirge",
            wave: Wave::Sawtooth,
            detune_cents: 12.0,
            scale: PHRYGIAN,
            octave: 0.75,
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
        // Bright airy bells — a clean detune-free sine struck high in a sunny
        // major, a long lush ring over the birdsong: hopeful and luminous.
        ThemeArchetype::Solarpunk => ThemeVoice {
            id: "theme_marimba",
            wave: Wave::Sine,
            detune_cents: 0.0,
            scale: PENTATONIC_MAJOR,
            octave: 1.25,
            attack_s: 0.01,
            decay_s: 0.4,
            sustain_level: 0.0,
            release_s: 1.5,
            note_count: (4, 7),
            gate: (0.2, 0.5),
            volume: (0.08, 0.14),
            arp: false,
            reverb_mix: 0.52,
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
        // Stately modal lyre/bells — a sparse Dorian struck with a long
        // ceremonial ring; the dignified voice that also backstops every
        // un-built theme as the settlement fallback (#461).
        ThemeArchetype::AncientClassical => ThemeVoice {
            id: "theme_lyre",
            wave: Wave::Triangle,
            detune_cents: 0.0,
            scale: DORIAN,
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
    };
    apply_voice_variety(&mut voice, scene.theme, seed);
    voice
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
/// Onset-free tail at the end of the loop region, so the last phrase's
/// gate + release lands inside the crossfade overhang instead of being
/// clipped at the seam. Sized for the worst case: the longest-tailed voice
/// (the monolith drone, gate ≤ 2.5 + release 2.5) plus the variety jitter
/// (+12% gate, +15% release) ≈ 5.7 beats, which must fit the overhang
/// (`LOOP_BEATS` end → `duration + crossfade`). 4.0 keeps a late onset's
/// tail inside that window for every seed, not just the tested ones.
const ONSET_TAIL_BEATS: f32 = 4.0;
/// Hard ceiling on note events per voice — keeps bake time + the mixdown
/// bounded however the phrase/scatter maths lands on a long loop.
const MAX_NOTES: usize = 40;

/// One melodic event at `t` on scale `deg`/`octave`, with the voice's
/// per-note volume / gate / release sampled from its bands.
fn note(voice: &ThemeVoice, rng: &mut ChaCha8Rng, t: f32, deg: usize, octave: f32) -> Event {
    Event {
        time_beats: t,
        instrument_id: voice.id.to_string(),
        pitch_multiplier: voice.scale[deg] * octave,
        volume: range_f32(rng, voice.volume.0, voice.volume.1),
        gate_beats: range_f32(rng, voice.gate.0, voice.gate.1),
        release_beats: voice.release_s,
        pitch_mode: PitchMode::Varispeed,
    }
}

fn build_events(voice: &ThemeVoice, rng: &mut ChaCha8Rng) -> Vec<Event> {
    let n = voice.scale.len();
    // Beats available for onsets after the warm-up run-up.
    let span = (LOOP_BEATS - ONSET_TAIL_BEATS).max(4.0);
    let mut events = if voice.arp {
        arp_phrases(voice, rng, span, n)
    } else {
        sparse_scatter(voice, rng, span, n)
    };
    events.sort_by(|a, b| a.time_beats.total_cmp(&b.time_beats));
    events
}

/// Arpeggio voices fill the loop with eighth-note scale runs, but every
/// phrase is transformed so a long loop reads as developing A/B/C phrases
/// rather than one tiled arpeggio: the melodic contour (ascending,
/// descending, arch, pendulum) is re-rolled per phrase, an occasional phrase
/// plays a shorter motif fragment, every third phrase jumps an octave, and
/// whole-phrase rests punctuate. All seeded, so two rooms of the same theme
/// also lay their phrases out differently.
fn arp_phrases(voice: &ThemeVoice, rng: &mut ChaCha8Rng, span: f32, n: usize) -> Vec<Event> {
    const STEP: f32 = 0.5; // eighth notes at 60 BPM
    let end = WARMUP_BEATS + span;
    let mut events = Vec::new();
    let mut t = WARMUP_BEATS;
    let mut phrase = 0u32;
    loop {
        // Most phrases run the full scale; some play a shorter motif so the
        // rhythm isn't a metronomic n-note tile.
        let len = if phrase > 0 && unit_f32(rng) < 0.3 {
            (n / 2 + 1).clamp(3.min(n), n)
        } else {
            n
        };
        let phrase_beats = len as f32 * STEP;
        if t + phrase_beats > end || events.len() + len > MAX_NOTES {
            break;
        }
        // An occasional whole-phrase rest (never the first) breaks the tile.
        let rest_phrase = phrase > 0 && unit_f32(rng) < 0.18;
        if !rest_phrase {
            let octave = if phrase % 3 == 1 { 2.0 } else { 1.0 };
            let contour = (unit_f32(rng) * 4.0) as u32;
            for k in 0..len {
                let deg = contour_deg(contour, k, len, n);
                events.push(note(voice, rng, t + k as f32 * STEP, deg, octave));
            }
        }
        // A short rest between phrases keeps them from running solid.
        t += phrase_beats + range_f32(rng, 0.5, 1.5);
        phrase += 1;
    }
    events
}

/// Map note index `k` (`0..len`) to a scale degree (`0..n`) following one of
/// four melodic contours, so successive phrases trace different shapes.
fn contour_deg(contour: u32, k: usize, len: usize, n: usize) -> usize {
    match contour % 4 {
        // Ascending run up the scale.
        0 => k % n,
        // Descending run back down.
        1 => (n - 1).saturating_sub(k % n),
        // Arch: climb to the phrase midpoint, then fall back toward the root.
        2 => {
            let mid = len / 2;
            let pos = if k <= mid { k } else { len.saturating_sub(k) };
            pos.min(n - 1)
        }
        // Pendulum: alternate low and high degrees.
        _ => {
            let h = (k / 2) % n;
            if k.is_multiple_of(2) {
                h
            } else {
                (n - 1).saturating_sub(h)
            }
        }
    }
}

/// Sparse voices place their onsets as a handful of *gestures* — short runs
/// of close notes tracing a small up/down contour from a seeded root degree,
/// separated by rests and spread across the loop. This reads as a breathing,
/// developing phrase (and lays out differently per seed) rather than a
/// uniform random sprinkle. The note budget still scales with loop length.
fn sparse_scatter(voice: &ThemeVoice, rng: &mut ChaCha8Rng, span: f32, n: usize) -> Vec<Event> {
    let band = (voice.note_count.1 - voice.note_count.0 + 1) as f32;
    let base = voice.note_count.0 + (range_f32(rng, 0.0, band) as u32);
    let count = ((base as f32 * (LOOP_BEATS / 16.0)).round() as usize).min(MAX_NOTES);
    if count == 0 {
        return Vec::new();
    }
    // A few gestures, each owning a slice of the span. The final gesture mops
    // up whatever notes remain so the total stays exactly `count`.
    let gestures = (count / 2).clamp(2, 5);
    let slice = span / gestures as f32;
    let mut events = Vec::with_capacity(count);
    let mut remaining = count;
    for g in 0..gestures {
        let here = if g == gestures - 1 {
            remaining
        } else {
            (remaining / (gestures - g)).max(1)
        };
        // Gesture onset near the front of its slice; notes then step on a
        // quarter-ish grid, clamped inside the onset window.
        let start = WARMUP_BEATS + g as f32 * slice + range_f32(rng, 0.0, slice * 0.4);
        let dir: i32 = if unit_f32(rng) < 0.5 { 1 } else { -1 };
        let root = (range_f32(rng, 0.0, n as f32) as usize).min(n - 1);
        for j in 0..here {
            let t = (start + j as f32 * range_f32(rng, 0.5, 1.0)).min(WARMUP_BEATS + span);
            let deg = (root as i32 + dir * j as i32).rem_euclid(n as i32) as usize;
            let octave = if unit_f32(rng) < 0.2 { 2.0 } else { 1.0 };
            events.push(note(voice, rng, t, deg, octave));
        }
        remaining = remaining.saturating_sub(here);
        if remaining == 0 {
            break;
        }
    }
    events
}

/// Hz floor for the bass fundamental so the octave-below-the-melody drop
/// never pushes a low-drone theme in a register-lowering biome (Volcanic)
/// into sub-sonic rumble.
const BASS_FLOOR_HZ: f32 = 40.0;

/// The bass octave multiplier: one octave below the melody, but never
/// shallower than the deep `0.5` register the mid/high voices already sit
/// at. Low-drone themes (melody octave < 1.0) therefore drop to a real
/// octave below their drone instead of doubling it at unison; mid/high
/// themes keep the existing deep pad.
fn bass_octave_for(melody_octave: f32) -> f32 {
    (melody_octave * 0.5).min(0.5)
}

/// Low drone / pad second voice — a sustained sine one octave below the
/// melody (see [`bass_octave_for`]) that fills the long loop's bottom end
/// and shares the bed's reverb. Its voicing varies by seed (a static held
/// drone, or a slow walk across 2–3 scale degrees), but it is always present
/// so every room reads layered. Returns the instrument + its track.
pub(super) fn build_bass(
    scene: &SceneCharacter,
    params: &AmbientParams,
    rng: &mut ChaCha8Rng,
    seed: u64,
) -> (Instrument, Track) {
    let melody = voice_for(scene, seed);
    // Prosperity lifts the pad volume a touch, mirroring the melody.
    let wealth = (scene.prosperity.clamp(0.0, 1.0) - 0.5) * 2.0;
    let vol = (0.07 + 0.02 * wealth).clamp(0.04, 0.11);
    let bass = ThemeVoice {
        id: "theme_bass",
        wave: Wave::Sine,
        detune_cents: 0.0,
        scale: melody.scale,
        octave: bass_octave_for(melody.octave),
        attack_s: 0.8,
        decay_s: 0.6,
        sustain_level: 0.8,
        release_s: 1.5,
        note_count: (1, 3),
        gate: (0.0, 0.0),
        volume: (vol, vol),
        arp: false,
        reverb_mix: (melody.reverb_mix + 0.1).min(0.6),
    };
    // One octave below the melody for clean separation; the Hz floor keeps a
    // register-lowering biome from pushing the drop sub-sonic.
    let root_hz = (220.0
        * 2.0_f32.powf(scene.base_hue_deg / 360.0)
        * biome_register(scene.biome)
        * bass.octave)
        .max(BASS_FLOOR_HZ);
    let patch = build_patch(&bass, root_hz, params, seed ^ 0x0BA5_50DD);
    let events = build_bass_events(&bass, rng);
    (
        Instrument {
            id: bass.id.to_string(),
            patch,
        },
        Track { events },
    )
}

/// 1–3 long held notes spanning the loop. One note is a static drone; two
/// or three step slowly across low scale degrees, each held for its share of
/// the loop with a release tail into the seam. The walk *shape* is seeded
/// (I–V–I, I–♭VII–V, or I–III–VI-ish) so the bottom end isn't a fixed
/// figure across every room of a theme.
fn build_bass_events(bass: &ThemeVoice, rng: &mut ChaCha8Rng) -> Vec<Event> {
    let n = bass.scale.len();
    let notes = (1 + (unit_f32(rng) * 3.0) as u32).clamp(1, 3); // 1..=3
    // Low scale degrees for a slow root movement; clamped to the scale.
    let walk: [usize; 3] = match (unit_f32(rng) * 3.0) as usize % 3 {
        0 => [0, (n / 2).min(n - 1), 0],
        1 => [0, n - 1, (n / 2).min(n - 1)],
        _ => [0, (n / 3).min(n - 1), (2 * n / 3).min(n - 1)],
    };
    let seg = LOOP_BEATS / notes as f32;
    let mut events = Vec::with_capacity(notes as usize);
    for k in 0..notes {
        let deg = walk[(k as usize) % walk.len()].min(n - 1);
        events.push(Event {
            time_beats: WARMUP_BEATS + k as f32 * seg,
            instrument_id: bass.id.to_string(),
            pitch_multiplier: bass.scale[deg],
            volume: bass.volume.0,
            // Hold each segment; the final note's release tails into the seam.
            gate_beats: seg,
            release_beats: bass.release_s,
            pitch_mode: PitchMode::Varispeed,
        });
    }
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
    let mut voice = voice_for(scene, seed);
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

    /// A fixed seed for the identity checks. Wave / arp / octave / attack /
    /// detune-sign are seed-stable (variety only picks the mode + jitters the
    /// ring/colour), so any seed exercises the signature; the *signature
    /// scale* is asserted via `theme_scales(..)[0]` since the active mode is
    /// now seed-picked from the family.
    const ID_SEED: u64 = 0xA5A5;

    #[test]
    fn authored_themes_use_their_signature_wave_and_scale() {
        use ThemeArchetype as T;
        let cy = voice_for(&scene_with(T::Cyberpunk, 0.0), ID_SEED);
        assert!(matches!(cy.wave, Wave::Sawtooth) && cy.arp && cy.detune_cents > 0.0);
        assert_eq!(theme_scales(T::Cyberpunk)[0], PHRYGIAN);
        assert_eq!(theme_scales(T::Medieval)[0], DORIAN);
        let nor = voice_for(&scene_with(T::Nordic, 0.0), ID_SEED);
        assert!(matches!(nor.wave, Wave::Triangle) && !nor.arp && nor.octave < 1.0);
        assert_eq!(theme_scales(T::Nordic)[0], PENTATONIC_MINOR);
        assert_eq!(theme_scales(T::FeudalJapan)[0], HIRAJOSHI);
        let meso = voice_for(&scene_with(T::Mesoamerican, 0.0), ID_SEED);
        assert!(matches!(meso.wave, Wave::Sine) && !meso.arp);
        assert_eq!(theme_scales(T::Mesoamerican)[0], PENTATONIC_MINOR);
        let city = voice_for(&scene_with(T::ModernCity, 0.0), ID_SEED);
        assert!(matches!(city.wave, Wave::Sawtooth) && !city.arp && city.detune_cents > 0.0);
        assert_eq!(theme_scales(T::ModernCity)[0], PENTATONIC_MAJOR);
        let sub = voice_for(&scene_with(T::Suburban, 0.0), ID_SEED);
        assert!(matches!(sub.wave, Wave::Triangle) && sub.octave > 1.0);
        assert_eq!(theme_scales(T::Suburban)[0], PENTATONIC_MAJOR);
        let farm = voice_for(&scene_with(T::RuralFarmland, 0.0), ID_SEED);
        assert!(
            matches!(farm.wave, Wave::Sawtooth) && farm.detune_cents > 0.0 && farm.attack_s < 0.1
        );
        assert_eq!(theme_scales(T::RuralFarmland)[0], PENTATONIC_MAJOR);
        let ind = voice_for(&scene_with(T::IndustrialPark, 0.0), ID_SEED);
        assert!(matches!(ind.wave, Wave::Sawtooth) && !ind.arp && ind.octave < 1.0);
        assert_eq!(theme_scales(T::IndustrialPark)[0], PHRYGIAN);
        assert_eq!(theme_scales(T::AncientClassical)[0], DORIAN);
        let coast = voice_for(&scene_with(T::CoastalResort, 0.0), ID_SEED);
        assert!(matches!(coast.wave, Wave::Sine) && coast.detune_cents > 0.0 && coast.octave > 1.0);
        assert_eq!(theme_scales(T::CoastalResort)[0], PENTATONIC_MAJOR);
        let road = voice_for(&scene_with(T::Roadside, 0.0), ID_SEED);
        assert!(matches!(road.wave, Wave::Triangle) && road.detune_cents > 0.0 && !road.arp);
        assert_eq!(theme_scales(T::Roadside)[0], PENTATONIC_MINOR);
        let civ = voice_for(&scene_with(T::CivicCampus, 0.0), ID_SEED);
        assert!(matches!(civ.wave, Wave::Sawtooth) && civ.detune_cents > 0.0 && civ.attack_s > 0.1);
        assert_eq!(theme_scales(T::CivicCampus)[0], DORIAN);
        let spr = voice_for(&scene_with(T::SportsRec, 0.0), ID_SEED);
        assert!(matches!(spr.wave, Wave::Sawtooth) && spr.arp && spr.detune_cents > 0.0);
        assert_eq!(theme_scales(T::SportsRec)[0], PENTATONIC_MAJOR);
        let stm = voice_for(&scene_with(T::Steampunk, 0.0), ID_SEED);
        assert!(matches!(stm.wave, Wave::Triangle) && stm.arp);
        assert_eq!(theme_scales(T::Steampunk)[0], PENTATONIC_MINOR);
        let sol = voice_for(&scene_with(T::Solarpunk, 0.0), ID_SEED);
        assert!(matches!(sol.wave, Wave::Sine) && !sol.arp && sol.detune_cents == 0.0);
        assert_eq!(theme_scales(T::Solarpunk)[0], PENTATONIC_MAJOR);
        let spo = voice_for(&scene_with(T::SpaceOutpost, 0.0), ID_SEED);
        assert!(matches!(spo.wave, Wave::Sine) && spo.octave > 1.0);
        assert_eq!(theme_scales(T::SpaceOutpost)[0], PENTATONIC_MINOR);
        let fan = voice_for(&scene_with(T::Fantasy, 0.0), ID_SEED);
        assert!(matches!(fan.wave, Wave::Triangle) && fan.arp && fan.octave > 1.0);
        assert_eq!(theme_scales(T::Fantasy)[0], PENTATONIC_MAJOR);
        let got = voice_for(&scene_with(T::GothicHorror, 0.0), ID_SEED);
        assert!(matches!(got.wave, Wave::Sawtooth) && !got.arp && got.attack_s > 0.1);
        assert_eq!(theme_scales(T::GothicHorror)[0], PHRYGIAN);
        let alo = voice_for(&scene_with(T::AlienOrganic, 0.0), ID_SEED);
        assert!(matches!(alo.wave, Wave::Sine) && alo.detune_cents > 0.0);
        assert_eq!(theme_scales(T::AlienOrganic)[0], PHRYGIAN);
        let alm = voice_for(&scene_with(T::AlienMonolithic, 0.0), ID_SEED);
        assert!(matches!(alm.wave, Wave::Sine) && alm.octave < 1.0 && alm.detune_cents == 0.0);
        assert_eq!(theme_scales(T::AlienMonolithic)[0], PHRYGIAN);
        let pa = voice_for(&scene_with(T::PostApoc, 0.0), ID_SEED);
        assert!(matches!(pa.wave, Wave::Sawtooth) && pa.octave < 1.0 && !pa.arp);
        assert_eq!(theme_scales(T::PostApoc)[0], PENTATONIC_MINOR);
        let ww = voice_for(&scene_with(T::WildWest, 0.0), ID_SEED);
        assert!(
            matches!(ww.wave, Wave::Triangle)
                && !ww.arp
                && ww.detune_cents > 0.0
                && (ww.octave - 1.0).abs() < 1e-6
        );
        assert_eq!(theme_scales(T::WildWest)[0], PENTATONIC_MAJOR);
    }

    /// Every mode a theme can emit stays inside its curated family, and the
    /// multi-mode themes actually exercise more than one mode across seeds
    /// (the across-room harmonic variety #500 calls for).
    #[test]
    fn voice_mode_stays_in_family_and_multi_mode_themes_vary() {
        use ThemeArchetype as T;
        for theme in [
            T::Cyberpunk,
            T::Medieval,
            T::GothicHorror,
            T::WildWest,
            T::Nordic,
            T::AncientClassical,
            T::FeudalJapan,
            T::Suburban,
        ] {
            let mut seen = std::collections::BTreeSet::new();
            for s in 0..128u64 {
                let v = voice_for(&scene_with(theme, 0.0), s);
                assert!(
                    theme_scales(theme).iter().any(|sc| *sc == v.scale),
                    "{theme:?} emitted an out-of-family scale"
                );
                seen.insert(v.scale.as_ptr() as usize);
            }
            let expected = theme_scales(theme).len().min(2).max(1);
            if theme_scales(theme).len() > 1 {
                assert!(
                    seen.len() >= expected,
                    "{theme:?} never varied its mode across seeds"
                );
            } else {
                assert_eq!(seen.len(), 1, "{theme:?} is single-mode by design");
            }
        }
    }

    /// The longest-tailed voice (AlienMonolithic: gate ≤ 2.5 + release 2.5,
    /// widened by the variety jitter) is the binding case for
    /// `ONSET_TAIL_BEATS`. Every onset's gate + release must land inside the
    /// loop + crossfade overhang — for *every* seed, at the escalation that
    /// keeps gates longest (calm; conflict tightens them).
    #[test]
    fn longest_tailed_voice_stays_inside_the_overhang() {
        let overhang = WARMUP_BEATS + LOOP_BEATS + super::super::CROSSFADE_BEATS;
        for s in 0..256u64 {
            let mut scene = scene_with(ThemeArchetype::AlienMonolithic, 0.0);
            scene.escalation = 0.0;
            scene.prosperity = 1.0;
            let mut voice = voice_for(&scene, s);
            apply_socio(&mut voice, &scene);
            let mut rng = ChaCha8Rng::seed_from_u64(s);
            for e in build_events(&voice, &mut rng) {
                assert!(
                    e.time_beats + e.gate_beats + e.release_beats <= overhang + 1e-3,
                    "seed {s}: tail {} exceeds overhang {overhang}",
                    e.time_beats + e.gate_beats + e.release_beats
                );
            }
        }
    }

    /// The bass pad sits an octave below the melody for low-drone themes
    /// (was unison, doubling the drone) while mid/high themes keep the deep
    /// fixed pad.
    #[test]
    fn bass_separates_from_low_drones() {
        // Mid/high themes keep the deep fixed bass register.
        assert_eq!(bass_octave_for(1.5), 0.5);
        assert_eq!(bass_octave_for(1.25), 0.5);
        assert_eq!(bass_octave_for(1.0), 0.5);
        // Low-drone themes drop to a real octave below their melody.
        assert_eq!(bass_octave_for(0.75), 0.375);
        assert_eq!(bass_octave_for(0.5), 0.25);
        assert!(
            bass_octave_for(0.5) < 0.5,
            "low drones must separate from the pad, not double it at unison"
        );
    }

    /// Voice variety is a pure function of the seed — same seed, same voice.
    #[test]
    fn voice_variety_is_deterministic_in_seed() {
        let a = voice_for(&scene_with(ThemeArchetype::Medieval, 0.0), 42);
        let b = voice_for(&scene_with(ThemeArchetype::Medieval, 0.0), 42);
        assert_eq!(a.scale.as_ptr(), b.scale.as_ptr());
        assert_eq!(a.release_s, b.release_s);
        assert_eq!(a.decay_s, b.decay_s);
        assert_eq!(a.detune_cents, b.detune_cents);
        assert_eq!(a.reverb_mix, b.reverb_mix);
    }

    #[test]
    fn conflict_makes_the_voice_busier_and_more_dissonant() {
        let mut scene = scene_with(ThemeArchetype::Medieval, 0.0);
        scene.prosperity = 0.5;
        scene.escalation = 0.0;
        let mut calm = voice_for(&scene, ID_SEED);
        apply_socio(&mut calm, &scene);

        scene.escalation = 1.0;
        let mut war = voice_for(&scene, ID_SEED);
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
        let mut rich = voice_for(&scene, ID_SEED);
        apply_socio(&mut rich, &scene);
        scene.prosperity = 0.05;
        let mut poor = voice_for(&scene, ID_SEED);
        apply_socio(&mut poor, &scene);
        assert!(rich.reverb_mix > poor.reverb_mix, "rich rings more present");
        assert!(rich.volume.1 > poor.volume.1, "rich is louder");
        assert!(rich.volume.1 <= 0.3, "still under the bed");
    }
}
