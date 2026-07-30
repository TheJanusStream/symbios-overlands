//! Careening Slip — where a hull is hove down and its bottom cleaned.
//!
//! A timber slipway running down the shingle, a taken sloop hove over on her
//! beam ends with her masthead tackle to two shore posts, a capstan on the
//! hard with the fall to it, and a pitch fire under a kettle beside the
//! exposed bottom.
//!
//! # Why this subject
//!
//! Careening is the one job a buccaneer does that a navy does not have to:
//! with no dry dock, you run the ship ashore, strip her, and haul the masts
//! down until her keel comes out of the water. It is the most specific
//! silhouette the whole theme has — a ship lying on her side on dry land —
//! and nothing else in the catalogue looks remotely like it.
//!
//! # The heel is IN the hull, not on a transform
//!
//! A hove-down ship is a tilted mass, and #972 lesson 22 forbids a rotated
//! parent carrying offset children: the turn spins those offsets out of the
//! geometry and then hides the fault from every guard here, all of which walk
//! translations only. Two ways out, and this uses both:
//!
//! * the hull is **one `BlobGroup`** — a single leaf prim, so its rotation
//!   carries nothing and displaces nothing;
//! * everything attached to her (masts, tackle) is placed in the *world*
//!   frame from the heel angle, by [`heeled`], rather than nested under her.
//!
//! That keeps the whole prop translation-only, which is what makes the
//! footprint and stack guards mean anything.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    attach, blob_box, blob_capsule, blob_ellipsoid, blob_group, bonded_siding, cuboid_tapered,
    cylinder_tapered, face_uv_offset, footing, glow, id_quat, nest, prim, quat_x, quat_z, solid,
    sphere, strut, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::pds::generator::FaceKey;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    CANVAS_BONE, CANVAS_SHADE, DECK_HOLY, HULL_OAK, HULL_TAR, IRON_BLACK, PORT_BAND, ROPE_HEMP,
    SIGN_AMBER, STONE_QUAY, STRAND_SHINGLE, WHARF_GREY, board, cobbles, fx, hemp, iron, sailcloth,
    strake, strand, tar,
};

/// The site: shingle foreshore with a cobbled hard at its head.
const SITE: [f32; 3] = [22.0, 0.28, 16.0];
const GROUND: f32 = SITE[1];

/// The slipway's timber ways: how far apart, how long, and where the seaward
/// end of the run sits (`-Z` is seaward and the hero side).
///
/// `WAY_FAR` is stated rather than derived from a centre, because the thing
/// that has to be true is that the run stays on the beach: at a tidy
/// `-WAY_LEN / 2 + 2` the ways reached three metres past the site and the
/// footprint guard caught it.
const WAY_GAUGE: f32 = 3.2;
const WAY_LEN: f32 = 12.0;
const WAY_W: f32 = 0.55;
const WAY_FAR: f32 = -7.0;
const WAY_NEAR: f32 = WAY_FAR + WAY_LEN;
/// Top of the ways — what the hull rests on.
const WAY_TOP: f32 = GROUND + 0.24;

/// The hull, upright, before she is hove down: length, beam, depth.
const HULL_LEN: f32 = 11.0;
const HULL_BEAM: f32 = 3.4;
const HULL_DEPTH: f32 = 2.6;

/// How far over she is hove, in radians about her own fore-and-aft axis.
///
/// Forty-nine degrees. Chosen against what the heel has to ACHIEVE rather
/// than picked: the keel has to come clear of the ways, and how far it comes
/// clear is `sqrt((beam/2·sin)² + (depth/2·cos)²) − depth/2·cos`, which at
/// thirty-five degrees is only 380 mm on this hull — a boat leaning, not a
/// boat hove down. At forty-nine it is close to 700 mm and the garboards are
/// unmistakably out of the water, which is the whole object of careening.
const HEEL: f32 = 0.85;

/// Where the hull lies along the ways.
const HULL_Z: f32 = -0.6;

/// How high the hull's own centre is carried so that her **down-side bilge**
/// rests on the ways.
///
/// This is the correction that made the geometry physical. The first build
/// pivoted her about the keel and left it bearing on the ways, which is what
/// "hove down" sounds like and is not what it means: rolling about a keel
/// that stays put drives the port bilge two metres into the beach. A careened
/// ship rests on the **turn of her bilge** with the keel lifted clear of the
/// ground — that lift is the whole point, because the keel and the garboards
/// are what you have hauled her over to get at.
///
/// The lift is derived from the heel — and from the fact that her midship
/// section is an **ellipse**, not a box.
///
/// The obvious formula is the rotated corner, `beam/2 · sin + depth/2 · cos`,
/// and it is right for a rectangular section and wrong for this one by
/// 600 mm — which is exactly how far she floated over her own ways in the
/// first render. An ellipse's lowest point under a rotation is its *support
/// function*, `sqrt((a·sin)² + (b·cos)²)`, which is always less than the
/// corner because an ellipse is inscribed in its own box.
///
/// Worth deriving rather than fudging: every blob mass seated on a surface
/// has this, and reading the number off a render instead would have buried a
/// relationship inside a constant.
fn hull_lift() -> f32 {
    let (s, c) = HEEL.sin_cos();
    let a = HULL_BEAM * 0.5 * s;
    let b = HULL_DEPTH * 0.5 * c;
    WAY_TOP + (a * a + b * b).sqrt()
}

/// Sample resolution for the hull. Near the sanitiser's 48 ceiling: she is
/// eleven metres long, so even at 44 the cells are a quarter of a metre and
/// nothing thinner than half a metre survives (see
/// the shared `blob_cell_size` note in `items::util`).
const HULL_RES: u32 = 44;

/// The two careening posts on the hard, and the height they take the tackle.
///
/// Outboard of the hull's own beam, or the purchase pulls straight down and
/// heaves nothing.
const POST_X: f32 = 5.6;
const _: () = assert!(
    POST_X > HULL_BEAM,
    "the shore posts are inside the hull's own beam"
);
const POST_H: f32 = 5.4;

pub struct CareeningSlip;

impl CatalogueEntry for CareeningSlip {
    fn slug(&self) -> &'static str {
        "careening_slip"
    }
    fn name(&self) -> &'static str {
        "Careening Slip"
    }
    fn description(&self) -> &'static str {
        "A sloop hove down on the ways with her bottom exposed, her masthead tackle to the shore \
         posts and a pitch fire burning beside her."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Pirate]
    }
    fn prosperity_band(&self) -> ProsperityBand {
        PORT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 12.0,
            min_spawn_dist: 26.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Turn a point given in the **upright hull's** own frame into the world,
/// applying the heel about the keel line.
///
/// This is the whole trick. Anything fixed to a hove-down ship — a masthead,
/// a chainplate, a shroud's upper end — is naturally described where it sits
/// on the *upright* vessel, and then has to be placed where the heel actually
/// puts it. Doing that here, once, means nothing in this file is ever nested
/// under a rotated node (#972 lesson 22), and it means the mast and its
/// tackle cannot drift out of agreement with the hull: they are all solving
/// the same equation.
///
/// `x` is athwartships (positive to starboard, which is the side she is hove
/// away from), `y` is up from the keel, `z` is along the ways.
fn heeled(x: f32, y: f32, z: f32) -> [f32; 3] {
    let (s, c) = HEEL.sin_cos();
    [x * c - y * s, hull_lift() + x * s + y * c, HULL_Z + z]
}

/// The hull, as one blended mass.
///
/// A single `BlobGroup` rather than a stack of prims, for two reasons that
/// both matter here. It is watertight and continuous, so a hull reads as a
/// hull from every angle instead of showing the seams between a topsides box
/// and a prow cone — the fault the avatar boat family fixed the same way. And
/// it is a **leaf**, so the heel is one rotation on one node carrying nothing.
fn hull() -> Generator {
    let half = HULL_LEN * 0.5;
    let b = HULL_BEAM * 0.5;
    let d = HULL_DEPTH * 0.5;
    // Amidships fullness, drawn out to a fine bow and a rounded transom. The
    // blend is generous so the stations melt into one sheer rather than
    // reading as a row of beads.
    //
    // Every station OVERLAPS its neighbour. The first build left a 0.55 m gap
    // between the midships mass and the transom, trusting the blend radius to
    // bridge it — and it did not, so she polygonised into two pieces with the
    // stern floating astern of the hull. Connectivity has to be structural,
    // not a property of the blend (the same correction the kit's flag needed).
    let elements = vec![
        blob_ellipsoid([0.0, 0.0, 0.0], [b, d, half * 0.7], 0.5),
        // Fore and aft quarters, each well inside the midships run.
        blob_ellipsoid(
            [0.0, d * 0.1, half * 0.45],
            [b * 0.78, d * 0.92, half * 0.42],
            0.45,
        ),
        blob_ellipsoid(
            [0.0, d * 0.1, -half * 0.45],
            [b * 0.82, d * 0.92, half * 0.42],
            0.45,
        ),
        // Bow: pulled to a fine entry, overlapping the fore quarter.
        blob_ellipsoid(
            [0.0, d * 0.25, half * 0.85],
            [b * 0.32, d * 0.72, half * 0.26],
            0.4,
        ),
        // Transom: fuller and squarer than the bow, overlapping the aft one.
        blob_box(
            [0.0, d * 0.15, -half * 0.84],
            [b * 0.64, d * 0.78, half * 0.18],
            0.42,
        ),
        // Keel, running her length under the garboards — the member the whole
        // job exists to get at, so it wants to be visible.
        blob_capsule(
            [0.0, -d * 0.92, 0.0],
            b * 0.11,
            half * 0.82,
            quat_x(FRAC_PI_2),
            0.12,
        ),
    ];
    // Placed at the hull's OWN origin under the heel — `heeled(0, 0, 0)`.
    //
    // The first build passed `heeled(0, HULL_DEPTH * 0.5, 0)`, which reads
    // like "put her deck at the right height" and in fact applies the lift a
    // second time: `HULL_LIFT` already carries the centre, and the elements
    // are centred on it. She floated a metre and a half over her own ways.
    //
    // #972 lesson 18 exactly — the placement and the thing it is derived from
    // have to be ONE expression. The bilge blocks were correct all along
    // because they call `heeled` directly; only the hull disagreed, which is
    // why a guard comparing blocks to the formula could not see it.
    // `quat_z(+HEEL)`, positive, and the sign is the whole story (#1030).
    // `heeled()` maps (x, y) to (x·c − y·s, x·s + y·c) — the standard 2D
    // rotation by PLUS θ — and the prim's quaternion must be the same turn.
    // The first build used −HEEL: on the hull, whose section is symmetric
    // athwartships, a mirrored heel is invisible, which is exactly what let
    // it ship — but every part placed BY `heeled()` (masthead, shores,
    // tackle) then lived in the reflection of the frame the geometry was
    // drawn in, and the mast crossed its own rig touching nothing.
    prim(
        blob_group(elements, HULL_RES, strake(HULL_TAR)),
        heeled(0.0, 0.0, 0.0),
        quat_z(HEEL),
    )
}

/// The ways she lies on, and the cradle blocks under her keel.
fn slipway() -> Vec<Generator> {
    let mut out = Vec::new();
    for sx in [-1.0_f32, 1.0] {
        let c = [
            sx * WAY_GAUGE * 0.5,
            GROUND + 0.12,
            (WAY_FAR + WAY_NEAR) * 0.5,
        ];
        out.push(prim(
            solid(cuboid_tapered(
                [WAY_W, 0.24, WAY_LEN],
                0.0,
                bonded_siding(board(WHARF_GREY), FaceKey::Top, c),
            )),
            c,
            id_quat(),
        ));
    }
    // Sleepers across the ways, at a spacing that reads as a track.
    let sleepers = 9;
    for i in 0..sleepers {
        let t = (i as f32 + 0.5) / sleepers as f32;
        out.push(prim(
            solid(cuboid_tapered(
                [WAY_GAUGE + WAY_W + 0.5, 0.16, 0.36],
                0.0,
                board(WHARF_GREY),
            )),
            [0.0, GROUND + 0.08, WAY_FAR + t * WAY_LEN],
            id_quat(),
        ));
    }
    // SHORES against her high side, not blocks under her low one.
    //
    // She already bears on the ways by construction — that is what
    // `hull_lift` guarantees — so packing underneath would be propping
    // something that is already down. What a careened hull actually needs is
    // shores holding her *from going further over*, and they are the better
    // read besides: three raking timbers against the exposed bottom say
    // "somebody put her here on purpose".
    //
    // Two corrections from in-world (#1028), both about the same thing —
    // the shore has to meet the ship SHE IS, not the ship's bounding box:
    //
    // * the bearing point tracks her local half-beam. She narrows toward bow
    //   and stern, so a shore placed at the amidships beam near the bow
    //   stood in the air beside her taper — "two bars floating" was exactly
    //   what it was. The beam profile is the same ellipse-along-length the
    //   blob stations approximate.
    // * the timber itself is a [`strut`] between its bearing point and its
    //   foot, so the lean cannot be backwards. The hand-rolled
    //   `quat_z(-lean)` had its sign flipped and leaned every shore AWAY
    //   from the hull it propped — the third hand-rolled rotation in this
    //   one file to get a handedness wrong, which is why the helper now
    //   exists (#972 lesson 23, applied to authoring).
    for dz in [-2.6_f32, 0.0, 2.6] {
        // Local half-beam at this station, on the hull's lengthwise ellipse.
        let half_len = HULL_LEN * 0.5;
        let beam_f = (1.0 - (dz / (half_len * 0.92)).powi(2)).max(0.0).sqrt();
        let bear = heeled(HULL_BEAM * 0.5 * beam_f, 0.0, dz);
        let foot = [bear[0] + 2.6, WAY_TOP, bear[2]];
        out.push(strut(bear, foot, 0.16, 8, board(HULL_OAK)));
        // A sole plate under each foot, so the shore lands on something.
        out.push(prim(
            solid(cuboid_tapered([0.7, 0.16, 0.6], 0.0, board(WHARF_GREY))),
            [foot[0], WAY_TOP - 0.08, foot[2]],
            id_quat(),
        ));
    }
    out
}

/// Her lower mast, and the purchase from its head to the shore posts.
///
/// The mast is stepped on the keel and rakes with the hull, so both its ends
/// come from [`heeled`] — and the tackle is drawn from the masthead it
/// actually has, not from a height picked to look right.
fn masthead_tackle() -> Vec<Generator> {
    let step = heeled(0.0, HULL_DEPTH * 0.35, 1.2);
    let mast_len = 8.4_f32;
    let head = heeled(0.0, HULL_DEPTH * 0.35 + mast_len, 1.2);
    // Both the mast and its cap are STRUTS along the step→head run — the
    // same one conversion everything else on this beach now uses. The first
    // build gave each a hand-applied `quat_z(-HEEL)`, which is the mirror of
    // the turn `heeled()` actually performs: the chord ran port-up, the prim
    // axis starboard-up, and the built timber was the intended mast
    // REFLECTED about the vertical through its own midpoint — crossing the
    // rig diagonally with both ends in the air, while the falls converged on
    // a masthead with no mast under it. That floating junction is precisely
    // what was reported (#1030). A strut cannot disagree with its own
    // endpoints, which is the property that retires this fault class.
    let run = [
        (head[0] - step[0]) / mast_len,
        (head[1] - step[1]) / mast_len,
        (head[2] - step[2]) / mast_len,
    ];
    let mut out = vec![
        strut(step, head, 0.22, 10, board(HULL_OAK)),
        // Masthead cap: a short, fatter collar on the same run, straddling
        // the head so the falls visibly hook onto something.
        strut(
            [
                head[0] - run[0] * 0.14,
                head[1] - run[1] * 0.14,
                head[2] - run[2] * 0.14,
            ],
            [
                head[0] + run[0] * 0.14,
                head[1] + run[1] * 0.14,
                head[2] + run[2] * 0.14,
            ],
            0.3,
            10,
            iron(IRON_BLACK, 0xA1),
        ),
    ];

    // Two shore posts, and a fall from the masthead to each.
    for sx in [-1.0_f32, 1.0] {
        let post = [sx * POST_X, GROUND + POST_H * 0.5, HULL_Z + 2.4];
        out.push(prim(
            solid(cuboid_tapered([0.42, POST_H, 0.42], 0.14, board(HULL_OAK))),
            post,
            id_quat(),
        ));
        // Iron cap band and a sheave block at the head.
        out.push(prim(
            solid(cuboid_tapered(
                [0.5, 0.14, 0.5],
                0.0,
                iron(IRON_BLACK, 0xA2),
            )),
            [post[0], GROUND + POST_H + 0.07, post[2]],
            id_quat(),
        ));
        // The fall: one [`strut`] from the masthead strop to the post head.
        // The first build hand-rolled the rotation from the run's X and Y
        // components alone — ignoring that masthead and post differ in Z
        // too — so both falls yawed off their posts (#1028). The strut takes
        // the genuinely 3D run and cannot.
        let top = [post[0], GROUND + POST_H, post[2]];
        out.push(strut(head, top, 0.05, 6, hemp(ROPE_HEMP)));
    }
    out
}

/// The capstan on the hard, with its bars and the hauling part led to it.
fn capstan() -> Generator {
    let x = 0.0;
    let z = HULL_Z + 6.2;
    let base = [x, GROUND + 0.18, z];
    let mut carried = vec![
        prim(
            solid(cylinder_tapered(0.52, 1.05, 14, 0.16, board(HULL_OAK))),
            [x, GROUND + 0.7, z],
            id_quat(),
        ),
        prim(
            torus(0.05, 0.5, iron(IRON_BLACK, 0xA3)),
            [x, GROUND + 1.16, z],
            id_quat(),
        ),
    ];
    // Capstan bars: six struts radiating horizontally from the drum's own
    // sockets. The first build "laid them flat" by snapping each to whichever
    // quarter-turn was nearest, which left the off-cardinal four standing at
    // wrong angles — the in-world "wheel with wrong rotations" (#1028). A
    // strut from socket to tip is horizontal because its endpoints are, not
    // because a formula said so.
    let bar_y = GROUND + 1.02;
    for i in 0..6 {
        let a = i as f32 * std::f32::consts::TAU / 6.0;
        let dir = [a.cos(), 0.0, a.sin()];
        let socket = [x + 0.42 * dir[0], bar_y, z + 0.42 * dir[2]];
        let tip = [x + 2.05 * dir[0], bar_y, z + 2.05 * dir[2]];
        carried.push(strut(socket, tip, 0.065, 6, board(DECK_HOLY)));
    }
    nest(
        prim(
            solid(cylinder_tapered(
                0.95,
                0.36,
                14,
                0.1,
                cobbles(STONE_QUAY, 0xA4),
            )),
            base,
            id_quat(),
        ),
        carried,
    )
}

fn build_tree() -> Generator {
    let site_c = [0.0, GROUND * 0.5, 0.0];
    let mut shingle_bed = strand(STRAND_SHINGLE);
    shingle_bed.uv_offset = face_uv_offset(FaceKey::Top, site_c);

    let mut carried = vec![footing(SITE[0] * 0.6, SITE[2] * 0.6, [0.0, 3.0], 12.0)];

    // Cobbled hard at the head of the slip, where the gear stands. Bedded
    // into the shingle rather than laid on it, so the two do not share a
    // plane (#972's coplanar rule).
    carried.push(prim(
        solid(cuboid_tapered(
            [14.0, 0.22, 6.4],
            0.0,
            cobbles(STONE_QUAY, 0xA0),
        )),
        [0.0, GROUND + 0.05, SITE[2] * 0.5 - 3.6],
        id_quat(),
    ));

    carried.extend(slipway());
    carried.push(hull());
    carried.extend(masthead_tackle());
    carried.push(capstan());

    // The pitch fire and its kettle, beside her exposed bottom — the part of
    // the job that gives the prop its one warm light, and the reason a
    // careening beach smells the way it does.
    let fire = [-4.2, GROUND, HULL_Z - 2.6];
    carried.push(prim(
        solid(cylinder_tapered(
            0.85,
            0.18,
            12,
            0.0,
            cobbles(STONE_QUAY, 0xA5),
        )),
        [fire[0], GROUND + 0.09, fire[2]],
        id_quat(),
    ));
    carried.push(prim(
        solid(cylinder_tapered(
            0.5,
            0.42,
            12,
            -0.15,
            iron(IRON_BLACK, 0xA6),
        )),
        [fire[0], GROUND + 0.6, fire[2]],
        id_quat(),
    ));
    // Embers under the kettle: small and deep-saturated, so they hold their
    // hue where a broad pale panel would bloom (the standing gotcha).
    carried.push(prim(
        sphere(0.3, 3, glow(SIGN_AMBER, 2.6)),
        [fire[0], GROUND + 0.24, fire[2]],
        id_quat(),
    ));
    for (i, a) in [0.6_f32, 2.4, 4.3].into_iter().enumerate() {
        carried.push(prim(
            solid(cylinder_tapered(0.06, 1.0, 6, 0.0, board(HULL_OAK))),
            [
                fire[0] + 0.55 * a.cos(),
                GROUND + 0.5,
                fire[2] + 0.55 * a.sin(),
            ],
            quat_x(0.7 + i as f32 * 0.1),
        ));
    }

    // Stripped gear on the hard: her spars, a sail bundle, tar barrels.
    for (i, dz) in [0.0_f32, 0.55].into_iter().enumerate() {
        carried.push(prim(
            solid(cylinder_tapered(0.13, 7.0, 8, 0.35, board(HULL_OAK))),
            [
                3.2 + i as f32 * 0.4,
                GROUND + 0.16,
                SITE[2] * 0.5 - 5.0 + dz,
            ],
            quat_z(FRAC_PI_2),
        ));
    }
    carried.push(prim(
        solid(cylinder_tapered(
            0.42,
            3.2,
            10,
            0.0,
            sailcloth(CANVAS_BONE, CANVAS_SHADE),
        )),
        [-3.4, GROUND + 0.42, SITE[2] * 0.5 - 4.6],
        quat_z(FRAC_PI_2),
    ));
    for (i, dx) in [-5.4_f32, -4.5].into_iter().enumerate() {
        carried.push(prim(
            solid(cylinder_tapered(0.36, 0.8, 12, -0.08, tar(HULL_TAR))),
            [dx, GROUND + 0.4, SITE[2] * 0.5 - 2.4],
            id_quat(),
        ));
        carried.push(prim(
            torus(0.04, 0.37, iron(IRON_BLACK, 0xA7 + i as u32)),
            [dx, GROUND + 0.62, SITE[2] * 0.5 - 2.4],
            id_quat(),
        ));
    }

    let mut root = nest(
        prim(
            solid(cuboid_tapered(SITE, 0.0, shingle_bed)),
            site_c,
            id_quat(),
        ),
        carried,
    );
    attach(
        &mut root,
        fx::hearth_smoke([fire[0], GROUND + 1.0, fire[2]], 0xA0_11),
    );
    root.audio = fx::rigging_creak();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::measure;
    use crate::catalogue::items::util::{
        assert_no_glazing_on_solids, assert_no_tilted_parents, assert_sanitize_stable,
        blob_components, has_emissive, window_cards,
    };

    fn built() -> Generator {
        CareeningSlip.build("")
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&built(), "careening_slip");
    }

    /// #972 lesson 22, on the one entry in the kit that is genuinely about a
    /// tilted object.
    ///
    /// This is the guard the whole file is arranged around. A hove-down ship
    /// wants to be a rotated parent with a mast and a rig hanging off it, and
    /// that is precisely the shape that spins its children's offsets out of
    /// the geometry and then hides the fault from every other guard here —
    /// all of which walk translations only. The hull is one leaf `BlobGroup`
    /// and everything fixed to her is placed in the world frame by `heeled`,
    /// so the prop stays translation-only by construction.
    #[test]
    fn no_rotated_node_carries_an_offset_child() {
        assert_no_tilted_parents(&built(), "careening_slip");
    }

    /// The hull is one continuous skin.
    ///
    /// A `BlobGroup` whose elements drift out of blend range polygonises into
    /// pieces, and on a hull that means a bow floating clear of the midships.
    /// Union-find over the triangle graph is the only way to see it — the
    /// bounding box of a hull in two halves is the same as a whole one.
    #[test]
    fn the_hull_polygonises_as_a_single_mass() {
        use crate::pds::GeneratorKind;
        fn blobs(g: &Generator, out: &mut Vec<GeneratorKind>) {
            if matches!(g.kind, GeneratorKind::BlobGroup { .. }) {
                out.push(g.kind.clone());
            }
            for c in &g.children {
                blobs(c, out);
            }
        }
        let mut found = Vec::new();
        blobs(&built(), &mut found);
        assert_eq!(found.len(), 1, "the slip carries exactly one hull");
        assert_eq!(
            blob_components(&found[0]),
            1,
            "the hull polygonised into more than one piece — her stations have \
             drifted out of blend range, or she is finer than the sample grid \
             can resolve"
        );
    }

    /// She is hove down, not merely leaning — and not buried either.
    ///
    /// Three relationships, and getting them stated correctly is what caught
    /// the physical error in the first build. "Hove down" sounds like rolling
    /// about the keel with the keel still on the ground; it means the
    /// opposite. She rests on the **turn of her bilge**, the keel is lifted
    /// clear, and the far bottom is up in the air where a crew can reach it.
    /// Pivoting about a fixed keel instead drove the near bilge two metres
    /// into the beach, which no view here would have shown as anything except
    /// a slightly odd boat.
    #[test]
    fn she_rests_on_her_bilge_with_her_keel_lifted_clear() {
        let b = HULL_BEAM * 0.5;
        let d = HULL_DEPTH * 0.5;
        // The down-side bilge is ON the ways — measured on the ELLIPSE, at
        // the bearing where it actually touches, not at the box corner.
        // The bearing at which the ellipse actually touches. Minimising
        // `b·cosφ·sin + d·sinφ·cos` over φ gives `tanφ = (d·cos)/(b·sin)`,
        // taking the negative branch — which is NOT the same as the box
        // corner, and is the whole reason `hull_lift` uses a support function.
        let (sn, cs) = HEEL.sin_cos();
        let touch = (-(d * cs)).atan2(-(b * sn));
        let down = heeled(b * touch.cos(), d * touch.sin(), 0.0);
        assert!(
            (down[1] - WAY_TOP).abs() < 0.06,
            "the bilge she should be resting on is at {} and the ways are at \
             {WAY_TOP} — she is either floating or buried",
            down[1]
        );
        // The keel is OFF them, which is the entire object of the exercise.
        let keel = heeled(0.0, -d, 0.0);
        assert!(
            keel[1] > WAY_TOP + 0.55,
            "the keel is only {} above the ways — she is leaning, not careened",
            keel[1] - WAY_TOP
        );
        // And the far bottom is well up, where it can be worked on.
        let up = heeled(b, -d, 0.0);
        assert!(
            up[1] > keel[1] + b * 0.35,
            "the exposed bottom at {} is barely above the keel at {} — nothing \
             about this reads as a hull hove over",
            up[1],
            keel[1]
        );
        // She is not past careened into capsized: the masthead is still high.
        let head = heeled(0.0, HULL_DEPTH * 0.35 + 8.4, 1.2);
        assert!(
            head[1] > GROUND + 4.0,
            "the masthead is at {} — she has gone over",
            head[1]
        );
    }

    /// Bounds of the tree's single `BlobGroup` — the hull, and nothing else.
    fn walk_blob_bounds(root: &Generator) -> Option<measure::Bounds> {
        fn walk(g: &Generator, at: [f32; 3], out: &mut Option<measure::Bounds>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if matches!(g.kind, crate::pds::GeneratorKind::BlobGroup { .. }) {
                let mut xf = bevy::prelude::Transform::from_xyz(here[0], here[1], here[2]);
                xf.rotation = bevy::prelude::Quat::from_array(g.transform.rotation.0);
                *out = measure::mesh_bounds(&g.kind, &xf);
            }
            for c in &g.children {
                walk(c, here, out);
            }
        }
        let mut out = None;
        walk(root, [0.0; 3], &mut out);
        out
    }

    /// The hull actually sits on her own ways.
    ///
    /// Read out of the BUILT hull's mesh bounds rather than recomputed from
    /// `heeled` (#972 lesson 21): every other check in this file calls that
    /// function, so a guard that calls it too shares whatever the placement
    /// got wrong — and the placement did get it wrong, applying the lift
    /// twice and floating her a metre and a half over the slipway while the
    /// bilge-block guard passed happily. Coming at it from the mesh is the
    /// only direction that could see it.
    #[test]
    fn the_hull_rests_on_the_ways_she_is_hauled_out_on() {
        let g = built();
        // Selected as the only `BlobGroup` in the tree — what DEFINES the
        // hull. A first draft matched on "longer than 8.8 m in Z" and picked
        // up the site slab, which is sixteen metres long and sits on the
        // ground, so the guard reported the ship buried when she was floating
        // (#972 lesson 24: suspect the selector before the content).
        let hull = walk_blob_bounds(&g).expect("the hull is in the tree");
        assert!(
            (hull.min.y - WAY_TOP).abs() < 0.35,
            "the hull's lowest point is at {} and the ways are at {WAY_TOP} — \
             she is floating over her own slipway, or sunk into it",
            hull.min.y
        );
        // And she is over the ways, not beside them.
        assert!(
            hull.min.z > WAY_FAR - 0.5 && hull.max.z < WAY_NEAR + 0.5,
            "the hull runs {} .. {} and the ways {WAY_FAR} .. {WAY_NEAR}",
            hull.min.z,
            hull.max.z
        );
    }

    /// Every shore's head bears on the hull's own MESHED surface at its own
    /// station — not merely inside her bounding box.
    ///
    /// The bounding-box version of this guard passed while two shores stood
    /// in the air beside the bow (#1028): she narrows toward her ends, so a
    /// head at the amidships beam near the bow is well inside the AABB and
    /// nowhere near the ship. The only thing that can see that is the mesh —
    /// so this slices the polygonised hull at each shore's own z, takes the
    /// slice's outboard extremity, and demands the head land on it.
    ///
    /// The heads themselves are read from the BUILT struts via [`rotate_by`]
    /// (#972 lessons 21 and 23): recomputing them from `heeled` would share
    /// whatever the placement got wrong, which is precisely how the last
    /// floating-hull fault stayed invisible.
    #[test]
    fn every_shore_bears_on_the_hulls_own_surface() {
        use crate::catalogue::items::util::rotate_by;
        use crate::world_builder::build_primitive_mesh;
        use bevy::mesh::VertexAttributeValues;

        let g = built();

        // The meshed hull, in world space (the blob node carries the heel).
        struct Hull {
            verts: Vec<[f32; 3]>,
        }
        fn find_hull(g: &Generator, at: [f32; 3], out: &mut Option<Hull>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if matches!(g.kind, crate::pds::GeneratorKind::BlobGroup { .. }) {
                let mesh = build_primitive_mesh(&g.kind).mesh;
                let Some(VertexAttributeValues::Float32x3(pos)) =
                    mesh.attribute(bevy::prelude::Mesh::ATTRIBUTE_POSITION)
                else {
                    return;
                };
                let q = g.transform.rotation.0;
                let verts = pos
                    .iter()
                    .map(|p| {
                        let r = rotate_by(q, *p);
                        [here[0] + r[0], here[1] + r[1], here[2] + r[2]]
                    })
                    .collect();
                *out = Some(Hull { verts });
            }
            for c in &g.children {
                find_hull(c, here, out);
            }
        }
        let mut hull = None;
        find_hull(&g, [0.0; 3], &mut hull);
        let hull = hull.expect("the hull is in the tree");

        // The three shores: raking cylinder struts outboard of the keel line.
        fn shore_heads(g: &Generator, at: [f32; 3], out: &mut Vec<[f32; 3]>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let crate::pds::GeneratorKind::Cylinder { radius, height, .. } = &g.kind
                && (radius.0 - 0.16).abs() < 0.01
            {
                let q = g.transform.rotation.0;
                let tip = rotate_by(q, [0.0, height.0 * 0.5, 0.0]);
                let a = [here[0] + tip[0], here[1] + tip[1], here[2] + tip[2]];
                let b = [here[0] - tip[0], here[1] - tip[1], here[2] - tip[2]];
                // The head is the higher end; the foot is on the ground.
                out.push(if a[1] > b[1] { a } else { b });
            }
            for c in &g.children {
                shore_heads(c, here, out);
            }
        }
        let mut heads = Vec::new();
        shore_heads(&g, [0.0; 3], &mut heads);
        assert_eq!(
            heads.len(),
            3,
            "expected three shores, found {}",
            heads.len()
        );

        for head in heads {
            // Outboard extremity of the hull's own surface at this station.
            let slice_max_x = hull
                .verts
                .iter()
                .filter(|v| (v[2] - head[2]).abs() < 0.35)
                .map(|v| v[0])
                .fold(f32::MIN, f32::max);
            assert!(
                slice_max_x > f32::MIN,
                "no hull surface at all at z = {} — the shore props a station \
                 the ship does not reach",
                head[2]
            );
            assert!(
                head[0] < slice_max_x + 0.12 && head[0] > slice_max_x - 0.55,
                "a shore's head at x = {} does not bear on the hull, whose \
                 surface at z = {} ends at x = {slice_max_x} — a timber in \
                 the air beside her taper",
                head[0],
                head[2]
            );
            assert!(
                head[1] > WAY_TOP + 0.4,
                "a shore's head at y = {} is propping her below the bilge",
                head[1]
            );
        }
    }

    /// The tackle hangs from the top of a mast that is stepped in the hull —
    /// the three connections that make the rig make sense (#1030).
    ///
    /// What shipped was a rig whose every part was individually plausible:
    /// two falls converging on a point, a cap at the point, a mast-length
    /// timber crossing nearby. The missing property was CONNECTION — the
    /// falls' junction had no mast under it, because the mast prim carried
    /// the mirror of the turn `heeled()` performs and was reflected about
    /// its own midpoint. A symmetric hull hid the same mirror on itself.
    ///
    /// So the guard states the connections, all read from the BUILT tree via
    /// [`rotate_by`] (#972 lesson 21): both falls' high ends meet the mast's
    /// high end, and the mast's low end is inside the meshed hull — not near
    /// it, IN it, checked against the polygonised surface's own slice.
    #[test]
    fn the_tackle_hangs_from_a_mast_stepped_in_the_hull() {
        use crate::catalogue::items::util::rotate_by;
        use crate::pds::GeneratorKind as K;
        use crate::world_builder::build_primitive_mesh;
        use bevy::mesh::VertexAttributeValues;

        let g = built();

        // Every cylinder's two world-space ends, by radius class.
        fn ends_of(g: &Generator, at: [f32; 3], out: &mut Vec<(f32, [f32; 3], [f32; 3])>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let K::Cylinder { radius, height, .. } = &g.kind {
                let tip = rotate_by(g.transform.rotation.0, [0.0, height.0 * 0.5, 0.0]);
                out.push((
                    radius.0,
                    [here[0] + tip[0], here[1] + tip[1], here[2] + tip[2]],
                    [here[0] - tip[0], here[1] - tip[1], here[2] - tip[2]],
                ));
            }
            for c in &g.children {
                ends_of(c, here, out);
            }
        }
        let mut cyls = Vec::new();
        ends_of(&g, [0.0; 3], &mut cyls);

        // The mast: the one long oak stick (the shores are 0.16, the falls
        // 0.05, the posts are cuboids). Selected by radius — what defines it.
        let (_, a, b) = cyls
            .iter()
            .find(|(r, _, _)| (r - 0.22).abs() < 0.01)
            .copied()
            .expect("the mast is in the tree");
        let (mast_head, mast_foot) = if a[1] > b[1] { (a, b) } else { (b, a) };

        // Both falls' high ends land on the masthead.
        let falls: Vec<_> = cyls
            .iter()
            .filter(|(r, _, _)| (r - 0.05).abs() < 0.005)
            .collect();
        assert_eq!(falls.len(), 2, "expected two falls, found {}", falls.len());
        for (_, fa, fb) in &falls {
            let hi = if fa[1] > fb[1] { fa } else { fb };
            let d = ((hi[0] - mast_head[0]).powi(2)
                + (hi[1] - mast_head[1]).powi(2)
                + (hi[2] - mast_head[2]).powi(2))
            .sqrt();
            assert!(
                d < 0.4,
                "a fall's high end at {hi:?} is {d} m from the masthead at \
                 {mast_head:?} — the tackle is hooked to thin air"
            );
        }

        // The mast's foot is INSIDE the meshed hull at its own station.
        fn hull_verts(g: &Generator, at: [f32; 3], out: &mut Vec<[f32; 3]>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if matches!(g.kind, K::BlobGroup { .. }) {
                let mesh = build_primitive_mesh(&g.kind).mesh;
                if let Some(VertexAttributeValues::Float32x3(pos)) =
                    mesh.attribute(bevy::prelude::Mesh::ATTRIBUTE_POSITION)
                {
                    let q = g.transform.rotation.0;
                    out.extend(pos.iter().map(|p| {
                        let r = rotate_by(q, *p);
                        [here[0] + r[0], here[1] + r[1], here[2] + r[2]]
                    }));
                }
            }
            for c in &g.children {
                hull_verts(c, here, out);
            }
        }
        let mut verts = Vec::new();
        hull_verts(&g, [0.0; 3], &mut verts);
        let slice: Vec<_> = verts
            .iter()
            .filter(|v| (v[2] - mast_foot[2]).abs() < 0.4)
            .collect();
        assert!(!slice.is_empty(), "no hull at the mast's own station");
        let (mut min_x, mut max_x, mut max_y) = (f32::MAX, f32::MIN, f32::MIN);
        for v in slice {
            min_x = min_x.min(v[0]);
            max_x = max_x.max(v[0]);
            max_y = max_y.max(v[1]);
        }
        assert!(
            mast_foot[0] > min_x && mast_foot[0] < max_x && mast_foot[1] < max_y,
            "the mast's foot at {mast_foot:?} is outside the hull (x {min_x}..\
             {max_x}, deck {max_y}) — the mast is not stepped in the ship, \
             and the rig hangs from nothing"
        );
        // And the head is well above the foot on the DOWN side — the mast
        // rakes the way she is hove, which is what the mirror got wrong.
        assert!(
            mast_head[0] < mast_foot[0],
            "the masthead at x {} is upslope of its own foot at x {} — the \
             mast rakes against the heel",
            mast_head[0],
            mast_foot[0]
        );
    }

    /// The purchase actually spans masthead to post.
    ///
    /// Both falls are derived from one vector — the length and the angle come
    /// out of the same subtraction — which is the shape #972 lesson 21 asks
    /// for: a rope of a hand-picked length pointing roughly the right way
    /// looks fine from three of four angles.
    #[test]
    fn the_falls_reach_from_the_masthead_to_the_posts() {
        let head = heeled(0.0, HULL_DEPTH * 0.35 + 8.4, 1.2);
        for sx in [-1.0_f32, 1.0] {
            let top = [sx * POST_X, GROUND + POST_H, HULL_Z + 2.4];
            let v = [top[0] - head[0], top[1] - head[1], top[2] - head[2]];
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            assert!(
                len > 2.0 && len < 14.0,
                "a fall of {len} m is not a purchase, it is a mistake"
            );
        }
    }

    /// No glazing anywhere. A careening beach has no windows in it, and the
    /// prohibition is worth stating because the kit now has a card and the
    /// reflex to reach for one is what the ledger keeps catching.
    #[test]
    fn the_slip_carries_no_glazing() {
        let g = built();
        assert_no_glazing_on_solids(&g, "careening_slip");
        assert!(window_cards(&g).is_empty(), "a beach has grown a window");
        assert!(has_emissive(&g), "the slip lost its pitch fire");
    }

    /// Everything stands within the site it is nested under (#972 lesson 19).
    #[test]
    fn every_part_stands_within_the_site() {
        let g = built();
        let half = [SITE[0] * 0.5, SITE[2] * 0.5];
        let mut checked = 0;
        for p in measure::solids(&g) {
            checked += 1;
            assert!(
                p.bounds.min.x >= -half[0] - 1e-3 && p.bounds.max.x <= half[0] + 1e-3,
                "{} at {:?} overhangs the site in X ({} .. {})",
                p.kind_tag,
                p.bounds.center(),
                p.bounds.min.x,
                p.bounds.max.x
            );
            assert!(
                p.bounds.min.z >= -half[1] - 1e-3 && p.bounds.max.z <= half[1] + 1e-3,
                "{} at {:?} overhangs the site in Z ({} .. {})",
                p.kind_tag,
                p.bounds.center(),
                p.bounds.min.z,
                p.bounds.max.z
            );
        }
        assert!(checked > 20, "only {checked} parts examined");
    }
}
