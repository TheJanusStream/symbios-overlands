//! Her stance (#1624 Q-B): tail-down on her tail wheel, as a C.30 sits -
//! every part she is drawn with turned [`GROUND_DEG`] nose-up about her
//! main axle - and the box she stands in, measured off her as she stands.
//!
//! The turn is the prototype's `tilt_tree`, and it stands her in the
//! family's own level frame: her mains' tyres' lowest points are under the
//! axle the turn is about, so they stay on the ground as she turns, and her
//! tail wheel's axle is set where the turn brings its tyre down beside them
//! ([`super::gear::tail_wheel_y`]). Nothing the family does with a level
//! box changes: the Helicopter preset rests it on its floor (#1612), which
//! is her tyres, and her lean (twelve degrees) levels her flying forward.

use bevy::math::{Affine3A, Quat, Vec3};

use crate::catalogue::items::measure::Bounds;
use crate::pds::generator::{Generator, GeneratorKind};
use crate::pds::types::{Fp3, Fp4};
use crate::seeded_defaults::OrnatenessTier;
use crate::world_builder::primitive_outline;

use super::super::{AutogyroColours, PodPlan};
use super::{MAIN_Z, PROFILE, WHEEL_R, draw};

/// The attitude she stands at on her three wheels (degrees, nose up): a
/// C.30's.
pub(crate) const GROUND_DEG: f32 = 12.0;

/// The turn that stands her: [`GROUND_DEG`] nose-up, about her lateral
/// axis.
pub(super) fn turn() -> Quat {
    Quat::from_rotation_x(-GROUND_DEG.to_radians())
}

/// Her main axle's line, at her middle (drawn frame): what she is turned
/// about.
fn pivot(plan: &PodPlan) -> Vec3 {
    Vec3::from(plan.at(0.0, WHEEL_R, MAIN_Z))
}

/// Where a point of hers drawn level stands with her (drawn frame).
pub(super) fn stood(plan: &PodPlan, at: [f32; 3]) -> [f32; 3] {
    let pivot = pivot(plan);
    (pivot + turn() * (Vec3::from(at) - pivot)).to_array()
}

/// Stand her: turn every part `kids` holds, drawn level, about her main
/// axle onto her tail wheel.
///
/// A sweep's stations are turned and its node is not: a flattened sweep
/// keeps its own scale, square to the world, which under a turn would be no
/// similarity - and `touch` cannot read one. Every other part is carried
/// round the axle and turned with it, but for a part turning about her
/// lateral axis: a wheel turned about its own axle is the same wheel, and
/// unturned each main tyre's lowest vertex stays on the ground.
pub(super) fn stand(kids: &mut [Generator], plan: &PodPlan) {
    let (pivot, turn) = (pivot(plan), turn());
    let about = |p: Vec3| pivot + turn * (p - pivot);
    for part in kids.iter_mut() {
        let t = &mut part.transform;
        let at = Vec3::from(t.translation.0);
        if let GeneratorKind::Spine { points, .. } = &mut part.kind {
            debug_assert!(
                Quat::from_array(t.rotation.0).is_near_identity(),
                "a sweep is drawn unturned, its scale square to the world"
            );
            let scale = Vec3::from(t.scale.0);
            for p in points.iter_mut() {
                let drawn = about(at + Vec3::from(p.position.0) * scale);
                p.position = Fp3((drawn / scale).to_array());
            }
            t.translation = Fp3([0.0; 3]);
            continue;
        }
        t.translation = Fp3(about(at).to_array());
        let wheel = part
            .spin
            .as_ref()
            .is_some_and(|s| s.axis.0 == [1.0, 0.0, 0.0]);
        if !wheel {
            let r = Quat::from_array(part.transform.rotation.0);
            part.transform.rotation = Fp4((turn * r).to_array());
        }
    }
}

/// Every part she is drawn with at `ornateness`, level, in the default
/// material - her shape, which is the same in any paint. What the guards
/// read where a claim holds whatever her stance, and what [`drawn`] stands.
pub(crate) fn draw_level(plan: &PodPlan, ornateness: OrnatenessTier) -> Vec<Generator> {
    let mut kids = Vec::new();
    draw(&mut kids, plan, &AutogyroColours::default(), ornateness);
    kids
}

/// The bounds of the machine she draws at `ornateness`, standing (drawn
/// frame): the mesher's own vertices ([`primitive_outline`], which reads a
/// sweep's without building its mesh), through every part's transform and
/// through a turning hub's to what it carries - but its blades, each a
/// block it carries that does not turn of itself. Her box holds her
/// airframe, and the blades stand outside it (owner decision 3 of #1611):
/// the rotor's disc and the airscrew's. Her tail wheel is a turning hub
/// carried by a turning hub, and in it.
pub(super) fn drawn(plan: &PodPlan, ornateness: OrnatenessTier) -> Bounds {
    let mut kids = draw_level(plan, ornateness);
    stand(&mut kids, plan);
    let mut out: Option<Bounds> = None;
    for part in &kids {
        reach_into(part, Affine3A::IDENTITY, &mut out);
    }
    out.expect("she draws something")
}

/// `part`'s vertices under `parent`, and its children's - a turning hub's
/// but its blades: the blocks it carries that do not turn of themselves -
/// folded into `out`.
fn reach_into(part: &Generator, parent: Affine3A, out: &mut Option<Bounds>) {
    let t = &part.transform;
    let world = parent
        * Affine3A::from_scale_rotation_translation(
            Vec3::from(t.scale.0),
            Quat::from_array(t.rotation.0).normalize(),
            Vec3::from(t.translation.0),
        );
    for p in primitive_outline(&part.kind) {
        let at = world.transform_point3(p);
        *out = Some(match *out {
            None => Bounds { min: at, max: at },
            Some(b) => Bounds {
                min: b.min.min(at),
                max: b.max.max(at),
            },
        });
    }
    let turning = part.spin.is_some();
    for child in &part.children {
        if turning && matches!(child.kind, GeneratorKind::Cuboid { .. }) && child.spin.is_none() {
            continue;
        }
        reach_into(child, world, out);
    }
}

/// How far ahead of her first station and aft of her boom's end she is
/// drawn standing (m, prototype frame) - her hubcap's tip, raised with her
/// nose and drawn back by the turn, and the top of her tail post's end,
/// carried aft as it drops, or on her shorter booms her fin's trailing
/// edge, which stands higher and so is carried further - read off the
/// plain machine as she stands, blades pruned ([`drawn`]). No tier draws
/// past either end. The turn is rigid, so neither reach moves with the
/// centring it sets.
pub(super) fn reach(plan: &PodPlan) -> (f32, f32) {
    let drawn = drawn(plan, OrnatenessTier::Plain);
    let nose = plan.at(0.0, 0.0, plan.nose())[2];
    let tail = plan.at(0.0, 0.0, PROFILE.boom_end)[2];
    (
        (drawn.max.z - nose) / plan.len(1.0),
        (tail - drawn.min.z) / plan.len(1.0),
    )
}
