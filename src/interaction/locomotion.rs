//! Locomotion → footprint-radius adapter. One trait, one impl per
//! locomotion preset, plus a single match dispatcher
//! ([`locomotion_footprint`]) used by the producer.
//!
//! `footprint_radius` is the canonical "how big is this avatar on a
//! surface" answer used by every consumer channel (shader ripple radii,
//! particle emission discs, stain stamp sizes). Centralising it here
//! means a per-preset tweak — say, a slightly larger emission disc for
//! the hover-boat — automatically reaches every effect.
//!
//! The trait lives in [`crate::interaction`] rather than alongside
//! [`crate::pds::avatar::locomotion`] to keep the PDS layer free of
//! gameplay-effect concerns. PDS describes "what the avatar is";
//! interaction describes "how it looks to the world."

use crate::pds::{
    AirplaneParams, CarParams, HelicopterParams, HoverBoatParams, HumanoidParams, LocomotionConfig,
};

/// Trait implemented by every locomotion preset (`*Params`) to expose a
/// single scalar: the world-space radius the avatar effectively
/// occupies on a contact surface.
///
/// Numbers here are *visual-effect radii*, not strict collider extents
/// — a 1.5× multiplier on a humanoid's capsule radius gives a splash
/// disc that surrounds the body instead of cutting into it. Tune per
/// preset; consumers don't second-guess.
pub trait LocomotionFootprint {
    fn footprint_radius(&self) -> f32;

    /// World-space vertical extent of the avatar — used to normalise
    /// water depth into the intensity scalar on a contact sample (and to
    /// derive body-bottom as `origin.y − total_height/2`). Clamped
    /// positive per impl so downstream divisions never see zero.
    fn total_height(&self) -> f32;
}

impl LocomotionFootprint for HumanoidParams {
    fn footprint_radius(&self) -> f32 {
        // Slightly larger than the literal capsule so effects ring the
        // body rather than emerge from inside it. Capsule radius is
        // already clamped ≥ 0.05 m by the PDS sanitiser, so the result
        // is always positive.
        self.capsule_radius.0 * 1.5
    }

    fn total_height(&self) -> f32 {
        // Delegates to the preset's own (inherent, PDS-side) method so
        // the collider and the contact classifier agree on one number.
        HumanoidParams::total_height(self)
    }
}

/// XZ "footprint" of a chassis-cuboid vehicle — the longer of width or
/// length. Shared by every cuboid-chassis preset so wakes and dust
/// scale with the vehicle's largest planar dimension regardless of
/// orientation.
fn cuboid_footprint(half_extents: [f32; 3]) -> f32 {
    half_extents[0].max(half_extents[2]).max(0.0)
}

/// Vertical extent of a chassis-cuboid vehicle. Floored so a degenerate
/// authored chassis can't zero the intensity denominator.
fn cuboid_height(half_extents: [f32; 3]) -> f32 {
    (half_extents[1] * 2.0).max(0.01)
}

impl LocomotionFootprint for HoverBoatParams {
    fn footprint_radius(&self) -> f32 {
        cuboid_footprint(self.chassis_half_extents.0)
    }
    fn total_height(&self) -> f32 {
        cuboid_height(self.chassis_half_extents.0)
    }
}

impl LocomotionFootprint for CarParams {
    fn footprint_radius(&self) -> f32 {
        cuboid_footprint(self.chassis_half_extents.0)
    }
    fn total_height(&self) -> f32 {
        cuboid_height(self.chassis_half_extents.0)
    }
}

impl LocomotionFootprint for HelicopterParams {
    fn footprint_radius(&self) -> f32 {
        cuboid_footprint(self.chassis_half_extents.0)
    }
    fn total_height(&self) -> f32 {
        cuboid_height(self.chassis_half_extents.0)
    }
}

impl LocomotionFootprint for AirplaneParams {
    fn footprint_radius(&self) -> f32 {
        cuboid_footprint(self.chassis_half_extents.0)
    }
    fn total_height(&self) -> f32 {
        cuboid_height(self.chassis_half_extents.0)
    }
}

/// Fallback for [`LocomotionConfig::Unknown`] (older record, new
/// client). Small enough to look "near point-source" but non-zero so
/// downstream divisions don't have to guard against it.
pub const UNKNOWN_FOOTPRINT: f32 = 0.5;

/// Vertical-extent fallback for [`LocomotionConfig::Unknown`] — a
/// roughly person-sized guess, non-zero for the same reason as
/// [`UNKNOWN_FOOTPRINT`].
pub const UNKNOWN_TOTAL_HEIGHT: f32 = 1.0;

/// Single-call accessor used by the contact classifier. Pulls the
/// footprint radius out of any locomotion config variant, returning
/// [`UNKNOWN_FOOTPRINT`] for `Unknown` so callers don't have to handle
/// the open-union case themselves.
pub fn locomotion_footprint(cfg: &LocomotionConfig) -> f32 {
    match cfg {
        LocomotionConfig::Humanoid(p) => p.footprint_radius(),
        LocomotionConfig::HoverBoat(p) => p.footprint_radius(),
        LocomotionConfig::Car(p) => p.footprint_radius(),
        LocomotionConfig::Helicopter(p) => p.footprint_radius(),
        LocomotionConfig::Airplane(p) => p.footprint_radius(),
        LocomotionConfig::Unknown => UNKNOWN_FOOTPRINT,
    }
}

/// Avatar "vertical extent" used to normalise water depth into the
/// intensity scalar on a contact sample. Matches the same per-preset
/// dispatch pattern as [`locomotion_footprint`], pulling
/// [`LocomotionFootprint::total_height`] out of any config variant.
pub fn locomotion_total_height(cfg: &LocomotionConfig) -> f32 {
    match cfg {
        LocomotionConfig::Humanoid(p) => LocomotionFootprint::total_height(p.as_ref()),
        LocomotionConfig::HoverBoat(p) => p.total_height(),
        LocomotionConfig::Car(p) => p.total_height(),
        LocomotionConfig::Helicopter(p) => p.total_height(),
        LocomotionConfig::Airplane(p) => p.total_height(),
        LocomotionConfig::Unknown => UNKNOWN_TOTAL_HEIGHT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::types::{Fp, Fp3};

    #[test]
    fn humanoid_footprint_scales_with_capsule_radius() {
        let mut h = HumanoidParams::default();
        let baseline = h.footprint_radius();
        h.capsule_radius = Fp(h.capsule_radius.0 * 2.0);
        assert!((h.footprint_radius() - baseline * 2.0).abs() < 1e-5);
    }

    #[test]
    fn cuboid_footprint_uses_larger_of_x_or_z() {
        let mut hb = HoverBoatParams {
            chassis_half_extents: Fp3([1.0, 0.4, 3.0]),
            ..Default::default()
        };
        assert!((hb.footprint_radius() - 3.0).abs() < 1e-5);
        hb.chassis_half_extents = Fp3([4.0, 0.4, 0.5]);
        assert!((hb.footprint_radius() - 4.0).abs() < 1e-5);
    }

    #[test]
    fn unknown_uses_default_fallback() {
        let cfg = LocomotionConfig::Unknown;
        assert!((locomotion_footprint(&cfg) - UNKNOWN_FOOTPRINT).abs() < 1e-5);
        assert!((locomotion_total_height(&cfg) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn humanoid_total_height_matches_preset() {
        let h = HumanoidParams::default();
        let cfg = LocomotionConfig::Humanoid(Box::new(h.clone()));
        assert!((locomotion_total_height(&cfg) - h.total_height()).abs() < 1e-5);
    }
}
