//! Theme ambient *music* — the tonal melodic voice that gives a
//! settlement its character. Each
//! [`ThemeArchetype`](crate::seeded_defaults::scene::ThemeArchetype) maps to a
//! [`ThemeVoice`](voices::ThemeVoice) descriptor (instrument timbre +
//! scale + note pattern); the
//! match is exhaustive, so every theme has an authored voice and a new
//! archetype must add one. The biome still nudges the register and the voice
//! shares the bed's reverb space — so some of the music is "based on biome"
//! while its identity comes from the theme.
//!
//! Split (#655): [`voices`] holds the per-theme voice table and its
//! seeded/socio adjustments, [`patch`] the synth-graph plumbing, and
//! [`patterns`] the note/onset generation; this mod keeps the [`build`]
//! orchestration + the identity tests.

mod patch;
mod patterns;
mod voices;

pub(crate) use patterns::build_bass;

use bevy_symbios_audio::{Instrument, Track};
use rand_chacha::ChaCha8Rng;

use super::bed::AmbientParams;
use crate::seeded_defaults::scene::SceneCharacter;

use patch::build_patch;
use patterns::build_events;
#[cfg(test)]
use voices::Wave;
use voices::{apply_socio, biome_register, voice_for};

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
    use super::super::scales::{DORIAN, HIRAJOSHI, PENTATONIC_MAJOR, PENTATONIC_MINOR, PHRYGIAN};
    use super::super::{LOOP_BEATS, WARMUP_BEATS};
    use super::patterns::bass_octave_for;
    use super::voices::{base_voice, theme_alt_scales};
    use super::*;
    use crate::seeded_defaults::scene::ThemeArchetype;
    use rand_chacha::rand_core::SeedableRng;

    fn scene_with(theme: ThemeArchetype, temp: f32) -> SceneCharacter {
        let mut s = SceneCharacter::for_seed(7);
        s.theme = theme;
        s.temperature = temp;
        s
    }

    /// A fixed seed for the identity checks. Wave / arp / octave / attack /
    /// detune-sign are seed-stable (variety only picks the mode + jitters the
    /// ring/colour), so any seed exercises the signature; the *signature
    /// scale* is asserted via `base_voice(..).scale` (the unvaried voice) since
    /// the active mode is now seed-picked from the family.
    const ID_SEED: u64 = 0xA5A5;

    #[test]
    fn authored_themes_use_their_signature_wave_and_scale() {
        use ThemeArchetype as T;
        let cy = voice_for(&scene_with(T::Cyberpunk, 0.0), ID_SEED);
        assert!(matches!(cy.wave, Wave::Sawtooth) && cy.arp && cy.detune_cents > 0.0);
        assert_eq!(base_voice(T::Cyberpunk).scale, PHRYGIAN);
        assert_eq!(base_voice(T::Medieval).scale, DORIAN);
        let nor = voice_for(&scene_with(T::Nordic, 0.0), ID_SEED);
        assert!(matches!(nor.wave, Wave::Triangle) && !nor.arp && nor.octave < 1.0);
        assert_eq!(base_voice(T::Nordic).scale, PENTATONIC_MINOR);
        assert_eq!(base_voice(T::FeudalJapan).scale, HIRAJOSHI);
        let meso = voice_for(&scene_with(T::Mesoamerican, 0.0), ID_SEED);
        assert!(matches!(meso.wave, Wave::Sine) && !meso.arp);
        assert_eq!(base_voice(T::Mesoamerican).scale, PENTATONIC_MINOR);
        let city = voice_for(&scene_with(T::ModernCity, 0.0), ID_SEED);
        assert!(matches!(city.wave, Wave::Sawtooth) && !city.arp && city.detune_cents > 0.0);
        assert_eq!(base_voice(T::ModernCity).scale, PENTATONIC_MAJOR);
        let sub = voice_for(&scene_with(T::Suburban, 0.0), ID_SEED);
        assert!(matches!(sub.wave, Wave::Triangle) && sub.octave > 1.0);
        assert_eq!(base_voice(T::Suburban).scale, PENTATONIC_MAJOR);
        let farm = voice_for(&scene_with(T::RuralFarmland, 0.0), ID_SEED);
        assert!(
            matches!(farm.wave, Wave::Sawtooth) && farm.detune_cents > 0.0 && farm.attack_s < 0.1
        );
        assert_eq!(base_voice(T::RuralFarmland).scale, PENTATONIC_MAJOR);
        let ind = voice_for(&scene_with(T::IndustrialPark, 0.0), ID_SEED);
        assert!(matches!(ind.wave, Wave::Sawtooth) && !ind.arp && ind.octave < 1.0);
        assert_eq!(base_voice(T::IndustrialPark).scale, PHRYGIAN);
        assert_eq!(base_voice(T::AncientClassical).scale, DORIAN);
        let coast = voice_for(&scene_with(T::CoastalResort, 0.0), ID_SEED);
        assert!(matches!(coast.wave, Wave::Sine) && coast.detune_cents > 0.0 && coast.octave > 1.0);
        assert_eq!(base_voice(T::CoastalResort).scale, PENTATONIC_MAJOR);
        let road = voice_for(&scene_with(T::Roadside, 0.0), ID_SEED);
        assert!(matches!(road.wave, Wave::Triangle) && road.detune_cents > 0.0 && !road.arp);
        assert_eq!(base_voice(T::Roadside).scale, PENTATONIC_MINOR);
        let civ = voice_for(&scene_with(T::CivicCampus, 0.0), ID_SEED);
        assert!(matches!(civ.wave, Wave::Sawtooth) && civ.detune_cents > 0.0 && civ.attack_s > 0.1);
        assert_eq!(base_voice(T::CivicCampus).scale, DORIAN);
        let spr = voice_for(&scene_with(T::SportsRec, 0.0), ID_SEED);
        assert!(matches!(spr.wave, Wave::Sawtooth) && spr.arp && spr.detune_cents > 0.0);
        assert_eq!(base_voice(T::SportsRec).scale, PENTATONIC_MAJOR);
        let stm = voice_for(&scene_with(T::Steampunk, 0.0), ID_SEED);
        assert!(matches!(stm.wave, Wave::Triangle) && stm.arp);
        assert_eq!(base_voice(T::Steampunk).scale, PENTATONIC_MINOR);
        let sol = voice_for(&scene_with(T::Solarpunk, 0.0), ID_SEED);
        assert!(matches!(sol.wave, Wave::Sine) && !sol.arp && sol.detune_cents == 0.0);
        assert_eq!(base_voice(T::Solarpunk).scale, PENTATONIC_MAJOR);
        let spo = voice_for(&scene_with(T::SpaceOutpost, 0.0), ID_SEED);
        assert!(matches!(spo.wave, Wave::Sine) && spo.octave > 1.0);
        assert_eq!(base_voice(T::SpaceOutpost).scale, PENTATONIC_MINOR);
        let fan = voice_for(&scene_with(T::Fantasy, 0.0), ID_SEED);
        assert!(matches!(fan.wave, Wave::Triangle) && fan.arp && fan.octave > 1.0);
        assert_eq!(base_voice(T::Fantasy).scale, PENTATONIC_MAJOR);
        let got = voice_for(&scene_with(T::GothicHorror, 0.0), ID_SEED);
        assert!(matches!(got.wave, Wave::Sawtooth) && !got.arp && got.attack_s > 0.1);
        assert_eq!(base_voice(T::GothicHorror).scale, PHRYGIAN);
        let alo = voice_for(&scene_with(T::AlienOrganic, 0.0), ID_SEED);
        assert!(matches!(alo.wave, Wave::Sine) && alo.detune_cents > 0.0);
        assert_eq!(base_voice(T::AlienOrganic).scale, PHRYGIAN);
        let alm = voice_for(&scene_with(T::AlienMonolithic, 0.0), ID_SEED);
        assert!(matches!(alm.wave, Wave::Sine) && alm.octave < 1.0 && alm.detune_cents == 0.0);
        assert_eq!(base_voice(T::AlienMonolithic).scale, PHRYGIAN);
        let pa = voice_for(&scene_with(T::PostApoc, 0.0), ID_SEED);
        assert!(matches!(pa.wave, Wave::Sawtooth) && pa.octave < 1.0 && !pa.arp);
        assert_eq!(base_voice(T::PostApoc).scale, PENTATONIC_MINOR);
        let ww = voice_for(&scene_with(T::WildWest, 0.0), ID_SEED);
        assert!(
            matches!(ww.wave, Wave::Triangle)
                && !ww.arp
                && ww.detune_cents > 0.0
                && (ww.octave - 1.0).abs() < 1e-6
        );
        assert_eq!(base_voice(T::WildWest).scale, PENTATONIC_MAJOR);
    }

    /// Every mode a theme can emit stays inside its curated family (the
    /// signature scale plus its alternates), and the multi-mode themes actually
    /// exercise more than one mode across seeds (the across-room harmonic
    /// variety #500 calls for).
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
            let signature = base_voice(theme).scale;
            let alts = theme_alt_scales(theme);
            let family_len = 1 + alts.len();
            let mut seen = std::collections::BTreeSet::new();
            for s in 0..128u64 {
                let v = voice_for(&scene_with(theme, 0.0), s);
                assert!(
                    v.scale == signature || alts.contains(&v.scale),
                    "{theme:?} emitted an out-of-family scale"
                );
                seen.insert(v.scale.as_ptr() as usize);
            }
            if family_len > 1 {
                assert!(
                    seen.len() >= family_len.min(2),
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
