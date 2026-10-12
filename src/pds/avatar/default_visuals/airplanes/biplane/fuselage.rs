//! The fuselage: the one sweep and the skins laid on it - the cheat line,
//! and the two open cockpits, each a lit well with a leather lip at its
//! back and a windscreen ahead - the radial in its open cowl ring, the
//! exhaust stack behind the ring and the airscrew ahead of it.
//!
//! The skins are the family's ([`skin`]): second sweeps over the fuselage's
//! own stations, trimmed fore and aft by arc length and round the section
//! by angle (`0.25` is the crown).

use std::f32::consts::{FRAC_PI_2, PI};

use crate::pds::generator::Generator;
use crate::pds::sanitize::limits::MAX_SPIN_RATE_DEG;
use crate::pds::{Fp, Spin, SpinTerm};

use super::super::super::common::{
    cuboid, cylinder, id_quat, lathe, prim, quat_mul, quat_x, quat_xyzw, quat_y, quat_z, sphere,
    with_cut, with_shape,
};
use super::super::{BiplaneColours, FUSE_RES, FusePlan, MIN_DIM, dim, over_fuselage, skin};
use super::hub;

/// The two cockpits' runs, front then rear (prototype frame).
const COCKPITS: [(f32, f32); 2] = [(0.50, 0.78), (0.14, 0.42)];

/// A windscreen's width and height, how far ahead of its cockpit it stands
/// and how far it leans back (degrees) - its bottom edge in the crown
/// (prototype frame).
const WIND_W: f32 = 0.17;
const WIND_H: f32 = 0.06;
const WIND_AHEAD: f32 = 0.03;
const WIND_TILT_DEG: f32 = 35.0;

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
pub(super) const COWL_R: f32 = 0.235;
const COWL_HOLLOW: f32 = 0.86;

/// Where the airscrew's hub turns: 3 cm ahead of the cowl ring's lip, so
/// her blades clear it by a centimetre (port change 2 in the module
/// header); her hubcap's silhouette, `(radius, height)` base to tip along
/// its axis; and the shaft back into the crankcase (prototype frame).
pub(super) const PROP_Z: f32 = 1.53;
pub(super) const HUBCAP_LEN: f32 = 0.08;
const HUBCAP: [(f32, f32); 4] = [(0.05, 0.0), (0.05, 0.03), (0.035, 0.06), (0.0, HUBCAP_LEN)];
const SHAFT_R: f32 = 0.025;
const SHAFT_LEN: f32 = 0.06;

/// An airscrew blade's size (chord, length, thickness), how far its middle
/// stands out from the hub, how far its chord narrows toward its tip, and
/// its pitch (degrees) - all prototype frame.
const BLADE: [f32; 3] = [0.065, 0.44, 0.016];
const BLADE_OUT: f32 = 0.24;
const BLADE_TAPER: f32 = 0.40;
const BLADE_PITCH_DEG: f32 = 18.0;

/// The airscrew's spin - the high-wing's, as the owner agreed (#1623 Q-H):
/// idling at 540 degrees a second and 55 more for each metre a second she
/// travels, which reaches the runtime's ceiling, four turns a second, at
/// 16.4 m/s - under her level-flight speed of 17 - and holds there.
pub(crate) const PROP_IDLE_DEG_S: f32 = 540.0;
pub(crate) const PROP_GAIN_DEG_PER_M: f32 = 55.0;

/// The airscrew's blades.
const PROP_BLADES: f32 = 2.0;

/// How many times a second a blade passes a point flying level, where her
/// airscrew turns at the ceiling: what her radial voice's thrum is gated at
/// (`fx::radial_drone`), so what is heard flying level is what is seen.
/// Slower it passes less often - seven times a second at the 13.5 m/s the
/// agent's pilot cruises her at, half again her stall - and the voice, a
/// one-second loop, cannot follow her speed.
pub(crate) const BLADE_PASS_HZ: f32 = PROP_BLADES * MAX_SPIN_RATE_DEG / 360.0;

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

/// The fuselage: one sweep, crankcase to tail post.
pub(super) fn fuselage(kids: &mut Vec<Generator>, plan: &FusePlan, c: &BiplaneColours) {
    kids.push(over_fuselage(plan, 0.0, FUSE_RES, c.body.clone()));
}

/// The cheat line: a stripe down each flank, firewall to tail - the seed's
/// accent, the fleet's identity slot.
pub(super) fn skins(kids: &mut Vec<Generator>, plan: &FusePlan, c: &BiplaneColours) {
    skin(
        kids,
        plan,
        (-1.25, 1.10),
        &[Some((0.035, 0.075)), Some((0.425, 0.465))],
        &c.stripe,
        0.003,
    );
}

/// The two open cockpits in tandem (#1623 Q-A), front then rear: each a
/// well in her decking lit with the fleet's window light - the instruments'
/// glow washing the cockpit - a small windscreen ahead of it, raked back,
/// and a leather lip across its back edge.
pub(super) fn cockpits(kids: &mut Vec<Generator>, plan: &FusePlan, c: &BiplaneColours) {
    for (z0, z1) in COCKPITS {
        skin(kids, plan, (z0, z1), &[Some((0.17, 0.33))], &c.lamp, 0.004);
        let z = z1 + WIND_AHEAD;
        kids.push(prim(
            cuboid(
                [plan.len(WIND_W), plan.len(WIND_H), dim(plan.len(MIN_DIM))],
                c.glass.clone(),
            ),
            plan.at(0.0, plan.crown(z) + WIND_H * 0.4, z),
            quat_xyzw(quat_x(-WIND_TILT_DEG.to_radians())),
        ));
        skin(
            kids,
            plan,
            (z0 - 0.02, z0),
            &[Some((0.16, 0.34))],
            &c.leather,
            0.010,
        );
    }
}

/// The radial (#1623 Q-C): a crankcase at the nose, seven cylinders round
/// it, and the cowl ring round them in the trim colour, open at its middle
/// so the cylinders show.
pub(super) fn radial(kids: &mut Vec<Generator>, plan: &FusePlan, c: &BiplaneColours) {
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
/// the high-wing's, where her aura leaves her, ahead of her gear's front
/// legs.
pub(super) fn exhaust(kids: &mut Vec<Generator>, plan: &FusePlan, c: &BiplaneColours) {
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

/// The stack's mouth, in the prototype frame: its lower end, the stack's
/// own half-length along its raked axis from its centre - where an exhaust
/// aura leaves her (`Biplane::fx_mount`).
pub(super) fn exhaust_mouth(plan: &FusePlan) -> (f32, f32, f32) {
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
pub(super) fn airscrew(kids: &mut Vec<Generator>, plan: &FusePlan, c: &BiplaneColours) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::avatar::default_visuals::airplanes::AirplaneCraft;

    /// Her voice is gated at what her airscrew does flying level (#1623
    /// Q-H): two blades at the ceiling a speed term's rate is held to -
    /// which her airscrew, the high-wing's, reaches at or under the speed
    /// her own handling flies level at.
    #[test]
    fn the_blade_pass_is_her_airscrew_at_the_ceiling_flying_level() {
        assert_eq!(BLADE_PASS_HZ, 8.0);
        let level = super::super::Biplane
            .feel()
            .handling
            .expect("she flies her own handling")
            .cruise;
        let reached = (MAX_SPIN_RATE_DEG - PROP_IDLE_DEG_S) / PROP_GAIN_DEG_PER_M;
        assert!(
            reached <= level,
            "her airscrew reaches the ceiling at {reached} m/s, past her level-flight speed of {level}"
        );
    }
}
