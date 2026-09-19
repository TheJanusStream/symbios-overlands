//! The cyclecar's ladder: masses that read at play distance, never trinkets
//! (#1359 F6), and wear where the chase camera looks. Cumulatively:
//!
//! - **Adorned**: a dorsal TAIL FIN drawn up off the pod's tail crown - the
//!   streamliner's fin, and the first thing the chase camera sees;
//! - **Ornate**: CYCLE WINGS as well, a flattened arc over each front tyre
//!   and a stay down to its hub - the Morgan's;
//! - **Battered**: a PRIMER PATCH on the tail's top.
//!
//! A worn cyclecar's near-front rim in bare steel is drawn with the wheels.

use std::f32::consts::PI;

use crate::pds::avatar::livery::CyclecarColours;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{OrnatenessTier, WearTier};

use super::{CyclecarPlan, line, sweep};

/// One mass: it draws itself onto the kids.
type Mass = fn(&mut Vec<Generator>, &CyclecarPlan, &CyclecarColours);

/// What an ornateness tier adds, cumulatively.
fn ladder(o: OrnatenessTier) -> &'static [Mass] {
    match o {
        OrnatenessTier::Plain => &[],
        OrnatenessTier::Adorned => &[tail_fin],
        OrnatenessTier::Ornate => &[tail_fin, cycle_wings],
    }
}

/// What a wear tier adds - the wear the wheels do not draw.
fn wear(w: WearTier) -> &'static [Mass] {
    match w {
        WearTier::Pristine | WearTier::Worn => &[],
        WearTier::Battered => &[patch],
    }
}

/// The tier's masses, then the wear's.
pub(super) fn dress(
    kids: &mut Vec<Generator>,
    plan: &CyclecarPlan,
    c: &CyclecarColours,
    o: OrnatenessTier,
    w: WearTier,
) {
    for mass in ladder(o).iter().chain(wear(w)) {
        mass(kids, plan, c);
    }
}

/// The fin's stations, `(z, rise over the crown, radius)` as fractions of the
/// length, from its root over the tail forward to its tip over the tail's
/// end.
const FIN: [(f32, f32, f32); 5] = [
    (-0.160, -0.010, 0.030),
    (-0.260, 0.020, 0.036),
    (-0.360, 0.060, 0.034),
    (-0.430, 0.090, 0.022),
    (-0.460, 0.098, 0.012),
];

/// Adorned: a dorsal fin drawn up off the pod's tail crown and flattened
/// athwartships, facing the chase camera.
fn tail_fin(kids: &mut Vec<Generator>, plan: &CyclecarPlan, c: &CyclecarColours) {
    let pts: Vec<([f32; 3], f32)> = FIN
        .iter()
        .map(|&(zf, rise, rad)| {
            let z = plan.at(zf);
            (
                [
                    0.0,
                    plan.crown_at(z) + plan.at(rise) - plan.at(rad) * 0.4,
                    z,
                ],
                plan.at(rad),
            )
        })
        .collect();
    kids.push(sweep(
        &pts,
        10,
        [0.22, 1.0, 1.0],
        [0.0, 1.0],
        0.0,
        c.body.clone(),
    ));
}

/// The cycle wing's arc radius over the tyre's, and the angles its five
/// stations stand at round the hub (degrees, from behind the tyre over its
/// crown to ahead of it).
const WING_ARC: f32 = 1.14;
const WING_AT: [f32; 5] = [165.0, 128.0, 90.0, 52.0, 25.0];

/// Ornate: a cycle wing over each front tyre - a flattened arc hugging the
/// tyre's crown from behind it round to ahead of it - and a stay from its
/// middle down to the hub, where the stub axle ends.
fn cycle_wings(kids: &mut Vec<Generator>, plan: &CyclecarPlan, c: &CyclecarColours) {
    let l = plan.length;
    let a = plan.axle(true);
    let arc_r = a.r * WING_ARC;
    // The tube never thinner than 0.42 of the tyre's half-width: at 0.012 of
    // the length alone the node scale reached 4.02 on a narrow seed, over
    // rule 8's 4.0.
    let tube = (0.012 * l).max(a.w * 0.42);
    let sx = a.w * 1.30 / tube;
    for s in [-1.0f32, 1.0] {
        let x = s * plan.track * 0.5;
        let arc: Vec<([f32; 3], f32)> = WING_AT
            .iter()
            .map(|&deg| {
                let t = deg * PI / 180.0;
                ([x, a.y + arc_r * t.sin(), a.z - arc_r * t.cos()], tube)
            })
            .collect();
        kids.push(sweep(
            &arc,
            8,
            [sx, 1.0, 1.0],
            [0.0, 1.0],
            0.0,
            c.body.clone(),
        ));
        let hub = [s * (plan.track * 0.5 - 0.55 * a.w), a.y, a.z];
        let top = arc[2].0;
        kids.push(line(
            &[([hub[0], top[1], top[2]], 0.008 * l), (hub, 0.008 * l)],
            6,
            &c.arm,
        ));
    }
}

/// Battered: a panel patched in primer on the tail's top, where the chase
/// camera looks - a sector of the pod over a stretch of the tail, 3 % proud
/// (a hair proud came out ragged, as the window band did).
fn patch(kids: &mut Vec<Generator>, plan: &CyclecarPlan, c: &CyclecarColours) {
    kids.push(sweep(
        &plan.over_pod(plan.at(-0.400), plan.at(-0.300), 1.030),
        10,
        plan.scale(),
        [0.28, 0.44],
        0.0,
        c.primer.clone(),
    ));
}
