//! Berlin's low building in the medieval theme's dress (#1598): where it
//! houses people, a row of craftsmen's cottages - timber-framed, a door and
//! a window each, under one thatched roof; where it trades, a workshop -
//! a stone front of open bays under a shingled pentice, the shutters let
//! down as counters and a sign over each.
//!
//! - **The cottage row**: as many cottages as the frontage holds, each a
//!   window and a door between oak posts on a stone sole, the limewash
//!   rolled once for the row and every household its own door (`%` per
//!   cottage), a jettied loft storey where it has two.
//! - **The workshop**: stone piers between wide bays, a shingled pentice
//!   along the whole front, a loft of small windows where it has two
//!   storeys, and a steep tiled roof.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    HERALD_BLUE, SLATE_GREY, STONE_GREY, THATCH_STRAW, WOOD_DARK, WOOD_OAK, daub, panelling,
    shingle, stone, thatch, timber,
};

/// The low building (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "medieval_street_low",
    name: "Craftsmen's Row",
    description: "A row of timber-framed craftsmen's cottages under one thatched roof, or a \
                  stone workshop with open bays under a tiled pentice.",
    themes: &[ThemeArchetype::Medieval],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // A workshop's ground storey, and the loft over it.
    storey_m: (3.4, 2.8),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BA2_0003,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Timber", "Shutter", "Sign",
    ],
};

/// Oak framing, limewashed daub, stone, joinery, signs, and the thatch and
/// tiles over them.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.24, 0.17, 0.10],
        panes: (3, 3),
        room: [1.0, 0.66, 0.34],
        shop: [1.0, 0.74, 0.42],
    });
    m.extend([
        ("Oak".to_string(), timber(WOOD_OAK)),
        ("DarkOak".to_string(), timber(WOOD_DARK)),
        ("Oxblood".to_string(), timber([0.40, 0.13, 0.09])),
        ("DaubCream".to_string(), daub([0.92, 0.88, 0.76])),
        ("DaubOchre".to_string(), daub([0.88, 0.72, 0.46])),
        ("DaubWhite".to_string(), daub([0.95, 0.93, 0.88])),
        ("DaubRose".to_string(), daub([0.88, 0.70, 0.64])),
        ("Stone".to_string(), stone(STONE_GREY)),
        ("Dressing".to_string(), stone([0.70, 0.66, 0.58])),
        ("Door".to_string(), panelling([0.34, 0.22, 0.12])),
        ("Shutter".to_string(), panelling([0.26, 0.34, 0.22])),
        ("Tile".to_string(), shingle([0.52, 0.24, 0.16])),
        ("Slate".to_string(), shingle(SLATE_GREY)),
        ("Thatch".to_string(), thatch(THATCH_STRAW)),
        ("Sign".to_string(), panelling(HERALD_BLUE)),
    ]);
    m
}
