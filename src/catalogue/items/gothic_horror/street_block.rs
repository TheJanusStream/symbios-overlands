//! Berlin's long block in the gothic horror theme's dress (#1598): an
//! asylum or a workhouse - a long grim range of dark masonry, rows of tall
//! pointed windows, a rusticated ground storey, a steep slate roof with a
//! pointed gable and a pair of tall chimney stacks over every section.
//!
//! - **One institution, one masonry**, rolled once at the lot.
//! - **Sections of twelve metres**, each a ward: a pointed doorway up a
//!   step on its axis, stone quoins at its ends, a steep gable over its
//!   middle bays on the roof and tall brick stacks either side of it.
//! - **Windows in pointed hoods**, a string course of pale stone at every
//!   floor, and a ground storey of shops or of barred-looking narrow
//!   windows.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{IRON_BLACK, STONE_DARK, STONE_MOSS, brick, iron, matte, mossy, slate, stone, wood};

/// The long block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "gothic_horror_street_block",
    name: "Asylum Block",
    description: "A grim gothic institution range: dark masonry, rows of tall pointed windows, a \
                  steep slate roof with a pointed gable and tall chimney stacks over every \
                  section.",
    themes: &[ThemeArchetype::GothicHorror],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // The rusticated ground storey, and the ward storeys over it.
    storey_m: (4.2, 3.2),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BB2_0002,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Cornice", "Fascia", "Chimney", "Iron",
    ],
};

/// Soot-dark brick and stone, moss, pale dressings, slate, black iron, and
/// the black doors and fascias.
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
        ("Mossy".to_string(), mossy(STONE_MOSS)),
        ("Dressing".to_string(), stone([0.66, 0.64, 0.58])),
        ("Slate".to_string(), slate([0.20, 0.21, 0.24])),
        ("Iron".to_string(), iron(IRON_BLACK)),
        ("DoorBlack".to_string(), wood([0.14, 0.13, 0.13])),
        ("Fascia".to_string(), matte([0.06, 0.06, 0.07])),
    ]);
    m
}
