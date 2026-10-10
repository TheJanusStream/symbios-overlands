//! Berlin's street house in the gothic horror theme's dress (#1598): a
//! Victorian gothic town house - soot-black brick or dark stone, tall
//! windows under pointed hoods, an oriel stacked up one side, a steep
//! slate roof with iron cresting along its ridge and a pointed gable over
//! the oriel.
//!
//! - **One house, one masonry**, rolled once at the lot; its pale
//!   dressings (sills, pointed hoods, string courses, the cornice) name
//!   their own.
//! - **Pointed windows**: each a tall window under a little gable of
//!   dressed stone, so a front reads as a row of lancets.
//! - **A shop or a stoop.** A trading house (`Trade`) has a black-fascia
//!   shopfront either side of its door, the undertaker's and the
//!   apothecary's; a house of homes has its door up a flight of steps and
//!   pointed windows over a high sill.
//! - **The oriel** (`Pick`ed once) stands out of the end bay at every upper
//!   storey under a steep pointed gable on the roof; the roof itself runs
//!   along the street, iron cresting along its ridge.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    IRON_BLACK, STAINED_TINT, STONE_DARK, brick, iron, matte, slate, stained, stone, wood,
};

/// The street house (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "gothic_horror_street_house",
    name: "Gothic Townhouse",
    description: "A Victorian gothic town house: soot-dark brick or stone, tall windows under \
                  pointed hoods, a stacked oriel, and a steep slate roof with iron cresting.",
    themes: &[ThemeArchetype::GothicHorror],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // The raised ground storey, and the tall storeys over it.
    storey_m: (4.0, 3.4),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BB2_0001,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Cornice", "Fascia", "Oriel", "Iron",
    ],
};

/// Soot-dark brick and stone, pale dressings, slate, black iron, oxblood
/// and black doors, the shops' fascias, and a little stained glass.
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
