//! Berlin's low building in the nordic theme's dress (#1598): where it
//! trades, a boathouse front - tarred boards under steep gables turned to
//! the street, a wide plank door under each and a carved post at each peak;
//! where it houses people, a longhouse row - low log-and-board cottages
//! under one turf roof along the street.
//!
//! - **The boathouse**: one gable to every five or six metres of frontage,
//!   each over a wide door between narrow windows, a loft window over it
//!   where it has two storeys.
//! - **The longhouse row**: a door and two small framed windows to each
//!   cottage, the boards rolled once for the row, and a low-pitched turf
//!   roof whose gable ends are the party walls.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    STONE_COLD, STONE_GREY, TURF_GREEN, WOOD_WARM, boards, rough_stone, shingle, stone, timber,
    turf,
};

/// The low building (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "nordic_street_low",
    name: "Boathouse and Longhouse",
    description: "A row of tarred boathouse gables with wide plank doors and carved peaks, or a \
                  low longhouse row of board cottages under a turf roof.",
    themes: &[ThemeArchetype::Nordic],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // The boathouse's tall ground storey, and the loft over it.
    storey_m: (3.6, 2.6),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BA4_0003,
    materials,
    round_meshes: &[],
    solid_meshes: &["Wall", "Door", "Pane", "Roof", "Trim", "Plinth", "Carving"],
};

/// Tarred and painted boards, logs, white trim, fieldstone, carving, and
/// the turf and shakes over them.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.92, 0.90, 0.85],
        panes: (2, 3),
        room: [1.0, 0.72, 0.42],
        shop: [1.0, 0.82, 0.55],
    });
    m.extend([
        ("FaluRed".to_string(), boards([0.50, 0.15, 0.11])),
        ("Tar".to_string(), boards([0.22, 0.16, 0.11])),
        ("Trim".to_string(), timber([0.90, 0.88, 0.82])),
        ("Plinth".to_string(), rough_stone(STONE_GREY)),
        ("Stone".to_string(), stone(STONE_COLD)),
        ("Log".to_string(), timber([0.36, 0.24, 0.14])),
        ("Carving".to_string(), timber(WOOD_WARM)),
        ("Door".to_string(), timber([0.20, 0.30, 0.24])),
        ("Shakes".to_string(), shingle([0.24, 0.18, 0.12])),
        ("Turf".to_string(), turf(TURF_GREEN)),
    ]);
    m
}
