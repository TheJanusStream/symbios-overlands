//! Town Gate — the Medieval social gateway (#760). A fortified burgh
//! gatehouse replacing the neutral placeholder arch for this theme: two
//! battlemented dressed-ashlar towers flank a round-arched carriage passage,
//! their crenellated wall-walk bridging the span above a raised iron
//! portcullis. Iron cresset torches flank the threshold and warm shutter-glow
//! windows light the tower fronts, so the gate reads as the manned entrance to
//! a walled town at dusk. The lord's colours hang over the arch on the −Z
//! (camera) face.
//!
//! The only functional element is the single [`GeneratorKind::Gateway`] zone
//! standing in the passage — walking into it opens the destination picker of
//! the room owner's mutual follows. Everything else is masonry framing that
//! reads the box as a gate you pass under.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid, torus,
    with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    FORGE_ORANGE, HERALD_BLUE, HERALD_GOLD, IRON_DARK, STONE_GREY, STONE_PALE, WOOD_DARK, cloth,
    crenellations, iron, rough_stone, stone, timber,
};

pub struct MedievalGateway;

impl CatalogueEntry for MedievalGateway {
    fn slug(&self) -> &'static str {
        "medieval_gateway"
    }
    fn name(&self) -> &'static str {
        "Town Gate"
    }
    fn description(&self) -> &'static str {
        "Fortified burgh gatehouse: two crenellated ashlar towers over a round-arched, portcullis-hung passage lit by iron cressets."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Medieval]
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
    // ── Key dimensions ──
    let slab_top = 0.3_f32;
    let tcx = 2.2_f32; // tower centre offset along X (± flanks the passage)
    let thw = 0.9_f32; // tower half-width (X); inner face at 1.3 → 2.6 m gap
    let thz = 0.95_f32; // tower half-depth (Z)
    let tower_h = 4.9_f32;
    let tower_top = slab_top + tower_h; // 5.2
    let arch_r = 1.3_f32; // round-arch radius = half the passage width
    let spring_y = 2.9_f32; // arch springline; apex at spring_y + arch_r = 4.2
    let apex_y = spring_y + arch_r;
    let front = -1.0_f32; // hero convention: gate front faces −Z

    // ── Cobbled threshold slab: the flat-base root (never tilt a root — every
    //    child inherits its transform). ──
    let mut prims = vec![prim(
        solid(cuboid_tapered(
            [6.4, slab_top, 2.8],
            0.0,
            rough_stone(STONE_GREY),
        )),
        [0.0, slab_top * 0.5, 0.0],
        id_quat(),
    )];

    // ── Two flanking gatehouse towers ──
    for sx in [-1.0_f32, 1.0] {
        let cx = sx * tcx;
        // Dressed-ashlar shaft, lightly battered.
        prims.push(prim(
            solid(cuboid_tapered(
                [thw * 2.0, tower_h, thz * 2.0],
                0.04,
                stone(STONE_PALE),
            )),
            [cx, slab_top + tower_h * 0.5, 0.0],
            id_quat(),
        ));
        // Oversailing corbel string-course under the parapet.
        prims.push(prim(
            solid(cuboid_tapered(
                [thw * 2.0 + 0.28, 0.3, thz * 2.0 + 0.28],
                0.0,
                stone(STONE_GREY),
            )),
            [cx, tower_top - 0.15, 0.0],
            id_quat(),
        ));
        // Battlemented parapet ring — the defining medieval silhouette.
        prims.extend(crenellations(
            [cx, tower_top, 0.0],
            thw + 0.14,
            thz + 0.14,
            0.55,
            0.38,
            0.32,
            stone(STONE_GREY),
        ));
        // Cross-shaped arrow loop on the tower's −Z (camera) face.
        let loop_z = front * (thz + 0.02);
        prims.push(prim(
            cuboid_tapered([0.1, 1.0, 0.08], 0.0, iron(IRON_DARK)),
            [cx, 2.6, loop_z],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([0.42, 0.12, 0.08], 0.0, iron(IRON_DARK)),
            [cx, 2.75, loop_z],
            id_quat(),
        ));
        // Warm shutter-glow window high on the tower front — the gate reads
        // inhabited at dusk. Broad lit face → low strength so it holds colour.
        prims.push(prim(
            cuboid_tapered([0.42, 0.55, 0.08], 0.0, glow(FORGE_ORANGE, 2.6)),
            [cx, 4.0, loop_z],
            id_quat(),
        ));
    }

    // ── Springer / impost blocks the arch rises from, on each tower's inner
    //    face at the springline. ──
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.56, 0.24, 1.95], 0.0, stone(STONE_GREY))),
            [sx * 1.25, spring_y - 0.12, 0.0],
            id_quat(),
        ));
    }

    // ── Round-arch barrel across the passage: four ashlar ribs stepping
    //    through the depth so the opening reads as a masonry vault, plus a
    //    proud archivolt hood ring on the front face. ──
    for z in [-0.85_f32, -0.28, 0.28, 0.85] {
        prims.push(prim(
            with_cut(
                torus(0.18, arch_r, stone(STONE_GREY)),
                [0.0, 0.5],
                [0.0, 1.0],
                0.0,
            ),
            [0.0, spring_y, z],
            quat_x(-FRAC_PI_2),
        ));
    }
    // Archivolt / dripstone hood standing proud of the front face.
    prims.push(prim(
        with_cut(
            torus(0.14, arch_r + 0.2, stone(STONE_PALE)),
            [0.0, 0.5],
            [0.0, 1.0],
            0.0,
        ),
        [0.0, spring_y, front * 0.98],
        quat_x(-FRAC_PI_2),
    ));

    // ── Curtain wall bridging the towers above the arch — the horizontal span
    //    tying the gatehouse together — with its own crenellated wall-walk. ──
    let wall_h = 0.85_f32;
    let wall_cy = apex_y + wall_h * 0.5; // bottom flush with the arch apex
    prims.push(prim(
        solid(cuboid_tapered([5.0, wall_h, 1.0], 0.0, stone(STONE_GREY))),
        [0.0, wall_cy, 0.0],
        id_quat(),
    ));
    // Machicolation string-course oversailing the front of the wall-walk.
    prims.push(prim(
        solid(cuboid_tapered([3.0, 0.2, 0.28], 0.0, stone(STONE_PALE))),
        [0.0, apex_y + 0.12, front * 0.52],
        id_quat(),
    ));
    // Wall-walk battlements over the gate (hz small → merlons only front/back).
    let wall_top = wall_cy + wall_h * 0.5;
    prims.extend(crenellations(
        [0.0, wall_top, 0.0],
        1.45,
        0.5,
        0.5,
        0.36,
        0.3,
        stone(STONE_GREY),
    ));

    // ── Raised iron portcullis, hoisted clear of head height under the arch:
    //    a grille of vertical and horizontal bars set just behind the front
    //    arch rib. ──
    let port_z = front * 0.5;
    for bx in [-1.0_f32, -0.5, 0.0, 0.5, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.06, 0.95, 0.06], 0.0, iron(IRON_DARK))),
            [bx, 3.72, port_z],
            id_quat(),
        ));
    }
    for by in [3.34_f32, 4.05] {
        prims.push(prim(
            solid(cuboid_tapered([2.1, 0.06, 0.06], 0.0, iron(IRON_DARK))),
            [0.0, by, port_z],
            id_quat(),
        ));
    }

    // ── Heraldic banner over the arch on the solid curtain-wall front (−Z),
    //    with an applied gold cross device — the town's colours. Each layer
    //    steps further toward the camera so the cross reads proud on the cloth.
    prims.push(prim(
        solid(cuboid_tapered([1.0, 0.06, 0.06], 0.0, timber(WOOD_DARK))),
        [0.0, wall_top - 0.08, front * 0.55],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.9, 0.9, 0.05], 0.0, cloth(HERALD_BLUE, HERALD_GOLD)),
        [0.0, 4.5, front * 0.56],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.16, 0.6, 0.05], 0.0, cloth(HERALD_GOLD, HERALD_GOLD)),
        [0.0, 4.5, front * 0.6],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.5, 0.16, 0.05], 0.0, cloth(HERALD_GOLD, HERALD_GOLD)),
        [0.0, 4.66, front * 0.6],
        id_quat(),
    ));

    // ── Iron cresset torches flanking the threshold — the emissive accent. An
    //    iron sconce cup and a hot little flame on each tower's inner-front
    //    corner, framing the mouth of the passage. Thin flame → runs hot. ──
    for sx in [-1.0_f32, 1.0] {
        let cx = sx * 1.22;
        prims.push(prim(
            solid(cylinder_tapered(0.1, 0.2, 8, 0.3, iron(IRON_DARK))),
            [cx, 2.5, front * 0.7],
            id_quat(),
        ));
        prims.push(prim(
            cone(0.11, 0.4, 8, glow(FORGE_ORANGE, 5.5)),
            [cx, 2.78, front * 0.7],
            id_quat(),
        ));
    }

    // ── The single functional zone: the walk-in gateway box, centred in the
    //    passage, floor-to-lintel. ──
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.6, 3.2, 1.4]),
        },
        [0.0, 1.9, 0.0],
        id_quat(),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&MedievalGateway.build(""), "medieval_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is masonry, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = MedievalGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
