//! Berlin's low building in the sports theme's dress (#1598): where it
//! houses people, a row of changing rooms along a ground's edge - a door
//! and a high window to each, under a mono-pitch roof; where it trades, a
//! club kiosk with a striped awning over its counters and a scoreboard
//! for a sign.
//!
//! - **The changing rooms**: block walls painted once for the row, every
//!   room its own door in the club's colour (`Pick`ed once) under a deep
//!   canopy along the whole front, a bench rail under the high windows;
//!   where it has two storeys, a clubroom's windows over them.
//! - **The kiosk**: serving hatches over counters either side of a door,
//!   an awning in the club's colour and white stripes, and a lit score
//!   sign on the parapet over it; a flat roof behind the parapet.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::catalogue::items::util::glow;
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    ASPHALT_DARK, CONCRETE_GREY, CORRUGATED_GREY, LINE_WHITE, SCORE_LIT, STEEL_GREY, asphalt,
    concrete, corrugated, enamel, painted, steel,
};

/// The low building (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "sports_rec_street_low",
    name: "Changing Rooms",
    description: "A row of changing rooms under a mono-pitch roof, a door in the club's colour \
                  to each - or, where the street trades, a club kiosk under a striped awning.",
    themes: &[ThemeArchetype::SportsRec],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // A kiosk's or changing room's ground storey, a clubroom over it.
    storey_m: (3.6, 3.0),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BAD_0003,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Band", "Canopy", "Awning", "Counter", "Fascia",
        "Sign", "Rail",
    ],
};

/// Painted block in white, cream and grey, the club's four colours, the
/// roof's sheet, and the kiosk's counters and lit sign.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.90, 0.91, 0.92],
        panes: (2, 1),
        room: [1.0, 0.90, 0.72],
        shop: [1.0, 0.92, 0.74],
    });
    m.extend([
        ("White".to_string(), painted([0.88, 0.87, 0.84])),
        ("Cream".to_string(), painted([0.84, 0.80, 0.68])),
        ("Block".to_string(), concrete(CONCRETE_GREY)),
        ("TeamRed".to_string(), enamel([0.80, 0.12, 0.10])),
        ("TeamBlue".to_string(), enamel([0.08, 0.30, 0.74])),
        ("TeamGreen".to_string(), enamel([0.08, 0.52, 0.22])),
        ("TeamOrange".to_string(), enamel([0.96, 0.46, 0.06])),
        ("Stripe".to_string(), painted(LINE_WHITE)),
        ("Sheet".to_string(), corrugated(CORRUGATED_GREY)),
        ("Steel".to_string(), steel(STEEL_GREY)),
        ("Counter".to_string(), enamel([0.30, 0.31, 0.33])),
        ("Deck".to_string(), asphalt(ASPHALT_DARK)),
        ("Score".to_string(), glow(SCORE_LIT, 1.8)),
    ]);
    m
}
