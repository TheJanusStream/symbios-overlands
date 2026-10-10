//! Berlin's street house in the industrial park's dress (#1598): the brick
//! loft building of a Hinterhof workshop quarter - tall storeys of big
//! steel-framed windows between brick piers, concrete lintels and sills,
//! a loading door beside the stair door, and a timber water tank on steel
//! legs on the roof.
//!
//! - **One building, one brick**, rolled once at the lot; the piers, the
//!   parapet and the walls between the windows all inherit it.
//! - **Piers and bays.** Brick piers stand proud of the front its full
//!   height, and between them each storey has one big steel-framed window
//!   on a concrete sill under a concrete lintel: the loft's grid.
//! - **A working ground floor.** A trading building has showroom windows on
//!   a concrete plinth either side of its door; one of homes has a roller
//!   loading door and windows set high over a plinth. A concrete band runs
//!   the frontage over it.
//! - **A stepped parapet** with a concrete coping crowns the front, and a
//!   flat deck behind it carries a round timber water tank on steel legs
//!   where the building has one (`Pick`ed once).

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRICK_DARK, CONCRETE_GREY, LAMP_AMBER, PIPE_GREY, RUST_BROWN, STEEL_BLUE, WINDOW_LIT, brick,
    cladding, concrete, rust, tank_steel, timber,
};

/// The loft building (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "industrial_park_street_house",
    name: "Brick Loft Building",
    description: "A brick loft building: big steel-framed windows between proud piers, \
                  concrete lintels, a loading door or a showroom at street level, and a \
                  timber water tank on the roof.",
    themes: &[ThemeArchetype::IndustrialPark],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // A loft's tall workrooms: the ground storey, and every storey above.
    storey_m: (4.6, 4.0),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BA9_0001,
    materials,
    round_meshes: &["Tank"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Fascia", "Tank", "Steel",
    ],
};

/// Three sooty bricks, concrete dressings, the steel of the window frames,
/// doors and tank legs, the tank's staves and its rusty lid.
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
        ("Concrete".to_string(), concrete(CONCRETE_GREY)),
        ("Steel".to_string(), tank_steel(PIPE_GREY)),
        ("SteelDark".to_string(), tank_steel([0.22, 0.24, 0.26])),
        ("Roller".to_string(), cladding(STEEL_BLUE)),
        ("Tank".to_string(), timber([0.42, 0.30, 0.20])),
        ("TankLid".to_string(), rust(RUST_BROWN)),
        ("Fascia".to_string(), tank_steel([0.16, 0.24, 0.20])),
        ("Deck".to_string(), concrete([0.26, 0.26, 0.27])),
    ]);
    m
}
