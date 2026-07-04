//! Humanoid proportion blueprint — turns the seeded [`AvatarBody`] canon
//! knobs into concrete world-space dimensions (metres, hips at `y = 0`).
//!
//! One struct implements the classical figure canons once, so the part
//! builders ([`crate::pds::avatar::parts`]), the assembler
//! ([`crate::pds::avatar::default_visuals`]), and the locomotion capsule all
//! agree on the same skeleton. The landmark rules encoded here (see
//! `docs`/the avatar-overhaul research notes):
//!
//! - the crotch line sits at `crotch_frac` of total height (~50 %: legs are
//!   half the figure),
//! - the hanging wrist lands at the crotch line (fingertips mid-thigh),
//! - the upper limb segment is ~1.2× the lower (equal segments read
//!   mechanical),
//! - shoulder span is measured in head-heights (`shoulder_span_h`),
//! - limbs taper distally (`limb_taper`) and the hands/feet flare back out,
//! - the trunk is wider than deep (`depth_flatten`) and V-tapers from chest
//!   to waist (`waist_taper`).

use super::body::AvatarBody;
pub use super::body::StylizationTier;

/// Concrete humanoid skeleton dimensions in metres, hips-origin (`y = 0`,
/// feet at `-leg_total()`, crown at `total_h - leg_total()`). Derived purely
/// from [`AvatarBody`] — same seed, same blueprint.
#[derive(Clone, Copy, Debug)]
pub struct HumanoidBlueprint {
    pub tier: StylizationTier,
    /// Full ground→crown height (the locomotion capsule height too).
    pub total_h: f32,
    /// The head-unit H — total height / heads-tall.
    pub head_unit: f32,

    // --- Head / neck (heights relative to the hips origin) ---
    /// Head-part mount: the skull sphere's centre.
    pub head_y: f32,
    /// Skull sphere radius (hair sits on top of this).
    pub head_r: f32,
    /// Hat mount — the crown, just above the hair mass.
    pub hat_y: f32,
    /// Visible neck column height (0 ≈ head seated on the shoulders).
    pub neck_len: f32,
    pub neck_r: f32,

    // --- Torso ---
    /// Shoulder line — arm mounts and the top of the trunk's yoke.
    pub shoulder_y: f32,
    /// Arm mount lateral offset (shoulder pivot).
    pub shoulder_x: f32,
    /// Trunk radius at the chest (top) and waist (bottom).
    pub chest_r: f32,
    pub waist_r: f32,
    /// Torso-part mount (trunk capsule centre).
    pub torso_y: f32,
    /// Trunk capsule cylinder length.
    pub trunk_len: f32,
    /// Z-depth as a fraction of X-width for the whole trunk.
    pub depth: f32,

    // --- Arms (part-local lengths; the part hangs from the shoulder) ---
    pub arm_r: f32,
    pub upper_arm: f32,
    pub forearm: f32,
    pub hand_len: f32,
    /// Distal/proximal radius ratio for both limbs.
    pub limb_taper: f32,

    // --- Legs ---
    /// Leg mount lateral offset (hip pivot).
    pub hip_x: f32,
    /// Leg girth at the knee (thigh flares above, shin tapers below).
    pub leg_r: f32,
    pub thigh: f32,
    pub shin: f32,
    pub foot_len: f32,
}

impl HumanoidBlueprint {
    pub fn for_seed(seed: u64) -> Self {
        Self::from_body(&AvatarBody::for_seed(seed))
    }

    pub fn from_body(b: &AvatarBody) -> Self {
        let total_h = b.total_height_m;
        let h = total_h / b.heads_tall;

        // Legs: ground→crotch, nudged by the legacy torso:leg ratio so
        // two same-tier seeds still differ in build. Thigh reads ~1.15×
        // the shin (upper segment longer), and the foot's sole/ankle
        // drop comes out of the leg budget so feet meet the ground.
        let crotch = (b.crotch_frac - (b.torso_leg_ratio - 0.5) * 0.08).clamp(0.40, 0.54);
        let leg_total = crotch * total_h;
        let sole_h = 0.03 + 0.09 * h;
        let thigh = 0.535 * (leg_total - sole_h);
        let shin = 0.465 * (leg_total - sole_h);

        // Head: the skull sphere fills most of the head unit; hair adds
        // the rest above (~0.3 r), so the crown of the *hair* touches
        // the figure's total height.
        let upper_h = total_h - leg_total;
        let head_r = 0.40 * h * b.head_scale;
        let head_y = upper_h - 1.32 * head_r;
        let chin = head_y - 1.12 * head_r;
        let hat_y = head_y + 1.35 * head_r;

        // Neck: short and thick (a thin bare cylinder is the weakest
        // read on a primitive figure — ≥ half the head's width).
        let neck_len = b.neck_frac * h;
        let neck_r = (0.50 * head_r).max(0.035);
        let shoulder_y = chin - neck_len;

        // Shoulders / trunk: span in head-heights; the trunk's chest
        // reaches the deltoids' inner edge so arms attach to mass, and
        // the waist V-tapers below.
        let shoulder_half = b.shoulder_span_h * h * 0.5;
        let arm_r = 0.165 * h * b.limb_thickness_scale;
        let shoulder_x = shoulder_half - arm_r;
        let chest_r = (shoulder_x - 0.55 * arm_r).max(0.055);
        let waist_r = b.waist_taper * chest_r;
        // Trunk cylinder spans from just above the pelvis to a little
        // under the shoulder line (the yoke fills the last stretch). Its
        // top hemisphere peaks a waist-radius above the cylinder, so cap
        // it below the chin — on short-necked, low-taper builds the dome
        // was swallowing the jaw.
        let trunk_bottom = 0.02_f32;
        let trunk_top = (shoulder_y - 0.35 * chest_r).min(chin - waist_r - 0.02);
        let trunk_len = (trunk_top - trunk_bottom).max(0.08);
        let torso_y = 0.5 * (trunk_top + trunk_bottom);

        // Arms: the hanging wrist lands at the crotch line (y ≈ 0),
        // upper ~1.2× the forearm; splay/bend eat a few percent of the
        // straight-line reach, and the upper arm sinks ~0.3 r into the
        // shoulder joint (the anti-poke seat), which this factor repays.
        let arm_total = 0.93 * shoulder_y;
        let upper_arm = 0.545 * arm_total;
        let forearm = 0.455 * arm_total;
        let hand_len = b.hand_frac * h;

        // Legs' lateral stance: thighs nearly touch at the crotch.
        let leg_r = 0.235 * h * b.limb_thickness_scale;
        let hip_x = (leg_r * 1.08).min(waist_r * 0.75);
        let foot_len = b.foot_frac * h;

        Self {
            tier: b.tier,
            total_h,
            head_unit: h,
            head_y,
            head_r,
            hat_y,
            neck_len,
            neck_r,
            shoulder_y,
            shoulder_x,
            chest_r,
            waist_r,
            torso_y,
            trunk_len,
            depth: b.depth_flatten,
            arm_r,
            upper_arm,
            forearm,
            hand_len,
            limb_taper: b.limb_taper,
            hip_x,
            leg_r,
            thigh,
            shin,
            foot_len,
        }
    }

    /// Ground→hips distance (feet sit at `-leg_total()` in part space).
    pub fn leg_total(&self) -> f32 {
        self.thigh + self.shin + (0.03 + 0.09 * self.head_unit)
    }

    /// Trunk surface radius at a world height — linear waist→chest along
    /// the cylinder (the capsule flare is linear in the vertex pass), so
    /// chest decals can seat on the actual surface instead of floating at
    /// the top radius.
    pub fn trunk_radius_at(&self, y: f32) -> f32 {
        let bottom = self.torso_y - 0.5 * self.trunk_len;
        let t = ((y - bottom) / self.trunk_len).clamp(0.0, 1.0);
        self.waist_r + (self.chest_r - self.waist_r) * t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = HumanoidBlueprint::for_seed(7);
        let b = HumanoidBlueprint::for_seed(7);
        assert_eq!(a.total_h, b.total_h);
        assert_eq!(a.shoulder_x, b.shoulder_x);
    }

    #[test]
    fn landmarks_hold_across_seeds() {
        for s in 0u64..128 {
            let bp = HumanoidBlueprint::for_seed(s);
            // Feet meet the ground: legs + sole = the crotch line.
            let crotch_frac = bp.leg_total() / bp.total_h;
            assert!(
                (0.40..=0.54).contains(&crotch_frac),
                "seed {s}: crotch at {crotch_frac}"
            );
            // The hanging wrist lands near the crotch line (y = 0):
            // shoulder height minus the two arm segments.
            let wrist_y = bp.shoulder_y - (bp.upper_arm + bp.forearm);
            assert!(
                wrist_y.abs() <= 0.06 * bp.total_h,
                "seed {s}: wrist at {wrist_y}"
            );
            // Upper segments are longer than lower ones.
            assert!(bp.upper_arm > bp.forearm);
            assert!(bp.thigh > bp.shin);
            // The trunk stays inside the shoulder span and keeps a V.
            assert!(bp.chest_r < bp.shoulder_x);
            assert!(bp.waist_r <= bp.chest_r);
            // Everything is positive and finite.
            for v in [
                bp.total_h,
                bp.head_r,
                bp.trunk_len,
                bp.arm_r,
                bp.leg_r,
                bp.thigh,
                bp.shin,
                bp.foot_len,
                bp.hand_len,
            ] {
                assert!(v.is_finite() && v > 0.0, "seed {s}: bad dim {v}");
            }
            // The head stays above the shoulders, shoulders above hips.
            assert!(bp.head_y - bp.head_r > bp.shoulder_y);
            assert!(bp.shoulder_y > bp.torso_y);
        }
    }

    #[test]
    fn tiers_read_in_the_silhouette() {
        // Find one seed per tier and check the register actually shows.
        let mut seen = [false; 4];
        for s in 0u64..512 {
            let bp = HumanoidBlueprint::for_seed(s);
            match bp.tier {
                StylizationTier::Toy => {
                    seen[0] = true;
                    // A toy figure is short with a proportionally huge head.
                    assert!(bp.total_h <= 1.25);
                    assert!(bp.head_r / bp.total_h >= 0.08);
                }
                StylizationTier::Heroic => {
                    seen[1] = true;
                    assert!(bp.total_h >= 1.80);
                }
                StylizationTier::Stylized => seen[2] = true,
                StylizationTier::Realistic => seen[3] = true,
            }
        }
        assert_eq!(seen, [true; 4], "some tier never sampled in 512 seeds");
    }
}
