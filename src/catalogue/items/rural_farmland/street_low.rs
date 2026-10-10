//! Berlin's low building in the farm country's dress (#1598): where it
//! trades, a barn-built farm shop - red board-and-batten under a gambrel
//! roof, a big plank door flanked by produce windows, and a tin canopy on
//! posts over the stalls along its front; where it houses people, a row of
//! clapboard farm cottages, each a window and a door under a porch hood,
//! under one shingle roof.
//!
//! - **The farm shop**: one board colour rolled for it, a door where a bay
//!   is wide enough, a loft door in its second storey, and a gambrel along
//!   the street whose gable ends are the party walls.
//! - **The cottages**: as many as the frontage holds, every household its
//!   own door colour, a shingle gable along the street.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BARN_RED, CLAPBOARD_CREAM, LAMP_WARM, ROOF_GREY, STONE_GREY, TRACTOR_GREEN, TRIM_WHITE,
    WOOD_GREY, barn_board, clapboard, metal_roof, shingle, stone, weathered,
};

/// The low building (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "rural_farmland_street_low",
    name: "Barn Farm Shop",
    description: "A barn-built farm shop of red boards under a gambrel roof, a big plank door \
                  and produce stalls under a tin canopy - or, where nothing trades, a row of \
                  clapboard farm cottages.",
    themes: &[ThemeArchetype::RuralFarmland],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // A barn's high ground storey, and a cottage's upper one.
    storey_m: (3.8, 2.8),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BA8_0003,
    materials,
    round_meshes: &[],
    solid_meshes: &["Wall", "Door", "Pane", "Roof", "Trim", "Canopy", "Post"],
};

/// Barn boards and clapboard, white trim, the doors, and the metal and
/// shingle over them.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: TRIM_WHITE,
        panes: (2, 2),
        room: LAMP_WARM,
        shop: [1.0, 0.88, 0.62],
    });
    m.extend([
        ("BarnRed".to_string(), barn_board(BARN_RED)),
        ("BarnGrey".to_string(), weathered(WOOD_GREY)),
        ("Cream".to_string(), clapboard(CLAPBOARD_CREAM)),
        ("White".to_string(), clapboard(TRIM_WHITE)),
        ("Sage".to_string(), clapboard([0.62, 0.70, 0.58])),
        ("Trim".to_string(), clapboard(TRIM_WHITE)),
        ("Stone".to_string(), stone(STONE_GREY)),
        ("Door".to_string(), barn_board([0.36, 0.24, 0.15])),
        ("DoorGreen".to_string(), clapboard(TRACTOR_GREEN)),
        ("DoorRed".to_string(), clapboard([0.56, 0.16, 0.12])),
        ("Post".to_string(), weathered(WOOD_GREY)),
        ("Shingle".to_string(), shingle(ROOF_GREY)),
        ("MetalRoof".to_string(), metal_roof(ROOF_GREY)),
    ]);
    m
}
