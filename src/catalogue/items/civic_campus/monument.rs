//! Owner Dedication — the Civic-Campus identity monument (#975).
//!
//! The dedication wall every campus has by its gate: a broad pale-stone wall
//! on a stepped base, the room owner's portrait behind a verdigris copper
//! surround, a copper cornice over it, and a campus lamp standing at each end.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, glow, id_quat, nest, pfp_panel, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, COPPER_VERDIGRIS, LAMP_WARM, MARBLE_WHITE, STEEL_GREY, STONE_PALE, copper,
    marble, paving, steel, stone,
};

const PANEL: f32 = 1.8;
const PANEL_Y: f32 = 2.85;

pub struct CivicCampusMonument;

impl CatalogueEntry for CivicCampusMonument {
    fn slug(&self) -> &'static str {
        "civic_campus_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Dedication"
    }
    fn description(&self) -> &'static str {
        "Stone dedication wall with a copper-framed portrait of the room's owner."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CivicCampus]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.0,
            min_spawn_dist: 8.0,
        }
    }
    fn build(&self, local_did: &str) -> Generator {
        build_tree(local_did)
    }
}

fn build_tree(did: &str) -> Generator {
    let step = prim(
        solid(cuboid_tapered(
            [4.6, 0.34, 2.0],
            0.05,
            paving(CONCRETE_GREY),
        )),
        [0.0, 0.17, 0.0],
        id_quat(),
    );
    let plinth = prim(
        solid(cuboid_tapered([4.0, 0.5, 1.5], 0.03, marble(MARBLE_WHITE))),
        [0.0, 0.59, 0.0],
        id_quat(),
    );
    // The wall itself — broad rather than tall, which is what makes it read as
    // civic dedication instead of headstone.
    let wall = prim(
        solid(cuboid_tapered([3.6, 3.6, 0.62], 0.02, stone(STONE_PALE))),
        [0.0, 2.64, 0.0],
        id_quat(),
    );

    nest(
        step,
        vec![nest(
            plinth,
            vec![nest(wall, dedication(did)), lamp(-2.05), lamp(2.05)],
        )],
    )
}

/// The portrait, its copper surround, the cornice and the blank dedication
/// band under it.
fn dedication(did: &str) -> Vec<Generator> {
    let z = -0.35;
    let bar = 0.16;
    let mut out = vec![
        // Recessed field, and the backing the single-sided panel needs.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.36, PANEL + 0.36, 0.09],
                0.0,
                stone([0.72, 0.71, 0.67]),
            )),
            [0.0, PANEL_Y, z + 0.06],
            id_quat(),
        ),
        pfp_panel(did, PANEL, [0.0, PANEL_Y, z]),
        // Copper cornice, oversailing the wall so its head is not a cut edge.
        prim(
            solid(cuboid_tapered(
                [3.9, 0.3, 0.9],
                0.06,
                copper(COPPER_VERDIGRIS),
            )),
            [0.0, 4.59, 0.0],
            id_quat(),
        ),
        // Dedication band — blank copper, because there is no text renderer;
        // it reads as the plate a name is cast into.
        prim(
            solid(cuboid_tapered(
                [2.4, 0.3, 0.08],
                0.0,
                copper(COPPER_VERDIGRIS),
            )),
            [0.0, PANEL_Y - PANEL * 0.5 - 0.44, z - 0.02],
            id_quat(),
        ),
    ];
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [bar, PANEL + bar * 2.0, 0.13],
                0.0,
                copper(COPPER_VERDIGRIS),
            )),
            [sx * (PANEL + bar) * 0.5, PANEL_Y, z - 0.03],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [PANEL, bar, 0.13],
                0.0,
                copper(COPPER_VERDIGRIS),
            )),
            [0.0, PANEL_Y + sy * (PANEL + bar) * 0.5, z - 0.03],
            id_quat(),
        ));
    }
    out
}

/// A campus lamp standing on the plinth. The portrait is unlit and reads on
/// its own; these are for the stone, which otherwise goes flat at dusk.
fn lamp(x: f32) -> Generator {
    let post = prim(
        solid(cylinder_tapered(0.09, 2.4, 12, 0.15, steel(STEEL_GREY))),
        [x, 2.04, -0.4],
        id_quat(),
    );
    nest(
        post,
        vec![
            prim(
                solid(cuboid_tapered([0.34, 0.28, 0.34], 0.5, steel(STEEL_GREY))),
                [x, 3.36, -0.4],
                id_quat(),
            ),
            prim(
                cuboid_tapered([0.22, 0.2, 0.22], 0.3, glow(LAMP_WARM, 1.8)),
                [x, 3.3, -0.4],
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
            &CivicCampusMonument.build("did:plc:test"),
            "civic_campus_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&CivicCampusMonument, "did:plc:test");
    }
}
