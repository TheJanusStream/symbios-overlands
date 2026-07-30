//! Gibbet Cage — an iron cage on a post at the tide line, with what is left of
//! somebody in it.
//!
//! A squared post with a projecting arm braced by a knee, a chain and swivel,
//! and a body-shaped basket of iron straps hanging clear so it turns in the
//! wind. Bones inside. Witchfire in the ribs. A warning board nailed to the
//! post, and the shingle under it left to the wrack.
//!
//! # The cage has to be see-through or none of it works
//!
//! This is the whole design constraint, and it is the reason the cage is
//! twenty-odd struts and hoops rather than one tapered shell. A gibbet is
//! horrifying because you can *see in*, and a solid basket is a lantern. So the
//! cage is built the way the real ones were — horizontal hoops at the head,
//! shoulders, hips and feet, with flat straps riveted down the outside between
//! them — and the bones are placed inside it, sized to read through the gaps
//! rather than to fill them.
//!
//! It also has to hang **clear**: clear of the post, so it can turn, and clear
//! of the ground, so it is hanging rather than standing. Both are guarded, from
//! the built prims, because "it looks like it is hanging" is the easiest thing
//! in this kit to get almost right (#1028, #1030).
//!
//! # One warning, stated once
//!
//! There is a skull, two long bones, and nothing else. The temptation with this
//! subject is to keep adding — more bones, more cages, a scatter of skulls up
//! the beach — and the result reads as a joke rather than as a warning. The
//! register's horror is that this is *ordinary*: a piece of harbour furniture,
//! maintained, with a notice on it.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::catalogue::items::util::{
    attach, cuboid_tapered, cylinder_tapered, face_uv_offset, footing, glow, id_quat, nest, prim,
    quat_x, quat_y, quat_z, solid, sphere, strut, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::pds::generator::FaceKey;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BONE_PALE, HULL_OAK, IRON_BLACK, PORT_POOR, ROPE_HEMP, STRAND_SHINGLE, WHARF_GREY, WITCHFIRE,
    board, bone, fx, hemp, iron, strand, tar,
};

/// The shingle it stands on — the sub-root every footprint guard measures
/// against (#972 lesson 19).
const PAD: [f32; 3] = [4.8, 0.26, 4.8];
const GROUND: f32 = PAD[1];

/// The post: a squared baulk, its section and its height above the shingle.
const POST_W: f32 = 0.32;
const POST_H: f32 = 4.1;

/// How far the arm projects, and at what height. Toward `-Z`, the hero side —
/// so the cage hangs between the approach and the post rather than behind it.
const ARM_REACH: f32 = 1.85;
const ARM_Y: f32 = GROUND + POST_H - 0.34;
const ARM_SECTION: f32 = 0.22;

/// Length of the chain from the arm's tip to the cage's head hoop.
const CHAIN_LEN: f32 = 0.62;

/// The cage's hoops: `(height below the head hoop, radius)`.
///
/// A gibbet cage is made to a body, which is why the widest hoop is at the
/// shoulders and not at the middle — and why it tapers to the feet. Getting that
/// order right is most of what makes a basket of straps read as a person-shaped
/// thing rather than as a lobster pot.
const HOOPS: [(f32, f32); 4] = [
    (0.0, 0.19),  // head
    (0.42, 0.34), // shoulders — the widest
    (1.02, 0.29), // hips
    (1.58, 0.17), // feet
];

/// How many straps run down the outside of the cage between the hoops.
const STRAPS: usize = 6;
const STRAP_R: f32 = 0.026;
const HOOP_R: f32 = 0.032;

/// Where the cage's head hoop hangs.
const CAGE_TOP_Y: f32 = ARM_Y - CHAIN_LEN;
const CAGE_Z: f32 = -ARM_REACH;

/// The rope the chain is *not* — this is the radius the chain's guard selects on.
const CHAIN_R: f32 = 0.045;

/// Hero side — the render tool and the settlement placer both look down `-Z`.
const FRONT: f32 = -1.0;

const _: () = assert!(
    ARM_REACH > POST_W * 2.0 + HOOPS[1].1,
    "the arm does not reach far enough for the cage to hang clear of the post — \
     it would grind against it instead of turning"
);
const _: () = assert!(
    CAGE_TOP_Y - HOOPS[3].0 > GROUND + 0.8,
    "the cage's feet are within a stride of the shingle — it reads as standing \
     on the beach rather than hanging over it"
);

pub struct GibbetCage;

impl CatalogueEntry for GibbetCage {
    fn slug(&self) -> &'static str {
        "gibbet_cage"
    }
    fn name(&self) -> &'static str {
        "Gibbet Cage"
    }
    fn description(&self) -> &'static str {
        "An iron cage swinging from a braced post at the tide line, with a warning board nailed \
         beneath it."
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
            clearance: 3.0,
            min_spawn_dist: 13.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The post, its arm, the knee that braces the arm, and the iron bands.
fn gallows() -> Vec<Generator> {
    let arm_tip = [0.0, ARM_Y, CAGE_Z];
    let arm_root = [0.0, ARM_Y, POST_W * 0.5];
    let mut out = vec![
        // The post itself, standing on the shingle.
        prim(
            solid(cuboid_tapered(
                [POST_W, POST_H, POST_W],
                0.06,
                board(HULL_OAK),
            )),
            [0.0, GROUND + POST_H * 0.5, 0.0],
            id_quat(),
        ),
        // A weathered cap, so the head grain is not left open to the rain — the
        // detail that says this thing is *maintained*, which is the horror.
        prim(
            solid(cuboid_tapered(
                [POST_W * 1.5, 0.09, POST_W * 1.5],
                0.25,
                board(WHARF_GREY),
            )),
            [0.0, GROUND + POST_H + 0.045, 0.0],
            id_quat(),
        ),
        // The arm. A strut, so it runs between two points that both exist
        // rather than approximately toward one (#1028).
        strut(arm_root, arm_tip, ARM_SECTION * 0.5, 5, board(HULL_OAK)),
    ];
    // The knee bracing it — from partway down the post to partway out the arm.
    // Both ends are derived from the members they land on, so a retuned reach or
    // height cannot leave the brace floating.
    let knee_post = [0.0, ARM_Y - ARM_REACH * 0.55, POST_W * 0.5];
    let knee_arm = [0.0, ARM_Y - ARM_SECTION * 0.4, CAGE_Z * 0.55];
    out.push(strut(knee_post, knee_arm, 0.075, 5, board(HULL_OAK)));

    // Iron bands round the post at the arm and at the ground line — wrought
    // iron is what keeps a gibbet standing after the timber has gone.
    for (y, r) in [(ARM_Y - 0.16, POST_W * 0.82), (GROUND + 0.5, POST_W * 0.86)] {
        out.push(prim(
            torus(0.03, r, iron(IRON_BLACK, 0xE1)),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }
    // The eye and swivel at the arm's tip, and the chain down to the cage.
    out.push(prim(
        torus(0.028, 0.1, iron(IRON_BLACK, 0xE2)),
        [0.0, ARM_Y - 0.14, CAGE_Z],
        quat_x(FRAC_PI_2),
    ));
    out.push(strut(
        [0.0, ARM_Y - 0.1, CAGE_Z],
        [0.0, CAGE_TOP_Y, CAGE_Z],
        CHAIN_R,
        6,
        iron(IRON_BLACK, 0xE3),
    ));
    out
}

/// The cage: four hoops and the straps between them.
///
/// Every strap is a [`strut`] from one hoop's rim to the next hoop's rim at the
/// same bearing, so the basket's shape is entirely a consequence of [`HOOPS`] —
/// retune the shoulders and the straps follow. Building the straps to guessed
/// endpoints instead is how a basket ends up with its widest point in the wrong
/// place and reads as a lobster pot.
fn cage() -> Vec<Generator> {
    let mut out = Vec::new();
    for (i, (drop, r)) in HOOPS.into_iter().enumerate() {
        out.push(prim(
            torus(HOOP_R, r, iron(IRON_BLACK, 0xE4 + i as u32)),
            [0.0, CAGE_TOP_Y - drop, CAGE_Z],
            id_quat(),
        ));
    }
    for s in 0..STRAPS {
        let a = s as f32 / STRAPS as f32 * 2.0 * PI;
        let (sn, cs) = a.sin_cos();
        for w in HOOPS.windows(2) {
            let ((d0, r0), (d1, r1)) = (w[0], w[1]);
            out.push(strut(
                [cs * r0, CAGE_TOP_Y - d0, CAGE_Z + sn * r0],
                [cs * r1, CAGE_TOP_Y - d1, CAGE_Z + sn * r1],
                STRAP_R,
                4,
                iron(IRON_BLACK, 0xE8 + s as u32),
            ));
        }
    }
    out
}

/// What is left of the occupant, and the cold light coming off it.
///
/// A skull and two long bones. Sized against the cage's own hoops so they read
/// *through* the straps: bones drawn to look right on their own are either lost
/// inside the basket or bulging out of it.
fn occupant() -> Vec<Generator> {
    // Drawn to the head hoop, so the skull nearly fills the top of the cage.
    // That is what a gibbet looks like — the hoop was made to go round a head —
    // and at 0.72 of the hoop it read as a pebble somewhere inside a basket.
    let skull_r = HOOPS[0].1 * 0.88;
    vec![
        prim(
            solid(sphere(skull_r, 4, bone(BONE_PALE))),
            [0.0, CAGE_TOP_Y - HOOPS[0].0 - skull_r * 0.6, CAGE_Z],
            id_quat(),
        ),
        // The jaw, dropped — one prim, and it is what turns a pale ball into a
        // skull at ten metres.
        prim(
            solid(cuboid_tapered(
                [skull_r * 1.05, skull_r * 0.34, skull_r * 0.9],
                0.4,
                bone(BONE_PALE),
            )),
            [
                0.0,
                CAGE_TOP_Y - HOOPS[0].0 - skull_r * 1.5,
                CAGE_Z - skull_r * 0.2,
            ],
            id_quat(),
        ),
        // Two long bones hanging where the legs were, inside the taper.
        prim(
            solid(cylinder_tapered(0.055, 0.66, 5, 0.1, bone(BONE_PALE))),
            [
                HOOPS[2].1 * 0.4,
                CAGE_TOP_Y - HOOPS[2].0 - 0.28,
                CAGE_Z + 0.04,
            ],
            quat_z(0.12),
        ),
        prim(
            solid(cylinder_tapered(0.05, 0.58, 5, 0.1, bone(BONE_PALE))),
            [
                -HOOPS[2].1 * 0.34,
                CAGE_TOP_Y - HOOPS[2].0 - 0.34,
                CAGE_Z - 0.05,
            ],
            quat_z(-0.16),
        ),
        // Witchfire low in the cage, at the hips — small, deep-saturated and at
        // LOW strength, which is the only way this hue survives (see
        // `WITCHFIRE`).
        //
        // Small *and* low for a reason the first render made obvious: a glow
        // ball drawn to half the shoulder hoop sat exactly where the skull is
        // and swamped it, so the cage read as a lantern with something green in
        // it. The light belongs under the bones, lighting them from below, not
        // in front of them.
        prim(
            solid(sphere(HOOPS[2].1 * 0.3, 3, glow(WITCHFIRE, 0.45))),
            [0.0, CAGE_TOP_Y - HOOPS[2].0 - 0.06, CAGE_Z],
            id_quat(),
        ),
    ]
}

fn build_tree() -> Generator {
    let pad_c = [0.0, GROUND * 0.5, 0.0];
    let mut shingle = strand(STRAND_SHINGLE);
    shingle.uv_offset = face_uv_offset(FaceKey::Top, pad_c);

    let mut carried = vec![footing(POST_W * 4.0, POST_W * 4.0, [0.0, 0.0], 3.0)];
    carried.extend(gallows());
    carried.extend(cage());
    carried.extend(occupant());

    // The warning board, nailed to the post below the arm and facing the
    // approach. No lettering is possible, and that is fine: a framed board at
    // head height on a gibbet post reads as a notice, and what it says is
    // obvious from what it is nailed to.
    let board_y = GROUND + 1.9;
    carried.push(prim(
        solid(cuboid_tapered([0.78, 0.52, 0.05], 0.0, board(WHARF_GREY))),
        [0.0, board_y, FRONT * (POST_W * 0.5 + 0.03)],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        carried.push(prim(
            solid(cuboid_tapered([0.06, 0.6, 0.04], 0.0, board(HULL_OAK))),
            [sx * 0.42, board_y, FRONT * (POST_W * 0.5 + 0.05)],
            id_quat(),
        ));
    }

    // Wrack along the tide line, one bone half-buried in the shingle under the
    // cage, and a rotted coil somebody left. Placed off the PAD's own
    // half-extent and each piece's own reach (#972 lesson 8).
    let tide_z = FRONT * (PAD[2] * 0.5 - 0.75);
    for (i, dx) in [-1.7_f32, 0.2, 1.6].into_iter().enumerate() {
        carried.push(prim(
            solid(cuboid_tapered(
                [1.4, 0.06, 0.3],
                0.4,
                tar([0.24, 0.25, 0.19]),
            )),
            [dx, GROUND + 0.03, tide_z + i as f32 * 0.24],
            quat_y(0.16 * i as f32),
        ));
    }
    // One bone in the shingle, directly under the cage — the piece that says
    // this has been going on a while.
    carried.push(prim(
        solid(cylinder_tapered(0.055, 0.5, 5, 0.12, bone(BONE_PALE))),
        [0.34, GROUND + 0.055, CAGE_Z - 0.3],
        quat_z(FRAC_PI_2),
    ));
    carried.push(prim(
        torus(0.045, 0.26, hemp(ROPE_HEMP)),
        [PAD[0] * 0.5 - 0.9, GROUND + 0.045, 1.5],
        id_quat(),
    ));
    // Two beach stones, so the shingle is not a bare plane under all this.
    for (i, (dx, dz)) in [(-1.55_f32, 1.35_f32), (1.35, 0.55)]
        .into_iter()
        .enumerate()
    {
        carried.push(prim(
            solid(sphere(0.2 + i as f32 * 0.05, 3, board([0.44, 0.42, 0.38]))),
            [dx, GROUND + 0.08, dz],
            id_quat(),
        ));
    }

    let mut root = nest(
        prim(solid(cuboid_tapered(PAD, 0.0, shingle)), pad_c, id_quat()),
        carried,
    );
    root.audio = fx::witchfire_hiss();
    let fire = fx::witchfire([0.0, CAGE_TOP_Y - HOOPS[2].0, CAGE_Z], 0xE0);
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
        GibbetCage.build("")
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&built(), "gibbet_cage");
    }

    #[test]
    fn no_rotated_node_carries_an_offset_child() {
        assert_no_tilted_parents(&built(), "gibbet_cage");
    }

    #[test]
    fn the_gibbet_carries_no_glazing() {
        let g = built();
        assert_no_glazing_on_solids(&g, "gibbet_cage");
        assert!(window_cards(&g).is_empty(), "a gibbet has grown a window");
    }

    /// The cage hangs: clear of the post, clear of the shingle, and off the
    /// arm's tip.
    ///
    /// Three separate facts, and all three are what "hanging" means. A cage
    /// touching the post cannot turn; a cage touching the ground is standing;
    /// and a chain that stops short of either end is the fault class this kit
    /// retired with `strut` (#1028, #1030). Measured off the built prims.
    #[test]
    fn the_cage_hangs_clear_of_the_post_and_the_beach() {
        let solids = measure::solids(&built());
        // The cage's own extent: its hoops, selected on the section that
        // defines them.
        let hoops: Vec<_> = solids
            .iter()
            .filter(|p| p.kind_tag == "Torus")
            .filter(|p| p.bounds.size().y < HOOP_R * 3.0 && p.bounds.center().z < CAGE_Z + 0.3)
            .collect();
        assert_eq!(
            hoops.len(),
            HOOPS.len(),
            "expected {} cage hoops, found {}",
            HOOPS.len(),
            hoops.len()
        );
        let feet = hoops
            .iter()
            .map(|h| h.bounds.min.y)
            .fold(f32::MAX, f32::min);
        assert!(
            feet > GROUND + 0.8,
            "the cage's lowest hoop is {} above the shingle — it reads as \
             standing on the beach, not hanging over it",
            feet - GROUND
        );
        // Clear of the post in Z: the whole cage is forward of the post's face.
        let widest = hoops
            .iter()
            .map(|h| h.bounds.max.z)
            .fold(f32::MIN, f32::max);
        assert!(
            widest < -POST_W * 0.5 - 0.2,
            "the cage reaches back to z = {widest}, within 200 mm of a post face \
             at {} — it would grind against the post instead of turning",
            -POST_W * 0.5
        );
        // And the chain actually joins the arm's tip to the head hoop.
        let mut chains = Vec::new();
        fn chains_of(g: &Generator, at: [f32; 3], out: &mut Vec<([f32; 3], [f32; 3])>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let K::Cylinder { radius, height, .. } = &g.kind
                && (radius.0 - CHAIN_R).abs() < 0.003
            {
                let tip = rotate_by(g.transform.rotation.0, [0.0, height.0 * 0.5, 0.0]);
                out.push((
                    [here[0] + tip[0], here[1] + tip[1], here[2] + tip[2]],
                    [here[0] - tip[0], here[1] - tip[1], here[2] - tip[2]],
                ));
            }
            for c in &g.children {
                chains_of(c, here, out);
            }
        }
        chains_of(&built(), [0.0; 3], &mut chains);
        assert_eq!(
            chains.len(),
            1,
            "expected one chain, found {}",
            chains.len()
        );
        let (a, b) = chains[0];
        let (hi, lo) = if a[1] > b[1] { (a, b) } else { (b, a) };
        assert!(
            (hi[1] - ARM_Y).abs() < 0.16 && (hi[2] - CAGE_Z).abs() < 0.05,
            "the chain's upper end at {hi:?} is not made fast at the arm's tip"
        );
        let head = hoops
            .iter()
            .max_by(|x, y| {
                x.bounds
                    .center()
                    .y
                    .partial_cmp(&y.bounds.center().y)
                    .expect("finite")
            })
            .expect("checked above");
        assert!(
            (lo[1] - head.bounds.center().y).abs() < 0.08,
            "the chain's lower end is at {} and the head hoop is at {} — the \
             cage is hanging on nothing",
            lo[1],
            head.bounds.center().y
        );
    }

    /// The arm is braced, and the knee lands on both the post and the arm.
    ///
    /// An unbraced arm carrying a cage at the end of nearly two metres is the
    /// thing a viewer notices without knowing why. Read from the built struts.
    #[test]
    fn the_arm_is_braced_against_the_post() {
        let mut struts = Vec::new();
        fn struts_of(g: &Generator, at: [f32; 3], out: &mut Vec<(f32, [f32; 3], [f32; 3])>) {
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
                struts_of(c, here, out);
            }
        }
        struts_of(&built(), [0.0; 3], &mut struts);
        let knee = struts
            .iter()
            .find(|(r, _, _)| (r - 0.075).abs() < 0.003)
            .expect("the arm carries a knee brace");
        let (_, a, b) = knee;
        let (hi, lo) = if a[1] > b[1] { (a, b) } else { (b, a) };
        // The upper end is out along the arm, under it.
        assert!(
            hi[2] < -0.4 && (hi[1] - ARM_Y).abs() < 0.3,
            "the knee's upper end at {hi:?} does not land under the arm"
        );
        // The lower end is on the post, well below the arm.
        assert!(
            lo[2].abs() < POST_W && lo[1] < ARM_Y - 0.6,
            "the knee's lower end at {lo:?} does not land on the post"
        );
        // The arm itself reaches the point the chain hangs from.
        let arm = struts
            .iter()
            .find(|(r, _, _)| (r - ARM_SECTION * 0.5).abs() < 0.003)
            .expect("there is an arm");
        let reach = arm.1[2].min(arm.2[2]);
        assert!(
            (reach - CAGE_Z).abs() < 0.05,
            "the arm reaches z = {reach} but the cage hangs at {CAGE_Z}"
        );
    }

    /// The cage is a basket, not a shell — every strap joins two consecutive
    /// hoops at one bearing, so the shape follows [`HOOPS`].
    #[test]
    fn every_strap_joins_two_hoops() {
        let mut straps = Vec::new();
        fn straps_of(g: &Generator, at: [f32; 3], out: &mut Vec<([f32; 3], [f32; 3])>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let K::Cylinder { radius, height, .. } = &g.kind
                && (radius.0 - STRAP_R).abs() < 0.003
            {
                let tip = rotate_by(g.transform.rotation.0, [0.0, height.0 * 0.5, 0.0]);
                out.push((
                    [here[0] + tip[0], here[1] + tip[1], here[2] + tip[2]],
                    [here[0] - tip[0], here[1] - tip[1], here[2] - tip[2]],
                ));
            }
            for c in &g.children {
                straps_of(c, here, out);
            }
        }
        straps_of(&built(), [0.0; 3], &mut straps);
        assert_eq!(
            straps.len(),
            STRAPS * (HOOPS.len() - 1),
            "expected {} straps, found {}",
            STRAPS * (HOOPS.len() - 1),
            straps.len()
        );
        for (a, b) in &straps {
            for end in [a, b] {
                // Each end lies on some hoop's plane, at that hoop's own radius.
                let hit = HOOPS.iter().any(|(drop, r)| {
                    let y = CAGE_TOP_Y - drop;
                    let dx = end[0];
                    let dz = end[2] - CAGE_Z;
                    (end[1] - y).abs() < 0.02 && ((dx * dx + dz * dz).sqrt() - r).abs() < 0.02
                });
                assert!(
                    hit,
                    "a strap ends at {end:?}, which is on no hoop's rim — the \
                     basket's shape has stopped following its own hoops"
                );
            }
        }
        // The widest hoop is at the shoulders, not the middle: that is what
        // makes a basket of straps read as person-shaped.
        let widest = HOOPS
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.1.partial_cmp(&b.1.1).expect("finite"))
            .map(|(i, _)| i)
            .expect("non-empty");
        assert_eq!(
            widest, 1,
            "the cage's widest hoop is number {widest} — a gibbet cage is made \
             to a body, so it belongs at the shoulders"
        );
    }

    /// One warning, stated once: a skull and two long bones inside, and one
    /// bone in the shingle. Not a beach of skulls.
    #[test]
    fn the_bones_are_stated_once() {
        let bones: Vec<_> = measure::solids(&built())
            .into_iter()
            .filter(|p| {
                // Bone is the kit's one untextured pale material; select on it
                // rather than on shape, since a skull and a femur share none
                // (#972 lesson 24).
                p.kind_tag == "Sphere" || p.kind_tag == "Cylinder" || p.kind_tag == "Cuboid"
            })
            .collect();
        assert!(!bones.is_empty(), "nothing was measured");
        fn count_bone(g: &Generator) -> usize {
            let own = g
                .kind
                .material()
                .is_some_and(|m| m.base_color.0 == BONE_PALE) as usize;
            own + g.children.iter().map(count_bone).sum::<usize>()
        }
        let n = count_bone(&built());
        assert_eq!(
            n, 5,
            "found {n} bone pieces — a skull, its jaw, two long bones in the \
             cage and one in the shingle is the whole statement; more reads as \
             a joke rather than as a warning"
        );
    }

    /// Everything stands on the shingle it is nested under (#972 lessons 8, 19).
    #[test]
    fn every_part_stands_on_the_pad() {
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
        assert!(checked > 25, "only {checked} parts examined");
    }
}
