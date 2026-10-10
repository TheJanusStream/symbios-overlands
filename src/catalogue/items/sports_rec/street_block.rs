//! Berlin's long block in the sports theme's dress (#1598): a sports
//! centre slab - a glazed hall two storeys high along the street, and over
//! it storeys of the sports school's rooms in panelled bands.
//!
//! - **The hall front**: the ground storey and the one over it are one
//!   lit glazed wall of mullions, its doors under a canopy at each section,
//!   a band in the club's colour over it with a lit sign for the centre's
//!   name; where the block trades, a fascia over the shops in its glass.
//! - **One centre, one colour.** The panels are rolled once at the lot and
//!   the club's colour is `Pick`ed once, for the bands, the canopies, the
//!   fascias, the parapet's stripe and the window fins.
//! - **The upper storeys**: wide windows between panel joints, a fin in
//!   the club's colour beside each, standing out of the front, where the
//!   block has them (`Pick`ed once).
//! - **A flat roof** behind a parapet, floodlight masts at each section.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::catalogue::items::util::glow;
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    ASPHALT_DARK, CONCRETE_GREY, CORRUGATED_GREY, FLOOD_LIT, LINE_WHITE, STEEL_GREY, asphalt,
    concrete, corrugated, enamel, painted, steel,
};

/// The block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "sports_rec_street_block",
    name: "Sports Centre Block",
    description: "A sports centre slab: a two-storey glazed hall along the street, and over it \
                  panelled storeys with wide windows and fins in the club's colour.",
    themes: &[ThemeArchetype::SportsRec],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // The hall's storeys, then the sports school's rooms over it.
    storey_m: (4.0, 3.1),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BAD_0002,
    materials,
    round_meshes: &["Mast"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Band", "Mullion", "Canopy", "Fin", "Sign", "Mast",
        "Lamp",
    ],
};

/// Grey and white panels, board-formed concrete, the club's four colours,
/// the hall's steel and the lit name band.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.30, 0.31, 0.34],
        panes: (2, 2),
        room: [1.0, 0.92, 0.78],
        shop: [0.92, 0.97, 1.0],
    });
    m.extend([
        ("Panel".to_string(), corrugated(CORRUGATED_GREY)),
        ("White".to_string(), enamel([0.86, 0.87, 0.88])),
        ("Concrete".to_string(), concrete(CONCRETE_GREY)),
        ("Base".to_string(), concrete([0.34, 0.34, 0.35])),
        ("TeamRed".to_string(), enamel([0.80, 0.12, 0.10])),
        ("TeamBlue".to_string(), enamel([0.08, 0.30, 0.74])),
        ("TeamGreen".to_string(), enamel([0.08, 0.52, 0.22])),
        ("TeamOrange".to_string(), enamel([0.96, 0.46, 0.06])),
        ("Trim".to_string(), painted(LINE_WHITE)),
        ("Steel".to_string(), steel(STEEL_GREY)),
        ("Door".to_string(), enamel([0.14, 0.15, 0.17])),
        ("Deck".to_string(), asphalt(ASPHALT_DARK)),
        ("Lamp".to_string(), glow(FLOOD_LIT, 3.0)),
        ("NameLit".to_string(), glow(LINE_WHITE, 1.6)),
    ]);
    m
}
