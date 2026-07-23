//! Saloon — the Wild-West landmark and the kit's lit hero. A two-storey red
//! clapboard saloon with a tall false-front parapet, a covered porch and
//! upstairs gallery, lit amber windows and a hanging sign. ~10 m wide, so it
//! anchors the boomtown and reads as the saloon from across the home region.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`]. The body is a hollow shell:
//! the front wall is a punched screen of piers, sills and headers, and behind
//! the cut window panes is the barroom itself — back-bar, bottle shelf and a
//! warm hanging lamp downstairs, curtained rooms up — so the windows look
//! *into* a lively saloon instead of being glowing panels stuck on a solid
//! wall (#945). The false front is only the parapet *above* the roofline so
//! it never buries the storefront (render FRONT = −Z).

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, foundation_block, glow, id_quat, plane, prim, quat_x, solid, sphere,
    window_card,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CLAP_RED, CLAP_TAN, CLAP_WHITE, GLASS_WARM, IRON_DARK, TIN_GREY, WOOD_RAW, clapboard, fx, iron,
    tin,
};

/// Warm barroom lamplight — the glow that spills through the cut window panes
/// and the batwing doors as an occupied saloon after dark.
const BAR_WARM: [f32; 3] = [1.0, 0.64, 0.28];
/// Back-bar bottle tints — the row of glass on the shelf behind the counter,
/// the one spot of jewel colour in the warm room.
const BOTTLES: [[f32; 3]; 5] = [
    [0.22, 0.46, 0.26],
    [0.62, 0.44, 0.16],
    [0.40, 0.18, 0.10],
    [0.16, 0.34, 0.48],
    [0.52, 0.14, 0.14],
];

pub struct Saloon;

impl CatalogueEntry for Saloon {
    fn slug(&self) -> &'static str {
        "saloon"
    }
    fn name(&self) -> &'static str {
        "Saloon"
    }
    fn description(&self) -> &'static str {
        "Two-storey clapboard saloon with a false front, porch gallery and lit windows."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::WildWest]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FRONTIER_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 11.0,
            min_spawn_dist: 52.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Build one storey of the punched front wall: a header above the openings,
/// piers between them, and a sill under any opening that starts above the
/// floor. `levels` is `[floor, head, top]`; `openings` are `(centre-x,
/// half-width, sill-y)` — a sill equal to `floor` (a doorway) gets no sill
/// panel. The wall sits in the XY plane at `z`, 0.2 m thick.
fn punch_wall(
    prims: &mut Vec<Generator>,
    width: f32,
    levels: [f32; 3],
    z: f32,
    mat: &SovereignMaterialSettings,
    openings: &[(f32, f32, f32)],
) {
    let [floor, head, top] = levels;
    if top - head > 0.01 {
        prims.push(prim(
            solid(cuboid_tapered([width, top - head, 0.2], 0.0, mat.clone())),
            [0.0, (head + top) * 0.5, z],
            id_quat(),
        ));
    }
    // Piers: the even chunks of the boundary list (wall-end, opening edges,
    // wall-end). Openings are given left-to-right.
    let mut edges = vec![-width * 0.5];
    for &(cx, half, _) in openings {
        edges.push(cx - half);
        edges.push(cx + half);
    }
    edges.push(width * 0.5);
    for pair in edges.chunks(2) {
        let [a, b] = [pair[0], pair[1]];
        if b - a > 0.01 {
            prims.push(prim(
                solid(cuboid_tapered([b - a, head - floor, 0.2], 0.0, mat.clone())),
                [(a + b) * 0.5, (floor + head) * 0.5, z],
                id_quat(),
            ));
        }
    }
    for &(cx, half, sill) in openings {
        if sill - floor > 0.01 {
            prims.push(prim(
                solid(cuboid_tapered(
                    [half * 2.0, sill - floor, 0.2],
                    0.0,
                    mat.clone(),
                )),
                [cx, (floor + sill) * 0.5, z],
                id_quat(),
            ));
        }
    }
}

fn build_tree() -> Generator {
    let slab_h = 0.3_f32;
    let body_w = 8.0_f32;
    let body_h = 6.0_f32;
    let body_d = 7.0_f32;
    let body_top = slab_h + body_h; // 6.3
    // Render FRONT = −Z — the front wall is the punched hero face; the barroom
    // fills the shell behind it.
    let front_z = -body_d * 0.5; // -3.5
    let back_z = body_d * 0.5 - 0.1; // interior face of the rear wall
    let glaze_z = front_z - 0.02; // panes just proud of the wall
    let mid_y = slab_h + 3.4; // storey line / gallery floor

    let mut prims = vec![
        // Clapboard floor slab — the root.
        prim(
            solid(cuboid_tapered(
                [10.0, slab_h, 8.0],
                0.0,
                clapboard(WOOD_RAW),
            )),
            [0.0, slab_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    prims.push(foundation_block(10.0, 8.0, [0.0, 0.0], 1.2));

    // --- Hollow body shell: rear + side walls, a storey floor and a ceiling.
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w, body_h, 0.2],
            0.0,
            clapboard(CLAP_RED),
        )),
        [0.0, slab_h + body_h * 0.5, back_z],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.2, body_h, body_d],
                0.0,
                clapboard(CLAP_RED),
            )),
            [sx * (body_w * 0.5 - 0.1), slab_h + body_h * 0.5, 0.0],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w, 0.2, body_d],
            0.0,
            clapboard(WOOD_RAW),
        )),
        [0.0, mid_y, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w, 0.2, body_d],
            0.0,
            clapboard(CLAP_RED),
        )),
        [0.0, body_top - 0.1, 0.0],
        id_quat(),
    ));

    // White corner pilasters framing the front wall.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.32, body_h, 0.32],
                0.0,
                clapboard(CLAP_WHITE),
            )),
            [
                sx * (body_w * 0.5 - 0.04),
                slab_h + body_h * 0.5,
                front_z + 0.08,
            ],
            id_quat(),
        ));
    }
    // Low tin roof.
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w + 0.4, 0.4, body_d + 0.4],
            0.0,
            tin(TIN_GREY),
        )),
        [0.0, body_top + 0.2, 0.0],
        id_quat(),
    ));

    // False front: a tall parapet rising ABOVE the roofline, with an
    // overhanging cornice + sign band.
    let para_z = front_z - 0.15;
    let para_face = para_z - 0.2;
    let para_h = 3.3_f32;
    let para_cy = body_top + para_h * 0.5;
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w + 0.8, para_h, 0.4],
            0.0,
            clapboard(CLAP_RED),
        )),
        [0.0, para_cy, para_z],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w + 1.3, 0.34, 0.8],
            0.0,
            clapboard(CLAP_WHITE),
        )),
        [0.0, body_top + para_h + 0.15, para_z],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([5.4, 1.2, 0.16], 0.0, clapboard(CLAP_WHITE))),
        [0.0, body_top + 1.4, para_face - 0.08],
        id_quat(),
    ));

    // --- Barroom interior, seen through the ground-floor openings: a back-bar
    //     against the rear wall, a mirror, a row of bottles, a counter, and a
    //     warm hanging lamp.
    let bar_z = back_z - 0.25;
    prims.push(prim(
        solid(cuboid_tapered(
            [6.0, 2.4, 0.4],
            0.0,
            clapboard([0.30, 0.20, 0.12]),
        )),
        [0.0, slab_h + 1.4, bar_z],
        id_quat(),
    ));
    // Mirror behind the bottles — a warm-lit back panel, so the barroom reads
    // as a glowing amber room through the windows, not a dim box.
    prims.push(prim(
        cuboid_tapered([4.4, 1.6, 0.05], 0.0, glow([1.0, 0.72, 0.4], 1.6)),
        [0.0, slab_h + 1.9, bar_z - 0.22],
        id_quat(),
    ));
    for (i, tint) in BOTTLES.iter().chain(BOTTLES.iter()).enumerate() {
        let x = -2.7 + i as f32 * 0.6;
        prims.push(prim(
            cuboid_tapered([0.14, 0.5, 0.12], 0.0, glow(*tint, 1.8)),
            [x, slab_h + 1.35, bar_z - 0.28],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([5.4, 1.1, 0.5], 0.0, clapboard(CLAP_TAN))),
        [0.0, slab_h + 0.55, bar_z - 1.2],
        id_quat(),
    ));
    // Warm hanging lamp filling the barroom with light.
    prims.push(prim(
        cuboid_tapered([5.8, 0.35, 3.4], 0.0, glow(BAR_WARM, 3.6)),
        [0.0, mid_y - 0.3, 0.1],
        id_quat(),
    ));

    // --- Ground-floor punched front wall: a batwing doorway flanked by two
    //     tall lit windows.
    let g_win = 2.5_f32;
    let g_sill = slab_h + 0.7; // 1.0 — the window sits a low sill above the floor
    let g_head = 3.0_f32;
    punch_wall(
        &mut prims,
        body_w,
        [slab_h, g_head, mid_y],
        front_z,
        &clapboard(CLAP_RED),
        &[
            (-g_win, 0.9, g_sill),
            (0.0, 0.9, slab_h), // doorway to the floor
            (g_win, 0.9, g_sill),
        ],
    );
    // Window glazing — clear amber panes on planes filling their openings
    // (sill to head), cut open over the barroom.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            plane(
                [1.8, g_head - g_sill],
                window_card(GLASS_WARM, 4, 3, 0.3, 0.05),
            ),
            [sx * g_win, (g_sill + g_head) * 0.5, glaze_z],
            quat_x(-FRAC_PI_2),
        ));
        // Warm porch lantern on an iron bracket.
        prims.push(prim(
            solid(cuboid_tapered([0.08, 0.08, 0.45], 0.0, iron(IRON_DARK))),
            [sx * 1.4, slab_h + 2.45, front_z - 0.3],
            id_quat(),
        ));
        prims.push(prim(
            solid(sphere(0.17, 3, glow([1.0, 0.5, 0.14], 3.4))),
            [sx * 1.4, slab_h + 2.35, front_z - 0.55],
            id_quat(),
        ));
    }
    // Batwing doors: two half-height louvered leaves in the doorway, with a
    // gap below (boots) and above (hats) so the warm room shows through.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.78, 1.2, 0.06],
                0.0,
                clapboard([0.42, 0.3, 0.18]),
            )),
            [sx * 0.42, slab_h + 1.15, front_z - 0.04],
            id_quat(),
        ));
    }

    // --- Upstairs gallery: floor on posts, a balustrade, lit windows + a door.
    let gallery_y = slab_h + 3.4;
    let gallery_front = front_z - 1.3;
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w + 0.6, 0.22, 1.3],
            0.0,
            clapboard(WOOD_RAW),
        )),
        [0.0, gallery_y, front_z - 0.65],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.2, gallery_y, 0.2],
                0.0,
                clapboard(WOOD_RAW),
            )),
            [sx * 3.7, gallery_y * 0.5, gallery_front + 0.1],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w + 0.6, 0.16, 1.0],
            0.0,
            clapboard(WOOD_RAW),
        )),
        [0.0, slab_h + 0.08, gallery_front + 0.35],
        id_quat(),
    ));
    // Balustrade: top rail + turned balusters.
    prims.push(prim(
        cuboid_tapered([body_w + 0.6, 0.12, 0.12], 0.0, clapboard(CLAP_WHITE)),
        [0.0, gallery_y + 0.65, gallery_front],
        id_quat(),
    ));
    let balusters = 11;
    for i in 0..balusters {
        let t = i as f32 / (balusters - 1) as f32;
        prims.push(prim(
            cuboid_tapered([0.06, 0.55, 0.06], 0.0, clapboard(CLAP_WHITE)),
            [-3.8 + t * 7.6, gallery_y + 0.35, gallery_front],
            id_quat(),
        ));
    }

    // --- Upper storey: curtained rooms behind a balcony door and two windows.
    let u_win = 2.6_f32;
    let u_sill = mid_y + 0.6;
    let u_head = mid_y + 2.1;
    // Warm room light + deep-red curtains framing each window.
    prims.push(prim(
        cuboid_tapered([5.5, 0.3, 3.0], 0.0, glow(BAR_WARM, 3.0)),
        [0.0, body_top - 0.4, 0.2],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        for cx in [sx * u_win - 0.5, sx * u_win + 0.5] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.35, 1.5, 0.05],
                    0.0,
                    clapboard([0.46, 0.12, 0.12]),
                )),
                [cx, (u_sill + u_head) * 0.5, front_z + 0.35],
                id_quat(),
            ));
        }
    }
    punch_wall(
        &mut prims,
        body_w,
        [mid_y, u_head, body_top],
        front_z,
        &clapboard(CLAP_RED),
        &[
            (-u_win, 0.75, u_sill),
            (0.0, 0.6, mid_y), // balcony doorway to the gallery floor
            (u_win, 0.75, u_sill),
        ],
    );
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            plane(
                [1.5, u_head - u_sill],
                window_card(GLASS_WARM, 3, 3, 0.35, 0.06),
            ),
            [sx * u_win, (u_sill + u_head) * 0.5, glaze_z],
            quat_x(-FRAC_PI_2),
        ));
    }
    // Balcony door: a panelled leaf with a glazed upper light.
    prims.push(prim(
        solid(cuboid_tapered(
            [1.05, mid_y + 1.9 - mid_y, 0.1],
            0.0,
            clapboard(CLAP_TAN),
        )),
        [0.0, mid_y + 0.95, front_z - 0.03],
        id_quat(),
    ));
    prims.push(prim(
        plane([0.7, 0.7], window_card(GLASS_WARM, 2, 2, 0.35, 0.08)),
        [0.0, mid_y + 1.45, glaze_z - 0.04],
        quat_x(-FRAC_PI_2),
    ));

    // Hanging perpendicular sign on an iron bracket at the corner.
    prims.push(prim(
        solid(cuboid_tapered([0.1, 0.1, 1.0], 0.0, iron(IRON_DARK))),
        [-3.6, slab_h + 5.0, front_z - 0.5],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.12, 0.9, 1.4], 0.0, clapboard(CLAP_WHITE))),
        [-3.6, slab_h + 4.3, front_z - 1.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: a dry prairie wind, dust skating the street.
    root.audio = fx::prairie_wind();
    root.children
        .push(fx::dust_drift([0.0, 0.3, front_z - 3.5], 0x0DE5_5A12));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;
    use crate::pds::{GeneratorKind, SovereignTextureConfig};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Saloon.build(""), "saloon");
    }

    #[test]
    fn has_lit_windows() {
        assert!(crate::catalogue::items::util::has_emissive(
            &Saloon.build("")
        ));
    }

    /// #945: every `Window` card sits on a `Plane` at `uv_scale` 1.0 (spans
    /// once, not tiled), and the built tree survives a serde round-trip (the
    /// `window_card` fixed-point fix from #943).
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
        let mut g = Saloon.build("");
        walk(&mut g);
        let back: Generator = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert!(
            !crate::state::records_differ(&g, &back),
            "saloon must survive a serde round-trip"
        );
    }
}
