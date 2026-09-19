//! The power catamaran (#1372): SportsRec's runabout, and where the legacy
//! catamaran survives. Two sweeps of ONE demihull profile, a crowned bridge
//! deck whose flat underside is the tunnel roof, stopping short so her two
//! bows stand forward as separate points - the cat read from the chase
//! camera - a centre console, a helm seat, and twin outboards on the
//! transoms with their legs in the water.

use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::seeded_defaults::{OrnatenessTier, WearTier};

use super::super::super::common::quat_x;
use super::super::RunaboutColours;
use super::super::profile::{HullProfile, PlaningForm};
use super::hull::{UPRIGHT, deck_run, line, panel, run_z, skin, steering_wheel, sweep, underbody};
use super::{Launch, LaunchForm};

/// The demihull's plan form: slim, full aft, a knife bow.
const PLAN: &[(f32, f32)] = &[
    (-0.500, 0.90),
    (-0.300, 1.00),
    (-0.050, 1.00),
    (0.150, 0.92),
    (0.290, 0.74),
    (0.390, 0.52),
    (0.460, 0.28),
    (0.500, 0.06),
];

/// The demihull's half-beam over the blueprint's (the form's `beam`), and
/// the whole craft's beam over the blueprint's: the two hulls sit at
/// +-(overall / 2 - demihull half-beam).
const DEMIHULL_BEAM: f32 = 0.52;
const OVERALL_BEAM: f32 = 1.62;

/// How far forward the bridge deck runs, as a fraction of the length; the
/// bows stand clear of it.
const BRIDGE_FWD: f32 = 0.22;

/// The bridge deck's crown, as a section-depth factor on its own sweep.
const BRIDGE_CROWN: f32 = 0.10;

pub(super) struct Catamaran;

const FORM: LaunchForm = LaunchForm {
    plan: PLAN,
    planing: PlaningForm {
        beam: DEMIHULL_BEAM,
        freeboard: 0.64,
        rise: 1.45,
        section: 1.45,
        skeg: 0.030,
    },
    screen: 0.060,
    cockpit_aft: -0.300,
};

/// Each demihull's centreline offset from the craft's, read off the demihull
/// profile alone (its half-beam carries the blueprint's beam).
pub(super) fn demihull_offset(hull: &HullProfile) -> f32 {
    hull.half_beam * (OVERALL_BEAM / DEMIHULL_BEAM) - hull.half_beam
}

/// The bridge deck's top at `z`, `x` off the centreline.
fn deck_top(hull: &HullProfile, off: f32, z: f32, x: f32) -> f32 {
    let r = off + hull.half_beam_at(z);
    hull.sheer_z(z) + r * BRIDGE_CROWN * (1.0 - (x / r).powi(2)).max(0.0).sqrt()
}

impl Launch for Catamaran {
    fn form(&self) -> &'static LaunchForm {
        &FORM
    }

    fn overall_beam(&self, hull: &HullProfile) -> f32 {
        2.0 * (demihull_offset(hull) + hull.half_beam)
    }

    /// The crossbeam from hull to hull under the bridge deck: the waterline
    /// centre, where the travel pose puts the root, is open tunnel here.
    fn root(&self, hull: &HullProfile, c: &RunaboutColours) -> Generator {
        let l = hull.loa;
        let z = -0.10 * l;
        let y = hull.sheer_z(z) - l * 0.010;
        let off = demihull_offset(hull);
        line(
            &[([-off, y, z], l * 0.012), ([off, y, z], l * 0.012)],
            8,
            &c.leg,
        )
    }

    fn build(
        &self,
        kids: &mut Vec<Generator>,
        hull: &HullProfile,
        c: &RunaboutColours,
        ctx: &PartCtx,
    ) {
        let l = hull.loa;
        let off = demihull_offset(hull);
        let hulls = [-off, off];
        for x0 in hulls {
            skin(kids, hull, c, x0, 0.0);
            underbody(kids, hull, c, x0, false);
        }
        // The bridge deck: an UPPER half-pipe over the demihulls' own
        // stations, its radius reaching each one's outer rail.
        let z1 = BRIDGE_FWD * l;
        let bridge: Vec<_> = run_z(hull, hull.transom_z(), z1)
            .into_iter()
            .map(|z| {
                (
                    [0.0, hull.sheer_z(z) - l * 0.002, z],
                    off + hull.half_beam_at(z) * 0.985,
                )
            })
            .collect();
        kids.push(sweep(
            &bridge,
            16,
            [1.0, BRIDGE_CROWN, 1.0],
            [0.0, 0.5],
            &c.deck,
            0.0,
        ));
        // Each bow's own foredeck, forward of the bridge.
        for x0 in hulls {
            kids.push(deck_run(
                hull,
                z1 - l * 0.04,
                hull.stem_z() - l * 0.004,
                &c.deck,
                x0,
            ));
        }
        // The centre console.
        let cz = -0.02 * l;
        let (cw, ch, cd) = (l * 0.17, l * 0.12, l * 0.11);
        let cy = deck_top(hull, off, cz, 0.0);
        kids.push(panel(
            [cw, ch, cd],
            &c.console,
            [0.0, cy + ch * 0.5 - l * 0.006, cz],
            UPRIGHT,
            l * 0.02,
        ));
        // Its screen: a small frame over the console's forward edge.
        let sy = cy + ch - l * 0.006;
        let r = l * 0.005;
        let hw = cw * 0.52;
        kids.push(line(
            &[
                ([-hw, sy - l * 0.004, cz + cd * 0.20], r),
                ([-hw, sy + l * 0.05, cz + cd * 0.05], r),
                ([0.0, sy + l * 0.056, cz + cd * 0.30], r),
                ([hw, sy + l * 0.05, cz + cd * 0.05], r),
                ([hw, sy - l * 0.004, cz + cd * 0.20], r),
            ],
            8,
            &c.chrome,
        ));
        steering_wheel(
            kids,
            hull,
            c,
            [0.0, cy + ch * 0.80, cz - cd * 0.52],
            l * 0.024,
        );
        // A helm seat on a pedestal behind it.
        let hz = cz - cd * 0.5 - l * 0.10;
        let hy = deck_top(hull, off, hz, 0.0);
        kids.push(line(
            &[
                ([0.0, hy - l * 0.004, hz], l * 0.014),
                ([0.0, hy + l * 0.06, hz], l * 0.012),
            ],
            6,
            &c.chrome,
        ));
        kids.push(panel(
            [l * 0.10, l * 0.030, l * 0.075],
            &c.upholstery,
            [0.0, hy + l * 0.07, hz],
            UPRIGHT,
            l * 0.012,
        ));
        kids.push(panel(
            [l * 0.10, l * 0.060, l * 0.020],
            &c.upholstery,
            [0.0, hy + l * 0.105, hz - l * 0.035],
            quat_x(-0.25),
            l * 0.008,
        ));
        // Twin outboards, one on each transom. Worn: one engine was replaced,
        // and its white cowl does not match - on a catamaran the engines are
        // what the chase camera sees, where a deck patch on a pale deck was
        // invisible.
        let t = hull.transom_z();
        let worn = matches!(ctx.wear, WearTier::Worn | WearTier::Battered);
        for (i, x0) in hulls.into_iter().enumerate() {
            let y = hull.sheer_z(t);
            let (cwid, chgt, cdep) = (l * 0.070, l * 0.105, l * 0.090);
            let cowl = if worn && i == 0 { &c.cowl_odd } else { &c.cowl };
            kids.push(panel(
                [cwid, chgt, cdep],
                cowl,
                [x0, y + chgt * 0.40, t - cdep * 0.50],
                UPRIGHT,
                l * 0.028,
            ));
            let leg = [
                ([x0, y + l * 0.004, t - cdep * 0.40], l * 0.020),
                (
                    [x0, hull.keel_at(t) - l * 0.035, t - cdep * 0.45],
                    l * 0.016,
                ),
            ];
            kids.push(sweep(&leg, 8, [0.45, 1.0, 1.0], [0.0, 1.0], &c.leg, 0.0));
        }
        if ctx.ornateness != OrnatenessTier::Plain {
            // A T-top: canvas on a chrome frame over the console and helm.
            let h = l * 0.30;
            let (z_a, z_b) = (cz + cd * 0.1, hz - l * 0.02);
            for z in [z_a, z_b] {
                let y = deck_top(hull, off, z, cw * 0.45);
                kids.push(line(
                    &[
                        ([-cw * 0.45, y - l * 0.004, z], l * 0.006),
                        ([-cw * 0.45, cy + h, z], l * 0.006),
                        ([cw * 0.45, cy + h, z], l * 0.006),
                        ([cw * 0.45, y - l * 0.004, z], l * 0.006),
                    ],
                    6,
                    &c.chrome,
                ));
            }
            let top = [
                ([0.0, cy + h - l * 0.002, z_b - l * 0.04], cw * 0.62),
                ([0.0, cy + h - l * 0.002, z_a + l * 0.04], cw * 0.62),
            ];
            kids.push(sweep(
                &top,
                10,
                [1.0, 0.14, 1.0],
                [0.0, 0.5],
                &c.canvas,
                0.0,
            ));
        }
        if ctx.ornateness == OrnatenessTier::Ornate {
            // A wakeboard tower over the helm - SportsRec's own silhouette.
            let z = hz - l * 0.02;
            let x = off;
            let y = deck_top(hull, off, z, x);
            let top = cy + l * 0.36;
            let t = l * 0.010;
            kids.push(line(
                &[
                    ([-x, y - l * 0.004, z + l * 0.02], t),
                    ([-x * 0.85, top - l * 0.04, z - l * 0.03], t),
                    ([-x * 0.45, top, z - l * 0.05], t),
                    ([x * 0.45, top, z - l * 0.05], t),
                    ([x * 0.85, top - l * 0.04, z - l * 0.03], t),
                    ([x, y - l * 0.004, z + l * 0.02], t),
                ],
                8,
                &c.chrome,
            ));
        }
        if ctx.wear == WearTier::Battered {
            // A tarp lashed over the console, and a fuel can aft.
            let tarp = [
                ([0.0, cy + ch - l * 0.012, cz - cd * 0.55], cw * 0.56),
                ([0.0, cy + ch - l * 0.008, cz + cd * 0.55], cw * 0.56),
            ];
            kids.push(sweep(&tarp, 10, [1.0, 0.45, 1.0], [0.0, 0.5], &c.tarp, 0.0));
            let can = l * 0.050;
            let z = t + l * 0.10;
            kids.push(panel(
                [can * 0.95, can * 1.2, can * 0.55],
                &c.can,
                [0.0, deck_top(hull, off, z, 0.0) + can * 0.6 - l * 0.004, z],
                UPRIGHT,
                l * 0.004,
            ));
        }
    }
}
