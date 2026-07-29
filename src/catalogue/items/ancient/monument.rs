//! Owner Herm — the Ancient/Classical identity monument (#975).
//!
//! A marble aedicula on a stepped krepis: two fluted columns carry an
//! architrave and a low pediment, and the room owner's portrait is set between
//! them behind a bronze surround, the way a votive panel sat in a shrine
//! niche. Two bronze braziers on the top step light the stone.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares — square panel, blank-legible frame, backing plate.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, footing, glow, id_quat, nest, pfp_panel, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRONZE_GREEN, EMBER_ORANGE, MARBLE_WHITE, SANDSTONE_GOLD, bronze, marble, sandstone};

const PANEL: f32 = 1.7;
const PANEL_Y: f32 = 3.05;

pub struct AncientMonument;

impl CatalogueEntry for AncientMonument {
    fn slug(&self) -> &'static str {
        "ancient_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Herm"
    }
    fn description(&self) -> &'static str {
        "Marble aedicula on a stepped krepis, the owner's portrait between its columns."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AncientClassical]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.8,
            min_spawn_dist: 8.0,
        }
    }
    fn build(&self, local_did: &str) -> Generator {
        build_tree(local_did)
    }
}

fn build_tree(did: &str) -> Generator {
    let step0 = prim(
        solid(cuboid_tapered(
            [4.0, 0.32, 2.6],
            0.0,
            sandstone(SANDSTONE_GOLD),
        )),
        [0.0, 0.16, 0.0],
        id_quat(),
    );
    let step1 = prim(
        solid(cuboid_tapered(
            [3.4, 0.3, 2.1],
            0.0,
            sandstone(SANDSTONE_GOLD),
        )),
        [0.0, 0.47, 0.0],
        id_quat(),
    );
    let dado = prim(
        solid(cuboid_tapered([2.7, 0.7, 1.5], 0.0, marble(MARBLE_WHITE))),
        [0.0, 0.97, 0.0],
        id_quat(),
    );

    let mut on_dado = vec![nest(back_wall(), portrait(did))];
    for sx in [-1.0_f32, 1.0] {
        on_dado.push(column(sx * 1.18));
        on_dado.push(brazier(sx * 1.62));
    }

    nest(
        step0,
        vec![
            nest(step1, vec![nest(dado, on_dado)]),
            // Buried footing under the bottom krepis step, so a terrain-snapped
            // herm shows plinth instead of daylight under its downhill edge.
            // `nest` rebases it out of the world frame like every other child.
            footing(4.0, 2.6, [0.0, 0.0], 2.8),
        ],
    )
}

/// The wall the portrait is fixed to — and the backing that stops the shrine
/// being see-through, since the panel is single-sided.
fn back_wall() -> Generator {
    prim(
        solid(cuboid_tapered([2.3, 3.3, 0.34], 0.0, marble(MARBLE_WHITE))),
        [0.0, 2.97, 0.22],
        id_quat(),
    )
}

/// Portrait, bronze surround, and the entablature and pediment over it.
fn portrait(did: &str) -> Vec<Generator> {
    let front = -0.08;
    let mut out = vec![
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.24, PANEL + 0.24, 0.1],
                0.0,
                bronze(BRONZE_GREEN),
            )),
            [0.0, PANEL_Y, front],
            id_quat(),
        ),
        pfp_panel(did, PANEL, [0.0, PANEL_Y, front - 0.07]),
        // Architrave across the columns, then the pediment on it.
        prim(
            solid(cuboid_tapered([3.0, 0.42, 1.0], 0.0, marble(MARBLE_WHITE))),
            [0.0, 4.68, 0.0],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered([2.9, 0.9, 0.95], 0.92, marble(MARBLE_WHITE))),
            [0.0, 5.34, 0.0],
            id_quat(),
        ),
    ];
    // Votive garland band under the portrait.
    out.push(prim(
        solid(cuboid_tapered([1.5, 0.22, 0.12], 0.0, bronze(BRONZE_GREEN))),
        [0.0, PANEL_Y - PANEL * 0.5 - 0.34, front - 0.02],
        id_quat(),
    ));
    out
}

/// A fluted column on the dado, with its own base and capital.
fn column(x: f32) -> Generator {
    let base = prim(
        solid(cuboid_tapered(
            [0.52, 0.18, 0.52],
            0.0,
            marble(MARBLE_WHITE),
        )),
        [x, 1.41, -0.42],
        id_quat(),
    );
    nest(
        base,
        vec![
            prim(
                solid(cylinder_tapered(0.2, 3.0, 14, 0.08, marble(MARBLE_WHITE))),
                [x, 3.0, -0.42],
                id_quat(),
            ),
            prim(
                solid(cuboid_tapered([0.5, 0.24, 0.5], 0.0, marble(MARBLE_WHITE))),
                [x, 4.62, -0.42],
                id_quat(),
            ),
        ],
    )
}

/// A bronze brazier on the top step — the panel is unlit and reads on its own,
/// so this is for the marble, which otherwise goes flat at dusk.
fn brazier(x: f32) -> Generator {
    let post = prim(
        solid(cylinder_tapered(0.12, 1.0, 10, 0.15, bronze(BRONZE_GREEN))),
        [x, 1.82, -0.95],
        id_quat(),
    );
    nest(
        post,
        vec![
            prim(
                solid(cylinder_tapered(0.34, 0.26, 12, 0.5, bronze(BRONZE_GREEN))),
                [x, 2.42, -0.95],
                id_quat(),
            ),
            prim(
                cuboid_tapered([0.34, 0.24, 0.34], 0.6, glow(EMBER_ORANGE, 2.2)),
                [x, 2.58, -0.95],
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
        assert_sanitize_stable(&AncientMonument.build("did:plc:test"), "ancient_monument");
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&AncientMonument, "did:plc:test");
    }
}
