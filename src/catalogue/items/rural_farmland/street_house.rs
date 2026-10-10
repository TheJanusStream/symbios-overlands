//! Berlin's street house in the farm country's dress (#1598): the tall
//! brick house of a market town, built as a farmhouse is - its gable to
//! the street under a half-hipped roof, a loft door and a hoist beam high
//! in the gable, red brick storeys of white-framed windows under brick
//! arches and green shutters, on a fieldstone plinth, and a feed store or
//! a farm shop in its ground floor where the house trades.
//!
//! - **One house, one brick**, rolled once at the lot and inherited by
//!   every wall below it that names no material of its own; the plinth,
//!   the trim, the shutters, the loft door and the roof name theirs.
//! - **Shops or a home.** A trading house has shop bays either side of its
//!   entrance under a tin canopy, each a window over the plinth with a door
//!   where the bay is wide enough; a house of homes has tall windows.
//! - **Arches and shutters**: a soldier-brick arch over every front window
//!   and a shutter either side of it (`Pick`ed once a house: shuttered or
//!   not).
//! - **The roof is half-hipped** with its gable to the street, shingle or
//!   ribbed metal (`Pick`ed once), so a row of copies stands gable by
//!   gable, a valley between each two. The gable carries the loft door and
//!   the hoist beam over the street.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BARN_RED, LAMP_WARM, ROOF_GREY, STONE_GREY, TRACTOR_GREEN, TRIM_WHITE, WOOD_GREY, barn_board,
    brick, clapboard, enamel, metal_roof, shingle, stone,
};

/// The house (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "rural_farmland_street_house",
    name: "Brick Farmhouse",
    description: "A tall brick market-town house built like a farmhouse: its gable to the \
                  street under a half-hipped roof, a loft door and hoist beam in the gable, \
                  arched and shuttered windows, and a farm shop below.",
    themes: &[ThemeArchetype::RuralFarmland],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // The shop's or the kitchen's high ground storey, and the bedrooms'.
    storey_m: (4.0, 3.3),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BA8_0001,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Canopy", "Shutter", "Beam", "Fascia",
    ],
};

/// Market-town brick in three burns, fieldstone, white trim and green
/// shutters, the loft's boards, and the roofs over them.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: TRIM_WHITE,
        panes: (2, 3),
        room: LAMP_WARM,
        shop: [1.0, 0.88, 0.62],
    });
    m.extend([
        ("BrickRed".to_string(), brick([0.56, 0.24, 0.17])),
        ("BrickDark".to_string(), brick([0.42, 0.20, 0.15])),
        ("BrickBuff".to_string(), brick([0.72, 0.56, 0.38])),
        ("Arch".to_string(), brick([0.36, 0.16, 0.12])),
        ("Stone".to_string(), stone(STONE_GREY)),
        ("Trim".to_string(), clapboard(TRIM_WHITE)),
        ("Shutter".to_string(), clapboard(TRACTOR_GREEN)),
        ("Door".to_string(), clapboard([0.30, 0.20, 0.13])),
        ("Loft".to_string(), barn_board(BARN_RED)),
        ("Beam".to_string(), barn_board(WOOD_GREY)),
        ("Shingle".to_string(), shingle(ROOF_GREY)),
        ("MetalRoof".to_string(), metal_roof([0.46, 0.20, 0.16])),
        ("Canopy".to_string(), metal_roof(ROOF_GREY)),
        ("Sign".to_string(), enamel([0.20, 0.32, 0.20])),
    ]);
    m
}
