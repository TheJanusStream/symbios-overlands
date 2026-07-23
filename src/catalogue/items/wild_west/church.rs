//! Church — a Wild-West secondary. A white clapboard chapel with a steepled
//! bell tower, a cross and lit lancet windows. The frontier town's chapel.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the slab. The nave is a
//! hollow shell — rear, front and two *punched* side walls around a warm-lit
//! interior (a chancel reredos, an altar cross and a hanging nave light) — so
//! the lancets are cut panes you see *into* a glowing chapel through, not amber
//! panels stuck on a solid wall (#947). Render FRONT = −Z: the tower entrance
//! and rose oculus face −Z.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, glow, id_quat, plane,
    prim, quat_x, quat_z, solid, torus, window_card, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{CLAP_WHITE, GLASS_WARM, IRON_DARK, TIN_GREY, WOOD_RAW, clapboard, iron, tin};

/// Warm chapel light — the amber glow that fills the nave and spills through
/// the cut lancet panes as candlelit worship.
const WARM_NAVE: [f32; 3] = [1.0, 0.74, 0.42];

pub struct Church;

impl CatalogueEntry for Church {
    fn slug(&self) -> &'static str {
        "church"
    }
    fn name(&self) -> &'static str {
        "Church"
    }
    fn description(&self) -> &'static str {
        "White clapboard chapel with a steepled bell tower, a cross and lit windows."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::WildWest]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FRONTIER_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 7.0,
            min_spawn_dist: 40.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Punch one nave side wall (thin in X, spanning Y×Z) at `x`: a header above
/// the lancet openings, piers between them and a sill under each. Mirrors
/// saloon's `punch_wall`, rotated into the YZ plane. `levels` is
/// `[floor, head, top]`; `openings` are `(centre-z, half-width-z, sill-y)`.
fn punch_side_wall(
    prims: &mut Vec<Generator>,
    x: f32,
    depth: f32,
    levels: [f32; 3],
    mat: &SovereignMaterialSettings,
    openings: &[(f32, f32, f32)],
) {
    let [floor, head, top] = levels;
    // Header above the openings, spanning the full depth.
    if top - head > 0.01 {
        prims.push(prim(
            solid(cuboid_tapered([0.2, top - head, depth], 0.0, mat.clone())),
            [x, (head + top) * 0.5, 0.0],
            id_quat(),
        ));
    }
    // Piers: the even chunks of the boundary list along Z (wall-end, opening
    // edges, wall-end). Openings are given front-to-back.
    let mut edges = vec![-depth * 0.5];
    for &(cz, half, _) in openings {
        edges.push(cz - half);
        edges.push(cz + half);
    }
    edges.push(depth * 0.5);
    for pair in edges.chunks(2) {
        let [a, b] = [pair[0], pair[1]];
        if b - a > 0.01 {
            prims.push(prim(
                solid(cuboid_tapered([0.2, head - floor, b - a], 0.0, mat.clone())),
                [x, (floor + head) * 0.5, (a + b) * 0.5],
                id_quat(),
            ));
        }
    }
    // Sills under any opening that starts above the floor.
    for &(cz, half, sill) in openings {
        if sill - floor > 0.01 {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.2, sill - floor, half * 2.0],
                    0.0,
                    mat.clone(),
                )),
                [x, (floor + sill) * 0.5, cz],
                id_quat(),
            ));
        }
    }
}

fn build_tree() -> Generator {
    let slab_h = 0.3_f32;
    let body_w = 5.5_f32;
    let body_h = 4.5_f32;
    let body_d = 8.0_f32;
    let body_top = slab_h + body_h; // 4.8
    // Render FRONT = −Z — the tower entrance and oculus face −Z.
    let front_z = -body_d * 0.5; // -4.0
    let back_in = body_d * 0.5 - 0.1; // interior face of the rear wall
    let side_x = body_w * 0.5; // outer face of each side wall

    let mut prims = vec![
        // Clapboard slab — the root. Extended forward (and its centre shifted
        // with it) so it runs under the tower and its entrance landing — the
        // bell tower projects well past the nave front. `assemble` rebases the
        // children off the root's translation, so the shift leaves every other
        // piece where it was authored.
        prim(
            solid(cuboid_tapered(
                [7.0, slab_h, 10.8],
                0.0,
                clapboard(WOOD_RAW),
            )),
            [0.0, slab_h * 0.5, -0.9],
            id_quat(),
        ),
    ];

    // --- Hollow nave shell: rear + front gable walls, an interior floor and a
    //     flat ceiling under the roof. The side walls are punched separately.
    for z in [back_in, front_z + 0.1] {
        prims.push(prim(
            solid(cuboid_tapered(
                [body_w, body_h, 0.2],
                0.0,
                clapboard(CLAP_WHITE),
            )),
            [0.0, slab_h + body_h * 0.5, z],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w - 0.4, 0.1, body_d - 0.4],
            0.0,
            clapboard(WOOD_RAW),
        )),
        [0.0, slab_h + 0.05, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w, 0.2, body_d],
            0.0,
            clapboard(WOOD_RAW),
        )),
        [0.0, body_top - 0.1, 0.0],
        id_quat(),
    ));

    // Pitched tin gable roof — ridge running along X, gables facing ±Z.
    prims.push(prim(
        solid(cuboid_tapered_xz(
            [body_w + 0.5, 2.0, body_d + 0.4],
            [0.0, 0.92],
            tin(TIN_GREY),
        )),
        [0.0, body_top + 1.0, 0.0],
        id_quat(),
    ));

    // --- Lit interior. Two warm liner walls stand just inside the side walls:
    //     through the cut lancets you see a glowing chapel with real depth, and
    //     they close the sight-line so the aligned openings on the opposite
    //     wall never show daylight straight through (the "holes show sky" trap).
    //     A low altar and a warm cross hold the chancel against the rear wall.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.12, 3.0, body_d - 0.9], 0.0, glow(WARM_NAVE, 2.2)),
            [sx * (side_x - 0.9), slab_h + 1.6, -0.1],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered(
            [1.9, 1.0, 0.6],
            0.0,
            clapboard([0.5, 0.4, 0.28]),
        )),
        [0.0, slab_h + 0.5, back_in - 0.6],
        id_quat(),
    ));
    // Altar cross (a warm interior echo of the spire cross).
    prims.push(prim(
        solid(cuboid_tapered([0.14, 1.1, 0.14], 0.0, glow(WARM_NAVE, 2.4))),
        [0.0, slab_h + 1.8, back_in - 0.7],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.6, 0.14, 0.14], 0.0, glow(WARM_NAVE, 2.4))),
        [0.0, slab_h + 2.05, back_in - 0.7],
        id_quat(),
    ));

    // --- Lancet windows down both nave sides: punched openings in the white
    //     wall, each glazed with a cut amber pane on its own plane.
    let l_sill = slab_h + 1.0; // 1.3
    let l_head = slab_h + 3.1; // 3.4
    let openings = [
        (-2.0_f32, 0.45, l_sill),
        (0.0, 0.45, l_sill),
        (2.0, 0.45, l_sill),
    ];
    for sx in [-1.0_f32, 1.0] {
        punch_side_wall(
            &mut prims,
            sx * side_x,
            body_d,
            [slab_h, l_head, body_top],
            &clapboard(CLAP_WHITE),
            &openings,
        );
        // Glazing planes face outward (±X): a plane's quad lies in local XZ,
        // so `quat_z(±FRAC_PI_2)` stands it on the side wall — `size` reads as
        // `[height, width-along-Z]`, and after that turn `panes_x` counts the
        // vertical lights, `panes_y` the horizontal, hence 3×1 for a tall
        // lancet. Panes sit just proud of the wall so they never z-fight it.
        let quat = if sx < 0.0 {
            quat_z(FRAC_PI_2)
        } else {
            quat_z(-FRAC_PI_2)
        };
        for &(cz, half, sill) in &openings {
            prims.push(prim(
                plane(
                    [l_head - sill, half * 2.0],
                    window_card(GLASS_WARM, 3, 1, 0.3, 0.06),
                ),
                [sx * (side_x + 0.02), (sill + l_head) * 0.5, cz],
                quat,
            ));
        }
    }

    // Square bell tower projecting from the front, over the entrance.
    let tower_z = front_z - 0.5;
    let tower_face = tower_z - 1.2;
    let tower_h = 8.5_f32;
    prims.push(prim(
        solid(cuboid_tapered(
            [2.4, tower_h, 2.4],
            0.0,
            clapboard(CLAP_WHITE),
        )),
        [0.0, slab_h + tower_h * 0.5, tower_z],
        id_quat(),
    ));
    // Double doors under a rounded arch.
    prims.push(prim(
        solid(cuboid_tapered([1.6, 2.6, 0.2], 0.0, clapboard(WOOD_RAW))),
        [0.0, slab_h + 1.3, tower_face + 0.02],
        id_quat(),
    ));
    prims.push(prim(
        with_cut(
            torus(0.16, 0.78, clapboard(CLAP_WHITE)),
            [0.0, 0.5],
            [0.0, 1.0],
            0.0,
        ),
        [0.0, slab_h + 2.6, tower_face - 0.04],
        quat_x(-FRAC_PI_2),
    ));
    // Oculus (rose window) above the door: a lit disc in a white ring — a plain
    // emissive rose, so no `Window` alpha-card lands on a curved cylinder.
    prims.push(prim(
        torus(0.1, 0.62, clapboard(CLAP_WHITE)),
        [0.0, slab_h + 4.3, tower_face - 0.02],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        solid(cylinder_tapered(
            0.52,
            0.14,
            16,
            0.0,
            glow([1.0, 0.72, 0.46], 2.4),
        )),
        [0.0, slab_h + 4.3, tower_face - 0.02],
        quat_x(FRAC_PI_2),
    ));
    // Belfry: dark louvered openings and a hanging bell near the top.
    for ly in [slab_h + 6.0, slab_h + 6.35, slab_h + 6.7] {
        prims.push(prim(
            solid(cuboid_tapered(
                [1.4, 0.12, 0.06],
                0.0,
                clapboard([0.2, 0.18, 0.15]),
            )),
            [0.0, ly, tower_face + 0.02],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cone(0.32, 0.5, 12, iron(IRON_DARK))),
        [0.0, slab_h + 6.5, tower_z],
        id_quat(),
    ));

    // Tall sharp spire + cross over the belfry.
    prims.push(prim(
        solid(cone(1.35, 3.6, 12, tin(TIN_GREY))),
        [0.0, slab_h + tower_h + 1.8, tower_z],
        id_quat(),
    ));
    // Seated a little into the spire tip (base ~0.4 m below the apex) so it
    // reads as mounted, not balancing on the very point.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.16, 1.0, 0.16],
            0.0,
            glow([0.95, 0.82, 0.5], 1.4),
        )),
        [0.0, slab_h + tower_h + 3.7, tower_z],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.6, 0.16, 0.16],
            0.0,
            glow([0.95, 0.82, 0.5], 1.4),
        )),
        [0.0, slab_h + tower_h + 3.8, tower_z],
        id_quat(),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;
    use crate::pds::{GeneratorKind, SovereignTextureConfig};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Church.build(""), "church");
    }

    #[test]
    fn has_lit_windows() {
        assert!(crate::catalogue::items::util::has_emissive(
            &Church.build("")
        ));
    }

    /// #947: every `Window` card sits on a `Plane` at `uv_scale` 1.0 (spans
    /// once, not tiled), and the built tree survives a serde round-trip.
    #[test]
    fn glazing_is_planes_and_round_trips() {
        use crate::pds::material_finish::node_materials_mut;
        fn walk(g: &mut Generator) {
            let tag = g.kind.kind_tag();
            let is_plane = matches!(g.kind, GeneratorKind::Plane { .. });
            for m in node_materials_mut(&mut g.kind) {
                if matches!(m.texture, SovereignTextureConfig::Window(_)) {
                    assert!(is_plane, "Window card must sit on a Plane, found {tag}");
                    assert_eq!(m.uv_scale.0, 1.0, "Window cards must stay at uv_scale 1.0");
                }
            }
            for c in &mut g.children {
                walk(c);
            }
        }
        let mut g = Church.build("");
        walk(&mut g);
        let back: Generator = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert!(
            !crate::state::records_differ(&g, &back),
            "church must survive a serde round-trip"
        );
    }
}
