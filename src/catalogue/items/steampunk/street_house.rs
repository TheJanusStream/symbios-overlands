//! Berlin's street house in the steampunk theme's dress (#1598): a
//! brick-and-iron townhouse of the gaslight age - sooty brick between
//! riveted iron bands, iron-framed shopfronts under brass-lettered boards,
//! copper downpipes the height of the front, a glowing clock dial in the
//! attic and a riveted iron mansard bristling with copper stacks.
//!
//! - **One house, one brick**, rolled once at the lot; iron, brass and
//!   copper name their own.
//! - **Shops or a workshop home.** A trading house has iron-framed
//!   shopfronts either side of its door under a fascia board with a brass
//!   rail; a house of homes has tall windows over a high sill.
//! - **Riveted bands**: an iron band at every floor the whole frontage, so
//!   a row's bands run on unbroken, and each window under an iron lintel.
//! - **Pipework**: a copper downpipe at each end of the front, from the
//!   pavement to the cornice, and a brass boss at every floor along it.
//! - **The crown** is an iron cornice and a mansard, `Pick`ed once in
//!   riveted iron plate or in slate; a clock dial glows in its front, and
//!   copper stacks stand on its ridge.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRASS, BRICK_SOOT, COPPER_ORANGE, GAUGE_AMBER, IRON_DARK, LAMP_GAS, WOOD_BROWN, brass, brick,
    copper, iron, plank, slate,
};

/// The house (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "steampunk_street_house",
    name: "Brass and Brick Townhouse",
    description: "A gaslight townhouse: sooty brick between riveted iron bands, iron-framed \
                  shopfronts, copper downpipes, a glowing clock dial and an iron mansard \
                  crowned with copper stacks.",
    themes: &[ThemeArchetype::Steampunk],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // A shop's tall ground storey, then the house's storeys.
    storey_m: (4.4, 3.5),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BAE_0001,
    materials,
    round_meshes: &["Pipe", "Stack", "Dial"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Band", "Cornice", "Fascia", "Pipe", "Stack",
        "Dial", "Balcony",
    ],
};

/// Sooty and red brick, riveted iron, polished brass, weathered copper,
/// slate, and the gas-lit rooms behind brass-framed glass.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.50, 0.38, 0.16],
        panes: (2, 3),
        room: LAMP_GAS,
        shop: GAUGE_AMBER,
    });
    m.extend([
        ("BrickSoot".to_string(), brick(BRICK_SOOT)),
        ("BrickRed".to_string(), brick([0.52, 0.24, 0.18])),
        ("BrickBrown".to_string(), brick([0.40, 0.30, 0.22])),
        ("BrickYellow".to_string(), brick([0.64, 0.52, 0.34])),
        ("Iron".to_string(), iron(IRON_DARK)),
        ("IronPlate".to_string(), iron([0.30, 0.30, 0.32])),
        ("Brass".to_string(), brass(BRASS)),
        ("Copper".to_string(), copper(COPPER_ORANGE)),
        ("Slate".to_string(), slate([0.24, 0.25, 0.28])),
        ("Door".to_string(), plank(WOOD_BROWN)),
        ("DoorGreen".to_string(), plank([0.18, 0.30, 0.22])),
        ("Sign".to_string(), iron([0.10, 0.20, 0.16])),
        (
            "Dial".to_string(),
            crate::catalogue::items::util::glow(GAUGE_AMBER, 1.8),
        ),
    ]);
    m
}
