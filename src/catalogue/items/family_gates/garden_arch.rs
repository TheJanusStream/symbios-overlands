//! Garden Arch Gateway - a family gate for gardened worlds (#1796), stood
//! by suburban and solarpunk worlds beside their own. A white trellis arch
//! over a brick path: latticed sides under a round top of ribs and rafters,
//! dressed with climbing roses, a picket gate's two leaves standing open
//! before it and a lamp post at the path's side.
//!
//! `themes()` stays empty, as `civic_gateway`'s does: the gate reaches the
//! suburban and solarpunk pools through `kindred_gates`.

use std::f32::consts::PI;

use crate::catalogue::items::solarpunk::{LEAF_GREEN, MOSS_GREEN, foliage};
use crate::catalogue::items::suburban::{BRICK_TAN, PORCH_WARM, WOOD_WHITE, brick, enamel, wood};
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, footing, glow, id_quat, prim,
    prim_scaled, solid, sphere, strut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp, Fp3, Generator, GeneratorKind, SovereignMaterialSettings};

pub struct GardenArchGateway;

impl CatalogueEntry for GardenArchGateway {
    fn slug(&self) -> &'static str {
        "garden_arch_gateway"
    }
    fn name(&self) -> &'static str {
        "Garden Arch Gateway"
    }
    fn description(&self) -> &'static str {
        "A white trellis arch dressed with climbing roses, its picket gate standing open."
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

/// Half the opening between the trellis posts' inner faces (m).
const HALF_OPENING: f32 = 1.44;
/// The posts' centre line out from the path's middle.
const POST_X: f32 = 1.5;
/// Top of the brick path.
const PATH_TOP: f32 = 0.2;
/// Underside of the headers, the head of the opening.
const HEADER_UNDER: f32 = 3.2;
/// Where the front and back posts stand.
const POST_Z: f32 = 0.35;
/// A deep rose red, and the lamp post's dark green iron.
const ROSE_RED: [f32; 3] = [0.62, 0.06, 0.12];
const IRON_GREEN: [f32; 3] = [0.10, 0.15, 0.12];

fn build_tree() -> Generator {
    let white = || wood(WOOD_WHITE);
    let path_z = -0.4;
    let mut prims = vec![footing(5.0, 3.0, [0.0, path_z], 3.5)];
    // The brick path through the arch, running out in front to the gate.
    prims.push(prim(
        solid(cuboid_tapered([5.0, PATH_TOP, 3.0], 0.0, brick(BRICK_TAN))),
        [0.0, PATH_TOP * 0.5, path_z],
        id_quat(),
    ));

    let post_h = HEADER_UNDER + 0.05 - PATH_TOP + 0.02;
    let post_y = PATH_TOP - 0.02 + post_h * 0.5;
    for sx in [-1.0_f32, 1.0] {
        let x = sx * POST_X;
        // Front and back posts, and the centre stile the lattice is nailed
        // to, each sunk 2 cm into the path.
        for z in [-POST_Z, POST_Z] {
            prims.push(prim(
                solid(cuboid_tapered([0.12, post_h, 0.12], 0.0, white())),
                [x, post_y, z],
                id_quat(),
            ));
        }
        prims.push(prim(
            solid(cuboid_tapered([0.12, post_h, 0.28], 0.0, white())),
            [x, post_y, 0.0],
            id_quat(),
        ));
        // The lattice: rails across, slats up, laid on either side of their
        // shared face.
        for k in 0..7 {
            prims.push(prim(
                cuboid_tapered([0.03, 0.04, 2.0 * POST_Z], 0.0, paint()),
                [sx * (POST_X - 0.015), 0.62 + k as f32 * 0.38, 0.0],
                id_quat(),
            ));
        }
        for z in [-0.18_f32, 0.18] {
            prims.push(prim(
                cuboid_tapered([0.03, HEADER_UNDER - 0.6, 0.03], 0.0, paint()),
                [sx * (POST_X + 0.015), (HEADER_UNDER + 0.4) * 0.5, z],
                id_quat(),
            ));
        }
        // Roses climbing the side, outside the opening.
        prims.extend(rose_clump(
            [sx * (POST_X + 0.1), 1.1, 0.05],
            [0.16, 0.6, 0.42],
            sx,
        ));
        prims.extend(rose_clump(
            [sx * (POST_X + 0.08), 2.3, -0.08],
            [0.15, 0.55, 0.38],
            -sx,
        ));
        prims.extend(picket_leaf(sx));
    }

    // Front, centre and back headers over the opening.
    for (z, d) in [(-POST_Z, 0.1_f32), (0.0, 0.27), (POST_Z, 0.1)] {
        prims.push(prim(
            solid(cuboid_tapered([2.0 * POST_X + 0.3, 0.14, d], 0.0, white())),
            [0.0, HEADER_UNDER + 0.07, z],
            id_quat(),
        ));
    }
    // The round top: a rib of six chords over the front and the back,
    // rafters across between them, roses along it.
    let top = HEADER_UNDER + 0.14;
    let rib_r = POST_X;
    let arc = |a: f32| [rib_r * a.cos(), top + rib_r * a.sin() * 0.75];
    for z in [-POST_Z, POST_Z] {
        for i in 0..6 {
            let [x0, y0] = arc(PI * i as f32 / 6.0);
            let [x1, y1] = arc(PI * (i + 1) as f32 / 6.0);
            prims.push(strut([x0, y0, z], [x1, y1, z], 0.05, 8, white()));
        }
    }
    for i in 1..6 {
        let [x, y] = arc(PI * i as f32 / 6.0);
        prims.push(strut(
            [x, y, -POST_Z - 0.05],
            [x, y, POST_Z + 0.05],
            0.03,
            6,
            paint(),
        ));
    }
    for (k, a) in [0.5_f32, 1.25, 1.95, 2.65].into_iter().enumerate() {
        let [x, y] = arc(a);
        let flip = if k % 2 == 0 { 1.0 } else { -1.0 };
        prims.extend(rose_clump([x, y + 0.08, 0.0], [0.42, 0.24, 0.5], flip));
    }

    prims.extend(lamp_post([-2.1, PATH_TOP, -1.35]));

    // The walk-in zone in the plane of the centre stiles, its edges bedded
    // 4 cm in the stiles, the centre header and the path (#1006).
    let veil_h = HEADER_UNDER - PATH_TOP + 0.08;
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.0 * HALF_OPENING + 0.08, veil_h, 0.25]),
        },
        [0.0, PATH_TOP - 0.04 + veil_h * 0.5, 0.0],
        id_quat(),
    ));

    assemble(prims)
}

/// White paint with no grain for the thin stuff - lattice, rafters,
/// pickets - where a plank texture reads as nothing at their size and costs
/// the record a texture each.
fn paint() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(WOOD_WHITE),
        roughness: Fp(0.8),
        ..Default::default()
    }
}

/// A clump of climbing rose at `centre`, its leaves an ellipsoid of
/// `semi` axes, with blooms set half into its skin; `turn` mirrors the
/// pattern of blooms so neighbouring clumps differ.
fn rose_clump(centre: [f32; 3], semi: [f32; 3], turn: f32) -> Vec<Generator> {
    let [cx, cy, cz] = centre;
    let mut parts = vec![prim_scaled(
        sphere(1.0, 4, foliage(LEAF_GREEN)),
        centre,
        id_quat(),
        semi,
    )];
    // A darker under-leaf, offset so the clump reads as foliage, not a ball.
    parts.push(prim_scaled(
        sphere(1.0, 3, foliage(MOSS_GREEN)),
        [cx, cy - semi[1] * 0.3, cz + 0.04],
        id_quat(),
        [semi[0] * 0.85, semi[1] * 0.8, semi[2] * 0.85],
    ));
    let bloom = SovereignMaterialSettings {
        roughness: Fp(0.8),
        ..foliage(ROSE_RED)
    };
    for k in 0..6 {
        let a = turn * (k as f32 * 2.4 + 0.3);
        let b = (k as f32 * 0.9).sin() * 0.9;
        let dir = [a.cos() * b.cos(), b.sin(), a.sin() * b.cos()];
        parts.push(prim(
            sphere(0.075, 3, bloom.clone()),
            [
                cx + semi[0] * dir[0] * 0.92,
                cy + semi[1] * dir[1] * 0.92,
                cz + semi[2] * dir[2] * 0.92,
            ],
            id_quat(),
        ));
    }
    parts
}

/// One leaf of the picket gate on side `sx`, standing open: swung out from
/// its front post to lie along the path's edge, pickets on two rails, a
/// strap hinge bridging it to the post.
fn picket_leaf(sx: f32) -> Vec<Generator> {
    let x = sx * POST_X;
    let z0 = -POST_Z - 0.08;
    let len = 1.4;
    let zc = z0 - len * 0.5;
    let mut parts = Vec::new();
    // Two rails, set in behind the pickets.
    for y in [PATH_TOP + 0.3, PATH_TOP + 0.85] {
        parts.push(prim(
            cuboid_tapered([0.035, 0.08, len], 0.0, paint()),
            [x - sx * 0.0375, y, zc],
            id_quat(),
        ));
    }
    // Pickets with pointed tops, the first a stile at the hinge.
    for k in 0..7 {
        let z = z0 - 0.06 - k as f32 * 0.213;
        let h = 1.0;
        parts.push(prim(
            cuboid_tapered([0.04, h, 0.09], 0.0, paint()),
            [x, PATH_TOP + 0.08 + h * 0.5, z],
            id_quat(),
        ));
        parts.push(prim(
            cuboid_tapered_xz([0.04, 0.1, 0.09], [0.0, 0.95], paint()),
            [x, PATH_TOP + 0.08 + h + 0.05, z],
            id_quat(),
        ));
    }
    // Strap hinges reaching back to the post.
    for y in [PATH_TOP + 0.3, PATH_TOP + 0.85] {
        parts.push(prim(
            cuboid_tapered([0.05, 0.05, 0.16], 0.0, enamel([0.1, 0.1, 0.1])),
            [x, y, z0 + 0.02],
            id_quat(),
        ));
    }
    parts
}

/// A garden lamp post standing on the path at `foot`: a cast base, a
/// slender column and a lantern head lit warm.
fn lamp_post(foot: [f32; 3]) -> Vec<Generator> {
    let [x, y, z] = foot;
    let iron = || enamel(IRON_GREEN);
    let col_h = 2.4;
    let head_y = y + col_h + 0.2;
    vec![
        prim(
            solid(cylinder_tapered(0.14, 0.3, 12, 0.35, iron())),
            [x, y + 0.14, z],
            id_quat(),
        ),
        prim(
            solid(cylinder_tapered(0.05, col_h, 10, 0.2, iron())),
            [x, y + col_h * 0.5, z],
            id_quat(),
        ),
        prim(
            cylinder_tapered(0.11, 0.06, 10, 0.0, iron()),
            [x, y + col_h + 0.02, z],
            id_quat(),
        ),
        // The lantern's glass, small and warm.
        prim(
            cylinder_tapered(0.1, 0.3, 8, 0.0, glow(PORCH_WARM, 2.6)),
            [x, head_y, z],
            id_quat(),
        ),
        prim(
            cylinder_tapered(0.17, 0.14, 8, 0.9, iron()),
            [x, head_y + 0.21, z],
            id_quat(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_a_family_gate() {
        super::super::assert_family_gate(&GardenArchGateway);
    }
}
