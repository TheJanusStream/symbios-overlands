//! The Pirate kit (#1379): what a sloop carries when her owner's theme is
//! [`mood::BUCCANEER`](crate::seeded_defaults::avatar::mood::BUCCANEER), and
//! nothing any other theme ever draws.
//!
//! # Why this is geometry and not a livery
//!
//! The retired boat catalogue had a "buccaneer kit" of tagged parts, and when
//! it went in #1363 a comment promised the pirate's black colours were "a
//! livery and a rig variant now". They were neither, and they could not be:
//! [`crate::pds::avatar::livery`]'s header is a written decision that nothing
//! there is keyed to the avatar's theme, because hue-keying by theme would
//! put every avatar of a theme in the same boat - and the sail's colour is
//! the scheme's too. The gaff cutter is not hers either; it is equally at
//! home on MARTIAL and WORKING.
//!
//! So the kit is geometry in this type's own builder, gated on the theme the
//! way the longship's serpent is (#1369), and the three colours it needs are
//! fixed ones that ask nothing about the theme.
//!
//! # Why the Pirate and no one else
//!
//! Every other boat theme reaches the sloop plus one to three types of its
//! own; measured over the affinity tables, **Pirate is the only theme in the
//! population whose boat is always the family floor**. Without a kit there is
//! nothing on a Pirate's boat that says Pirate, and four of the first ten
//! Pirate sloop seeds wear the White scheme - a pirate in a white yacht.
//!
//! # What it draws, and what was cut
//!
//! Agreed on the phase-1 renders (`target/dump/vehicles2026-09/pirate/`):
//!
//! | tier     | adds                                         |
//! |----------|----------------------------------------------|
//! | every    | the black ensign and the roger on it         |
//! | Adorned  | gunports, three a side                       |
//! | Battered | the ensign's fly tattered into three tongues |
//!
//! The ensign is on EVERY tier because it is the theme's identity and not an
//! ornament - a Plain pirate is still a pirate.
//!
//! A skull CARVED at the stem head was drawn twice and cut both times. As a
//! modelled head it read at 109 px a metre as a white BALL, which is the
//! ball-on-a-post finial this redesign retired; as a flat bone plaque on the
//! stem cheeks it read as a white smudge low on the bow, nearer to damage
//! than to ornament.

use bevy::math::{Quat, Vec3};

use crate::pds::generator::Generator;
use crate::seeded_defaults::{OrnatenessTier, WearTier};

use super::super::super::common::{cuboid, id_quat, prim, quat_x, quat_xyzw};
use super::super::profile::HullProfile;
use super::super::{BoatColours, dim};
use super::hull::line;
use super::rig::Head;

/// The ensign's fly and drop as fractions of the overall length: 0.30 x 0.20 m
/// on the nominal 3 m boat, which is 33 x 22 px at the play camera's 109 px a
/// metre.
///
/// Agreed on render at that size and DELIBERATELY NOT LARGER. The skull and
/// the crossed bones are both legible at 1080 lines; a bigger flag on a 2.8 m
/// boat reads as a cartoon.
const FLY: f32 = 0.098;
const DROP: f32 = 0.065;

/// The bunting's thickness - the burgee's own, so the two flags on one boat
/// are cut from one cloth.
const CLOTH: f32 = 0.0043;

/// The taffrail staff, for the two rigs that set no gaff: how far forward of
/// the transom it is stepped, how tall it stands over the sheer there, how
/// thick it is, and how far its heel is sunk below the deck - all of the
/// overall length.
const STAFF_STATION: f32 = 0.012;
const STAFF_HEIGHT: f32 = 0.165;
const STAFF_RADIUS: f32 = 0.0055;
const STAFF_HEEL: f32 = 0.010;

/// The hoist a BATTERED ensign keeps whole, as a fraction of the fly, and the
/// three tongues the rest of it goes to: each one's rise off the flag's
/// middle and how far it reaches, as fractions of the drop and of what is
/// left of the fly.
const TATTER_HOIST: f32 = 0.62;
const TONGUES: [(f32, f32); 3] = [(0.30, 1.00), (-0.06, 0.74), (-0.36, 0.52)];

/// The roger: the bone disc's radius as a fraction of the ensign's drop,
/// where it sits along the fly, and how far the crossed bones are laid over.
const DISC_OF_DROP: f32 = 0.30;
const DISC_OF_FLY: f32 = 0.34;
const BONE_LAY_DEGREES: f32 = 38.0;

/// How far round the section a gunport sits, in radians down from the deck
/// edge.
///
/// THE BAND IS BOUNDED BY TWO IDENTITY SLOTS, not by taste: the cove line is
/// scribed 0.22 rad down and the boot top is at 0.754 (the hull's `BOOT_F`),
/// and each is a tube seated centre-on-surface, so each eats its own radius
/// either side. 0.50 puts the port and its raised lid inside what is left on
/// every hull form at every blueprint corner. The first angle tried, 0.40,
/// put the lid THROUGH the cove, and only measuring the span caught it.
const PORT_THETA: f32 = 0.50;

/// Where the three ports a side stand, as z fractions of the overall length -
/// amidships, where the topsides still face outboard at the chase camera's
/// 22.9 degrees of down-angle. Further forward they foreshorten into the turn
/// of the bow.
const PORT_STATIONS: [f32; 3] = [-0.22, -0.06, 0.10];

/// A gunport's side, as a fraction of the overall length: 0.10 m on the
/// nominal boat, 11 px at play distance.
const PORT_SIZE: f32 = 0.033;

/// The dark panel's thickness, and the lid's.
const PORT_THICK: f32 = 0.0035;
const LID_THICK: f32 = 0.0040;

/// The lid: how far up the section it is hinged and its own height and width,
/// as fractions of the port, and how far it has swung open.
///
/// 0 degrees would lie it flat over the port (shut); 90 would stand it
/// straight out. 65 lifts it far enough to read as a raised flap at 109 px a
/// metre while its hinge edge stays on the planking.
const LID_HINGE: f32 = 0.60;
const LID_HEIGHT: f32 = 0.34;
const LID_WIDTH: f32 = 0.98;
const LID_OPEN_DEGREES: f32 = 65.0;

/// Dress a Pirate's sloop. `peak` is the gaff's head where her rig sets one
/// and `None` where it does not - see [`super::rig::Rigging::has_gaff_peak`].
pub(super) fn dress(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    peak: Option<Head>,
    c: &BoatColours,
    ornateness: OrnatenessTier,
    wear: WearTier,
) {
    ensign(kids, hull, peak, c, wear);
    if ornateness >= OrnatenessTier::Adorned {
        gunports(kids, hull, c);
    }
}

/// The black colours and the roger on them, flown from the gaff's peak or
/// from a staff at the taffrail.
///
/// SEATED half a spar radius inside whatever it flies from, which is the rule
/// [`super::rig::Rig::jaw`] takes for the boom and the gaff and the burgee
/// takes for the masthead. The mount is READ - off the gaff spar's own head,
/// or off the hull's sheer at the transom - never restated as a fraction,
/// which is how the burgee once came to float clear of a gunter's yard
/// (#1366 defect 4).
fn ensign(
    kids: &mut Vec<Generator>,
    hull: &HullProfile,
    peak: Option<Head>,
    c: &BoatColours,
    wear: WearTier,
) {
    let loa = hull.loa;
    let (fly, drop) = (loa * FLY, loa * DROP);
    let cloth = dim(loa * CLOTH);
    // The ensign is LACED TO THE SPAR, so its head sits on that spar's own
    // CENTRELINE and overlaps the spar's radius from below. Hanging it a
    // fixed fraction of the length under the spar head instead left it
    // floating on every hull over about 1.9 m, because the drop outgrew the
    // spar it was supposed to be touching - the burgee's centre is only a
    // hair under the truck for the same reason (#1366 defect 4).
    let (top, hoist_z) = match peak {
        Some(head) => (head.y, head.z + head.r * 0.5),
        None => {
            // No peak: a short staff stepped on the after deck, its foot on
            // the sheer AT that station rather than at a remembered height,
            // so a hull with more rise aft carries it higher.
            let z = hull.transom_z() + loa * STAFF_STATION;
            let radius = dim(loa * STAFF_RADIUS);
            let foot = hull.sheer_z(z);
            let head = foot + loa * STAFF_HEIGHT;
            kids.push(line(
                &[
                    ([0.0, foot - loa * STAFF_HEEL, z], radius),
                    ([0.0, head, z], radius),
                ],
                8,
                c.timber.clone(),
            ));
            (head, z + radius * 0.5)
        }
    };
    let mid_y = top - drop * 0.5;
    if wear == WearTier::Battered {
        // BATTERED tatters the FLY, which is the end that flogs: the hoist
        // two thirds hold and the rest goes to three tongues with the wind
        // through them. The brief's own "tattered banner, BATTERED only",
        // given to the one theme that has a banner to tatter.
        let hoist = fly * TATTER_HOIST;
        kids.push(flag(
            [cloth, drop, hoist],
            c,
            [0.0, mid_y, hoist_z - hoist * 0.5],
        ));
        let tail = fly - hoist;
        for (rise, reach) in TONGUES {
            kids.push(flag(
                [cloth * 0.92, drop * 0.26, tail * reach],
                c,
                [
                    0.0,
                    mid_y + drop * rise,
                    hoist_z - hoist - tail * reach * 0.5,
                ],
            ));
        }
    } else {
        kids.push(flag(
            [cloth, drop, fly],
            c,
            [0.0, mid_y, hoist_z - fly * 0.5],
        ));
    }
    roger(kids, cloth, drop, mid_y, hoist_z - fly * DISC_OF_FLY, c);
}

/// One panel of bunting.
fn flag(size: [f32; 3], c: &BoatColours, at: [f32; 3]) -> Generator {
    prim(cuboid(size.map(dim), c.bunting.clone()), at, id_quat())
}

/// A skull over crossed bones: a bone disc and two bars, each standing proud
/// of the cloth on BOTH faces so the flag reads the same from either side.
///
/// The disc is a two-point spine across the cloth rather than a box, which is
/// a disc seen face on for the same one node. Nothing here is sized to MEET
/// the bunting - a face flush with another face stipples (the coplanar trap)
/// - so both stand a little through it.
fn roger(kids: &mut Vec<Generator>, cloth: f32, drop: f32, mid_y: f32, at_z: f32, c: &BoatColours) {
    let radius = dim(drop * DISC_OF_DROP);
    kids.push(line(
        &[
            ([-cloth * 0.85, mid_y, at_z], radius),
            ([cloth * 0.85, mid_y, at_z], radius),
        ],
        12,
        c.bone.clone(),
    ));
    let bar = [cloth * 1.7, drop * 0.085, radius * 2.5].map(dim);
    for lay in [1.0f32, -1.0] {
        kids.push(prim(
            cuboid(bar, c.bone.clone()),
            [0.0, mid_y - radius * 0.95, at_z],
            quat_xyzw(quat_x(lay * BONE_LAY_DEGREES.to_radians())),
        ));
    }
}

/// Three gunports a side: a dark panel let into the topsides with its lid
/// hinged UP and standing open over it.
///
/// PIERCED WITHOUT A HOLE, because there is no alpha to cut one with (#1359
/// rule 4): the dark of an open port is a panel, and the lid over it is what
/// says the darkness is a port rather than a stain. The lid is NOT optional -
/// on a black scheme the dark panel disappears into the topsides and the lid
/// is the whole read.
///
/// Every station and every angle is read off the [`HullProfile`], and each
/// part is turned to face along the skin's own outward normal.
fn gunports(kids: &mut Vec<Generator>, hull: &HullProfile, c: &BoatColours) {
    let loa = hull.loa;
    let port = loa * PORT_SIZE;
    for side in [1.0f32, -1.0] {
        for zf in PORT_STATIONS {
            let z = zf * loa;
            let facing = facing_quat(hull, z, PORT_THETA, side);
            // The panel's centre sits ON the skin, as every trim line on this
            // hull does, so half of it is buried and no face is coplanar.
            kids.push(prim(
                cuboid([loa * PORT_THICK, port, port].map(dim), c.port.clone()),
                skin_point(hull, z, PORT_THETA, side),
                quat_xyzw(facing.to_array()),
            ));
            // The lid, HINGED ON THE SKIN at the port's top edge and swung
            // up and out from there. Its hinge edge stays on the hull - a
            // flap that merely stands off the planking is a flap floating in
            // the air, which is what the one-machine guard catches.
            let up = arc(hull, z, port * LID_HINGE);
            let hinge_theta = PORT_THETA - up;
            let hinged = facing_quat(hull, z, hinge_theta, side);
            let height = port * LID_HEIGHT;
            let swung = hinged * Quat::from_rotation_z(LID_OPEN_DEGREES.to_radians());
            // The hinge edge sunk half the lid's thickness into the skin, and
            // the lid's centre half its height down its own swung plane.
            let at = Vec3::from(skin_point(hull, z, hinge_theta, side))
                - Vec3::from(normal(hull, z, hinge_theta, side)) * loa * LID_THICK * 0.5
                + swung * Vec3::new(0.0, -height * 0.5, 0.0);
            kids.push(prim(
                cuboid(
                    [loa * LID_THICK, height, port * LID_WIDTH].map(dim),
                    c.timber.clone(),
                ),
                at.into(),
                quat_xyzw(swung.to_array()),
            ));
        }
    }
}

/// A point on the topsides, `theta` radians down from the deck edge at
/// station `z` on `side` - the hull's own elliptical section, the one the
/// boot stripe and the cove line are placed by.
fn skin_point(hull: &HullProfile, z: f32, theta: f32, side: f32) -> [f32; 3] {
    let half_beam = hull.half_beam_at(z);
    [
        side * half_beam * theta.cos(),
        hull.sheer_z(z) - half_beam * hull.section * theta.sin(),
        z,
    ]
}

/// How many radians of the section `metres` of arc spans at station `z`.
fn arc(hull: &HullProfile, z: f32, metres: f32) -> f32 {
    metres / (hull.half_beam_at(z) * hull.section).max(1e-4)
}

/// The skin's outward unit normal there, by difference on [`skin_point`]
/// along both of the surface's own directions.
///
/// Differenced rather than derived: the hull tapers along her length as well
/// as round her section, so the normal leans forward as well as outward, and
/// a formula for it would have to restate the section law and the plan form
/// that [`HullProfile`] already owns.
fn normal(hull: &HullProfile, z: f32, theta: f32, side: f32) -> [f32; 3] {
    let step = hull.loa * 1e-3;
    let at = skin_point(hull, z, theta, side);
    let round = skin_point(hull, z, theta + 1e-3, side);
    let along = skin_point(hull, z + step, theta, side);
    let u = [round[0] - at[0], round[1] - at[1], round[2] - at[2]];
    let v = [along[0] - at[0], along[1] - at[1], along[2] - at[2]];
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
    let unit = [n[0] / len, n[1] / len, n[2] / len];
    // The cross product's sign follows the parametrisation, which flips with
    // the side; take whichever of the two points away from the centreline.
    if unit[0] * side > 0.0 {
        unit
    } else {
        [-unit[0], -unit[1], -unit[2]]
    }
}

/// The rotation that lays a part's thin (X) axis along the skin's normal
/// there, so a flat panel lies on a curved hull instead of cutting into it.
///
/// Pitch about Z takes +X to `(cos p, sin p, 0)`; yaw about Y then swings
/// that into the plan. Composed in that order, +X lands on the normal.
fn facing_quat(hull: &HullProfile, z: f32, theta: f32, side: f32) -> Quat {
    let n = normal(hull, z, theta, side);
    let pitch = n[1].clamp(-1.0, 1.0).asin();
    let flat = (n[0] * n[0] + n[2] * n[2]).sqrt();
    // A normal straight up or down has no plan direction to yaw into; the
    // topsides never produce one, but the fallback keeps this total.
    // `Quat::from_rotation_arc` would be shorter and is NOT used: on the port
    // side the normal is nearly anti-parallel to +X, which is its degenerate
    // case, and it answers that with an arbitrary perpendicular axis - an
    // arbitrary ROLL on a panel that is taller than it is wide.
    let yaw = if flat < 1e-6 {
        0.0
    } else {
        (-n[2]).atan2(n[0])
    };
    Quat::from_rotation_y(yaw) * Quat::from_rotation_z(pitch)
}
