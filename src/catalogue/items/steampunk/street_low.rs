//! Berlin's low building in the steampunk theme's dress (#1598): where it
//! trades, a tinker's workshop under an iron gantry; where it houses
//! people, a boiler-house cottage row.
//!
//! - **The workshop**: wide iron-framed shop windows and a double door in
//!   bays between brick piers, an iron gantry beam the whole front on
//!   posts with its hoist, a brass-railed sign board, a sawtooth roof of
//!   amber northlights and a copper stack.
//! - **The cottages**: as many as the frontage holds, each a brass-framed
//!   window and a plank door under an iron hood; a rusted iron roof along
//!   the street and a copper stack to each.
//! - **One brick**, rolled once; an iron band over the ground storey, and
//!   small windows in the upper one where it has two.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRASS, BRICK_SOOT, COPPER_ORANGE, CORRUGATED_RUST, GLASS_AMBER, IRON_DARK, LAMP_GAS,
    WOOD_BROWN, brass, brick, copper, corrugated, glass, iron, plank,
};

/// The low building (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "steampunk_street_low",
    name: "Tinker's Workshop",
    description: "A tinker's brick workshop under an iron gantry, with a sawtooth of amber \
                  northlights and a copper stack - or, where nobody trades, a row of \
                  boiler-house cottages.",
    themes: &[ThemeArchetype::Steampunk],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // A workshop's tall ground storey, and an upper one.
    storey_m: (4.0, 3.0),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BAE_0003,
    materials,
    round_meshes: &["Stack"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Band", "Gantry", "Stack", "Fascia",
    ],
};

/// Brick, iron, brass, copper, rusted sheeting, plank doors and amber
/// northlights.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.50, 0.38, 0.16],
        panes: (3, 2),
        room: LAMP_GAS,
        shop: [1.0, 0.68, 0.36],
    });
    m.extend([
        ("BrickSoot".to_string(), brick(BRICK_SOOT)),
        ("BrickRed".to_string(), brick([0.52, 0.24, 0.18])),
        ("BrickBrown".to_string(), brick([0.40, 0.30, 0.22])),
        ("Iron".to_string(), iron(IRON_DARK)),
        ("Brass".to_string(), brass(BRASS)),
        ("Copper".to_string(), copper(COPPER_ORANGE)),
        ("Sheet".to_string(), corrugated(CORRUGATED_RUST)),
        ("NorthLight".to_string(), glass(GLASS_AMBER, 0.6)),
        ("Door".to_string(), plank(WOOD_BROWN)),
        ("DoorGreen".to_string(), plank([0.18, 0.30, 0.22])),
        ("Sign".to_string(), iron([0.10, 0.20, 0.16])),
    ]);
    m
}
