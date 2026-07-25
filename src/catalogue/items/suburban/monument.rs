//! Owner Yard Board — the Suburban identity monument (#975).
//!
//! The subdivision notice board: two white-painted posts on a brick footing
//! carry a shingled gable cap, and the room owner's portrait sits under it
//! behind white casing, with a coach lamp on one post and clipped hedge at the
//! foot. The board at the mouth of a street that says whose neighbourhood this
//! is — the same object the theme's gateway already speaks in.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use crate::catalogue::items::solarpunk::{crop_tufts, foliage};
use crate::catalogue::items::util::{
    self, cuboid_tapered, cuboid_tapered_xz, glow, id_quat, nest, pfp_panel, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::pds::generator::FaceKey;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRICK_TAN, HEDGE_GREEN, ROOF_GREY, WOOD_WHITE, brick, concrete, enamel, shingle, wood,
};

const PANEL: f32 = 1.8;
const PANEL_Y: f32 = 3.0;
/// Brick length in metres — a real 215 mm brick (#966).
const BRICK_LEN: f32 = 0.215;
/// Coach-lamp amber, deep-saturated at low strength so it reads as a colour
/// under bloom rather than washing to white.
const LAMP: [f32; 3] = [1.0, 0.62, 0.24];

pub struct SuburbanMonument;

impl CatalogueEntry for SuburbanMonument {
    fn slug(&self) -> &'static str {
        "suburban_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Yard Board"
    }
    fn description(&self) -> &'static str {
        "Shingle-capped notice board on brick piers, showing the room owner's portrait."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Suburban]
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
    let pad = prim(
        solid(cuboid_tapered(
            [3.6, 0.3, 1.5],
            0.03,
            concrete([0.60, 0.59, 0.57]),
        )),
        [0.0, 0.15, 0.0],
        id_quat(),
    );

    let mut parts = vec![nest(pier(-1.42), vec![]), nest(pier(1.42), vec![])];
    parts.push(nest(head(), board(did)));
    parts.push(coach_lamp(1.42));
    parts.extend(crop_tufts(
        [0.0, 0.3, -0.72],
        [2.6, 0.5],
        4,
        1,
        0.6,
        foliage(HEDGE_GREEN),
    ));
    nest(pad, parts)
}

/// A brick pier with a white coping cap.
///
/// The brick is laid flat at a real brick's size and bonded into the shared
/// world course frame ([`util::bonded_brick`]), so the two piers read as cut
/// from one wall rather than each restarting their own courses.
fn pier(x: f32) -> Generator {
    let at = [x, 1.35, 0.0];
    let shaft = prim(
        solid(cuboid_tapered(
            [0.5, 2.1, 0.5],
            0.0,
            util::bonded_brick(brick(BRICK_TAN), BRICK_LEN, FaceKey::SideNz, at),
        )),
        at,
        id_quat(),
    );
    nest(
        shaft,
        vec![prim(
            solid(cuboid_tapered([0.64, 0.14, 0.64], 0.06, wood(WOOD_WHITE))),
            [x, 2.47, 0.0],
            id_quat(),
        )],
    )
}

/// The head beam spanning the piers — and the parent of the board and the cap
/// it carries.
fn head() -> Generator {
    prim(
        solid(cuboid_tapered([3.5, 0.26, 0.3], 0.0, wood(WOOD_WHITE))),
        [0.0, 4.28, 0.0],
        id_quat(),
    )
}

/// The notice board: backing, portrait, casing and the shingled gable over it.
fn board(did: &str) -> Vec<Generator> {
    let z = -0.14;
    let case = 0.14;
    let mut out = vec![
        // Painted backing board — the panel is single-sided, and a notice
        // board is solid from behind.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.5, PANEL + 0.5, 0.09],
                0.0,
                wood([0.86, 0.85, 0.80]),
            )),
            [0.0, PANEL_Y, z + 0.06],
            id_quat(),
        ),
        pfp_panel(did, PANEL, [0.0, PANEL_Y, z]),
        // Gable cap: a ridge along X, so its triangle faces the approach.
        prim(
            solid(cuboid_tapered_xz(
                [3.7, 0.8, 1.3],
                [0.06, 0.92],
                shingle(ROOF_GREY),
            )),
            [0.0, 4.78, 0.0],
            id_quat(),
        ),
    ];
    // White casing round the portrait — proud, and never sized to meet the
    // backing exactly.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [case, PANEL + case * 2.0, 0.12],
                0.0,
                wood(WOOD_WHITE),
            )),
            [sx * (PANEL + case) * 0.5, PANEL_Y, z - 0.03],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered([PANEL, case, 0.12], 0.0, wood(WOOD_WHITE))),
            [0.0, PANEL_Y + sy * (PANEL + case) * 0.5, z - 0.03],
            id_quat(),
        ));
    }
    out
}

/// A black coach lamp on one pier — the kit's signature light, and what keeps
/// the paint and the shingle alive at dusk.
fn coach_lamp(x: f32) -> Generator {
    let housing = prim(
        solid(cuboid_tapered(
            [0.22, 0.3, 0.22],
            0.35,
            enamel([0.14, 0.14, 0.15]),
        )),
        [x, 2.78, -0.02],
        id_quat(),
    );
    nest(
        housing,
        vec![prim(
            cuboid_tapered([0.13, 0.17, 0.13], 0.2, glow(LAMP, 1.9)),
            [x, 2.78, -0.02],
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
        assert_sanitize_stable(&SuburbanMonument.build("did:plc:test"), "suburban_monument");
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&SuburbanMonument, "did:plc:test");
    }
}
