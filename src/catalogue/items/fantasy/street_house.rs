//! Berlin's street house in the fantasy theme's dress (#1598): a crooked
//! storybook townhouse - a guild's stone ground floor, jettied storeys of
//! dark timber over coloured daub, leaded windows, round turret oriels
//! under slate cones, and a steep slate roof.
//!
//! - **One house, one daub.** The infill colour is rolled once at the lot
//!   and inherited by every wall above the stone ground floor; the stone,
//!   the timbers, the doors and the signs name their own.
//! - **Shops or a home.** A trading house has guild shops either side of
//!   its door, each a leaded window over a mossy plinth under a painted
//!   sign board, every shop its own colour; a house of homes has raised
//!   windows over a high sill.
//! - **Jettied storeys.** Every upper storey stands on a timber jetty beam
//!   the whole frontage, its bays between proud posts, each window under a
//!   pointed timber hood.
//! - **Turret oriels.** `Pick`ed once: two round turrets, one, or none,
//!   each a stone shaft a storey at a time with a lancet window on its
//!   front, and over the eaves a slate cone with a gold finial.
//! - **The roof** is steep slate, `Pick`ed once: a gable to the street - a
//!   row of them makes a sawtooth - or a ridge along the street with its
//!   gable ends the party walls.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    GOLD, MANA_TEAL, STONE_MOSS, TIMBER_DARK, daub, gold, matte, mossy, slate, stone, timber,
};

/// The house (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "fantasy_street_house",
    name: "Turret Townhouse",
    description: "A storybook townhouse: guild shops in a stone ground floor, jettied storeys \
                  of dark timber and coloured daub, round turret oriels under slate cones, \
                  and a steep slate roof.",
    themes: &[ThemeArchetype::Fantasy],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // A guild hall's tall ground storey, then the jettied storeys.
    storey_m: (4.2, 3.4),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BB1_0001,
    materials,
    round_meshes: &["Turret", "Cap", "Finial"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Beam", "Turret", "Cap", "Finial", "Fascia",
    ],
};

/// Storybook daubs, the guild's stone and timbers, leaded glass, painted
/// signs and a blue or green slate.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.20, 0.19, 0.22],
        panes: (3, 4),
        room: [1.0, 0.76, 0.46],
        shop: [0.84, 0.70, 1.0],
    });
    m.extend([
        ("Cream".to_string(), daub([0.90, 0.85, 0.72])),
        ("Ochre".to_string(), daub([0.82, 0.66, 0.42])),
        ("Sage".to_string(), daub([0.64, 0.70, 0.56])),
        ("Rose".to_string(), daub([0.82, 0.62, 0.58])),
        ("Lilac".to_string(), daub([0.70, 0.64, 0.78])),
        ("Stone".to_string(), stone([0.60, 0.58, 0.54])),
        ("Moss".to_string(), mossy(STONE_MOSS)),
        ("Timber".to_string(), timber(TIMBER_DARK)),
        ("Door".to_string(), timber([0.36, 0.20, 0.12])),
        ("DoorBlue".to_string(), timber([0.18, 0.26, 0.42])),
        ("SignRed".to_string(), matte([0.60, 0.16, 0.14])),
        ("SignBlue".to_string(), matte([0.16, 0.26, 0.52])),
        ("SignGreen".to_string(), matte([0.18, 0.40, 0.24])),
        ("Gold".to_string(), gold(GOLD)),
        (
            "Rune".to_string(),
            crate::catalogue::items::util::glow(MANA_TEAL, 2.4),
        ),
        ("Slate".to_string(), slate([0.30, 0.33, 0.44])),
        ("SlateGreen".to_string(), slate([0.30, 0.40, 0.34])),
    ]);
    m
}
