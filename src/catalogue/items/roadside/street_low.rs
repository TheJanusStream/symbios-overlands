//! Berlin's low building in the roadside strip's dress (#1598): where it
//! trades, a chrome-banded diner with a ribbon of big windows and a neon
//! sign on its roof; where it houses people, a motor court - a row of
//! rooms behind a covered walk on steel posts, each with its own coloured
//! door.
//!
//! - **The diner**: one enamel colour rolled for the building, chrome bands
//!   at the sill and the head of a ribbon of windows, a door under a
//!   canopy, a flat roof behind a chrome-capped parapet, and a sign board
//!   on steel legs with a grid of neon cells. Where it has two storeys, a
//!   ribbon of windows runs over the diner.
//! - **The motor court**: painted brick, rooms along the front each a door
//!   and a window, a corrugated walk roof on steel posts the whole
//!   frontage, and - where it has two storeys - a gallery with a chrome
//!   rail over it and a second row of rooms.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::catalogue::items::util::glow;
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    CANOPY_LIT, CHROME_BRIGHT, CONCRETE_GREY, CORRUGATED_GREY, ENAMEL_BLUE, ENAMEL_CREAM,
    ENAMEL_RED, NEON_CYAN, NEON_RED, SIGN_AMBER, STEEL_GREY, brick, chrome, concrete, corrugated,
    enamel, steel,
};

/// The diner or motor court (see the module docs); its rules are
/// `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "roadside_street_low",
    name: "Diner and Motor Court",
    description: "A chrome-banded enamel diner with big windows and a neon roof sign - or, \
                  where the street does not trade, a motor court of coloured doors behind \
                  a covered walk.",
    themes: &[ThemeArchetype::Roadside],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // The diner's tall storey, and a room storey over it.
    storey_m: (4.0, 3.0),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BAB_0003,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Canopy", "Balcony", "Sign", "Steel",
    ],
};

/// The diner's enamels and chrome, the court's painted brick and doors, the
/// walk's corrugated roof, and the sign's steel and neon.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.80, 0.82, 0.86],
        panes: (2, 1),
        room: [1.0, 0.86, 0.62],
        shop: CANOPY_LIT,
    });
    m.extend([
        ("DinerRed".to_string(), enamel(ENAMEL_RED)),
        ("DinerTeal".to_string(), enamel([0.22, 0.62, 0.62])),
        ("DinerCream".to_string(), enamel(ENAMEL_CREAM)),
        ("PaintCream".to_string(), brick([0.86, 0.80, 0.66])),
        ("PaintMint".to_string(), brick([0.62, 0.80, 0.72])),
        ("PaintSalmon".to_string(), brick([0.86, 0.60, 0.50])),
        ("Base".to_string(), concrete(CONCRETE_GREY)),
        ("Chrome".to_string(), chrome(CHROME_BRIGHT)),
        ("Steel".to_string(), steel(STEEL_GREY)),
        ("WalkRoof".to_string(), corrugated(CORRUGATED_GREY)),
        ("DoorRed".to_string(), enamel(ENAMEL_RED)),
        ("DoorBlue".to_string(), enamel(ENAMEL_BLUE)),
        ("DoorTeal".to_string(), enamel([0.14, 0.52, 0.52])),
        ("DoorYellow".to_string(), enamel([0.90, 0.70, 0.16])),
        ("DoorChrome".to_string(), chrome([0.40, 0.42, 0.46])),
        ("SignBack".to_string(), enamel([0.10, 0.12, 0.16])),
        ("NeonRed".to_string(), glow(NEON_RED, 3.0)),
        ("NeonCyan".to_string(), glow(NEON_CYAN, 3.0)),
        ("NeonAmber".to_string(), glow(SIGN_AMBER, 3.0)),
        ("Deck".to_string(), concrete([0.22, 0.22, 0.23])),
    ]);
    m
}
