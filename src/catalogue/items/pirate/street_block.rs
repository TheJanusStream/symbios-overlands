//! Berlin's long block in the pirate theme's dress (#1598): the customs
//! house and prize warehouse of a sugar port - a long front of dressed
//! stone, a ground storey of arched bays or of barred store windows, rows
//! of small windows on dressed sills over it, and in each section a column
//! of loading doors under a hoist beam, all under a long shingle roof or a
//! parapet.
//!
//! - **One block, one stone**, rolled once at the lot: grey quay stone,
//!   pale limestone or a limewashed render over rubble; the dressings, the
//!   doors and the roof name their own.
//! - **Sections of twelve metres**: each one a stair door up a stone step
//!   and, on its axis, a stack of loading doors - one each storey - under a
//!   hoist beam standing out of the crown at its top.
//! - **Stores or a market on the quay**: a trading block opens its ground
//!   storey in arched bays under voussoirs and keystones, each a window and
//!   a door; one that only stores has barred windows high in its wall.
//! - **The roof is a long shingle gable or a flat deck** behind a parapet
//!   (`Pick`ed once), the gable's ends the party walls.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    HULL_OAK, IRON_BLACK, LAMP_TALLOW, OAK_JOINERY, SHINGLE_GREY, STONE_LIME, ashlar, board, iron,
    limewash, shingle, strake, tar,
};

/// The block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "pirate_street_block",
    name: "Customs House",
    description: "A long stone customs house and prize warehouse: arched bays on the quay, \
                  rows of barred windows, a stack of loading doors under a hoist beam in each \
                  section, and a long shingle roof.",
    themes: &[ThemeArchetype::Pirate],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // A high arcaded ground storey, and the warehouse's lofts over it.
    storey_m: (4.4, 3.3),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BB7_0002,
    materials,
    round_meshes: &[],
    solid_meshes: &["Wall", "Door", "Pane", "Roof", "Trim", "Beam", "Bars"],
};

/// Quay stone, limestone and limewash, tarred oak, iron, and the shingle
/// over them.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: OAK_JOINERY,
        panes: (3, 3),
        room: LAMP_TALLOW,
        shop: [1.0, 0.84, 0.54],
    });
    m.extend([
        (
            "QuayStone".to_string(),
            ashlar([0.54, 0.51, 0.46], 0xA5_0011),
        ),
        ("Limestone".to_string(), ashlar(STONE_LIME, 0xA5_0012)),
        ("Render".to_string(), limewash([0.84, 0.76, 0.60])),
        (
            "Dressing".to_string(),
            ashlar([0.76, 0.72, 0.64], 0xA5_0013),
        ),
        ("Door".to_string(), strake(HULL_OAK)),
        ("Hatch".to_string(), board([0.20, 0.30, 0.26])),
        ("Beam".to_string(), board(OAK_JOINERY)),
        ("Iron".to_string(), iron(IRON_BLACK, 0xA5_0014)),
        ("Shingle".to_string(), shingle(SHINGLE_GREY)),
        ("Deck".to_string(), tar([0.30, 0.28, 0.25])),
    ]);
    m
}
