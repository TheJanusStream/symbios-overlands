//! Owner Banner — the Medieval identity monument (#975).
//!
//! A heraldic banner on a gallows frame: two oak posts on a rough-stone
//! footing carry a cross-beam, and the room owner's portrait hangs from it on
//! iron rings as the banner's field, with a cloth valance below and an iron
//! cresset burning on one post. The arms a hall flies over its gate — with the
//! owner's own face in place of the charge.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, footing, glow, id_quat, nest, pfp_panel, prim, quat_x, solid,
    torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    FORGE_ORANGE, HERALD_GOLD, HERALD_RED, IRON_DARK, STONE_GREY, WOOD_OAK, cloth, iron,
    rough_stone, timber,
};

const PANEL: f32 = 1.8;
const PANEL_Y: f32 = 3.2;

pub struct MedievalMonument;

impl CatalogueEntry for MedievalMonument {
    fn slug(&self) -> &'static str {
        "medieval_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Banner"
    }
    fn description(&self) -> &'static str {
        "Heraldic banner on an oak gallows frame, bearing the room owner's arms."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Medieval]
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
    // Rough-stone footing — the root, and flat, so nothing above inherits a
    // tilt.
    let base = prim(
        solid(cuboid_tapered(
            [3.6, 0.45, 1.6],
            0.06,
            rough_stone(STONE_GREY),
        )),
        [0.0, 0.22, 0.0],
        id_quat(),
    );

    let mut parts = vec![footing(3.6, 1.6, [0.0, 0.0], 2.6)];
    for sx in [-1.0_f32, 1.0] {
        parts.push(post(sx * 1.42));
    }
    parts.push(nest(beam(), banner(did)));
    parts.push(cresset(-1.42));

    nest(base, parts)
}

/// An oak post with an iron collar at its foot.
fn post(x: f32) -> Generator {
    let shaft = prim(
        solid(cuboid_tapered([0.28, 4.6, 0.28], 0.06, timber(WOOD_OAK))),
        [x, 2.75, 0.0],
        id_quat(),
    );
    nest(
        shaft,
        vec![prim(
            solid(cuboid_tapered([0.36, 0.24, 0.36], 0.0, iron(IRON_DARK))),
            [x, 0.62, 0.0],
            id_quat(),
        )],
    )
}

/// The cross-beam the banner hangs from — and the parent of everything it
/// carries, so dragging the beam takes the banner with it.
fn beam() -> Generator {
    prim(
        solid(cuboid_tapered([3.5, 0.3, 0.34], 0.0, timber(WOOD_OAK))),
        [0.0, 4.9, 0.0],
        id_quat(),
    )
}

/// The banner: a cloth field, the portrait on it, iron hanging rings, and a
/// dagged valance along the bottom.
fn banner(did: &str) -> Vec<Generator> {
    let z = -0.1;
    let mut out = vec![
        // Cloth field. Also the backing plate: the panel is single-sided, so
        // this is what stops the banner being see-through from behind.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.5, PANEL + 0.7, 0.06],
                0.0,
                cloth(HERALD_RED, [0.34, 0.09, 0.09]),
            )),
            [0.0, PANEL_Y - 0.05, z + 0.05],
            id_quat(),
        ),
        pfp_panel(did, PANEL, [0.0, PANEL_Y, z]),
        // Gold fringe along the head of the banner.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.5, 0.12, 0.09],
                0.0,
                cloth(HERALD_GOLD, HERALD_GOLD),
            )),
            [0.0, PANEL_Y + PANEL * 0.5 + 0.3, z],
            id_quat(),
        ),
    ];
    // Hanging rings over the beam.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(torus(0.035, 0.13, iron(IRON_DARK))),
            [sx * 0.6, 4.78, z + 0.05],
            quat_x(FRAC_PI_2),
        ));
    }
    // Dagged valance: three tails along the banner's foot.
    for i in -1..=1 {
        out.push(prim(
            solid(cuboid_tapered(
                [0.62, 0.42, 0.05],
                0.85,
                cloth(HERALD_GOLD, HERALD_RED),
            )),
            [i as f32 * 0.72, PANEL_Y - PANEL * 0.5 - 0.42, z + 0.02],
            id_quat(),
        ));
    }
    out
}

/// An iron cresset on one post — for the oak and the stone, which go flat at
/// dusk; the portrait is unlit and reads on its own.
fn cresset(x: f32) -> Generator {
    let arm = prim(
        solid(cuboid_tapered([0.5, 0.12, 0.12], 0.0, iron(IRON_DARK))),
        [x - 0.3, 2.5, -0.2],
        id_quat(),
    );
    nest(
        arm,
        vec![
            prim(
                solid(cylinder_tapered(0.19, 0.3, 10, 0.45, iron(IRON_DARK))),
                [x - 0.52, 2.66, -0.2],
                id_quat(),
            ),
            prim(
                cuboid_tapered([0.22, 0.3, 0.22], 0.7, glow(FORGE_ORANGE, 2.4)),
                [x - 0.52, 2.88, -0.2],
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
        assert_sanitize_stable(&MedievalMonument.build("did:plc:test"), "medieval_monument");
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&MedievalMonument, "did:plc:test");
    }
}
