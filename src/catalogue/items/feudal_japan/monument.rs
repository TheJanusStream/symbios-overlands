//! Owner Ema Board — the Feudal-Japan identity monument (#975).
//!
//! A shrine notice-board: two lacquered posts on a dressed-stone base carry a
//! tiled kirizuma roof, and the room owner's portrait hangs beneath it as the
//! ema plaque, with a paper lantern on one post and a rope-and-fold shide
//! across the head. The board a visitor reads on the way in, saying whose
//! ground this is.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, glow, id_quat, nest, pfp_panel, prim,
    quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    LACQUER_RED, LANTERN_GLOW, PAPER_CREAM, STONE_GREY, TILE_SLATE, TIMBER_DARK, lacquer, paper,
    roof_tile, stone, timber,
};

const PANEL: f32 = 1.7;
const PANEL_Y: f32 = 2.95;
/// Blank tint — planed cedar, so an empty plaque reads as an unpainted ema.
const BLANK: [f32; 3] = [0.62, 0.50, 0.34];

pub struct FeudalJapanMonument;

impl CatalogueEntry for FeudalJapanMonument {
    fn slug(&self) -> &'static str {
        "feudal_japan_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Ema Board"
    }
    fn description(&self) -> &'static str {
        "Tile-roofed shrine board on lacquered posts, hanging the owner's ema plaque."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::FeudalJapan]
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
        solid(cuboid_tapered([3.4, 0.42, 1.5], 0.05, stone(STONE_GREY))),
        [0.0, 0.21, 0.0],
        id_quat(),
    );

    let mut parts = Vec::new();
    for sx in [-1.0_f32, 1.0] {
        parts.push(post(sx * 1.3));
    }
    parts.push(nest(head_beam(), board(did)));
    parts.push(lantern(1.3));
    nest(base, parts)
}

/// A lacquered post with a stone footing pad, so the timber never meets the
/// ground.
fn post(x: f32) -> Generator {
    let pad = prim(
        solid(cuboid_tapered([0.42, 0.18, 0.42], 0.08, stone(STONE_GREY))),
        [x, 0.51, 0.0],
        id_quat(),
    );
    nest(
        pad,
        vec![prim(
            solid(cylinder_tapered(0.15, 4.1, 12, 0.06, lacquer(LACQUER_RED))),
            [x, 2.65, 0.0],
            id_quat(),
        )],
    )
}

/// The head beam, and everything it carries — the board and the roof over it.
fn head_beam() -> Generator {
    prim(
        solid(cuboid_tapered([3.3, 0.24, 0.3], 0.0, timber(TIMBER_DARK))),
        [0.0, 4.32, 0.0],
        id_quat(),
    )
}

/// The hanging plaque, its cedar backing, the shide fold, and the tiled roof.
fn board(did: &str) -> Vec<Generator> {
    let z = -0.12;
    vec![
        // Cedar backing board — the plaque body, and the backing the
        // single-sided panel needs.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.46, PANEL + 0.4, 0.09],
                0.0,
                timber([0.52, 0.42, 0.30]),
            )),
            [0.0, PANEL_Y, z + 0.06],
            id_quat(),
        ),
        prim(
            pfp_panel(did, PANEL, BLANK),
            [0.0, PANEL_Y, z],
            quat_x(-FRAC_PI_2),
        ),
        // Lacquer frame rails top and bottom.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.5, 0.14, 0.13],
                0.0,
                lacquer(LACQUER_RED),
            )),
            [0.0, PANEL_Y + PANEL * 0.5 + 0.16, z - 0.02],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.5, 0.14, 0.13],
                0.0,
                lacquer(LACQUER_RED),
            )),
            [0.0, PANEL_Y - PANEL * 0.5 - 0.16, z - 0.02],
            id_quat(),
        ),
        // Paper shide folds hanging from the head beam.
        prim(
            solid(cuboid_tapered([2.9, 0.3, 0.05], 0.0, paper(PAPER_CREAM))),
            [0.0, 4.08, z - 0.05],
            id_quat(),
        ),
        // Kirizuma roof: a ridge along X, so the gable faces the approach.
        prim(
            solid(cuboid_tapered_xz(
                [4.0, 0.85, 1.7],
                [0.06, 0.94],
                roof_tile(TILE_SLATE),
            )),
            [0.0, 4.86, 0.0],
            id_quat(),
        ),
    ]
}

/// A paper lantern on one post. The plaque is unlit and reads on its own; this
/// is what keeps the lacquer and the tile alive at dusk.
fn lantern(x: f32) -> Generator {
    let arm = prim(
        solid(cuboid_tapered([0.36, 0.09, 0.09], 0.0, timber(TIMBER_DARK))),
        [x + 0.26, 3.15, -0.1],
        id_quat(),
    );
    nest(
        arm,
        vec![prim(
            solid(cylinder_tapered(
                0.22,
                0.5,
                12,
                -0.25,
                glow(LANTERN_GLOW, 1.6),
            )),
            [x + 0.46, 2.86, -0.1],
            id_quat(),
        )],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{assert_owner_panel, assert_sanitize_stable};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(
            &FeudalJapanMonument.build("did:plc:test"),
            "feudal_japan_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&FeudalJapanMonument, "did:plc:test");
    }
}
