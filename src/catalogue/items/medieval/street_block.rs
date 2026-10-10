//! Berlin's long block in the medieval theme's dress (#1598): a guild hall
//! or a merchants' warehouse of a Hanseatic town - an arcaded stone ground
//! storey, coursed storeys of paired windows over it, a crenellated parapet
//! and a steep roof behind it with a hoist gable over each section.
//!
//! - **One hall, one stone**, rolled once at the lot: pale ashlar, grey
//!   ashlar or rubble.
//! - **Sections of twelve metres**, as the merchants' bays are: each with a
//!   hall door on the street under a guild banner, a column of loading
//!   doors over it with a hoist beam at its head, and a hoist gable turned
//!   to the street on the roof.
//! - **The arcade or the windows**: a trading hall opens its ground storey
//!   in market bays between heavy piers; one of homes has small windows set
//!   high in the stone.
//! - **A crenellated parapet** the whole frontage, and a steep tiled or
//!   slated roof along the street behind it.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    HERALD_GOLD, HERALD_RED, SLATE_GREY, STONE_GREY, STONE_PALE, WOOD_OAK, cloth, panelling,
    rough_stone, shingle, stone, timber,
};

/// The long block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "medieval_street_block",
    name: "Guild Hall",
    description: "A medieval guild hall or merchants' warehouse: an arcaded stone ground storey, \
                  paired windows, stacked loading doors under hoist gables, and a crenellated \
                  parapet.",
    themes: &[ThemeArchetype::Medieval],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // The market hall's tall ground storey, and the store floors over it.
    storey_m: (4.4, 3.2),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BA2_0002,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Timber", "Shutter", "Banner",
    ],
};

/// The hall's ashlar and rubble, its dressings, oak doors and shutters,
/// guild banners and the roofs over them.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.24, 0.17, 0.10],
        panes: (3, 3),
        room: [1.0, 0.66, 0.34],
        shop: [1.0, 0.74, 0.42],
    });
    m.extend([
        ("Oak".to_string(), timber(WOOD_OAK)),
        ("Stone".to_string(), stone(STONE_GREY)),
        ("StonePale".to_string(), stone(STONE_PALE)),
        ("Rubble".to_string(), rough_stone(STONE_GREY)),
        ("Dressing".to_string(), stone([0.70, 0.66, 0.58])),
        ("Door".to_string(), panelling([0.34, 0.22, 0.12])),
        ("Shutter".to_string(), panelling([0.26, 0.34, 0.22])),
        ("Tile".to_string(), shingle([0.52, 0.24, 0.16])),
        ("Slate".to_string(), shingle(SLATE_GREY)),
        ("Banner".to_string(), cloth(HERALD_RED, HERALD_GOLD)),
    ]);
    m
}
