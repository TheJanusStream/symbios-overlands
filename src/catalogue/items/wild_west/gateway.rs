//! Frontier Gate — the Wild-West bespoke social gateway (#772). A ranch-style
//! entrance arch: two hewn timber posts on fieldstone footings carry a heavy
//! header beam, a painted board hung in the opening bears a branded wagon-wheel
//! emblem facing the street, and a caged oil lamp glows amber on each post to
//! light the threshold. It replaces the neutral placeholder gate for the theme.
//!
//! The functional element is the single [`GeneratorKind::Gateway`] zone child —
//! walking into the opening between the posts opens the destination picker.
//! Everything else is frontier set-dressing framing that walk-through.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], whose first prim (the stone threshold slab) is the untilted
//! root — a rotated root would spin every post, beam and lamp with it. The
//! render FRONT is −Z, so the hung sign and its emblem face −Z.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, foundation_mat, glow, id_quat, prim, quat_x, quat_z, solid,
    sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{CLAP_TAN, CLAP_WHITE, IRON_DARK, STONE_TAN, WOOD_RAW, clapboard, fx, iron, stone};

pub struct WildWestGateway;

impl CatalogueEntry for WildWestGateway {
    fn slug(&self) -> &'static str {
        "wild_west_gateway"
    }
    fn name(&self) -> &'static str {
        "Frontier Gate"
    }
    fn description(&self) -> &'static str {
        "Ranch-arch of timber posts and a header beam, a wagon-wheel sign and amber lamps lighting the way through."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::WildWest]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.5,
            min_spawn_dist: 8.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // Posts flank a ~2.9 m walk-through gap; front (the signed face) is −Z.
    let px = 1.7_f32;
    let footing_top = 1.0_f32;
    let post_h = 3.6_f32;
    let post_top = footing_top + post_h; // 4.6

    let mut prims = vec![
        // Fieldstone threshold slab — the flat-base root (never tilt a root:
        // every post, beam and lamp would spin into its frame).
        prim(
            solid(cuboid_tapered([5.4, 0.3, 3.0], 0.0, foundation_mat())),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];

    for sx in [-1.0_f32, 1.0] {
        let x = sx * px;
        // Fieldstone footing the post is stepped up on.
        prims.push(prim(
            solid(cuboid_tapered([0.9, 0.7, 1.0], 0.0, stone(STONE_TAN))),
            [x, 0.65, 0.0],
            id_quat(),
        ));
        // Hewn timber post, lightly tapered so it reads as a squared log.
        prims.push(prim(
            solid(cuboid_tapered(
                [0.5, post_h, 0.5],
                0.06,
                clapboard(WOOD_RAW),
            )),
            [x, footing_top + post_h * 0.5, 0.0],
            id_quat(),
        ));
        // Iron banding straps hooping the post near base and head.
        for y in [1.35_f32, 4.25] {
            prims.push(prim(
                solid(torus(0.06, 0.37, iron(IRON_DARK))),
                [x, y, 0.0],
                id_quat(),
            ));
        }
        // Bolted iron corner plate strapping the beam down onto the post,
        // face-mounted on the −Z front so the joint reads.
        prims.push(prim(
            solid(cuboid_tapered([0.5, 0.85, 0.1], 0.0, iron(IRON_DARK))),
            [x, 4.45, -0.34],
            id_quat(),
        ));
    }

    // Heavy header beam spanning and oversailing the posts.
    prims.push(prim(
        solid(cuboid_tapered([4.6, 0.6, 0.7], 0.0, clapboard(WOOD_RAW))),
        [0.0, post_top + 0.3, 0.0],
        id_quat(),
    ));
    // Overhanging painted cornice board along the top — the false-front cap.
    prims.push(prim(
        solid(cuboid_tapered([4.9, 0.22, 0.85], 0.0, clapboard(CLAP_TAN))),
        [0.0, post_top + 0.71, 0.0],
        id_quat(),
    ));

    // Painted board hung in the opening below the beam, facing −Z.
    let sign_y = 4.0_f32;
    let sign_z = -0.42_f32;
    prims.push(prim(
        solid(cuboid_tapered(
            [2.6, 0.72, 0.08],
            0.0,
            clapboard(CLAP_WHITE),
        )),
        [0.0, sign_y, sign_z],
        id_quat(),
    ));
    // Iron hangers suspending the board from the beam.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.05, 0.34, 0.05], 0.0, iron(IRON_DARK))),
            [sx * 0.65, 4.53, sign_z + 0.02],
            id_quat(),
        ));
    }

    // Branded wagon-wheel emblem on the board, standing proud toward −Z: an
    // iron rim, hub and eight spokes (two crossed bars + two diagonals).
    let wz = sign_z - 0.08;
    prims.push(prim(
        solid(torus(0.05, 0.26, iron(IRON_DARK))),
        [0.0, sign_y, wz],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        solid(sphere(0.06, 4, iron(IRON_DARK))),
        [0.0, sign_y, wz],
        id_quat(),
    ));
    for (size, rot) in [
        ([0.5_f32, 0.045, 0.03], id_quat()),
        ([0.045, 0.5, 0.03], id_quat()),
        ([0.5, 0.045, 0.03], quat_z(FRAC_PI_4)),
        ([0.5, 0.045, 0.03], quat_z(-FRAC_PI_4)),
    ] {
        prims.push(prim(
            solid(cuboid_tapered(size, 0.0, iron(IRON_DARK))),
            [0.0, sign_y, wz],
            rot,
        ));
    }

    // A caged oil lamp on an iron arm off each post, lighting the threshold.
    // The small amber flame runs warm-hot as a thin element without blooming
    // the broad faces white; the iron cap and hanger read it as a hung lamp.
    for sx in [-1.0_f32, 1.0] {
        // Arm reaching in from the post's inner face over the opening.
        prims.push(prim(
            solid(cuboid_tapered([0.4, 0.06, 0.06], 0.0, iron(IRON_DARK))),
            [sx * 1.25, 3.7, -0.1],
            id_quat(),
        ));
        // Hanger rod down to the lamp.
        prims.push(prim(
            solid(cuboid_tapered([0.03, 0.28, 0.03], 0.0, iron(IRON_DARK))),
            [sx * 1.05, 3.52, -0.1],
            id_quat(),
        ));
        // Iron cap over the flame.
        prims.push(prim(
            solid(cone(0.16, 0.14, 8, iron(IRON_DARK))),
            [sx * 1.05, 3.28, -0.1],
            id_quat(),
        ));
        // The amber flame — deep-saturated warm glow, small so it stays lit
        // colour not white bloom.
        prims.push(prim(
            sphere(0.14, 4, glow([1.0, 0.66, 0.28], 4.0)),
            [sx * 1.05, 3.12, -0.1],
            id_quat(),
        ));
    }

    // The walk-in zone between the posts: floor at the slab top, headroom up
    // to just under the hung board.
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.6, 3.2, 1.4]),
        },
        [0.0, 1.8, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: the dry prairie wind breathing over the empty street.
    root.audio = fx::prairie_wind();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&WildWestGateway.build(""), "wild_west_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is set-dressing, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = WildWestGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
