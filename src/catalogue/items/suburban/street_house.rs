//! Berlin's street house in the suburban theme's dress (#1598): the
//! stacked-porch apartment house of a streetcar suburb - a triple-decker
//! grown to the Altbau's three to seven storeys - clad in painted lap
//! siding under an asphalt-shingle gable.
//!
//! - **One house, one siding colour.** The colour is rolled once at the
//!   lot and inherited by every wall below it; the corner boards, the
//!   porches, the doors and the roof name their own.
//! - **A stack of porches.** One bay of the front, `Pick`ed to the left,
//!   the middle or the right, carries a porch on every storey: a deck, a
//!   rail and two posts, each storey's deck the roof of the porch below
//!   it, and a porch roof over the top one. Behind each porch is that
//!   flat's own door.
//! - **Shops or a home.** A trading house has a corner-store front on its
//!   ground floor - shop windows on a brick plinth under a striped awning
//!   and a lit sign band; a house of homes has a raised front porch under
//!   the stack and shuttered windows either side.
//! - **Corner boards and a frieze.** White boards stand at both ends of the
//!   front and a frieze board runs under the eaves, so a row of houses
//!   shows each house's edges as a suburb does.
//! - **The roof is a gable**, `Pick`ed once: turned to the street, its
//!   gable end over the frieze, or along it, its ends the party walls.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRICK_TAN, PORCH_WARM, ROOF_GREY, SIDING_BLUE, SIDING_CREAM, SIDING_SAGE, SIGN_GLOW,
    WOOD_BROWN, WOOD_WHITE, brick, concrete, enamel, shingle, siding, wood,
};

/// The house (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "suburban_street_house",
    name: "Suburban Porch House",
    description: "A stacked-porch apartment house in painted lap siding: a porch on every \
                  storey, white corner boards, a shingled gable, and a corner store where \
                  the street trades.",
    themes: &[ThemeArchetype::Suburban],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // A corner store's tall ground storey, then each flat's modest rooms.
    storey_m: (4.0, 3.0),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BA7_0001,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Porch", "Fascia", "Awning", "Sign",
    ],
};

/// Painted lap siding in the suburb's colours, white trim, grey porch
/// decks, the doors, the store's brick, awning and sign, and the shingles.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.93, 0.93, 0.90],
        panes: (1, 2),
        room: PORCH_WARM,
        shop: [1.0, 0.94, 0.80],
    });
    m.extend([
        ("SidingBlue".to_string(), siding(SIDING_BLUE)),
        ("SidingCream".to_string(), siding(SIDING_CREAM)),
        ("SidingSage".to_string(), siding(SIDING_SAGE)),
        ("SidingYellow".to_string(), siding([0.86, 0.76, 0.48])),
        ("SidingGrey".to_string(), siding([0.62, 0.64, 0.65])),
        ("Trim".to_string(), wood(WOOD_WHITE)),
        ("Deck".to_string(), wood([0.50, 0.50, 0.48])),
        ("Base".to_string(), concrete([0.58, 0.57, 0.54])),
        ("DoorRed".to_string(), enamel([0.50, 0.14, 0.12])),
        ("DoorBlue".to_string(), enamel([0.14, 0.22, 0.36])),
        ("DoorWood".to_string(), wood(WOOD_BROWN)),
        ("ShopBrick".to_string(), brick(BRICK_TAN)),
        ("Awning".to_string(), enamel([0.16, 0.38, 0.24])),
        ("AwningRed".to_string(), enamel([0.62, 0.16, 0.14])),
        ("Fascia".to_string(), enamel([0.12, 0.15, 0.20])),
        (
            "SignLit".to_string(),
            crate::catalogue::items::util::glow(SIGN_GLOW, 1.6),
        ),
        ("Shingle".to_string(), shingle(ROOF_GREY)),
    ]);
    m
}
