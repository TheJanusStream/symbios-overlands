//! Berlin's long block in the industrial park's dress (#1598): the
//! Gewerbehof - the many-storeyed factory block of the 1920s, its concrete
//! frame showing on the front as a grid of columns and floor bands, filled
//! with ribbons of steel-framed windows over brick or concrete spandrels,
//! loading bays at street level and a brick chimney on the roof.
//!
//! - **One block, one infill**, rolled once at the lot: the spandrels are
//!   red brick, yellow brick or concrete, and the frame is concrete
//!   whatever they are.
//! - **The frame**: concrete columns proud of the front its full height on
//!   a six-metre grid, and a floor band at every storey between them.
//! - **The ribbons**: each bay of each storey a run of steel-framed windows
//!   over its spandrel, so the factory floors read as the open halls they
//!   are.
//! - **A working ground floor**: loading bays with roller doors under a
//!   steel canopy, a stair door between them; a trading block has workshop
//!   showrooms in their place.
//! - **A flat roof** behind a parapet with a concrete coping, a stair
//!   tower, and a round brick chimney where the block has one (`Pick`ed
//!   once).

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRICK_DARK, CONCRETE_GREY, LAMP_AMBER, PIPE_GREY, STEEL_BLUE, WINDOW_LIT, brick, cladding,
    concrete, tank_steel,
};

/// The factory block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "industrial_park_street_block",
    name: "Factory Floors Block",
    description: "A many-storeyed factory block: a concrete frame filled with ribbons of \
                  steel-framed windows over brick spandrels, loading bays with roller doors, \
                  and a brick chimney on the roof.",
    themes: &[ThemeArchetype::IndustrialPark],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // The loading floor, then the factory floors over it.
    storey_m: (4.8, 3.9),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BA9_0002,
    materials,
    round_meshes: &["Stack"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Frame", "Canopy", "Stack",
    ],
};

/// The concrete frame, three infills, the steel of the windows, doors and
/// canopies, the roller doors' cladding and the chimney's brick.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.22, 0.24, 0.25],
        panes: (3, 3),
        room: WINDOW_LIT,
        shop: LAMP_AMBER,
    });
    m.extend([
        ("Frame".to_string(), concrete([0.70, 0.69, 0.66])),
        ("BrickRed".to_string(), brick([0.52, 0.27, 0.20])),
        ("BrickYellow".to_string(), brick([0.74, 0.62, 0.42])),
        ("Concrete".to_string(), concrete(CONCRETE_GREY)),
        ("Base".to_string(), concrete([0.40, 0.40, 0.41])),
        ("Steel".to_string(), tank_steel(PIPE_GREY)),
        ("Roller".to_string(), cladding(STEEL_BLUE)),
        ("Door".to_string(), tank_steel([0.22, 0.30, 0.26])),
        ("Stack".to_string(), brick(BRICK_DARK)),
        ("Deck".to_string(), concrete([0.26, 0.26, 0.27])),
    ]);
    m
}
