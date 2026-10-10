//! Berlin's detached house in the modern city's dress (#1600): the house in
//! its garden that fills an eighth of the city - the rendered family house
//! of the outer districts, a town villa under a hipped roof, a white cube
//! under a flat slab.
//!
//! The reference house standing free: the conventions every theme's street
//! buildings keep are in [`crate::catalogue::items::street`], and this, with
//! its rules in `street_detached.cga`, is how a detached house keeps them.
//!
//! - **It stands in from its lot.** Its walls stand `Eave` in from every
//!   side, so its roof reaches out over them to the lot's edges and no
//!   further: eaves and verges, and nothing past the lot's sides.
//! - **Windows on all four sides.** No neighbour stands against it: its
//!   sides run the full depth, turning the corners, each with its windows;
//!   the front has its door up a step under a canopy, the garden side a
//!   wide window to the terrace in one bay of two.
//! - **One render, one roof.** The render is rolled once at the lot, and
//!   `Pick` decides the roof once: a tiled gable along the street, a hipped
//!   roof, or a flat slab reaching out to the lot's edges; the tiles red or
//!   anthracite, `Pick`ed once too.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{LAMP_WARM, concrete, enamel, slate, steel, stucco, timber};

/// The house (see the module docs); its rules are `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "city_street_detached",
    name: "City Detached House",
    description: "A family house in its garden: rendered walls with windows all round, a door \
                  under a canopy, and a tiled gable, a hipped roof or a flat slab.",
    themes: &[ThemeArchetype::ModernCity],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // A family house's rooms: a little taller below, where it is entered.
    storey_m: (3.0, 2.8),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BA0_0004,
    materials,
    round_meshes: &[],
    solid_meshes: &["Wall", "Door", "Pane", "Roof", "Trim", "Canopy"],
};

/// The renders, the doors and their canopies, and the tiles.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.94, 0.94, 0.93],
        panes: (2, 2),
        room: LAMP_WARM,
        shop: [0.98, 0.95, 0.86],
    });
    m.extend([
        ("White".to_string(), stucco([0.90, 0.89, 0.86])),
        ("Cream".to_string(), stucco([0.86, 0.81, 0.69])),
        ("Ochre".to_string(), stucco([0.80, 0.67, 0.46])),
        ("Grey".to_string(), stucco([0.62, 0.62, 0.61])),
        ("Trim".to_string(), stucco([0.93, 0.93, 0.91])),
        ("Stone".to_string(), concrete([0.60, 0.59, 0.57])),
        ("DoorWood".to_string(), timber([0.40, 0.27, 0.16])),
        ("DoorGrey".to_string(), enamel([0.25, 0.27, 0.29])),
        ("DoorRed".to_string(), enamel([0.50, 0.13, 0.12])),
        ("Metal".to_string(), steel([0.32, 0.34, 0.36])),
        ("TilesRed".to_string(), slate([0.52, 0.24, 0.17])),
        ("TilesDark".to_string(), slate([0.20, 0.21, 0.23])),
    ]);
    m
}
