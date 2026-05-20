//! Coordinated avatar palette — skin / hair / eye + primary &
//! secondary accent colours, all derived from the avatar owner's DID.
//!
//! Sampled in OkLCH so colours stay tonally related; the accent
//! colours sit near the avatar's `base_hue` while skin / hair / eye
//! are picked from small curated lookup tables rather than uniform
//! sRGB sampling (random skin/hair colours look wrong fast).
//!
//! Independent of [`super::super::room::palette`] — the avatar carries
//! its own hue anchor so a user's avatar reads the same regardless of
//! which room they're standing in.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::seeded_defaults::hash::fnv1a_64;
use crate::seeded_defaults::oklch::{oklch_to_srgb, wrap_hue_deg};
use crate::seeded_defaults::scene::{pick, range_f32, unit_f32};

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

/// All seeded avatar colours.
///
/// `skin_tone`, `hair_color`, and `eye_color` are not currently
/// consumed by the hover-boat default visuals (there's no skin/hair/
/// eye on a hover-boat) — they're computed because the proposal's
/// avatar surface includes them, and a future humanoid spawn path will
/// pick them up without needing to extend the deriver.
#[derive(Clone, Copy, Debug)]
pub struct AvatarPalette {
    /// Hue anchor (degrees) for the OkLCH-derived accent colours.
    /// Independent of the room palette's base_hue.
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
        let mut rng = ChaCha8Rng::seed_from_u64(fnv1a_64(did) ^ AVATAR_PALETTE_SALT);

        let base_hue_deg = unit_f32(&mut rng) * 360.0;

        let skin_tone = pick(SKIN_TONES, &mut rng);
        let hair_color = pick(HAIR_COLORS, &mut rng);
        let eye_color = pick(EYE_COLORS, &mut rng);

        // Primary accent: medium L, mid-high chroma, hue == base.
        let primary_accent = oklch_to_srgb([
            range_f32(&mut rng, 0.42, 0.58),
            range_f32(&mut rng, 0.10, 0.18),
            wrap_hue_deg(base_hue_deg),
        ]);
        // Secondary accent: complementary hue (180° offset), slightly
        // lower chroma so the primary still leads.
        let secondary_accent = oklch_to_srgb([
            range_f32(&mut rng, 0.35, 0.50),
            range_f32(&mut rng, 0.08, 0.14),
            wrap_hue_deg(base_hue_deg + 180.0 + range_f32(&mut rng, -30.0, 30.0)),
        ]);
        // Tertiary accent: triadic (120° offset), low-mid chroma.
        let tertiary_accent = oklch_to_srgb([
            range_f32(&mut rng, 0.55, 0.72),
            range_f32(&mut rng, 0.06, 0.12),
            wrap_hue_deg(base_hue_deg + 120.0 + range_f32(&mut rng, -30.0, 30.0)),
        ]);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = AvatarPalette::for_did("did:plc:abc");
        let b = AvatarPalette::for_did("did:plc:abc");
        assert_eq!(a.primary_accent, b.primary_accent);
        assert_eq!(a.skin_tone, b.skin_tone);
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
        // The picked tone must literally appear in the table — if it
        // doesn't, the sampler skipped the curation step.
        for s in 0u64..16 {
            let p = AvatarPalette::for_did(&format!("did:test:{s}"));
            assert!(SKIN_TONES.iter().any(|c| *c == p.skin_tone));
            assert!(HAIR_COLORS.iter().any(|c| *c == p.hair_color));
            assert!(EYE_COLORS.iter().any(|c| *c == p.eye_color));
        }
    }
}
