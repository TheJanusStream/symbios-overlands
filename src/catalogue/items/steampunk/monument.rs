//! Owner Daguerreotype — the Steampunk identity monument (#975).
//!
//! A brass-cased portrait on a riveted iron pedestal: the room owner's picture
//! sits behind a heavy brass bezel with a cog turning at each side, copper
//! pipework running up the flanks and a gas lamp on a swan-neck bracket over
//! the case.
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
    BRASS, BRICK_SOOT, COPPER_ORANGE, IRON_DARK, LAMP_GAS, brass, brick, cog, copper, iron,
};

const PANEL: f32 = 1.7;
const PANEL_Y: f32 = 3.15;

pub struct SteampunkMonument;

impl CatalogueEntry for SteampunkMonument {
    fn slug(&self) -> &'static str {
        "steampunk_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Daguerreotype"
    }
    fn description(&self) -> &'static str {
        "Brass-cased portrait on a riveted pedestal, flanked by turning cogs."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Steampunk]
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
    // Sooted brick footing — the root, and flat.
    let footing = prim(
        solid(cuboid_tapered([3.0, 0.44, 1.7], 0.06, brick(BRICK_SOOT))),
        [0.0, 0.22, 0.0],
        id_quat(),
    );
    // Riveted iron pedestal.
    let pedestal = prim(
        solid(cuboid_tapered([2.3, 2.1, 1.1], 0.09, iron(IRON_DARK))),
        [0.0, 1.49, 0.0],
        id_quat(),
    );
    // The case the portrait lives in.
    let case = prim(
        solid(cuboid_tapered([2.4, 2.5, 0.7], 0.04, brass(BRASS))),
        [0.0, 3.3, 0.0],
        id_quat(),
    );

    nest(
        footing,
        vec![nest(
            pedestal,
            vec![nest(case, portrait(did)), pipe(-1.05), pipe(1.05)],
        )],
    )
}

/// The portrait: a dark backing, the plate, a heavy brass bezel, a cog at each
/// side, and the gas lamp over the case.
fn portrait(did: &str) -> Vec<Generator> {
    let z = -0.40;
    let bez = 0.17;
    let mut out = vec![
        // Backing — the panel is single-sided and a case has a back to it.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.2, PANEL + 0.2, 0.09],
                0.0,
                iron([0.16, 0.15, 0.14]),
            )),
            [0.0, PANEL_Y, z + 0.06],
            id_quat(),
        ),
        pfp_panel(did, PANEL, [0.0, PANEL_Y, z]),
        // Brass name plate under the case.
        prim(
            solid(cuboid_tapered([1.3, 0.26, 0.09], 0.0, brass(BRASS))),
            [0.0, PANEL_Y - PANEL * 0.5 - 0.34, z - 0.02],
            id_quat(),
        ),
        // Gas lamp on a swan-neck bracket. The plate is unlit and reads on its
        // own; this is what keeps the brass and iron alive after dark.
        prim(
            solid(cylinder_tapered(0.05, 0.7, 8, 0.0, brass(BRASS))),
            [0.0, 4.72, z + 0.1],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered([0.34, 0.36, 0.34], 0.35, brass(BRASS))),
            [0.0, 5.12, z - 0.08],
            id_quat(),
        ),
        prim(
            cuboid_tapered([0.2, 0.22, 0.2], 0.2, glow(LAMP_GAS, 2.0)),
            [0.0, 5.1, z - 0.08],
            id_quat(),
        ),
    ];
    // Brass bezel round the plate.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [bez, PANEL + bez * 2.0, 0.16],
                0.0,
                brass(BRASS),
            )),
            [sx * (PANEL + bez) * 0.5, PANEL_Y, z - 0.04],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered([PANEL, bez, 0.16], 0.0, brass(BRASS))),
            [0.0, PANEL_Y + sy * (PANEL + bez) * 0.5, z - 0.04],
            id_quat(),
        ));
    }
    // A cog either side of the case, standing in its own plane.
    for sx in [-1.0_f32, 1.0] {
        out.push(cog(
            [sx * 1.34, PANEL_Y - 0.3, z + 0.1],
            quat_x(FRAC_PI_2),
            0.34,
            0.12,
            10,
            copper(COPPER_ORANGE),
            brass(BRASS),
        ));
    }
    out
}

/// Copper pipework up a flank of the pedestal, capped with a union.
fn pipe(x: f32) -> Generator {
    let run = prim(
        solid(cylinder_tapered(0.08, 2.2, 10, 0.0, copper(COPPER_ORANGE))),
        [x, 2.6, 0.44],
        id_quat(),
    );
    nest(
        run,
        vec![prim(
            solid(cylinder_tapered(0.12, 0.16, 10, 0.0, brass(BRASS))),
            [x, 3.62, 0.44],
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
            &SteampunkMonument.build("did:plc:test"),
            "steampunk_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&SteampunkMonument, "did:plc:test");
    }
}
