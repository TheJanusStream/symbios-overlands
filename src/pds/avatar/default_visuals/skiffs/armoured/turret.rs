//! The armoured car's turret: an octagonal drum with a sloped upper course, a
//! hatch lying on its roof, a commander's cupola off to the near quarter, two
//! lit vision blocks on its forward quarters and the seed's accent as a band
//! round its base.
//!
//! **No barrel anywhere.** Her own height is what says armoured car - about
//! 0.35 m of turret over a 0.48 m hull, the real proportion for a small one -
//! and the chase camera, looking down at 22.9 degrees, sees the roof, the
//! hatch and the cupola before anything else. Drawn without her, as a
//! commander's cupola straight on the roof, she is a low dome on a long box:
//! an armoured van.
//!
//! One node is the drum: a Bevel with a flat chamfer IS an octagonal prism,
//! and `taper` turns it into the frustum a real turret is. A Lathe cannot do
//! this - its ring normals are radial whatever its resolution, so a res-8
//! turret shades as a smooth dome.

use crate::pds::avatar::livery::ArmouredColours;
use crate::pds::generator::Generator;
use crate::seeded_defaults::OrnatenessTier;

use super::{
    ArmouredPlan, CUPOLA_SHARE, NEAR, NO_TURN, TURRET_BED, TURRET_R, TURRET_TAPER, TURRET_Z, board,
    plate, quat_x, quat_y, tapered_plate,
};

/// The chamfer every turret plate is cut at, as a fraction of its smaller
/// footprint axis: 0.29 is the octagon, where a square's corner is cut back
/// far enough that its eight faces are even.
const OCTAGON: f32 = 0.29;

/// The hatch's radius and the cupola's, each over the turret's TOP radius.
const HATCH_R: f32 = 0.62;
const CUPOLA_R: f32 = 0.46;

/// How far the open hatch's lid is thrown back (rad), and how far a vision
/// block stands round the turret from dead ahead.
const HATCH_LIFT: f32 = 1.15;
const BLOCK_AT: f32 = 0.62;

/// A vision block's half-width over the turret's radius, and its height over
/// the drum's.
const BLOCK: (f32, f32) = (0.30, 0.10);

/// Where the turret stands (m), which the drum, the hatch, the cupola, the
/// vision blocks and the band all read.
#[derive(Clone, Copy, Debug)]
struct Drum {
    /// Its station along the machine.
    z: f32,
    /// Its base radius.
    r: f32,
    /// Its base over the datum.
    base: f32,
    /// Its own height, cupola excluded.
    h: f32,
}

/// Where the turret stands.
///
/// It stands ON the flat roof, sunk [`TURRET_BED`] into it, and the drum plus
/// the cupola on it reach exactly the blueprint's own height. It may overhang
/// the roof facet onto the upper flanks, as a real turret overhangs its
/// ring, but never past the DRAWN flank at the height its base sits at, or
/// it hangs in the air.
fn turret_at(plan: &ArmouredPlan) -> Drum {
    let z = plan.at(TURRET_Z);
    let base = plan.crown_at(z) - plan.at(TURRET_BED);
    let r = (plan.half_width_at(z) * TURRET_R).min(plan.flank_x(z, base) * 0.99);
    let h = (plan.screen_top() - base) / (1.0 + CUPOLA_SHARE);
    Drum { z, r, base, h }
}

/// The turret, its hatch - thrown OPEN at Ornate - its cupola and its vision
/// blocks.
///
/// The open lid, hinged up on the roof, is the one thing that says "crew"
/// without saying "gun" from a camera looking down at her.
pub(super) fn turret(
    kids: &mut Vec<Generator>,
    plan: &ArmouredPlan,
    c: &ArmouredColours,
    o: OrnatenessTier,
) {
    let d = turret_at(plan);
    let rt = d.r * TURRET_TAPER;
    kids.push(tapered_plate(
        [d.r * 2.0, d.h, d.r * 2.10],
        &c.hull,
        [0.0, d.base + d.h * 0.5, d.z],
        NO_TURN,
        OCTAGON,
        [1.0 - TURRET_TAPER; 2],
        [0.0; 2],
    ));
    let hr = rt * HATCH_R;
    let hz = d.z - rt * 0.16;
    if o == OrnatenessTier::Ornate {
        kids.push(plate(
            [hr * 2.0, plan.at(0.016), hr * 2.0],
            &c.hatch,
            [
                0.0,
                d.base + d.h + hr * HATCH_LIFT.sin() * 0.55,
                hz - hr * 0.70 + hr * HATCH_LIFT.cos() * 0.55,
            ],
            quat_x(-HATCH_LIFT),
            OCTAGON,
        ));
    } else {
        kids.push(plate(
            [hr * 2.0, plan.at(0.020), hr * 2.0],
            &c.hatch,
            [0.0, d.base + d.h - plan.at(0.004), hz],
            NO_TURN,
            OCTAGON,
        ));
    }
    cupola(kids, plan, c, d, o);
    blocks(kids, plan, c, d);
}

/// The commander's cupola: a small faceted drum with its own lid from Adorned
/// up, set on the near quarter of the turret's roof so the chase camera sees
/// it stand off the turret.
fn cupola(
    kids: &mut Vec<Generator>,
    plan: &ArmouredPlan,
    c: &ArmouredColours,
    d: Drum,
    o: OrnatenessTier,
) {
    let rt = d.r * TURRET_TAPER;
    let (z, y) = (d.z + rt * 0.52, d.base + d.h - plan.at(0.012));
    let r = rt * CUPOLA_R;
    let h = d.h * CUPOLA_SHARE / 1.16 + plan.at(0.012);
    let x = NEAR * r * 0.55;
    kids.push(tapered_plate(
        [r * 2.0, h * 1.16, r * 2.0],
        &c.hull,
        [x, y + h * 0.58, z],
        NO_TURN,
        OCTAGON,
        [0.14; 2],
        [0.0; 2],
    ));
    if o != OrnatenessTier::Plain {
        kids.push(plate(
            [r * 1.60, plan.at(0.014), r * 1.60],
            &c.hatch,
            [x, y + h * 1.16 - plan.at(0.004), z],
            NO_TURN,
            OCTAGON,
        ));
    }
}

/// Two vision blocks on the turret's forward quarters, lit like every other
/// window band in the fleet.
fn blocks(kids: &mut Vec<Generator>, plan: &ArmouredPlan, c: &ArmouredColours, d: Drum) {
    let y = d.base + d.h * 0.62;
    for s in [-1.0f32, 1.0] {
        let a = s * BLOCK_AT;
        kids.push(board(
            [d.r * BLOCK.0 * 2.0, d.h * BLOCK.1, plan.at(0.016)],
            &c.glass,
            [a.sin() * d.r * 0.94, y, d.z + a.cos() * d.r * 0.94],
            quat_y(a),
            plan.at(0.006),
        ));
    }
}

/// **Identity.** The seed's accent as a band round the turret's base - the
/// identity slot the chase camera cannot miss, since it looks down on the
/// roof.
pub(super) fn band(kids: &mut Vec<Generator>, plan: &ArmouredPlan, c: &ArmouredColours) {
    let d = turret_at(plan);
    kids.push(tapered_plate(
        [d.r * 2.06, d.h * 0.16, d.r * 2.16],
        &c.flash,
        [0.0, d.base + d.h * 0.24, d.z],
        NO_TURN,
        OCTAGON,
        [0.10; 2],
        [0.0; 2],
    ));
}
