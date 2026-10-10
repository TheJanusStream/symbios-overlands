//! Berlin's low building in the civic campus's dress (#1598): a classical
//! campus pavilion - a porter's lodge, a faculty house, a small reading
//! room - in red brick or stone behind a colonnade of round columns the
//! whole frontage, under a low copper roof; where it trades, the campus
//! shop or cafe behind the same colonnade, its name on the entablature.
//!
//! - **One pavilion, one wall**, rolled once at the lot; the colonnade, its
//!   stylobate and its entablature are marble whatever it is.
//! - **The colonnade**: round columns on a stylobate a step high, carrying
//!   an entablature tied back into the front over the ground storey, a
//!   wider span in the middle framing the door.
//! - **Rooms or a shop behind it**: a door and tall windows, or a shop's
//!   windows on a plinth and a door, and a painted sign band on the
//!   entablature where it trades. Where it has two storeys, windows with
//!   marble sills run over the colonnade.
//! - **The roof**: a low copper gable along the street, its ends the party
//!   walls, over a marble cornice.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRICK_RED, COPPER_VERDIGRIS, LAMP_WARM, MARBLE_WHITE, NOTICE_GREEN, STONE_PALE, WINDOW_WARM,
    brick, copper, marble, painted, plank, stone,
};

/// The pavilion (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "civic_campus_street_low",
    name: "Campus Colonnade Pavilion",
    description: "A classical campus pavilion of brick or stone behind a marble colonnade \
                  under a low copper roof - a lodge, or where the street trades, the campus \
                  shop with its name on the entablature.",
    themes: &[ThemeArchetype::CivicCampus],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // The colonnade storey, and a storey of rooms over it.
    storey_m: (4.4, 3.4),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BAC_0003,
    materials,
    round_meshes: &["Column"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Cornice", "Column", "Fascia",
    ],
};

/// Red brick and two stones, the marble of the colonnade, the doors, the
/// sign band and the copper roof.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.90, 0.89, 0.85],
        panes: (2, 3),
        room: WINDOW_WARM,
        shop: LAMP_WARM,
    });
    m.extend([
        ("BrickRed".to_string(), brick(BRICK_RED)),
        ("StonePale".to_string(), stone(STONE_PALE)),
        ("StoneBuff".to_string(), stone([0.74, 0.66, 0.52])),
        ("Marble".to_string(), marble(MARBLE_WHITE)),
        ("Door".to_string(), plank([0.30, 0.20, 0.13])),
        ("Fascia".to_string(), painted(NOTICE_GREEN)),
        ("Copper".to_string(), copper(COPPER_VERDIGRIS)),
    ]);
    m
}
