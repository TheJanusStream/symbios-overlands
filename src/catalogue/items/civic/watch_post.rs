//! Watch post — a stilted timber platform with a railing and a pyramidal
//! roof. An escalation-Conflict scatter prop: a hasty lookout reads the
//! same whether it overlooks a medieval road or a cyberpunk checkpoint.

use crate::catalogue::items::util::{
    cone, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{EscalationBand, EscalationTier, ThemeArchetype};

use super::{LANTERN_WARM, WOOD, WOOD_GREY, quat_z, wood};

pub struct WatchPost;

impl CatalogueEntry for WatchPost {
    fn slug(&self) -> &'static str {
        "watch_post"
    }
    fn name(&self) -> &'static str {
        "Watch Post"
    }
    fn description(&self) -> &'static str {
        "Stilted timber platform with a railing and a peaked roof."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        super::all_themes()
    }
    fn escalation_band(&self) -> EscalationBand {
        EscalationBand::only(EscalationTier::Conflict)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.4,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let leg_h = 2.0;
    let half = 0.55;
    let deck_y = leg_h;
    let post_h = 1.1; // railing-to-eave corner post
    let eave_y = deck_y + post_h;
    let leg = || solid(cylinder_tapered(0.08, leg_h, 8, 0.0, wood(WOOD)));

    let mut prims = Vec::new();

    // Four stilt legs.
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(leg(), [sx * half, leg_h * 0.5, sz * half], id_quat()));
    }

    // X-braces tying the legs on the back and side faces (front left open).
    for (cx, cz, vert) in [
        (0.0_f32, half, false), // back face
        (0.0, -half, false),    // front face
        (-half, 0.0, true),     // left face
        (half, 0.0, true),      // right face
    ] {
        for s in [-1.0_f32, 1.0] {
            let rot = if vert {
                quat_x(s * 0.75)
            } else {
                quat_z(s * 0.75)
            };
            prims.push(prim(
                solid(cuboid_tapered([0.05, 1.5, 0.05], 0.0, wood(WOOD_GREY))),
                [cx, leg_h * 0.5, cz],
                rot,
            ));
        }
    }

    // Platform deck + plank boards.
    prims.push(prim(
        solid(cuboid_tapered([1.45, 0.12, 1.45], 0.0, wood(WOOD_GREY))),
        [0.0, deck_y, 0.0],
        id_quat(),
    ));
    for dz in [-0.5_f32, -0.17, 0.17, 0.5] {
        prims.push(prim(
            solid(cuboid_tapered([1.4, 0.04, 0.28], 0.0, wood(WOOD))),
            [0.0, deck_y + 0.08, dz],
            id_quat(),
        ));
    }

    // Corner posts rising from the deck to carry the roof eaves.
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cylinder_tapered(0.06, post_h, 8, 0.0, wood(WOOD))),
            [sx * 0.66, deck_y + post_h * 0.5, sz * 0.66],
            id_quat(),
        ));
    }

    // Railing — top + mid rails around the back and sides; the front (-Z)
    // is left open as the lookout's vantage.
    for (rail_y, _label) in [(deck_y + 0.46, "top"), (deck_y + 0.24, "mid")] {
        // Back rail.
        prims.push(prim(
            solid(cuboid_tapered([1.4, 0.07, 0.07], 0.0, wood(WOOD))),
            [0.0, rail_y, 0.66],
            id_quat(),
        ));
        // Side rails.
        for sx in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.07, 0.07, 1.4], 0.0, wood(WOOD))),
                [sx * 0.66, rail_y, 0.0],
                id_quat(),
            ));
        }
    }

    // Square eave fascia the roof sits on.
    prims.push(prim(
        solid(cuboid_tapered(
            [1.55, 0.1, 1.55],
            0.0,
            wood([0.3, 0.2, 0.12]),
        )),
        [0.0, eave_y, 0.0],
        id_quat(),
    ));
    // Pyramidal roof resting on the posts, plus a finial.
    prims.push(prim(
        cone(1.15, 0.75, 4, wood([0.32, 0.22, 0.13])),
        [0.0, eave_y + 0.42, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cylinder_tapered(0.05, 0.2, 6, 0.0, wood(WOOD)),
        [0.0, eave_y + 0.85, 0.0],
        id_quat(),
    ));

    // A warning lantern hung under the eave at the open front.
    prims.push(prim(
        cuboid_tapered([0.16, 0.22, 0.16], 0.0, glow(LANTERN_WARM, 2.6)),
        [0.0, eave_y - 0.2, -0.5],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.04, 0.18, 0.04], 0.0, wood(WOOD))),
        [0.0, eave_y - 0.02, -0.5],
        id_quat(),
    ));

    // A leaning access ladder up the right side to the deck.
    let lad_x = half + 0.35;
    for sz in [-0.16_f32, 0.16] {
        prims.push(prim(
            solid(cuboid_tapered([0.06, 2.3, 0.06], 0.0, wood(WOOD))),
            [lad_x, leg_h * 0.55, sz],
            quat_z(0.18),
        ));
    }
    for r in 0..5 {
        let ry = 0.3 + r as f32 * 0.4;
        prims.push(prim(
            solid(cuboid_tapered([0.36, 0.05, 0.05], 0.0, wood(WOOD_GREY))),
            [lad_x + (ry - leg_h * 0.55) * 0.18, ry, 0.0],
            id_quat(),
        ));
    }

    super::assemble(prims)
}
