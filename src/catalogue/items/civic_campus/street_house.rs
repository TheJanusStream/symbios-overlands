//! Berlin's street house in the civic campus's dress (#1598): the civic
//! townhouse of a university quarter - an institute, a society, a faculty
//! house - in dressed ashlar over a rusticated ground floor, marble
//! pilasters between its bays, pedimented window hoods, a porticoed door,
//! and a copper roof behind a balustraded cornice.
//!
//! - **One house, one stone**, rolled once at the lot: pale, buff or grey
//!   ashlar, or red brick with stone dressings; the ground floor is a
//!   darker rusticated stone whatever it is.
//! - **The portico**: the door up marble steps between two round columns
//!   carrying an entablature and a copper-roofed pediment, standing out
//!   from the front.
//! - **Shops or a hall.** A trading house has a bookshop's or a cafe's
//!   windows either side of its portico under a painted sign band; one
//!   that does not has tall windows over a high sill.
//! - **Pilasters and hoods.** Marble pilasters stand the height of the
//!   upper storeys between its bays, and each window has a marble sill and
//!   a hood over it, a pediment on the first storey's.
//! - **The crown**: a marble cornice the whole frontage, an attic over it,
//!   and a copper mansard along the street or a flat roof, `Pick`ed once.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRICK_RED, COPPER_VERDIGRIS, LAMP_WARM, MARBLE_WHITE, NOTICE_GREEN, STONE_PALE, WINDOW_WARM,
    brick, concrete, copper, marble, painted, plank, stone,
};

/// The townhouse (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "civic_campus_street_house",
    name: "Civic Townhouse",
    description: "An institute's townhouse in dressed ashlar: a porticoed door between two \
                  columns, marble pilasters and window hoods, a balustraded cornice and a \
                  copper roof.",
    themes: &[ThemeArchetype::CivicCampus],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // The piano nobile's tall rooms: the ground storey, and every storey
    // above it.
    storey_m: (4.6, 3.8),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BAC_0001,
    materials,
    round_meshes: &["Column"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Cornice", "Column", "Fascia",
    ],
};

/// Three ashlars and a red brick, the rusticated base, marble dressings,
/// the door, the sign band and the copper roof.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.90, 0.89, 0.85],
        panes: (2, 4),
        room: WINDOW_WARM,
        shop: LAMP_WARM,
    });
    m.extend([
        ("StonePale".to_string(), stone(STONE_PALE)),
        ("StoneBuff".to_string(), stone([0.74, 0.66, 0.52])),
        ("StoneGrey".to_string(), stone([0.58, 0.58, 0.56])),
        ("BrickRed".to_string(), brick(BRICK_RED)),
        ("Rustic".to_string(), stone([0.50, 0.48, 0.44])),
        ("Marble".to_string(), marble(MARBLE_WHITE)),
        ("Door".to_string(), plank([0.30, 0.20, 0.13])),
        ("Fascia".to_string(), painted(NOTICE_GREEN)),
        ("Copper".to_string(), copper(COPPER_VERDIGRIS)),
        ("Deck".to_string(), concrete([0.30, 0.30, 0.31])),
    ]);
    m
}
