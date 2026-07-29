//! Owner Rune-Stone — the Nordic identity monument (#975).
//!
//! A raised standing stone on a packed cairn: the room owner's portrait is set
//! into the face inside a carved timber knotwork border, with two smaller
//! guardian stones flanking it and an iron fire-bowl at its foot. The memorial
//! stone a family raises by the road, with the face it commemorates on it.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, footing, glow, id_quat, nest, pfp_panel, prim, quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    FIRE_ORANGE, IRON_DARK, STONE_COLD, STONE_GREY, WOOD_DARK, iron, rough_stone, stone, timber,
};

// The slab is `rough_stone` rather than dressed `stone`: a raised memorial is
// a split boulder, not masonry, and the rougher surface is also the paler of
// the two — which matters because the border, the field and a blank panel are
// all dark, and the whole monument was reading as one unlit mass.

const PANEL: f32 = 1.7;
const PANEL_Y: f32 = 3.15;

pub struct NordicMonument;

impl CatalogueEntry for NordicMonument {
    fn slug(&self) -> &'static str {
        "nordic_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Rune-Stone"
    }
    fn description(&self) -> &'static str {
        "Raised standing stone in a knotwork border, carrying the room owner's likeness."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Nordic]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.7,
            min_spawn_dist: 8.0,
        }
    }
    fn build(&self, local_did: &str) -> Generator {
        build_tree(local_did)
    }
}

fn build_tree(did: &str) -> Generator {
    // Packed cairn — the root, flat, so the leaning guardian stones above
    // cannot spin anything.
    let cairn = prim(
        solid(cuboid_tapered(
            [3.8, 0.5, 2.2],
            0.18,
            rough_stone(STONE_COLD),
        )),
        [0.0, 0.25, 0.0],
        id_quat(),
    );

    // The stone itself: wide, slightly tapered, with a rounded shoulder — the
    // silhouette that says rune-stone rather than slab.
    let slab = prim(
        solid(cuboid_tapered(
            [2.5, 4.4, 0.6],
            0.16,
            rough_stone(STONE_GREY),
        )),
        [0.0, 2.7, 0.1],
        id_quat(),
    );

    let mut parts = vec![nest(slab, face(did)), fire_bowl()];
    for sx in [-1.0_f32, 1.0] {
        parts.push(guardian(sx * 1.55, sx * 0.14));
    }
    // Buried footing under the cairn. `nest` rebases it out of the ground
    // frame into the cairn's local one.
    parts.push(footing(3.8, 2.2, [0.0, 0.0], 2.7));
    nest(cairn, parts)
}

/// The carved face: knotwork border, the portrait, and a painted rune band.
fn face(did: &str) -> Vec<Generator> {
    let z = -0.24;
    let bar = 0.18;
    let mut out = vec![
        // Sunk field — the backing the portrait is carved into, and what
        // stops the stone being see-through from behind.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.16, PANEL + 0.16, 0.1],
                0.0,
                stone([0.66, 0.65, 0.62]),
            )),
            [0.0, PANEL_Y, z + 0.06],
            id_quat(),
        ),
        pfp_panel(did, PANEL, [0.0, PANEL_Y, z]),
        // Rune band across the foot of the stone.
        prim(
            solid(cuboid_tapered([2.0, 0.3, 0.1], 0.0, timber(WOOD_DARK))),
            [0.0, PANEL_Y - PANEL * 0.5 - 0.5, z - 0.02],
            id_quat(),
        ),
    ];
    // Knotwork border — four carved timber bars around the field.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [bar, PANEL + bar * 2.0, 0.14],
                0.0,
                timber(WOOD_DARK),
            )),
            [sx * (PANEL + bar) * 0.5, PANEL_Y, z - 0.03],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered([PANEL, bar, 0.14], 0.0, timber(WOOD_DARK))),
            [0.0, PANEL_Y + sy * (PANEL + bar) * 0.5, z - 0.03],
            id_quat(),
        ));
    }
    out
}

/// A smaller stone leaning at the base. The lean is on the leaf, never on a
/// parent: a tilted sub-root would spin everything above it.
fn guardian(x: f32, lean: f32) -> Generator {
    prim(
        solid(cuboid_tapered(
            [0.7, 1.7, 0.4],
            0.3,
            rough_stone(STONE_GREY),
        )),
        [x, 1.35, -0.35],
        quat_z(lean),
    )
}

/// Iron fire-bowl on a tripod at the stone's foot — for the granite, which
/// goes flat at dusk; the portrait is unlit and reads on its own.
fn fire_bowl() -> Generator {
    let legs = prim(
        solid(cylinder_tapered(0.26, 0.7, 8, 0.5, iron(IRON_DARK))),
        [0.0, 0.85, -0.95],
        id_quat(),
    );
    nest(
        legs,
        vec![
            prim(
                solid(cylinder_tapered(0.42, 0.26, 12, 0.4, iron(IRON_DARK))),
                [0.0, 1.28, -0.95],
                id_quat(),
            ),
            prim(
                cuboid_tapered([0.42, 0.3, 0.42], 0.6, glow(FIRE_ORANGE, 2.4)),
                [0.0, 1.44, -0.95],
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
        assert_sanitize_stable(&NordicMonument.build("did:plc:test"), "nordic_monument");
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&NordicMonument, "did:plc:test");
    }
}
