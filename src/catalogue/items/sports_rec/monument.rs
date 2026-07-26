//! Owner Scoreboard — the Sports-and-Rec identity monument (#975).
//!
//! The board at the end of the ground: a steel truss on two concrete pads
//! carries a black scoreboard housing, the room owner's portrait is the big
//! screen in it, a lit score strip runs beneath and a floodlight bank sits on
//! the head.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use crate::catalogue::items::util::{cuboid_tapered, glow, id_quat, nest, pfp_panel, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, FLOOD_LIT, SCORE_LIT, SCORE_RED, STEEL_GREY, concrete, enamel, steel};

const PANEL: f32 = 2.0;
const PANEL_Y: f32 = 3.4;

pub struct SportsRecMonument;

impl CatalogueEntry for SportsRecMonument {
    fn slug(&self) -> &'static str {
        "sports_rec_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Scoreboard"
    }
    fn description(&self) -> &'static str {
        "Truss-mounted scoreboard whose screen carries the room owner's portrait."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SportsRec]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.8,
            min_spawn_dist: 8.0,
        }
    }
    fn build(&self, local_did: &str) -> Generator {
        build_tree(local_did)
    }
}

fn build_tree(did: &str) -> Generator {
    let pad = prim(
        solid(cuboid_tapered(
            [4.0, 0.3, 1.5],
            0.05,
            concrete(CONCRETE_GREY),
        )),
        [0.0, 0.15, 0.0],
        id_quat(),
    );

    let mut parts = Vec::new();
    for sx in [-1.0_f32, 1.0] {
        parts.push(leg(sx * 1.5));
    }
    parts.push(nest(truss(), housing(did)));
    nest(pad, parts)
}

/// A truss leg with two cross-braces, so it reads as lattice rather than as a
/// solid post.
fn leg(x: f32) -> Generator {
    let column = prim(
        solid(cuboid_tapered([0.2, 4.4, 0.2], 0.03, steel(STEEL_GREY))),
        [x, 2.5, 0.0],
        id_quat(),
    );
    let mut braces = Vec::new();
    for i in 0..2 {
        braces.push(prim(
            solid(cuboid_tapered([0.28, 0.1, 0.28], 0.0, steel(STEEL_GREY))),
            [x, 1.4 + i as f32 * 1.5, 0.0],
            id_quat(),
        ));
    }
    nest(column, braces)
}

/// The head truss, and everything it carries.
fn truss() -> Generator {
    prim(
        solid(cuboid_tapered([3.9, 0.3, 0.32], 0.0, steel(STEEL_GREY))),
        [0.0, 4.62, 0.0],
        id_quat(),
    )
}

/// The scoreboard: black housing, the screen, a lit score strip and the
/// floodlight bank on top.
fn housing(did: &str) -> Vec<Generator> {
    let z = -0.18;
    let bez = 0.15;
    let mut out = vec![
        // Housing box — the panel is single-sided, and a scoreboard is a solid
        // black case from behind.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.55, PANEL + 0.95, 0.28],
                0.0,
                enamel([0.10, 0.11, 0.12]),
            )),
            [0.0, PANEL_Y - 0.18, z + 0.22],
            id_quat(),
        ),
        pfp_panel(did, PANEL, [0.0, PANEL_Y, z]),
        // Score strip below the screen — two lit digit blocks and a red
        // period marker, the layout the kit's own scoreboards use.
        prim(
            cuboid_tapered([0.62, 0.34, 0.06], 0.0, glow(SCORE_LIT, 2.0)),
            [-0.6, PANEL_Y - PANEL * 0.5 - 0.4, z - 0.04],
            id_quat(),
        ),
        prim(
            cuboid_tapered([0.62, 0.34, 0.06], 0.0, glow(SCORE_LIT, 2.0)),
            [0.6, PANEL_Y - PANEL * 0.5 - 0.4, z - 0.04],
            id_quat(),
        ),
        prim(
            cuboid_tapered([0.18, 0.18, 0.06], 0.0, glow(SCORE_RED, 1.8)),
            [0.0, PANEL_Y - PANEL * 0.5 - 0.4, z - 0.04],
            id_quat(),
        ),
    ];
    // Bezel returns round the screen.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [bez, PANEL + bez * 2.0, 0.14],
                0.0,
                steel([0.62, 0.63, 0.66]),
            )),
            [sx * (PANEL + bez) * 0.5, PANEL_Y, z - 0.04],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [PANEL, bez, 0.14],
                0.0,
                steel([0.62, 0.63, 0.66]),
            )),
            [0.0, PANEL_Y + sy * (PANEL + bez) * 0.5, z - 0.04],
            id_quat(),
        ));
    }
    // Floodlight bank on the head. The screen is unlit and legible on its own;
    // this is what makes the truss read at night.
    for i in -1..=1 {
        out.push(prim(
            solid(cuboid_tapered([0.44, 0.3, 0.24], 0.15, steel(STEEL_GREY))),
            [i as f32 * 0.72, 4.94, z - 0.1],
            id_quat(),
        ));
        out.push(prim(
            cuboid_tapered([0.34, 0.2, 0.06], 0.0, glow(FLOOD_LIT, 2.2)),
            [i as f32 * 0.72, 4.94, z - 0.24],
            id_quat(),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{assert_owner_panel, assert_sanitize_stable};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(
            &SportsRecMonument.build("did:plc:test"),
            "sports_rec_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&SportsRecMonument, "did:plc:test");
    }
}
