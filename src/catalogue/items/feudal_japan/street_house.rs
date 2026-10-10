//! Berlin's street house in the Edo theme's dress (#1598): a machiya grown
//! tall - a merchant's tower house with a tiled pent eave over every
//! storey, lattice fronts between dark posts, a shop under indigo noren
//! curtains, and a tiled gable roof between plastered fire walls.
//!
//! - **One house, one plaster**, rolled once at the lot - white lime,
//!   earthen ochre or grey; the posts, lattices and tiles name their own.
//! - **A shop or a home.** A trading house opens its ground storey as a
//!   shop behind a lattice under noren, each curtain its own colour, with
//!   a paper lantern at its door; a house of homes has a close lattice
//!   front and a sliding door.
//! - **Stacked eaves.** Every storey ends in a tiled pent eave the whole
//!   frontage, sloping to the street, so a row's eaves run on unbroken.
//! - **Lattice fronts**: each upper storey's bays between proud posts, a
//!   lattice window in each, its timbers dark or bengara red (`Pick`ed
//!   once).
//! - **The roof**: a tiled gable along the street, its ends the party
//!   walls, and where the house has them (`Pick`ed once) an udatsu fire
//!   wall at each party wall from the first floor up, standing out through
//!   the eaves under its own tile cap.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    LANTERN_GLOW, PLASTER_WHITE, STONE_GREY, TIMBER_BROWN, TIMBER_DARK, lacquer, paper, plaster,
    roof_tile, stone, timber,
};

/// The house (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "feudal_japan_street_house",
    name: "Merchant Tower House",
    description: "A machiya grown tall: a tiled pent eave over every storey, lattice fronts \
                  between dark posts, a shop under noren curtains and paper lanterns, and a \
                  tiled gable between plastered fire walls.",
    themes: &[ThemeArchetype::FeudalJapan],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // A shop's ground storey, then the storeys under their eaves.
    storey_m: (4.0, 3.4),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BA5_0001,
    materials,
    round_meshes: &["Lantern"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Post", "Eave", "Fascia", "Firewall",
    ],
};

/// Lime and earthen plasters, dark and bengara-red timbers, kawara tiles,
/// stone, noren cloth and lantern paper, behind lattice glazing.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.20, 0.14, 0.10],
        panes: (6, 4),
        room: [1.0, 0.80, 0.52],
        shop: [1.0, 0.86, 0.62],
    });
    m.extend(feudal_street_palette());
    m
}

/// The plasters, timbers, tiles and cloths every Edo street building
/// shares (#1598): the block's and the low building's palettes start
/// from it.
pub(crate) fn feudal_street_palette() -> Vec<(String, SovereignMaterialSettings)> {
    vec![
        ("Plaster".to_string(), plaster(PLASTER_WHITE)),
        ("Earth".to_string(), plaster([0.76, 0.64, 0.46])),
        ("Ash".to_string(), plaster([0.64, 0.63, 0.60])),
        ("Timber".to_string(), timber(TIMBER_BROWN)),
        ("TimberDark".to_string(), timber(TIMBER_DARK)),
        ("Bengara".to_string(), lacquer([0.44, 0.15, 0.10])),
        ("Tile".to_string(), roof_tile([0.13, 0.15, 0.18])),
        ("Stone".to_string(), stone(STONE_GREY)),
        ("Indigo".to_string(), paper([0.12, 0.18, 0.36])),
        ("Madder".to_string(), paper([0.52, 0.16, 0.12])),
        ("Undyed".to_string(), paper([0.84, 0.80, 0.70])),
        (
            "Lantern".to_string(),
            crate::catalogue::items::util::glow(LANTERN_GLOW, 2.2),
        ),
    ]
}
