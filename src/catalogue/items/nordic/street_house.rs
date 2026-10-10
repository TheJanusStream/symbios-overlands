//! Berlin's street house in the nordic theme's dress (#1598): a wooden
//! town house of a northern port - painted board cladding on a fieldstone
//! plinth, white corner boards and window frames, a steep roof of tarred
//! shakes, turf or slate along the street, and a cross-gable over its
//! middle where the house has one.
//!
//! - **One house, one paint**, rolled once at the lot: falu red, ochre,
//!   white or grey-blue boards, inherited by every wall that names no
//!   material of its own; the white trim names its own.
//! - **A shop or a home.** A trading house (`Trade`) has wide shop windows
//!   either side of its door and a woven sign over it; a house of homes has
//!   framed windows over a high sill.
//! - **Framed windows**: each a white frame proud of the boards, a sill
//!   under it and a head board over it.
//! - **The roof** is `Pick`ed once - shakes, turf or slate - and so is the
//!   cross-gable over the middle bays: a framed window in a storey of its
//!   own over the eave board, its bargeboards the eaves of its own little
//!   roof.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    SHIELD_BLUE, SHIELD_GOLD, STONE_COLD, STONE_GREY, TURF_GREEN, boards, cloth, rough_stone,
    shingle, stone, timber, turf,
};

/// The street house (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "nordic_street_house",
    name: "Nordic Timber Townhouse",
    description: "A northern wooden town house: painted board cladding on a stone plinth, \
                  white-framed windows, and a steep roof of shakes, turf or slate with a \
                  cross-gable.",
    themes: &[ThemeArchetype::Nordic],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // The shop storey over its plinth, and the board storeys over it.
    storey_m: (3.6, 3.0),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BA4_0001,
    materials,
    round_meshes: &[],
    solid_meshes: &["Wall", "Door", "Pane", "Roof", "Trim", "Plinth", "Sign"],
};

/// Painted boards in the north's colours, white trim, fieldstone, a woven
/// sign, and the shakes, slate and turf over them.
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
        ("Trim".to_string(), timber([0.90, 0.88, 0.82])),
        ("Plinth".to_string(), rough_stone(STONE_GREY)),
        ("Stone".to_string(), stone(STONE_COLD)),
        ("Door".to_string(), timber([0.20, 0.30, 0.24])),
        ("Shakes".to_string(), shingle([0.24, 0.18, 0.12])),
        ("Slate".to_string(), shingle([0.30, 0.32, 0.35])),
        ("Turf".to_string(), turf(TURF_GREEN)),
        ("Sign".to_string(), cloth(SHIELD_BLUE, SHIELD_GOLD)),
    ]);
    m
}
