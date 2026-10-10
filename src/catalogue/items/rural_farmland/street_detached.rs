//! Berlin's detached house in the farm country's dress (#1600): the
//! farmhouse alone in its yard - white or cream clapboard, or the market
//! town's red brick, on a fieldstone plinth, green-shuttered windows on
//! every side, a veranda on posts under a tin roof, and a steep gable or
//! half-hipped roof with a brick chimney.
//!
//! - **One house, one cladding**, rolled once at the lot - clapboard in
//!   white, cream or sage, or brick - and inherited by every wall; the
//!   plinth, the trim, the shutters, the doors and the roof name their own.
//! - **It stands in from its lot.** Its walls stand `Eave` in from every
//!   side, so its eaves and verges reach out to the lot's edges and no
//!   further.
//! - **Windows on all four sides**, white-framed on a sill under a head
//!   board, the front's between green shutters where the house has them
//!   (`Pick`ed once).
//! - **The veranda** (`Pick`ed once): along the whole front, a deck on the
//!   plinth, posts and a rail, under a tin roof; or a hood over the door
//!   alone.
//! - **The roof** is `Pick`ed once: a steep gable along the street or a
//!   half-hipped one, shingle or ribbed metal, a brick chimney up one end.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    CLAPBOARD_CREAM, LAMP_WARM, ROOF_GREY, STONE_GREY, TRACTOR_GREEN, TRIM_WHITE, WOOD_GREY, brick,
    clapboard, metal_roof, shingle, stone, weathered,
};

/// The farmhouse (see the module docs); its rules are `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "rural_farmland_street_detached",
    name: "Clapboard Farmhouse",
    description: "A farmhouse alone in its yard: clapboard or brick on a fieldstone plinth, \
                  green-shuttered windows on every side, a veranda on posts under a tin roof, \
                  and a steep gable or half-hipped roof with a brick chimney.",
    themes: &[ThemeArchetype::RuralFarmland],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // A farm kitchen's ground storey, and the bedrooms over it.
    storey_m: (3.1, 2.9),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BA8_0004,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Porch", "Post", "Shutter", "Chimney",
    ],
};

/// Clapboard in three paints and the farm's brick, fieldstone, white trim
/// and green shutters, the doors, the veranda, and the roofs over them.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: TRIM_WHITE,
        panes: (2, 2),
        room: LAMP_WARM,
        shop: [1.0, 0.88, 0.62],
    });
    m.extend([
        ("White".to_string(), clapboard(TRIM_WHITE)),
        ("Cream".to_string(), clapboard(CLAPBOARD_CREAM)),
        ("Sage".to_string(), clapboard([0.62, 0.70, 0.58])),
        ("BrickRed".to_string(), brick([0.56, 0.24, 0.17])),
        ("Stone".to_string(), stone(STONE_GREY)),
        ("Trim".to_string(), clapboard(TRIM_WHITE)),
        ("Shutter".to_string(), clapboard(TRACTOR_GREEN)),
        ("Door".to_string(), clapboard([0.30, 0.20, 0.13])),
        ("DoorGreen".to_string(), clapboard(TRACTOR_GREEN)),
        ("DoorRed".to_string(), clapboard([0.56, 0.16, 0.12])),
        ("Post".to_string(), weathered(WOOD_GREY)),
        ("Deck".to_string(), weathered([0.44, 0.40, 0.34])),
        ("Chimney".to_string(), brick([0.46, 0.22, 0.16])),
        ("Shingle".to_string(), shingle(ROOF_GREY)),
        ("MetalRoof".to_string(), metal_roof([0.46, 0.20, 0.16])),
        ("Tin".to_string(), metal_roof(ROOF_GREY)),
    ]);
    m
}
