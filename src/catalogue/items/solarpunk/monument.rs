//! Owner Living Frame — the Solarpunk identity monument (#975).
//!
//! A warm-timber frame with a photovoltaic canopy: two posts on a pale
//! concrete kerb carry the room owner's portrait in a planed timber surround,
//! a PV wing tilts over it and a planted trough runs along the foot, with a
//! low warm lamp under the canopy. The panel that shades it is also, in this
//! theme, the panel that powers the lamp — which is the whole argument the
//! theme makes.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, glow, id_quat, nest, pfp_panel, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_PALE, CROP_GREEN, LAMP_WARM, LEAF_GREEN, PV_BLUE, SOIL_DARK, STEEL_WHITE, TIMBER_WARM,
    concrete, crop_tufts, foliage, pv, steel, timber,
};

const PANEL: f32 = 1.8;
const PANEL_Y: f32 = 3.05;

pub struct SolarpunkMonument;

impl CatalogueEntry for SolarpunkMonument {
    fn slug(&self) -> &'static str {
        "solarpunk_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Living Frame"
    }
    fn description(&self) -> &'static str {
        "Timber frame under a PV canopy, holding the room owner's portrait over a planter."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Solarpunk]
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
    let kerb = prim(
        solid(cuboid_tapered(
            [3.6, 0.3, 1.6],
            0.05,
            concrete(CONCRETE_PALE),
        )),
        [0.0, 0.15, 0.0],
        id_quat(),
    );

    let mut parts = Vec::new();
    for sx in [-1.0_f32, 1.0] {
        parts.push(post(sx * 1.38));
    }
    parts.push(nest(head(), frame(did)));
    parts.push(planter());
    parts.extend(crop_tufts(
        [0.0, 0.72, -0.55],
        [2.4, 0.35],
        5,
        1,
        0.5,
        foliage(CROP_GREEN),
    ));
    nest(kerb, parts)
}

/// A planed timber post on a slim steel shoe, so the timber never sits in wet
/// soil — the detail the theme's own buildings all use.
fn post(x: f32) -> Generator {
    let shoe = prim(
        solid(cuboid_tapered([0.2, 0.22, 0.2], 0.1, steel(STEEL_WHITE))),
        [x, 0.41, 0.0],
        id_quat(),
    );
    nest(
        shoe,
        vec![prim(
            solid(cuboid_tapered([0.22, 4.1, 0.22], 0.05, timber(TIMBER_WARM))),
            [x, 2.57, 0.0],
            id_quat(),
        )],
    )
}

/// The head rail and everything above it.
fn head() -> Generator {
    prim(
        solid(cuboid_tapered([3.4, 0.22, 0.26], 0.0, timber(TIMBER_WARM))),
        [0.0, 4.4, 0.0],
        id_quat(),
    )
}

/// The portrait frame, the PV canopy over it, and the lamp under the canopy.
fn frame(did: &str) -> Vec<Generator> {
    let z = -0.15;
    let fr = 0.15;
    let mut out = vec![
        // Lime-plaster backing board — the panel is single-sided.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.4, PANEL + 0.4, 0.09],
                0.0,
                concrete([0.84, 0.82, 0.76]),
            )),
            [0.0, PANEL_Y, z + 0.06],
            id_quat(),
        ),
        pfp_panel(did, PANEL, [0.0, PANEL_Y, z]),
        // PV wing, tilted to catch the sun rather than lying flat — the tilt
        // is on a leaf, so it spins nothing.
        prim(
            solid(cuboid_tapered([3.5, 0.09, 1.5], 0.0, pv(PV_BLUE))),
            [0.0, 4.72, -0.4],
            quat_x(-0.26),
        ),
        // Warm lamp under the canopy. The portrait is unlit and reads on its
        // own; this is what keeps the timber and greenery alive at dusk.
        prim(
            cuboid_tapered([0.9, 0.07, 0.14], 0.0, glow(LAMP_WARM, 1.5)),
            [0.0, 4.3, z - 0.3],
            id_quat(),
        ),
    ];
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [fr, PANEL + fr * 2.0, 0.13],
                0.0,
                timber(TIMBER_WARM),
            )),
            [sx * (PANEL + fr) * 0.5, PANEL_Y, z - 0.03],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered([PANEL, fr, 0.13], 0.0, timber(TIMBER_WARM))),
            [0.0, PANEL_Y + sy * (PANEL + fr) * 0.5, z - 0.03],
            id_quat(),
        ));
    }
    out
}

/// The planted trough along the foot: a timber box with soil in it, so the
/// tufts above are growing in something.
fn planter() -> Generator {
    let box_ = prim(
        solid(cuboid_tapered([2.8, 0.5, 0.6], 0.04, timber(TIMBER_WARM))),
        [0.0, 0.55, -0.55],
        id_quat(),
    );
    nest(
        box_,
        vec![
            prim(
                solid(cuboid_tapered([2.62, 0.1, 0.46], 0.0, concrete(SOIL_DARK))),
                [0.0, 0.74, -0.55],
                id_quat(),
            ),
            prim(
                solid(cylinder_tapered(0.06, 0.34, 8, 0.1, foliage(LEAF_GREEN))),
                [1.1, 0.92, -0.55],
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
        assert_sanitize_stable(
            &SolarpunkMonument.build("did:plc:test"),
            "solarpunk_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&SolarpunkMonument, "did:plc:test");
    }
}
