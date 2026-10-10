//! Berlin's low building in the classical theme's dress (#1598): where it
//! trades, a row of tabernae - open shop bays between travertine piers,
//! each over its counter under a painted sign, a pentice of tiles over
//! them; where it houses people, a domus front - a plain plastered wall
//! with few small windows, and a doorway between two marble columns under
//! a pediment.
//!
//! - **The row of tabernae**: as many bays as the frontage holds, a door
//!   where a bay is wide enough, and a tiled pent roof along the whole
//!   front; a row of small windows over it where it has two storeys.
//! - **The domus front**: windows set high, the door in the middle of the
//!   frontage, its columns turned round and its pediment a little gable
//!   facing the street.
//! - **A low tiled roof** along the street, its gable ends the party walls.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    MARBLE_WHITE, TERRACOTTA, adobe, brick, marble, roof_tile, sandstone, terracotta, wood,
};

/// The low building (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "ancient_street_low",
    name: "Roman Tabernae",
    description: "A row of Roman tabernae shops under a tiled pentice, or a domus front with a \
                  columned, pedimented doorway, under a low tiled roof.",
    themes: &[ThemeArchetype::AncientClassical],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // A taberna's tall ground storey, and the loft or the rooms over it.
    storey_m: (4.0, 3.0),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BA1_0003,
    materials,
    round_meshes: &["Column"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Cornice", "Fascia", "Column",
    ],
};

/// The domus's plasters, the tabernae's brick and travertine, the marble
/// of a doorway, timber, doors, signs and the tiles over them.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.36, 0.25, 0.15],
        panes: (2, 2),
        room: [1.0, 0.70, 0.40],
        shop: [1.0, 0.80, 0.52],
    });
    m.extend([
        ("PlasterOchre".to_string(), adobe([0.90, 0.70, 0.46])),
        ("PlasterRed".to_string(), adobe([0.64, 0.30, 0.22])),
        ("PlasterCream".to_string(), adobe([0.94, 0.88, 0.74])),
        ("Brick".to_string(), brick([0.68, 0.38, 0.25])),
        ("Travertine".to_string(), sandstone([0.84, 0.79, 0.68])),
        ("Marble".to_string(), marble(MARBLE_WHITE)),
        ("Timber".to_string(), wood([0.42, 0.27, 0.15])),
        ("Door".to_string(), wood([0.30, 0.18, 0.10])),
        ("Tile".to_string(), roof_tile(TERRACOTTA)),
        ("Sign".to_string(), terracotta([0.56, 0.14, 0.10])),
    ]);
    m
}
