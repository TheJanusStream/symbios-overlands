//! Berlin's hall in the farm country's dress (#1600): the big barn of a
//! farm or a co-op - a machine shed or a hay barn of red or grey boards on
//! a fieldstone plinth, its big sliding doors braced in white, or, where it
//! trades, a feed-and-seed store with produce windows under a tin canopy.
//!
//! - **One barn, one board**, rolled once at the lot: barn red, weathered
//!   grey or ribbed green metal; its sides are party walls, blank, so halls
//!   stand flush in a row.
//! - **A barn or a store.** A barn (`Trade` 0) has a door for people at one
//!   end and big sliding doors along the rest, each a pair of plank leaves
//!   in a white frame with a white cross brace over a concrete apron, some
//!   bays blank, under a row of small loft windows; a store (`Trade` 1) has
//!   produce windows on the plinth between plank doors, a tin canopy along
//!   the front on posts and a painted sign board over it.
//! - **A hayloft**, where it has two storeys: loft windows either side of a
//!   loft door, front and back.
//! - **The roof**: a gambrel along the street where the barn is shallow
//!   enough for one, its break set from the depth; a low gable of ribbed
//!   metal over a deeper one. Red or grey metal, `Pick`ed once, and a row
//!   of ventilator cupolas along the ridge where the barn has them.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BARN_RED, CONCRETE_PALE, LAMP_WARM, ROOF_GREY, STONE_GREY, TRACTOR_GREEN, TRIM_WHITE,
    WOOD_GREY, barn_board, concrete, enamel, metal_roof, stone, weathered,
};

/// The barn hall (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "rural_farmland_street_hall",
    name: "Board Barn Hall",
    description: "A big farm barn of red or grey boards on a fieldstone plinth: white-braced \
                  sliding doors under loft windows and a gambrel or low metal roof - or, where \
                  it trades, a feed-and-seed store under a tin canopy.",
    themes: &[ThemeArchetype::RuralFarmland],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A barn's floor, high enough for a combine, and a hayloft over it.
    storey_m: (5.5, 3.6),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BA8_0005,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Canopy", "Post", "Fascia",
    ],
};

/// Barn boards and ribbed metal, fieldstone, white trim, plank doors,
/// concrete aprons, the store's sign, and red or grey roofing.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: TRIM_WHITE,
        panes: (2, 2),
        room: LAMP_WARM,
        shop: [1.0, 0.88, 0.62],
    });
    m.extend([
        ("BarnRed".to_string(), barn_board(BARN_RED)),
        ("BarnGrey".to_string(), weathered(WOOD_GREY)),
        ("MetalGreen".to_string(), metal_roof([0.24, 0.36, 0.26])),
        ("Stone".to_string(), stone(STONE_GREY)),
        ("Trim".to_string(), barn_board(TRIM_WHITE)),
        ("Door".to_string(), barn_board([0.36, 0.24, 0.15])),
        ("Apron".to_string(), concrete(CONCRETE_PALE)),
        ("Post".to_string(), weathered(WOOD_GREY)),
        ("Sign".to_string(), enamel(TRACTOR_GREEN)),
        ("MetalRoof".to_string(), metal_roof([0.46, 0.20, 0.16])),
        ("MetalGrey".to_string(), metal_roof(ROOF_GREY)),
        ("Canopy".to_string(), metal_roof(ROOF_GREY)),
    ]);
    m
}
