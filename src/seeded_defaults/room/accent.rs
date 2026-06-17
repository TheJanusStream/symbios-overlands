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
use crate::seeded_defaults::scene::ThemeArchetype;

/// A bounded, additive nudge applied to the room's fog / sky / cloud and
/// (optionally) ambient particles after the biome derivers have run.
#[derive(Clone, Copy, Debug)]
pub struct ThemeAccent {
    /// sRGB colour the fog + sky are blended toward.
    pub tint: [f32; 3],
    /// Blend weight (`0..~0.3`) for [`Self::tint`] into fog/sky — kept
    /// small so the biome palette still dominates.
    pub tint_strength: f32,
    /// Additive cloud cover (`0..~0.3`) for smoggy / overcast themes.
    pub haze: f32,
    /// Ambient particle mood override (`None` keeps the biome's mood).
    pub particle_mood: Option<ParticleMood>,
}

impl ThemeAccent {
    /// The do-nothing accent: leaves every natural deriver untouched.
    pub const NEUTRAL: Self = Self {
        tint: [0.0, 0.0, 0.0],
        tint_strength: 0.0,
        haze: 0.0,
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
                particle_mood: None,
            },
            Cyberpunk => Self {
                tint: [0.85, 0.10, 0.70],
                tint_strength: 0.22,
                haze: 0.10,
                particle_mood: None,
            },
            AlienOrganic => Self {
                tint: [0.25, 0.85, 0.40],
                tint_strength: 0.20,
                haze: 0.08,
                particle_mood: Some(ParticleMood::Fireflies),
            },
            AlienMonolithic => Self {
                tint: [0.40, 0.55, 0.95],
                tint_strength: 0.20,
                haze: 0.06,
                particle_mood: Some(ParticleMood::MistMotes),
            },
            GothicHorror => Self {
                tint: [0.18, 0.18, 0.24],
                tint_strength: 0.24,
                haze: 0.18,
                particle_mood: None,
            },
            IndustrialPark => Self {
                tint: [0.50, 0.50, 0.52],
                tint_strength: 0.18,
                haze: 0.20,
                particle_mood: None,
            },
            Steampunk => Self {
                tint: [0.72, 0.52, 0.26],
                tint_strength: 0.18,
                haze: 0.16,
                particle_mood: None,
            },
            RuralFarmland => Self {
                tint: [0.95, 0.80, 0.45],
                tint_strength: 0.12,
                haze: 0.0,
                particle_mood: None,
            },
            PostApoc => Self {
                tint: [0.62, 0.55, 0.45],
                tint_strength: 0.18,
                haze: 0.16,
                particle_mood: Some(ParticleMood::DustMotes),
            },
            _ => Self::NEUTRAL,
        }
    }

    /// `true` if this accent changes nothing — lets callers skip the
    /// blend entirely for the common neutral case.
    pub fn is_noop(&self) -> bool {
        self.tint_strength <= 0.0 && self.haze <= 0.0 && self.particle_mood.is_none()
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
}
