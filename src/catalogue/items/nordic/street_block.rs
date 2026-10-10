//! Berlin's long block in the nordic theme's dress (#1598): a wharf row
//! like Bergen's Bryggen - a run of tall timber houses, each section its
//! own painted boards under its own steep gable turned to the street, on a
//! fieldstone plinth, with stacked open galleries on some of them.
//!
//! - **Sections of twelve metres**, each a house of its own: its paint
//!   rolled per section (`%`), a door at street level, a column of loft
//!   doors over it, and a steep gable of tarred shakes or slate with a
//!   carved mast up its middle - a hoist beam out of it under the peak and
//!   a finial over it - so a long block reads as the row of gables a
//!   wharf is.
//! - **Galleries**: some houses (`%`) carry a gallery at every storey, a
//!   deck at the storey's foot and a board rail at its edge, stacked up
//!   the front.
//! - **Shops or homes on the street**: a trading row has wide shop windows
//!   in its ground storey, one of homes framed windows.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    SHIELD_BLUE, SHIELD_GOLD, STONE_COLD, STONE_GREY, WOOD_WARM, boards, cloth, rough_stone,
    shingle, stone, timber,
};

/// The long block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "nordic_street_block",
    name: "Wharf Row",
    description: "A Bryggen-like wharf row: tall timber houses side by side, each painted its \
                  own colour under its own steep gable to the street, some with stacked \
                  galleries.",
    themes: &[ThemeArchetype::Nordic],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // The warehouse ground storey, and the board storeys over it.
    storey_m: (3.6, 2.9),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BA4_0002,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Plinth", "Gallery", "Carving", "Sign",
    ],
};

/// Painted and tarred boards, white trim, fieldstone, gallery timber,
/// carving, a woven sign, and the shakes and slate over them.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.92, 0.90, 0.85],
        panes: (2, 3),
        room: [1.0, 0.72, 0.42],
        shop: [1.0, 0.82, 0.55],
    });
    m.extend([
        ("FaluRed".to_string(), boards([0.50, 0.15, 0.11])),
        ("Ochre".to_string(), boards([0.76, 0.56, 0.24])),
        ("White".to_string(), boards([0.86, 0.84, 0.78])),
        ("GreyBlue".to_string(), boards([0.44, 0.52, 0.58])),
        ("Tar".to_string(), boards([0.22, 0.16, 0.11])),
        ("Trim".to_string(), timber([0.90, 0.88, 0.82])),
        ("Plinth".to_string(), rough_stone(STONE_GREY)),
        ("Stone".to_string(), stone(STONE_COLD)),
        ("Log".to_string(), timber([0.36, 0.24, 0.14])),
        ("Carving".to_string(), timber(WOOD_WARM)),
        ("Door".to_string(), timber([0.20, 0.30, 0.24])),
        ("Shakes".to_string(), shingle([0.24, 0.18, 0.12])),
        ("Slate".to_string(), shingle([0.30, 0.32, 0.35])),
        ("Sign".to_string(), cloth(SHIELD_BLUE, SHIELD_GOLD)),
    ]);
    m
}
