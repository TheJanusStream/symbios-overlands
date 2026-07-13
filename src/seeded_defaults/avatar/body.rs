//! Avatar body proportions — overall scale, shoulder width, head
//! size, limb thickness, body archetype.
//!
//! These seeded knobs are the shared anchor every chassis family's
//! blueprint reads: a humanoid derives its figure via
//! [`HumanoidBlueprint`](crate::seeded_defaults::HumanoidBlueprint), and
//! each vehicle its proportions via
//! [`VehicleBlueprint`](crate::seeded_defaults::VehicleBlueprint) (a boat's
//! hull length / beam / freeboard, an airship's envelope length / girth +
//! gondola, a skiff's body + wheels). `height_scale` sets overall size,
//! `shoulder_width_scale` the lateral spread (a figure's shoulders, a hull's
//! beam), `head_scale` a crowning mass (a head, an airship gondola),
//! `limb_thickness_scale` slender-vs-stout members (limbs, pontoons, wheels).
//! `torso_leg_ratio` is humanoid-only — the vehicle blueprints ignore it.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::seeded_defaults::hash::fnv1a_64;
use crate::seeded_defaults::scene::{pick, range_f32};

const AVATAR_BODY_SALT: u64 = 0xB0DD_B0DD_B0DD_B0DD;

/// Stylization register the whole figure is drawn in — the "heads-tall"
/// dial of classical figure canons. Sampled *first* (weighted, not
/// uniform: most of the population sits in the friendly mid-band) and
/// every other proportion knob then samples inside this tier's band, so
/// the parameters covary — a big Toy head always arrives with short
/// chubby limbs, never on realistic-length legs (the "bobblehead on
/// stilts" mixed-stylization failure).
///
/// Tiers change *physical* world height too (the locomotion capsule and
/// walk gait derive from the blueprint): a Toy avatar is genuinely small
/// in-world, a Heroic one genuinely tall.
///
/// Note "Realistic" is deliberately the ~6.5–7-head *everyman* canon,
/// not the full 7.5–8-head academic figure: on primitive-built bodies a
/// canon-realistic head reads as a pinhead at game camera distance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StylizationTier {
    /// ~6.5–7 heads, 1.70–1.80 m — everyman proportions.
    Realistic,
    /// ~7.0–7.5 heads, 1.80–1.95 m — long-legged, broad, imposing.
    Heroic,
    /// ~5–6 heads, 1.50–1.70 m — friendly cartoon adult (the bulk of
    /// the population).
    Stylized,
    /// ~3–4 heads, 1.00–1.25 m — toy / chibi register.
    Toy,
}

impl StylizationTier {
    /// Weighted seeded pick: Stylized 45 % / Heroic 25 % / Toy 15 % /
    /// Realistic 15 %.
    fn sample(rng: &mut impl rand_chacha::rand_core::RngCore) -> Self {
        let roll = range_f32(rng, 0.0, 1.0);
        if roll < 0.45 {
            Self::Stylized
        } else if roll < 0.70 {
            Self::Heroic
        } else if roll < 0.85 {
            Self::Toy
        } else {
            Self::Realistic
        }
    }
}

/// Discrete body family. Sampled first, then continuous knobs are
/// biased per archetype so a "stocky" body doesn't roll thin shoulders
/// and a "slim" body doesn't roll thick limbs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyArchetype {
    Slim,
    Average,
    Stocky,
}

impl BodyArchetype {
    pub const ALL: [Self; 3] = [Self::Slim, Self::Average, Self::Stocky];
}

/// All seeded avatar body proportions. Every scale is a multiplier
/// against the body's nominal dimensions (i.e. `1.0` is "default
/// size"); the hover-boat default impl reads them as multiplicative
/// scalars on the existing cuboid / capsule sizes.
#[derive(Clone, Copy, Debug)]
pub struct AvatarBody {
    pub archetype: BodyArchetype,
    /// Stylization register — the master proportion dial every canon
    /// field below is banded by. Humanoid-only expression; the vehicle
    /// chassis keep reading the legacy multiplier knobs.
    pub tier: StylizationTier,
    /// Overall body height multiplier. Affects every dimension
    /// uniformly (it's the "I'm tall / short" knob).
    pub height_scale: f32,
    /// Torso : leg length ratio (`0.5` is balanced). Unused by the
    /// vehicle chassis; the humanoid blueprint nudges its crotch line
    /// by it, so a high ratio reads as a long-torsoed figure.
    pub torso_leg_ratio: f32,
    /// Head / finial scale.
    pub head_scale: f32,
    /// Lateral (X-axis) scale. Hull width / shoulder width.
    pub shoulder_width_scale: f32,
    /// Limb thickness — pontoon radius, mast radius, arm/leg girth.
    pub limb_thickness_scale: f32,

    // --- Canon proportion knobs (humanoid blueprint inputs), all
    // sampled inside the tier's band so they covary. Fractions are of
    // the head-unit H (total height / heads_tall) unless noted. ---
    /// Physical height in metres — drives the locomotion capsule too.
    pub total_height_m: f32,
    /// Figure height in head-units (the classical canon dial).
    pub heads_tall: f32,
    /// Ground→crotch line as a fraction of total height (~0.5 = legs
    /// are half the figure; lower = stubbier, comedic).
    pub crotch_frac: f32,
    /// Shoulder span in head-*heights* (Loomis: 2⅓ for the ideal male).
    pub shoulder_span_h: f32,
    /// Waist radius as a fraction of chest radius (the V-taper; 1.0 is
    /// a straight tube, lower is more athletic).
    pub waist_taper: f32,
    /// Distal/proximal limb radius ratio (wrist/elbow, ankle/knee).
    /// 1.0 = untapered tube; the canon appeal band is 0.6–0.8.
    pub limb_taper: f32,
    /// Hand length / H (canon: hand ≈ face ≈ 0.75 H; toy canons
    /// oversize it).
    pub hand_frac: f32,
    /// Foot length / H (canon ≈ 1.0 H, stylized shorter).
    pub foot_frac: f32,
    /// Visible neck height / H (0 = head sits on the shoulders).
    pub neck_frac: f32,
    /// Torso depth (Z) as a fraction of its width (X) — bodies are
    /// wider than deep; a full-depth trunk reads as a barrel.
    pub depth_flatten: f32,
}

impl AvatarBody {
    pub fn for_did(did: &str) -> Self {
        Self::for_seed(fnv1a_64(did))
    }

    /// Derive from a pre-computed seed — the manual re-roll path.
    /// `for_did(did)` is exactly `for_seed(fnv1a_64(did))`.
    pub fn for_seed(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ AVATAR_BODY_SALT);
        let tier = StylizationTier::sample(&mut rng);
        let archetype = pick(&BodyArchetype::ALL, &mut rng);

        // Archetype biases the continuous sampling ranges. Slim gets a
        // narrower shoulder + thinner limbs; stocky goes the other way;
        // average sits in the middle of every band.
        let (height_lo, height_hi, shoulder_lo, shoulder_hi, limb_lo, limb_hi) = match archetype {
            BodyArchetype::Slim => (1.00, 1.15, 0.82, 0.95, 0.80, 0.95),
            BodyArchetype::Average => (0.92, 1.08, 0.92, 1.08, 0.92, 1.08),
            BodyArchetype::Stocky => (0.85, 1.00, 1.05, 1.18, 1.05, 1.20),
        };

        // Canon bands per tier: (height m, heads tall, crotch frac,
        // shoulder span H, waist taper, limb taper, hand frac, foot
        // frac, neck frac, depth flatten). Each row is one coherent
        // stylization register — see [`StylizationTier`].
        #[rustfmt::skip]
        let (h, heads, crotch, span, waist, ltaper, hand, foot, neck, depth) = match tier {
            StylizationTier::Realistic =>
                ((1.70, 1.80), (6.5, 7.0), (0.49, 0.51), (2.00, 2.20), (0.62, 0.72),
                 (0.68, 0.76), (0.72, 0.80), (0.85, 0.95), (0.28, 0.33), 0.78),
            StylizationTier::Heroic =>
                ((1.80, 1.95), (7.0, 7.5), (0.51, 0.53), (2.25, 2.50), (0.55, 0.65),
                 (0.62, 0.70), (0.78, 0.85), (0.95, 1.05), (0.28, 0.34), 0.76),
            StylizationTier::Stylized =>
                ((1.50, 1.70), (5.0, 6.0), (0.46, 0.50), (1.85, 2.20), (0.60, 0.75),
                 (0.70, 0.80), (0.82, 0.95), (0.75, 0.95), (0.20, 0.28), 0.80),
            StylizationTier::Toy =>
                ((1.00, 1.25), (3.2, 4.0), (0.42, 0.47), (1.30, 1.60), (0.78, 0.95),
                 (0.88, 1.00), (0.62, 0.78), (0.55, 0.75), (0.06, 0.10), 0.92),
        };
        // Stocky bodies keep more waist (boxier trunk), slim ones a
        // touch less — applied as a shift so it stays inside sane range.
        let waist_shift = match archetype {
            BodyArchetype::Slim => -0.04,
            BodyArchetype::Average => 0.0,
            BodyArchetype::Stocky => 0.10,
        };

        Self {
            archetype,
            tier,
            height_scale: range_f32(&mut rng, height_lo, height_hi),
            torso_leg_ratio: range_f32(&mut rng, 0.42, 0.58),
            head_scale: range_f32(&mut rng, 0.90, 1.10),
            shoulder_width_scale: range_f32(&mut rng, shoulder_lo, shoulder_hi),
            limb_thickness_scale: range_f32(&mut rng, limb_lo, limb_hi),
            total_height_m: range_f32(&mut rng, h.0, h.1),
            heads_tall: range_f32(&mut rng, heads.0, heads.1),
            crotch_frac: range_f32(&mut rng, crotch.0, crotch.1),
            shoulder_span_h: range_f32(&mut rng, span.0, span.1),
            waist_taper: (range_f32(&mut rng, waist.0, waist.1) + waist_shift).min(1.0),
            limb_taper: range_f32(&mut rng, ltaper.0, ltaper.1),
            hand_frac: range_f32(&mut rng, hand.0, hand.1),
            foot_frac: range_f32(&mut rng, foot.0, foot.1),
            neck_frac: range_f32(&mut rng, neck.0, neck.1),
            depth_flatten: depth,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = AvatarBody::for_did("did:plc:test");
        let b = AvatarBody::for_did("did:plc:test");
        assert_eq!(a.archetype, b.archetype);
        assert_eq!(a.height_scale, b.height_scale);
        assert_eq!(a.shoulder_width_scale, b.shoulder_width_scale);
    }

    #[test]
    fn scales_within_bounds() {
        for s in 0u64..16 {
            let b = AvatarBody::for_did(&format!("did:test:{s}"));
            assert!((0.7..=1.3).contains(&b.height_scale));
            assert!((0.4..=0.6).contains(&b.torso_leg_ratio));
            assert!((0.85..=1.15).contains(&b.head_scale));
            assert!((0.75..=1.25).contains(&b.shoulder_width_scale));
            assert!((0.75..=1.30).contains(&b.limb_thickness_scale));
            assert!((1.0..=1.95).contains(&b.total_height_m));
            assert!((3.2..=7.5).contains(&b.heads_tall));
            assert!((0.42..=0.53).contains(&b.crotch_frac));
            assert!((1.3..=2.5).contains(&b.shoulder_span_h));
            assert!((0.5..=1.0).contains(&b.waist_taper));
            assert!((0.6..=1.0).contains(&b.limb_taper));
            assert!((0.0..=0.4).contains(&b.neck_frac));
            assert!((0.7..=1.0).contains(&b.depth_flatten));
        }
    }

    #[test]
    fn tiers_covary() {
        // The canon fields must stay inside their tier's band — a Toy
        // body never rolls realistic-length legs or a tall head count.
        for s in 0u64..256 {
            let b = AvatarBody::for_seed(s);
            match b.tier {
                StylizationTier::Toy => {
                    assert!(b.heads_tall <= 4.0, "seed {s}: toy heads {}", b.heads_tall);
                    assert!(b.total_height_m <= 1.25);
                    assert!(b.neck_frac <= 0.10);
                }
                StylizationTier::Heroic => {
                    assert!(b.heads_tall >= 7.0);
                    assert!(b.total_height_m >= 1.80);
                }
                StylizationTier::Stylized => assert!((5.0..=6.0).contains(&b.heads_tall)),
                StylizationTier::Realistic => assert!((6.5..=7.0).contains(&b.heads_tall)),
            }
        }
    }

    #[test]
    fn tier_mix_is_weighted() {
        // Over a large population the weighted pick should land near
        // 45/25/15/15 — assert loose brackets, not exact frequencies.
        let mut counts = [0u32; 4];
        const N: u64 = 2000;
        for s in 0..N {
            let i = match AvatarBody::for_seed(s).tier {
                StylizationTier::Stylized => 0,
                StylizationTier::Heroic => 1,
                StylizationTier::Toy => 2,
                StylizationTier::Realistic => 3,
            };
            counts[i] += 1;
        }
        let frac = |c: u32| c as f32 / N as f32;
        assert!((0.38..=0.52).contains(&frac(counts[0])), "{counts:?}");
        assert!((0.19..=0.31).contains(&frac(counts[1])), "{counts:?}");
        assert!((0.10..=0.20).contains(&frac(counts[2])), "{counts:?}");
        assert!((0.10..=0.20).contains(&frac(counts[3])), "{counts:?}");
    }
}
