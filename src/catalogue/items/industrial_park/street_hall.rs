//! Berlin's hall in the industrial park's dress (#1600): the factory hall
//! itself - brick piers marching along its front, tall steel-framed windows
//! of many panes between them over a concrete plinth wall, roller doors in
//! some bays, under a sawtooth of northlights or a gable with a glazed
//! monitor along its ridge.
//!
//! - **One hall, one skin**, rolled once at the lot: one of the loft
//!   building's three bricks, or corrugated cladding; its sides are party
//!   walls, blank, so halls stand flush in a row.
//! - **A works or a trade counter.** A works hall (`Trade` 0) has a steel
//!   door for people at one end and bays between brick piers along the
//!   rest, each a roller door over a concrete apron or a tall window over
//!   the plinth wall; a trade counter (`Trade` 1) - a builders' merchant, a
//!   tool hire - has showroom windows on a concrete plinth either side of
//!   its doors under a painted steel fascia. Over both, the bays' high
//!   windows run up to a concrete band.
//! - **An upper storey**, where it has one, has offices: a ribbon of steel
//!   windows front and back.
//! - **The roof** is `Pick`ed once: shed teeth across the street, so their
//!   saw profile is the hall's front, each one's glazed northlight facing
//!   along the street; or a low gable along the street with a raised,
//!   glazed monitor running its length. A round brick chimney stands at
//!   the back where the works has one.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRICK_DARK, CONCRETE_GREY, LAMP_AMBER, PIPE_GREY, STEEL_BLUE, WINDOW_LIT, brick, cladding,
    concrete, glass, tank_steel,
};

/// The factory hall (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "industrial_park_street_hall",
    name: "Brick Factory Hall",
    description: "A factory hall of brick piers and tall steel-framed windows over a concrete \
                  plinth wall: roller doors or a trade counter's showroom, under a sawtooth \
                  of northlights or a gable with a glazed monitor.",
    themes: &[ThemeArchetype::IndustrialPark],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A works hall's clear height, and an office storey over it.
    storey_m: (6.5, 4.0),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BA9_0005,
    materials,
    round_meshes: &["Stack"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Fascia", "Steel", "Stack",
    ],
};

/// Three sooty bricks and two claddings, concrete, the steel of the doors,
/// frames and fascia, the northlights' glass and the roof's sheeting.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.20, 0.22, 0.24],
        panes: (4, 4),
        room: WINDOW_LIT,
        shop: LAMP_AMBER,
    });
    m.extend([
        ("BrickDark".to_string(), brick(BRICK_DARK)),
        ("BrickRed".to_string(), brick([0.52, 0.27, 0.20])),
        ("BrickBrown".to_string(), brick([0.44, 0.32, 0.24])),
        ("CladBlue".to_string(), cladding(STEEL_BLUE)),
        ("CladGrey".to_string(), cladding([0.62, 0.63, 0.62])),
        ("Concrete".to_string(), concrete(CONCRETE_GREY)),
        ("Steel".to_string(), tank_steel(PIPE_GREY)),
        ("Roller".to_string(), cladding([0.70, 0.70, 0.68])),
        ("Door".to_string(), tank_steel([0.20, 0.28, 0.36])),
        ("Fascia".to_string(), tank_steel([0.86, 0.62, 0.16])),
        ("RoofSheet".to_string(), cladding([0.48, 0.50, 0.52])),
        ("Northlight".to_string(), glass([0.30, 0.38, 0.40], 0.6)),
        ("Stack".to_string(), brick([0.36, 0.22, 0.18])),
    ]);
    m
}
