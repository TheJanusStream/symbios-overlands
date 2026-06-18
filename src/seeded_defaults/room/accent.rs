//! Theme accents — the *light* nudge a [`ThemeArchetype`] applies back
//! onto the natural derivers so a settlement's surroundings echo its
//! artificial theme (cyberpunk magenta haze, alien biolume motes,
//! gothic gloom). Strictly additive and bounded: the biome palette stays
//! the primary nature driver, and a theme with no accent
//! ([`ThemeAccent::NEUTRAL`]) is a no-op.
//!
//! The accent is keyed on the room's own [`ThemeArchetype`] (not the
//! settlement's fallback theme), so a not-yet-authored theme still
//! previews its identity through the haze while its structures are still
//! standing in via the AncientClassical fallback.

use super::particles::ParticleMood;
use crate::seeded_defaults::scene::{SceneCharacter, ThemeArchetype};

/// Smoky dark-red the fog/sky drift toward as a room's escalation rises —
/// the haze of a fought-over settlement.
const SMOKE_RED: [f32; 3] = [0.50, 0.16, 0.10];

/// A bounded, additive nudge applied to the room's fog / sky / cloud and
/// (optionally) ambient particles after the biome derivers have run.
#[derive(Clone, Copy, Debug)]
pub struct ThemeAccent {
    /// sRGB colour the fog + sky are blended toward.
    pub tint: [f32; 3],
    /// Blend weight (`0..~0.4`) for [`Self::tint`] into fog/sky — kept
    /// small so the biome palette still dominates.
    pub tint_strength: f32,
    /// Additive cloud cover (`0..~0.45`) for smoggy / overcast / smoke-
    /// filled themes.
    pub haze: f32,
    /// Multiplicative brightness on fog / sky / cloud colour (`1.0` =
    /// unchanged). Prosperity nudges this — affluent rooms read a touch
    /// brighter, destitute ones dimmer.
    pub brightness: f32,
    /// Ambient particle mood override (`None` keeps the biome's mood).
    pub particle_mood: Option<ParticleMood>,
}

impl ThemeAccent {
    /// The do-nothing accent: leaves every natural deriver untouched.
    pub const NEUTRAL: Self = Self {
        tint: [0.0, 0.0, 0.0],
        tint_strength: 0.0,
        haze: 0.0,
        brightness: 1.0,
        particle_mood: None,
    };

    /// The accent for `theme`. Most themes are [`Self::NEUTRAL`]; the
    /// distinctive ones nudge fog / sky / particles toward their identity.
    pub fn for_theme(theme: ThemeArchetype) -> Self {
        use ThemeArchetype::*;
        match theme {
            Medieval => Self {
                tint: [0.46, 0.50, 0.60],
                tint_strength: 0.12,
                haze: 0.10,
                brightness: 1.0,
                particle_mood: None,
            },
            Cyberpunk => Self {
                tint: [0.85, 0.10, 0.70],
                tint_strength: 0.22,
                haze: 0.10,
                brightness: 1.0,
                particle_mood: None,
            },
            // Cold northern light — steel-blue fjord air over the steading.
            Nordic => Self {
                tint: [0.55, 0.66, 0.85],
                tint_strength: 0.16,
                haze: 0.06,
                brightness: 1.0,
                particle_mood: None,
            },
            // Soft blossom air — a warm rose haze with cherry petals drifting.
            FeudalJapan => Self {
                tint: [0.92, 0.74, 0.76],
                tint_strength: 0.12,
                haze: 0.05,
                brightness: 1.0,
                particle_mood: Some(ParticleMood::Petals),
            },
            // Humid jungle-gold air — warm amber haze over the temple city.
            Mesoamerican => Self {
                tint: [0.80, 0.66, 0.36],
                tint_strength: 0.14,
                haze: 0.12,
                brightness: 1.0,
                particle_mood: None,
            },
            // City smog — a cool grey haze hanging over the downtown.
            ModernCity => Self {
                tint: [0.60, 0.62, 0.66],
                tint_strength: 0.12,
                haze: 0.13,
                brightness: 1.0,
                particle_mood: None,
            },
            // A soft sunny haze over green lawns (birdsong rides the
            // community center's spatial fx).
            Suburban => Self {
                tint: [0.84, 0.86, 0.68],
                tint_strength: 0.08,
                haze: 0.0,
                brightness: 1.0,
                particle_mood: None,
            },
            AlienOrganic => Self {
                tint: [0.25, 0.85, 0.40],
                tint_strength: 0.20,
                haze: 0.08,
                brightness: 1.0,
                particle_mood: Some(ParticleMood::Fireflies),
            },
            AlienMonolithic => Self {
                tint: [0.40, 0.55, 0.95],
                tint_strength: 0.20,
                haze: 0.06,
                brightness: 1.0,
                particle_mood: Some(ParticleMood::MistMotes),
            },
            GothicHorror => Self {
                tint: [0.18, 0.18, 0.24],
                tint_strength: 0.24,
                haze: 0.18,
                brightness: 1.0,
                particle_mood: None,
            },
            IndustrialPark => Self {
                tint: [0.50, 0.50, 0.52],
                tint_strength: 0.18,
                haze: 0.20,
                brightness: 1.0,
                particle_mood: None,
            },
            Steampunk => Self {
                tint: [0.72, 0.52, 0.26],
                tint_strength: 0.18,
                haze: 0.16,
                brightness: 1.0,
                particle_mood: None,
            },
            RuralFarmland => Self {
                tint: [0.95, 0.80, 0.45],
                tint_strength: 0.12,
                haze: 0.0,
                brightness: 1.0,
                particle_mood: None,
            },
            PostApoc => Self {
                tint: [0.62, 0.55, 0.45],
                tint_strength: 0.18,
                haze: 0.16,
                brightness: 1.0,
                particle_mood: Some(ParticleMood::DustMotes),
            },
            // Bright clear-sky seaside air — a light sky-blue wash, no haze.
            CoastalResort => Self {
                tint: [0.55, 0.74, 0.92],
                tint_strength: 0.10,
                haze: 0.0,
                brightness: 1.0,
                particle_mood: None,
            },
            // Dusty sodium-amber highway air — warm grit hangs over the strip.
            Roadside => Self {
                tint: [0.66, 0.56, 0.42],
                tint_strength: 0.13,
                haze: 0.10,
                brightness: 1.0,
                particle_mood: Some(ParticleMood::DustMotes),
            },
            // Dignified warm-sandstone air — a soft golden academic light.
            CivicCampus => Self {
                tint: [0.86, 0.80, 0.66],
                tint_strength: 0.09,
                haze: 0.0,
                brightness: 1.0,
                particle_mood: None,
            },
            // Bright field-day air — a clean, faintly green daylight over the
            // turf, no haze.
            SportsRec => Self {
                tint: [0.78, 0.86, 0.74],
                tint_strength: 0.08,
                haze: 0.0,
                brightness: 1.0,
                particle_mood: None,
            },
            // Fresh clean green air — a bright, haze-free verdant wash.
            Solarpunk => Self {
                tint: [0.62, 0.82, 0.66],
                tint_strength: 0.10,
                haze: 0.0,
                brightness: 1.0,
                particle_mood: None,
            },
            // Thin rust-grey atmosphere — a pale dusty sky with regolith motes.
            SpaceOutpost => Self {
                tint: [0.64, 0.58, 0.54],
                tint_strength: 0.10,
                haze: 0.04,
                brightness: 1.0,
                particle_mood: Some(ParticleMood::DustMotes),
            },
            // Arcane air — a soft violet wash thick with drifting magic motes.
            Fantasy => Self {
                tint: [0.60, 0.48, 0.82],
                tint_strength: 0.14,
                haze: 0.06,
                brightness: 1.0,
                particle_mood: Some(ParticleMood::Fireflies),
            },
            // Sun-bleached frontier dust — a warm tan haze with drifting motes.
            WildWest => Self {
                tint: [0.80, 0.68, 0.46],
                tint_strength: 0.12,
                haze: 0.08,
                brightness: 1.0,
                particle_mood: Some(ParticleMood::DustMotes),
            },
            _ => Self::NEUTRAL,
        }
    }

    /// The full per-room accent: the theme accent ([`Self::for_theme`])
    /// with the socio-political axes layered on top. Escalation drifts the
    /// tint toward [`SMOKE_RED`] and adds haze (the smoke of conflict);
    /// prosperity sets [`Self::brightness`] (affluent brighter, destitute
    /// dimmer). Both are gated so a mid-prosperity, peaceful room collapses
    /// back to the plain theme accent.
    pub fn for_scene(scene: &SceneCharacter) -> Self {
        let mut a = Self::for_theme(scene.theme);

        // Escalation ramps in above ~0.45 so calm/tense rooms are untouched
        // and only genuine conflict smokes the air.
        let conflict = ((scene.escalation - 0.45) / 0.55).clamp(0.0, 1.0);
        if conflict > 0.0 {
            a.tint = [
                a.tint[0] * (1.0 - conflict) + SMOKE_RED[0] * conflict,
                a.tint[1] * (1.0 - conflict) + SMOKE_RED[1] * conflict,
                a.tint[2] * (1.0 - conflict) + SMOKE_RED[2] * conflict,
            ];
            a.tint_strength = (a.tint_strength + 0.20 * conflict).min(0.4);
            a.haze = (a.haze + 0.18 * conflict).min(0.45);
        }

        // Prosperity brightness: centred at 0.5 (no change), ±20% at the
        // extremes.
        let wealth = (scene.prosperity.clamp(0.0, 1.0) - 0.5) * 2.0;
        a.brightness = (1.0 + 0.2 * wealth).clamp(0.8, 1.2);

        a
    }

    /// `true` if this accent changes nothing — lets callers skip the
    /// blend entirely for the common neutral case.
    pub fn is_noop(&self) -> bool {
        self.tint_strength <= 0.0
            && self.haze <= 0.0
            && self.particle_mood.is_none()
            && (self.brightness - 1.0).abs() < 1e-6
    }

    /// Blend an sRGB colour toward [`Self::tint`] by [`Self::tint_strength`].
    pub fn tint_rgb(&self, c: [f32; 3]) -> [f32; 3] {
        let t = self.tint_strength.clamp(0.0, 1.0);
        [
            c[0] * (1.0 - t) + self.tint[0] * t,
            c[1] * (1.0 - t) + self.tint[1] * t,
            c[2] * (1.0 - t) + self.tint[2] * t,
        ]
    }

    /// [`Self::tint_rgb`] followed by the [`Self::brightness`] multiply,
    /// clamped to `[0, 1]` — the full colour adjustment applied to fog /
    /// sky / cloud.
    pub fn adjust_rgb(&self, c: [f32; 3]) -> [f32; 3] {
        let t = self.tint_rgb(c);
        let b = self.brightness;
        [
            (t[0] * b).clamp(0.0, 1.0),
            (t[1] * b).clamp(0.0, 1.0),
            (t[2] * b).clamp(0.0, 1.0),
        ]
    }
}

/// How much natural daylight a theme keeps, `0..=1`. `1.0` is the full
/// biome-derived day/dusk and is the default for every theme; a value
/// below `1.0` darkens the room toward night so a *self-lit* theme — neon
/// signage, biolume — becomes the dominant light source instead of
/// competing with a noon sun (whose floor sits at ~9 000 lux even at dusk).
///
/// Consumed by the wiring layer's nightfall pass (`apply_nightfall` in
/// [`crate::pds::room`]), which scales the sun + ambient down and darkens
/// the sky / fog / cloud colour together — dimming the sun alone would
/// leave a bright daytime sky cuboid over a dark ground.
///
/// Kept a standalone per-variant function rather than a [`ThemeAccent`]
/// field so the common `1.0` case carries no per-theme boilerplate and the
/// daylight themes stay byte-for-byte unchanged.
pub fn theme_luminosity(theme: ThemeArchetype) -> f32 {
    use ThemeArchetype::*;
    match theme {
        // Neon-noir: drop the sun to a dim moonlight key so the kit's
        // emissive trim carries the scene.
        Cyberpunk => 0.12,
        _ => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_is_noop_and_identity() {
        assert!(ThemeAccent::NEUTRAL.is_noop());
        let c = [0.3, 0.6, 0.9];
        assert_eq!(ThemeAccent::NEUTRAL.tint_rgb(c), c);
    }

    #[test]
    fn ancient_is_neutral_distinctive_themes_are_not() {
        assert!(ThemeAccent::for_theme(ThemeArchetype::AncientClassical).is_noop());
        assert!(!ThemeAccent::for_theme(ThemeArchetype::Cyberpunk).is_noop());
        assert!(!ThemeAccent::for_theme(ThemeArchetype::Medieval).is_noop());
    }

    #[test]
    fn nordic_accent_leans_cold_blue() {
        let a = ThemeAccent::for_theme(ThemeArchetype::Nordic);
        assert!(!a.is_noop());
        // Steel-blue: the blue channel dominates the red.
        assert!(a.tint[2] > a.tint[0], "nordic accent should lean blue");
        let out = a.tint_rgb([0.4, 0.4, 0.4]);
        assert!(out[2] > out[0]);
        for ch in out {
            assert!((0.0..=1.0).contains(&ch));
        }
    }

    #[test]
    fn coastal_resort_accent_leans_sky_blue() {
        let a = ThemeAccent::for_theme(ThemeArchetype::CoastalResort);
        assert!(!a.is_noop());
        // Clear sky: blue dominates, and the air stays haze-free.
        assert!(a.tint[2] > a.tint[0], "coastal accent should lean sky-blue");
        assert_eq!(a.haze, 0.0, "a clear-sky resort adds no haze");
        let out = a.tint_rgb([0.4, 0.4, 0.4]);
        assert!(out[2] > out[0]);
        for ch in out {
            assert!((0.0..=1.0).contains(&ch));
        }
    }

    #[test]
    fn roadside_accent_leans_dusty_amber() {
        let a = ThemeAccent::for_theme(ThemeArchetype::Roadside);
        assert!(!a.is_noop());
        // Warm sodium dust: red dominates blue, and a little haze hangs.
        assert!(a.tint[0] > a.tint[2], "roadside accent should lean amber");
        assert!(a.haze > 0.0, "the dusty strip adds haze");
        assert_eq!(a.particle_mood, Some(ParticleMood::DustMotes));
    }

    #[test]
    fn civic_campus_accent_leans_warm_sandstone() {
        let a = ThemeAccent::for_theme(ThemeArchetype::CivicCampus);
        assert!(!a.is_noop());
        // Warm stone: red dominates blue, no haze over the quad.
        assert!(a.tint[0] > a.tint[2], "civic accent should lean warm");
        assert_eq!(a.haze, 0.0, "the open quad adds no haze");
    }

    #[test]
    fn sports_rec_accent_is_a_bright_clear_field() {
        let a = ThemeAccent::for_theme(ThemeArchetype::SportsRec);
        assert!(!a.is_noop());
        // Clean field daylight: faintly green, no haze.
        assert!(a.tint[1] > a.tint[0] && a.tint[1] > a.tint[2]);
        assert_eq!(a.haze, 0.0, "the open field adds no haze");
    }

    #[test]
    fn steampunk_accent_is_amber_smog() {
        let a = ThemeAccent::for_theme(ThemeArchetype::Steampunk);
        assert!(!a.is_noop());
        // Amber smog: warm tint and a hanging haze.
        assert!(a.tint[0] > a.tint[2], "steampunk accent should lean amber");
        assert!(a.haze > 0.0, "the smoggy works adds haze");
    }

    #[test]
    fn solarpunk_accent_is_fresh_green() {
        let a = ThemeAccent::for_theme(ThemeArchetype::Solarpunk);
        assert!(!a.is_noop());
        // Verdant clean air: green dominates, no haze.
        assert!(a.tint[1] > a.tint[0] && a.tint[1] > a.tint[2]);
        assert_eq!(a.haze, 0.0, "clean solar air adds no haze");
    }

    #[test]
    fn space_outpost_accent_is_thin_dusty_air() {
        let a = ThemeAccent::for_theme(ThemeArchetype::SpaceOutpost);
        assert!(!a.is_noop());
        // Thin rust atmosphere with drifting regolith motes.
        assert!(a.tint[0] > a.tint[2], "space accent should lean rust");
        assert_eq!(a.particle_mood, Some(ParticleMood::DustMotes));
    }

    #[test]
    fn fantasy_accent_is_arcane_motes() {
        let a = ThemeAccent::for_theme(ThemeArchetype::Fantasy);
        assert!(!a.is_noop());
        // Violet arcane air carrying magic motes.
        assert!(a.tint[2] > a.tint[1], "fantasy accent should lean violet");
        assert_eq!(a.particle_mood, Some(ParticleMood::Fireflies));
    }

    #[test]
    fn gothic_horror_accent_is_gloom_and_fog() {
        let a = ThemeAccent::for_theme(ThemeArchetype::GothicHorror);
        assert!(!a.is_noop());
        // Dark desaturating gloom with a hanging fog.
        assert!(
            a.tint[0] < 0.3 && a.tint[1] < 0.3,
            "gothic accent should be dark"
        );
        assert!(a.haze > 0.1, "gothic fog hangs heavy");
    }

    #[test]
    fn post_apoc_accent_is_dust_haze() {
        let a = ThemeAccent::for_theme(ThemeArchetype::PostApoc);
        assert!(!a.is_noop());
        // Warm grit hangs in the air with drifting dust motes.
        assert!(a.haze > 0.1, "the wasteland air hangs with dust");
        assert_eq!(a.particle_mood, Some(ParticleMood::DustMotes));
    }

    #[test]
    fn wild_west_accent_is_sun_bleached_dust() {
        let a = ThemeAccent::for_theme(ThemeArchetype::WildWest);
        assert!(!a.is_noop());
        // Warm frontier dust hangs in the air with drifting motes.
        assert!(a.tint[0] > a.tint[2], "wild west accent should lean warm");
        assert_eq!(a.particle_mood, Some(ParticleMood::DustMotes));
    }

    #[test]
    fn alien_themes_override_particle_mood() {
        assert_eq!(
            ThemeAccent::for_theme(ThemeArchetype::AlienOrganic).particle_mood,
            Some(ParticleMood::Fireflies)
        );
        assert_eq!(
            ThemeAccent::for_theme(ThemeArchetype::AlienMonolithic).particle_mood,
            Some(ParticleMood::MistMotes)
        );
        assert_eq!(
            ThemeAccent::for_theme(ThemeArchetype::FeudalJapan).particle_mood,
            Some(ParticleMood::Petals)
        );
    }

    #[test]
    fn tint_blends_toward_target_and_stays_bounded() {
        let a = ThemeAccent::for_theme(ThemeArchetype::Cyberpunk);
        let out = a.tint_rgb([0.0, 0.0, 0.0]);
        // Pulled toward magenta tint by tint_strength, never past it.
        assert!(out[0] > 0.0 && out[2] > 0.0);
        for ch in out {
            assert!((0.0..=1.0).contains(&ch));
        }
    }

    #[test]
    fn calm_neutral_room_is_a_noop() {
        // A neutral theme at mid prosperity and peace must leave everything
        // untouched, so the common path still skips the blend.
        let mut scene = SceneCharacter::for_seed(4);
        scene.theme = ThemeArchetype::AncientClassical;
        scene.prosperity = 0.5;
        scene.escalation = 0.0;
        assert!(ThemeAccent::for_scene(&scene).is_noop());
    }

    #[test]
    fn conflict_adds_smoke_tint_and_haze() {
        let mut scene = SceneCharacter::for_seed(4);
        scene.theme = ThemeArchetype::AncientClassical;
        scene.prosperity = 0.5;
        let calm = {
            let mut s = scene;
            s.escalation = 0.0;
            ThemeAccent::for_scene(&s)
        };
        scene.escalation = 1.0;
        let war = ThemeAccent::for_scene(&scene);
        assert!(war.haze > calm.haze, "conflict should add haze");
        assert!(
            war.tint_strength > calm.tint_strength,
            "conflict tints the air"
        );
        // The tint leans red (smoke), and the result stays bounded.
        assert!(war.tint[0] > war.tint[2]);
        for ch in war.adjust_rgb([0.4, 0.4, 0.4]) {
            assert!((0.0..=1.0).contains(&ch));
        }
    }

    #[test]
    fn prosperity_sets_brightness_either_way() {
        let mut scene = SceneCharacter::for_seed(4);
        scene.theme = ThemeArchetype::AncientClassical;
        scene.escalation = 0.0;
        scene.prosperity = 0.95;
        assert!(
            ThemeAccent::for_scene(&scene).brightness > 1.0,
            "rich brighter"
        );
        scene.prosperity = 0.05;
        assert!(
            ThemeAccent::for_scene(&scene).brightness < 1.0,
            "poor dimmer"
        );
    }

    #[test]
    fn cyberpunk_is_nocturnal_daylight_themes_are_full() {
        // The neon theme keeps only a fraction of daylight; every other
        // theme is full day (identity for the nightfall pass).
        assert!(theme_luminosity(ThemeArchetype::Cyberpunk) < 1.0);
        assert_eq!(theme_luminosity(ThemeArchetype::AncientClassical), 1.0);
        assert_eq!(theme_luminosity(ThemeArchetype::Medieval), 1.0);
        assert_eq!(theme_luminosity(ThemeArchetype::CoastalResort), 1.0);
    }
}
