//! Owner Barn Quilt — the Rural-Farmland identity monument (#975).
//!
//! The painted quilt block a farm hangs on its gable end, raised on its own
//! frame at the gate: two weathered posts on a stone footing carry a
//! barn-board panel, the room owner's portrait is the quilt square on it under
//! a corrugated drip cap, and a hay bale and a yard lamp sit at the foot.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use crate::catalogue::items::util::{
    cuboid_tapered, cuboid_tapered_xz, footing, glow, id_quat, nest, pfp_panel, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BARN_RED, HAY_GOLD, LAMP_WARM, ROOF_GREY, STONE_GREY, TRIM_WHITE, WOOD_GREY, barn_board,
    enamel, metal_roof, stone, weathered,
};

const PANEL: f32 = 1.9;
const PANEL_Y: f32 = 3.15;

pub struct RuralFarmlandMonument;

impl CatalogueEntry for RuralFarmlandMonument {
    fn slug(&self) -> &'static str {
        "rural_farmland_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Barn Quilt"
    }
    fn description(&self) -> &'static str {
        "Barn-board quilt board on weathered posts, its square block the room owner."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::RuralFarmland]
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
    let base = prim(
        solid(cuboid_tapered([3.5, 0.34, 1.5], 0.07, stone(STONE_GREY))),
        [0.0, 0.17, 0.0],
        id_quat(),
    );

    let mut parts = Vec::new();
    // Buried plinth under the stone footing, so a slope-snapped board keeps
    // its ground under the downhill edge.
    parts.push(footing(3.5, 1.5, [0.0, 0.0], 2.6));
    for sx in [-1.0_f32, 1.0] {
        parts.push(post(sx * 1.36));
    }
    parts.push(nest(head(), quilt(did)));
    parts.push(hay_bale(-1.55));
    parts.push(yard_lamp(1.36));
    nest(base, parts)
}

/// A weathered post with an iron-strapped foot.
fn post(x: f32) -> Generator {
    let shaft = prim(
        solid(cuboid_tapered(
            [0.26, 4.3, 0.26],
            0.05,
            weathered(WOOD_GREY),
        )),
        [x, 2.49, 0.0],
        id_quat(),
    );
    nest(
        shaft,
        vec![prim(
            solid(cuboid_tapered(
                [0.34, 0.2, 0.34],
                0.0,
                enamel([0.28, 0.28, 0.29]),
            )),
            [x, 0.5, 0.0],
            id_quat(),
        )],
    )
}

/// The head rail, and everything it carries.
fn head() -> Generator {
    prim(
        solid(cuboid_tapered([3.4, 0.24, 0.28], 0.0, weathered(WOOD_GREY))),
        [0.0, 4.5, 0.0],
        id_quat(),
    )
}

/// The quilt board: barn-board backing, the portrait as the block, white
/// battens, and a corrugated drip cap over the lot.
fn quilt(did: &str) -> Vec<Generator> {
    let z = -0.16;
    let bat = 0.15;
    let mut out = vec![
        // Barn-board backing — the panel is single-sided, and a quilt board is
        // a solid sheet of siding.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.6, PANEL + 0.6, 0.1],
                0.0,
                barn_board(BARN_RED),
            )),
            [0.0, PANEL_Y, z + 0.07],
            id_quat(),
        ),
        pfp_panel(did, PANEL, [0.0, PANEL_Y, z]),
        // Corrugated drip cap: a ridge along X, so its slope sheds toward the
        // approach rather than presenting a flat lid.
        prim(
            solid(cuboid_tapered_xz(
                [3.5, 0.5, 1.1],
                [0.05, 0.9],
                metal_roof(ROOF_GREY),
            )),
            [0.0, 4.9, 0.0],
            id_quat(),
        ),
    ];
    // White battens framing the block.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [bat, PANEL + bat * 2.0, 0.12],
                0.0,
                weathered(TRIM_WHITE),
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
                weathered(TRIM_WHITE),
            )),
            [0.0, PANEL_Y + sy * (PANEL + bat) * 0.5, z - 0.03],
            id_quat(),
        ));
    }
    out
}

/// A hay bale leaning at the foot — the one piece of clutter, and the thing
/// that says farm rather than noticeboard.
fn hay_bale(x: f32) -> Generator {
    prim(
        solid(cuboid_tapered([0.9, 0.6, 0.62], 0.05, weathered(HAY_GOLD))),
        [x, 0.64, -0.5],
        id_quat(),
    )
}

/// A gooseneck yard lamp on one post. The quilt block is unlit and reads on
/// its own; this is what keeps the barn board and the weathered posts alive
/// after sunset.
fn yard_lamp(x: f32) -> Generator {
    let arm = prim(
        solid(cuboid_tapered(
            [0.5, 0.08, 0.08],
            0.0,
            enamel([0.26, 0.26, 0.27]),
        )),
        [x - 0.28, 3.9, -0.24],
        id_quat(),
    );
    nest(
        arm,
        vec![
            prim(
                solid(cuboid_tapered(
                    [0.4, 0.22, 0.4],
                    0.75,
                    enamel([0.24, 0.24, 0.25]),
                )),
                [x - 0.52, 3.78, -0.24],
                id_quat(),
            ),
            prim(
                cuboid_tapered([0.22, 0.08, 0.22], 0.0, glow(LAMP_WARM, 1.9)),
                [x - 0.52, 3.66, -0.24],
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
            &RuralFarmlandMonument.build("did:plc:test"),
            "rural_farmland_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&RuralFarmlandMonument, "did:plc:test");
    }
}
