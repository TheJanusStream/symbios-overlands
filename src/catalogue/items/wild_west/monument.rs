//! Owner Notice Board — the Wild-West identity monument (#975).
//!
//! The board outside the sheriff's office: rough-sawn posts on a stone cairn
//! carry a plank board under a tin awning, the room owner's likeness pinned to
//! it as the poster, an iron hitching rail across the front and an oil lantern
//! hung off one post.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, footing, glow, id_quat, nest, pfp_panel, prim, quat_x,
    quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CLAP_TAN, CLAP_WHITE, GLASS_WARM, IRON_DARK, STONE_TAN, TIN_GREY, WOOD_RAW, clapboard, iron,
    stone, tin,
};

const PANEL: f32 = 1.8;
const PANEL_Y: f32 = 3.0;

pub struct WildWestMonument;

impl CatalogueEntry for WildWestMonument {
    fn slug(&self) -> &'static str {
        "wild_west_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Notice Board"
    }
    fn description(&self) -> &'static str {
        "Plank notice board under a tin awning, the room owner's likeness posted on it."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::WildWest]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.6,
            min_spawn_dist: 8.0,
        }
    }
    fn build(&self, local_did: &str) -> Generator {
        build_tree(local_did)
    }
}

fn build_tree(did: &str) -> Generator {
    // Stone cairn footing — the root, flat, so the canted post above spins
    // nothing.
    let cairn = prim(
        solid(cuboid_tapered([3.4, 0.4, 1.6], 0.16, stone(STONE_TAN))),
        [0.0, 0.2, 0.0],
        id_quat(),
    );

    let mut parts = Vec::new();
    for (sx, lean) in [(-1.0_f32, 0.0_f32), (1.0, -0.035)] {
        parts.push(post(sx * 1.32, lean));
    }
    parts.push(nest(header(), board(did)));
    parts.push(hitching_rail());
    parts.push(lantern(-1.32));
    // Buried footing under the cairn, so a terrain-snapped board on a slope
    // shows plinth rather than daylight under its downhill edge.
    parts.push(footing(3.4, 1.6, [0.0, 0.0], 2.6));
    nest(cairn, parts)
}

/// A rough-sawn post. The lean is on the post, which carries only its own
/// footing block.
fn post(x: f32, lean: f32) -> Generator {
    let shaft = prim(
        solid(cuboid_tapered([0.26, 4.2, 0.26], 0.05, clapboard(WOOD_RAW))),
        [x, 2.5, 0.0],
        quat_z(lean),
    );
    nest(
        shaft,
        vec![prim(
            solid(cuboid_tapered([0.4, 0.24, 0.4], 0.1, stone(STONE_TAN))),
            [x, 0.52, 0.0],
            id_quat(),
        )],
    )
}

/// The header the board hangs from, and everything above it.
fn header() -> Generator {
    prim(
        solid(cuboid_tapered([3.2, 0.24, 0.28], 0.0, clapboard(WOOD_RAW))),
        [0.0, 4.36, 0.0],
        id_quat(),
    )
}

/// The board: plank backing, the poster, a whitewashed batten frame and the
/// tin awning over it.
fn board(did: &str) -> Vec<Generator> {
    let z = -0.15;
    let bat = 0.13;
    let mut out = vec![
        // Plank backing — the panel is single-sided, and a notice board is a
        // solid sheet of boards.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.5, PANEL + 0.5, 0.1],
                0.0,
                clapboard(CLAP_TAN),
            )),
            [0.0, PANEL_Y, z + 0.07],
            id_quat(),
        ),
        pfp_panel(did, PANEL, [0.0, PANEL_Y, z]),
        // Tin awning, canted to shed rain over the poster. A leaf, so its tilt
        // carries nothing.
        prim(
            solid(cuboid_tapered([3.3, 0.1, 1.2], 0.0, tin(TIN_GREY))),
            [0.0, 4.62, -0.42],
            quat_x(-0.22),
        ),
    ];
    // Whitewashed battens.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [bat, PANEL + bat * 2.0, 0.12],
                0.0,
                clapboard(CLAP_WHITE),
            )),
            [sx * (PANEL + bat) * 0.5, PANEL_Y, z - 0.03],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [PANEL, bat, 0.12],
                0.0,
                clapboard(CLAP_WHITE),
            )),
            [0.0, PANEL_Y + sy * (PANEL + bat) * 0.5, z - 0.03],
            id_quat(),
        ));
    }
    out
}

/// An iron hitching rail across the front, on two short stubs — the prop that
/// says main street rather than parish noticeboard.
fn rail() -> Generator {
    prim(
        solid(cylinder_tapered(0.06, 2.6, 10, 0.0, iron(IRON_DARK))),
        [0.0, 1.25, -0.7],
        quat_z(FRAC_PI_2),
    )
}

fn hitching_rail() -> Generator {
    let mut stubs = vec![rail()];
    for sx in [-1.0_f32, 1.0] {
        stubs.push(prim(
            solid(cuboid_tapered([0.12, 0.9, 0.12], 0.06, clapboard(WOOD_RAW))),
            [sx * 1.2, 0.85, -0.7],
            id_quat(),
        ));
    }
    // The near stub is the sub-root: the rail and its far stub stand on the
    // same ground, and dragging one should take the whole rail.
    let root = stubs.remove(1);
    nest(root, stubs)
}

/// An oil lantern hung off one post. The poster is unlit and reads on its own;
/// this is what keeps the raw timber and tin alive after dark.
fn lantern(x: f32) -> Generator {
    let hook = prim(
        solid(cuboid_tapered([0.28, 0.07, 0.07], 0.0, iron(IRON_DARK))),
        [x + 0.18, 3.5, -0.2],
        id_quat(),
    );
    nest(
        hook,
        vec![
            prim(
                solid(cuboid_tapered([0.24, 0.34, 0.24], 0.2, iron(IRON_DARK))),
                [x + 0.32, 3.3, -0.2],
                id_quat(),
            ),
            prim(
                cuboid_tapered([0.15, 0.2, 0.15], 0.15, glow(GLASS_WARM, 2.0)),
                [x + 0.32, 3.3, -0.2],
                id_quat(),
            ),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{assert_owner_panel, assert_sanitize_stable};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(
            &WildWestMonument.build("did:plc:test"),
            "wild_west_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&WildWestMonument, "did:plc:test");
    }
}
