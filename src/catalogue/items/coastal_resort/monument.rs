//! Owner Boardwalk Sign — the Coastal-Resort identity monument (#975).
//!
//! The painted board at the head of a pier: two creosoted pilings on a
//! concrete kerb carry a white-framed sign, the room owner's portrait fills
//! it, a striped canvas valance shades it and a life ring and a string lamp
//! hang off the frame.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, glow, id_quat, nest, pfp_panel, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    AWNING_RED, AWNING_WHITE, BUOY_RED, DECK_PALE, LAMP_WARM, PILING_GREY, canvas, concrete,
    enamel, plank,
};

const PANEL: f32 = 1.9;
const PANEL_Y: f32 = 3.05;

pub struct CoastalResortMonument;

impl CatalogueEntry for CoastalResortMonument {
    fn slug(&self) -> &'static str {
        "coastal_resort_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Boardwalk Sign"
    }
    fn description(&self) -> &'static str {
        "Piling-mounted boardwalk sign under a striped valance, showing the room owner."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CoastalResort]
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
    let kerb = prim(
        solid(cuboid_tapered(
            [3.6, 0.3, 1.4],
            0.06,
            concrete([0.74, 0.72, 0.66]),
        )),
        [0.0, 0.15, 0.0],
        id_quat(),
    );

    let mut parts = Vec::new();
    for sx in [-1.0_f32, 1.0] {
        parts.push(piling(sx * 1.4));
    }
    parts.push(nest(head_rail(), sign(did)));
    parts.push(life_ring(-1.4));
    parts.push(string_lamp(1.4));
    nest(kerb, parts)
}

/// A round creosoted piling — round, because a resort pier's are, and a square
/// post would read as a fence.
fn piling(x: f32) -> Generator {
    let pile = prim(
        solid(cylinder_tapered(0.17, 4.4, 12, 0.07, plank(PILING_GREY))),
        [x, 2.5, 0.0],
        id_quat(),
    );
    nest(
        pile,
        vec![prim(
            solid(cylinder_tapered(
                0.2,
                0.12,
                12,
                0.3,
                enamel([0.86, 0.86, 0.84]),
            )),
            [x, 4.74, 0.0],
            id_quat(),
        )],
    )
}

/// The head rail across the pilings, and everything it carries.
fn head_rail() -> Generator {
    prim(
        solid(cuboid_tapered([3.4, 0.24, 0.26], 0.0, plank(DECK_PALE))),
        [0.0, 4.5, 0.0],
        id_quat(),
    )
}

/// The sign board: plank backing, portrait, white frame, striped valance.
fn sign(did: &str) -> Vec<Generator> {
    let z = -0.15;
    let fr = 0.15;
    let mut out = vec![
        // Plank backing — the panel is single-sided, and a boardwalk sign is
        // a solid board seen from both sides of the pier.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.5, PANEL + 0.5, 0.1],
                0.0,
                plank(DECK_PALE),
            )),
            [0.0, PANEL_Y, z + 0.07],
            id_quat(),
        ),
        pfp_panel(did, PANEL, [0.0, PANEL_Y, z]),
    ];
    // Striped canvas valance over the board, on the head rail.
    for (i, x) in [-1.2_f32, -0.4, 0.4, 1.2].iter().enumerate() {
        let (a, b) = if i % 2 == 0 {
            (AWNING_RED, AWNING_WHITE)
        } else {
            (AWNING_WHITE, AWNING_RED)
        };
        out.push(prim(
            solid(cuboid_tapered([0.8, 0.42, 0.7], 0.0, canvas(a, b))),
            [*x, 4.28, z - 0.35],
            id_quat(),
        ));
    }
    // White signwriter's frame.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [fr, PANEL + fr * 2.0, 0.13],
                0.0,
                enamel(AWNING_WHITE),
            )),
            [sx * (PANEL + fr) * 0.5, PANEL_Y, z - 0.03],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered([PANEL, fr, 0.13], 0.0, enamel(AWNING_WHITE))),
            [0.0, PANEL_Y + sy * (PANEL + fr) * 0.5, z - 0.03],
            id_quat(),
        ));
    }
    out
}

/// A life ring hung on one piling — the prop that says seaside at a glance.
fn life_ring(x: f32) -> Generator {
    prim(
        solid(torus(0.09, 0.32, enamel(BUOY_RED))),
        [x, 1.6, -0.28],
        quat_x(FRAC_PI_2),
    )
}

/// A festoon lamp on the other piling. The board is unlit and reads on its
/// own; this is what keeps the timber and canvas alive at dusk.
fn string_lamp(x: f32) -> Generator {
    let bracket = prim(
        solid(cuboid_tapered(
            [0.34, 0.07, 0.07],
            0.0,
            enamel([0.9, 0.9, 0.88]),
        )),
        [x - 0.2, 3.9, -0.2],
        id_quat(),
    );
    nest(
        bracket,
        vec![prim(
            solid(cylinder_tapered(0.15, 0.28, 10, -0.3, glow(LAMP_WARM, 1.7))),
            [x - 0.38, 3.78, -0.2],
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
            &CoastalResortMonument.build("did:plc:test"),
            "coastal_resort_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&CoastalResortMonument, "did:plc:test");
    }
}
