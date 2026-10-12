//! The fuselage: the one sweep and what is laid on it - the cheat line down
//! each flank, the windscreen wrapped round the nose's top and the
//! cockpit's side windows in dark glass, and the cabin's row of lit windows
//! down each side.
//!
//! The skins are the family's ([`skin`]): second sweeps over the fuselage's
//! own stations, trimmed fore and aft by arc length and round the section
//! by angle (`0` is `+X`, `0.25` the crown, `0.5` is `-X`) - each set of
//! bands mirror-symmetric, so the same whichever way round a sweep's frame
//! turns. The cabin's windows are panes set into the flank, not skins: a
//! row of them as skins would be a sweep a window.

use crate::pds::generator::Generator;

use super::super::super::common::{cuboid, id_quat, prim};
use super::super::{FUSE_RES, FusePlan, TwinColours, over_fuselage, skin};

/// The cabin's windows (prototype frame): how many a side, the first's
/// station and the step between them aft (each riding the cabin's
/// stretch), how far over the centreline they stand, each pane's depth,
/// height and length, and how far its middle stands inside the flank: its
/// outer face 3 mm proud of it, its inner face 2.7 cm in.
const WINDOWS: usize = 5;
const WINDOW_Z: f32 = 0.62;
const WINDOW_STEP: f32 = 0.15;
const WINDOW_RISE: f32 = 0.06;
const WINDOW: [f32; 3] = [0.030, 0.065, 0.085];
const WINDOW_IN: f32 = 0.012;

/// The fuselage: one sweep, nose tip to tail post.
pub(super) fn fuselage(kids: &mut Vec<Generator>, plan: &FusePlan, c: &TwinColours) {
    kids.push(over_fuselage(plan, 0.0, FUSE_RES, c.body.clone()));
}

/// Every skin, in the prototype's order: the cheat line under the windows,
/// nose to tail - the seed's accent, the fleet's identity slot - then the
/// windscreen wrapped round the nose's top where it falls to the tip, and
/// the cockpit's side windows behind it.
pub(super) fn skins(kids: &mut Vec<Generator>, plan: &FusePlan, c: &TwinColours) {
    skin(
        kids,
        plan,
        (-1.30, 1.30),
        &[Some((0.965, 0.985)), Some((0.515, 0.535))],
        &c.stripe,
        0.003,
    );
    skin(
        kids,
        plan,
        (0.98, 1.17),
        &[Some((0.09, 0.41))],
        &c.glass,
        0.005,
    );
    skin(
        kids,
        plan,
        (0.80, 0.98),
        &[Some((0.04, 0.13)), Some((0.37, 0.46))],
        &c.glass,
        0.005,
    );
}

/// The cabin's windows (#1625 Q-B): a row of lit panes down each flank, a
/// little over the centreline, each set into the flank where it stands -
/// the fleet's window light, held off her body.
pub(super) fn windows(kids: &mut Vec<Generator>, plan: &FusePlan, c: &TwinColours) {
    let size = WINDOW.map(|v| plan.len(v));
    for k in 0..WINDOWS {
        let z = WINDOW_Z - k as f32 * WINDOW_STEP;
        let y = plan.fuse_at(z).0 + WINDOW_RISE;
        for side in [-1.0f32, 1.0] {
            let x = side * (plan.flank(z, y) - WINDOW_IN);
            kids.push(prim(
                cuboid(size, c.lamp.clone()),
                plan.at(x, y, z),
                id_quat(),
            ));
        }
    }
}
