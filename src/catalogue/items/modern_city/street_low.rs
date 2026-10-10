//! Berlin's low building in the modern city's dress (#1598): where it
//! houses people, a terrace of Siedlung cottages of the 1920s under one
//! tiled roof; where it trades, a shop pavilion of the kind that fills a
//! gap in a Berlin street with one or two storeys.
//!
//! - **The terrace**: as many cottages as the lot's frontage holds, each a
//!   window and a door under a hood - every household its own door
//!   colour - and, where it has two storeys, two windows over them; one
//!   render rolled for the terrace, and a tiled gable along the street
//!   whose ends are the party walls.
//! - **The pavilion**: shopfronts in bays, a door where a bay is wide
//!   enough, a deep canopy along the whole front and a sign band over it;
//!   a ribbon of office windows where it has two storeys; a flat roof
//!   behind a parapet.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{LAMP_WARM, brick, concrete, enamel, slate, steel, stucco, timber};

/// The low building (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "city_street_low",
    name: "City Low Building",
    description: "A terrace of Siedlung cottages under a tiled gable, or - where the street \
                  trades - a shop pavilion with a deep canopy and a sign band.",
    themes: &[ThemeArchetype::ModernCity],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // A shop's tall ground storey, and a cottage's upper one.
    storey_m: (3.6, 2.9),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BA0_0003,
    materials,
    round_meshes: &[],
    solid_meshes: &["Wall", "Door", "Pane", "Roof", "Trim", "Canopy", "Fascia"],
};

/// The cottages' render and their doors, the pavilion's panels and its
/// metalwork, and the roofs over both.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.92, 0.92, 0.90],
        panes: (2, 2),
        room: LAMP_WARM,
        shop: [0.98, 0.95, 0.86],
    });
    m.extend([
        ("Lime".to_string(), stucco([0.88, 0.86, 0.78])),
        ("Cream".to_string(), stucco([0.86, 0.80, 0.66])),
        ("Ochre".to_string(), stucco([0.82, 0.66, 0.42])),
        ("Brick".to_string(), brick([0.55, 0.30, 0.22])),
        ("Stone".to_string(), concrete([0.62, 0.61, 0.58])),
        ("DoorGreen".to_string(), enamel([0.13, 0.30, 0.22])),
        ("DoorRed".to_string(), enamel([0.52, 0.14, 0.12])),
        ("DoorWood".to_string(), timber([0.38, 0.25, 0.15])),
        ("Tiles".to_string(), slate([0.50, 0.24, 0.18])),
        ("PanelWhite".to_string(), stucco([0.90, 0.90, 0.88])),
        ("PanelGrey".to_string(), concrete([0.55, 0.56, 0.57])),
        ("DoorMetal".to_string(), steel([0.40, 0.42, 0.44])),
        ("Metal".to_string(), steel([0.30, 0.32, 0.34])),
        ("Sign".to_string(), enamel([0.12, 0.22, 0.36])),
        ("Deck".to_string(), concrete([0.18, 0.18, 0.19])),
    ]);
    m
}
