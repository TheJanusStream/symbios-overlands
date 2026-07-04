//! Seeded face identity — head shape, feature layout, resting expression,
//! facial hair, and hair style for the humanoid chassis.
//!
//! Follows the Mii/Animal-Crossing population recipe: identity lives in
//! *type* choices (face shape, hair style, nose kind, facial hair) plus a
//! small placement/rotation parameter set, and the resting expression is
//! sampled from a handful of **disposition presets** then jittered — never
//! from independent per-parameter rolls, which is what produces enraged or
//! dead-eyed resting faces (the "hypercube corner" failure).
//!
//! Every landmark fraction is banded by the avatar's [`StylizationTier`]:
//! the eye line is the highest-stakes number in the whole face (low = cute
//! infant schema, midline = adult), so it is clamped per tier and never
//! blended across tiers. Sizes here are *fractions* (of head height from
//! the top, of head width, of eye size); the head part builder multiplies
//! them out against its skull radius.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use super::body::StylizationTier;
use crate::seeded_defaults::scene::range_f32;

const FACE_SALT: u64 = 0xFACE_FACE_FACE_FACE;

/// Front-view silhouette archetype — a jaw-construction recipe on the same
/// cranium (see the phase-C design reference).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceShape {
    /// Jaw absorbed into the ball, full cheeks — young, jolly.
    Round,
    /// Smooth 0.9→0.6 jaw taper — the trustworthy default.
    Oval,
    /// Near-straight jaw + full-width chin — tough, dependable.
    Square,
    /// Oval stretched vertically — serious, aristocratic.
    Oblong,
    /// Wide temples tapering to a small chin — elfin, mischievous.
    Heart,
    /// Narrow temples and chin, widest at the cheekbones — sharp, elegant.
    Diamond,
}

/// Resting-expression preset. The population reads mostly warm: the
/// weighted table below has no outright angry face, and frowny presets
/// keep less curvature than the smiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    Cheerful,
    Calm,
    Shy,
    Dreamy,
    Determined,
    Deadpan,
    Sly,
    Gruff,
}

/// Nose minimalism ladder — tiers pick from different rungs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoseKind {
    NoNose,
    Dot,
    Nub,
    /// Comedy ball nose — stylized tier only, rare.
    Ball,
    Wedge,
    StrongWedge,
}

/// Seeded facial-hair mass (a minority roll; colour follows hair).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FacialHair {
    NoFacialHair,
    Moustache,
    Goatee,
    MoustacheGoatee,
    FullBeard,
    MuttonChops,
}

/// Hair archetype — each decomposes into 1–6 primitive clump masses in the
/// head builder (hairline-first: the front rim of the dome / fringe edge IS
/// the drawn hairline).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HairStyle {
    Crop,
    Buzz,
    Bob,
    SidePart,
    SlickBack,
    Ponytail,
    Bun,
    Pigtails,
    Spikes,
    Afro,
    Curls,
    Long,
    Horseshoe,
    Bald,
}

impl HairStyle {
    /// Style actually built when a hat occupies the crown: tall crown
    /// masses degrade to a low style instead of clipping the hat.
    pub fn under_hat(self) -> Self {
        match self {
            Self::Spikes | Self::Bun | Self::Afro => Self::Crop,
            Self::Curls => Self::Bob,
            other => other,
        }
    }
}

/// The full seeded face. All `*_line` fields are fractions of head height
/// measured from the crown (hair top); widths are fractions of head width;
/// eye-relative values are in units of the eye size itself.
#[derive(Clone, Copy, Debug)]
pub struct FaceParams {
    pub shape: FaceShape,
    pub disposition: Disposition,

    // Expression (disposition preset + jitter).
    /// Brow rotation, radians; positive = inner ends up (gentle/worried),
    /// negative = inner ends down (determined). Resting range ±~0.35.
    pub brow_angle: f32,
    /// Brow lift above the eye, in eye-heights (0.15–0.8).
    pub brow_height: f32,
    /// Extra rotation applied to one brow only (the sly/skeptical bit).
    pub brow_asym: f32,
    /// Mouth curvature −0.5…1.0 (negative = slight frown; frowns are kept
    /// weaker than smiles by construction).
    pub mouth_curve: f32,
    /// Mouth width as a fraction of head width.
    pub mouth_width: f32,
    /// Lateral mouth offset (smirk), fraction of head width.
    pub mouth_off: f32,
    /// Eye openness 0.4–1.1 (vertical scale on the eye masses; low = the
    /// half-lidded cool/dreamy read).
    pub eye_open: f32,

    // Feature layout (tier-banded).
    /// Eye line, fraction of head height from the crown. The single most
    /// identity-critical number: ~0.5 adult, 0.6–0.7 toy.
    pub eye_line: f32,
    pub nose_line: f32,
    pub mouth_line: f32,
    /// Forehead hairline, fraction from the crown.
    pub hairline: f32,
    /// Eye width, fraction of head width (per eye).
    pub eye_size: f32,
    /// Gap between the two eyes' inner edges, in eye-widths.
    pub eye_gap: f32,
    /// Iris diameter as a fraction of the eye. ≥0.9 renders as a solid
    /// dark bead ("dot eye") instead of sclera+iris.
    pub iris_frac: f32,
    /// Whether the upper lid line cuts the eye (the calm/adult read).
    pub lidded: bool,

    pub nose: NoseKind,
    pub blush: bool,
    pub freckles: bool,
    pub facial_hair: FacialHair,
    pub hair: HairStyle,
}

/// Cumulative-weight pick: `roll` in 0..1 against `(weight, value)` rows.
fn weighted<T: Copy>(roll: f32, rows: &[(f32, T)]) -> T {
    let total: f32 = rows.iter().map(|(w, _)| w).sum();
    let mut acc = 0.0;
    for (w, v) in rows {
        acc += w / total;
        if roll < acc {
            return *v;
        }
    }
    rows[rows.len() - 1].1
}

impl FaceParams {
    /// Derive the face for an avatar seed. `tier` comes from the same
    /// seed's [`AvatarBody`](super::body::AvatarBody) so face stylization
    /// always matches body stylization.
    pub fn for_seed(seed: u64, tier: StylizationTier) -> Self {
        use StylizationTier::*;
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ FACE_SALT);
        let mut roll = |lo: f32, hi: f32| range_f32(&mut rng, lo, hi);

        // ---- Discrete identity picks -----------------------------------
        let shape = {
            let r = roll(0.0, 1.0);
            match tier {
                Toy => weighted(r, &[(55.0, FaceShape::Round), (45.0, FaceShape::Oval)]),
                Stylized => weighted(
                    r,
                    &[
                        (30.0, FaceShape::Oval),
                        (25.0, FaceShape::Round),
                        (18.0, FaceShape::Heart),
                        (12.0, FaceShape::Square),
                        (8.0, FaceShape::Oblong),
                        (7.0, FaceShape::Diamond),
                    ],
                ),
                Realistic => weighted(
                    r,
                    &[
                        (35.0, FaceShape::Oval),
                        (15.0, FaceShape::Round),
                        (18.0, FaceShape::Square),
                        (15.0, FaceShape::Oblong),
                        (10.0, FaceShape::Heart),
                        (7.0, FaceShape::Diamond),
                    ],
                ),
                Heroic => weighted(
                    r,
                    &[
                        (30.0, FaceShape::Square),
                        (20.0, FaceShape::Oval),
                        (20.0, FaceShape::Oblong),
                        (12.0, FaceShape::Diamond),
                        (10.0, FaceShape::Heart),
                        (8.0, FaceShape::Round),
                    ],
                ),
            }
        };

        // Warm-weighted disposition table (user call: mostly warm, some
        // neutral/determined, no resting anger).
        let disposition = weighted(
            roll(0.0, 1.0),
            &[
                (22.0, Disposition::Cheerful),
                (20.0, Disposition::Calm),
                (14.0, Disposition::Shy),
                (12.0, Disposition::Dreamy),
                (12.0, Disposition::Determined),
                (8.0, Disposition::Deadpan),
                (7.0, Disposition::Sly),
                (5.0, Disposition::Gruff),
            ],
        );

        // Preset 5-param combos: (brow angle rad, brow height, mouth curve,
        // mouth width, eye openness). Kept inside the appeal envelope —
        // frown curvature ≤ half of smile curvature, resting brow ≤ ±0.30.
        let (mut ba, mut bh, mut mc, mut mw, mut eo) = match disposition {
            Disposition::Cheerful => (0.07, 0.55, 0.75, 0.30, 1.0),
            Disposition::Calm => (0.0, 0.35, 0.30, 0.22, 0.9),
            Disposition::Shy => (0.20, 0.45, 0.35, 0.16, 0.95),
            Disposition::Dreamy => (0.10, 0.55, 0.30, 0.20, 0.55),
            Disposition::Determined => (-0.27, 0.22, 0.15, 0.26, 1.0),
            Disposition::Deadpan => (0.0, 0.20, 0.06, 0.24, 0.75),
            Disposition::Sly => (-0.13, 0.30, 0.45, 0.24, 0.8),
            Disposition::Gruff => (-0.20, 0.20, -0.12, 0.28, 0.85),
        };
        // Jitter ±12 % multiplicative (additive for the near-zero angles)
        // so two calm faces still differ, without leaving the preset's
        // character.
        ba += roll(-0.05, 0.05);
        bh *= roll(0.88, 1.12);
        mc *= roll(0.88, 1.12);
        mw *= roll(0.88, 1.12);
        eo = (eo * roll(0.92, 1.08)).clamp(0.4, 1.1);
        // The asymmetry bit: one brow cocked. Sly always; others ~20 %.
        let asym_on = disposition == Disposition::Sly || roll(0.0, 1.0) < 0.20;
        let brow_asym = if asym_on { roll(0.08, 0.16) } else { 0.0 };
        let mouth_off = if disposition == Disposition::Sly {
            0.06
        } else {
            0.0
        };

        // ---- Tier-banded layout ----------------------------------------
        let (eye_line, nose_line, mouth_line, hairline) = match tier {
            Toy => (
                roll(0.60, 0.68),
                roll(0.76, 0.80),
                roll(0.83, 0.87),
                roll(0.30, 0.36),
            ),
            Stylized => (
                roll(0.55, 0.60),
                roll(0.72, 0.76),
                roll(0.80, 0.84),
                roll(0.26, 0.31),
            ),
            Realistic => (
                roll(0.49, 0.52),
                roll(0.66, 0.69),
                roll(0.77, 0.80),
                roll(0.24, 0.27),
            ),
            Heroic => (
                roll(0.48, 0.52),
                roll(0.67, 0.70),
                roll(0.78, 0.82),
                roll(0.22, 0.26),
            ),
        };
        // Eye sizes stay a touch above the sanitiser's 1 cm floor on the
        // small realistic/heroic heads.
        let (eye_size, eye_gap, iris_frac) = match tier {
            Toy => (roll(0.15, 0.21), roll(1.2, 1.5), roll(0.68, 0.78)),
            Stylized => (roll(0.11, 0.15), roll(1.0, 1.2), roll(0.62, 0.72)),
            Realistic => (roll(0.095, 0.11), roll(0.95, 1.05), roll(0.52, 0.60)),
            Heroic => (roll(0.09, 0.105), roll(1.0, 1.1), roll(0.50, 0.58)),
        };
        // Toy dot-eyes: ~45 % of toys use the solid dark bead instead of
        // sclera+iris (the Crossy Road / LEGO read).
        let iris_frac = if tier == Toy && roll(0.0, 1.0) < 0.45 {
            0.95
        } else {
            iris_frac
        };
        // The lid line is the adult/cool signal: always on for the two
        // realistic registers, on for half-lidded stylized faces, never on
        // a toy face.
        let lidded = match tier {
            Realistic | Heroic => true,
            Stylized => eo < 0.75,
            Toy => false,
        };

        let nose = {
            let r = roll(0.0, 1.0);
            match tier {
                Toy => weighted(r, &[(60.0, NoseKind::NoNose), (40.0, NoseKind::Dot)]),
                Stylized => weighted(
                    r,
                    &[
                        (62.0, NoseKind::Nub),
                        (22.0, NoseKind::Dot),
                        (16.0, NoseKind::Ball),
                    ],
                ),
                Realistic => NoseKind::Wedge,
                Heroic => NoseKind::StrongWedge,
            }
        };

        let blush_p = match tier {
            Toy => 0.65,
            Stylized => 0.22,
            _ => 0.0,
        };
        let blush = roll(0.0, 1.0) < blush_p;
        let freckles = matches!(tier, Toy | Stylized) && roll(0.0, 1.0) < 0.20;

        let facial_hair = {
            let r = roll(0.0, 1.0);
            if tier == Toy {
                // Toys stay mostly clean-faced; the rare toy beard is a
                // deliberate dwarf-figurine read.
                weighted(
                    r,
                    &[
                        (88.0, FacialHair::NoFacialHair),
                        (4.0, FacialHair::Moustache),
                        (8.0, FacialHair::FullBeard),
                    ],
                )
            } else {
                weighted(
                    r,
                    &[
                        (70.0, FacialHair::NoFacialHair),
                        (8.0, FacialHair::Moustache),
                        (6.0, FacialHair::Goatee),
                        (5.0, FacialHair::MoustacheGoatee),
                        (7.0, FacialHair::FullBeard),
                        (4.0, FacialHair::MuttonChops),
                    ],
                )
            }
        };

        let hair = weighted(
            roll(0.0, 1.0),
            &[
                (11.0, HairStyle::Crop),
                (7.0, HairStyle::Buzz),
                (12.0, HairStyle::Bob),
                (12.0, HairStyle::SidePart),
                (6.0, HairStyle::SlickBack),
                (10.0, HairStyle::Ponytail),
                (7.0, HairStyle::Bun),
                (5.0, HairStyle::Pigtails),
                (5.0, HairStyle::Spikes),
                (6.0, HairStyle::Afro),
                (7.0, HairStyle::Curls),
                (7.0, HairStyle::Long),
                (3.0, HairStyle::Horseshoe),
                (2.0, HairStyle::Bald),
            ],
        );

        Self {
            shape,
            disposition,
            brow_angle: ba.clamp(-0.32, 0.30),
            brow_height: bh.clamp(0.15, 0.8),
            brow_asym,
            mouth_curve: mc.clamp(-0.25, 1.0),
            mouth_width: mw.clamp(0.15, 0.40),
            mouth_off,
            eye_open: eo,
            eye_line,
            nose_line,
            mouth_line,
            hairline,
            eye_size,
            eye_gap,
            iris_frac,
            lidded,
            nose,
            blush,
            freckles,
            facial_hair,
            hair,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = FaceParams::for_seed(7, StylizationTier::Stylized);
        let b = FaceParams::for_seed(7, StylizationTier::Stylized);
        assert_eq!(a.shape, b.shape);
        assert_eq!(a.hair, b.hair);
        assert_eq!(a.brow_angle, b.brow_angle);
    }

    #[test]
    fn stays_inside_the_appeal_envelope() {
        for s in 0u64..256 {
            for tier in [
                StylizationTier::Toy,
                StylizationTier::Stylized,
                StylizationTier::Realistic,
                StylizationTier::Heroic,
            ] {
                let f = FaceParams::for_seed(s, tier);
                assert!((-0.32..=0.30).contains(&f.brow_angle));
                assert!((0.15..=0.8).contains(&f.brow_height));
                assert!((-0.25..=1.0).contains(&f.mouth_curve));
                assert!((0.15..=0.40).contains(&f.mouth_width));
                assert!((0.4..=1.1).contains(&f.eye_open));
                // No menace combo: a real frown never pairs with fierce brows.
                assert!(
                    !(f.mouth_curve < -0.1 && f.brow_angle < -0.25),
                    "seed {s}: hostile resting face"
                );
                // Never a dead face: at least one feature carries signal.
                assert!(
                    f.mouth_curve.abs() > 0.02
                        || f.brow_angle.abs() > 0.02
                        || f.brow_asym > 0.0
                        || f.blush,
                    "seed {s}: mannequin face"
                );
            }
        }
    }

    #[test]
    fn eye_line_is_tier_locked() {
        // The highest-stakes number: toy eyes sit low (cute), adult eyes
        // at the midline — the bands must never overlap.
        for s in 0u64..128 {
            let toy = FaceParams::for_seed(s, StylizationTier::Toy);
            let real = FaceParams::for_seed(s, StylizationTier::Realistic);
            assert!(toy.eye_line >= 0.60);
            assert!(real.eye_line <= 0.52);
            assert!(real.iris_frac <= 0.9, "dot eyes are toy-only");
            assert!(real.lidded && !toy.lidded);
        }
    }

    #[test]
    fn population_is_mostly_warm() {
        let mut warm = 0;
        const N: u64 = 800;
        for s in 0..N {
            let f = FaceParams::for_seed(s, StylizationTier::Stylized);
            if f.mouth_curve > 0.1 {
                warm += 1;
            }
        }
        // Cheerful+calm+shy+dreamy+sly+determined presets all carry a
        // positive curve; only deadpan/gruff sit at or below neutral.
        assert!(warm as f32 / N as f32 > 0.75, "only {warm}/{N} warm faces");
    }
}
