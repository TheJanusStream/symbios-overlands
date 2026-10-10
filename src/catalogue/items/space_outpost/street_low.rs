//! Berlin's low building in the outpost's dress (#1598): where it houses
//! people, a row of crew cabins under one pressure vault; where it trades,
//! a supply depot off the landing pad, the kind of shed that fills a gap in
//! a colony street with one or two storeys.
//!
//! - **The cabins**: as many as the lot's frontage holds, each a viewport
//!   and an airlock hatch under a status lamp - every crew its own hatch
//!   colour - and, where it has two storeys, two ports over them; one hull
//!   livery rolled for the row, and a pressure vault along the street - a
//!   turned hull laid on its side, a steel hoop at every cabin's end -
//!   stopping short of the party walls.
//! - **The depot**: supply counters in bays, a hatch where a bay is wide
//!   enough, a deep steel canopy along the whole front and a lit sign band
//!   over it; a ribbon of control-room ports where it has two storeys; a
//!   flat roof behind a hazard-striped parapet with a comms mast and its
//!   red beacon.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::catalogue::items::util::glow;
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BEACON_RED, HAZARD_YELLOW, HULL_PANEL, HULL_WHITE, INTERIOR_WARM, PAD_GREY, STATUS_GREEN,
    STEEL_DARK, VIEWPORT_LIT, concrete, hull, painted, steel,
};

/// The low building (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "space_outpost_street_low",
    name: "Outpost Crew Cabins",
    description: "A row of crew cabins under one hull vault, each with its own airlock hatch, \
                  or - where the street trades - a supply depot with a deep canopy, a lit sign \
                  band and a beacon mast.",
    themes: &[ThemeArchetype::SpaceOutpost],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // A depot's tall ground storey, and a cabin's upper deck.
    storey_m: (3.6, 2.9),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BB0_0003,
    materials,
    round_meshes: &["Mast", "Vault", "Hoop"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Frame", "Ring", "Fascia", "Canopy", "Mast",
        "Vault", "Hoop",
    ],
};

/// Hull plating in three liveries, the cabins' hatch colours, and the
/// depot's steel, hazard paint and lamps.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.52, 0.55, 0.60],
        panes: (2, 1),
        room: INTERIOR_WARM,
        shop: [0.80, 0.96, 1.0],
    });
    m.extend([
        ("HullWhite".to_string(), hull(HULL_WHITE)),
        ("HullGrey".to_string(), hull(HULL_PANEL)),
        ("HullSand".to_string(), hull([0.80, 0.76, 0.66])),
        ("Steel".to_string(), steel(STEEL_DARK)),
        ("Frame".to_string(), steel([0.50, 0.52, 0.56])),
        ("Hazard".to_string(), painted(HAZARD_YELLOW)),
        ("HazardDark".to_string(), painted([0.10, 0.10, 0.11])),
        ("HatchGrey".to_string(), steel([0.40, 0.43, 0.48])),
        ("HatchOrange".to_string(), painted([0.80, 0.38, 0.10])),
        ("HatchBlue".to_string(), painted([0.16, 0.30, 0.56])),
        ("Sign".to_string(), glow(VIEWPORT_LIT, 1.4)),
        ("Status".to_string(), glow(STATUS_GREEN, 2.0)),
        ("Beacon".to_string(), glow(BEACON_RED, 2.4)),
        ("Deck".to_string(), concrete(PAD_GREY)),
    ]);
    m
}
