//! Owner Billboard — the Roadside identity monument (#975).
//!
//! The small highway board with the room owner's face on it: a chrome-collared
//! steel pole on a concrete footing carries a cream-framed billboard, two
//! gooseneck lamps rake it from above, and a neon strip runs under the frame.
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
    CHROME_BRIGHT, CONCRETE_GREY, ENAMEL_CREAM, ENAMEL_RED, NEON_CYAN, SIGN_AMBER, chrome,
    concrete, enamel, steel,
};

const PANEL: f32 = 2.0;
const PANEL_Y: f32 = 3.5;

pub struct RoadsideMonument;

impl CatalogueEntry for RoadsideMonument {
    fn slug(&self) -> &'static str {
        "roadside_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Billboard"
    }
    fn description(&self) -> &'static str {
        "Pole-mounted roadside billboard with gooseneck lamps, showing the room owner."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Roadside]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.5,
            min_spawn_dist: 8.0,
        }
    }
    fn build(&self, local_did: &str) -> Generator {
        build_tree(local_did)
    }
}

fn build_tree(did: &str) -> Generator {
    // Concrete footing — the root, and the only thing touching the ground:
    // a billboard is a single mast, not a frame on legs.
    let footing = prim(
        solid(cuboid_tapered(
            [1.5, 0.36, 1.3],
            0.12,
            concrete(CONCRETE_GREY),
        )),
        [0.0, 0.18, 0.0],
        id_quat(),
    );
    let mast = prim(
        solid(cylinder_tapered(
            0.22,
            3.4,
            14,
            0.08,
            steel([0.5, 0.51, 0.54]),
        )),
        [0.0, 2.06, 0.0],
        id_quat(),
    );
    let collar = prim(
        solid(cylinder_tapered(0.28, 0.24, 14, 0.1, chrome(CHROME_BRIGHT))),
        [0.0, 0.5, 0.0],
        id_quat(),
    );

    nest(footing, vec![collar, nest(mast, board(did))])
}

/// The board itself: enamel backing, portrait, cream frame, neon strip and
/// two gooseneck lamps.
fn board(did: &str) -> Vec<Generator> {
    let z = -0.16;
    let fr = 0.16;
    let mut out = vec![
        // Enamel backing — the panel is single-sided, and a billboard's back
        // is a blank painted sheet, not a hole.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.5, PANEL + 0.5, 0.12],
                0.0,
                enamel([0.30, 0.31, 0.33]),
            )),
            [0.0, PANEL_Y, z + 0.08],
            id_quat(),
        ),
        pfp_panel(did, PANEL, [0.0, PANEL_Y, z]),
        // Neon strip under the frame — deep-saturated at low strength, so it
        // reads as a colour under bloom instead of washing to white.
        prim(
            cuboid_tapered([PANEL + 0.4, 0.09, 0.09], 0.0, glow(NEON_CYAN, 1.5)),
            [0.0, PANEL_Y - PANEL * 0.5 - 0.26, z - 0.06],
            id_quat(),
        ),
        // Red top spar, the bit of enamel that says roadside rather than
        // civic.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.6, 0.2, 0.16],
                0.0,
                enamel(ENAMEL_RED),
            )),
            [0.0, PANEL_Y + PANEL * 0.5 + 0.34, z - 0.02],
            id_quat(),
        ),
    ];
    // Cream frame.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [fr, PANEL + fr * 2.0, 0.14],
                0.0,
                enamel(ENAMEL_CREAM),
            )),
            [sx * (PANEL + fr) * 0.5, PANEL_Y, z - 0.03],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered([PANEL, fr, 0.14], 0.0, enamel(ENAMEL_CREAM))),
            [0.0, PANEL_Y + sy * (PANEL + fr) * 0.5, z - 0.03],
            id_quat(),
        ));
    }
    // Gooseneck lamps over the board. The portrait is unlit and legible on its
    // own; these are what stop the enamel going dead after dark.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [0.09, 0.5, 0.09],
                0.0,
                chrome(CHROME_BRIGHT),
            )),
            [sx * 0.75, PANEL_Y + PANEL * 0.5 + 0.6, z - 0.1],
            id_quat(),
        ));
        out.push(prim(
            solid(cylinder_tapered(0.19, 0.2, 12, 0.5, chrome(CHROME_BRIGHT))),
            [sx * 0.75, PANEL_Y + PANEL * 0.5 + 0.82, z - 0.34],
            id_quat(),
        ));
        out.push(prim(
            cuboid_tapered([0.2, 0.06, 0.2], 0.0, glow(SIGN_AMBER, 1.8)),
            [sx * 0.75, PANEL_Y + PANEL * 0.5 + 0.71, z - 0.34],
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
        assert_sanitize_stable(&RoadsideMonument.build("did:plc:test"), "roadside_monument");
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&RoadsideMonument, "did:plc:test");
    }
}
