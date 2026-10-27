//! Yakuimon Gateway - a family gate for feudal Japan (#1796), stood beside
//! the theme's own torii. A roofed wooden gate: two heavy main posts and two
//! lighter rear posts on stone bases, tied by pierced beams and a lintel,
//! carry a tiled hip roof; the iron-studded leaves stand open, folded back
//! along the sides of the way, and a pair of paper lanterns hangs from the
//! front eave.
//!
//! `themes()` stays empty, as `civic_gateway`'s does: the gate reaches the
//! feudal Japan pool through `kindred_gates`.

use crate::catalogue::items::feudal_japan::{
    LANTERN_GLOW, PAPER_CREAM, STONE_GREY, TILE_SLATE, TIMBER_BROWN, TIMBER_DARK, roof_tile, stone,
    timber,
};
use crate::catalogue::items::medieval::{IRON_DARK, iron};
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, footing, glow, id_quat, prim,
    solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};

pub struct YakuimonGateway;

impl CatalogueEntry for YakuimonGateway {
    fn slug(&self) -> &'static str {
        "yakuimon_gateway"
    }
    fn name(&self) -> &'static str {
        "Yakuimon Gateway"
    }
    fn description(&self) -> &'static str {
        "A roofed wooden gate under a tiled hip roof, its studded leaves standing open."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn footprint(&self) -> Footprint {
        super::gate_footprint()
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Half the opening between the main posts' inner faces (m).
const HALF_OPENING: f32 = 1.4;
/// Top of the stone sill.
const SILL_TOP: f32 = 0.3;
/// Underside of the front lintel, the head of the opening.
const LINTEL_UNDER: f32 = 3.64;
/// Where the main posts stand, and the rear posts behind them.
const MAIN_Z: f32 = -0.5;
const REAR_Z: f32 = 0.9;
/// Underside of the side beams the roof rests on.
const BEAM_UNDER: f32 = 4.06;

fn build_tree() -> Generator {
    let main_w = 0.42;
    let post_x = HALF_OPENING + main_w * 0.5;
    let mid_z = (MAIN_Z + REAR_Z) * 0.5;
    let base_top = SILL_TOP + 0.16;

    let mut prims = vec![footing(5.4, 3.0, [0.0, mid_z], 3.5)];
    // Dressed-stone sill under the whole gate.
    prims.push(prim(
        solid(cuboid_tapered([5.4, SILL_TOP, 3.0], 0.0, stone(STONE_GREY))),
        [0.0, SILL_TOP * 0.5, mid_z],
        id_quat(),
    ));

    for sx in [-1.0_f32, 1.0] {
        let x = sx * post_x;
        // Stone bases, then the posts, each sunk 2 cm into what it stands
        // on; the post tops stop inside the beams over them.
        for (z, w, top) in [(MAIN_Z, main_w, 4.0_f32), (REAR_Z, 0.3, 4.1)] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [w + 0.2, 0.18, w + 0.2],
                    0.15,
                    stone(STONE_GREY),
                )),
                [x, SILL_TOP + 0.07, z],
                id_quat(),
            ));
            prims.push(prim(
                solid(cuboid_tapered(
                    [w, top - base_top + 0.02, w],
                    0.0,
                    timber(TIMBER_BROWN),
                )),
                [x, (top + base_top - 0.02) * 0.5, z],
                id_quat(),
            ));
        }
        // Pierced tie beams through the main and rear posts.
        for y in [1.1_f32, 3.3] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.14, 0.24, REAR_Z - MAIN_Z + 0.5],
                    0.0,
                    timber(TIMBER_DARK),
                )),
                [x, y, mid_z],
                id_quat(),
            ));
        }
        // The side beam over each pair of posts, carrying the roof.
        prims.push(prim(
            solid(cuboid_tapered(
                [0.34, 0.3, REAR_Z - MAIN_Z + 0.9],
                0.0,
                timber(TIMBER_DARK),
            )),
            [x, BEAM_UNDER + 0.15, mid_z],
            id_quat(),
        ));
        prims.extend(leaf(sx));
        prims.extend(lantern([sx * 1.0, 0.0, -1.2]));
    }

    // Front lintel through the main posts' heads, and a lighter rear one.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.0 * post_x + 0.5, BEAM_UNDER - LINTEL_UNDER, 0.5],
            0.0,
            timber(TIMBER_DARK),
        )),
        [0.0, (BEAM_UNDER + LINTEL_UNDER) * 0.5, MAIN_Z],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [2.0 * post_x + 0.4, 0.3, 0.36],
            0.0,
            timber(TIMBER_DARK),
        )),
        [0.0, BEAM_UNDER - 0.15, REAR_Z],
        id_quat(),
    ));

    // Eave board and the tiled hip roof, its ridge along the way's width.
    let eave_y = BEAM_UNDER + 0.3;
    prims.push(prim(
        solid(cuboid_tapered([5.5, 0.14, 3.9], 0.0, timber(TIMBER_DARK))),
        [0.0, eave_y + 0.07, mid_z],
        id_quat(),
    ));
    let roof_h = 1.45;
    let roof_y = eave_y + 0.13;
    prims.push(prim(
        solid(cuboid_tapered_xz(
            [5.4, roof_h, 3.8],
            [0.5, 0.92],
            roof_tile(TILE_SLATE),
        )),
        [0.0, roof_y + roof_h * 0.5, mid_z],
        id_quat(),
    ));
    // Ridge cap and its two end tiles.
    let ridge_y = roof_y + roof_h;
    prims.push(prim(
        solid(cuboid_tapered(
            [2.9, 0.22, 0.38],
            0.0,
            roof_tile([0.14, 0.16, 0.19]),
        )),
        [0.0, ridge_y + 0.09, mid_z],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.24, 0.46, 0.44],
                0.2,
                roof_tile([0.14, 0.16, 0.19]),
            )),
            [sx * 1.5, ridge_y + 0.19, mid_z],
            id_quat(),
        ));
    }

    // The walk-in zone in the plane of the main posts, its edges bedded 4 cm
    // in the posts, the lintel and the sill (#1006).
    let veil_h = LINTEL_UNDER - SILL_TOP + 0.08;
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.0 * HALF_OPENING + 0.08, veil_h, 0.36]),
        },
        [0.0, SILL_TOP - 0.04 + veil_h * 0.5, MAIN_Z],
        id_quat(),
    ));

    assemble(prims)
}

/// One gate leaf on side `sx`, standing open: folded back from its main
/// post along the side of the way, its studded face toward the opening,
/// iron straps bridging it to the post.
fn leaf(sx: f32) -> Vec<Generator> {
    let thick = 0.1;
    let x = sx * (HALF_OPENING - thick * 0.5 - 0.02);
    let z0 = MAIN_Z + 0.23;
    let len = 1.25;
    let zc = z0 + len * 0.5;
    let (y0, y1) = (SILL_TOP + 0.08, SILL_TOP + 3.0);
    let face = x - sx * thick * 0.5;
    let mut parts = vec![prim(
        solid(cuboid_tapered(
            [thick, y1 - y0, len],
            0.0,
            timber(TIMBER_DARK),
        )),
        [x, (y0 + y1) * 0.5, zc],
        id_quat(),
    )];
    // Hinge straps reaching back to the post.
    for y in [y0 + 0.4, y1 - 0.4] {
        parts.push(prim(
            cuboid_tapered([0.12, 0.08, 0.32], 0.0, iron(IRON_DARK)),
            [x, y, z0 - 0.1],
            id_quat(),
        ));
    }
    // Two iron bands across the face and a grid of studs between them.
    for y in [y0 + 0.75, y1 - 0.75] {
        parts.push(prim(
            cuboid_tapered([0.03, 0.1, len - 0.1], 0.0, iron(IRON_DARK)),
            [face - sx * 0.005, y, zc],
            id_quat(),
        ));
    }
    for row in 0..5 {
        let y = y0 + 0.4 + row as f32 * ((y1 - y0 - 0.8) / 4.0);
        for col in 0..3 {
            let z = zc + (col as f32 - 1.0) * 0.36;
            parts.push(prim(
                sphere(0.035, 2, iron(IRON_DARK)),
                [face, y, z],
                id_quat(),
            ));
        }
    }
    parts
}

/// A paper lantern hung from the front eave at `at` (its x and z; the cord
/// runs up to the eave's underside).
fn lantern(at: [f32; 3]) -> Vec<Generator> {
    let [x, _, z] = at;
    let eave_under = BEAM_UNDER + 0.3;
    let body_y = 3.4;
    let body_h = 0.46;
    let top = body_y + body_h * 0.5;
    vec![
        prim(
            cylinder_tapered(0.012, eave_under - top, 6, 0.0, timber(TIMBER_DARK)),
            [x, (eave_under + top) * 0.5 + 0.02, z],
            id_quat(),
        ),
        prim(
            cylinder_tapered(0.12, 0.06, 12, 0.0, timber(TIMBER_DARK)),
            [x, top + 0.01, z],
            id_quat(),
        ),
        // A broad lit paper face at low strength, so it reads as warm paper
        // rather than a white ball.
        prim(
            cylinder_tapered(0.17, body_h, 14, 0.12, glow(LANTERN_GLOW, 1.3)),
            [x, body_y, z],
            id_quat(),
        ),
        prim(
            cylinder_tapered(0.13, 0.06, 12, 0.0, timber(PAPER_CREAM)),
            [x, body_y - body_h * 0.5 - 0.01, z],
            id_quat(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_a_family_gate() {
        super::super::assert_family_gate(&YakuimonGateway);
    }
}
