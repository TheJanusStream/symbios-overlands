//! Berlin's low building in the pirate theme's dress (#1598): where it
//! trades, a harbour tavern - a tarred plank front over a stone plinth,
//! small-paned windows, a broad door, and its sign hung out square to the
//! street from an iron bracket; where it houses people, a row of
//! fishermen's cottages in limewash under one shingle roof.
//!
//! - **The tavern**: its front of ship's strakes on a quay-stone plinth,
//!   small-paned windows either side of the door, the sign board on its
//!   bracket beside the door, and a loft of shuttered windows where it has
//!   two storeys; a steep shingle gable along the street.
//! - **The cottages**: as many as the frontage holds, each a shuttered
//!   window and a plank door - every household its own colour of limewash
//!   and of door - under one shingle gable whose ends are the party walls.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    HULL_TAR, IRON_BLACK, LAMP_TALLOW, OAK_JOINERY, SHINGLE_GREY, STONE_QUAY, ashlar, board, iron,
    limewash, shingle, strake,
};

/// The low building (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "pirate_street_low",
    name: "Harbour Tavern",
    description: "A harbour tavern of tarred strakes on a stone plinth, its sign hung out on \
                  an iron bracket - or, where nothing trades, a row of limewashed fishermen's \
                  cottages under one shingle roof.",
    themes: &[ThemeArchetype::Pirate],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // A taproom's ground storey, and a low loft over it.
    storey_m: (3.6, 2.9),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BB7_0003,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Fascia", "Shutter", "Bracket",
    ],
};

/// Tarred strakes, quay stone, the cottages' limewash colours, their
/// doors, and the shingle over them.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: OAK_JOINERY,
        panes: (3, 3),
        room: LAMP_TALLOW,
        shop: [1.0, 0.80, 0.46],
    });
    m.extend([
        ("Strake".to_string(), strake(HULL_TAR)),
        ("Stone".to_string(), ashlar(STONE_QUAY, 0xA5_0021)),
        ("Ochre".to_string(), limewash([0.84, 0.66, 0.38])),
        ("Coral".to_string(), limewash([0.86, 0.58, 0.50])),
        ("SeaGreen".to_string(), limewash([0.52, 0.72, 0.62])),
        ("Sky".to_string(), limewash([0.56, 0.70, 0.80])),
        ("Lime".to_string(), limewash([0.90, 0.88, 0.80])),
        ("Trim".to_string(), board([0.82, 0.78, 0.68])),
        ("Door".to_string(), board(OAK_JOINERY)),
        ("DoorBlue".to_string(), board([0.20, 0.34, 0.52])),
        ("DoorRed".to_string(), board([0.54, 0.18, 0.14])),
        ("Shutter".to_string(), board([0.18, 0.40, 0.30])),
        ("Sign".to_string(), board([0.50, 0.12, 0.10])),
        ("Iron".to_string(), iron(IRON_BLACK, 0xA5_0022)),
        ("Shingle".to_string(), shingle(SHINGLE_GREY)),
    ]);
    m
}
