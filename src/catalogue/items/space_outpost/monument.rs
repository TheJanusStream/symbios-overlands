//! Owner Crew Marker — the Space-Outpost identity monument (#975).
//!
//! The crew placard bolted by an airlock: a white composite pylon on a scorched
//! landing pad, the room owner's portrait behind a cyan-lit viewport bezel, a
//! hazard-striped kick plate, a status light and a stub antenna on the head.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, glow, id_quat, nest, pfp_panel, prim, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BEACON_RED, HAZARD_YELLOW, HULL_PANEL, HULL_WHITE, PAD_GREY, STATUS_GREEN, STEEL_DARK,
    VIEWPORT_LIT, concrete, hull, painted, steel,
};

const PANEL: f32 = 1.8;
const PANEL_Y: f32 = 3.25;
/// Blank tint — a powered-down flight display, which on an outpost reads as a
/// screen on standby rather than as a failure.
const BLANK: [f32; 3] = [0.16, 0.20, 0.23];

pub struct SpaceOutpostMonument;

impl CatalogueEntry for SpaceOutpostMonument {
    fn slug(&self) -> &'static str {
        "space_outpost_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Crew Marker"
    }
    fn description(&self) -> &'static str {
        "Composite crew placard with a lit viewport bezel, carrying the room owner."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SpaceOutpost]
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
    let pad = prim(
        solid(cylinder_tapered(1.6, 0.28, 16, 0.06, concrete(PAD_GREY))),
        [0.0, 0.14, 0.0],
        id_quat(),
    );
    // The pylon: a single composite mass with a slight batter, so it reads as
    // moulded rather than fabricated.
    let pylon = prim(
        solid(cuboid_tapered([2.4, 5.0, 0.62], 0.08, hull(HULL_WHITE))),
        [0.0, 2.78, 0.0],
        id_quat(),
    );

    nest(pad, vec![nest(pylon, placard(did))])
}

/// The placard: hull backing, portrait, viewport bezel, kick plate, status
/// light and antenna.
fn placard(did: &str) -> Vec<Generator> {
    let z = -0.3;
    let bez = 0.14;
    let mut out = vec![
        // Recessed hull panel — the backing the single-sided panel needs.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.3, PANEL + 0.3, 0.08],
                0.0,
                hull(HULL_PANEL),
            )),
            [0.0, PANEL_Y, z + 0.06],
            id_quat(),
        ),
        prim(
            pfp_panel(did, PANEL, BLANK),
            [0.0, PANEL_Y, z],
            quat_x(-FRAC_PI_2),
        ),
        // Hazard-striped kick plate at the foot, where boots and cargo hit it.
        prim(
            solid(cuboid_tapered(
                [2.2, 0.44, 0.1],
                0.0,
                painted(HAZARD_YELLOW),
            )),
            [0.0, 0.72, z - 0.02],
            id_quat(),
        ),
        // Blank designation strip under the display.
        prim(
            solid(cuboid_tapered([1.5, 0.24, 0.07], 0.0, painted(STEEL_DARK))),
            [0.0, PANEL_Y - PANEL * 0.5 - 0.4, z - 0.02],
            id_quat(),
        ),
        // Stub antenna and a status light on the head.
        prim(
            solid(cylinder_tapered(0.05, 1.1, 8, 0.4, steel(STEEL_DARK))),
            [0.62, 5.75, 0.0],
            id_quat(),
        ),
        prim(
            // Resolution 6: the sanitiser clamps a sphere there, and anything
            // higher is silently rewritten — which fails the round-trip test
            // rather than rendering differently.
            solid(sphere(0.12, 6, glow(BEACON_RED, 2.0))),
            [0.62, 6.34, 0.0],
            id_quat(),
        ),
        prim(
            cuboid_tapered([0.16, 0.16, 0.06], 0.0, glow(STATUS_GREEN, 1.6)),
            [-0.86, 1.5, z - 0.02],
            id_quat(),
        ),
    ];
    // Lit viewport bezel — thin strips, so the light reads as an edge seal
    // rather than a washed panel.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            cuboid_tapered([bez, PANEL + bez * 2.0, 0.12], 0.0, glow(VIEWPORT_LIT, 1.2)),
            [sx * (PANEL + bez) * 0.5, PANEL_Y, z - 0.03],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        out.push(prim(
            cuboid_tapered([PANEL, bez, 0.12], 0.0, glow(VIEWPORT_LIT, 1.2)),
            [0.0, PANEL_Y + sy * (PANEL + bez) * 0.5, z - 0.03],
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
            &SpaceOutpostMonument.build("did:plc:test"),
            "space_outpost_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&SpaceOutpostMonument, "did:plc:test");
    }
}
