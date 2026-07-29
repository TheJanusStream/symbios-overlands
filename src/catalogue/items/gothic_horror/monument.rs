//! Owner Memorial — the Gothic-Horror identity monument (#975).
//!
//! A graveyard memorial: a dark stone shrine on a mossy plinth, the room
//! owner's portrait set behind a wrought-iron surround under a pointed arch,
//! a spire needle over it, iron finials at the corners and a votive candle
//! burning in a lantern at the foot.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, footing, glow, id_quat, nest, pfp_panel, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BONE, IRON_BLACK, STONE_DARK, STONE_MOSS, iron, mossy, pointed_arch, spire, stone};

const PANEL: f32 = 1.7;
const PANEL_Y: f32 = 2.95;
/// Candle flame — deep amber at low strength, so it reads as one small light
/// in the dark rather than a lantern that has bloomed to white.
const CANDLE: [f32; 3] = [1.0, 0.60, 0.22];

pub struct GothicHorrorMonument;

impl CatalogueEntry for GothicHorrorMonument {
    fn slug(&self) -> &'static str {
        "gothic_horror_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Memorial"
    }
    fn description(&self) -> &'static str {
        "Iron-framed memorial under a pointed arch, bearing the room owner's portrait."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
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
    let plinth = prim(
        solid(cuboid_tapered([3.4, 0.4, 2.0], 0.1, mossy(STONE_MOSS))),
        [0.0, 0.2, 0.0],
        id_quat(),
    );
    let step = prim(
        solid(cuboid_tapered([2.8, 0.34, 1.6], 0.05, stone(STONE_DARK))),
        [0.0, 0.57, 0.0],
        id_quat(),
    );
    // The shrine body the arch is cut into.
    let body = prim(
        solid(cuboid_tapered([2.4, 3.9, 0.8], 0.04, stone(STONE_DARK))),
        [0.0, 2.69, 0.0],
        id_quat(),
    );

    nest(
        plinth,
        vec![
            nest(step, vec![nest(body, shrine(did)), lantern(-1.2)]),
            footing(3.4, 2.0, [0.0, 0.0], 2.7),
        ],
    )
}

/// The shrine face: the portrait, its iron surround, the pointed arch over it,
/// corner finials and the spire needle on top.
fn shrine(did: &str) -> Vec<Generator> {
    let z = -0.46;
    let fr = 0.13;
    let mut out = vec![
        // Recessed tablet — the backing the single-sided panel needs, and the
        // stone the portrait reads as being cut into.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.26, PANEL + 0.26, 0.1],
                0.0,
                stone([0.30, 0.29, 0.30]),
            )),
            [0.0, PANEL_Y, z + 0.07],
            id_quat(),
        ),
        pfp_panel(did, PANEL, [0.0, PANEL_Y, z]),
        // Bone-pale dedication tablet under the portrait.
        prim(
            solid(cuboid_tapered([1.4, 0.3, 0.09], 0.0, stone(BONE))),
            [0.0, PANEL_Y - PANEL * 0.5 - 0.4, z - 0.02],
            id_quat(),
        ),
    ];
    // The kit's own pointed arch, springing above the portrait.
    out.extend(pointed_arch(
        [0.0, PANEL_Y + PANEL * 0.5 + 0.2, z - 0.04],
        1.05,
        0.18,
        stone(STONE_DARK),
    ));
    // Wrought-iron surround.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [fr, PANEL + fr * 2.0, 0.12],
                0.0,
                iron(IRON_BLACK),
            )),
            [sx * (PANEL + fr) * 0.5, PANEL_Y, z - 0.03],
            id_quat(),
        ));
        // Corner finial on the body's shoulder.
        out.push(prim(
            solid(cylinder_tapered(0.1, 0.6, 8, 0.6, iron(IRON_BLACK))),
            [sx * 1.05, 4.94, 0.0],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered([PANEL, fr, 0.12], 0.0, iron(IRON_BLACK))),
            [0.0, PANEL_Y + sy * (PANEL + fr) * 0.5, z - 0.03],
            id_quat(),
        ));
    }
    // Spire needle on the shrine's head.
    out.extend(spire([0.0, 4.66, 0.0], 0.36, 1.5, stone(STONE_DARK)));
    out
}

/// A votive lantern on the step. The portrait is unlit and reads on its own;
/// this is the only warm thing on a monument the theme otherwise keeps black.
fn lantern(x: f32) -> Generator {
    let post = prim(
        solid(cylinder_tapered(0.07, 1.0, 8, 0.1, iron(IRON_BLACK))),
        [x, 1.24, -0.75],
        id_quat(),
    );
    nest(
        post,
        vec![
            prim(
                solid(cuboid_tapered([0.26, 0.34, 0.26], 0.25, iron(IRON_BLACK))),
                [x, 1.9, -0.75],
                id_quat(),
            ),
            prim(
                cuboid_tapered([0.15, 0.2, 0.15], 0.2, glow(CANDLE, 2.0)),
                [x, 1.88, -0.75],
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
            &GothicHorrorMonument.build("did:plc:test"),
            "gothic_horror_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&GothicHorrorMonument, "did:plc:test");
    }
}
