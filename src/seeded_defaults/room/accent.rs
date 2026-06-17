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
    fn alien_themes_override_particle_mood() {
        assert_eq!(
            ThemeAccent::for_theme(ThemeArchetype::AlienOrganic).particle_mood,
            Some(ParticleMood::Fireflies)
        );
        assert_eq!(
            ThemeAccent::for_theme(ThemeArchetype::AlienMonolithic).particle_mood,
            Some(ParticleMood::MistMotes)
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
}
