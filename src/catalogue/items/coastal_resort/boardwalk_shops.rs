//! Boardwalk shops — a Coastal-Resort secondary. A short plank promenade
//! lined with three stucco kiosks under striped awnings, their serving
//! hatches open onto lit counters: the ice-cream, postcard and beach-tat
//! stalls of the strip.
//!
//! Rebuilt as a shell under #972, and the interesting decision here is what
//! the shopfront *is*. It used to be a `Window`-textured slab hung in front of
//! a solid stucco box — a frame with holes onto the render behind it, with
//! nothing to see through it. The obvious repair is a card over a real
//! opening; the better one is **no glazing at all**. A seafront kiosk serves
//! over a counter through an open hatch, so the hatch is a genuine hole, the
//! goods behind it are the point, and the alpha-card idiom never enters into
//! it. The awning is what closes the stall at night, which is also why it is
//! the biggest thing on the prop.
//!
//! The rest is the usual ledger: the counter and the head band *frame* the
//! opening rather than being laid on it, each kiosk is laid out with its own
//! stock (#972 lesson 9 — three identical stalls is one stall rendered three
//! times), the back rail is a [`util::railing`] instead of a bar on two posts,
//! and the tree stands the way the boardwalk does.
//!
//! [`util::railing`]: crate::catalogue::items::util::railing

use crate::catalogue::items::util::{
    self, cone, cuboid_tapered, cylinder_tapered, glow, id_quat, lit_interior, nest, prim, quat_x,
    solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    AWNING_RED, AWNING_TEAL, AWNING_WHITE, BUOY_RED, DECK_PALE, DECK_WOOD, SIGN_AMBER, SIGN_GOLD,
    STEEL_GREY, STUCCO_WHITE, canvas, enamel, plank, steel, stucco,
};

// --- Dimensions. Everything below derives from these. ----------------------

/// Boardwalk deck plan, and how far it stands off the sand.
const DECK_W: f32 = 13.4;
const DECK_D: f32 = 5.4;
const DECK_Y: f32 = 0.4;
const DECK_T: f32 = 0.25;
/// Top of the boards — the datum for everything above.
const DECK_TOP: f32 = DECK_Y + DECK_T * 0.5;

/// Kiosk plan, wall height, and where the row sits in Z. Set back far enough
/// that the promenade in front is somewhere people can actually stand — the
/// shipped row left 1.3 m and the awnings hung over most of it.
const KIOSK_W: f32 = 3.4;
const KIOSK_D: f32 = 3.0;
const KIOSK_H: f32 = 2.9;
const KIOSK_Z: f32 = 0.75;
const WALL_T: f32 = 0.24;
const KIOSK_X: [f32; 3] = [-4.3, 0.0, 4.3];

/// Outer face of the serving elevation — the `-Z` hero direction the render
/// tool and the settlement placer both look down.
const FRONT: f32 = KIOSK_Z - KIOSK_D * 0.5;
const FRONT_MID: f32 = FRONT + WALL_T * 0.5;
/// Where the back-bar lining stands, and the goods in front of it.
const BAR_Z: f32 = FRONT + 0.95;

/// The serving hatch: counter height, head height, and clear width.
const COUNTER_H: f32 = 1.05;
const HATCH_HEAD: f32 = 2.3;
const HATCH_W: f32 = 2.6;

/// Awning depth and its fall across that depth.
///
/// Deliberately short. An awning is the biggest coloured surface on the prop,
/// and at 1.7 m it reached within 0.25 m of the deck's front edge: from any
/// angle above eye level the row read as three canvas plates with a boardwalk
/// under them, and the counters, the stock and the signs — everything the
/// rebuild is *for* — were all underneath it. It shades the counter, not the
/// promenade.
const AWNING_D: f32 = 1.05;
const AWNING_FALL: f32 = 0.3;

/// Back-rail height above the deck.
const RAIL_H: f32 = 1.0;

// --- Shared construction. --------------------------------------------------

/// Whitewashed render laid in the wall's own frame.
fn render_mat(center: [f32; 3], face: FaceKey) -> SovereignMaterialSettings {
    let mut m = stucco(STUCCO_WHITE);
    m.uv_offset = util::face_uv_offset(face, center);
    m
}

/// One stucco slab of a kiosk.
fn wall(size: [f32; 3], center: [f32; 3], face: FaceKey) -> Generator {
    prim(
        solid(cuboid_tapered(size, 0.0, render_mat(center, face))),
        center,
        id_quat(),
    )
}

/// A lit surface inside a kiosk — the back bar and the stock on it. Nothing
/// lights the inside of an enclosed prop, so what shows through the hatch has
/// to carry a low self-lit term of its own.
fn stock(size: [f32; 3], center: [f32; 3], color: [f32; 3], lit: f32) -> Generator {
    prim(
        cuboid_tapered(size, 0.0, lit_interior(color, lit)),
        center,
        id_quat(),
    )
}

pub struct BoardwalkShops;

impl CatalogueEntry for BoardwalkShops {
    fn slug(&self) -> &'static str {
        "boardwalk_shops"
    }
    fn name(&self) -> &'static str {
        "Boardwalk Shops"
    }
    fn description(&self) -> &'static str {
        "A plank promenade of three awninged kiosks serving over lit counters."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CoastalResort]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::RESORT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 8.0,
            min_spawn_dist: 34.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The promenade as a tree that stands the way it does: the deck at the
/// bottom, three kiosks standing on it, and the back rail along its seaward
/// edge.
fn build_tree() -> Generator {
    let center = [0.0, DECK_Y, 0.0];
    let deck = prim(
        solid(cuboid_tapered(
            [DECK_W, DECK_T, DECK_D],
            0.0,
            util::bonded_siding(plank(DECK_PALE), FaceKey::Top, center),
        )),
        center,
        id_quat(),
    );

    let mut parts: Vec<Generator> = (0..3).map(kiosk).collect();

    // Back rail along the seaward edge, in two runs so the middle is a gap
    // people walk through — and posts and balusters, not a bar floating on
    // two stubs.
    let hz = DECK_D * 0.5 - 0.12;
    let hx = DECK_W * 0.5 - 0.12;
    for sx in [-1.0_f32, 1.0] {
        parts.extend(util::railing(
            [sx * hx, DECK_TOP, hz],
            [sx * 1.6, DECK_TOP, hz],
            RAIL_H,
            util::BALUSTER_PITCH,
            steel(STEEL_GREY),
        ));
    }
    // A lit lamp on a post between the middle and right stalls, so the
    // promenade itself reads as lit rather than only the stalls.
    let lamp_x = (KIOSK_X[1] + KIOSK_X[2]) * 0.5;
    parts.push(prim(
        solid(cylinder_tapered(0.07, 3.0, 8, 0.1, steel(STEEL_GREY))),
        [lamp_x, DECK_TOP + 1.5, FRONT - 0.9],
        id_quat(),
    ));
    parts.push(prim(
        solid(cuboid_tapered(
            [0.3, 0.34, 0.3],
            0.35,
            steel([0.3, 0.3, 0.32]),
        )),
        [lamp_x, DECK_TOP + 3.12, FRONT - 0.9],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.2, 0.2, 0.2], 0.0, glow(SIGN_GOLD, 2.4)),
        [lamp_x, DECK_TOP + 2.94, FRONT - 0.9],
        id_quat(),
    ));

    nest(deck, parts)
}

// --- One kiosk. ------------------------------------------------------------

/// A kiosk: the render that *frames* its serving hatch, the counter in it,
/// its own stock behind it, the awning and the sign.
///
/// `i` picks which stall it is, and that is the whole point of the parameter
/// — three identical stalls are one stall rendered three times, which is what
/// the shipped row was apart from an awning colour (#972 lesson 9).
fn kiosk(i: usize) -> Generator {
    let x = KIOSK_X[i];
    let mid_y = DECK_TOP + KIOSK_H * 0.5;
    let inner_d = KIOSK_D - WALL_T * 2.0;
    let mut parts = Vec::new();

    // Back and side walls — solid; only the serving face is cut.
    parts.push(wall(
        [KIOSK_W, KIOSK_H, WALL_T],
        [x, mid_y, KIOSK_Z + KIOSK_D * 0.5 - WALL_T * 0.5],
        FaceKey::SidePz,
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(wall(
            [WALL_T, KIOSK_H, inner_d],
            [x + sx * (KIOSK_W * 0.5 - WALL_T * 0.5), mid_y, KIOSK_Z],
            if sx > 0.0 {
                FaceKey::SidePx
            } else {
                FaceKey::SideNx
            },
        ));
    }
    // The serving face: two piers, the bulkhead under the hatch and the head
    // band over it.
    let (ha, hb) = (x - HATCH_W * 0.5, x + HATCH_W * 0.5);
    for (a, b) in [(x - KIOSK_W * 0.5, ha), (hb, x + KIOSK_W * 0.5)] {
        parts.push(wall(
            [b - a, KIOSK_H, WALL_T],
            [(a + b) * 0.5, mid_y, FRONT_MID],
            FaceKey::SideNz,
        ));
    }
    parts.push(wall(
        [HATCH_W, COUNTER_H, WALL_T],
        [x, DECK_TOP + COUNTER_H * 0.5, FRONT_MID],
        FaceKey::SideNz,
    ));
    parts.push(wall(
        [HATCH_W, KIOSK_H - HATCH_HEAD, WALL_T],
        [x, DECK_TOP + (HATCH_HEAD + KIOSK_H) * 0.5, FRONT_MID],
        FaceKey::SideNz,
    ));

    // The counter itself: a timber slab lapping proud of the bulkhead, so the
    // serving edge reads and nothing is coplanar with the render.
    parts.push(prim(
        solid(cuboid_tapered(
            [HATCH_W + 0.36, 0.1, 0.62],
            0.0,
            plank(DECK_WOOD),
        )),
        [x, DECK_TOP + COUNTER_H + 0.05, FRONT + 0.06],
        id_quat(),
    ));

    // Flat roof with a coping overhang. Without it the kiosk is an
    // open-topped box, which every angle above eye level shows straight into
    // — the one view a serving hatch is not meant to provide.
    parts.push(prim(
        solid(cuboid_tapered(
            [KIOSK_W + 0.26, 0.16, KIOSK_D + 0.26],
            0.0,
            plank(DECK_WOOD),
        )),
        [x, DECK_TOP + KIOSK_H + 0.08, KIOSK_Z],
        id_quat(),
    ));

    fit_out(&mut parts, i, x);
    parts.push(awning(i, x));
    parts.push(signboard(i, x));

    // The kiosk floor is the sub-root: the lowest piece, and everything else
    // stands on it.
    let floor = prim(
        cuboid_tapered(
            [KIOSK_W - WALL_T * 2.0, 0.06, inner_d],
            0.0,
            lit_interior([0.40, 0.36, 0.30], 0.16),
        ),
        [x, DECK_TOP + 0.03, KIOSK_Z],
        id_quat(),
    );
    nest(floor, parts)
}

/// What each hatch frames. One arm per stall, because the whole reason to
/// have three is that they sell different things.
fn fit_out(parts: &mut Vec<Generator>, i: usize, x: f32) {
    // Back bar, common to all three: a lit lining held close behind the
    // counter, warmer than the floor so the inside is not one flat tone.
    parts.push(stock(
        [KIOSK_W - 0.7, KIOSK_H - 0.5, 0.1],
        [x, DECK_TOP + KIOSK_H * 0.5 - 0.1, BAR_Z + 0.55],
        [0.54, 0.44, 0.32],
        0.34,
    ));
    // A lit strip under the head band, above the counter and below the
    // awning's shadow — the thing that says the stall is open.
    parts.push(prim(
        cuboid_tapered([HATCH_W - 0.3, 0.1, 0.22], 0.0, glow(SIGN_GOLD, 1.8)),
        [x, DECK_TOP + HATCH_HEAD - 0.22, FRONT + 0.42],
        id_quat(),
    ));

    let top = DECK_TOP + COUNTER_H;
    match i {
        // Ice cream: a chest freezer under the counter, tubs on it, and a
        // cone on a stick.
        0 => {
            parts.push(stock(
                [1.9, 0.75, 0.7],
                [x - 0.4, top + 0.3, BAR_Z],
                [0.72, 0.74, 0.76],
                0.3,
            ));
            for (k, c) in [
                (-0.55_f32, [0.86, 0.74, 0.52]),
                (0.0, [0.68, 0.34, 0.30]),
                (0.55, [0.52, 0.36, 0.26]),
            ] {
                parts.push(prim(
                    solid(cylinder_tapered(0.17, 0.2, 10, 0.0, lit_interior(c, 0.34))),
                    [x - 0.4 + k, top + 0.78, BAR_Z],
                    id_quat(),
                ));
            }
            parts.push(prim(
                cone(0.22, 0.5, 10, enamel([0.86, 0.72, 0.44])),
                [x + 1.2, top + 0.9, BAR_Z - 0.2],
                id_quat(),
            ));
        }
        // Postcards: a spinner rack and a shelf of racks behind it.
        1 => {
            parts.push(prim(
                solid(cylinder_tapered(0.05, 1.3, 8, 0.0, steel(STEEL_GREY))),
                [x - 0.7, top + 0.65, BAR_Z - 0.15],
                id_quat(),
            ));
            for k in 0..3 {
                let s = 0.62 - k as f32 * 0.1;
                parts.push(stock(
                    [s, 0.34, s],
                    [x - 0.7, top + 0.34 + k as f32 * 0.42, BAR_Z - 0.15],
                    [0.66, 0.58, 0.42],
                    0.32,
                ));
            }
            for k in 0..2 {
                parts.push(stock(
                    [1.5, 0.42, 0.16],
                    [x + 0.75, top + 0.32 + k as f32 * 0.55, BAR_Z + 0.35],
                    [0.60, 0.52, 0.40],
                    0.3,
                ));
            }
        }
        // Beach tat: stacked buckets and a rail of inflatable rings.
        _ => {
            for k in 0..3 {
                parts.push(prim(
                    solid(cylinder_tapered(
                        0.2,
                        0.26,
                        10,
                        0.22,
                        lit_interior([0.82, 0.46, 0.2], 0.32),
                    )),
                    [x - 0.85, top + 0.15 + k as f32 * 0.2, BAR_Z],
                    id_quat(),
                ));
            }
            parts.push(prim(
                solid(cylinder_tapered(0.04, 1.5, 6, 0.0, steel(STEEL_GREY))),
                [x + 0.7, top + 0.6, BAR_Z],
                util::quat_z(std::f32::consts::FRAC_PI_2),
            ));
            for (k, c) in [(-0.4_f32, BUOY_RED), (0.35, [0.9, 0.82, 0.3])] {
                parts.push(prim(
                    torus(0.07, 0.28, enamel(c)),
                    [x + 0.7 + k, top + 0.28, BAR_Z],
                    quat_x(std::f32::consts::FRAC_PI_2),
                ));
            }
        }
    }
}

/// The striped awning over one hatch: a header board on the render, the
/// sloping canvas hung off it, a valance at the leading edge, and two props.
///
/// The header is the **sub-root**, not the canvas. A tilted sub-root spins
/// everything nested under it, so hanging the props off the sloping canvas
/// turns them through the awning's own pitch and slides their feet along the
/// deck ([`util::assert_no_tilted_parents`] is the guard for exactly that).
///
/// [`util::assert_no_tilted_parents`]: crate::catalogue::items::util
fn awning(i: usize, x: f32) -> Generator {
    let stripe = [AWNING_RED, AWNING_TEAL, AWNING_RED][i];
    let head_y = DECK_TOP + HATCH_HEAD + 0.14;
    let front_z = FRONT - AWNING_D;
    let pitch = AWNING_FALL.atan2(AWNING_D);
    let span = AWNING_D.hypot(AWNING_FALL);

    let mut parts = vec![
        // The canvas. `-pitch` so the leading edge — at `-Z` — drops toward
        // the promenade; a positive turn would lift it instead.
        prim(
            cuboid_tapered(
                [KIOSK_W - 0.1, 0.14, span],
                0.0,
                canvas(stripe, AWNING_WHITE),
            ),
            [x, head_y - AWNING_FALL * 0.5, FRONT - AWNING_D * 0.5],
            quat_x(-pitch),
        ),
        // Scalloped valance at the leading edge — the one thing that stops a
        // canopy at this size reading as a flat coloured rectangle.
        prim(
            cuboid_tapered(
                [KIOSK_W - 0.1, 0.26, 0.07],
                0.12,
                canvas(AWNING_WHITE, stripe),
            ),
            [x, head_y - AWNING_FALL - 0.14, front_z],
            id_quat(),
        ),
    ];
    // Props down to the counter's outer edge, so the awning stands on
    // something rather than cantilevering off paint.
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered(
                [0.07, head_y - AWNING_FALL - DECK_TOP, 0.07],
                0.0,
                steel(STEEL_GREY),
            )),
            [
                x + sx * (KIOSK_W * 0.5 - 0.22),
                DECK_TOP + (head_y - AWNING_FALL - DECK_TOP) * 0.5,
                front_z + 0.05,
            ],
            id_quat(),
        ));
    }

    let header = prim(
        solid(cuboid_tapered(
            [KIOSK_W + 0.2, 0.18, 0.14],
            0.0,
            plank(DECK_WOOD),
        )),
        [x, head_y, FRONT - 0.07],
        id_quat(),
    );
    nest(header, parts)
}

/// The stall's name board — a fascia laid on the render **above the awning**,
/// with a smaller lit strip inside its frame.
///
/// On the wall rather than on a bracket above the roofline: hung out in front
/// it floated clear of everything and read as a rectangle in mid-air, which is
/// what a sign on a stick two metres proud of its own building looks like. A
/// shopfront's name goes on its fascia. The lit strip is smaller than the
/// board because a broad lit panel at strength blooms to a white blank.
fn signboard(i: usize, x: f32) -> Generator {
    let y = DECK_TOP + (HATCH_HEAD + KIOSK_H) * 0.5 + 0.12;
    let z = FRONT - 0.07;
    let board = prim(
        solid(cuboid_tapered(
            [KIOSK_W - 0.5, 0.46, 0.1],
            0.0,
            plank(DECK_WOOD),
        )),
        [x, y, z],
        id_quat(),
    );
    let tint = [SIGN_AMBER, SIGN_GOLD, SIGN_AMBER][i];
    nest(
        board,
        vec![prim(
            cuboid_tapered([KIOSK_W - 0.9, 0.26, 0.06], 0.0, glow(tint, 2.0)),
            [x, y, z - 0.06],
            id_quat(),
        )],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_no_glazing_on_solids, assert_no_tilted_parents, assert_sanitize_stable,
    };
    use crate::pds::GeneratorKind;

    fn walk(g: &Generator, at: [f32; 3], f: &mut dyn FnMut(&Generator, [f32; 3])) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        f(g, here);
        for c in &g.children {
            walk(c, here, f);
        }
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&BoardwalkShops.build(""), "boardwalk_shops");
    }

    /// #972 lesson 1, in the form this entry takes: a serving hatch is an
    /// *opening*, so there is no glazing here at all — and in particular none
    /// on a solid, which is what the shipped shopfronts were.
    #[test]
    fn no_glazing_lands_on_a_solid() {
        assert_no_glazing_on_solids(&BoardwalkShops.build(""), "boardwalk_shops");
    }

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&BoardwalkShops.build(""), "boardwalk_shops");
    }

    /// #972 lesson 9: each stall has its own stock behind its own hatch.
    /// Three identical kiosks are one kiosk rendered three times, which is
    /// what shipped — the only thing that differed was an awning colour.
    #[test]
    fn every_stall_sells_something_different() {
        let root = BoardwalkShops.build("");
        // Collect the lit contents of each stall, keyed by which one it is.
        let mut per_stall = [Vec::new(), Vec::new(), Vec::new()];
        walk(&root, [0.0; 3], &mut |g, at| {
            let material = match &g.kind {
                GeneratorKind::Cuboid { material, .. }
                | GeneratorKind::Cylinder { material, .. }
                | GeneratorKind::Cone { material, .. }
                | GeneratorKind::Torus { material, .. } => material,
                _ => return,
            };
            if material.emission_strength.0 < 0.15 || at[2] < FRONT || at[1] < DECK_TOP {
                return;
            }
            for (k, &kx) in KIOSK_X.iter().enumerate() {
                if (at[0] - kx).abs() < KIOSK_W * 0.5 {
                    per_stall[k]
                        .push((g.kind.kind_tag(), (material.base_color.0[0] * 100.0) as i32));
                }
            }
        });
        for (k, s) in per_stall.iter().enumerate() {
            assert!(
                s.len() >= 4,
                "stall {k} has {} lit things behind its hatch — the opening \
                 frames an empty box",
                s.len()
            );
        }
        // ...and no two stalls carry the same census, which is what "they sell
        // different things" actually means.
        for a in 0..3 {
            for b in a + 1..3 {
                let (mut x, mut y) = (per_stall[a].clone(), per_stall[b].clone());
                x.sort();
                y.sort();
                assert_ne!(x, y, "stalls {a} and {b} are the same stall twice");
            }
        }
    }

    /// The promenade in front of the stalls is somewhere a person can stand.
    /// The shipped row left 1.3 m and hung 1.5 m of awning over it.
    #[test]
    fn the_promenade_is_wide_enough_to_walk() {
        let clear = FRONT - (-DECK_D * 0.5);
        assert!(
            clear > 1.7,
            "only {clear} m of deck in front of the counters"
        );
        // ...and the awning shades the counter, not the whole promenade. At
        // 1.7 m deep it left 0.25 m of open deck and the row read from above
        // as three canvas plates with everything else underneath them.
        let unshaded = clear - AWNING_D;
        assert!(
            unshaded > 0.7,
            "the awning leaves only {unshaded} m of open promenade — from any \
             angle above eye level it *is* the prop"
        );
    }

    /// #972 lesson 8: everything stands on the deck it stands on.
    #[test]
    fn everything_stands_on_the_boardwalk() {
        let root = BoardwalkShops.build("");
        let (half_x, half_z) = (DECK_W * 0.5, DECK_D * 0.5);
        let mut checked = 0;
        walk(&root, [0.0; 3], &mut |g, at| {
            let half = match &g.kind {
                GeneratorKind::Cuboid { size, .. } => [size.0[0] * 0.5, size.0[2] * 0.5],
                GeneratorKind::Cylinder { radius, .. } | GeneratorKind::Cone { radius, .. } => {
                    [radius.0, radius.0]
                }
                _ => return,
            };
            // The deck itself is the thing being checked against.
            if half[0] >= half_x {
                return;
            }
            assert!(
                at[0] - half[0] > -half_x - 1e-3 && at[0] + half[0] < half_x + 1e-3,
                "a part at {at:?} (half {half:?}) hangs off the end of the deck"
            );
            assert!(
                at[2] - half[1] > -half_z - 1e-3 && at[2] + half[1] < half_z + 1e-3,
                "a part at {at:?} (half {half:?}) hangs off the front or back of \
                 the deck"
            );
            checked += 1;
        });
        assert!(checked > 40, "only {checked} parts walked");
    }

    /// The back rail is a railing, with posts and balusters.
    #[test]
    fn the_back_rail_has_posts_and_balusters() {
        let root = BoardwalkShops.build("");
        let mut balusters = 0;
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cuboid { size, .. } = &g.kind else {
                return;
            };
            let [sx, sy, sz] = size.0;
            if at[2] > 1.0 && sx < 0.09 && sz < 0.09 && sy > 0.5 {
                balusters += 1;
            }
        });
        assert!(
            balusters >= 14,
            "only {balusters} balusters — the back rail is a bar on two stubs"
        );
    }

    /// The stalls keep their lit signs — escalation's broken-emissive ruin
    /// pass needs something to snuff.
    #[test]
    fn has_lit_signs() {
        assert!(crate::catalogue::items::util::has_emissive(
            &BoardwalkShops.build("")
        ));
    }

    /// The editability contract: the deck carries three kiosks, and each
    /// kiosk carries its own awning, sign and stock.
    #[test]
    fn subtrees_carry_what_they_hold_up() {
        fn count(g: &Generator) -> usize {
            1 + g.children.iter().map(count).sum::<usize>()
        }
        let root = BoardwalkShops.build("");
        let kiosks: Vec<_> = root
            .children
            .iter()
            .filter(|c| c.children.len() > 10)
            .collect();
        assert_eq!(kiosks.len(), 3, "the deck carries three kiosks");
        for k in kiosks {
            assert!(
                k.children.iter().filter(|c| !c.children.is_empty()).count() >= 2,
                "a kiosk should carry its awning and its sign as sub-assemblies"
            );
        }
        assert!(count(&root) > 90, "the promenade lost most of its parts");
    }
}
