//! Berlin's long block in the coastal resort's dress (#1598): the
//! beachfront hotel slab of the sixties - every storey a continuous
//! balcony the whole frontage, its slab edge white and its rail steel, the
//! rooms' glass walls behind it in a pastel frame, a glazed lobby storey on
//! the promenade, and a sun deck under canvas shades on the roof.
//!
//! - **One slab, one pastel**, rolled once at the lot for the walls between
//!   the balconies; the slab edges, the rails and the shades name theirs.
//! - **Balcony bands**: a slab and a rail at every upper storey, cut from
//!   the full face so a row of copies shows one unbroken band, and a
//!   coloured divider between each room's balcony (`Pick`ed colour).
//! - **Sections of twelve metres**: a lobby door on each, under the first
//!   balcony; a trading slab has shops between the doors, one of rooms has
//!   the lobby's glass.
//! - **A sun deck** behind the parapet, a row of canvas shades on it over
//!   each section.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    AWNING_RED, AWNING_TEAL, AWNING_WHITE, LAMP_WARM, STEEL_GREY, STUCCO_SAND, STUCCO_WHITE,
    canvas, concrete, enamel, steel, stucco,
};

/// The block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "coastal_resort_street_block",
    name: "Beachfront Hotel Slab",
    description: "A sixties beachfront hotel slab: a continuous balcony on every storey, rooms \
                  of glass in a pastel frame, a glazed lobby storey on the promenade, and a \
                  sun deck under canvas shades.",
    themes: &[ThemeArchetype::CoastalResort],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // The lobby storey, and the hotel's storeys of rooms.
    storey_m: (4.4, 3.0),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BAA_0002,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Balcony", "Shade", "Post",
    ],
};

/// The slab's pastels, its white concrete, steel rails and dividers, and
/// the canvas over its roof.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.90, 0.91, 0.90],
        panes: (2, 1),
        room: LAMP_WARM,
        shop: [1.0, 0.94, 0.80],
    });
    m.extend([
        ("White".to_string(), stucco(STUCCO_WHITE)),
        ("Sand".to_string(), stucco(STUCCO_SAND)),
        ("Aqua".to_string(), stucco([0.62, 0.84, 0.84])),
        ("Coral".to_string(), stucco([0.94, 0.66, 0.56])),
        ("Slab".to_string(), concrete([0.90, 0.89, 0.86])),
        ("Rail".to_string(), steel(STEEL_GREY)),
        ("DivTeal".to_string(), enamel(AWNING_TEAL)),
        ("DivYellow".to_string(), enamel([0.94, 0.76, 0.22])),
        ("DivBlue".to_string(), enamel([0.22, 0.42, 0.70])),
        ("Door".to_string(), enamel([0.18, 0.22, 0.26])),
        ("ShadeRed".to_string(), canvas(AWNING_RED, AWNING_WHITE)),
        ("ShadeTeal".to_string(), canvas(AWNING_TEAL, AWNING_WHITE)),
        ("Deck".to_string(), concrete([0.56, 0.55, 0.52])),
    ]);
    m
}
