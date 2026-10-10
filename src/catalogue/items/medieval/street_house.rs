//! Berlin's street house in the medieval theme's dress (#1598): a burgher
//! house of a Hanseatic town - a stone ground storey with its hall door or
//! its shop shutters, timber-framed storeys over it each jettied out past
//! the one below, and steep gables turned to the street.
//!
//! - **One frame, one infill.** The frame's timber is rolled once at the lot
//!   and inherited by every post and rail; the daub between them is `Pick`ed
//!   once, so a house is one colour of limewash in one colour of oak.
//! - **Shop or hall.** A trading house (`Trade`) opens its ground storey in
//!   bays, each a window over a shutter let down as a counter, a sign
//!   hanging by the door; a house of homes has its great hall door and small
//!   windows set high in the stone.
//! - **Jetties.** Each storey stands a step further out than the one below,
//!   on a bressumer beam that closes the jetty's underside, so the top
//!   storey overhangs the street - within the two metres a front may reach.
//! - **Gables to the street**: one steep gable on a narrow house, two on a
//!   wide one, jettied out with the top storey; their slopes meet the
//!   neighbours' in valleys at the party walls.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    HERALD_BLUE, IRON_DARK, SLATE_GREY, STONE_GREY, WOOD_DARK, WOOD_OAK, daub, iron, panelling,
    shingle, stone, timber,
};

/// The street house (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "medieval_street_house",
    name: "Burgher House",
    description: "A Hanseatic burgher house: a stone ground storey with shop shutters or a hall \
                  door, jettied timber-framed storeys, and steep gables to the street.",
    themes: &[ThemeArchetype::Medieval],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // The stone hall storey, and the framed storeys over it.
    storey_m: (3.8, 3.0),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BA2_0001,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Timber", "Shutter", "Sign",
    ],
};

/// Oak framing in three stains, limewashed daub, the stone of the ground
/// storey, joinery, iron, signs and the roofs over them.
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
        ("Iron".to_string(), iron(IRON_DARK)),
        ("Sign".to_string(), panelling(HERALD_BLUE)),
    ]);
    m
}
