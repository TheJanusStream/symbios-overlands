//! Berlin's low building in the suburban theme's dress (#1598): where it
//! houses people, a row of sided townhouses, each with its own front-facing
//! gable, its garage door and its porch; where it trades, a strip mall of
//! shops under a shingled mansard fascia.
//!
//! - **The townhouse row**: as many units as the lot's frontage holds, each
//!   a garage door, and a front door and a window behind a porch on two
//!   posts, the units mirrored one to the next; where it has two storeys,
//!   two windows over them. Each unit has its own gable to the street, so
//!   the row reads as houses, not as a shed - or, `Pick`ed in its place,
//!   one gable runs along the street. One siding colour rolled for the
//!   row; each door its own.
//! - **The strip mall**: shopfronts in bays on a brick plinth, a door where
//!   a bay is wide enough, a deep canopy along the front with a shingled
//!   mansard fascia and a lit sign band over it; a row of office windows
//!   where it has two storeys; a flat roof behind a parapet.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRICK_TAN, PORCH_WARM, ROOF_GREY, SIDING_BLUE, SIDING_CREAM, SIDING_SAGE, SIGN_GLOW,
    WOOD_BROWN, WOOD_WHITE, brick, concrete, enamel, render, shingle, siding, wood,
};

/// The low building (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "suburban_street_low",
    name: "Suburban Townhouse Row",
    description: "A row of sided townhouses, each with its own front gable, garage door and \
                  porch - or, where the street trades, a strip mall under a shingled \
                  mansard fascia and a lit sign band.",
    themes: &[ThemeArchetype::Suburban],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // A shop's tall ground storey, and a townhouse's bedrooms over it.
    storey_m: (3.8, 2.8),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BA7_0003,
    materials,
    round_meshes: &[],
    solid_meshes: &["Wall", "Door", "Pane", "Roof", "Trim", "Canopy", "Sign"],
};

/// The row's siding and the mall's brick and render, garage and front
/// doors, white trim, the canopy, the signs and the shingles.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.93, 0.93, 0.90],
        panes: (1, 2),
        room: PORCH_WARM,
        shop: [1.0, 0.95, 0.86],
    });
    m.extend([
        ("SidingBlue".to_string(), siding(SIDING_BLUE)),
        ("SidingCream".to_string(), siding(SIDING_CREAM)),
        ("SidingSage".to_string(), siding(SIDING_SAGE)),
        ("SidingGrey".to_string(), siding([0.62, 0.64, 0.65])),
        ("MallBrick".to_string(), brick(BRICK_TAN)),
        ("MallRender".to_string(), render([0.84, 0.80, 0.70])),
        ("Trim".to_string(), wood(WOOD_WHITE)),
        ("Base".to_string(), concrete([0.58, 0.57, 0.54])),
        ("Garage".to_string(), enamel([0.86, 0.86, 0.83])),
        ("DoorRed".to_string(), enamel([0.50, 0.14, 0.12])),
        ("DoorBlue".to_string(), enamel([0.14, 0.22, 0.36])),
        ("DoorWood".to_string(), wood(WOOD_BROWN)),
        ("DoorGlass".to_string(), enamel([0.30, 0.32, 0.34])),
        ("Canopy".to_string(), concrete([0.80, 0.79, 0.75])),
        (
            "SignLit".to_string(),
            crate::catalogue::items::util::glow(SIGN_GLOW, 1.6),
        ),
        ("Shingle".to_string(), shingle(ROOF_GREY)),
        ("ShingleBrown".to_string(), shingle([0.36, 0.27, 0.21])),
        ("Deck".to_string(), concrete([0.30, 0.30, 0.31])),
    ]);
    m
}
