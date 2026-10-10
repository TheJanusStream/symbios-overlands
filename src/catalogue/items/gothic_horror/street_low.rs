//! Berlin's low building in the gothic horror theme's dress (#1598): where
//! it trades, an undertaker's parlour - a black shopfront of tall windows
//! under pointed hoods and a deep fascia, a steep slate roof along the
//! street; where it houses people, an almshouse row - stone cottages each
//! under its own steep gable turned to the street, a pointed door and
//! window to each.
//!
//! - **The almshouse row**: as many cottages as the frontage holds, each
//!   its own gable, its door under a pointed hood and its stone rolled once
//!   for the row, a window in its gable storey where it has two.
//! - **The parlour**: shop windows in bays either side of a door, a black
//!   fascia the whole front, a row of pointed windows over it where it has
//!   two storeys, a pointed gablet over the door, and iron cresting along
//!   the ridge.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    IRON_BLACK, STAINED_TINT, STONE_DARK, STONE_MOSS, brick, iron, matte, mossy, slate, stained,
    stone, wood,
};

/// The low building (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "gothic_horror_street_low",
    name: "Undertaker and Almshouses",
    description: "A gothic undertaker's parlour with a black fascia and pointed windows, or an \
                  almshouse row of stone cottages each under its own steep gable.",
    themes: &[ThemeArchetype::GothicHorror],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // The parlour's tall ground storey, and the rooms over it.
    storey_m: (3.8, 3.0),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BB2_0003,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Cornice", "Fascia", "Iron",
    ],
};

/// Soot-dark brick and stone, moss, pale dressings, slate, black iron, the
/// doors and fascias, and a little stained glass.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.16, 0.15, 0.14],
        panes: (2, 4),
        room: [0.95, 0.66, 0.34],
        shop: [0.92, 0.74, 0.46],
    });
    m.extend([
        ("Brick".to_string(), brick([0.36, 0.19, 0.15])),
        ("BlackBrick".to_string(), brick([0.22, 0.20, 0.20])),
        ("Stone".to_string(), stone(STONE_DARK)),
        ("PaleStone".to_string(), stone([0.58, 0.57, 0.54])),
        ("Mossy".to_string(), mossy(STONE_MOSS)),
        ("Dressing".to_string(), stone([0.66, 0.64, 0.58])),
        ("Slate".to_string(), slate([0.20, 0.21, 0.24])),
        ("Iron".to_string(), iron(IRON_BLACK)),
        ("Door".to_string(), wood([0.26, 0.10, 0.10])),
        ("DoorBlack".to_string(), wood([0.14, 0.13, 0.13])),
        ("Fascia".to_string(), matte([0.06, 0.06, 0.07])),
        ("Stained".to_string(), stained(STAINED_TINT, 1.2)),
    ]);
    m
}
