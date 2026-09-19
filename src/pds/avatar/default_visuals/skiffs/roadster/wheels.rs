//! The roadster's wheels: pressed discs, balloon tyres, or wire wheels -
//! three styles behind one trait, the way the sloop's rigs are (#1367).
//!
//! Every style is TURNED. A tyre is a smooth Lathe ring and the disc and its
//! proud cap a second one. **A Lathe whose profile ends at a non-zero radius
//! is closed with a full disc**, so the tyre is really a drum: a wheel disc
//! has to stand PROUD of the tyre's end plane or it is swallowed, and that is
//! arranged here rather than left to luck - the disc's outer radius is the
//! tyre's own lip times 1.06. The INBOARD end is the same trap mirrored:
//! untreated, the far wheels present a matte black plate that at play
//! distance reads as a hole punched through the car, so each carries a brake
//! drum too.
//!
//! The wire wheel turns that same trap to use: its spokes stand just proud of
//! the tyre's flat end cap, and the cap is the dark ground they read against.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::pds::generator::Generator;
use crate::seeded_defaults::RoadsterWheels;

use super::super::super::common::quat_z;
use super::super::{SkiffColours, dim};
use super::RoadsterPlan;
use super::{line, turned};

/// The three turned silhouettes a wheel is made of.
pub(super) struct WheelProfiles {
    pub(super) tyre: Vec<(f32, f32)>,
    pub(super) disc: Vec<(f32, f32)>,
    /// The inboard face - see the module docs.
    pub(super) drum: Vec<(f32, f32)>,
    /// How far each flat end cap of the tyre stands from the wheel's centre
    /// plane (m): where anything mounted against the tyre's face meets it.
    pub(super) face: f32,
    /// The tyre's widest half-width (m) - what a mount's bedding is measured
    /// in.
    pub(super) half_width: f32,
}

/// One wheel style.
pub(super) trait Wheels: Sync {
    /// The outer radius over the blueprint's. The plan carries the result
    /// ([`super::plan_of`]), so the axle line follows it.
    fn radius_factor(&self) -> f32 {
        1.0
    }

    /// The silhouettes a wheel of radius `wheel_r` is turned from.
    fn profiles(&self, wheel_r: f32) -> WheelProfiles;

    /// The silhouettes a SPARE of radius `wheel_r` is turned from. Spares are
    /// pressed discs whatever the car rolls on: a spare's face is small, and
    /// spokes on it would cost four nodes for nothing a player can see.
    fn spare(&self, wheel_r: f32) -> WheelProfiles {
        pressed_disc(wheel_r)
    }

    /// Draw the road wheels on every anchor the plan publishes.
    fn build(&self, kids: &mut Vec<Generator>, plan: &RoadsterPlan, c: &SkiffColours) {
        let w = self.profiles(plan.wheel_r);
        for anchor in plan.wheel_anchors() {
            let lay = lay(anchor[0]);
            kids.push(turned(&w.tyre, 28, true, c.rubber.clone(), anchor, lay));
            kids.push(turned(&w.disc, 28, false, c.disc.clone(), anchor, lay));
            kids.push(turned(&w.drum, 20, false, c.machinery.clone(), anchor, lay));
        }
    }
}

/// The drawing for a wheel style - the one match over [`RoadsterWheels`].
pub(super) fn wheels(w: RoadsterWheels) -> &'static dyn Wheels {
    match w {
        RoadsterWheels::Disc => &PressedDisc,
        RoadsterWheels::Balloon => &Balloon,
        RoadsterWheels::Wire => &Wire,
    }
}

/// Lay a wheel on its axle with its cap facing outboard on BOTH sides.
fn lay(x: f32) -> [f32; 4] {
    quat_z(-x.signum() * FRAC_PI_2)
}

/// The pressed-steel disc the roadster was built with.
fn pressed_disc(wheel_r: f32) -> WheelProfiles {
    let lip = wheel_r * 0.683;
    let w = wheel_r * 0.193;
    let tyre = vec![
        (lip, -w * 0.86),
        (wheel_r * 0.873, -w),
        (wheel_r * 0.977, -w * 0.62),
        (wheel_r, 0.0),
        (wheel_r * 0.977, w * 0.62),
        (wheel_r * 0.873, w),
        (lip, w * 0.86),
    ];
    let disc = vec![
        (0.0, -w * 0.78),
        (lip * 1.034, -w * 0.78),
        (lip * 1.063, -w * 0.52),
        (lip * 1.063, w * 0.52),
        (lip * 1.000, w * 0.72),
        (lip * 0.732, w * 0.86),
        (lip * 0.366, w * 1.07),
        (lip * 0.293, w * 1.55),
        (0.0, w * 1.69),
    ];
    // Bottom to top, so it faces the other way: the drum is the inboard face.
    let drum = vec![
        (0.0, -w * 0.16),
        (lip * 0.52, -w * 0.12),
        (lip * 0.80, w * 0.02),
        (lip * 0.78, w * 0.30),
        (0.0, w * 0.30),
    ];
    WheelProfiles {
        tyre,
        disc,
        drum,
        face: w * 0.86,
        half_width: w,
    }
}

/// Pressed-steel discs - the floor.
struct PressedDisc;

impl Wheels for PressedDisc {
    fn profiles(&self, wheel_r: f32) -> WheelProfiles {
        pressed_disc(wheel_r)
    }
}

/// A balloon tyre's outer radius over the blueprint's.
const BALLOON_R: f32 = 1.06;

/// Low-pressure balloons on every Heavy machine: a deeper, rounder section 40 %
/// wider than the pressed disc's, on a smaller rim. The outer radius is the
/// PLAN's ([`Wheels::radius_factor`]), so the axle line stays one radius over
/// the ground by construction; the disc and the drum keep their shape,
/// re-seated on the smaller rim, so the disc still stands proud of this tyre's
/// own lip.
struct Balloon;

impl Wheels for Balloon {
    fn radius_factor(&self) -> f32 {
        BALLOON_R
    }

    fn profiles(&self, wheel_r: f32) -> WheelProfiles {
        let lip = wheel_r * 0.60;
        let w = wheel_r * 0.27;
        let tyre = vec![
            (lip, -w * 0.80),
            (wheel_r * 0.80, -w),
            (wheel_r * 0.94, -w * 0.80),
            (wheel_r, 0.0),
            (wheel_r * 0.94, w * 0.80),
            (wheel_r * 0.80, w),
            (lip, w * 0.80),
        ];
        let base = pressed_disc(wheel_r);
        let rim = lip / (wheel_r * 0.683);
        let deeper = w / (wheel_r * 0.193) * 0.9;
        WheelProfiles {
            tyre,
            disc: base
                .disc
                .iter()
                .map(|&(r, h)| (r * rim, h * deeper))
                .collect(),
            drum: base.drum.iter().map(|&(r, h)| (r * rim, h)).collect(),
            face: w * 0.80,
            half_width: w,
        }
    }

    /// A balloon car's spare is a balloon too - the tyre is what reads, and a
    /// thin one on a fat car would be a spare from another car.
    fn spare(&self, wheel_r: f32) -> WheelProfiles {
        self.profiles(wheel_r)
    }
}

/// Spoke PAIRS a wire wheel carries: each is one straight spine across the
/// wheel, rim to hub to rim, so eight spokes are four nodes. Twelve read finer
/// at play distance but cost eight more nodes a car, which the record guard
/// could not carry on a fully dressed machine (#1367).
const WIRE_PAIRS: usize = 4;
/// How far the hub stands outboard of the rim, in tyre half-widths - the dish
/// a wire wheel has.
const WIRE_DISH: f32 = 0.55;

/// Eight wire spokes to a turned hub - on the regal and sporting themes, and
/// the long low racer.
struct Wire;

impl Wheels for Wire {
    fn profiles(&self, wheel_r: f32) -> WheelProfiles {
        pressed_disc(wheel_r)
    }

    /// The tyre and the drum as built, the spokes, and a turned hub with its
    /// knock-off cap on the spokes' apex. The spokes stand just proud of the
    /// tyre's end cap and dish outboard to the hub; they carry the wheel
    /// centre's identity colour, and on a luminous style they glow. No rotated
    /// node: a spine takes explicit points.
    fn build(&self, kids: &mut Vec<Generator>, plan: &RoadsterPlan, c: &SkiffColours) {
        let base = pressed_disc(plan.wheel_r);
        let lip = plan.wheel_r * 0.683;
        let w = plan.wheel_r * 0.193;
        let spoke = dim(plan.length * 0.0035);
        let rim = lip * 0.97;
        let hub: Vec<(f32, f32)> = vec![
            (0.0, -w * 0.9),
            (lip * 0.24, -w * 0.9),
            (lip * 0.24, w * 0.10),
            (lip * 0.15, w * 0.35),
            (0.0, w * 0.42),
        ];
        for anchor in plan.wheel_anchors() {
            let side = anchor[0].signum();
            let lay = lay(anchor[0]);
            kids.push(turned(&base.tyre, 28, true, c.rubber.clone(), anchor, lay));
            kids.push(turned(
                &base.drum,
                20,
                false,
                c.machinery.clone(),
                anchor,
                lay,
            ));
            let face = anchor[0] + side * (base.face + spoke * 0.6);
            let hub_x = face + side * WIRE_DISH * w;
            for k in 0..WIRE_PAIRS {
                let (s, co) = (PI * k as f32 / WIRE_PAIRS as f32).sin_cos();
                kids.push(line(
                    &[
                        ([face, anchor[1] + rim * s, anchor[2] + rim * co], spoke),
                        ([hub_x, anchor[1], anchor[2]], spoke),
                        ([face, anchor[1] - rim * s, anchor[2] - rim * co], spoke),
                    ],
                    6,
                    c.disc.clone(),
                ));
            }
            kids.push(turned(
                &hub,
                16,
                false,
                c.brightwork.clone(),
                [hub_x - side * w * 0.10, anchor[1], anchor[2]],
                lay,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every style's tyre reaches exactly the radius the plan carries and no
    /// further - so with the axle one plan radius over the ground, the drawn
    /// tyre stands ON the ground whatever the car rolls on (#1367).
    #[test]
    fn every_tyre_is_exactly_the_plans_radius() {
        for rolls in RoadsterWheels::ALL {
            let style = wheels(rolls);
            for r in [0.2f32, 0.34, 0.45] {
                let p = style.profiles(r);
                let widest = p.tyre.iter().map(|&(x, _)| x).fold(0.0f32, f32::max);
                assert!(
                    (widest - r).abs() < 1e-6,
                    "{rolls:?}: the tyre is {widest} across a {r} wheel"
                );
                // And the disc stands proud of the lip, or the tyre's cap
                // swallows it.
                let lip = p.tyre[0].0;
                let disc = p.disc.iter().map(|&(x, _)| x).fold(0.0f32, f32::max);
                assert!(disc > lip, "{rolls:?}: the disc is inside the tyre's lip");
            }
        }
    }
}
