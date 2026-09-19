//! The neon skimmer (#1372): the runabout of the five neon themes. The same
//! planing hull on a finer entry, painted in the scheme, with two turned
//! thruster pods bedded into her transom quarters - the legacy stack-thruster
//! read - whose nozzles glow in the seed's accent, and a lit rub rail. Her
//! glow is GEOMETRY, on every skimmer: the particle aura stays the boat
//! family's Wake, because a thruster plume fired downward is an airship's.

use std::f32::consts::FRAC_PI_2;

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::OrnatenessTier;

use super::super::super::common::quat_x;
use super::super::RunaboutColours;
use super::super::profile::{FinlessForm, HullProfile};
use super::dressing::{Hatch, wear_ladder};
use super::hull::{
    CHINE, DECK_CROWN, HULL_HOLLOW, UPRIGHT, bench, bench_half_width, cockpit_sole, deck_run, line,
    panel, skin, sweep, turned, underbody, windscreen,
};
use super::{FALLING, Launch, LaunchForm};

/// The skimmer's plan form: the launch's wide transom, a longer finer entry.
const PLAN: &[(f32, f32)] = &[
    (-0.500, 0.96),
    (-0.380, 1.00),
    (-0.200, 1.00),
    (-0.040, 0.97),
    (0.100, 0.88),
    (0.220, 0.72),
    (0.320, 0.54),
    (0.400, 0.36),
    (0.460, 0.20),
    (0.500, 0.05),
];

pub(super) struct Skimmer;

const FORM: LaunchForm = LaunchForm {
    plan: PLAN,
    planing: FinlessForm {
        beam: 1.20,
        freeboard: 0.56,
        bow_rise: 0.90,
        stern_rise: 0.0,
        section: 0.58,
        allowance: 0.034,
        sheer: FALLING,
    },
    screen: 0.040,
    cockpit_aft: -0.230,
};

/// A pod's length and radius as fractions of the overall length.
const POD_LEN: f32 = 0.25;
const POD_R: f32 = 0.036;

/// Where a pod sits: its centre and its half-length.
struct Pod {
    at: [f32; 3],
    half: f32,
}

/// A turned thruster nacelle on `side`, bedded into the topsides at the
/// transom quarter and standing aft past the transom, with a lit nozzle puck
/// let half into its after end.
fn pod(kids: &mut Vec<Generator>, hull: &HullProfile, c: &RunaboutColours, side: f32) -> Pod {
    let l = hull.loa;
    let (length, r) = (l * POD_LEN, l * POD_R);
    let zc = hull.transom_z() + length * 0.20;
    let hb = hull.half_beam_at(zc);
    let y = hull.sheer_z(zc) - hb * hull.section * CHINE * 0.55;
    let x = side * (hb * (1.0 - 0.55 * (1.0 - CHINE)) - r * 0.15);
    // The lathe's +y is laid to run AFT.
    let lay = quat_x(-FRAC_PI_2);
    let h = length * 0.5;
    kids.push(turned(
        &[
            (r * 0.30, -h),
            (r * 0.80, -h * 0.70),
            (r, -h * 0.30),
            (r, h * 0.55),
            (r * 0.84, h),
        ],
        16,
        true,
        &c.pod,
        [x, y, zc],
        lay,
        0.0,
    ));
    kids.push(turned(
        &[(r * 0.70, h - l * 0.006), (r * 0.70, h + l * 0.004)],
        16,
        false,
        &c.glow,
        [x, y, zc],
        lay,
        0.0,
    ));
    Pod {
        at: [x, y, zc],
        half: h,
    }
}

impl Launch for Skimmer {
    fn form(&self) -> &'static LaunchForm {
        &FORM
    }

    fn build(
        &self,
        kids: &mut Vec<Generator>,
        hull: &HullProfile,
        c: &RunaboutColours,
        ctx: &PartCtx,
    ) {
        let l = hull.loa;
        let cockpit = FORM.cockpit(hull);
        let (za, zf) = cockpit;
        skin(kids, hull, c, 0.0, HULL_HOLLOW);
        underbody(kids, hull, c, 0.0, false);
        kids.push(deck_run(hull, zf, hull.stem_z() - l * 0.004, &c.deck, 0.0));
        kids.push(deck_run(hull, hull.transom_z(), za, &c.deck, 0.0));
        let sole_y = cockpit_sole(kids, hull, c, cockpit);
        let half = bench_half_width(hull, cockpit, sole_y);
        bench(
            kids,
            hull,
            c,
            zf - l * 0.080,
            l * 0.070,
            sole_y,
            half,
            l * 0.045,
            l * 0.080,
            0.45,
        );
        windscreen(
            kids,
            hull,
            c,
            zf + l * 0.005,
            l * 0.055,
            l * 0.060,
            0.90,
            false,
        );
        let pods = [pod(kids, hull, c, -1.0), pod(kids, hull, c, 1.0)];
        // An engine deck over the pods' roots: a low hatch aft.
        let hz = (hull.transom_z() + za) * 0.5;
        let hatch_len = (za - hull.transom_z()) * 0.62;
        kids.push(panel(
            [hull.half_beam_at(hz) * 1.2, l * 0.022, hatch_len],
            &c.hatch,
            [
                0.0,
                hull.sheer_z(hz) + hull.half_beam_at(hz) * DECK_CROWN * 0.6 + l * 0.004,
                hz,
            ],
            UPRIGHT,
            l * 0.03,
        ));
        let r = l * POD_R;
        if ctx.ornateness != OrnatenessTier::Plain {
            // A swept tail fin on each pod.
            for p in &pods {
                let [x, y, zc] = p.at;
                let fin = [
                    ([x, y + r * 0.5, zc + p.half * 0.05], l * 0.024),
                    ([x, y + r + l * 0.050, zc + p.half * 0.80], l * 0.012),
                ];
                kids.push(sweep(&fin, 8, [0.30, 1.0, 1.0], [0.0, 1.0], &c.pod, 0.0));
            }
        }
        if ctx.ornateness == OrnatenessTier::Ornate {
            // An arch over the cockpit's after end, with a lit bar across it.
            let z = za + l * 0.02;
            let hb = hull.half_beam_at(z) * 0.96;
            let y = hull.sheer_z(z);
            let h = l * 0.16;
            let t = l * 0.010;
            kids.push(line(
                &[
                    ([-hb, y - l * 0.004, z], t),
                    ([-hb * 0.85, y + h, z - l * 0.03], t),
                    ([0.0, y + h + l * 0.012, z - l * 0.045], t),
                    ([hb * 0.85, y + h, z - l * 0.03], t),
                    ([hb, y - l * 0.004, z], t),
                ],
                8,
                &c.pod,
            ));
            kids.push(line(
                &[
                    ([-hb * 0.6, y + h + l * 0.004, z - l * 0.040], l * 0.006),
                    ([hb * 0.6, y + h + l * 0.004, z - l * 0.040], l * 0.006),
                ],
                6,
                &c.glow,
            ));
        }
        wear_ladder(
            kids,
            hull,
            c,
            ctx,
            cockpit,
            &Hatch {
                z: hz,
                len: hatch_len,
                top: hull.sheer_z(hz) + l * 0.02,
            },
        );
    }
}
