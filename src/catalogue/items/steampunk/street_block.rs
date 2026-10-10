//! Berlin's long block in the steampunk theme's dress (#1598): a foundry
//! block - a works of sooty brick between piers, great iron-framed
//! windows, riveted bands at every floor, a sawtooth of northlights on its
//! roof and a round brick chimney stack over every section, its mouth
//! aglow.
//!
//! - **One works, one brick**, rolled once at the lot; the plinth is dark
//!   engineering brick whatever the walls are.
//! - **Sections of twelve metres**, each an iron door under an iron canopy
//!   with a column of stair lights over it, and either side a run of bays
//!   between brick piers.
//! - **Works windows**: every bay a wide iron-framed window of many panes
//!   under an iron lintel with a brass keystone; a riveted iron band at
//!   every floor, and a copper downpipe at each end of the front.
//! - **Showrooms or offices on the street**: a trading works shows its
//!   wares in iron-framed showrooms; one of homes has windows.
//! - **The crown**: an iron-coped parapet, behind it a sawtooth of sheds
//!   with amber northlights, and a stack to each section.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRASS, BRICK_SOOT, COPPER_ORANGE, CORRUGATED_RUST, FURNACE_ORANGE, GLASS_AMBER, IRON_DARK,
    LAMP_GAS, brass, brick, copper, corrugated, glass, iron,
};

/// The block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "steampunk_street_block",
    name: "Foundry Block",
    description: "A long foundry works: sooty brick between piers, great iron-framed windows, \
                  riveted bands, a sawtooth of amber northlights and a glowing chimney stack \
                  over every section.",
    themes: &[ThemeArchetype::Steampunk],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // A works' tall ground storey, then its floors.
    storey_m: (4.2, 3.3),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BAE_0002,
    materials,
    round_meshes: &["Stack", "Pipe"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Band", "Pier", "Fascia", "Stack", "Pipe", "Canopy",
    ],
};

/// Sooty brick, engineering brick, riveted iron, brass, copper, rusty
/// sheeting, amber northlights and the furnaces' glow.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.16, 0.16, 0.16],
        panes: (4, 4),
        room: [1.0, 0.66, 0.34],
        shop: LAMP_GAS,
    });
    m.extend([
        ("BrickSoot".to_string(), brick(BRICK_SOOT)),
        ("BrickRed".to_string(), brick([0.52, 0.24, 0.18])),
        ("BrickBrown".to_string(), brick([0.40, 0.30, 0.22])),
        ("Plinth".to_string(), brick([0.24, 0.19, 0.17])),
        ("Iron".to_string(), iron(IRON_DARK)),
        ("Brass".to_string(), brass(BRASS)),
        ("Copper".to_string(), copper(COPPER_ORANGE)),
        ("Sheet".to_string(), corrugated(CORRUGATED_RUST)),
        ("NorthLight".to_string(), glass(GLASS_AMBER, 0.6)),
        ("Door".to_string(), iron([0.28, 0.26, 0.24])),
        ("Sign".to_string(), iron([0.10, 0.20, 0.16])),
        (
            "Furnace".to_string(),
            crate::catalogue::items::util::glow(FURNACE_ORANGE, 2.6),
        ),
        ("Deck".to_string(), iron([0.18, 0.18, 0.18])),
    ]);
    m
}
