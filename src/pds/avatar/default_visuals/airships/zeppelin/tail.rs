//! The Zeppelin's cruciform tail: four fins standing on the hull's axis, each
//! a flattened sweep root to tip with its rudder or elevator hinged behind
//! it in the scheme's trim, and the nav lights on the two horizontal fins'
//! tips.
//!
//! A fin's root station is ON THE AXIS, deep inside the hull: the tail tapers
//! under the fin's whole chord, so a root laid on the skin would stand clear
//! of it aft, and four roots on one axis meet inside the tail cone, as a real
//! ship's did. Each fin is laid out where it stands - the dorsal and ventral
//! flattened across `X`, the two horizontal ones across `Y` - rather than
//! one fin turned into four places, because a sweep squashed by its node's
//! scale and then turned is not a similarity, which the connectivity guard
//! asserts.

use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;

use super::super::super::common::{id_quat, prim, sphere};
use super::super::{AirshipColours, HullPlan, flat_sweep};

/// The fin's leading edge leaves the axis here, reaches its outer edge here,
/// and its hinge line stands here; the rudder's trailing edge (shares of the
/// hull's length).
pub(super) const FIN_LE: f32 = 0.745;
const FIN_TIP_LE: f32 = 0.865;
pub(super) const FIN_TE: f32 = 0.962;
pub(super) const RUDDER_TE: f32 = 1.006;
/// The tip station's distance from the axis, in the hull's greatest radius.
pub(super) const FIN_REACH: f32 = 0.95;
/// A fin's thickness over its chord.
const FIN_THICK: f32 = 0.09;
/// A rudder's or elevator's own thickness, how far it reaches into the
/// fin ahead of the hinge and how far short of the fin's tip station it
/// stops (m, prototype frame).
const RUDDER_THICK: f32 = 0.016;
const RUDDER_LAP: f32 = 0.004;
/// The nav lights: their radius (m, prototype frame); each sits half sunk in
/// its fin's tip cap.
pub(super) const LAMP_R: f32 = 0.015;
const FIN_RES: u32 = 20;
const RUDDER_RES: u32 = 16;

/// The four directions a fin stands in, `(x, y)` off the axis: dorsal,
/// starboard (`-X`, her right), ventral and port (`+X`, her left) - the
/// prototype's order.
pub(super) const FINS: [[f32; 2]; 4] = [[0.0, 1.0], [-1.0, 0.0], [0.0, -1.0], [1.0, 0.0]];

/// The fin's root and tip stations, `(span, z, half-chord)` (drawn frame):
/// the root on the axis, the tip at [`FIN_REACH`].
pub(super) fn fin_stations(plan: &HullPlan) -> [(f32, f32, f32); 2] {
    let reach = FIN_REACH * plan.girth();
    let (zl, ztl, zt) = (plan.z(FIN_LE), plan.z(FIN_TIP_LE), plan.z(FIN_TE));
    [
        (0.0, (zl + zt) * 0.5, (zl - zt) * 0.5),
        (reach, (ztl + zt) * 0.5, (ztl - zt) * 0.5),
    ]
}

/// A sweep along `dir` through `(span, z, radius)` stations, squashed across
/// the fin to `thick` of its radius.
fn blade(
    stations: &[(f32, f32, f32)],
    dir: [f32; 2],
    thick: f32,
    res: u32,
    material: &SovereignMaterialSettings,
) -> Generator {
    let points: Vec<([f32; 3], f32)> = stations
        .iter()
        .map(|&(s, z, r)| ([dir[0] * s, dir[1] * s, z], r))
        .collect();
    // Flattened across the axis the fin does NOT run along.
    let axis = if dir[0] == 0.0 { 0 } else { 1 };
    flat_sweep(&points, res, 2, axis, thick, material, None)
}

/// Where a fin's tip-cap lamp sits (drawn frame): on the tip station's
/// centre, pushed half its radius out along the fin's spine so it stands
/// half sunk in the cap.
pub(super) fn lamp_at(plan: &HullPlan, dir: [f32; 2]) -> [f32; 3] {
    let [(s0, z0, _), (s1, z1, _)] = fin_stations(plan);
    let (ds, dz) = (s1 - s0, z1 - z0);
    let len = (ds * ds + dz * dz).sqrt();
    let out = plan.len(LAMP_R) * 0.5;
    let s = s1 + ds / len * out;
    [dir[0] * s, dir[1] * s, z1 + dz / len * out]
}

/// The four fins, their rudders and elevators, and the nav lights on the
/// horizontal pair: red to port (her left, `+X`), green to starboard.
pub(super) fn tail(kids: &mut Vec<Generator>, plan: &HullPlan, c: &AirshipColours) {
    let fin = fin_stations(plan);
    let reach = fin[1].0;
    let (zt, zr) = (plan.z(FIN_TE), plan.z(RUDDER_TE));
    let lap = plan.len(RUDDER_LAP);
    let (rc, rh) = ((zt + zr) * 0.5 + lap, (zt - zr) * 0.5 + lap);
    let rudder = [(0.0, rc, rh), (reach - lap, rc, rh)];
    let rudder_thick = plan.len(RUDDER_THICK) / (2.0 * rh);
    for dir in FINS {
        kids.push(blade(&fin, dir, FIN_THICK, FIN_RES, &c.fins));
        kids.push(blade(&rudder, dir, rudder_thick, RUDDER_RES, &c.trim));
        let lamp = match dir {
            [x, _] if x > 0.0 => Some(&c.port),
            [x, _] if x < 0.0 => Some(&c.starboard),
            _ => None,
        };
        if let Some(lamp) = lamp {
            kids.push(prim(
                sphere(plan.len(LAMP_R), 3, lamp.clone()),
                lamp_at(plan, dir),
                id_quat(),
            ));
        }
    }
}
