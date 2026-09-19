//! The wagon's running gear: open spoked wheels, and the axles and bolsters
//! the bed stands on.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::pds::avatar::livery::WagonColours;
use crate::pds::generator::Generator;

use super::super::super::common::quat_z;
use super::super::dim;
use super::{WagonPlan, board, compose, line, rotate, solid, turned};

/// The felloe's radial depth over the wheel's radius. The band is bored to
/// what is left, with `hollow` - see the module docs for why that is what
/// makes the wheel open.
const FELLOE: f32 = 0.13;
/// The iron tyre's radial depth over the wheel's radius. The sanitiser caps
/// `hollow` at 0.95, so a tyre drawn as a bored band cannot be thinner than
/// 5 % of the radius: this is as thin as it may be with a little room.
const TYRE: f32 = 0.055;
/// The tread's half-width over the wheel's radius - narrow, the brief's.
const TREAD: f32 = 0.055;

/// How far a nave stands proud of the spoke plane each way, over the wheel's
/// radius, at its plain size.
pub(super) const NAVE_HALF_LENGTH: f32 = 0.30;

/// How a wheel is drawn.
#[derive(Clone, Copy, Debug)]
pub(super) struct WheelStyle {
    /// Spokes: always even, since each swept spoke crosses the hub.
    pub(super) spokes: u32,
    /// An iron tyre band round the felloe, or the bare felloe.
    pub(super) tyre: bool,
    /// The nave's size over its plain one.
    pub(super) nave: f32,
}

/// The wagon wheel: twelve spokes, an iron tyre, a plain nave.
pub(super) const WAGON_WHEEL: WheelStyle = WheelStyle {
    spokes: 12,
    tyre: true,
    nave: 1.0,
};

/// Which way a wheel's turned parts face: outboard on both sides, like the
/// roadster's. `-0.0` counts as the right-hand side, as `+0.0` does.
fn side(x: f32) -> f32 {
    if x < 0.0 { -1.0 } else { 1.0 }
}

/// One open spoked wheel of outer radius `r` centred at `at`, laid across the
/// machine on the side `x_side` says, and then turned by `frame` - identity
/// for a wheel on an axle, a quarter turn for a spare hung on a tail.
///
/// The iron tyre and the felloe are Lathe BANDS bored with `hollow`, so their
/// end caps are annuli and the ground shows between the spokes; the nave is
/// closed at the axis both ends, so it has no cap to trap; each spoke is one
/// Spine rim to rim through the hub, seated half the felloe's depth into it.
pub(super) fn spoked_wheel(
    kids: &mut Vec<Generator>,
    at: [f32; 3],
    x_side: f32,
    frame: [f32; 4],
    r: f32,
    style: WheelStyle,
    c: &WagonColours,
) {
    let lay = compose(frame, quat_z(-side(x_side) * FRAC_PI_2));
    let hw = dim(r * TREAD);
    let r_f = if style.tyre { r * (1.0 - TYRE) } else { r };
    if style.tyre {
        let bore = (r_f * 0.995 / r).min(0.95);
        kids.push(turned(
            &[(r, -hw * 0.92), (r, hw * 0.92)],
            36,
            false,
            &c.iron,
            at,
            lay,
            bore,
            [0.0, 1.0],
        ));
    }
    let bore = (r_f - r * FELLOE) / (r_f * 1.004);
    kids.push(turned(
        &[(r_f * 1.004, -hw), (r_f * 1.004, hw)],
        36,
        false,
        &c.wheel,
        at,
        lay,
        bore,
        [0.0, 1.0],
    ));
    let (hr, hl) = (r * 0.20 * style.nave, r * NAVE_HALF_LENGTH * style.nave);
    let nave = [
        (0.0, -hl),
        (hr * 0.55, -hl),
        (hr * 0.92, -hl * 0.45),
        (hr, 0.0),
        (hr * 0.92, hl * 0.45),
        (hr * 0.55, hl),
        (0.0, hl),
    ];
    kids.push(solid(&nave, 16, true, &c.iron, at, lay));
    let r_in = r_f * (1.0 - FELLOE * 0.5 / (1.0 - TYRE));
    let sr = dim(r * 0.028);
    let pairs = style.spokes / 2;
    let point = |dy: f32, dz: f32| {
        let v = rotate(frame, [0.0, dy, dz]);
        [at[0] + v[0], at[1] + v[1], at[2] + v[2]]
    };
    for i in 0..pairs {
        let a = PI * (i as f32 + 0.5) / pairs as f32;
        let (dy, dz) = (a.cos() * r_in, a.sin() * r_in);
        kids.push(line(
            &[
                (point(dy, dz), sr),
                (point(0.0, 0.0), sr * 1.35),
                (point(-dy, -dz), sr),
            ],
            5,
            &c.wheel,
        ));
    }
}

/// Every wheel of the plan, drawn in `style`.
pub(super) fn wheels(
    kids: &mut Vec<Generator>,
    plan: &WagonPlan,
    style: WheelStyle,
    c: &WagonColours,
) {
    for (at, r) in plan.wheels() {
        spoked_wheel(kids, at, at[0], [0.0, 0.0, 0.0, 1.0], r, style, c);
    }
}

/// An iron axle hub to hub at each axle's own height, and a timber bolster
/// from it up into the floor - so the bed stands ON its running gear whatever
/// the seed's wheels and beltline. `radius` and `bolster` are the axle's
/// radius and the bolster's `[width, depth]`, as fractions of the length.
pub(super) fn axles(
    kids: &mut Vec<Generator>,
    plan: &WagonPlan,
    radius: f32,
    bolster: [f32; 2],
    top: f32,
    c: &WagonColours,
) {
    let l = plan.length;
    let ar = dim(l * radius);
    let half_track = plan.track * 0.5;
    for (z, y) in plan.axle_lines() {
        kids.push(line(
            &[([-half_track, y, z], ar), ([half_track, y, z], ar)],
            8,
            &c.iron,
        ));
        let bot = y - ar * 0.5;
        let h = (top - bot).max(ar * 2.0);
        kids.push(board(
            [plan.half_w() * bolster[0], h, dim(l * bolster[1])],
            &c.timber,
            [0.0, top - h * 0.5, z],
            [0.0, 0.0, 0.0, 1.0],
            l * 0.004,
        ));
    }
}
