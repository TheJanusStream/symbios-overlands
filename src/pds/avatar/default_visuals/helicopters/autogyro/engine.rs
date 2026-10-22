//! Her nose: the biplane's radial in its open cowl ring (#1624 Q-D) - a
//! crankcase, seven cylinders round it and the ring round them in the trim
//! colour - the exhaust stack under it, raked forward, and the two-blade
//! wooden airscrew on a shaft ahead of the ring.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::pds::generator::Generator;
use crate::pds::{Fp, Spin, SpinTerm};

use super::super::super::common::{
    cuboid, cylinder, id_quat, lathe, prim, quat_mul, quat_x, quat_xyzw, quat_y, quat_z, sphere,
    with_cut, with_shape,
};
use super::super::{AutogyroColours, PodPlan, dim};
use super::hub;

/// The radial (prototype frame): its crankcase's station and radius, its
/// cylinders' ring, radius and length - each at girth 1, growing with her -
/// and the cowl ring's two ends, radius at girth 1 and bore.
const CRANK_Z: f32 = 1.40;
const CRANK_R: f32 = 0.085;
const CYLINDERS: u32 = 7;
const CYL_RING: f32 = 0.125;
const CYL_R: f32 = 0.032;
const CYL_LEN: f32 = 0.10;
const COWL_Z: [f32; 2] = [1.30, 1.50];
const COWL_R: f32 = 0.235;
/// The cowl ring's bore, as a share of its radius: narrow enough that the
/// ring bears on her nose at its after lip (#1723), and still wide enough
/// to show the cylinders, whose heads it grazes. It bears on the crown at
/// every girth and on the keel too at the slimmer ones: the ring is centred
/// on the engine, and her nose's centreline rises against it as her girth
/// grows, so at the fullest it rides on the crown alone. Bored 0.86 it stood
/// clear of the nose all round, by 11 mm at the fullest girth, and of every
/// head - held by nothing.
const COWL_HOLLOW: f32 = 0.75;

/// Where the airscrew's hub turns: 3 cm ahead of the cowl ring's lip, so
/// her blades clear it - the biplane's; her hubcap's silhouette, `(radius,
/// height)` base to tip along its axis; and the shaft back into the
/// crankcase (prototype frame).
const PROP_Z: f32 = 1.53;
const HUBCAP: [(f32, f32); 4] = [(0.05, 0.0), (0.05, 0.03), (0.035, 0.06), (0.0, 0.08)];
const SHAFT_R: f32 = 0.025;
const SHAFT_LEN: f32 = 0.06;

/// An airscrew blade's size (chord, length, thickness), how far its middle
/// stands out from the hub, how far its chord narrows toward its tip, and
/// its pitch (degrees) - all prototype frame: a little shorter than the
/// biplane's.
const BLADE: [f32; 3] = [0.065, 0.42, 0.016];
const BLADE_OUT: f32 = 0.23;
const BLADE_TAPER: f32 = 0.40;
const BLADE_PITCH_DEG: f32 = 18.0;

/// The airscrew's spin (#1624 Q-I): idling at the biplane's 540 degrees a
/// second, and 76 more for each metre a second she travels - so it reaches
/// the runtime's ceiling, four turns a second, at 11.8 m/s, a hair under
/// her top speed of 11.9, where the biplane's 55 reaches it at 16.4. Two
/// blades at the ceiling pass eight times a second, the thrum her radial
/// voice is gated at (`fx::radial_drone`), so what is heard at her top
/// speed is what is seen.
pub(crate) const PROP_IDLE_DEG_S: f32 = 540.0;
pub(crate) const PROP_GAIN_DEG_PER_M: f32 = 76.0;

/// The exhaust stack (prototype frame): its station, the station whose
/// belly it hangs under and how far over that belly its middle stands, its
/// rake (degrees about `+X`; positive tips its top forward), length and
/// offset to port.
const EXHAUST_Z: f32 = 1.25;
const EXHAUST_SILL_Z: f32 = 1.28;
const EXHAUST_RISE: f32 = 0.02;
const EXHAUST_RAKE_DEG: f32 = 30.0;
const EXHAUST_LEN: f32 = 0.12;
const EXHAUST_X: f32 = 0.07;

/// The radial: a crankcase at the nose, seven cylinders round it, and the
/// cowl ring round them in the trim colour, open at its middle so the
/// cylinders show.
pub(super) fn radial(kids: &mut Vec<Generator>, plan: &PodPlan, c: &AutogyroColours) {
    let (_, yc, _) = plan.stations().next().expect("the fuselage has stations");
    let g = plan.girth();
    kids.push(prim(
        sphere(plan.len(CRANK_R * g), 3, c.metal.clone()),
        plan.at(0.0, yc, CRANK_Z),
        id_quat(),
    ));
    let ring = CYL_RING * g;
    for k in 0..CYLINDERS {
        // A cylinder stands on its +Y; turned about +Z by its own angle
        // less a quarter turn, it points out from the crankcase.
        let a = FRAC_PI_2 + k as f32 * 2.0 * PI / CYLINDERS as f32;
        kids.push(prim(
            cylinder(
                plan.len(CYL_R * g),
                plan.len(CYL_LEN * g),
                10,
                c.dark.clone(),
            ),
            plan.at(ring * a.cos(), yc + ring * a.sin(), CRANK_Z),
            quat_xyzw(quat_z(a - FRAC_PI_2)),
        ));
    }
    let [aft, fore] = COWL_Z;
    kids.push(prim(
        with_cut(
            cylinder(
                plan.len(COWL_R * g),
                plan.len(fore - aft),
                28,
                c.trim.clone(),
            ),
            [0.0, 1.0],
            [0.0, 1.0],
            COWL_HOLLOW,
        ),
        plan.at(0.0, yc, (aft + fore) * 0.5),
        // A quarter turn about +X lays the cylinder's +Y along her.
        quat_xyzw(quat_x(FRAC_PI_2)),
    ));
}

/// The exhaust stack under her nose behind the cowl ring, raked forward -
/// the biplane's, where her aura leaves her.
pub(super) fn exhaust(kids: &mut Vec<Generator>, plan: &PodPlan, c: &AutogyroColours) {
    kids.push(prim(
        cylinder(
            dim(plan.len(0.022)),
            plan.len(EXHAUST_LEN),
            12,
            c.dark.clone(),
        ),
        plan.at(
            EXHAUST_X,
            plan.sill(EXHAUST_SILL_Z) + EXHAUST_RISE,
            EXHAUST_Z,
        ),
        quat_xyzw(quat_x(EXHAUST_RAKE_DEG.to_radians())),
    ));
}

/// The stack's mouth, drawn level in the prototype frame: its lower end,
/// the stack's own half-length along its raked axis from its centre - where
/// an exhaust aura leaves her (`Autogyro::fx_mount`, which stands it with
/// her).
pub(super) fn exhaust_mouth(plan: &PodPlan) -> (f32, f32, f32) {
    let rake = EXHAUST_RAKE_DEG.to_radians();
    let half = EXHAUST_LEN * 0.5;
    // A turn of `rake` about +X carries the stack's +Y toward +Z by its
    // sine, so its -Y end - the mouth, under her nose - goes aft.
    (
        EXHAUST_X,
        plan.sill(EXHAUST_SILL_Z) + EXHAUST_RISE - half * rake.cos(),
        EXHAUST_Z - half * rake.sin(),
    )
}

/// The airscrew on a hidden hub ahead of the cowl ring: the hubcap, two
/// long wooden blades along `y` and the shaft back into the crankcase,
/// turning about `z` with her speed and idling at rest.
pub(super) fn airscrew(kids: &mut Vec<Generator>, plan: &PodPlan, c: &AutogyroColours) {
    let (_, yc, _) = plan.stations().next().expect("the fuselage has stations");
    let cap: Vec<(f32, f32)> = HUBCAP
        .iter()
        .map(|&(r, h)| (plan.len(r), plan.len(h)))
        .collect();
    // A lathe turns about its +Y, and a cylinder stands on it; a quarter
    // turn about +X lays each along +Z, her nose.
    let along = quat_xyzw(quat_x(FRAC_PI_2));
    let mut parts = vec![prim(
        lathe(&cap, 16, true, c.metal.clone()),
        [0.0; 3],
        along,
    )];
    for side in [-1.0f32, 1.0] {
        // Each blade built pointing +Y and narrowing toward that tip; the
        // lower one turned half round about the shaft, so its tip is down.
        let turn = if side > 0.0 { 0.0 } else { PI };
        let blade = with_shape(
            cuboid(
                [
                    plan.len(BLADE[0]),
                    plan.len(BLADE[1]),
                    dim(plan.len(BLADE[2])),
                ],
                c.wood.clone(),
            ),
            [BLADE_TAPER, 0.0],
            [0.0; 3],
            [0.0; 2],
        );
        parts.push(prim(
            blade,
            [0.0, side * plan.len(BLADE_OUT), 0.0],
            quat_xyzw(quat_mul(quat_z(turn), quat_y(BLADE_PITCH_DEG.to_radians()))),
        ));
    }
    parts.push(prim(
        cylinder(
            dim(plan.len(SHAFT_R)),
            plan.len(SHAFT_LEN),
            10,
            c.metal.clone(),
        ),
        [0.0, 0.0, -plan.len(SHAFT_LEN * 0.5)],
        along,
    ));
    let mut screw = hub(plan, plan.at(0.0, yc, PROP_Z), c);
    screw.children = parts;
    screw.spin = Some(Spin::about(
        [0.0, 0.0, 1.0],
        SpinTerm::Speed {
            idle: Fp(PROP_IDLE_DEG_S),
            gain: Fp(PROP_GAIN_DEG_PER_M),
        },
    ));
    kids.push(screw);
}
