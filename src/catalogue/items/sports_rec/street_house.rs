//! Berlin's street house in the sports theme's dress (#1598): the club
//! house of a sports club that took over an Altbau - a white-rendered or
//! panelled front banded in the club's colours, a clubroom terrace across
//! its first floor, and a five-a-side cage on its roof under floodlights.
//!
//! - **One club, one colour.** The cladding is rolled once at the lot, and
//!   the club's colour is `Pick`ed once: every band, fascia, terrace rail
//!   and the roof cage's frame wear it, so a row of copies reads as a row
//!   of clubs, each in its own strip.
//! - **Banded storeys**: a colour band at every floor line, standing proud
//!   of the full face, so a row's bands meet at the party walls.
//! - **The club terrace**: the first floor's front is one long balcony the
//!   width of the house, a railing in the club's colour, where the house
//!   has one (`Pick`ed once); otherwise windows with sills.
//! - **Shops or club rooms**: a trading house has a sports shop's windows
//!   and a fascia either side of its door; a house of homes has raised
//!   windows over a high sill.
//! - **The roof is a pitch**: a deck painted as a court inside a parapet,
//!   the club's scoreboard lit on the parapet's front, netting on posts
//!   along the front and back of the court, and two floodlight masts.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::catalogue::items::util::glow;
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    CHAIN_GREY, CONCRETE_GREY, CORRUGATED_GREY, COURT_BLUE, FLOOD_LIT, LINE_WHITE, SCORE_LIT,
    STEEL_GREY, chainlink, concrete, corrugated, enamel, painted, steel,
};

/// The house (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "sports_rec_street_house",
    name: "Club House",
    description: "A sports club's town house: a white or panelled front banded in the club's \
                  colour, a clubroom terrace, and a floodlit five-a-side cage on its roof.",
    themes: &[ThemeArchetype::SportsRec],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // A shop's or a clubroom's tall ground storey, then the club's rooms.
    storey_m: (4.2, 3.4),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BAD_0001,
    materials,
    round_meshes: &["Mast"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Band", "Fascia", "Balcony", "Rail", "Fence",
        "Mast", "Lamp", "Sign",
    ],
};

/// White render, board-formed concrete and grey panels; the club's four
/// colours; the steel, netting and painted court of its roof.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.90, 0.91, 0.92],
        panes: (2, 2),
        room: [1.0, 0.90, 0.72],
        shop: [0.98, 0.97, 0.92],
    });
    m.extend([
        ("Render".to_string(), painted([0.88, 0.87, 0.84])),
        ("Concrete".to_string(), concrete(CONCRETE_GREY)),
        ("Panel".to_string(), corrugated(CORRUGATED_GREY)),
        ("TeamRed".to_string(), enamel([0.80, 0.12, 0.10])),
        ("TeamBlue".to_string(), enamel([0.08, 0.30, 0.74])),
        ("TeamGreen".to_string(), enamel([0.08, 0.52, 0.22])),
        ("TeamOrange".to_string(), enamel([0.96, 0.46, 0.06])),
        ("Trim".to_string(), painted(LINE_WHITE)),
        ("Steel".to_string(), steel(STEEL_GREY)),
        ("Net".to_string(), chainlink(CHAIN_GREY)),
        ("Court".to_string(), painted(COURT_BLUE)),
        ("Door".to_string(), enamel([0.14, 0.15, 0.17])),
        ("Lamp".to_string(), glow(FLOOD_LIT, 3.0)),
        ("Score".to_string(), glow(SCORE_LIT, 1.8)),
    ]);
    m
}
