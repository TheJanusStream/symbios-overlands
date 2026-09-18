//! The coachwork: three swept runs of one plan and the trim that runs along
//! them, the radiator, the wings and the running boards that tie them to the
//! body, the lamps, the bumper and the side exhaust.

use std::f32::consts::FRAC_PI_2;

use crate::pds::generator::Generator;
use crate::pds::texture::SovereignMaterialSettings;

use super::super::super::common::{bevel, cuboid, id_quat, prim, quat_x, quat_xyzw};
use super::super::plan::BodyPlan;
use super::super::{SkiffColours, dim};
use super::{SECTION, TUB_HOLLOW, WING_ARC, WING_R, WING_SCALE_X, line, sweep, turned};

/// How far the radiator's turned filler cap is seated into the shell it stands
/// on, as a fraction of the length (#1367 defect 1).
///
/// It was authored with its base exactly ON the shell's top face - zero
/// overlap - so whether it touched was decided by the connectedness guard's
/// 1e-4 epsilon against the record's 0.1 mm rounding, seed by seed. On seed
/// 134's saved record it touched nothing. A fifth of a percent of the length
/// is contact by construction at every size.
const CAP_SINK: f32 = 0.002;

/// The side the mismatched wing is on: the near side, the one the play view's
/// lit three-quarter looks at, where the side-mount spare also stands.
pub(super) const NEAR_SIDE: f32 = -1.0;

/// Bonnet, tub, scuttle, tail deck - and the three lines that run along them.
pub(super) fn body(kids: &mut Vec<Generator>, plan: &BodyPlan, c: &SkiffColours) {
    let l = plan.length;
    let s = [1.0, SECTION, 1.0];
    let (cockpit_aft, cockpit_fwd) = plan.cockpit_z();
    // Bonnet: a full swept barrel from the cowl forward, tapering to the
    // radiator. No path cut - a bonnet is closed all round.
    kids.push(sweep(
        &plan.run(plan.cowl_z(), plan.nose_z()),
        26,
        s,
        [0.0, 1.0],
        0.0,
        c.paint.clone(),
    ));
    // Tub: the LOWER half-pipe, bored. The cut rim IS the coaming and the bore
    // IS the footwell, so the cockpit is a real hollow rather than a seat
    // standing on a lid - and it costs nothing, where the sloop had to plug
    // her well with a solid because a sweep cannot be holed.
    kids.push(sweep(
        &plan.run(plan.tail_z(), plan.cowl_z()),
        26,
        s,
        [0.5, 1.0],
        TUB_HOLLOW,
        c.paint.clone(),
    ));
    // Scuttle ahead of the cockpit and tail deck behind it: UPPER half-pipes
    // over the SAME stations, so the decks meet the tub's rim flush.
    kids.push(sweep(
        &plan.run(cockpit_fwd, plan.cowl_z()),
        26,
        s,
        [0.0, 0.5],
        0.0,
        c.paint.clone(),
    ));
    kids.push(sweep(
        &plan.run(plan.tail_z(), cockpit_aft),
        26,
        s,
        [0.0, 0.5],
        0.0,
        c.paint.clone(),
    ));
    // Bonnet hinge bead along the bonnet's own crown, from the cowl to the
    // radiator. Its stations still restate the roadster's cowl and nose,
    // which every body here shares; a type with another bonnet reads them.
    let bead: Vec<([f32; 3], f32)> = [0.082f32, 0.200, 0.320, 0.422]
        .iter()
        .map(|&zf| {
            let z = zf * l;
            ([0.0, plan.crown_at(z) + l * 0.0015, z], l * 0.0042)
        })
        .collect();
    kids.push(line(&bead, 6, c.brightwork.clone()));
    // Louvres: five pressed slots down each bonnet flank, seated ON the skin.
    for side in [-1.0f32, 1.0] {
        for k in 0..5 {
            let z = (0.160 + k as f32 * 0.058) * l;
            let y = plan.crown_at(z) * 0.18;
            kids.push(prim(
                cuboid(
                    [dim(l * 0.0045), dim(l * 0.040), dim(l * 0.013)],
                    c.machinery.clone(),
                ),
                [side * plan.side_at(z, y), y, z],
                id_quat(),
            ));
        }
    }
    // The COACHLINE along each flank at the coaming line - the machine's one
    // trim line and the first place the seed's own colour lands (#1365), with
    // its centre ON the skin the way a boot stripe is, because a tube tangent
    // to what it lies on stipples against it. On a period car this line is
    // painted by hand rather than plated, which is why it carries a colour at
    // all instead of being another length of brightwork. Its stations restate
    // a span every body here is longer than.
    for side in [-1.0f32, 1.0] {
        let strip: Vec<([f32; 3], f32)> = [-0.400f32, -0.200, 0.0, 0.200, 0.400]
            .iter()
            .map(|&zf| {
                let z = zf * l;
                ([side * plan.side_at(z, 0.0), 0.0, z], l * 0.0080)
            })
            .collect();
        kids.push(line(&strip, 6, c.coachline.clone()));
    }
}

/// A bevelled pressed shell, a dark matrix and a turned filler cap.
///
/// A [`Bevel`](crate::pds::generator::GeneratorKind::Bevel)'s rounded edges run
/// parallel to its Y extrusion axis, so the size is `[width, depth, height]`
/// and the shell takes a quarter turn about x; authored the other way it lies
/// flat across the nose like a tray (found by render, #1359).
pub(super) fn radiator(kids: &mut Vec<Generator>, plan: &BodyPlan, c: &SkiffColours) {
    let l = plan.length;
    let (nose, z) = (plan.nose_z(), plan.radiator_z());
    let (w, h) = (plan.half_width_at(nose) * 1.95, plan.crown_at(nose) * 2.30);
    // Its centre sits low enough that the shell stands proud UNDER the bonnet
    // line, which is where a radiator of this era is.
    let y = plan.crown_at(nose) - h * 0.5 + l * 0.0075;
    let turn = quat_x(FRAC_PI_2);
    kids.push(prim(
        bevel(
            [dim(w), dim(l * 0.034), dim(h)],
            l * 0.038,
            4,
            c.brightwork.clone(),
        ),
        [0.0, y, z],
        quat_xyzw(turn),
    ));
    kids.push(prim(
        bevel(
            [dim(w * 0.68), dim(l * 0.038), dim(h * 0.66)],
            l * 0.022,
            4,
            c.machinery.clone(),
        ),
        [0.0, y, z + l * 0.0015],
        quat_xyzw(turn),
    ));
    let cap: Vec<(f32, f32)> = [
        (0.0f32, 0.0f32),
        (0.0111, 0.0),
        (0.0111, 0.0074),
        (0.0067, 0.0111),
        (0.0067, 0.0185),
        (0.0, 0.0204),
    ]
    .iter()
    .map(|&(r, hh)| (r * l, hh * l))
    .collect();
    kids.push(turned(
        &cap,
        12,
        false,
        c.brightwork.clone(),
        [0.0, y + h * 0.5 - l * CAP_SINK, z],
        [0.0, 0.0, 0.0, 1.0],
    ));
}

/// A running board, as [`wings`] draws it - what the side-mount spare stands
/// on and the jerrycan is roped to, so neither restates where it is.
pub(super) struct Board {
    /// The board's centre height, and the height of its top face (m).
    pub(super) y: f32,
    pub(super) top: f32,
    /// Its inboard edge - inside the body's own flank at board height - and
    /// its outboard edge, the guard's (m from the centreline).
    pub(super) inner: f32,
    pub(super) outer: f32,
    /// Its forward and after ends (m).
    pub(super) front: f32,
    pub(super) back: f32,
}

/// The running board every guard sweeps into, read off the plan.
pub(super) fn board(plan: &BodyPlan) -> Board {
    let l = plan.length;
    let arc = plan.wheel_r * WING_ARC;
    let y = plan.axle_y() - l * 0.0113 + WING_R * l + l * 0.0023;
    let stations = plan.axle_stations();
    let (front, rear) = (stations[0], stations[stations.len() - 1]);
    Board {
        y,
        top: y + l * 0.007,
        inner: plan.side_at(0.0, y) * 0.78,
        outer: plan.track * 0.5 + WING_R * WING_SCALE_X * l,
        front: front - arc - 0.09 * l,
        back: rear + arc + 0.09 * l,
    }
}

/// One side's guard path: a leading arch, the run along the board, and a
/// trailing arch, swept from the WHEEL LANDMARKS - so a guard is over its own
/// wheel by construction, and a type with three or six wheels gets three or
/// six arches out of this same code rather than out of a slug-string check.
fn guard_path(plan: &BodyPlan, x: f32) -> Vec<([f32; 3], f32)> {
    let l = plan.length;
    let arc = plan.wheel_r * WING_ARC;
    let axle_y = plan.axle_y();
    let board_y = axle_y - l * 0.0113;
    let stations = plan.axle_stations();
    let mut pts: Vec<([f32; 3], f32)> = Vec::with_capacity(stations.len() * 7);
    for (i, &centre) in stations.iter().enumerate() {
        if i > 0 {
            let previous = stations[i - 1];
            pts.push(([x, board_y, previous - arc - 0.10 * l], WING_R * l));
            pts.push(([x, board_y, centre + arc + 0.10 * l], WING_R * l));
        }
        // A leading half-arc on the first guard and a trailing one on the
        // last, so the ends run out past their wheels the way a wing does;
        // the ones between are the arch alone.
        let lead = if i == 0 { 196.0f32 } else { 136.0 };
        let tail = if i + 1 == stations.len() {
            -14.0f32
        } else {
            44.0
        };
        for k in 0..5 {
            let d = lead + (tail - lead) * k as f32 / 4.0;
            let a = d.to_radians();
            pts.push((
                [x, axle_y + arc * a.sin(), centre - arc * a.cos()],
                WING_R * l,
            ));
        }
    }
    pts
}

/// One flattened swept guard.
fn guard(points: &[([f32; 3], f32)], m: SovereignMaterialSettings) -> Generator {
    sweep(points, 10, [WING_SCALE_X, 1.0, 1.0], [0.0, 1.0], 0.0, m)
}

/// One guard a side, and the board that ties it to the body.
///
/// On a worn car (`primer`) the near side's FRONT WING - the leading arch and
/// its valance down to the front of the board - is a separate sweep in grey
/// primer: a replacement panel that never matched (#1367). Wear as a mass,
/// not as texture noise. The split shares the board's first station, so the
/// two pieces meet end to end on it.
pub(super) fn wings(kids: &mut Vec<Generator>, plan: &BodyPlan, c: &SkiffColours, primer: bool) {
    let l = plan.length;
    let b = board(plan);
    let stations = plan.axle_stations();
    for side in [-1.0f32, 1.0] {
        let pts = guard_path(plan, side * plan.track * 0.5);
        if primer && side == NEAR_SIDE {
            // Five arch points and the board's first: the wing a crash takes.
            kids.push(guard(&pts[..6], c.primer.clone()));
            kids.push(guard(&pts[5..], c.guard.clone()));
        } else {
            kids.push(guard(&pts, c.guard.clone()));
        }
        // The running board BRIDGES: its inboard edge is inside the body's own
        // flank at board height and its outboard edge is the guard's. Perched
        // on the guard's waist instead - which is where the prototype first
        // put it - it leaves a quarter of a metre of daylight between guard
        // and coachwork, and that gap was most of what read as "the parts are
        // not connected" (#1364).
        kids.push(prim(
            bevel(
                [
                    dim(b.outer - b.inner),
                    dim(l * 0.014),
                    dim(b.front - b.back),
                ],
                l * 0.007,
                4,
                c.machinery.clone(),
            ),
            [
                side * (b.inner + b.outer) * 0.5,
                b.y,
                (stations[0] + stations[stations.len() - 1]) * 0.5,
            ],
            id_quat(),
        ));
    }
}

/// Turned bullet lamp shells with a lit lens, tail lamps, a bumper and the
/// side exhaust.
pub(super) fn lamps_and_bumper(kids: &mut Vec<Generator>, plan: &BodyPlan, c: &SkiffColours) {
    let l = plan.length;
    let k = l / 2.7;
    let shell: Vec<(f32, f32)> = [
        (0.0f32, -0.130f32),
        (0.045, -0.100),
        (0.085, -0.040),
        (0.098, 0.020),
        (0.098, 0.050),
        (0.088, 0.058),
    ]
    .iter()
    .map(|&(r, h)| (r * k, h * k))
    .collect();
    let turn = quat_x(FRAC_PI_2);
    let mounts: Vec<[f32; 3]> = [-1.0f32, 1.0]
        .iter()
        .map(|&s| lamp_mount(plan, s))
        .collect();
    // The crossbar runs lamp to lamp THROUGH the bonnet, so each shell is
    // carried rather than stuck on the side of the panel.
    kids.push(line(
        &[(mounts[0], l * 0.0048), (mounts[1], l * 0.0048)],
        6,
        c.brightwork.clone(),
    ));
    for (i, &m) in mounts.iter().enumerate() {
        let side = if i == 0 { -1.0f32 } else { 1.0 };
        kids.push(turned(&shell, 20, true, c.brightwork.clone(), m, turn));
        kids.push(turned(
            &[(0.0, 0.0), (0.084 * k, 0.0), (0.084 * k, 0.010 * k)],
            18,
            false,
            c.lamp.clone(),
            [m[0], m[1], m[2] + 0.058 * k],
            turn,
        ));
        // Tail lamp on the quarter, seated on the body's own flank at the
        // station the BODY names (#1367 defect 3): restated as a fraction of
        // the length, it stood behind a bobtail's back in open air.
        let tz = plan.tail_lamps_z();
        let ty = plan.crown_at(tz) * 0.10;
        kids.push(turned(
            &[(0.0, 0.0), (0.034 * k, 0.0), (0.034 * k, 0.012 * k)],
            12,
            false,
            c.tail_lamp.clone(),
            [side * plan.side_at(tz, ty) * 1.02, ty, tz - l * 0.004],
            quat_x(-FRAC_PI_2),
        ));
    }
    // Bumper: a bowed bar on two short irons, ahead of the radiator and INSIDE
    // the guard line - a bar as wide as the track reads as a chrome moustache
    // at 109 px/m, whatever it is in life.
    let (bz, by) = (plan.bumper_z(), plan.sill_at(0.0) * 0.50);
    let bx = plan.half_width_at(plan.nose_z()) * 1.30;
    kids.push(line(
        &[
            ([-bx, by, bz - l * 0.022], l * 0.0090),
            ([-bx * 0.45, by, bz], l * 0.0090),
            ([bx * 0.45, by, bz], l * 0.0090),
            ([bx, by, bz - l * 0.022], l * 0.0090),
        ],
        8,
        c.brightwork.clone(),
    ));
    for side in [-1.0f32, 1.0] {
        kids.push(line(
            &[
                ([side * bx * 0.42, by, bz - l * 0.0015], l * 0.0055),
                (
                    [
                        side * bx * 0.42,
                        by + l * 0.030,
                        plan.radiator_z() - l * 0.010,
                    ],
                    l * 0.0055,
                ),
            ],
            6,
            c.brightwork.clone(),
        ));
    }
    // Side exhaust, out of the bonnet flank and along the sill.
    let pipe: Vec<([f32; 3], f32)> = exhaust_path(plan)
        .iter()
        .map(|&(p, _)| (p, l * 0.0105))
        .collect();
    kids.push(line(&pipe, 10, c.brightwork.clone()));
}

/// The headlamp station on `side` - standing off the bonnet's shoulder, read
/// off the flank so the shell is always against the panel.
fn lamp_mount(plan: &BodyPlan, side: f32) -> [f32; 3] {
    let z = 0.400 * plan.length;
    let y = plan.crown_at(z) * 0.30;
    [side * plan.side_at(z, y) * 1.12, y + plan.length * 0.017, z]
}

/// The side exhaust's stations, tail last, read off the body's OWN flank at
/// every one.
///
/// A fixed height would leave the pipe hanging in air aft, where the section
/// is a third of what it is amidships - the same mistake as a mount on a
/// guessed fraction, one part further out.
pub(super) fn exhaust_path(plan: &BodyPlan) -> Vec<([f32; 3], f32)> {
    let l = plan.length;
    [0.300f32, 0.180, 0.0, -0.250, -0.392]
        .iter()
        .enumerate()
        .map(|(i, &zf)| {
            let z = zf * l;
            let y = plan.sill_at(z) * if i == 0 { 0.30 } else { 0.66 };
            ([plan.side_at(z, y) + l * 0.006, y, z], l * 0.0105)
        })
        .collect()
}
