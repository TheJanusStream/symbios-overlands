//! Owner Stela — the Mesoamerican identity monument (#975).
//!
//! A carved stela on a stepped limestone platform: the room owner's portrait
//! is the ruler-panel, sunk into the shaft behind a jade border under a gold
//! glyph band, with a feathered crest above and a fire bowl at the foot. The
//! stone a city raises at its approach to say who holds it.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, glow, id_quat, nest, pfp_panel, prim, quat_x, quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    FIRE_ORANGE, GOLD_WARM, JADE_GREEN, LIMESTONE_PALE, STUCCO_RED, gold, jade, limestone, painted,
};

const PANEL: f32 = 1.7;
const PANEL_Y: f32 = 3.25;
/// Blank tint — the red cinnabar a stela's field is washed with, so an empty
/// panel reads as prepared-but-uncarved stone.
const BLANK: [f32; 3] = [0.52, 0.24, 0.18];

pub struct MesoamericanMonument;

impl CatalogueEntry for MesoamericanMonument {
    fn slug(&self) -> &'static str {
        "mesoamerican_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Stela"
    }
    fn description(&self) -> &'static str {
        "Carved stela on a stepped platform, its ruler-panel bearing the room owner."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Mesoamerican]
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
            [4.0, 0.4, 2.6],
            0.05,
            limestone(LIMESTONE_PALE),
        )),
        [0.0, 0.2, 0.0],
        id_quat(),
    );
    let step1 = prim(
        solid(cuboid_tapered(
            [3.2, 0.38, 2.0],
            0.05,
            limestone(LIMESTONE_PALE),
        )),
        [0.0, 0.59, 0.0],
        id_quat(),
    );
    let shaft = prim(
        solid(cuboid_tapered(
            [2.4, 4.4, 0.75],
            0.1,
            limestone([0.74, 0.72, 0.64]),
        )),
        [0.0, 2.98, 0.05],
        id_quat(),
    );

    nest(
        step0,
        vec![nest(
            step1,
            vec![nest(shaft, carving(did)), brazier(-1.35), brazier(1.35)],
        )],
    )
}

/// The carved face: jade border, portrait, gold glyph band, feathered crest.
fn carving(did: &str) -> Vec<Generator> {
    let z = -0.31;
    let bar = 0.17;
    let mut out = vec![
        // Sunk field, and the backing the single-sided panel needs.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.14, PANEL + 0.14, 0.1],
                0.0,
                painted(STUCCO_RED),
            )),
            [0.0, PANEL_Y, z + 0.06],
            id_quat(),
        ),
        prim(
            pfp_panel(did, PANEL, BLANK),
            [0.0, PANEL_Y, z],
            quat_x(-FRAC_PI_2),
        ),
        // Gold glyph band under the portrait — the cartouche a name is cut
        // into, blank because there is no text renderer.
        prim(
            solid(cuboid_tapered([1.9, 0.34, 0.12], 0.0, gold(GOLD_WARM))),
            [0.0, PANEL_Y - PANEL * 0.5 - 0.42, z - 0.02],
            id_quat(),
        ),
        // Crest slab over the shaft.
        prim(
            solid(cuboid_tapered(
                [2.7, 0.36, 1.0],
                0.08,
                limestone(LIMESTONE_PALE),
            )),
            [0.0, 5.32, 0.05],
            id_quat(),
        ),
    ];
    // Jade border around the field.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [bar, PANEL + bar * 2.0, 0.13],
                0.0,
                jade(JADE_GREEN),
            )),
            [sx * (PANEL + bar) * 0.5, PANEL_Y, z - 0.03],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered([PANEL, bar, 0.13], 0.0, jade(JADE_GREEN))),
            [0.0, PANEL_Y + sy * (PANEL + bar) * 0.5, z - 0.03],
            id_quat(),
        ));
    }
    // Feather crest: five painted blades fanning off the crest slab. The tilt
    // is on each leaf, never on a parent that carries anything.
    for i in -2..=2 {
        let f = i as f32;
        out.push(prim(
            solid(cuboid_tapered([0.2, 1.1, 0.1], 0.55, painted(STUCCO_RED))),
            [f * 0.52, 6.0, 0.05],
            quat_z(-f * 0.24),
        ));
    }
    out
}

/// A stone fire bowl on the platform — for the limestone, which goes flat at
/// dusk; the portrait is unlit and reads on its own.
fn brazier(x: f32) -> Generator {
    let bowl = prim(
        solid(cylinder_tapered(
            0.38,
            0.5,
            10,
            0.45,
            limestone([0.66, 0.62, 0.54]),
        )),
        [x, 1.03, -0.75],
        id_quat(),
    );
    nest(
        bowl,
        vec![prim(
            cuboid_tapered([0.36, 0.28, 0.36], 0.6, glow(FIRE_ORANGE, 2.3)),
            [x, 1.32, -0.75],
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
            &MesoamericanMonument.build("did:plc:test"),
            "mesoamerican_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&MesoamericanMonument, "did:plc:test");
    }
}
