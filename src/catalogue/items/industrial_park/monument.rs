//! Owner Site Board — the Industrial-Park identity monument (#975).
//!
//! The site-entrance board every works has at its gate: a steel gantry on a
//! concrete pad, hazard-striped legs, the room owner's portrait behind a
//! clad-steel bezel where the site notice goes, a floodlight raking it from
//! above and a rust-streaked bollard either side.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, glow, id_quat, nest, pfp_panel, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, FLOOD_WHITE, PIPE_GREY, RUST_BROWN, STEEL_BLUE, cladding, concrete, rust,
    tank_steel,
};

const PANEL: f32 = 1.9;
const PANEL_Y: f32 = 3.3;
/// Hazard yellow for the striped leg guards.
const HAZARD: [f32; 3] = [0.82, 0.68, 0.12];
/// Blank tint — the grey-green of an unprinted site notice, so an owner with
/// no picture reads as a board waiting to be posted.
const BLANK: [f32; 3] = [0.36, 0.39, 0.38];

pub struct IndustrialParkMonument;

impl CatalogueEntry for IndustrialParkMonument {
    fn slug(&self) -> &'static str {
        "industrial_park_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Site Board"
    }
    fn description(&self) -> &'static str {
        "Steel gantry site board on a concrete pad, carrying the room owner's portrait."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::IndustrialPark]
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
    let pad = prim(
        solid(cuboid_tapered(
            [3.8, 0.32, 1.7],
            0.04,
            concrete(CONCRETE_GREY),
        )),
        [0.0, 0.16, 0.0],
        id_quat(),
    );

    let mut parts = Vec::new();
    for sx in [-1.0_f32, 1.0] {
        parts.push(leg(sx * 1.5));
        parts.push(bollard(sx * 1.72));
    }
    parts.push(nest(gantry(), board(did)));
    nest(pad, parts)
}

/// A gantry leg: a steel column with a hazard-striped guard at ankle height,
/// which is where a works actually paints them.
fn leg(x: f32) -> Generator {
    let column = prim(
        solid(cuboid_tapered(
            [0.24, 4.6, 0.24],
            0.03,
            tank_steel(PIPE_GREY),
        )),
        [x, 2.62, 0.0],
        id_quat(),
    );
    let mut bands = Vec::new();
    for i in 0..3 {
        bands.push(prim(
            solid(cuboid_tapered([0.28, 0.16, 0.28], 0.0, cladding(HAZARD))),
            [x, 0.62 + i as f32 * 0.32, 0.0],
            id_quat(),
        ));
    }
    nest(column, bands)
}

/// The cross-head the board hangs from, and everything it carries.
fn gantry() -> Generator {
    prim(
        solid(cuboid_tapered(
            [3.7, 0.3, 0.34],
            0.0,
            tank_steel(STEEL_BLUE),
        )),
        [0.0, 4.8, 0.0],
        id_quat(),
    )
}

/// The notice board: clad backing, portrait, bolted bezel, and the floodlight
/// raking it from the gantry.
fn board(did: &str) -> Vec<Generator> {
    let z = -0.2;
    let bez = 0.14;
    let mut out = vec![
        // Clad backing sheet — the panel is single-sided, and a site board is
        // opaque from the yard behind it.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.44, PANEL + 0.44, 0.09],
                0.0,
                cladding(STEEL_BLUE),
            )),
            [0.0, PANEL_Y, z + 0.06],
            id_quat(),
        ),
        prim(
            pfp_panel(did, PANEL, BLANK),
            [0.0, PANEL_Y, z],
            quat_x(-FRAC_PI_2),
        ),
        // Floodlight on a bracket, aimed down the face. The portrait is unlit
        // and legible on its own; this is what stops the steel around it
        // going dead at night.
        prim(
            solid(cuboid_tapered([0.5, 0.24, 0.3], 0.2, tank_steel(PIPE_GREY))),
            [0.0, 4.48, z - 0.35],
            id_quat(),
        ),
        prim(
            cuboid_tapered([0.4, 0.1, 0.16], 0.0, glow(FLOOD_WHITE, 2.0)),
            [0.0, 4.38, z - 0.42],
            id_quat(),
        ),
        // Blank ident strip under the board, where the site name is stencilled.
        prim(
            solid(cuboid_tapered(
                [1.7, 0.28, 0.08],
                0.0,
                cladding([0.72, 0.72, 0.70]),
            )),
            [0.0, PANEL_Y - PANEL * 0.5 - 0.44, z - 0.02],
            id_quat(),
        ),
    ];
    // Bolted bezel returns.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [bez, PANEL + bez * 2.0, 0.14],
                0.0,
                tank_steel(PIPE_GREY),
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
                tank_steel(PIPE_GREY),
            )),
            [0.0, PANEL_Y + sy * (PANEL + bez) * 0.5, z - 0.04],
            id_quat(),
        ));
    }
    out
}

/// A rust-streaked bollard on the pad — the kit's signature wear.
fn bollard(x: f32) -> Generator {
    prim(
        solid(cylinder_tapered(0.13, 0.85, 12, 0.06, rust(RUST_BROWN))),
        [x, 0.75, -0.55],
        id_quat(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{assert_owner_panel, assert_sanitize_stable};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(
            &IndustrialParkMonument.build("did:plc:test"),
            "industrial_park_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&IndustrialParkMonument, "did:plc:test");
    }
}
