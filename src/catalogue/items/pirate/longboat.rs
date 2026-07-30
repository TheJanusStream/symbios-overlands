//! Longboat — the ship's boat, chocked up on the quay with her gear stowed.
//!
//! A five-and-a-half-metre pulling boat standing upright on two chocks,
//! broadside to the approach: tarred hull, open sheer, floorboards, three
//! thwarts, four oars stowed fore-and-aft on them, her mast and sail bundle
//! unshipped alongside, a bailer, and her painter led ashore to a ring bolt.
//!
//! # The open boat, and why the mass stops at the sole
//!
//! An open boat is the one hull shape a `BlobGroup` cannot simply be *hollowed*
//! into. Surface nets misses any feature thinner than about two sample cells,
//! and at this length the cells are 125 mm — so a carved-out shell of hull
//! planking would come out as a colander, which is the flag's fault (#1026) in
//! a place where it would be far more visible.
//!
//! So the hull is split at the sole. Everything below is one blended mass —
//! the round of the bilge, the entry, the run aft, all continuous. Everything
//! above is *prim* strakes standing on the mass's own edge, which is what
//! makes her genuinely open: you look down past the gunwale onto floorboards
//! and thwarts, not onto the top of a solid lump.
//!
//! # One table of stations drives the whole boat
//!
//! [`STATIONS`] is the boat's lines plan — five `(x, half-beam)` pairs — and
//! *everything* is derived from it: the blob elements' extents, where the
//! sheer strakes run, how long each thwart is, how wide the chocks are, and
//! how far outboard an oar may be stowed. A boat narrows toward both ends, so
//! anything sized off the bounding box instead floats or pokes through near
//! the bow and the stern. That is the careening slip's shore fault (#1030) and
//! the same lesson as #972 lesson 18: the placement and the thing it is
//! derived from must be one expression.
//!
//! # The bloom is measured, not assumed
//!
//! A blended iso-surface stands proud of its authored elements by roughly the
//! **blend radius**, and the first build allowed for a full [`BLEND`] of it by
//! drawing every element that far inside the envelope. At these blends it is
//! nothing like that much: 120 mm of blend on a five-metre hull moves the mesh
//! about 25 mm, so she came out 240 mm narrow and floating 95 mm over her own
//! chocks. The elements are authored **on** the envelope, and
//! `the_meshed_hull_lands_on_its_own_stations` reads the built mesh's own bounds
//! and says so if that ever stops being true — which is the only way to hold a
//! number like this honestly.
//!
//! Do not read that as "the bloom is zero". It scales with the blend, and a
//! blend set as a fraction of the object's *span* rather than of its own
//! thickness gets large fast: the kit's flag carried a blend a tenth of its width
//! and meshed 12 % oversize, which #1031 found by asking a scaled instance to be
//! the height it was asked for. Small blends bloom negligibly; generous ones
//! bloom about as much as they measure.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    blob_box, blob_ellipsoid, blob_group, cuboid_tapered, cylinder_tapered, face_uv_offset,
    footing, id_quat, nest, prim, quat_x, quat_y, solid, strut, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::pds::generator::FaceKey;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRONZE_FITTING, CANVAS_BONE, CANVAS_SHADE, DECK_HOLY, HULL_OAK, HULL_TAR, IRON_BLACK,
    PORT_BAND, ROPE_HEMP, STONE_QUAY, WHARF_GREY, board, bronze, cobbles, fx, hemp, iron,
    sailcloth, strake, tar,
};

/// The paved stand — the sub-root every footprint guard measures against
/// (#972 lesson 19).
const PAD: [f32; 3] = [7.6, 0.24, 3.8];
const GROUND: f32 = PAD[1];

/// The boat's lines: `(x, half-beam at the sheer)`, bow first.
///
/// She lies along `X` so the approach sees her broadside — a boat bow-on is a
/// wedge, and the whole read here is the sheer line.
const STATIONS: [(f32, f32); 5] = [
    (2.74, 0.30),  // stem
    (1.68, 0.80),  // fore quarter
    (0.0, 0.95),   // midships
    (-1.68, 0.82), // aft quarter
    (-2.46, 0.56), // transom
];

/// Blend radius between the hull's elements — enough to melt five stations
/// into one sheer, and small enough that the meshed surface stays on the
/// stations the elements are drawn to.
const BLEND: f32 = 0.12;

/// Depth of the blended mass, keel to sole, in the boat's own frame (`y = 0`
/// is the underside of the keel).
const MASS_T: f32 = 0.56;

/// Half-length of the transom element, and of the bow's — the two narrowest
/// things in the hull, and therefore what the sample grid has to resolve.
const TRANSOM_HALF: f32 = 0.25;
const BOW_HALF: f32 = 0.5;

/// Top of the gunwale above the keel. The sheer strakes fill `MASS_T..SHEER`.
const SHEER: f32 = 1.0;

/// Thickness of a sheer strake, and of the transom.
const PLANK_T: f32 = 0.06;

/// Height a thwart's top sits above the keel, and the board's own section.
const THWART_Y: f32 = MASS_T + 0.3;
const THWART_T: f32 = 0.06;
const THWART_W: f32 = 0.26;

/// Top of the floorboards — what the gear in her bottom stands on.
const SOLE_TOP: f32 = MASS_T + 0.055;

/// Sample resolution for the hull. She is five and a half metres long, so even
/// at 44 the cells are 125 mm and nothing thinner than a quarter of a metre
/// survives — see the shared `blob_cell_size` note in `items::util`.
const HULL_RES: u32 = 44;

/// Chock height, and the `X` stations the two chocks stand at.
const CHOCK_H: f32 = 0.5;
const CHOCK_X: [f32; 2] = [-1.25, 1.25];

/// Where the keel sits in the world: on top of the chocks.
const KEEL_Y: f32 = GROUND + CHOCK_H;

/// Where the painter is made fast on the stones, forward of the stem.
const RING_X: f32 = 3.3;

/// The rope the painter is laid up in, and the radius its guard selects on.
const PAINTER_R: f32 = 0.035;

const _: () = assert!(
    STATIONS[0].0 - BOW_HALF * 2.0 < STATIONS[1].0 + (STATIONS[0].0 - STATIONS[1].0) * 0.62,
    "the bow element no longer overlaps the fore quarter — the hull will \
     polygonise in pieces"
);
const _: () = assert!(
    STATIONS[4].0 + TRANSOM_HALF * 2.0
        > STATIONS[3].0 - (STATIONS[4].0 - STATIONS[3].0).abs() * 0.9,
    "the transom element no longer overlaps the aft quarter"
);
const _: () = assert!(
    RING_X < PAD[0] * 0.5,
    "the painter's ring bolt is off the stand"
);
const _: () = assert!(
    THWART_Y + THWART_T * 0.5 < SHEER,
    "the thwarts stand above the gunwale"
);

pub struct Longboat;

impl CatalogueEntry for Longboat {
    fn slug(&self) -> &'static str {
        "longboat"
    }
    fn name(&self) -> &'static str {
        "Longboat"
    }
    fn description(&self) -> &'static str {
        "A ship's boat chocked upright on the quay, oars stowed on her thwarts and her mast \
         unshipped alongside."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Pirate]
    }
    fn prosperity_band(&self) -> ProsperityBand {
        PORT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 4.4,
            min_spawn_dist: 15.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Half-beam at the sheer at station `x`, interpolated along [`STATIONS`].
///
/// The one function that answers "how wide is she here", which is the question
/// every thwart, oar, chock and thole pin in this file has to ask. A boat
/// sized off its bounding box instead has a thwart through its own planking
/// near the ends, and that is the fault this kit has already paid for at
/// eleven-metre scale (#1030).
fn beam_at(x: f32) -> f32 {
    if x >= STATIONS[0].0 {
        return STATIONS[0].1;
    }
    let last = STATIONS[STATIONS.len() - 1];
    if x <= last.0 {
        return last.1;
    }
    for w in STATIONS.windows(2) {
        let ((x0, b0), (x1, b1)) = (w[0], w[1]);
        if x <= x0 && x >= x1 {
            let t = (x0 - x) / (x0 - x1);
            return b0 + (b1 - b0) * t;
        }
    }
    STATIONS[2].1
}

/// Turn a point in the boat's own frame — `y` up from the keel — into the
/// world. One expression, so nothing here can disagree with the chocks about
/// how high she stands (#972 lesson 18).
fn aboard(x: f32, y: f32, z: f32) -> [f32; 3] {
    [x, KEEL_Y + y, z]
}

/// The hull below the sole, as one blended mass.
///
/// Every element is drawn to the envelope [`STATIONS`] describes, and every
/// element **overlaps** its neighbours structurally rather than trusting the
/// blend to bridge a gap — which is what left the careening slip's stern
/// floating astern of her own hull on the first build. All the bottoms sit at
/// exactly `y = 0`, so the underside comes out flat and the chocks meet it.
fn hull() -> Generator {
    let (stem_x, stem_b) = STATIONS[0];
    let (fore_x, fore_b) = STATIONS[1];
    let mid_b = STATIONS[2].1;
    let (aft_x, aft_b) = STATIONS[3];
    let (tran_x, tran_b) = STATIONS[4];

    let elements = vec![
        // Midships body — flat-bottomed, running out to just short of the
        // quarters.
        blob_box(
            [0.0, MASS_T * 0.5, 0.0],
            [fore_x * 0.97, MASS_T * 0.5, mid_b],
            BLEND,
        ),
        // Fore quarter, drawing in toward the entry.
        blob_ellipsoid(
            [fore_x * 1.05, MASS_T * 0.55, 0.0],
            [(stem_x - fore_x) * 0.62, MASS_T * 0.55, fore_b],
            BLEND,
        ),
        // Bow: a fine entry, and the forefoot standing a little proud of the
        // sole — which is the stem knee, and reads as one.
        blob_ellipsoid(
            [stem_x - BOW_HALF, MASS_T * 0.62, 0.0],
            [BOW_HALF, MASS_T * 0.62, stem_b],
            BLEND,
        ),
        // Aft quarter — fuller than the fore, as a pulling boat's run is.
        blob_ellipsoid(
            [aft_x * 1.03, MASS_T * 0.55, 0.0],
            [(tran_x - aft_x).abs() * 0.9, MASS_T * 0.55, aft_b],
            BLEND,
        ),
        // Transom: square, where the bow is fine.
        blob_box(
            [tran_x + TRANSOM_HALF, MASS_T * 0.52, 0.0],
            [TRANSOM_HALF, MASS_T * 0.52, tran_b],
            BLEND,
        ),
    ];
    prim(
        blob_group(elements, HULL_RES, strake(HULL_TAR)),
        aboard(0.0, 0.0, 0.0),
        id_quat(),
    )
}

/// One board of the sheer, from station `a` to station `b` on side `sz`.
///
/// The file's **only** direction-to-rotation conversion, and it is here rather
/// than inline because six hand-rolled ones across the earlier entries were
/// wrong (#1028). A board's long axis is its local `+Z`, and `quat_y(θ)`
/// carries `+Z` to `(sin θ, 0, cos θ)`, so `θ = atan2(dx, dz)` in the plan —
/// taken from the run itself, never from a guessed angle.
///
/// `section` is `[across, up]`, which is what lets the strake and the capping
/// that lands on it share one function: a strake is thin and tall, a capping
/// wide and flat, and both follow exactly the same sheer line.
fn sheer_board(
    a: (f32, f32),
    b: (f32, f32),
    sz: f32,
    y: f32,
    section: [f32; 2],
    material: crate::pds::SovereignMaterialSettings,
) -> Generator {
    let p0 = [a.0, sz * a.1];
    let p1 = [b.0, sz * b.1];
    let (dx, dz) = (p1[0] - p0[0], p1[1] - p0[1]);
    let len = (dx * dx + dz * dz).sqrt().max(1e-4);
    prim(
        solid(cuboid_tapered([section[0], section[1], len], 0.0, material)),
        aboard((p0[0] + p1[0]) * 0.5, y, (p0[1] + p1[1]) * 0.5),
        quat_y(dx.atan2(dz)),
    )
}

/// Width and thickness of the gunwale capping — the board laid flat along the
/// top of the sheer strake.
///
/// It exists for a reason worth stating: tarred planking is very dark, so a
/// boat built entirely in it reads from the side as one black silhouette with
/// no sheer at all. A capping in bleached deck timber draws the line the eye
/// actually uses to read a hull's shape, and it is a real fitting rather than
/// a stripe painted on to fix a render — every open boat has one, because it
/// is what covers the top edge of the planking.
const CAP: [f32; 2] = [0.15, 0.05];

/// The open topsides: sheer strakes both sides under a capping, a transom, and
/// a stem post.
fn topsides() -> Vec<Generator> {
    let h = SHEER - MASS_T;
    let y = MASS_T + h * 0.5;
    let cap_y = SHEER + CAP[1] * 0.5;
    let mut out = Vec::new();
    for sz in [-1.0_f32, 1.0] {
        for w in STATIONS.windows(2) {
            out.push(sheer_board(
                w[0],
                w[1],
                sz,
                y,
                [PLANK_T, h],
                strake(HULL_TAR),
            ));
            out.push(sheer_board(w[0], w[1], sz, cap_y, CAP, board(DECK_HOLY)));
        }
    }
    // Transom, closing the run aft between the two sheer strakes, capped to
    // match so the sheer runs unbroken right round her.
    let (tran_x, tran_b) = STATIONS[4];
    out.push(prim(
        solid(cuboid_tapered(
            [PLANK_T, h, tran_b * 2.0],
            0.0,
            board(HULL_OAK),
        )),
        aboard(tran_x, y, 0.0),
        quat_y(FRAC_PI_2),
    ));
    out.push(prim(
        solid(cuboid_tapered(
            [CAP[0], CAP[1], tran_b * 2.0],
            0.0,
            board(DECK_HOLY),
        )),
        aboard(tran_x, cap_y, 0.0),
        quat_y(FRAC_PI_2),
    ));
    // Stem post, standing a little above the gunwale — where the painter is
    // made fast, so it has to exist before the rope can reach it.
    //
    // Narrow athwartships, and that is the whole read. The first build sized it
    // off the stem station's half-beam and got a 480 mm plate standing above her
    // bow, which from the approach is a signboard rather than a stem head — the
    // battery's canvas apron fault (#1025) in a smaller place.
    out.push(prim(
        solid(cuboid_tapered(
            [0.16, h + 0.14, 0.22],
            0.15,
            board(HULL_OAK),
        )),
        aboard(STATIONS[0].0, y + 0.07, 0.0),
        id_quat(),
    ));
    out
}

/// Clear half-width inside her planking at station `x`.
fn inner_at(x: f32) -> f32 {
    beam_at(x) - PLANK_T * 0.5
}

/// The narrowest clear half-width across a piece spanning `x0..x1`.
///
/// The whole reason this exists: a boat narrows toward both ends, so a board
/// sized on the beam at its *centre* is inboard amidships and through her
/// planking at its own ends. The first build laid the sole as one 3.9 m panel
/// on the midships beam and it reached 80 mm outside her side at the after end.
/// One expression, so every fitted board asks the same question (#972
/// lesson 18).
fn fitted_half(x0: f32, x1: f32) -> f32 {
    inner_at(x0).min(inner_at(x1)) - 0.015
}

/// Floorboards and thwarts — the fit-out that makes her read as open.
fn fit_out() -> Vec<Generator> {
    let mut out = Vec::new();
    // Sole: three panels of floorboards over the mass, in bleached deck timber
    // against the tar — so the inside of the boat is a lit surface rather than
    // a black hole under the sheer. Each panel is cut to the narrower of its
    // own two ends.
    for (x0, x1) in [(-2.1_f32, -1.1_f32), (-1.1, 0.6), (0.6, 2.0)] {
        out.push(prim(
            solid(cuboid_tapered(
                [x1 - x0, 0.05, fitted_half(x0, x1) * 2.0],
                0.0,
                board(DECK_HOLY),
            )),
            aboard((x0 + x1) * 0.5, MASS_T + 0.03, 0.0),
            id_quat(),
        ));
    }
    for x in [1.1_f32, -0.1, -1.3] {
        let half = fitted_half(x - THWART_W * 0.5, x + THWART_W * 0.5);
        out.push(prim(
            solid(cuboid_tapered(
                [THWART_W, THWART_T, half * 2.0],
                0.0,
                board(DECK_HOLY),
            )),
            aboard(x, THWART_Y - THWART_T * 0.5, 0.0),
            id_quat(),
        ));
    }
    out
}

/// Four oars stowed fore-and-aft on the thwarts, blades aft.
///
/// Struts, so a shaft runs between two points that both exist. The `z`
/// stations are checked against [`beam_at`] at both ends by the guard: an oar
/// stowed 620 mm off the centreline is inside the boat amidships and through
/// her planking three-quarters of the way aft, which is exactly the shape of
/// mistake the careening slip's shores made.
fn oars() -> Vec<Generator> {
    let y = THWART_Y + 0.045;
    let (aft, fwd) = (-1.8_f32, 1.95);
    let mut out = Vec::new();
    for z in [-0.48_f32, -0.26, 0.26, 0.48] {
        out.push(strut(
            aboard(aft, y, z),
            aboard(fwd, y, z),
            0.045,
            6,
            board(DECK_HOLY),
        ));
        // The blade — flat, and the piece that makes a dowel read as an oar.
        out.push(prim(
            solid(cuboid_tapered([0.5, 0.03, 0.15], 0.2, board(DECK_HOLY))),
            aboard(aft - 0.15, y, z),
            id_quat(),
        ));
    }
    out
}

/// Her mast and sail bundle, unshipped and stowed, with the halyard coiled on
/// the sole.
///
/// The mast lies **on** the thwarts and the bundle **under** them, which is how
/// a boat's spars actually stow and also the only arrangement that fits: a
/// 220 mm sail bundle laid on top of the thwarts stands proud of the gunwale,
/// and one laid on the sole at that diameter drives up through them.
fn spars() -> Vec<Generator> {
    vec![
        strut(
            aboard(-2.1, THWART_Y + 0.07, 0.0),
            aboard(2.4, THWART_Y + 0.07, 0.0),
            0.07,
            8,
            board(HULL_OAK),
        ),
        strut(
            aboard(-1.6, SOLE_TOP + 0.11, 0.5),
            aboard(1.8, SOLE_TOP + 0.11, 0.5),
            0.11,
            8,
            sailcloth(CANVAS_BONE, CANVAS_SHADE),
        ),
        prim(
            torus(0.045, 0.24, hemp(ROPE_HEMP)),
            aboard(1.35, SOLE_TOP + 0.045, -0.4),
            id_quat(),
        ),
    ]
}

fn build_tree() -> Generator {
    let pad_c = [0.0, GROUND * 0.5, 0.0];
    let mut paving = cobbles(STONE_QUAY, 0xF8);
    paving.uv_offset = face_uv_offset(FaceKey::Top, pad_c);

    let mut carried = vec![footing(PAD[0] * 0.8, PAD[2] * 0.8, [0.0, 0.0], 4.4)];

    // The chocks she stands on. Their tops are the keel line by construction,
    // and their width comes from her own beam at their station.
    for x in CHOCK_X {
        carried.push(prim(
            solid(cuboid_tapered(
                [0.42, CHOCK_H, beam_at(x) * 1.25],
                0.22,
                board(WHARF_GREY),
            )),
            [x, GROUND + CHOCK_H * 0.5, 0.0],
            id_quat(),
        ));
    }

    carried.push(hull());
    carried.extend(topsides());
    carried.extend(fit_out());
    carried.extend(oars());
    carried.extend(spars());

    // Thole pins on the gunwale, one pair per oar — the fitting that says she
    // is pulled rather than towed. On the sheer line, so they follow the same
    // stations the strakes do.
    for (i, x) in [1.35_f32, 0.55, -0.35, -1.15].into_iter().enumerate() {
        for sz in [-1.0_f32, 1.0] {
            carried.push(prim(
                solid(cylinder_tapered(
                    0.028,
                    0.16,
                    6,
                    0.1,
                    iron(IRON_BLACK, 0xA0 + i as u32),
                )),
                aboard(x, SHEER + CAP[1] + 0.08, sz * beam_at(x)),
                id_quat(),
            ));
        }
    }

    // A bailer standing on the sole, and a can of tar — a boat with nothing in
    // her is a hull, not a boat that gets used.
    carried.push(prim(
        solid(cylinder_tapered(0.16, 0.26, 10, -0.1, board(DECK_HOLY))),
        aboard(-1.95, SOLE_TOP + 0.13, 0.0),
        id_quat(),
    ));
    carried.push(prim(
        solid(cylinder_tapered(
            0.13,
            0.2,
            10,
            0.05,
            iron(IRON_BLACK, 0xA6),
        )),
        aboard(-0.75, SOLE_TOP + 0.1, -0.5),
        id_quat(),
    ));
    carried.push(prim(
        solid(cylinder_tapered(0.115, 0.03, 10, 0.0, tar(HULL_TAR))),
        aboard(-0.75, SOLE_TOP + 0.19, -0.5),
        id_quat(),
    ));

    // The painter: made fast at the stem head, down to a ring bolt in the
    // paving. A strut, because a rope that points *near* its ring is the fault
    // this kit retired (#1028, #1030).
    let head = aboard(STATIONS[0].0, SHEER + 0.1, 0.0);
    let ring = [RING_X, GROUND + 0.04, 0.0];
    carried.push(strut(head, ring, PAINTER_R, 6, hemp(ROPE_HEMP)));
    carried.push(prim(
        torus(0.03, 0.11, iron(IRON_BLACK, 0xA7)),
        ring,
        quat_x(FRAC_PI_2),
    ));
    carried.push(prim(
        torus(0.05, 0.3, hemp(ROPE_HEMP)),
        [RING_X - 0.55, GROUND + 0.05, 0.62],
        id_quat(),
    ));

    // A caulking mallet and a pot of pitch on the stones under her bilge —
    // placed off the PAD's own half-extent, so a retuned stand cannot leave
    // them hanging off it (#972 lesson 8).
    let gear_z = PAD[2] * 0.5 - 0.62;
    carried.push(prim(
        solid(cylinder_tapered(
            0.17,
            0.24,
            10,
            0.06,
            bronze(BRONZE_FITTING, 0xA8),
        )),
        [-0.4, GROUND + 0.12, -gear_z],
        id_quat(),
    ));
    carried.push(strut(
        [0.6, GROUND + 0.06, -gear_z - 0.2],
        [1.35, GROUND + 0.06, -gear_z + 0.15],
        0.055,
        6,
        board(HULL_OAK),
    ));

    let mut root = nest(
        prim(solid(cuboid_tapered(PAD, 0.0, paving)), pad_c, id_quat()),
        carried,
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
        blob_cell_size, blob_components, rotate_by, window_cards,
    };
    use crate::pds::GeneratorKind as K;

    fn built() -> Generator {
        Longboat.build("")
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&built(), "longboat");
    }

    #[test]
    fn no_rotated_node_carries_an_offset_child() {
        assert_no_tilted_parents(&built(), "longboat");
    }

    #[test]
    fn the_boat_carries_no_glazing() {
        let g = built();
        assert_no_glazing_on_solids(&g, "longboat");
        assert!(window_cards(&g).is_empty(), "a longboat has grown a window");
    }

    /// World bounds of the tree's single `BlobGroup` — the hull, meshed.
    fn hull_bounds() -> measure::Bounds {
        fn walk(g: &Generator, at: [f32; 3], out: &mut Option<measure::Bounds>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if matches!(g.kind, K::BlobGroup { .. }) {
                let mut xf = bevy::prelude::Transform::from_xyz(here[0], here[1], here[2]);
                xf.rotation = bevy::prelude::Quat::from_array(g.transform.rotation.0);
                *out = measure::mesh_bounds(&g.kind, &xf);
            }
            for c in &g.children {
                walk(c, here, out);
            }
        }
        let mut out = None;
        walk(&built(), [0.0; 3], &mut out);
        out.expect("the boat carries a meshed hull")
    }

    /// The hull is one continuous skin, and thick enough for the grid to hold
    /// it.
    ///
    /// Two failures, one test, because they have the same symptom and opposite
    /// causes: elements that drift out of blend range polygonise into separate
    /// pieces, and a mass thinner than two sample cells polygonises *with
    /// holes*. Union-find over the triangle graph is the only thing that sees
    /// either — a hull in two halves has the same bounding box as a whole one,
    /// which is exactly how the flag shipped (#1026).
    #[test]
    fn the_hull_polygonises_as_a_single_mass() {
        fn blobs(g: &Generator, out: &mut Vec<K>) {
            if matches!(g.kind, K::BlobGroup { .. }) {
                out.push(g.kind.clone());
            }
            for c in &g.children {
                blobs(c, out);
            }
        }
        let mut found = Vec::new();
        blobs(&built(), &mut found);
        assert_eq!(found.len(), 1, "the boat carries exactly one hull");
        assert_eq!(
            blob_components(&found[0]),
            1,
            "the hull polygonised into more than one piece — her stations have \
             drifted out of blend range, or she is finer than the sample grid"
        );
        // And the arithmetic that has to hold for that to keep being true.
        let cell = blob_cell_size(hull_bounds().size().x, HULL_RES);
        let thinnest = TRANSOM_HALF.min(STATIONS[0].1).min(MASS_T * 0.5) * 2.0;
        assert!(
            thinnest > cell * 2.0,
            "the thinnest authored element is {thinnest} m across a {cell} m \
             sample cell — under two cells it comes out full of holes"
        );
    }

    /// She stands on her chocks, and her sole is where the fit-out expects.
    ///
    /// Read off the BUILT hull's mesh bounds, not recomputed from the
    /// constants — the bloom is the thing under test, and the whole point of
    /// authoring every element `BLEND` inside the envelope is that the meshed
    /// underside comes out flat on the chock tops. Measured from the chock up,
    /// which is the opposite direction to the placement (#972 lesson 21).
    #[test]
    fn the_hull_sits_on_its_chocks_with_its_sole_at_the_thwarts() {
        let hull = hull_bounds();
        let chock_top = GROUND + CHOCK_H;
        assert!(
            (hull.min.y - chock_top).abs() < 0.07,
            "the hull's underside is at {} and the chocks top out at \
             {chock_top} — she is floating or bedded in",
            hull.min.y
        );
        // The mass has to reach the sole, or the floorboards hang in the air
        // inside her.
        let sole = KEEL_Y + MASS_T;
        assert!(
            hull.max.y > sole - 0.06,
            "the hull mass tops out at {} but the floorboards are laid at \
             {sole} — the sole is over a void",
            hull.max.y
        );
        // ...and not through the gunwale, or she is not an open boat at all.
        assert!(
            hull.max.y < KEEL_Y + SHEER - 0.02,
            "the hull mass reaches {} — at or above the gunwale at {}, which \
             fills the boat in",
            hull.max.y,
            KEEL_Y + SHEER
        );
    }

    /// The meshed hull lands on the beam and length its stations describe.
    ///
    /// This is the guard on [`BLEND`] being the right bloom allowance: author
    /// too far inside and the sheer strakes stand off her side with daylight
    /// between, author too near and the mass swells out through them.
    #[test]
    fn the_meshed_hull_lands_on_its_own_stations() {
        let hull = hull_bounds();
        let beam = STATIONS[2].1;
        assert!(
            (hull.max.z - beam).abs() < 0.09 && (hull.min.z + beam).abs() < 0.09,
            "the meshed hull is {} .. {} in z against a station half-beam of \
             {beam} — the bloom allowance no longer matches the elements",
            hull.min.z,
            hull.max.z
        );
        assert!(
            (hull.max.x - STATIONS[0].0).abs() < 0.14,
            "her stem meshes at {} against a station at {}",
            hull.max.x,
            STATIONS[0].0
        );
        assert!(
            (hull.min.x - STATIONS[4].0).abs() < 0.14,
            "her transom meshes at {} against a station at {}",
            hull.min.x,
            STATIONS[4].0
        );
    }

    /// Every sheer strake runs from one station to the next, on the station
    /// line.
    ///
    /// Read from the built prims through [`rotate_by`], because this file's one
    /// direction-to-rotation conversion is the fault class that cost the kit
    /// six separate fixes (#1028): a plank of the right length at the wrong
    /// heading reads as correct from three of four angles.
    #[test]
    fn the_sheer_strakes_join_station_to_station() {
        let mut tips: Vec<[f32; 3]> = Vec::new();
        // Selected on the tarred ship-planking that the sheer strakes are made
        // of, NOT on their section: the transom is the same 60 mm board of the
        // same height, and the first version of this guard picked it up and
        // then complained that a board lying athwartships did not end on a
        // station line. #972 lesson 24 — select on what defines the thing.
        fn walk(g: &Generator, at: [f32; 3], out: &mut Vec<[f32; 3]>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let K::Cuboid { size, material, .. } = &g.kind
                && (size.0[0] - PLANK_T).abs() < 1e-4
                && material.base_color.0 == HULL_TAR
            {
                let half = rotate_by(g.transform.rotation.0, [0.0, 0.0, size.0[2] * 0.5]);
                out.push([here[0] + half[0], here[1] + half[1], here[2] + half[2]]);
                out.push([here[0] - half[0], here[1] - half[1], here[2] - half[2]]);
            }
            for c in &g.children {
                walk(c, here, out);
            }
        }
        walk(&built(), [0.0; 3], &mut tips);
        let expected = (STATIONS.len() - 1) * 2;
        assert_eq!(
            tips.len(),
            expected * 2,
            "expected {expected} sheer strakes, found {}",
            tips.len() / 2
        );
        for tip in &tips {
            let want = beam_at(tip[0]);
            assert!(
                (tip[2].abs() - want).abs() < 0.02,
                "a strake ends at {tip:?}, where her half-beam is {want} — the \
                 board is off the station line"
            );
            assert!(
                (tip[1] - (KEEL_Y + MASS_T + (SHEER - MASS_T) * 0.5)).abs() < 1e-3,
                "a strake end is at y = {} rather than on the sheer band",
                tip[1]
            );
            assert!(
                tip[0] >= STATIONS[4].0 - 1e-3 && tip[0] <= STATIONS[0].0 + 1e-3,
                "a strake reaches x = {} — past her own stem or transom",
                tip[0]
            );
        }
        // Every station is met from both directions, so the run is continuous
        // rather than four boards that happen to be near the right places.
        for (x, _) in STATIONS {
            let ends = tips.iter().filter(|t| (t[0] - x).abs() < 0.02).count();
            let want = if x == STATIONS[0].0 || x == STATIONS[4].0 {
                2
            } else {
                4
            };
            assert_eq!(
                ends, want,
                "station x = {x} is met by {ends} strake ends, not {want}"
            );
        }
    }

    /// Nothing stowed aboard reaches through her planking.
    ///
    /// Thwarts, oars, spars and the gear in her bottom are all checked against
    /// [`beam_at`] **at their own extremes**, which is where the fault lives: a
    /// piece sized on the midships beam is inboard amidships and outboard at
    /// the quarters.
    #[test]
    fn nothing_stowed_aboard_pokes_through_her_side() {
        let mut checked = 0;
        for p in measure::solids(&built()) {
            // Only what is stowed INSIDE her: standing on the sole or higher,
            // and wholly below the gunwale. The thole pins live on the sheer
            // line by definition and the strakes ARE the sheer line, so both
            // are above this band and neither is a fitting.
            if p.bounds.min.y < KEEL_Y + MASS_T - 0.02 || p.bounds.max.y > KEEL_Y + SHEER + 0.01 {
                continue;
            }
            // ...and it is a *fitting*, not the planking. A sheer strake fills
            // the whole freeboard, so its own section is what tells it apart
            // from everything stowed against it — and a strake sits ON the
            // station line, which is exactly what this guard forbids of a
            // fitting.
            if p.bounds.size().y >= SHEER - MASS_T - 1e-3 {
                continue;
            }
            checked += 1;
            for x in [p.bounds.min.x, p.bounds.max.x, p.bounds.center().x] {
                let room = inner_at(x.clamp(STATIONS[4].0, STATIONS[0].0));
                assert!(
                    p.bounds.max.z <= room + 1e-3 && p.bounds.min.z >= -room - 1e-3,
                    "{} at {:?} spans z {} .. {} where her inner half-beam at \
                     x = {x} is only {room}",
                    p.kind_tag,
                    p.bounds.center(),
                    p.bounds.min.z,
                    p.bounds.max.z
                );
            }
        }
        assert!(
            checked >= 10,
            "only {checked} pieces of fit-out were examined"
        );
    }

    /// The painter runs from the stem head to its ring bolt in the paving.
    #[test]
    fn the_painter_reaches_its_ring_bolt() {
        let mut found = Vec::new();
        fn ropes(g: &Generator, at: [f32; 3], out: &mut Vec<([f32; 3], [f32; 3])>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let K::Cylinder { radius, height, .. } = &g.kind
                && (radius.0 - PAINTER_R).abs() < 0.003
            {
                let tip = rotate_by(g.transform.rotation.0, [0.0, height.0 * 0.5, 0.0]);
                out.push((
                    [here[0] + tip[0], here[1] + tip[1], here[2] + tip[2]],
                    [here[0] - tip[0], here[1] - tip[1], here[2] - tip[2]],
                ));
            }
            for c in &g.children {
                ropes(c, here, out);
            }
        }
        ropes(&built(), [0.0; 3], &mut found);
        assert_eq!(
            found.len(),
            1,
            "expected one painter, found {}",
            found.len()
        );
        let (a, b) = found[0];
        let (hi, lo) = if a[1] > b[1] { (a, b) } else { (b, a) };
        assert!(
            (hi[0] - STATIONS[0].0).abs() < 0.06 && hi[1] > KEEL_Y + SHEER,
            "the painter's upper end at {hi:?} is not made fast at her stem \
             head"
        );
        assert!(
            (lo[0] - RING_X).abs() < 0.02 && lo[1] < GROUND + 0.1,
            "the painter's lower end at {lo:?} is not at its ring bolt"
        );
    }

    /// Nothing overhangs the stand it is nested under (#972 lessons 8, 19).
    #[test]
    fn every_part_stands_on_the_pad() {
        let half = [PAD[0] * 0.5, PAD[2] * 0.5];
        let mut checked = 0;
        for p in measure::solids(&built()) {
            checked += 1;
            assert!(
                p.bounds.min.x >= -half[0] - 1e-3 && p.bounds.max.x <= half[0] + 1e-3,
                "{} at {:?} overhangs the stand in X ({} .. {})",
                p.kind_tag,
                p.bounds.center(),
                p.bounds.min.x,
                p.bounds.max.x
            );
            assert!(
                p.bounds.min.z >= -half[1] - 1e-3 && p.bounds.max.z <= half[1] + 1e-3,
                "{} at {:?} overhangs the stand in Z ({} .. {})",
                p.kind_tag,
                p.bounds.center(),
                p.bounds.min.z,
                p.bounds.max.z
            );
        }
        assert!(checked > 25, "only {checked} parts examined");
    }

    /// The gear on the stones stands clear of the chocks she is standing on.
    ///
    /// Penetration, not contact: a chock bedded on the paving shares a plane
    /// with it, and that is what standing on something is.
    #[test]
    fn the_gear_on_the_stones_stands_clear() {
        /// How far two boxes must interpenetrate on every axis before it is a
        /// fault rather than two things touching.
        const BITE: f32 = 0.02;
        let solids = measure::solids(&built());
        let chocks: Vec<_> = solids
            .iter()
            .filter(|p| p.kind_tag == "Cuboid" && (p.bounds.size().y - CHOCK_H).abs() < 0.01)
            .collect();
        assert_eq!(
            chocks.len(),
            CHOCK_X.len(),
            "expected {} chocks, found {}",
            CHOCK_X.len(),
            chocks.len()
        );
        let gear: Vec<_> = solids
            .iter()
            .filter(|p| p.bounds.min.y < GROUND + 0.06 && p.bounds.size().x < 3.0)
            .filter(|p| !chocks.iter().any(|c| c.path == p.path))
            .collect();
        assert!(
            gear.len() >= 4,
            "only {} loose pieces found on the stones",
            gear.len()
        );
        for g in &gear {
            for c in &chocks {
                let bite = |ax: usize| {
                    let (ga, gb) = (g.bounds.min.to_array(), g.bounds.max.to_array());
                    let (ca, cb) = (c.bounds.min.to_array(), c.bounds.max.to_array());
                    gb[ax].min(cb[ax]) - ga[ax].max(ca[ax])
                };
                assert!(
                    !(0..3).all(|ax| bite(ax) > BITE),
                    "{} at {:?} is driven into the chock at {:?}",
                    g.kind_tag,
                    g.bounds.center(),
                    c.bounds.center()
                );
            }
        }
    }
}
