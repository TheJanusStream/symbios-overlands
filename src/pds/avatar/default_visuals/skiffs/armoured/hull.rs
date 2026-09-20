//! The armoured car's hull: three plates stepped one on another, the Wedge
//! glacis at her nose, her lit vision slits, her armoured lamps, the stowage
//! rails across her stern plate and the unit flash on each flank.
//!
//! Every part here is read off the section the plan publishes
//! ([`ArmouredPlan::flank`], [`ArmouredPlan::flank_x`]), so nothing can float
//! off the drawn flank or sink into it at any size.

use crate::pds::avatar::livery::ArmouredColours;
use crate::pds::generator::Generator;

use super::{
    ArmouredPlan, GLACIS_BED, GLACIS_H, GLACIS_RUN, NO_TURN, along_z, board, line, plate, ramp,
    solid, tapered_plate,
};

/// The chamfer a hull plate's vertical edges are cut at, as a fraction of its
/// smaller footprint axis - a bevelled edge on a welded plate, not a rounded
/// one.
const HULL_CHAMFER: f32 = 0.075;

/// The fighting compartment's after end (of the length), its width over the
/// lower hull's and how far its flanks slope in toward its roof.
///
/// Its forward end is the hull's own nose. Three steps of plate under a
/// turret is what the chase camera reads as armour rather than as a van: the
/// unstepped plate box was crisper than the swept hull it replaced and still
/// one long slab from nose to tail.
pub(super) const CASE_Z0: f32 = -0.310;
pub(super) const CASE_W: f32 = 0.82;
pub(super) const CASE_TAPER: [f32; 2] = [0.34, 0.10];

/// The rear deck's height over the hull's crown, and how far its own flanks
/// slope in. Abaft the compartment this is the DRAWN top
/// ([`ArmouredPlan::top_y`]).
pub(super) const CASE_BASE: f32 = 0.40;
pub(super) const DECK_TAPER: f32 = 0.20;

/// The hull as ARMOUR PLATE: a tapered belly tub the machine's whole length,
/// a rear deck over the after body, and the fighting compartment stepped in
/// over the forward two thirds.
///
/// Each is cut to the section the plan already publishes - `taper_bottom` 0.5
/// reproduces the lower half of the hexagon exactly and `taper` 0.5 the upper
/// half - so every mount reads the same numbers whichever form is drawn and
/// only the SHADING changes.
pub(super) fn plates(kids: &mut Vec<Generator>, plan: &ArmouredPlan, c: &ArmouredColours) {
    let (z0, z1) = (plan.tail_z(), plan.nose());
    // The BELLY TUB runs the machine's whole length: its vertical bow plate
    // is what keeps the glacis above it from reading as a plough blade.
    let (zt, tub_run) = ((z0 + plan.nose_z()) * 0.5, plan.nose_z() - z0);
    let (zc, run) = ((z0 + z1) * 0.5, z1 - z0);
    let (hw, crown) = (plan.half_width_at(zc), plan.crown_at(zc));
    kids.push(tapered_plate(
        [hw * 2.0, crown, tub_run],
        &c.hull,
        [0.0, -crown * 0.5, zt],
        NO_TURN,
        HULL_CHAMFER,
        [0.0; 2],
        [0.5, 0.0],
    ));
    kids.push(tapered_plate(
        [hw * 2.0, crown * CASE_BASE, run],
        &c.hull,
        [0.0, crown * CASE_BASE * 0.5, zc],
        NO_TURN,
        HULL_CHAMFER,
        [DECK_TAPER, 0.0],
        [0.0; 2],
    ));
    let (cz0, cz1) = (plan.at(CASE_Z0), plan.nose());
    kids.push(tapered_plate(
        [hw * 2.0 * CASE_W, crown * (1.0 - CASE_BASE), cz1 - cz0],
        &c.hull,
        [0.0, crown * (1.0 + CASE_BASE) * 0.5, (cz0 + cz1) * 0.5],
        NO_TURN,
        HULL_CHAMFER,
        CASE_TAPER,
        [0.0; 2],
    ));
}

/// The glacis wedge's box: `(its centre z, its centre y, its half height, its
/// run in z)`.
///
/// The ramp stands ON the belly tub's roof - the datum - and rises toward the
/// driver's plate, which keeps its share of the face above it. See
/// [`GLACIS_H`] for why it is the lower nose only.
pub(super) fn glacis_box(plan: &ArmouredPlan) -> (f32, f32, f32, f32) {
    let zf = plan.nose();
    let half_h = plan.crown_at(zf) * GLACIS_H * 0.5;
    let run = plan.at(GLACIS_RUN);
    (zf + run * (0.5 - GLACIS_BED), half_h, half_h, run)
}

/// Where the glacis ramp stands at height `y`, `bed` inside it (m).
///
/// The wedge rises from its front-bottom edge to its back-top one, so its
/// face is one straight line in `(y, z)` and everything laid on it - the tow
/// cable, a bolted plate - reads the same number.
pub(super) fn glacis_z(plan: &ArmouredPlan, y: f32, bed: f32) -> f32 {
    let (zc, yc, half_h, run) = glacis_box(plan);
    zc + run * 0.5 - run * (y - yc + half_h) / (2.0 * half_h) - bed
}

/// The sloped front plate: a Wedge whose upright face is bedded into the
/// hull's nose and whose ramp runs down and forward to the bottom of the bow
/// - the facet the chase camera sees whenever she is coming at you.
pub(super) fn glacis(kids: &mut Vec<Generator>, plan: &ArmouredPlan, c: &ArmouredColours) {
    let (zc, yc, half_h, run) = glacis_box(plan);
    let w = plan.half_width_at(plan.nose()) * 2.0 * 0.98;
    kids.push(ramp(
        [w, half_h * 2.0, run],
        &c.hull,
        [0.0, yc, zc],
        NO_TURN,
    ));
}

/// A flank slit's length and its centre station (of the length), and where on
/// the upper flank facet it lies.
const SLIT_RUN: f32 = 0.150;
const SLIT_Z: f32 = 0.150;
const SLIT_T: f32 = 0.58;

/// Her eyes: one lit slit a side lying ON the upper flank facet, and the
/// driver's slit across the plate over the glacis.
///
/// Without them the flank is one flat value at any distance, and the lit
/// driver's slit is the only thing that reads head-on. Both are the
/// tertiary's window light, as every lamp and window band in the fleet is.
pub(super) fn slits(kids: &mut Vec<Generator>, plan: &ArmouredPlan, c: &ArmouredColours) {
    flank_slits(kids, plan, c);
    driver_slit(kids, plan, c);
}

/// One lit slit a side, a thin plate turned to the facet's own tilt - which
/// the plan gives exactly.
fn flank_slits(kids: &mut Vec<Generator>, plan: &ArmouredPlan, c: &ArmouredColours) {
    let z = plan.at(SLIT_Z);
    let (x, y) = plan.flank(z, SLIT_T);
    for s in [-1.0f32, 1.0] {
        kids.push(board(
            [plan.at(SLIT_RUN), plan.at(0.014), plan.at(0.030)],
            &c.glass,
            [s * x * 0.99, y * 0.99, z],
            plan.on_flank(s),
            plan.at(0.006),
        ));
    }
}

/// The driver's slit across the plate over the glacis - a horizontal lit bar
/// on the one upright face the machine shows head-on.
fn driver_slit(kids: &mut Vec<Generator>, plan: &ArmouredPlan, c: &ArmouredColours) {
    let zf = plan.nose();
    let y = plan.crown_at(zf) * 0.40;
    kids.push(board(
        [
            plan.half_width_at(zf) * 1.20,
            plan.at(0.034),
            plan.at(0.014),
        ],
        &c.glass,
        [0.0, y, zf + plan.at(0.004)],
        NO_TURN,
        plan.at(0.006),
    ));
}

/// Two stowage rails across the stern plate - the flat face the chase camera
/// at the usual quarter looks straight at.
///
/// **A plate hull's stern is a PLANE**, not a swept form's ball, so anything
/// laid on it has to reach INSIDE it: the rails are bedded 0.008 of the
/// length in. Three nodes floated at the first plate render for exactly this.
pub(super) fn stern_rack(kids: &mut Vec<Generator>, plan: &ArmouredPlan, c: &ArmouredColours) {
    let z = plan.tail_z();
    let hw = plan.half_width_at(z);
    for yf in [0.62f32, -0.42] {
        let y = plan.crown_at(z) * yf;
        kids.push(line(
            &[
                ([-hw * 0.72, y, z - plan.at(0.008)], plan.at(0.010)),
                ([hw * 0.72, y, z - plan.at(0.008)], plan.at(0.010)),
            ],
            6,
            &c.arm,
        ));
    }
}

/// Two armoured headlamps on the driver's plate, each a turned shell with a
/// lit lens and a guard hoop over it; and two tail lamps on the stern plate,
/// reaching inside it as the rails do.
pub(super) fn lamps(kids: &mut Vec<Generator>, plan: &ArmouredPlan, c: &ArmouredColours) {
    let l = plan.length;
    let zf = plan.nose();
    let k = l / 2.7 * 0.62;
    for s in [-1.0f32, 1.0] {
        let hw = plan.half_width_at(zf);
        let x = s * hw * 0.70;
        let y = plan.crown_at(zf) * 0.78;
        let z = zf - plan.at(0.006);
        let shell = [
            (0.0, -0.070 * k),
            (0.055 * k, -0.055 * k),
            (0.085 * k, 0.0),
            (0.085 * k, 0.030 * k),
            (0.072 * k, 0.038 * k),
        ];
        let lens = [(0.0, 0.0), (0.070 * k, 0.0), (0.070 * k, 0.010 * k)];
        kids.push(solid(&shell, 12, true, &c.hull, [x, y, z], along_z()));
        kids.push(solid(
            &lens,
            10,
            false,
            &c.lamp,
            [x, y, z + 0.038 * k],
            along_z(),
        ));
        kids.push(line(
            &[
                (
                    [x - 0.095 * k, y - 0.060 * k, z + 0.010 * k],
                    plan.at(0.008),
                ),
                (
                    [x - 0.088 * k, y + 0.075 * k, z + 0.030 * k],
                    plan.at(0.008),
                ),
                (
                    [x + 0.088 * k, y + 0.075 * k, z + 0.030 * k],
                    plan.at(0.008),
                ),
                (
                    [x + 0.095 * k, y - 0.060 * k, z + 0.010 * k],
                    plan.at(0.008),
                ),
            ],
            6,
            &c.arm,
        ));
    }
    let zt = plan.tail_z();
    let tail = [
        (0.0, 0.0),
        (plan.at(0.030), 0.0),
        (plan.at(0.030), plan.at(0.012)),
    ];
    for s in [-1.0f32, 1.0] {
        let hw = plan.half_width_at(zt);
        kids.push(solid(
            &tail,
            10,
            false,
            &c.tail_lamp,
            [s * hw * 0.62, plan.crown_at(zt) * 0.52, zt - plan.at(0.006)],
            along_z(),
        ));
    }
}

/// **Identity.** The seed's accent as a painted unit flash on each flank,
/// ahead of the rear arch, lying ON the upper flank facet.
///
/// Not the wheel centres: painted hubs on a military machine read as a toy at
/// 12 m (the buggy's lesson about brights, again), and hers stay machinery
/// steel. The flash and the band round the turret carry the accent instead.
pub(super) fn unit_flash(kids: &mut Vec<Generator>, plan: &ArmouredPlan, c: &ArmouredColours) {
    let z = plan.at(-0.130);
    let (x, y) = plan.flank(z, 0.40);
    for s in [-1.0f32, 1.0] {
        kids.push(plate(
            [plan.at(0.075), plan.at(0.010), plan.at(0.052)],
            &c.flash,
            [s * x * 0.995, y * 0.995, z],
            plan.on_flank(s),
            0.10,
        ));
    }
}
