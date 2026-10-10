//! Berlin's detached house in the industrial park's dress (#1600): the
//! works foreman's house at the edge of the yard - sooty brick, or the
//! corrugated sheet of a converted shed, steel-framed windows on concrete
//! sills under concrete lintels on every side, a steel canopy over the
//! door, and a corrugated roof with a brick stack.
//!
//! - **One house, one skin**, rolled once at the lot - three of the loft
//!   building's bricks, or grey or blue corrugated cladding - and
//!   inherited by every wall; the plinth, the dressings, the steel and the
//!   roof name their own.
//! - **It stands in from its lot.** Its walls stand `Eave` in from every
//!   side, so its eaves and verges reach out to the lot's edges and no
//!   further.
//! - **Windows on all four sides**: wide steel-framed windows of many
//!   panes, each on a concrete sill under a concrete lintel, over a
//!   concrete plinth course; a steel door up a concrete step under a steel
//!   canopy on the front.
//! - **The roof** is `Pick`ed once: a corrugated gable along the street, a
//!   monopitch rising to the back, or a flat deck behind a parapet with a
//!   concrete coping; a brick chimney stack up one side where the house
//!   has one.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRICK_DARK, CONCRETE_GREY, LAMP_AMBER, PIPE_GREY, STEEL_BLUE, WINDOW_LIT, brick, cladding,
    concrete, tank_steel,
};

/// The foreman's house (see the module docs); its rules are
/// `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "industrial_park_street_detached",
    name: "Foreman's Brick House",
    description: "A works foreman's house in its own yard: sooty brick or corrugated sheet, \
                  steel-framed windows under concrete lintels on every side, a steel canopy \
                  over the door, and a corrugated roof with a brick stack.",
    themes: &[ThemeArchetype::IndustrialPark],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // A house's rooms: a little taller below, where it is entered.
    storey_m: (3.2, 3.0),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BA9_0004,
    materials,
    round_meshes: &[],
    solid_meshes: &["Wall", "Door", "Pane", "Roof", "Trim", "Steel", "Chimney"],
};

/// Three sooty bricks and two corrugated sheets, concrete dressings, the
/// steel of the doors and canopy, and the roof's sheeting.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.20, 0.22, 0.24],
        panes: (3, 3),
        room: WINDOW_LIT,
        shop: LAMP_AMBER,
    });
    m.extend([
        ("BrickDark".to_string(), brick(BRICK_DARK)),
        ("BrickRed".to_string(), brick([0.52, 0.27, 0.20])),
        ("BrickBrown".to_string(), brick([0.44, 0.32, 0.24])),
        ("CladGrey".to_string(), cladding([0.62, 0.63, 0.62])),
        ("CladBlue".to_string(), cladding(STEEL_BLUE)),
        ("Concrete".to_string(), concrete(CONCRETE_GREY)),
        ("Steel".to_string(), tank_steel(PIPE_GREY)),
        ("SteelDark".to_string(), tank_steel([0.22, 0.24, 0.26])),
        ("Door".to_string(), tank_steel([0.20, 0.28, 0.36])),
        ("Chimney".to_string(), brick([0.36, 0.22, 0.18])),
        ("RoofSheet".to_string(), cladding([0.48, 0.50, 0.52])),
        ("RoofRust".to_string(), cladding([0.46, 0.30, 0.20])),
        ("Deck".to_string(), concrete([0.26, 0.26, 0.27])),
    ]);
    m
}
