//! Mailbox — a Suburban prop. A roadside post-mounted mailbox: a timber post
//! with a braced arm, a tunnel-topped box with a hinged door toward the
//! road, and a raised red flag on its side.
//!
//! Rebuilt from scratch under #972 after an in-world check ("z-fighting on
//! co-planar surfaces and the box floats above the post"). The shipped box
//! was a cuboid with a full [`cylinder_tapered`] laid on it as a lid —
//! exactly as long as the box, so the lid's end discs sat on the box's end
//! faces and tied for depth (the z-fight), with the lid's lower half buried
//! in the body — and the whole thing hung 80 mm above the post top with
//! nothing between. Now: post → mounting board → box, one unbroken stack
//! (lesson 33); a diagonal [`strut`] brace from the post to the board; a
//! lid that is a HALF cylinder ([`with_cut`]) whose flat is sunk 2 mm into
//! the body so no face of it meets a face of the body on one plane; a door
//! proud of the road end with the same profile; a latch, a raised flag on
//! its pivot, and a house-number plate.
//!
//! #972 lesson 37: **a rounded top is a half, not a cylinder buried to its
//! axis.** A full cylinder as long as its box puts two discs on the box's
//! two ends — same plane, same normal, same rectangle — which is the
//! definition of a z-fight, and it costs the whole lower half of the
//! cylinder in hidden geometry too. Cut the half you can see
//! (`path_cut`), then check the kept arc's midpoint actually points UP by
//! rotating it through the built quaternion: with the cylinder turned
//! `quat_x(+π/2)` the upper half is the second half-turn, `[0.5, 1.0]`, and
//! the other one is a trough under the box. `util::assert_no_coplanar_faces`
//! now states the depth-tie rule for every item that runs it.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, strut, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{WOOD_BROWN, enamel, wood};

/// Mailbox-flag red.
const FLAG_RED: [f32; 3] = [0.66, 0.14, 0.12];
const BOX_GREY: [f32; 3] = [0.5, 0.5, 0.55];
const DOOR_GREY: [f32; 3] = [0.42, 0.42, 0.47];
const PLATE_DARK: [f32; 3] = [0.15, 0.15, 0.18];

const POST_W: f32 = 0.1;
const POST_H: f32 = 1.05;
/// Mounting board on the post top, the box's own slab.
const BOARD: [f32; 3] = [0.3, 0.035, 0.55];
const BOARD_Y: f32 = POST_H + BOARD[1] * 0.5;
/// Box: rectangular lower body with a half-round lid, long axis `Z`, door
/// toward the road (`-Z`).
const BOX_W: f32 = 0.22;
const BOX_LEN: f32 = 0.5;
const BODY_H: f32 = 0.13;
const BODY_Y: f32 = POST_H + BOARD[1] + BODY_H * 0.5;
const BODY_TOP: f32 = BODY_Y + BODY_H * 0.5;
const LID_R: f32 = BOX_W * 0.5;
/// The lid's flat is sunk this far into the body so the two never share a
/// plane.
const LID_SINK: f32 = 0.002;
/// The upper half of a cylinder turned `quat_x(+π/2)`: angles π..2π.
const UPPER_HALF: [f32; 2] = [0.5, 1.0];
/// Door: the same profile 5 mm bigger, proud of the road end.
const DOOR_T: f32 = 0.02;
const DOOR_LIP: f32 = 0.005;
const DOOR_Z: f32 = -BOX_LEN * 0.5 - DOOR_T * 0.5;
/// Flag pivot on the `+X` side, arm raised.
const FLAG_Z: f32 = -0.05;
const ARM_H: f32 = 0.2;
/// Diagonal brace from inside the post to inside the board's road end.
const BRACE_FOOT: [f32; 3] = [0.0, 0.72, 0.0];
const BRACE_HEAD: [f32; 3] = [0.0, POST_H + 0.012, -0.2];

pub struct Mailbox;

impl CatalogueEntry for Mailbox {
    fn slug(&self) -> &'static str {
        "mailbox"
    }
    fn name(&self) -> &'static str {
        "Mailbox"
    }
    fn description(&self) -> &'static str {
        "Post-mounted roadside mailbox with a rounded lid and a red flag."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Suburban]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SUB_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.8,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// A half-round lid of radius `r` and length `len` lying along `Z`, its
/// flat at `y` (sunk by `LID_SINK`) — the box's top and the door's crown.
fn half_lid(r: f32, len: f32, at: [f32; 3], colour: [f32; 3]) -> Generator {
    prim(
        solid(with_cut(
            cylinder_tapered(r, len, 14, 0.0, enamel(colour)),
            UPPER_HALF,
            [0.0, 1.0],
            0.0,
        )),
        [at[0], at[1] - LID_SINK, at[2]],
        quat_x(FRAC_PI_2),
    )
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Timber post — the root.
        prim(
            solid(cuboid_tapered(
                [POST_W, POST_H, POST_W],
                0.0,
                wood(WOOD_BROWN),
            )),
            [0.0, POST_H * 0.5, 0.0],
            id_quat(),
        ),
        // Mounting board the box stands on.
        prim(
            solid(cuboid_tapered(BOARD, 0.0, wood(WOOD_BROWN))),
            [0.0, BOARD_Y, 0.0],
            id_quat(),
        ),
        // Diagonal brace under the board's road end.
        strut(BRACE_FOOT, BRACE_HEAD, 0.02, 8, wood(WOOD_BROWN)),
        // Box body and its half-round lid.
        prim(
            solid(cuboid_tapered(
                [BOX_W, BODY_H, BOX_LEN],
                0.0,
                enamel(BOX_GREY),
            )),
            [0.0, BODY_Y, 0.0],
            id_quat(),
        ),
        half_lid(LID_R, BOX_LEN, [0.0, BODY_TOP, 0.0], BOX_GREY),
        // Hinged door on the road end, the same profile with a lip.
        prim(
            solid(cuboid_tapered(
                [BOX_W + DOOR_LIP * 2.0, BODY_H, DOOR_T],
                0.0,
                enamel(DOOR_GREY),
            )),
            [0.0, BODY_Y, DOOR_Z],
            id_quat(),
        ),
        half_lid(LID_R + DOOR_LIP, DOOR_T, [0.0, BODY_TOP, DOOR_Z], DOOR_GREY),
        // Latch tab at the top of the door.
        prim(
            cuboid_tapered([0.03, 0.04, 0.015], 0.0, enamel(PLATE_DARK)),
            [0.0, BODY_TOP + 0.03, DOOR_Z - DOOR_T * 0.5 - 0.0075],
            id_quat(),
        ),
        // House-number plate on the post, facing the road.
        prim(
            cuboid_tapered([0.14, 0.09, 0.01], 0.0, enamel(PLATE_DARK)),
            [0.0, 0.62, -POST_W * 0.5 - 0.005],
            id_quat(),
        ),
    ];

    // Raised flag on the +X side: a pivot pin, an arm up from it, a plate.
    let pin_x = BOX_W * 0.5 + 0.015;
    let arm_x = BOX_W * 0.5 + 0.03 + 0.0075;
    prims.push(prim(
        cylinder_tapered(0.012, 0.03, 8, 0.0, enamel(PLATE_DARK)),
        [pin_x, BODY_Y, FLAG_Z],
        crate::catalogue::items::util::quat_z(FRAC_PI_2),
    ));
    prims.push(prim(
        cuboid_tapered([0.015, ARM_H, 0.025], 0.0, enamel(FLAG_RED)),
        [arm_x, BODY_Y + ARM_H * 0.5, FLAG_Z],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.012, 0.09, 0.12], 0.0, enamel(FLAG_RED)),
        [arm_x, BODY_Y + ARM_H + 0.04, FLAG_Z],
        id_quat(),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_no_coplanar_faces, assert_no_tilted_parents, assert_sanitize_stable, rotate_by,
    };
    use crate::pds::{GeneratorKind, PrimCommon};
    use std::f32::consts::TAU;

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
        assert_sanitize_stable(&Mailbox.build(""), "mailbox");
    }

    #[test]
    fn no_sub_assembly_hangs_off_a_tilted_root() {
        assert_no_tilted_parents(&Mailbox.build(""), "mailbox");
    }

    /// Lesson 37's rule, stated for the whole tree. Against the shipped
    /// build this names the lid's end discs on the body's end faces.
    #[test]
    fn no_two_faces_tie_for_depth() {
        assert_no_coplanar_faces(&Mailbox.build(""), "mailbox");
    }

    /// **The box does not float.** Every upright box on the post's axis,
    /// sorted by height, chains from the ground to the box body's underside
    /// with no gap (#972 lesson 33). Against the shipped build: the post
    /// reaches 1.1 and the box starts at 1.18.
    #[test]
    fn the_box_is_carried_by_an_unbroken_stack() {
        let mut spans: Vec<(f32, f32)> = Vec::new();
        let mut body_bottom = f32::MAX;
        walk(&Mailbox.build(""), [0.0; 3], &mut |g, at| {
            if g.transform.rotation.0 != [0.0, 0.0, 0.0, 1.0] || at[0].abs() > 0.02 {
                return;
            }
            if let GeneratorKind::Cuboid { size, .. } = &g.kind {
                let (lo, hi) = (at[1] - size.0[1] * 0.5, at[1] + size.0[1] * 0.5);
                // The body is the widest thing above the post, along Z.
                if size.0[2] > 0.4 && size.0[0] > 0.2 && at[2].abs() < 0.01 {
                    body_bottom = body_bottom.min(lo);
                }
                spans.push((lo, hi));
            }
        });
        assert!(body_bottom < f32::MAX, "no box body found");
        spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let mut reach = 0.0_f32;
        for (lo, hi) in &spans {
            if *lo > reach + 1e-4 {
                break;
            }
            reach = reach.max(*hi);
        }
        assert!(
            reach >= body_bottom - 1e-4,
            "mailbox: the stack under the box reaches {reach} and the box starts at \
             {body_bottom} — it floats on {} m of air",
            body_bottom - reach
        );
    }

    /// **The lid is the upper half, and its crown points up.** Read from the
    /// built tree: the kept arc's midpoint (`cos`, `sin` in the prim's own
    /// XZ) rotated by the built quaternion must land on `+Y`, and its flat
    /// must sit just inside the body's top. Choosing the other half gives a
    /// trough under the box with nothing in the record looking wrong.
    #[test]
    fn the_lid_is_a_half_with_its_crown_up_sunk_into_the_body() {
        let root = Mailbox.build("");
        let mut lids = 0;
        walk(&root, [0.0; 3], &mut |g, at| {
            let GeneratorKind::Cylinder {
                radius,
                height,
                common: PrimCommon { torture, .. },
                ..
            } = &g.kind
            else {
                return;
            };
            let cut = torture.path_cut.0;
            if (cut[1] - cut[0] - 1.0).abs() < 1e-4 {
                return; // an uncut drum: the pin
            }
            lids += 1;
            assert!(
                (cut[1] - cut[0] - 0.5).abs() < 1e-4,
                "mailbox: a lid keeps {cut:?} of a turn, not a half"
            );
            let mid = (cut[0] + cut[1]) * 0.5 * TAU;
            let crown = rotate_by(g.transform.rotation.0, [mid.cos(), 0.0, mid.sin()]);
            assert!(
                crown[1] > 0.999,
                "mailbox: a lid at {at:?} has its crown pointing {crown:?} — the wrong half \
                 was kept, or the turn is wrong"
            );
            // The lid's axis runs along Z and its flat is just inside the body top.
            let axis = rotate_by(g.transform.rotation.0, [0.0, 1.0, 0.0]);
            assert!(
                axis[2].abs() > 0.999,
                "a lid's axis runs {axis:?}, not along Z"
            );
            assert!(
                at[1] < BODY_TOP && at[1] > BODY_TOP - 0.01,
                "mailbox: a lid's flat at {} is not sunk into the body top at {BODY_TOP}",
                at[1]
            );
            assert!(radius.0 >= LID_R && height.0 > 0.0);
        });
        assert_eq!(lids, 2, "the box lid and the door's crown");
    }

    /// The door stands proud of the road end with a lip all round, and the
    /// brace lands inside the post and inside the board (ends read from the
    /// built strut).
    #[test]
    fn the_door_is_proud_of_the_road_end_and_the_brace_lands_in_both_members() {
        let root = Mailbox.build("");
        let mut body: Option<([f32; 3], [f32; 3])> = None;
        let mut door: Option<([f32; 3], [f32; 3])> = None;
        let mut brace: Option<([f32; 3], [f32; 3])> = None;
        let mut post: Option<([f32; 3], [f32; 3])> = None;
        let mut board: Option<([f32; 3], [f32; 3])> = None;
        walk(&root, [0.0; 3], &mut |g, at| match &g.kind {
            GeneratorKind::Cuboid { size, .. } => {
                let s = size.0;
                if s[2] > 0.4 && s[0] > 0.2 && s[0] < 0.25 {
                    body = Some((at, s));
                } else if s[2] < 0.03 && s[1] > 0.1 && s[0] > 0.2 {
                    // As wide as the box and thin along it — not the flag's arm.
                    door = Some((at, s));
                } else if s[1] > 1.0 {
                    post = Some((at, s));
                } else if s[2] > 0.5 {
                    board = Some((at, s));
                }
            }
            // The brace is the one thin long drum; the lids are as long but
            // five times as thick.
            GeneratorKind::Cylinder { radius, height, .. } if height.0 > 0.2 && radius.0 < 0.03 => {
                let tip = rotate_by(g.transform.rotation.0, [0.0, height.0 * 0.5, 0.0]);
                brace = Some((
                    [at[0] - tip[0], at[1] - tip[1], at[2] - tip[2]],
                    [at[0] + tip[0], at[1] + tip[1], at[2] + tip[2]],
                ));
            }
            _ => {}
        });
        let (b, bs) = body.expect("a box body");
        let (d, ds) = door.expect("a door");
        assert!(
            d[2] + ds[2] * 0.5 <= b[2] - bs[2] * 0.5 + 1e-4 && d[2] < b[2],
            "mailbox: the door at {d:?} is not proud of the body's road end"
        );
        assert!(
            ds[0] > bs[0] && (ds[1] - bs[1]).abs() < 1e-4,
            "the door has no lip"
        );
        let (a, c) = brace.expect("a brace strut");
        let (lo, hi) = if a[1] < c[1] { (a, c) } else { (c, a) };
        let inside = |p: [f32; 3], (at, s): ([f32; 3], [f32; 3])| {
            (0..3).all(|i| (p[i] - at[i]).abs() <= s[i] * 0.5 + 1e-4)
        };
        assert!(
            inside(lo, post.expect("a post")),
            "the brace's foot at {lo:?} misses the post"
        );
        assert!(
            inside(hi, board.expect("a board")),
            "the brace's head at {hi:?} misses the board"
        );
    }

    /// The flag stands raised on the box's side, its arm rising from its
    /// pivot and its plate on top of the arm.
    #[test]
    fn the_flag_is_raised_on_the_side_of_the_box() {
        let root = Mailbox.build("");
        let mut reds: Vec<([f32; 3], [f32; 3])> = Vec::new();
        walk(&root, [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Cuboid {
                size,
                common: PrimCommon { material, .. },
                ..
            } = &g.kind
                && material.base_color.0 == FLAG_RED
            {
                reds.push((at, size.0));
            }
        });
        assert_eq!(reds.len(), 2, "an arm and a plate");
        reds.sort_by(|a, b| a.0[1].partial_cmp(&b.0[1]).unwrap());
        let (arm, arm_s) = reds[0];
        let (plate, plate_s) = reds[1];
        assert!(
            arm[0] - arm_s[0] * 0.5 >= BOX_W * 0.5,
            "the flag arm is inside the box"
        );
        assert!(
            (plate[1] - plate_s[1] * 0.5) < arm[1] + arm_s[1] * 0.5 && plate[1] > arm[1],
            "the plate is not on top of the arm"
        );
        assert!(
            arm[1] - arm_s[1] * 0.5 < BODY_TOP,
            "the arm does not reach down to its pivot"
        );
    }
}
