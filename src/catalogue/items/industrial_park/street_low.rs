//! Berlin's low building in the industrial park's dress (#1598): the
//! workshop hall of a Gewerbehof's yard or a gap in the street - corrugated
//! cladding over a concrete plinth wall, a sawtooth roof of northlights,
//! and roller doors where it trades; where it houses people, the same hall
//! converted into live-work lofts, a door and big windows to each.
//!
//! - **One hall, one cladding colour**, rolled once at the lot; the plinth
//!   wall is concrete whatever it is.
//! - **The sawtooth**: shed-roof teeth across the street, so their saw
//!   profile is the hall's front over a painted steel fascia, each one's
//!   glazed northlight facing along the street, as many as the frontage
//!   holds. A shed slopes along its scope's depth, so the grammar turns
//!   the roof's scope a quarter first.
//! - **Roller doors or lofts.** A trading hall has roller doors in every
//!   other bay and a door and a window in the bays between; a hall of
//!   homes has, in each bay, a door up a step and a big steel-framed
//!   window. Where it has two storeys, a row of office windows runs over
//!   them.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    CONCRETE_GREY, CONTAINER_GREEN, CONTAINER_RED, LAMP_AMBER, PIPE_GREY, STEEL_BLUE, WINDOW_LIT,
    cladding, concrete, glass, tank_steel,
};

/// The workshop hall (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "industrial_park_street_low",
    name: "Sawtooth Workshop Hall",
    description: "A corrugated workshop hall on a concrete plinth wall under a sawtooth \
                  roof of northlights: roller doors where the street trades, live-work \
                  lofts where it does not.",
    themes: &[ThemeArchetype::IndustrialPark],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // A hall's tall ground storey, and an office floor over it.
    storey_m: (5.0, 3.2),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BA9_0003,
    materials,
    round_meshes: &[],
    solid_meshes: &["Wall", "Door", "Pane", "Roof", "Trim", "Fascia"],
};

/// Four claddings, the concrete plinth wall, the steel of the doors and
/// fascia, the northlights' glass and the roof's sheeting.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.22, 0.24, 0.25],
        panes: (3, 3),
        room: WINDOW_LIT,
        shop: LAMP_AMBER,
    });
    m.extend([
        ("CladBlue".to_string(), cladding(STEEL_BLUE)),
        ("CladGrey".to_string(), cladding([0.62, 0.63, 0.62])),
        ("CladGreen".to_string(), cladding(CONTAINER_GREEN)),
        ("CladRed".to_string(), cladding(CONTAINER_RED)),
        ("Base".to_string(), concrete(CONCRETE_GREY)),
        ("Steel".to_string(), tank_steel(PIPE_GREY)),
        ("Roller".to_string(), cladding([0.70, 0.70, 0.68])),
        ("Door".to_string(), tank_steel([0.20, 0.28, 0.36])),
        ("Fascia".to_string(), tank_steel([0.86, 0.62, 0.16])),
        ("RoofSheet".to_string(), cladding([0.48, 0.50, 0.52])),
        ("Northlight".to_string(), glass([0.30, 0.38, 0.40], 0.6)),
    ]);
    m
}
