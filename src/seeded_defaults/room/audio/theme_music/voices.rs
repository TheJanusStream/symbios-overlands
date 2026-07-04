//! The per-[`ThemeArchetype`] voice table: each theme's authored
//! [`ThemeVoice`] (timbre + scale + note pattern), the seeded variety /
//! biome-register / socio-political adjustments layered on top, and the
//! [`voice_for`] entry point the orchestrator calls.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use super::super::scales::{DORIAN, HIRAJOSHI, PENTATONIC_MAJOR, PENTATONIC_MINOR, PHRYGIAN};
use crate::seeded_defaults::scene::{BiomeArchetype, SceneCharacter, ThemeArchetype, unit_f32};

#[derive(Clone, Copy)]
pub(super) enum Wave {
    Sine,
    Triangle,
    Sawtooth,
}

/// A theme's melodic voice: a single synth instrument plus the shape of
/// the pattern it plays.
pub(super) struct ThemeVoice {
    /// Stable instrument id.
    pub(super) id: &'static str,
    pub(super) wave: Wave,
    /// Cents of detune for a second stacked oscillator (synth width);
    /// `0.0` = a single oscillator.
    pub(super) detune_cents: f32,
    /// Just-intonation ratio table the pattern walks.
    pub(super) scale: &'static [f32],
    /// Octave multiplier applied on top of the biome register.
    pub(super) octave: f32,
    pub(super) attack_s: f32,
    pub(super) decay_s: f32,
    pub(super) sustain_level: f32,
    pub(super) release_s: f32,
    /// Inclusive notes-per-loop band.
    pub(super) note_count: (u32, u32),
    /// Per-note gate-length band (beats).
    pub(super) gate: (f32, f32),
    /// Per-note volume band (kept under the bed).
    pub(super) volume: (f32, f32),
    /// Dense even eighth-note arpeggio (`true`) vs sparse scattered
    /// onsets (`false`).
    pub(super) arp: bool,
    pub(super) reverb_mix: f32,
}

/// Biome register multiplier — volcanic tolls an octave down, tundra
/// rings an octave up, alpine a fifth. Keeps the music seated in the
/// biome even when the theme owns the melody.
pub(super) fn biome_register(biome: BiomeArchetype) -> f32 {
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

/// The in-character *alternate* modes a theme may drift into, beyond its
/// signature scale (which lives on the voice literal in [`base_voice`] — the
/// single source of truth for the signature). [`apply_voice_variety`] picks
/// across the signature plus these by seed, so two settlements of the same
/// theme can sit in different modes while staying inside the theme's harmonic
/// family. Empty for a single-mode theme — one whose mode *is* its identity
/// (Feudal-Japan's Hirajōshi) or whose brightness only one scale carries (the
/// sunny-major themes); those draw their variety from key / register / voicing
/// / pattern instead.
pub(super) fn theme_alt_scales(theme: ThemeArchetype) -> &'static [&'static [f32]] {
    match theme {
        ThemeArchetype::Cyberpunk => &[PENTATONIC_MINOR],
        ThemeArchetype::FeudalJapan => &[],
        ThemeArchetype::IndustrialPark => &[PENTATONIC_MINOR],
        ThemeArchetype::RuralFarmland => &[DORIAN],
        ThemeArchetype::Suburban => &[],
        ThemeArchetype::ModernCity => &[DORIAN],
        ThemeArchetype::Mesoamerican => &[PHRYGIAN],
        ThemeArchetype::Nordic => &[DORIAN],
        ThemeArchetype::Medieval => &[PHRYGIAN],
        ThemeArchetype::WildWest => &[PENTATONIC_MINOR],
        ThemeArchetype::PostApoc => &[PHRYGIAN],
        ThemeArchetype::AlienMonolithic => &[PENTATONIC_MINOR],
        ThemeArchetype::AlienOrganic => &[DORIAN],
        ThemeArchetype::GothicHorror => &[PENTATONIC_MINOR],
        ThemeArchetype::Fantasy => &[DORIAN],
        ThemeArchetype::SpaceOutpost => &[PHRYGIAN],
        ThemeArchetype::Solarpunk => &[],
        ThemeArchetype::Steampunk => &[DORIAN],
        ThemeArchetype::SportsRec => &[],
        ThemeArchetype::CivicCampus => &[PENTATONIC_MAJOR],
        ThemeArchetype::Roadside => &[PENTATONIC_MAJOR],
        ThemeArchetype::CoastalResort => &[],
        ThemeArchetype::AncientClassical => &[PHRYGIAN],
    }
}

/// Seeded per-room variety layered on top of the signature voice: choose a
/// mode from the theme's curated family and nudge the timbre (ring, colour,
/// articulation) a little, so the same theme reads fresh across rooms
/// without losing its identity. Deterministic in `seed`; its own rng stream
/// keeps it independent of the pattern generator. Octave / wave / attack are
/// left untouched — those carry the recognisable signature.
pub(super) fn apply_voice_variety(voice: &mut ThemeVoice, theme: ThemeArchetype, seed: u64) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ VOICE_VARIETY_SALT);
    // The harmonic family is the signature scale (already on the voice — the
    // single source) plus the theme's alternates. Index 0 keeps the signature,
    // so the draw maps exactly as it did when the signature headed a combined
    // list; only a higher index swaps in an alternate.
    let alts = theme_alt_scales(theme);
    let family_len = 1 + alts.len();
    let idx = ((unit_f32(&mut rng) * family_len as f32) as usize).min(family_len - 1);
    if idx > 0 {
        voice.scale = alts[idx - 1];
    }

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

/// The theme's signature voice — its authored timbre + signature scale, before
/// any per-room variety. Exhaustive over [`ThemeArchetype`], so a new theme
/// must add a voice here. The `scale` field is the single source of truth for
/// the theme's signature mode; [`theme_alt_scales`] lists only the *other*
/// modes the family allows.
pub(super) fn base_voice(theme: ThemeArchetype) -> ThemeVoice {
    match theme {
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
    }
}

/// The voice for a room: the theme's [`base_voice`] with seeded per-room
/// variety layered on top (mode pick + timbre jitter).
pub(super) fn voice_for(scene: &SceneCharacter, seed: u64) -> ThemeVoice {
    let mut voice = base_voice(scene.theme);
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
pub(super) fn apply_socio(voice: &mut ThemeVoice, scene: &SceneCharacter) {
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
