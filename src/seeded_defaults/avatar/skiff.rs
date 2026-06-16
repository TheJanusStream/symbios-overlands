//! Land-skiff design knobs for the ground-vehicle default avatar
//! family.
//!
//! Third sibling of [`super::vessel`] / [`super::airship`]. The skiff
//! is the Car-locomotion family: a low chassis slab with wheels or
//! skids, a cockpit canopy, and the shared Steam / Solar / Hybrid
//! ornament axis ([`VesselArchetype`]) deciding whether the engine
//! block grows exhaust funnels or a solar wing.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::seeded_defaults::avatar::vessel::VesselArchetype;
use crate::seeded_defaults::hash::fnv1a_64;
use crate::seeded_defaults::scene::{pick, range_f32, unit_f32};

const AVATAR_SKIFF_SALT: u64 = 0x5C1F_F000_5C1F_F000;

/// Running-gear family — what the chassis stands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkiffForm {
    /// Four wheels, one per corner.
    Rover,
    /// No wheels: two long hover-skids under the chassis.
    DuneSkiff,
    /// Single fat front wheel + two rear — a trike stance.
    Trike,
}

impl SkiffForm {
    pub const ALL: [Self; 3] = [Self::Rover, Self::DuneSkiff, Self::Trike];
}

/// Cockpit treatment over the chassis midsection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanopyStyle {
    /// Glassy dome sphere sunk into the deck.
    Bubble,
    /// Angular tapered shell.
    Shell,
    /// Open cockpit — just a low windscreen plate.
    Open,
}

impl CanopyStyle {
    pub const ALL: [Self; 3] = [Self::Bubble, Self::Shell, Self::Open];
}

/// All seeded skiff knobs. Dimension scales are multipliers on the
/// builder's nominal sizes (`1.0` = nominal).
#[derive(Clone, Copy, Debug)]
pub struct SkiffDesign {
    pub archetype: VesselArchetype,
    pub form: SkiffForm,
    pub canopy: CanopyStyle,

    /// Chassis slab length / width scales.
    pub chassis_length_scale: f32,
    pub chassis_width_scale: f32,
    /// Wheel radius scale (also skid girth for [`SkiffForm::DuneSkiff`]).
    pub wheel_radius_scale: f32,

    /// Exhaust funnels on the engine block (`0` on Solar archetypes,
    /// otherwise `1..=2`).
    pub exhaust_count: u32,
    /// Backward rake of the flag whip-antenna (radians around X).
    pub antenna_rake_rad: f32,
    /// Whether the stern carries a spoiler wing on struts.
    pub spoiler: bool,
}

impl SkiffDesign {
    pub fn for_did(did: &str) -> Self {
        Self::for_seed(fnv1a_64(did))
    }

    /// Derive from a pre-computed seed — the manual re-roll path.
    /// `for_did(did)` is exactly `for_seed(fnv1a_64(did))`.
    pub fn for_seed(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ AVATAR_SKIFF_SALT);

        let archetype = pick(&VesselArchetype::ALL, &mut rng);
        let form = pick(&SkiffForm::ALL, &mut rng);
        let canopy = pick(&CanopyStyle::ALL, &mut rng);

        let chassis_length_scale = range_f32(&mut rng, 0.90, 1.35);
        let chassis_width_scale = range_f32(&mut rng, 0.85, 1.25);
        let wheel_radius_scale = range_f32(&mut rng, 0.85, 1.40);

        let exhaust_count = if archetype.has_smokestacks() {
            1 + (unit_f32(&mut rng) * 2.0) as u32 // 1..=2
        } else {
            let _ = unit_f32(&mut rng);
            0
        };
        let antenna_rake_rad = range_f32(&mut rng, 10.0_f32.to_radians(), 35.0_f32.to_radians());
        let spoiler = unit_f32(&mut rng) < 0.5;

        Self {
            archetype,
            form,
            canopy,
            chassis_length_scale,
            chassis_width_scale,
            wheel_radius_scale,
            exhaust_count,
            antenna_rake_rad,
            spoiler,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = SkiffDesign::for_did("did:plc:test");
        let b = SkiffDesign::for_did("did:plc:test");
        assert_eq!(a.form, b.form);
        assert_eq!(a.canopy, b.canopy);
        assert_eq!(a.chassis_length_scale, b.chassis_length_scale);
        assert_eq!(a.exhaust_count, b.exhaust_count);
    }

    #[test]
    fn fields_in_range() {
        for s in 0u64..64 {
            let k = SkiffDesign::for_did(&format!("did:test:{s}"));
            assert!((0.9..=1.35).contains(&k.chassis_length_scale));
            assert!((0.85..=1.25).contains(&k.chassis_width_scale));
            assert!((0.85..=1.4).contains(&k.wheel_radius_scale));
            assert!(k.exhaust_count <= 2);
            assert!(k.antenna_rake_rad.is_finite());
        }
    }

    #[test]
    fn exhausts_match_archetype() {
        for s in 0u64..128 {
            let k = SkiffDesign::for_did(&format!("did:test:{s}"));
            if k.archetype.has_smokestacks() {
                assert!(k.exhaust_count >= 1, "Steam/Hybrid skiff without exhaust");
            } else {
                assert_eq!(k.exhaust_count, 0, "Solar skiff with exhaust");
            }
        }
    }
}
