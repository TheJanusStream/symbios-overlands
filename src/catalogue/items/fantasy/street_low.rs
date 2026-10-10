//! Berlin's low building in the fantasy theme's dress (#1598): where it
//! houses people, a row of thatched cottages; where it trades, an
//! apothecary with bow windows full of glowing phials.
//!
//! - **The cottage row**: as many cottages as the frontage holds, each a
//!   leaded window with a flower box and a plank door in its own colour,
//!   on a mossy sill between timber posts; one daub rolled for the row, a
//!   golden thatch along the street with thick eave rolls front and back,
//!   and a round stone chimney on the ridge over each cottage.
//! - **The apothecary**: bow windows standing out on timber sills, a door
//!   between them under a painted sign, and the shop behind lit in a
//!   potion's green; the same thatch over it.
//! - **A loft storey** where it has two: over a timber beam, posts and
//!   small windows with flower boxes under the eaves.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{STONE_MOSS, TIMBER_DARK, daub, matte, mossy, stone, thatch, timber};

/// The low building (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "fantasy_street_low",
    name: "Thatched Cottage Row",
    description: "A row of thatched storybook cottages with flower boxes and round chimneys, \
                  or - where the street trades - an apothecary with glowing bow windows.",
    themes: &[ThemeArchetype::Fantasy],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // A cottage's ground storey, and its loft.
    storey_m: (3.4, 2.8),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BB1_0003,
    materials,
    round_meshes: &["Chimney"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Beam", "Chimney", "Fascia", "Oriel", "Planter",
    ],
};

/// Cottage daubs, fieldstone, dark timbers, golden thatch, painted doors
/// and the apothecary's sign.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.24, 0.20, 0.16],
        panes: (3, 3),
        room: [1.0, 0.74, 0.44],
        shop: [0.62, 1.0, 0.66],
    });
    m.extend([
        ("Cream".to_string(), daub([0.90, 0.86, 0.74])),
        ("Ochre".to_string(), daub([0.84, 0.70, 0.46])),
        ("Sage".to_string(), daub([0.66, 0.72, 0.58])),
        ("Rose".to_string(), daub([0.84, 0.66, 0.60])),
        ("Stone".to_string(), stone([0.58, 0.56, 0.52])),
        ("Moss".to_string(), mossy(STONE_MOSS)),
        ("Timber".to_string(), timber(TIMBER_DARK)),
        ("Thatch".to_string(), thatch([0.70, 0.58, 0.32])),
        ("DoorRed".to_string(), timber([0.50, 0.18, 0.12])),
        ("DoorBlue".to_string(), timber([0.20, 0.30, 0.46])),
        ("DoorGreen".to_string(), timber([0.22, 0.38, 0.22])),
        ("DoorOak".to_string(), timber([0.40, 0.26, 0.14])),
        ("Leaf".to_string(), matte([0.24, 0.44, 0.20])),
        ("Sign".to_string(), matte([0.36, 0.16, 0.44])),
    ]);
    m
}
