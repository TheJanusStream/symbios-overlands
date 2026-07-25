//! Owner Holo-Pylon — the Cyberpunk identity monument (#975).
//!
//! The ident pylon bolted outside every block: a dark-metal monolith on a
//! cracked concrete pad, the room owner's portrait as its screen behind a
//! neon-edged bezel, a magenta underglow, a grille vent and a bundle of
//! conduit running up the back.
//!
//! Neon is deep-saturated at low strength here, deliberately: a broad panel at
//! strength blooms to white, and a pylon whose whole face has washed out is
//! the one thing this monument must never do.
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

use super::{DARK_METAL, NEON_CYAN, NEON_MAGENTA, RUST_BROWN, concrete, metal, rust};

const PANEL: f32 = 2.0;
const PANEL_Y: f32 = 3.4;
/// Blank tint — a dead display, which in this theme is a *reading*, not a
/// fault: half the screens in a cyberpunk street are out.
const BLANK: [f32; 3] = [0.10, 0.11, 0.14];

pub struct CyberpunkMonument;

impl CatalogueEntry for CyberpunkMonument {
    fn slug(&self) -> &'static str {
        "cyberpunk_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Holo-Pylon"
    }
    fn description(&self) -> &'static str {
        "Neon-edged ident pylon whose screen carries the room owner's portrait."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Cyberpunk]
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
        solid(cuboid_tapered(
            [3.0, 0.3, 1.6],
            0.06,
            concrete([0.30, 0.30, 0.32]),
        )),
        [0.0, 0.15, 0.0],
        id_quat(),
    );
    // The monolith. One clean mass, because the theme's noise belongs on it
    // rather than in its silhouette.
    let pylon = prim(
        solid(cuboid_tapered([2.6, 5.2, 0.6], 0.04, metal(DARK_METAL))),
        [0.0, 2.92, 0.0],
        id_quat(),
    );

    nest(pad, vec![nest(pylon, screen(did)), conduit(1.05)])
}

/// The screen: dark backing, portrait, neon bezel, underglow and a grille.
fn screen(did: &str) -> Vec<Generator> {
    let z = -0.29;
    let bez = 0.1;
    let mut out = vec![
        // Backing plate — the panel is single-sided and a pylon is opaque.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.3, PANEL + 0.3, 0.08],
                0.0,
                metal([0.12, 0.13, 0.15]),
            )),
            [0.0, PANEL_Y, z + 0.06],
            id_quat(),
        ),
        prim(
            pfp_panel(did, PANEL, BLANK),
            [0.0, PANEL_Y, z],
            quat_x(-FRAC_PI_2),
        ),
        // Magenta underglow washing the pad.
        prim(
            cuboid_tapered([PANEL + 0.4, 0.07, 0.08], 0.0, glow(NEON_MAGENTA, 1.4)),
            [0.0, PANEL_Y - PANEL * 0.5 - 0.34, z - 0.05],
            id_quat(),
        ),
        // Ventilation grille below the screen, and a rust streak under it —
        // the pylon has been out here a while.
        prim(
            solid(cuboid_tapered(
                [1.5, 0.5, 0.1],
                0.0,
                metal([0.18, 0.19, 0.21]),
            )),
            [0.0, 1.15, z - 0.02],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered([0.3, 1.0, 0.06], 0.0, rust(RUST_BROWN))),
            [-0.85, 0.75, z - 0.02],
            id_quat(),
        ),
    ];
    // Cyan neon bezel: four thin strips, not a slab, so the light is an edge
    // rather than a wash.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            cuboid_tapered([bez, PANEL + bez * 2.0, 0.1], 0.0, glow(NEON_CYAN, 1.3)),
            [sx * (PANEL + bez) * 0.5, PANEL_Y, z - 0.03],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        out.push(prim(
            cuboid_tapered([PANEL, bez, 0.1], 0.0, glow(NEON_CYAN, 1.3)),
            [0.0, PANEL_Y + sy * (PANEL + bez) * 0.5, z - 0.03],
            id_quat(),
        ));
    }
    out
}

/// A bundle of conduit up the pylon's flank, terminating in a junction box.
fn conduit(x: f32) -> Generator {
    let trunk = prim(
        solid(cylinder_tapered(
            0.09,
            4.6,
            8,
            0.0,
            metal([0.2, 0.21, 0.23]),
        )),
        [x + 0.22, 2.7, 0.36],
        id_quat(),
    );
    nest(
        trunk,
        vec![prim(
            solid(cuboid_tapered([0.34, 0.4, 0.26], 0.05, rust(RUST_BROWN))),
            [x + 0.22, 1.1, 0.36],
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
            &CyberpunkMonument.build("did:plc:test"),
            "cyberpunk_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&CyberpunkMonument, "did:plc:test");
    }
}
