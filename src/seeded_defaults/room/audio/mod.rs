//! Seeded ambient-audio recipe deriver — the orchestrator.
//!
//! Assembles one [`bevy_symbios_audio::SequenceRecipe`] per room from
//! layers that mirror the biome/theme axis split used everywhere else:
//!
//! - **Biome texture** ([`bed`]) — an atonal noise bed plus a wind-gust
//!   voice. This is the environment's *sound*.
//! - **Biome punctuation** ([`punctuation`]) — the biome's natural
//!   signature voice (bird chirps, distant howls, …).
//! - **Theme music** ([`theme_music`]) — a tonal melodic voice plus a
//!   low bass-pad voice, both authored per-theme. This is the
//!   settlement's *music*.
//! - **Tension** ([`tension`]) — a conflict-gated siren layer, present
//!   only when escalation reaches Conflict.
//!
//! All contribute instruments + tracks into one recipe; the sequencer
//! plays the tracks simultaneously. The biome texture is derived first
//! so the theme music can reuse its acoustic space (reverb). The summed
//! per-event volumes are kept under the mixdown tanh soft-clip knee.
//!
//! # Looping
//!
//! `WARMUP_BEATS` of run-up (states warm) + `LOOP_BEATS` of loop region
//! (`loop_start_beats = WARMUP_BEATS`). Sustained voices carry
//! `release_beats = CROSSFADE_BEATS` so the baker has tail material to
//! blend into the loop start, and every LFO rate is whole cycles per
//! loop region so modulation phase is continuous across the seam.

mod bed;
mod punctuation;
mod scales;
mod tension;
mod theme_music;

use bevy_symbios_audio::{Event, PitchMode, SequenceRecipe};
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::seeded_defaults::scene::SceneCharacter;

/// Sub-stream salt distinct from palette / terrain / textures / atmosphere.
const AUDIO_STREAM_SALT: u64 = 0xAD17_BEEF_C0DE_AC1D;

/// One-shot run-up before the loop region — long enough for filter /
/// reverb states to reach steady level so the loop never replays the
/// cold-start fade-in.
pub(super) const WARMUP_BEATS: f32 = 2.0;
/// Length of the looped region (= seconds at 60 BPM). A longer loop
/// (#459) so the ambient music doesn't read as a fast tile; the melody
/// fills it with A/B phrase variation + rests and a low pad layer, and
/// every LFO rate stays whole-cycles-per-loop by construction.
pub(super) const LOOP_BEATS: f32 = 32.0;

/// Uniform pick of `lo..=hi` whole LFO cycles per loop region, returned
/// as a rate in Hz. Whole-cycle rates make the modulation phase identical
/// at the loop start and loop end, so the seam never jumps mid-sweep.
/// Shared by the bed and tension layers (#655 — was a literal duplicate).
pub(super) fn loop_synced_rate(rng: &mut rand_chacha::ChaCha8Rng, lo: u32, hi: u32) -> f32 {
    let cycles = lo
        + (crate::seeded_defaults::scene::range_f32(rng, 0.0, (hi - lo + 1) as f32) as u32)
            .min(hi - lo);
    // 60 BPM: one beat = one second, so the loop region is LOOP_BEATS seconds.
    cycles as f32 / LOOP_BEATS
}

/// Tail-crossfade window blending the timeline end into the loop start.
pub(super) const CROSSFADE_BEATS: f32 = 2.0;

/// Top-level seeded recipe — the value the wiring layer hands to the PDS
/// record (and the loading-gate baker consumes).
pub struct AmbientRecipe {
    pub recipe: SequenceRecipe,
}

impl AmbientRecipe {
    /// Derive a deterministic ambient recipe from the room's scene anchor
    /// and the room seed. Same inputs -> same recipe.
    pub fn from_scene(scene: &SceneCharacter, room_seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(room_seed ^ AUDIO_STREAM_SALT);
        // Biome texture first — it derives the shared acoustic space.
        let (params, mut instruments, mut tracks) = bed::build_texture(scene, &mut rng, room_seed);
        // Biome punctuation (the natural signature voice) then theme music
        // (the melody) then the theme bass pad — all reuse the bed's reverb
        // space. Order fixes the layer indices at
        // [bed, gust, punct, melody, bass].
        let (punct_instrument, punct_track) =
            punctuation::build(scene, &params, &mut rng, room_seed);
        instruments.push(punct_instrument);
        tracks.push(punct_track);
        let (theme_instrument, theme_track) =
            theme_music::build(scene, &params, &mut rng, room_seed);
        instruments.push(theme_instrument);
        tracks.push(theme_track);
        // Low pad / drone second theme voice (#459) — always present so the
        // longer loop reads layered, with seeded voicing. Index 4.
        let (bass_instrument, bass_track) =
            theme_music::build_bass(scene, &params, &mut rng, room_seed);
        instruments.push(bass_instrument);
        tracks.push(bass_track);
        // Conflict-only tension siren (gated): an extra layer appears only
        // when escalation reaches Conflict, so calm/tense rooms keep the
        // five-layer recipe. Pushed last (index 5) so the theme voices keep
        // indices 3 (melody) and 4 (bass).
        if let Some((tension_instrument, tension_track)) =
            tension::build(scene, &params, &mut rng, room_seed)
        {
            instruments.push(tension_instrument);
            tracks.push(tension_track);
        }

        let recipe = SequenceRecipe {
            bpm: 60.0,
            // 22.05 kHz halves the baked WAV and its decoded playback buffer
            // versus 44.1 kHz; the ambient bed's pad / drone content sits well
            // within the 11 kHz Nyquist, so the memory win is inaudible. On
            // wasm this is doubly worth it — freed linear memory never returns
            // to the OS, so every re-bake's high-water mark is permanent (#568).
            sample_rate: 22_050,
            duration_beats: WARMUP_BEATS + LOOP_BEATS,
            loop_start_beats: Some(WARMUP_BEATS),
            loop_crossfade_beats: CROSSFADE_BEATS,
            instruments,
            tracks,
        };
        Self { recipe }
    }
}

/// Full-timeline sustained event for the bed + gust texture tracks: one
/// voice covering the whole loop, with a release tail past the end so the
/// loop crossfade has real material to blend into the loop start.
pub(super) fn sustained(id: &str, volume: f32) -> Event {
    Event {
        time_beats: 0.0,
        instrument_id: id.to_string(),
        pitch_multiplier: 1.0,
        volume,
        gate_beats: WARMUP_BEATS + LOOP_BEATS,
        release_beats: CROSSFADE_BEATS,
        pitch_mode: PitchMode::Varispeed,
    }
}

#[cfg(test)]
mod tests {
    use super::punctuation::PUNCT_INSTRUMENT_ID;
    use super::*;
    use crate::seeded_defaults::scene::BiomeArchetype;
    use bevy_symbios_audio::{AudioPatch, NodeKind};

    #[test]
    fn recipe_layers_biome_texture_and_theme_music() {
        // The framework contract: every recipe carries the biome layer
        // (bed + gust + punctuation) AND a theme music melody voice. Pin a
        // peaceful room so the conflict tension layer doesn't add a fifth.
        let mut scene = SceneCharacter::for_seed(3);
        scene.escalation = 0.0;
        let recipe = AmbientRecipe::from_scene(&scene, 3).recipe;
        assert_eq!(recipe.instruments.len(), 5);
        let ids: Vec<&str> = recipe.instruments.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"ambient_bed"), "missing biome texture bed");
        assert!(ids.contains(&"gust_swell"), "missing biome texture gust");
        assert!(
            ids.contains(&PUNCT_INSTRUMENT_ID),
            "missing biome punctuation"
        );
        // Theme melody is index 3; its id varies by theme (theme_synth,
        // theme_lyre, theme_melody, …).
        assert!(
            recipe.instruments[3].id.starts_with("theme_"),
            "missing theme music voice"
        );
        // Theme bass pad is index 4 (#459).
        assert_eq!(
            recipe.instruments[4].id, "theme_bass",
            "missing theme bass pad"
        );
    }

    #[test]
    fn deterministic_five_layer_recipe() {
        let mut scene = SceneCharacter::for_seed(9);
        scene.escalation = 0.0; // peaceful: exactly the five base layers
        let a = AmbientRecipe::from_scene(&scene, 9);
        let b = AmbientRecipe::from_scene(&scene, 9);
        assert_eq!(
            a.recipe.instruments.len(),
            5,
            "bed + gust + punctuation + theme melody + theme bass"
        );
        assert_eq!(a.recipe.tracks.len(), 5);
        let ev_a = &a.recipe.tracks[3].events;
        let ev_b = &b.recipe.tracks[3].events;
        assert_eq!(ev_a.len(), ev_b.len());
        for (x, y) in ev_a.iter().zip(ev_b.iter()) {
            assert_eq!(x.time_beats, y.time_beats);
            assert_eq!(x.pitch_multiplier, y.pitch_multiplier);
            assert_eq!(x.volume, y.volume);
        }
    }

    #[test]
    fn conflict_room_adds_a_sixth_tension_layer() {
        use super::tension::TENSION_INSTRUMENT_ID;
        // A conflict room grows a siren layer on top of the five base layers;
        // a calm room of the same seed keeps the five base layers.
        let mut war = SceneCharacter::for_seed(2);
        war.escalation = 0.95;
        let war_recipe = AmbientRecipe::from_scene(&war, 2).recipe;
        assert_eq!(war_recipe.instruments.len(), 6, "conflict adds the siren");
        assert_eq!(war_recipe.instruments[5].id, TENSION_INSTRUMENT_ID);
        assert_eq!(war_recipe.tracks.len(), 6);
        // The theme melody is still index 3 and the bass index 4 — the siren
        // appends after them.
        assert!(war_recipe.instruments[3].id.starts_with("theme_"));
        assert_eq!(war_recipe.instruments[4].id, "theme_bass");

        let mut calm = SceneCharacter::for_seed(2);
        calm.escalation = 0.0;
        assert_eq!(
            AmbientRecipe::from_scene(&calm, 2).recipe.instruments.len(),
            5,
            "calm room keeps the five base layers"
        );
    }

    #[test]
    fn loop_region_starts_hot_and_lfos_are_loop_synced() {
        for s in 0u64..16 {
            let scene = SceneCharacter::for_seed(s);
            let recipe = AmbientRecipe::from_scene(&scene, s).recipe;
            assert_eq!(recipe.duration_beats, WARMUP_BEATS + LOOP_BEATS);
            assert_eq!(recipe.loop_start_beats, Some(WARMUP_BEATS));
            // The sustained voices must cover the whole timeline and
            // leave a release tail for the crossfade — a zero tail is
            // exactly the fade-in-at-loop-start bug.
            for track in &recipe.tracks[0..2] {
                let e = &track.events[0];
                assert_eq!(e.time_beats, 0.0);
                assert_eq!(e.gate_beats, recipe.duration_beats);
                assert_eq!(e.release_beats, recipe.loop_crossfade_beats);
            }
            // Every LFO rate is whole cycles per loop region (loop is
            // LOOP_BEATS seconds at 60 BPM) so modulation phase is
            // continuous across the seam.
            for instrument in &recipe.instruments {
                for node in &instrument.patch.graph.nodes {
                    if let NodeKind::Lfo(lfo) = &node.kind {
                        let cycles = lfo.rate_hz * LOOP_BEATS;
                        assert!(
                            (cycles - cycles.round()).abs() < 1e-4,
                            "LFO rate {} Hz is not loop-synced ({cycles} cycles)",
                            lfo.rate_hz
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn theme_melody_stays_in_loop_region_and_under_the_bed() {
        for s in 0u64..32 {
            let scene = SceneCharacter::for_seed(s);
            let recipe = AmbientRecipe::from_scene(&scene, s).recipe;
            let melody = &recipe.tracks[3].events;
            assert!(!melody.is_empty(), "every room gets a melodic voice");
            // Sparse voices are a few notes; arpeggios are denser but
            // still bounded by MAX_NOTES even on the long loop.
            assert!(melody.len() <= 40, "melody {} over MAX_NOTES", melody.len());
            for e in melody {
                // Never in the play-once warm-up run-up.
                assert!(e.time_beats >= WARMUP_BEATS);
                // Tails may overhang the timeline end by at most the
                // crossfade window (that overhang *is* the seam blend).
                assert!(
                    e.time_beats + e.gate_beats + e.release_beats
                        <= recipe.duration_beats + recipe.loop_crossfade_beats + 1e-3,
                    "melody tail exceeds the crossfade overhang: onset {} gate {} release {}",
                    e.time_beats,
                    e.gate_beats,
                    e.release_beats
                );
                assert!(e.volume <= 0.3, "melody stays under the bed");
            }
            // Onsets are sorted so peers serialise identical recipes.
            for pair in melody.windows(2) {
                assert!(pair[0].time_beats <= pair[1].time_beats);
            }
        }
    }

    #[test]
    fn longer_loop_is_filled_with_phrases_and_a_bass_pad() {
        // #459: the loop is lengthened, and the melody phrases + a low bass
        // pad fill it rather than tiling the first 16 s.
        const { assert!(LOOP_BEATS >= 32.0, "loop region lengthened") };
        let mut melody_reaches_second_half = false;
        for s in 0u64..24 {
            let mut scene = SceneCharacter::for_seed(s);
            scene.escalation = 0.0;
            let recipe = AmbientRecipe::from_scene(&scene, s).recipe;
            // Bass pad is index 4: present, under the bed, inside the loop.
            let bass = &recipe.tracks[4].events;
            assert!(!bass.is_empty(), "seed {s}: bass pad present");
            for e in bass {
                assert!(e.volume <= 0.12, "bass stays under the bed");
                assert!(e.time_beats >= WARMUP_BEATS);
                assert!(
                    e.time_beats + e.gate_beats + e.release_beats
                        <= recipe.duration_beats + recipe.loop_crossfade_beats + 1e-3,
                    "bass tail exceeds the crossfade overhang"
                );
            }
            if recipe.tracks[3]
                .events
                .iter()
                .any(|e| e.time_beats > WARMUP_BEATS + LOOP_BEATS * 0.5)
            {
                melody_reaches_second_half = true;
            }
        }
        assert!(
            melody_reaches_second_half,
            "some room's melody should reach the loop's second half"
        );
    }

    #[test]
    fn punctuation_onsets_span_the_full_loop() {
        // #663: the onset window derives from LOOP_BEATS — a hardcoded
        // 12-beat window used to cluster punctuation in the loop's first
        // ~40% and leave the tail silent. Across seeds, some onsets must
        // land past the old ceiling, and none past the loop region.
        let old_ceiling = WARMUP_BEATS + 12.0;
        let mut reaches_past_old_window = false;
        for s in 0u64..24 {
            let scene = SceneCharacter::for_seed(s);
            let recipe = AmbientRecipe::from_scene(&scene, s).recipe;
            for e in &recipe.tracks[2].events {
                assert!(e.time_beats >= WARMUP_BEATS);
                assert!(
                    e.time_beats <= WARMUP_BEATS + LOOP_BEATS,
                    "onset outside the loop region"
                );
                if e.time_beats > old_ceiling {
                    reaches_past_old_window = true;
                }
            }
        }
        assert!(
            reaches_past_old_window,
            "some seed's punctuation should land past the old 12-beat window"
        );
    }

    #[test]
    fn punctuation_differs_in_kind_per_biome() {
        use BiomeArchetype::*;
        // The punctuation patch must be structurally different across
        // biomes — tonal moods carry a Sine source, noise moods a
        // WhiteNoise → Bandpass chain — and every event must fit the
        // loop-plus-crossfade window.
        let has_sine = |patch: &AudioPatch| {
            patch
                .graph
                .nodes
                .iter()
                .any(|n| matches!(n.kind, NodeKind::Sine(_)))
        };
        // Tonal punctuation moods (BirdChirps / SubBoom / IceTing /
        // DistantHowl / FrogChorus) carry a Sine; WaveWash / WhistleGust /
        // InsectChorus are filtered-noise voices.
        let expects_sine = |b: BiomeArchetype| {
            matches!(
                b,
                Lush | TemperateForest
                    | Jungle
                    | Volcanic
                    | Tundra
                    | Glacial
                    | Alpine
                    | Boreal
                    | Wetland
            )
        };
        for biome in BiomeArchetype::ALL {
            for s in 0u64..4 {
                let mut scene = SceneCharacter::for_seed(s);
                scene.biome = biome;
                let recipe = AmbientRecipe::from_scene(&scene, s).recipe;
                let punct_patch = &recipe.instruments[2].patch;
                assert_eq!(
                    has_sine(punct_patch),
                    expects_sine(biome),
                    "{biome:?} punctuation voice has the wrong source family"
                );
                let events = &recipe.tracks[2].events;
                assert!(!events.is_empty(), "{biome:?} has no punctuation events");
                for e in events {
                    assert_eq!(e.instrument_id, PUNCT_INSTRUMENT_ID);
                    assert!(e.time_beats >= WARMUP_BEATS);
                    assert!(
                        e.time_beats + e.gate_beats + e.release_beats
                            <= recipe.duration_beats + recipe.loop_crossfade_beats + 1e-3,
                        "{biome:?} punctuation tail exceeds the crossfade overhang"
                    );
                }
            }
        }
        // Arid/tundra beds run highpass; lush stays lowpass.
        for (biome, expect_hp) in [(Arid, true), (Tundra, true), (Lush, false)] {
            let mut scene = SceneCharacter::for_seed(1);
            scene.biome = biome;
            let recipe = AmbientRecipe::from_scene(&scene, 1).recipe;
            let bed = &recipe.instruments[0].patch;
            let hp = bed
                .graph
                .nodes
                .iter()
                .any(|n| matches!(n.kind, NodeKind::BiquadHighpass(_)));
            assert_eq!(hp, expect_hp, "{biome:?} bed filter family wrong");
        }
    }
}
