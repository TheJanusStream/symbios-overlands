//! Coordinated avatar palette — skin / hair / eye + primary &
//! secondary accent colours, all derived from the avatar owner's DID
//! through the shared [`AvatarCharacter`] anchor.
//!
//! Sampled in OkLCH so colours stay tonally related. The accent colours
//! sit near the anchor's `base_hue` and are biased by the avatar's
//! [`ThemeArchetype`] style (a cyberpunk avatar reads high-chroma and
//! near-neon, a medieval one muted and earthy), nudged warm/cool by the
//! anchor `temperature`, and dulled by the anchor `wear` (a battered
//! avatar's paint is greyer and darker). Skin / hair / eye stay curated
//! lookup-table picks — random skin/hair colours look wrong fast, and
//! `wear` is equipment grime, not biology, so it leaves them untouched.
//!
//! Independent of [`super::super::room::palette`]: the avatar carries its
//! own hue anchor so a user's avatar reads the same regardless of which
//! room they're standing in. Material *finish* (gloss, emissive, grime)
//! is the partner concern and lives in [`super::materials`].

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use super::character::AvatarCharacter;
use crate::seeded_defaults::oklch::{oklch_to_srgb, wrap_hue_deg};
use crate::seeded_defaults::scene::{ThemeArchetype, pick, range_f32};

const AVATAR_PALETTE_SALT: u64 = 0xA1A1_5EED_5EED_A1A1;

/// Curated skin-tone palette (sRGB). Spans cool-pale to deep-warm; one
/// is sampled per avatar so we never produce out-of-distribution
/// "purple skin" surprises. Authored as a tasteful set the user can
/// re-curate without touching the deriver code.
const SKIN_TONES: &[[f32; 3]] = &[
    [0.95, 0.84, 0.74], // very fair pink
    [0.92, 0.78, 0.66], // fair warm
    [0.84, 0.66, 0.51], // tan
    [0.66, 0.49, 0.36], // mid warm
    [0.50, 0.36, 0.26], // deep
    [0.34, 0.22, 0.15], // very deep
    [0.88, 0.72, 0.58], // neutral mid
    [0.75, 0.58, 0.42], // olive
];

/// Curated hair colours (sRGB). Includes naturals + a handful of bold
/// fantasy accents — picks are weighted naturally by the table itself
/// (more natural entries → more natural draws).
const HAIR_COLORS: &[[f32; 3]] = &[
    [0.10, 0.07, 0.05], // black
    [0.20, 0.12, 0.07], // dark brown
    [0.30, 0.18, 0.10], // medium brown
    [0.45, 0.30, 0.18], // chestnut
    [0.66, 0.46, 0.24], // warm brown
    [0.88, 0.74, 0.46], // blonde
    [0.95, 0.92, 0.85], // platinum
    [0.55, 0.55, 0.58], // grey
    [0.85, 0.32, 0.18], // ginger
    [0.62, 0.18, 0.28], // burgundy
    [0.20, 0.50, 0.80], // electric blue (fantasy)
    [0.45, 0.22, 0.62], // purple (fantasy)
];

/// Curated eye colours. Small set — natural eye colour space is
/// narrower than skin/hair.
const EYE_COLORS: &[[f32; 3]] = &[
    [0.10, 0.08, 0.06], // dark brown
    [0.36, 0.24, 0.12], // amber
    [0.20, 0.42, 0.55], // blue
    [0.30, 0.55, 0.40], // green
    [0.42, 0.34, 0.18], // hazel
    [0.18, 0.20, 0.25], // grey-blue
];

/// How a [`ThemeArchetype`] style colours an avatar's accents. The 23
/// themes group into five moods so the accent sampler stays compact and a
/// new theme falls into a sensible bucket by editing one match arm.
#[derive(Clone, Copy, Debug)]
struct StyleMood {
    /// Multiplier on the sampled OkLCH chroma — `>1` pushes toward neon,
    /// `<1` toward muted / greyed.
    chroma_mul: f32,
    /// Added to the sampled OkLCH lightness — `+` for bright/clean styles,
    /// `-` for dark/gloomy ones.
    light_bias: f32,
}

impl StyleMood {
    fn for_style(style: ThemeArchetype) -> Self {
        use ThemeArchetype::*;
        match style {
            // Neon / speculative: saturated and bright (their accents also
            // read emissive — see `materials::MaterialKit`).
            Cyberpunk | AlienMonolithic | Fantasy | Solarpunk | SpaceOutpost => Self {
                chroma_mul: 1.5,
                light_bias: 0.05,
            },
            // Living / organic: richly saturated, neutral lightness.
            AlienOrganic | FeudalJapan => Self {
                chroma_mul: 1.25,
                light_bias: 0.0,
            },
            // Bright / clean civic & leisure: gently desaturated but light.
            CoastalResort | CivicCampus | SportsRec | Suburban => Self {
                chroma_mul: 1.0,
                light_bias: 0.10,
            },
            // Cold / industrial: heavily greyed and a touch dark.
            GothicHorror | IndustrialPark | ModernCity => Self {
                chroma_mul: 0.5,
                light_bias: -0.05,
            },
            // Earthy / historical: muted, slightly dark — the default mood
            // for the historical and frontier themes.
            Medieval | AncientClassical | Nordic | Mesoamerican | Steampunk | RuralFarmland
            | Roadside | PostApoc | WildWest => Self {
                chroma_mul: 0.7,
                light_bias: -0.03,
            },
        }
    }
}

/// All seeded avatar colours.
///
/// `skin_tone`, `hair_color`, and `eye_color` are curated-table picks; the
/// three accents are OkLCH-coordinated around the anchor hue and biased by
/// style / temperature / wear. Builders read accents for clothing / hull /
/// trim and skin/hair/eye for the figure; material *finish* comes from
/// [`super::materials::MaterialKit`].
#[derive(Clone, Copy, Debug)]
pub struct AvatarPalette {
    /// Hue anchor (degrees) the OkLCH-derived accent colours sit near.
    /// Comes from the [`AvatarCharacter`] anchor, independent of the room
    /// palette's base_hue.
    pub base_hue_deg: f32,
    pub skin_tone: [f32; 3],
    pub hair_color: [f32; 3],
    pub eye_color: [f32; 3],
    /// Hull / clothing primary — the most visible "this is me" colour.
    pub primary_accent: [f32; 3],
    /// Trim / pontoons / belt — complementary to primary.
    pub secondary_accent: [f32; 3],
    /// A third small-area highlight (mast / cuff / hat band).
    pub tertiary_accent: [f32; 3],
}

impl AvatarPalette {
    pub fn for_did(did: &str) -> Self {
        Self::for_character(&AvatarCharacter::for_did(did))
    }

    /// Derive from a pre-computed seed — the manual re-roll path.
    /// `for_did(did)` is exactly `for_seed(fnv1a_64(did))`.
    pub fn for_seed(seed: u64) -> Self {
        Self::for_character(&AvatarCharacter::for_seed(seed))
    }

    /// Derive the palette from the shared avatar anchor. The accents read
    /// the anchor's `base_hue` / `temperature` / `style` / `wear`; skin /
    /// hair / eye are sampled from the curated tables (their own draws,
    /// independent of the anchor's continuous knobs).
    pub fn for_character(c: &AvatarCharacter) -> Self {
        // Own salted sub-stream off the anchor's seed — the table picks and
        // accent maths vary per DID while staying independent of the other
        // derivers' streams.
        let mut rng = ChaCha8Rng::seed_from_u64(c.seed ^ AVATAR_PALETTE_SALT);

        let skin_tone = pick(SKIN_TONES, &mut rng);
        let hair_color = pick(HAIR_COLORS, &mut rng);
        let eye_color = pick(EYE_COLORS, &mut rng);

        let mood = StyleMood::for_style(c.style);
        let base_hue_deg = c.base_hue_deg;

        // Accent builder: sample L/C around the per-accent band, apply the
        // style mood (chroma scale + lightness bias) and the wear dulling
        // (battered paint is greyer + darker), convert to sRGB, then mix a
        // small warm/cool temperature tint. Takes `rng` as a parameter
        // (rather than capturing it) so the per-call hue jitter can be drawn
        // from the same stream without a double-borrow.
        let accent =
            |rng: &mut ChaCha8Rng, l_lo: f32, l_hi: f32, c_lo: f32, c_hi: f32, hue: f32| {
                let l = (range_f32(rng, l_lo, l_hi) + mood.light_bias) * (1.0 - 0.25 * c.wear);
                let chroma = range_f32(rng, c_lo, c_hi) * mood.chroma_mul * (1.0 - 0.40 * c.wear);
                let srgb = oklch_to_srgb([l.clamp(0.0, 1.0), chroma.max(0.0), wrap_hue_deg(hue)]);
                temperature_tint(srgb, c.temperature)
            };

        // Primary: mid L, mid-high chroma, hue == base.
        let primary_accent = accent(&mut rng, 0.42, 0.58, 0.10, 0.18, base_hue_deg);
        // Secondary: complementary hue (≈180°), slightly lower chroma.
        let sec_jitter = range_f32(&mut rng, -30.0, 30.0);
        let secondary_accent = accent(
            &mut rng,
            0.35,
            0.50,
            0.08,
            0.14,
            base_hue_deg + 180.0 + sec_jitter,
        );
        // Tertiary: triadic (≈120°), low-mid chroma, lighter.
        let ter_jitter = range_f32(&mut rng, -30.0, 30.0);
        let tertiary_accent = accent(
            &mut rng,
            0.55,
            0.72,
            0.06,
            0.12,
            base_hue_deg + 120.0 + ter_jitter,
        );

        Self {
            base_hue_deg,
            skin_tone,
            hair_color,
            eye_color,
            primary_accent,
            secondary_accent,
            tertiary_accent,
        }
    }
}

/// Warm / cool target tints the temperature axis pulls an accent toward.
const WARM_TINT: [f32; 3] = [1.0, 0.55, 0.15];
const COOL_TINT: [f32; 3] = [0.25, 0.50, 1.0];

/// Mix `color` a little toward the warm (amber) or cool (blue) target by
/// the signed `temperature` (`+` warm, `-` cool). Strength is capped low so
/// the avatar's own hue still leads.
fn temperature_tint(color: [f32; 3], temperature: f32) -> [f32; 3] {
    let t = (temperature.abs() * 0.12).clamp(0.0, 0.3);
    let target = if temperature >= 0.0 {
        WARM_TINT
    } else {
        COOL_TINT
    };
    [
        (color[0] * (1.0 - t) + target[0] * t).clamp(0.0, 1.0),
        (color[1] * (1.0 - t) + target[1] * t).clamp(0.0, 1.0),
        (color[2] * (1.0 - t) + target[2] * t).clamp(0.0, 1.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeded_defaults::hash::fnv1a_64;
    use crate::seeded_defaults::scene::ThemeArchetype;

    #[test]
    fn deterministic() {
        let a = AvatarPalette::for_did("did:plc:abc");
        let b = AvatarPalette::for_did("did:plc:abc");
        assert_eq!(a.primary_accent, b.primary_accent);
        assert_eq!(a.skin_tone, b.skin_tone);
    }

    #[test]
    fn for_did_equals_for_seed_of_hashed_did() {
        let did = "did:plc:palette";
        let a = AvatarPalette::for_did(did);
        let b = AvatarPalette::for_seed(fnv1a_64(did));
        assert_eq!(a.primary_accent, b.primary_accent);
        assert_eq!(a.base_hue_deg, b.base_hue_deg);
    }

    #[test]
    fn distinct_dids_vary() {
        let a = AvatarPalette::for_did("did:plc:abc");
        let b = AvatarPalette::for_did("did:plc:def");
        // At least one colour differs.
        assert!(a.primary_accent != b.primary_accent || a.hair_color != b.hair_color);
    }

    #[test]
    fn skin_hair_eye_come_from_curated_tables() {
        // The picked tone must literally appear in the table — wear must
        // not have leaked into the biological colours.
        for s in 0u64..32 {
            let p = AvatarPalette::for_did(&format!("did:test:{s}"));
            assert!(SKIN_TONES.contains(&p.skin_tone));
            assert!(HAIR_COLORS.contains(&p.hair_color));
            assert!(EYE_COLORS.contains(&p.eye_color));
        }
    }

    #[test]
    fn accents_stay_in_gamut() {
        for s in 0u64..48 {
            let p = AvatarPalette::for_seed(s);
            for accent in [p.primary_accent, p.secondary_accent, p.tertiary_accent] {
                for ch in accent {
                    assert!(
                        (0.0..=1.0).contains(&ch),
                        "accent OOB at seed {s}: {accent:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn wear_dulls_the_accent() {
        // Hold every anchor field fixed but ramp wear: a battered avatar's
        // primary accent is both darker and less saturated than a pristine
        // one's.
        let luma = |c: [f32; 3]| 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
        let chroma = |c: [f32; 3]| {
            let max = c[0].max(c[1]).max(c[2]);
            let min = c[0].min(c[1]).min(c[2]);
            max - min
        };
        let mut pristine = AvatarCharacter::for_seed(3);
        pristine.style = ThemeArchetype::Medieval;
        pristine.temperature = 0.0;
        pristine.wear = 0.0;
        let mut battered = pristine;
        battered.wear = 1.0;
        let p = AvatarPalette::for_character(&pristine).primary_accent;
        let b = AvatarPalette::for_character(&battered).primary_accent;
        assert!(
            luma(b) < luma(p),
            "battered should be darker: {b:?} vs {p:?}"
        );
        assert!(
            chroma(b) < chroma(p),
            "battered should be less saturated: {b:?} vs {p:?}"
        );
    }

    #[test]
    fn neon_style_is_more_saturated_than_cold_style() {
        // Same anchor, swap only the style: a neon style yields a
        // higher-chroma primary accent than a cold/greyed one.
        let chroma = |c: [f32; 3]| c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2]);
        let mut neon = AvatarCharacter::for_seed(5);
        neon.temperature = 0.0;
        neon.wear = 0.0;
        neon.style = ThemeArchetype::Cyberpunk;
        let mut cold = neon;
        cold.style = ThemeArchetype::IndustrialPark;
        let n = AvatarPalette::for_character(&neon).primary_accent;
        let c = AvatarPalette::for_character(&cold).primary_accent;
        assert!(
            chroma(n) > chroma(c),
            "neon should out-saturate cold: {n:?} vs {c:?}"
        );
    }

    #[test]
    fn every_style_stays_in_gamut() {
        // The mood table must keep every theme's accents valid sRGB.
        for style in ThemeArchetype::ALL {
            let mut c = AvatarCharacter::for_seed(9);
            c.style = style;
            let p = AvatarPalette::for_character(&c);
            for accent in [p.primary_accent, p.secondary_accent, p.tertiary_accent] {
                for ch in accent {
                    assert!(
                        (0.0..=1.0).contains(&ch),
                        "{style:?} accent OOB: {accent:?}"
                    );
                }
            }
        }
    }
}
