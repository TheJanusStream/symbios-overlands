//! Berlin's long block in the civic campus's dress (#1598): the modernist
//! lecture and institute block of a post-war campus - a concrete frame
//! behind a screen of deep vertical fins, panels of brick, stone or
//! concrete between them, a glazed ground floor under a long canopy on
//! round columns, and a deep concrete attic band.
//!
//! - **One block, one panel**, rolled once at the lot: red brick, pale
//!   stone or warm concrete; the fins and the attic are white concrete
//!   whatever it is.
//! - **The fins**: a brise-soleil of concrete fins standing out of the front
//!   the full height of the upper storeys, a window between each pair in
//!   every storey over a panel.
//! - **The entrances**: one to each twelve-metre section, glazed doors up a
//!   step under a canopy that runs the frontage on round columns.
//! - **Shops or a lobby on the street**: a trading block has a campus
//!   bookshop's or a cafe's windows under a painted sign band; one that
//!   does not has a lobby's tall glazing.
//! - **The attic**: a deep band of white concrete over the top storey, a
//!   flat roof behind it and a copper-clad plant house on it.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRICK_RED, CONCRETE_GREY, COPPER_VERDIGRIS, FLAG_RED, LAMP_WARM, STEEL_GREY, STONE_PALE,
    WINDOW_WARM, brick, concrete, copper, painted, steel, stone,
};

/// The lecture block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "civic_campus_street_block",
    name: "Campus Lecture Block",
    description: "A modernist lecture block: a screen of deep concrete fins over brick or \
                  stone panels, a glazed ground floor under a long canopy on round columns, \
                  and a deep white attic band.",
    themes: &[ThemeArchetype::CivicCampus],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // The lobby storey, then the seminar storeys.
    storey_m: (4.4, 3.6),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BAC_0002,
    materials,
    round_meshes: &["Column"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Fin", "Canopy", "Column", "Fascia",
    ],
};

/// White concrete for the fins and attic, three panels, the base, steel
/// doors and copings, the sign band and the plant house's copper.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.30, 0.32, 0.34],
        panes: (2, 2),
        room: WINDOW_WARM,
        shop: LAMP_WARM,
    });
    m.extend([
        ("White".to_string(), concrete([0.84, 0.83, 0.79])),
        ("PanelBrick".to_string(), brick(BRICK_RED)),
        ("PanelStone".to_string(), stone(STONE_PALE)),
        ("PanelConcrete".to_string(), concrete([0.68, 0.62, 0.54])),
        ("Base".to_string(), stone([0.42, 0.42, 0.42])),
        ("Steel".to_string(), steel(STEEL_GREY)),
        ("Door".to_string(), steel([0.26, 0.28, 0.30])),
        ("Fascia".to_string(), painted(FLAG_RED)),
        ("Copper".to_string(), copper(COPPER_VERDIGRIS)),
        ("Deck".to_string(), concrete(CONCRETE_GREY)),
    ]);
    m
}
