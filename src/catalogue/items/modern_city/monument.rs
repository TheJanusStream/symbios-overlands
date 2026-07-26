//! Owner Marker — the Modern-City identity monument (#975).
//!
//! A civic wayfinding pylon: a brushed-steel monolith on a board-formed
//! concrete plinth, the room owner's portrait behind a recessed steel bezel
//! with a lit strip washing it from below, and a low bollard either side. The
//! object a plaza puts at its entrance to tell you what you have arrived at.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, glow, id_quat, nest, pfp_panel, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, LAMP_WARM, STEEL_GREY, concrete, enamel, steel};

const PANEL: f32 = 1.9;
const PANEL_Y: f32 = 3.2;

pub struct ModernCityMonument;

impl CatalogueEntry for ModernCityMonument {
    fn slug(&self) -> &'static str {
        "modern_city_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Marker"
    }
    fn description(&self) -> &'static str {
        "Brushed-steel wayfinding pylon displaying the room owner's portrait."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::ModernCity]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.4,
            min_spawn_dist: 8.0,
        }
    }
    fn build(&self, local_did: &str) -> Generator {
        build_tree(local_did)
    }
}

fn build_tree(did: &str) -> Generator {
    // Board-formed plinth — the root, and the thing the pylon and both
    // bollards stand on.
    let plinth = prim(
        solid(cuboid_tapered(
            [3.2, 0.36, 1.6],
            0.04,
            concrete(CONCRETE_GREY),
        )),
        [0.0, 0.18, 0.0],
        id_quat(),
    );
    // The monolith: a single clean mass, slightly tapered so it is not a
    // packing crate.
    let pylon = prim(
        solid(cuboid_tapered([2.5, 5.0, 0.55], 0.05, steel(STEEL_GREY))),
        [0.0, 2.86, 0.0],
        id_quat(),
    );

    nest(
        plinth,
        vec![nest(pylon, display(did)), bollard(-1.35), bollard(1.35)],
    )
}

/// The display: recessed bezel, portrait, an uplight strip and a shadow gap
/// that reads as the panel floating in the face of the steel.
fn display(did: &str) -> Vec<Generator> {
    let z = -0.31;
    let bezel = 0.13;
    let mut out = vec![
        // Dark backing plate — the panel is single-sided, and a wayfinding
        // pylon is opaque from behind.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.3, PANEL + 0.3, 0.09],
                0.0,
                enamel([0.16, 0.17, 0.19]),
            )),
            [0.0, PANEL_Y, z + 0.06],
            id_quat(),
        ),
        pfp_panel(did, PANEL, [0.0, PANEL_Y, z]),
        // Uplight washing the panel from a slot in the bezel's foot. The
        // portrait is unlit and legible on its own; this is what stops the
        // steel around it going dead after sunset.
        prim(
            cuboid_tapered([PANEL * 0.8, 0.06, 0.1], 0.0, glow(LAMP_WARM, 1.6)),
            [0.0, PANEL_Y - PANEL * 0.5 - 0.2, z - 0.05],
            id_quat(),
        ),
        // Blank ident band under the display.
        prim(
            solid(cuboid_tapered(
                [1.6, 0.26, 0.07],
                0.0,
                enamel([0.30, 0.32, 0.35]),
            )),
            [0.0, PANEL_Y - PANEL * 0.5 - 0.46, z - 0.02],
            id_quat(),
        ),
    ];
    // Bezel: four steel returns standing proud of the plate.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [bezel, PANEL + bezel * 2.0, 0.16],
                0.0,
                steel([0.68, 0.70, 0.73]),
            )),
            [sx * (PANEL + bezel) * 0.5, PANEL_Y, z - 0.04],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [PANEL, bezel, 0.16],
                0.0,
                steel([0.68, 0.70, 0.73]),
            )),
            [0.0, PANEL_Y + sy * (PANEL + bezel) * 0.5, z - 0.04],
            id_quat(),
        ));
    }
    out
}

/// A low steel bollard on the plinth, capped so it is not an open pipe.
fn bollard(x: f32) -> Generator {
    let post = prim(
        solid(cylinder_tapered(0.12, 0.9, 12, 0.04, steel(STEEL_GREY))),
        [x, 0.81, -0.5],
        id_quat(),
    );
    nest(
        post,
        vec![prim(
            solid(cylinder_tapered(
                0.14,
                0.08,
                12,
                0.3,
                steel([0.72, 0.74, 0.77]),
            )),
            [x, 1.3, -0.5],
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
            &ModernCityMonument.build("did:plc:test"),
            "modern_city_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&ModernCityMonument, "did:plc:test");
    }
}
