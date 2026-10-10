//! Berlin's long block in the frontier theme's dress (#1598): the brick
//! bank-and-hotel block a railroad town raised once it had a kiln - long
//! runs of tall windows under sandstone lintels, cast-iron shopfronts in a
//! stone-trimmed ground storey, a sandstone sill course at every floor, and
//! a bracketed cornice under a parapet with a name board over each section.
//!
//! - **One block, one brick**, rolled once at the lot: red, buff or a dark
//!   brown; the sandstone trim, the iron and the signs name their own.
//! - **Sections of twelve metres**, as a commercial block is let: each one
//!   a doorway up a stone step to the stair, a column of windows over it,
//!   and a name board standing on the parapet over its axis.
//! - **Shops or offices on the street**: a trading block has cast-iron
//!   shopfronts on plinths under canvas awnings (`Pick`ed once); one of
//!   offices has tall windows over a sill.
//! - **Courses and a cornice**: a sandstone course over the ground storey,
//!   a sill course at every storey above it, and a corbelled cornice on a
//!   row of brackets the whole frontage, so a row of copies shows one
//!   unbroken line.
//! - **A flat roof** behind the parapet, its deck inside the parapet walls.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{CANVAS_TAN, IRON_DARK, STONE_TAN, brick, canvas, iron, lap_siding};

/// The block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "wild_west_street_block",
    name: "Frontier Bank Block",
    description: "A long brick bank-and-hotel block: tall windows under sandstone lintels, \
                  cast-iron shopfronts, string courses, and a bracketed cornice with a name \
                  board over each section.",
    themes: &[ThemeArchetype::WildWest],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // A lofty banking floor, then the hotel's and the offices' storeys.
    storey_m: (4.6, 3.5),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BB6_0002,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Cornice", "Fascia", "Canopy",
    ],
};

/// Railroad brick in three burns, sandstone dressings, cast iron, and the
/// gilt name boards and canvas awnings over them.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.30, 0.22, 0.14],
        panes: (2, 3),
        room: [1.0, 0.80, 0.46],
        shop: [1.0, 0.86, 0.58],
    });
    m.extend([
        ("BrickRed".to_string(), brick([0.55, 0.26, 0.18])),
        ("BrickBuff".to_string(), brick([0.74, 0.60, 0.42])),
        ("BrickBrown".to_string(), brick([0.40, 0.24, 0.17])),
        ("Stone".to_string(), canvas(STONE_TAN)),
        ("Iron".to_string(), iron(IRON_DARK)),
        ("Cornice".to_string(), canvas([0.30, 0.27, 0.24])),
        ("Door".to_string(), lap_siding([0.28, 0.18, 0.11])),
        ("Sign".to_string(), canvas([0.12, 0.16, 0.13])),
        ("Gilt".to_string(), canvas([0.82, 0.66, 0.30])),
        ("Awning".to_string(), canvas(CANVAS_TAN)),
        ("AwningRed".to_string(), canvas([0.56, 0.18, 0.14])),
        ("Deck".to_string(), canvas([0.22, 0.20, 0.19])),
    ]);
    m
}
