//! Berlin's street house in the roadside strip's dress (#1598): the motor
//! hotel of the 1950s and 60s grown to the Altbau's height - painted brick
//! storeys of rooms opening onto open galleries with chrome rails, a lobby
//! or a coffee shop at street level, and a big neon sign on the roof.
//!
//! - **One hotel, one paint**, rolled once at the lot: cream, mint or
//!   salmon brick; the galleries, the rails, the doors and the sign name
//!   their own.
//! - **Galleries.** Every storey over the ground one has an open gallery
//!   across the front, a concrete deck with a chrome rail, and the rooms
//!   along it each a door and a window - every room its own door colour.
//! - **Lobby or coffee shop.** A hotel whose street trades has a coffee
//!   shop either side of the lobby under a striped awning; one that does not
//!   has the lobby's windows. Either way the lobby door has a chrome-edged
//!   canopy.
//! - **The roof sign**: a board on steel legs over the parapet, its face
//!   a grid of neon cells - red, cyan or amber, `Pick`ed once - so it reads
//!   as a lit sign, not a glowing slab, from down the road.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::catalogue::items::util::glow;
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRICK_TAN, CANOPY_LIT, CHROME_BRIGHT, CONCRETE_GREY, ENAMEL_BLUE, ENAMEL_CREAM, ENAMEL_RED,
    NEON_CYAN, NEON_RED, SIGN_AMBER, STEEL_GREY, brick, chrome, concrete, enamel, steel,
};

/// The motor hotel (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "roadside_street_house",
    name: "Motor Hotel",
    description: "A painted-brick motor hotel: open galleries with chrome rails on every \
                  storey, a lobby or coffee shop at street level, and a big neon sign on \
                  steel legs over the roof.",
    themes: &[ThemeArchetype::Roadside],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // The lobby storey, then the room storeys.
    storey_m: (4.2, 3.0),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BAB_0001,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Balcony", "Canopy", "Awning", "Sign", "Steel",
    ],
};

/// Three paints of brick, the galleries' concrete and chrome, the room
/// doors, the awning, and the sign's steel and neon.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.80, 0.82, 0.86],
        panes: (2, 1),
        room: [1.0, 0.86, 0.62],
        shop: CANOPY_LIT,
    });
    m.extend([
        ("PaintCream".to_string(), brick([0.86, 0.80, 0.66])),
        ("PaintMint".to_string(), brick([0.62, 0.80, 0.72])),
        ("PaintSalmon".to_string(), brick([0.86, 0.60, 0.50])),
        ("Base".to_string(), brick(BRICK_TAN)),
        ("Deck".to_string(), concrete(CONCRETE_GREY)),
        ("Chrome".to_string(), chrome(CHROME_BRIGHT)),
        ("Steel".to_string(), steel(STEEL_GREY)),
        ("DoorRed".to_string(), enamel(ENAMEL_RED)),
        ("DoorBlue".to_string(), enamel(ENAMEL_BLUE)),
        ("DoorTeal".to_string(), enamel([0.14, 0.52, 0.52])),
        ("DoorYellow".to_string(), enamel([0.90, 0.70, 0.16])),
        ("Awning".to_string(), enamel(ENAMEL_RED)),
        ("AwningCream".to_string(), enamel(ENAMEL_CREAM)),
        ("SignBack".to_string(), enamel([0.10, 0.12, 0.16])),
        ("NeonRed".to_string(), glow(NEON_RED, 3.0)),
        ("NeonCyan".to_string(), glow(NEON_CYAN, 3.0)),
        ("NeonAmber".to_string(), glow(SIGN_AMBER, 3.0)),
        ("Roof".to_string(), concrete([0.20, 0.20, 0.21])),
    ]);
    m
}
