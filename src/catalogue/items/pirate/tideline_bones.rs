//! Tide-line Bones — the ribs of a small wreck standing out of the shingle,
//! with a burst chest and what the tide keeps bringing back.
//!
//! A keel half-buried along the strand, seven frames rising out of it and
//! settled over to one side, two strakes still fastened. Beside them a sea chest
//! with its lid off, a heap of corroded coin spilling out of it and a trail
//! running away down the beach, two pieces alight with witchfire. A skull, a
//! long bone, and a fragment of ribcage in the shingle.
//!
//! # The keel is what makes ribs read as a wreck
//!
//! Seven curved timbers standing in a beach are seven curved timbers. The same
//! seven with a keel running under them are a ship, and it costs one prim. That
//! is the whole design: the ribs are the recognisable thing, the keel is what
//! makes them recognisable *as* something, and both are drawn from one set of
//! lines so they cannot disagree.
//!
//! # Why she lies fore-and-aft down the approach
//!
//! Her keel runs along `Z`, so the hero view at `-Z` looks **down her length**
//! and the frames nest one inside the next. That is the only angle a row of
//! frames reads from as a *hull*: broadside, a `U` spanning athwartships
//! projects to a vertical line, and seven of them are a fence. The first build
//! laid her across the approach and the render showed exactly that. It is also
//! the [`super::rotting_hulk`]'s orientation, which is worth having twice — a
//! wreck is a wreck.
//!
//! Laying her this way also means her frames span local `X`, which is the
//! orientation [`super::hull_frame`] draws them in, so the tilt is a plain heel about
//! her own keel with no yaw folded into it.
//!
//! # The curse is on the money
//!
//! The coins are the only thing here that glows, and most of them do not. A
//! spill of uniformly green treasure is a special effect; a heap of dull
//! corroded coin with **two** pieces alight is a specific and much worse idea,
//! because the eye finds those two by itself. The register's whole method — see
//! [`WITCHFIRE`] — is one cold light among things that are not lit.
//!
//! # Reuse
//!
//! Both the ship's frames and the human ribcage are the shared [`super::hull_frame`]:
//! a rib is a rib, and the arc-that-must-be-translated-not-rotated lesson
//! recorded there cost three renders on the [`super::rotting_hulk`]. Using it at
//! 0.2 m as well as at 2.2 m is the return on having put it in the kit rather
//! than in one file (#972 lesson 5).

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    attach, cuboid_tapered, cylinder_tapered, face_uv_offset, footing, glow, id_quat, nest, prim,
    quat_mul, quat_x, quat_y, quat_z, solid, sphere, strut, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Fp4, Generator};
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BONE_PALE, GOLD_LEAF, HULL_OAK, HULL_TAR, IRON_BLACK, PORT_POOR, ROPE_HEMP, STRAND_SHINGLE,
    WITCHFIRE, board, bone, fx, hemp, iron, strake, strand, tar,
};

/// The shingle it lies in — the sub-root every footprint guard measures against
/// (#972 lesson 19).
const PAD: [f32; 3] = [7.2, 0.26, 6.0];
const GROUND: f32 = PAD[1];

/// Her lines: `(z, half-beam, height of the sheer above the keel)`, bow first.
///
/// A small craft — a longboat's worth, not a ship's — because the scale has to
/// say "this washed up" rather than "a vessel was lost here".
const STATIONS: [(f32, f32, f32); 3] = [
    (-2.4, 0.42, 0.62), // forward, drawing in
    (0.1, 1.12, 1.85),  // amidships
    (2.3, 0.56, 0.88),  // aft
];

/// How far she has settled over, about her own keel, and how deep the keel has
/// gone into the shingle.
///
/// Negative lift, because a wreck sitting *on* the beach with daylight under it
/// is a model of a wreck — the same correction the [`super::rotting_hulk`]
/// needed.
const HEEL: f32 = 0.24;
const KEEL_LIFT: f32 = -0.14;

/// The keel's own section, and the frames'.
const KEEL_W: f32 = 0.26;
const FRAME_R: f32 = 0.075;

/// How many frames stand out of the shingle.
const FRAMES: usize = 7;

/// The chest: where it lies, and how big it is. Outboard of her widest frame, on
/// the shingle beside the wreck rather than in the wreckage.
const CHEST_AT: [f32; 3] = [1.95, 0.0, 1.15];
const CHEST: [f32; 3] = [0.56, 0.5, 0.86];

/// How many loose coins are in the trail, how many of them are alight, and how
/// big one is.
///
/// Two alight, not all of them: the eye finds two by itself, and a uniformly
/// green spill is a special effect rather than a specific and worse idea.
const COINS: usize = 11;
const LIT_COINS: usize = 2;
const COIN_R: f32 = 0.1;

/// Corroded gold — dull, and deliberately *not* [`GOLD_LEAF`]'s gilding value.
/// The colour is the difference between treasure and treasure that has been in
/// the sea. Kept light enough to read against the shingle it is lying on, which
/// is the constraint the first pass missed: a colour chosen only for its story
/// disappears.
const COIN_CORRODED: [f32; 3] = [
    GOLD_LEAF[0] * 0.78,
    GOLD_LEAF[1] * 0.80,
    GOLD_LEAF[2] * 0.66,
];

/// Hero side — the render tool and the settlement placer both look down `-Z`.
const FRONT: f32 = -1.0;

const _: () = assert!(
    LIT_COINS < COINS / 3,
    "too much of the spill is alight — a uniformly green pile of money is an \
     effect, not a curse"
);
const _: () = assert!(
    CHEST_AT[0] - CHEST[0] * 0.5 > STATIONS[1].1,
    "the chest is inside her own frames — it belongs on the shingle beside the \
     wreck, not in the wreckage"
);

pub struct TidelineBones;

impl CatalogueEntry for TidelineBones {
    fn slug(&self) -> &'static str {
        "tideline_bones"
    }
    fn name(&self) -> &'static str {
        "Tide-line Bones"
    }
    fn description(&self) -> &'static str {
        "The ribs of a small wreck standing out of the shingle, with a burst chest and a spill of \
         corroded coin beside them."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Pirate]
    }
    fn prosperity_band(&self) -> ProsperityBand {
        PORT_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.8,
            min_spawn_dist: 15.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Half-beam and sheer height at station `z`, interpolated along [`STATIONS`].
fn lines_at(z: f32) -> (f32, f32) {
    if z <= STATIONS[0].0 {
        return (STATIONS[0].1, STATIONS[0].2);
    }
    let last = STATIONS[STATIONS.len() - 1];
    if z >= last.0 {
        return (last.1, last.2);
    }
    for w in STATIONS.windows(2) {
        let ((z0, b0, h0), (z1, b1, h1)) = (w[0], w[1]);
        if z >= z0 && z <= z1 {
            let t = (z - z0) / (z1 - z0);
            return (b0 + (b1 - b0) * t, h0 + (h1 - h0) * t);
        }
    }
    (STATIONS[1].1, STATIONS[1].2)
}

/// The tilt she has settled to: a heel about her own keel, which runs along `Z`.
///
/// A plain `quat_z`, with no yaw folded in, because her frames span local `X` —
/// which is the orientation [`super::hull_frame`] draws them in. That is a real benefit
/// of laying her fore-and-aft down the approach rather than across it, on top of
/// the reason recorded in the module note.
fn hull_tilt() -> Fp4 {
    quat_z(HEEL)
}

/// Turn a point in her own frame — `y` up from the keel, `x` athwart — into the
/// world.
///
/// The same plane rotation by PLUS the angle that [`hull_tilt`] carries, so the
/// two cannot disagree. The careening slip lost a whole revision to those two
/// disagreeing in sign, and a hull's own symmetry hid it (#1030).
fn settled(x: f32, y: f32, z: f32) -> [f32; 3] {
    let (s, c) = HEEL.sin_cos();
    [x * c - y * s, GROUND + KEEL_LIFT + x * s + y * c, z]
}

/// Where frame `i` of [`FRAMES`] stands along her keel.
fn frame_z(i: usize) -> f32 {
    let (z0, z1) = (STATIONS[0].0 + 0.25, STATIONS[2].0 - 0.2);
    z0 + (z1 - z0) * (i as f32 / (FRAMES - 1) as f32)
}

/// The keel, and the frames standing out of it.
fn wreck() -> Vec<Generator> {
    let mid_z = (STATIONS[0].0 + STATIONS[2].0) * 0.5;
    let mut out = vec![
        // The keel — half-buried, and the one prim that turns a row of curved
        // timbers into a ship.
        prim(
            solid(cuboid_tapered(
                [KEEL_W * 1.3, KEEL_W, STATIONS[2].0 - STATIONS[0].0 + 0.5],
                0.1,
                board(HULL_OAK),
            )),
            settled(0.0, KEEL_W * 0.4, mid_z),
            hull_tilt(),
        ),
    ];
    for i in 0..FRAMES {
        let z = frame_z(i);
        let (beam, height) = lines_at(z);
        out.push(super::hull_frame(
            // The ellipse centre goes one `height` above the keel in HER frame,
            // so the raise happens before the tilt — see `hull_frame`'s
            // contract.
            settled(0.0, height, z),
            beam,
            height,
            hull_tilt(),
            FRAME_R,
            board(HULL_OAK),
        ));
    }
    // Two strakes still fastened, on the side she has gone over onto — struts
    // between two frames' own heads, so the planking cannot float clear of the
    // timber it is nailed to (#1028).
    for (a, b) in [(1_usize, 3_usize), (3, 5)] {
        let (za, zb) = (frame_z(a), frame_z(b));
        let (ba, ha) = lines_at(za);
        let (bb, hb) = lines_at(zb);
        out.push(strut(
            settled(ba * 0.9, ha * 0.62, za),
            settled(bb * 0.92, hb * 0.5, zb),
            0.055,
            4,
            strake(HULL_TAR),
        ));
    }
    out
}

/// The chest, burst open, and the coin coming out of it.
///
/// The lid is off and lying against the box rather than hinged open, because a
/// chest that has been *broken* is the story and a chest standing open is a
/// display case.
///
/// The money is a **heap plus a trail**, and the heap is what does the work. The
/// first build was a trail of flat discs only: at 75 mm across, lying flat on
/// tan shingle in a dull brown, they were invisible from every angle. A low cone
/// of coin at the mouth reads instantly at any distance, and the loose pieces
/// then say which way it went.
fn chest() -> Vec<Generator> {
    let base_y = GROUND + CHEST[1] * 0.42;
    // Oak with iron bands, not tar. A near-black chest is a black cube, which is
    // what the first render produced — indistinguishable from a rock.
    let mut out = vec![
        prim(
            solid(cuboid_tapered(CHEST, 0.06, board(HULL_OAK))),
            [CHEST_AT[0], base_y, CHEST_AT[2]],
            id_quat(),
        ),
        // A dark interior, so the open box is not a solid block with a plank
        // beside it.
        prim(
            solid(cuboid_tapered(
                [CHEST[0] * 0.82, 0.05, CHEST[2] * 0.86],
                0.0,
                tar([0.1, 0.1, 0.09]),
            )),
            [CHEST_AT[0], base_y + CHEST[1] * 0.42, CHEST_AT[2]],
            id_quat(),
        ),
        // The lid, off and leaning against the box on the wreck side.
        prim(
            solid(cuboid_tapered(
                [0.07, CHEST[1] * 0.9, CHEST[2] * 0.94],
                0.05,
                board(HULL_TAR),
            )),
            [
                CHEST_AT[0] - CHEST[0] * 0.72,
                GROUND + CHEST[1] * 0.42,
                CHEST_AT[2],
            ],
            quat_z(-1.1),
        ),
    ];
    // Iron bands round the ends.
    for dz in [-CHEST[2] * 0.3, CHEST[2] * 0.3] {
        out.push(prim(
            solid(cuboid_tapered(
                [CHEST[0] * 1.06, CHEST[1] * 1.06, CHEST[2] * 0.1],
                0.0,
                iron(IRON_BLACK, 0xB1),
            )),
            [CHEST_AT[0], base_y, CHEST_AT[2] + dz],
            id_quat(),
        ));
    }

    // The heap at the mouth, and the trail running away from it down the beach.
    let mouth = [
        CHEST_AT[0] + CHEST[0] * 0.55,
        GROUND,
        CHEST_AT[2] - CHEST[2] * 0.1,
    ];
    out.push(prim(
        solid(cylinder_tapered(
            0.3,
            0.16,
            10,
            0.6,
            iron(COIN_CORRODED, 0xB2),
        )),
        [mouth[0] + 0.16, GROUND + 0.08, mouth[2]],
        id_quat(),
    ));
    // The trail's far end comes off the PAD's own edge less a coin's own reach,
    // rather than a length that happened to look right: laid off the mouth
    // instead, the last coin hung 8 mm over the shingle and the footprint guard
    // caught it (#972 lesson 8).
    let trail_start = mouth[0] + 0.3;
    let trail_end = PAD[0] * 0.5 - COIN_R - 0.1;
    for i in 0..COINS {
        let t = (i as f32 + 1.0) / COINS as f32;
        // Spread widens down the trail, as a spill does.
        let fan = ((i * 7) % 5) as f32 / 5.0 - 0.5;
        let at = [
            trail_start + (trail_end - trail_start) * t,
            GROUND + COIN_R * 0.25,
            mouth[2] + fan * t * 1.5,
        ];
        // The alight pieces are split between the heap's edge and the far end of
        // the trail, so the cold light is not one blob.
        let material = if i == 0 || i == COINS - 1 {
            glow(WITCHFIRE, 0.5)
        } else {
            iron(COIN_CORRODED, 0xB4 + i as u32)
        };
        out.push(prim(
            solid(cylinder_tapered(COIN_R, 0.03, 8, 0.0, material)),
            at,
            quat_z(fan * 0.3),
        ));
    }
    out
}

/// A skull, a long bone, and a fragment of ribcage.
///
/// The ribcage is the shared [`super::hull_frame`] at 0.2 m, which is the same shape a
/// ship's frame is and is drawn by the same code. Nothing else in the catalogue
/// reuses a lesson across two orders of magnitude, and it is only possible
/// because the helper takes its beam and rise as arguments rather than baking a
/// hull's proportions in.
fn remains() -> Vec<Generator> {
    let at = [-1.75_f32, 0.0, -1.5_f32];
    let mut out = vec![
        prim(
            solid(sphere(0.17, 4, bone(BONE_PALE))),
            [at[0], GROUND + 0.15, at[2]],
            id_quat(),
        ),
        // The jaw, dropped clear — one prim, and it is what makes a pale ball a
        // skull.
        prim(
            solid(cuboid_tapered([0.18, 0.06, 0.15], 0.4, bone(BONE_PALE))),
            [at[0] + 0.05, GROUND + 0.04, at[2] - 0.24],
            quat_y(0.5),
        ),
        prim(
            solid(cylinder_tapered(0.05, 0.62, 5, 0.12, bone(BONE_PALE))),
            [at[0] - 0.1, GROUND + 0.05, at[2] + 0.95],
            quat_mul(quat_x(FRAC_PI_2), quat_y(0.3)),
        ),
    ];
    // Three ribs in the shingle, on the same lines idiom as the ship's frames
    // and settled over the same way.
    for (i, dz) in [0.38_f32, 0.58, 0.76].into_iter().enumerate() {
        let h = 0.3 - i as f32 * 0.05;
        let b = 0.22 - i as f32 * 0.03;
        out.push(super::hull_frame(
            [at[0] + 0.06, GROUND + KEEL_LIFT * 0.4 + h * 0.6, at[2] + dz],
            b,
            h,
            quat_z(0.7),
            0.022,
            bone(BONE_PALE),
        ));
    }
    out
}

fn build_tree() -> Generator {
    let pad_c = [0.0, GROUND * 0.5, 0.0];
    let mut shingle = strand(STRAND_SHINGLE);
    shingle.uv_offset = face_uv_offset(FaceKey::Top, pad_c);

    let mut carried = vec![footing(PAD[0] * 0.5, PAD[2] * 0.7, [0.0, 0.0], 3.8)];
    carried.extend(wreck());
    carried.extend(chest());
    carried.extend(remains());

    // Wrack along the tide line, a broken block, and a rotted coil. The tide
    // line runs parallel to the water and so does anything the sea left on it,
    // which for a hull lying fore-and-aft means along `Z`. Placed off the PAD's
    // own half-extent and each piece's own reach (#972 lesson 8).
    let tide_x = FRONT * (PAD[0] * 0.5 - 0.8);
    for (i, dz) in [-1.9_f32, -0.2, 1.5].into_iter().enumerate() {
        carried.push(prim(
            solid(cuboid_tapered(
                [0.3, 0.06, 1.5],
                0.4,
                tar([0.24, 0.25, 0.19]),
            )),
            [tide_x + i as f32 * 0.22, GROUND + 0.03, dz],
            quat_y(0.15 * i as f32),
        ));
    }
    carried.push(prim(
        torus(0.04, 0.24, hemp(ROPE_HEMP)),
        [tide_x + 0.5, GROUND + 0.04, 2.4],
        id_quat(),
    ));
    // A block off her rigging, shell split, with its sheave still in it.
    carried.push(prim(
        solid(cuboid_tapered([0.26, 0.3, 0.13], 0.2, board(HULL_OAK))),
        [tide_x + 0.9, GROUND + 0.14, -2.5],
        quat_z(0.5),
    ));
    carried.push(prim(
        solid(cylinder_tapered(0.09, 0.06, 8, 0.0, iron(IRON_BLACK, 0xB3))),
        [tide_x + 0.9, GROUND + 0.16, -2.57],
        quat_x(FRAC_PI_2),
    ));

    let mut root = nest(
        prim(solid(cuboid_tapered(PAD, 0.0, shingle)), pad_c, id_quat()),
        carried,
    );
    root.audio = fx::witchfire_hiss();
    // The fire is over the MONEY, which is the whole idea — see the module note.
    let fire = fx::witchfire(
        [
            CHEST_AT[0] + CHEST[0] * 0.55 + 0.16,
            GROUND + 0.2,
            CHEST_AT[2] - CHEST[2] * 0.1,
        ],
        0xB0,
    );
    attach(&mut root, fire);
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::measure;
    use crate::catalogue::items::util::{
        assert_no_glazing_on_solids, assert_no_tilted_parents, assert_sanitize_stable, rotate_by,
        window_cards,
    };
    use crate::pds::GeneratorKind as K;

    fn built() -> Generator {
        TidelineBones.build("")
    }

    /// Base colour of the node a [`measure::SolidPiece`] came from.
    ///
    /// The bridge that lets a bounds-based guard select on **material**, which is
    /// what actually distinguishes a ship's frame from a rib in the shingle here:
    /// both are tori, both are arcs, and the first version of the frame guard
    /// counted eight frames because one bone rib's tilt pushed its bounding box
    /// over the height threshold it was filtering on. #972 lesson 24, and the
    /// tenth time this kit has paid for a selector keyed on a proxy.
    fn colour_of(root: &Generator, path: &[usize]) -> Option<[f32; 3]> {
        let mut g = root;
        for &i in path {
            g = g.children.get(i)?;
        }
        g.kind.material().map(|m| m.base_color.0)
    }

    /// The tori of one material, with their world bounds.
    fn arcs_of(colour: [f32; 3]) -> Vec<measure::SolidPiece> {
        let g = built();
        measure::solids(&g)
            .into_iter()
            .filter(|p| p.kind_tag == "Torus")
            .filter(|p| colour_of(&g, &p.path) == Some(colour))
            .collect()
    }

    /// Her keel, read off the built prims.
    fn keel() -> measure::SolidPiece {
        measure::solids(&built())
            .into_iter()
            .find(|p| p.kind_tag == "Cuboid" && p.bounds.size().z > 4.0)
            .expect("she has a keel")
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&built(), "tideline_bones");
    }

    #[test]
    fn no_rotated_node_carries_an_offset_child() {
        assert_no_tilted_parents(&built(), "tideline_bones");
    }

    #[test]
    fn the_wreck_carries_no_glazing() {
        let g = built();
        assert_no_glazing_on_solids(&g, "tideline_bones");
        assert!(window_cards(&g).is_empty(), "a wreck has grown a window");
    }

    /// Her frames stand on her keel, and they describe one hull.
    ///
    /// Both halves matter. Ribs that miss the keel are posts in a beach; ribs of
    /// equal size are a fence.
    #[test]
    fn the_frames_stand_on_a_keel_and_describe_one_hull() {
        let keel = keel();
        let ribs = arcs_of(HULL_OAK);
        assert_eq!(
            ribs.len(),
            FRAMES,
            "expected {FRAMES} ship's frames, found {}",
            ribs.len()
        );
        for r in &ribs {
            assert!(
                (r.bounds.min.y - keel.bounds.max.y).abs() < KEEL_W * 2.2,
                "a frame's trough is at {} against a keel topping out at {} — it \
                 is standing in the beach, not on her keel",
                r.bounds.min.y,
                keel.bounds.max.y
            );
            assert!(
                r.bounds.center().z > keel.bounds.min.z && r.bounds.center().z < keel.bounds.max.z,
                "a frame at z = {} is off the end of her own keel",
                r.bounds.center().z
            );
        }
        // One hull's lines: the frames vary in rise, and the deepest is
        // amidships rather than at an end.
        let heights: Vec<f32> = (0..FRAMES).map(|i| lines_at(frame_z(i)).1).collect();
        let hi = heights.iter().copied().fold(f32::MIN, f32::max);
        let lo = heights.iter().copied().fold(f32::MAX, f32::min);
        assert!(
            hi - lo > 0.5,
            "the frames vary by only {} m — they do not describe one hull",
            hi - lo
        );
        let deepest = heights
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).expect("finite"))
            .map(|(i, _)| i)
            .expect("non-empty");
        assert!(
            deepest > 0 && deepest < FRAMES - 1,
            "her deepest frame is number {deepest} of {FRAMES} — a hull's \
             midships is not at its stem"
        );
    }

    /// She has settled over onto one side, and is bedded into the shingle.
    #[test]
    fn she_has_settled_over_and_is_bedded_in() {
        // The heel shows up as the two ends of a frame being at different
        // heights — read through [`settled`] rather than off the constant, so
        // the guard fails if `settled` and [`hull_tilt`] ever stop agreeing.
        let (beam, height) = lines_at(0.1);
        let port = settled(-beam, height, 0.1);
        let starboard = settled(beam, height, 0.1);
        assert!(
            (port[1] - starboard[1]).abs() > beam * 0.3,
            "her two sheer edges are within {} m in height — she is sitting \
             upright, which is not what a wreck does",
            (port[1] - starboard[1]).abs()
        );
        assert!(
            keel().bounds.min.y < GROUND,
            "her keel's underside is at {} and the shingle is at {GROUND} — \
             there is daylight under it",
            keel().bounds.min.y
        );
    }

    /// The spill runs out of the chest, and only two pieces of it burn.
    #[test]
    fn the_spill_runs_from_the_chest_and_two_pieces_burn() {
        fn coins(g: &Generator, at: [f32; 3], out: &mut Vec<([f32; 3], bool)>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let K::Cylinder {
                radius, material, ..
            } = &g.kind
                && (radius.0 - COIN_R).abs() < 0.003
            {
                out.push((here, material.emission_strength.0 > 0.0));
            }
            for c in &g.children {
                coins(c, here, out);
            }
        }
        let mut found = Vec::new();
        coins(&built(), [0.0; 3], &mut found);
        assert_eq!(
            found.len(),
            COINS,
            "expected {COINS} loose coins in the trail, found {}",
            found.len()
        );
        assert_eq!(
            found.iter().filter(|(_, lit)| *lit).count(),
            LIT_COINS,
            "the wrong number of coins are alight — the curse is specific, and a \
             uniformly green pile of money is an effect"
        );
        for (at, _) in &found {
            assert!(
                at[1] < GROUND + 0.1,
                "a coin is at y = {} — the trail is floating",
                at[1]
            );
        }
        // The trail runs AWAY from the chest's mouth rather than surrounding it.
        let mouth_x = CHEST_AT[0] + CHEST[0] * 0.55;
        for (at, _) in &found {
            assert!(
                at[0] >= mouth_x,
                "a coin at x = {} is behind the chest's mouth at {mouth_x} — the \
                 spill is a heap round the box, not a trail out of it",
                at[0]
            );
        }
        let furthest = found.iter().map(|(at, _)| at[0]).fold(f32::MIN, f32::max);
        assert!(
            furthest - mouth_x > 0.9,
            "the trail only reaches {} m from the chest — it reads as a puddle",
            furthest - mouth_x
        );
        // ...and the two lit pieces are not next to each other, so the cold
        // light is spread down the spill rather than being one blob.
        let lit: Vec<f32> = found
            .iter()
            .filter(|(_, l)| *l)
            .map(|(at, _)| at[0])
            .collect();
        assert!(
            (lit[0] - lit[1]).abs() > 0.6,
            "both alight coins are within {} m of each other — that is one glow, \
             not two pieces the eye has to find",
            (lit[0] - lit[1]).abs()
        );
    }

    /// The chest reads as a chest: banded, open, and with something dark inside.
    #[test]
    fn the_chest_is_banded_and_open() {
        let g = built();
        let solids = measure::solids(&g);
        let bands = solids
            .iter()
            .filter(|p| p.kind_tag == "Cuboid")
            .filter(|p| colour_of(&g, &p.path) == Some(IRON_BLACK))
            .filter(|p| (p.bounds.center().x - CHEST_AT[0]).abs() < CHEST[0])
            .count();
        assert_eq!(bands, 2, "the chest carries {bands} iron bands, not two");
        // Its body is oak, not tar: a near-black chest is a black cube, which is
        // indistinguishable from a rock at any distance.
        let body = solids
            .iter()
            .find(|p| {
                p.kind_tag == "Cuboid"
                    && colour_of(&g, &p.path) == Some(HULL_OAK)
                    && (p.bounds.center().x - CHEST_AT[0]).abs() < 0.1
                    && (p.bounds.center().z - CHEST_AT[2]).abs() < 0.1
            })
            .expect("the chest has an oak body");
        assert!(
            body.bounds.size().y > 0.3,
            "the chest is only {} m deep — it reads as a lid on the ground",
            body.bounds.size().y
        );
    }

    /// The remains are a fragment of one body, and the ribcage is built out of
    /// the same helper the ship's frames are.
    #[test]
    fn the_remains_are_a_fragment_of_one_body() {
        fn bone_count(g: &Generator) -> usize {
            let own = g
                .kind
                .material()
                .is_some_and(|m| m.base_color.0 == BONE_PALE) as usize;
            own + g.children.iter().map(bone_count).sum::<usize>()
        }
        let n = bone_count(&built());
        assert_eq!(
            n, 6,
            "found {n} bone pieces — a skull, a jaw, a long bone and three ribs \
             is one body's worth; a beach of skulls reads as a joke"
        );
        let small = arcs_of(BONE_PALE);
        assert_eq!(
            small.len(),
            3,
            "expected three ribs in the shingle, found {}",
            small.len()
        );
        for r in &small {
            assert!(
                r.bounds.min.y < GROUND + 0.14,
                "a rib's underside is at {} — it is standing up out of the beach \
                 rather than lying in it",
                r.bounds.min.y
            );
        }
        // And they are much smaller than her frames, which is the whole point of
        // the shared helper taking its beam and rise as arguments.
        let ship = arcs_of(HULL_OAK)
            .iter()
            .map(|p| p.bounds.size().x)
            .fold(f32::MIN, f32::max);
        let human = small
            .iter()
            .map(|p| p.bounds.size().x)
            .fold(f32::MIN, f32::max);
        assert!(
            ship > human * 3.0,
            "her frames ({ship} m) and the ribcage ({human} m) are within a \
             factor of three — one of them is the wrong scale"
        );
    }

    /// Two strakes are still fastened, each between two frames' own heads.
    #[test]
    fn the_strakes_are_still_fastened_to_her_frames() {
        let mut found = Vec::new();
        fn strakes(g: &Generator, at: [f32; 3], out: &mut Vec<([f32; 3], [f32; 3])>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let K::Cylinder { radius, height, .. } = &g.kind
                && (radius.0 - 0.055).abs() < 0.003
            {
                let tip = rotate_by(g.transform.rotation.0, [0.0, height.0 * 0.5, 0.0]);
                out.push((
                    [here[0] + tip[0], here[1] + tip[1], here[2] + tip[2]],
                    [here[0] - tip[0], here[1] - tip[1], here[2] - tip[2]],
                ));
            }
            for c in &g.children {
                strakes(c, here, out);
            }
        }
        strakes(&built(), [0.0; 3], &mut found);
        assert_eq!(
            found.len(),
            2,
            "expected two strakes, found {}",
            found.len()
        );
        for (a, b) in &found {
            for end in [a, b] {
                let near = (0..FRAMES).any(|i| (end[2] - frame_z(i)).abs() < 0.12);
                assert!(
                    near,
                    "a strake ends at z = {}, which is at no frame — planking has \
                     to be nailed to something",
                    end[2]
                );
                assert!(
                    end[1] > GROUND + 0.1,
                    "a strake end is at y = {} — down in the shingle rather than \
                     on her side",
                    end[1]
                );
            }
        }
    }

    /// Everything lies on the shingle it is nested under (#972 lessons 8, 19).
    #[test]
    fn every_part_lies_on_the_pad() {
        let half = [PAD[0] * 0.5, PAD[2] * 0.5];
        let mut checked = 0;
        for p in measure::solids(&built()) {
            checked += 1;
            assert!(
                p.bounds.min.x >= -half[0] - 1e-3 && p.bounds.max.x <= half[0] + 1e-3,
                "{} at {:?} overhangs the shingle in X ({} .. {})",
                p.kind_tag,
                p.bounds.center(),
                p.bounds.min.x,
                p.bounds.max.x
            );
            assert!(
                p.bounds.min.z >= -half[1] - 1e-3 && p.bounds.max.z <= half[1] + 1e-3,
                "{} at {:?} overhangs the shingle in Z ({} .. {})",
                p.kind_tag,
                p.bounds.center(),
                p.bounds.min.z,
                p.bounds.max.z
            );
        }
        assert!(checked > 30, "only {checked} parts examined");
    }
}
