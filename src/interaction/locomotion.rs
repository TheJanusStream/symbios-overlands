//! Locomotion → footprint-radius adapter. One trait, one impl per
//! locomotion preset, plus a single match dispatcher
//! ([`locomotion_footprint`]) used by the producer.
//!
//! `footprint_radius` is the canonical "how big is this avatar on a
//! surface" answer used by every consumer channel (shader ripple radii,
//! particle emission discs, stain stamp sizes). Centralising it here
//! means a per-preset tweak - say, a slightly larger emission disc for
//! the hover-boat - automatically reaches every effect.
//!
//! The trait lives in [`crate::interaction`] rather than alongside
//! [`crate::pds::avatar::locomotion`] to keep the PDS layer free of
//! gameplay-effect concerns. PDS describes "what the avatar is";
//! interaction describes "how it looks to the world."

use bevy::math::Vec2;

use crate::config::terrain::water::wake as wcfg;
use crate::pds::{
    AirplaneParams, CarParams, HelicopterParams, HoverBoatParams, HumanoidParams, LocomotionConfig,
};

/// Trait implemented by every locomotion preset (`*Params`) to expose a
/// single scalar: the world-space radius the avatar effectively
/// occupies on a contact surface.
///
/// Numbers here are *visual-effect radii*, not strict collider extents -
/// a 1.5× multiplier on a humanoid's capsule radius gives a splash
/// disc that surrounds the body instead of cutting into it. Tune per
/// preset; consumers don't second-guess.
pub trait LocomotionFootprint {
    fn footprint_radius(&self) -> f32;

    /// World-space vertical extent of the avatar - used to normalise
    /// water depth into the intensity scalar on a contact sample (and to
    /// derive body-bottom as `origin.y − total_height/2`). Clamped
    /// positive per impl so downstream divisions never see zero.
    fn total_height(&self) -> f32;

    /// How far below its body's bottom the avatar reaches the ground (#1549):
    /// a wheeled or hovering body rides on its springs, its box clear of the
    /// ground by up to their rest length, and what touches the ground is its
    /// wheels or skirt. 0 for a body that stands on its own bottom.
    fn ground_reach(&self) -> f32 {
        0.0
    }

    /// How far above the water its body's bottom may be and still disturb it
    /// on entering (#1654). A person's feet must touch the water
    /// ([`wcfg::CONTACT_SLACK`]); a vehicle or an aircraft need only skim it
    /// ([`wcfg::SKIM_SLACK`]) - a hover-boat floats with its box clear of the
    /// surface, and an airframe flying low over a lake scores it.
    fn water_slack(&self) -> f32 {
        wcfg::CONTACT_SLACK
    }

    /// How far the body reaches from its middle to its outline toward a
    /// horizontal direction in its own frame - `x` across it, `y` ahead along
    /// it; only their sizes count (#1751). Where an eye stands on it: a
    /// first-person view at its nose, the agent's eye view toward whatever it
    /// looks at. Every way the footprint radius unless a preset knows its own
    /// outline.
    fn reach(&self, _toward: Vec2) -> f32 {
        self.footprint_radius()
    }
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

/// XZ "footprint" of a chassis-cuboid vehicle - the longer of width or
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

/// How far a chassis-cuboid vehicle reaches from its middle toward `toward`
/// in its own frame (#1751): to the face of its box the direction leaves it
/// through - its nose straight ahead, its flank to the side - where the
/// footprint, the longer of its width and length, put the eyes of a vehicle
/// wider than long out in front of its nose.
fn cuboid_reach(half_extents: [f32; 3], toward: Vec2) -> f32 {
    let toward = toward.normalize_or(Vec2::Y);
    let face = |half: f32, share: f32| {
        if share.abs() > 1e-6 {
            half / share.abs()
        } else {
            f32::INFINITY
        }
    };
    face(half_extents[0], toward.x)
        .min(face(half_extents[2], toward.y))
        .max(0.0)
}

impl LocomotionFootprint for HoverBoatParams {
    fn footprint_radius(&self) -> f32 {
        cuboid_footprint(self.chassis_half_extents.0)
    }
    fn total_height(&self) -> f32 {
        cuboid_height(self.chassis_half_extents.0)
    }
    fn reach(&self, toward: Vec2) -> f32 {
        cuboid_reach(self.chassis_half_extents.0, toward)
    }
    fn ground_reach(&self) -> f32 {
        self.suspension_rest_length.0.max(0.0)
    }
    fn water_slack(&self) -> f32 {
        wcfg::SKIM_SLACK
    }
}

impl LocomotionFootprint for CarParams {
    fn footprint_radius(&self) -> f32 {
        cuboid_footprint(self.chassis_half_extents.0)
    }
    fn total_height(&self) -> f32 {
        cuboid_height(self.chassis_half_extents.0)
    }
    fn reach(&self, toward: Vec2) -> f32 {
        cuboid_reach(self.chassis_half_extents.0, toward)
    }
    fn ground_reach(&self) -> f32 {
        self.suspension_rest_length.0.max(0.0)
    }
    fn water_slack(&self) -> f32 {
        wcfg::SKIM_SLACK
    }
}

impl LocomotionFootprint for HelicopterParams {
    fn footprint_radius(&self) -> f32 {
        cuboid_footprint(self.chassis_half_extents.0)
    }
    fn total_height(&self) -> f32 {
        cuboid_height(self.chassis_half_extents.0)
    }
    fn reach(&self, toward: Vec2) -> f32 {
        cuboid_reach(self.chassis_half_extents.0, toward)
    }
    fn water_slack(&self) -> f32 {
        wcfg::SKIM_SLACK
    }
}

impl LocomotionFootprint for AirplaneParams {
    fn footprint_radius(&self) -> f32 {
        cuboid_footprint(self.chassis_half_extents.0)
    }
    fn total_height(&self) -> f32 {
        cuboid_height(self.chassis_half_extents.0)
    }
    fn reach(&self, toward: Vec2) -> f32 {
        cuboid_reach(self.chassis_half_extents.0, toward)
    }
    fn water_slack(&self) -> f32 {
        wcfg::SKIM_SLACK
    }
}

/// Fallback for [`LocomotionConfig::Unknown`] (older record, new
/// client). Small enough to look "near point-source" but non-zero so
/// downstream divisions don't have to guard against it.
pub const UNKNOWN_FOOTPRINT: f32 = 0.5;

/// Vertical-extent fallback for [`LocomotionConfig::Unknown`] - a
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

/// How far any locomotion config's body reaches from its middle toward
/// `toward`, a horizontal direction in its own frame - `x` across, `y` ahead
/// ([`LocomotionFootprint::reach`], #1751); [`UNKNOWN_FOOTPRINT`] for
/// `Unknown`.
pub fn locomotion_reach(cfg: &LocomotionConfig, toward: Vec2) -> f32 {
    match cfg {
        LocomotionConfig::Humanoid(p) => p.reach(toward),
        LocomotionConfig::HoverBoat(p) => p.reach(toward),
        LocomotionConfig::Car(p) => p.reach(toward),
        LocomotionConfig::Helicopter(p) => p.reach(toward),
        LocomotionConfig::Airplane(p) => p.reach(toward),
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

/// How far below its body's bottom any locomotion config reaches the
/// ground ([`LocomotionFootprint::ground_reach`]); 0 for `Unknown`.
pub fn locomotion_ground_reach(cfg: &LocomotionConfig) -> f32 {
    match cfg {
        LocomotionConfig::Humanoid(p) => LocomotionFootprint::ground_reach(p.as_ref()),
        LocomotionConfig::HoverBoat(p) => p.ground_reach(),
        LocomotionConfig::Car(p) => p.ground_reach(),
        LocomotionConfig::Helicopter(p) => p.ground_reach(),
        LocomotionConfig::Airplane(p) => p.ground_reach(),
        LocomotionConfig::Unknown => 0.0,
    }
}

/// How far above the water any locomotion config's body bottom may be and
/// still disturb it on entering ([`LocomotionFootprint::water_slack`]); the
/// tight person's tolerance for `Unknown`.
pub fn locomotion_water_slack(cfg: &LocomotionConfig) -> f32 {
    match cfg {
        LocomotionConfig::Humanoid(p) => LocomotionFootprint::water_slack(p.as_ref()),
        LocomotionConfig::HoverBoat(p) => p.water_slack(),
        LocomotionConfig::Car(p) => p.water_slack(),
        LocomotionConfig::Helicopter(p) => p.water_slack(),
        LocomotionConfig::Airplane(p) => p.water_slack(),
        LocomotionConfig::Unknown => wcfg::CONTACT_SLACK,
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

    /// **A body's reach ends at its outline** (#1751): a vehicle's box at its
    /// nose straight ahead and its flank to the side - a vehicle wider than
    /// long reaches its half-length ahead, not its width, which put a nose
    /// camera out in front of it - and the face the direction leaves through
    /// on a diagonal; a person's footprint circle every way.
    #[test]
    fn a_reach_ends_at_the_bodys_outline() {
        let wide = LocomotionConfig::Car(Box::new(CarParams {
            chassis_half_extents: Fp3([2.0, 0.5, 1.0]),
            ..Default::default()
        }));
        let reach = |toward: Vec2| locomotion_reach(&wide, toward);
        assert!((reach(Vec2::Y) - 1.0).abs() < 1e-5, "ahead: the nose");
        assert!((reach(Vec2::NEG_Y) - 1.0).abs() < 1e-5, "behind: the tail");
        assert!((reach(Vec2::X) - 2.0).abs() < 1e-5, "across: the flank");
        let diagonal = reach(Vec2::new(1.0, 1.0));
        assert!(
            (diagonal - std::f32::consts::SQRT_2).abs() < 1e-5,
            "a diagonal leaves through the nose: {diagonal}"
        );
        let person = LocomotionConfig::Humanoid(Box::default());
        let radius = locomotion_footprint(&person);
        for toward in [Vec2::Y, Vec2::X, Vec2::new(0.6, -0.8)] {
            assert_eq!(locomotion_reach(&person, toward), radius);
        }
    }

    /// A vehicle or an aircraft skims the water and a person must touch it
    /// (#1654), whatever their tuning.
    #[test]
    fn vehicles_and_aircraft_skim_the_water_and_a_person_must_touch_it() {
        let slack = |cfg: LocomotionConfig| locomotion_water_slack(&cfg);
        assert_eq!(
            slack(LocomotionConfig::Humanoid(Box::default())),
            wcfg::CONTACT_SLACK
        );
        assert_eq!(slack(LocomotionConfig::Unknown), wcfg::CONTACT_SLACK);
        for cfg in [
            LocomotionConfig::HoverBoat(Box::default()),
            LocomotionConfig::Car(Box::default()),
            LocomotionConfig::Helicopter(Box::default()),
            LocomotionConfig::Airplane(Box::default()),
        ] {
            assert_eq!(slack(cfg.clone()), wcfg::SKIM_SLACK, "{cfg:?}");
        }
    }

    #[test]
    fn unknown_uses_default_fallback() {
        let cfg = LocomotionConfig::Unknown;
        assert!((locomotion_footprint(&cfg) - UNKNOWN_FOOTPRINT).abs() < 1e-5);
        assert!((locomotion_total_height(&cfg) - 1.0).abs() < 1e-5);
    }

    /// A car and a hover-boat reach the ground by their suspension's rest
    /// length below their box (#1549); a body that stands on its own bottom
    /// reaches no further.
    #[test]
    fn a_body_on_springs_reaches_the_ground_by_their_rest_length() {
        let car = CarParams::default();
        let boat = HoverBoatParams::default();
        let reach = |cfg: LocomotionConfig| locomotion_ground_reach(&cfg);
        assert_eq!(
            reach(LocomotionConfig::Car(Box::new(car.clone()))),
            car.suspension_rest_length.0
        );
        assert_eq!(
            reach(LocomotionConfig::HoverBoat(Box::new(boat.clone()))),
            boat.suspension_rest_length.0
        );
        assert_eq!(reach(LocomotionConfig::Humanoid(Box::default())), 0.0);
        assert_eq!(reach(LocomotionConfig::Unknown), 0.0);
    }

    #[test]
    fn humanoid_total_height_matches_preset() {
        let h = HumanoidParams::default();
        let cfg = LocomotionConfig::Humanoid(Box::new(h.clone()));
        assert!((locomotion_total_height(&cfg) - h.total_height()).abs() < 1e-5);
    }
}
