//! Berlin's low building in the coastal resort's dress (#1598): where it
//! trades, a run of boardwalk shops - painted plank fronts on a plank
//! boardwalk, a striped awning and a sign board over each shop's window
//! and door, and a little white gable over each on the parapet; where it
//! houses people, a row of beach cottages, each painted its own pastel,
//! under one cedar-shingle roof.
//!
//! - **The shops**: as many as the frontage holds, every shop its own
//!   paint and its own awning stripe; a ribbon of windows where it has two
//!   storeys; a flat roof behind the parapet.
//! - **The cottages**: as many as the frontage holds, each a window and a
//!   door under a hood, every cottage its own colour and every household
//!   its own door; a shingle gable along the street whose ends are the
//!   party walls.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    AWNING_RED, AWNING_TEAL, AWNING_WHITE, DECK_WOOD, DRIFT_GREY, LAMP_WARM, STUCCO_WHITE, canvas,
    concrete, enamel, plank, shingle, stucco,
};

/// The low building (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "coastal_resort_street_low",
    name: "Boardwalk Shops",
    description: "A run of painted plank boardwalk shops, each under its own striped awning \
                  and sign board - or, where nothing trades, a row of pastel beach cottages \
                  under one shingle roof.",
    themes: &[ThemeArchetype::CoastalResort],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // A shop's ground storey, and a cottage's upper one.
    storey_m: (3.6, 2.8),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BAA_0003,
    materials,
    round_meshes: &[],
    solid_meshes: &["Wall", "Door", "Pane", "Roof", "Trim", "Canopy", "Fascia"],
};

/// Painted planks in the beach's pastels, white trim, striped canvas, and
/// the shingle and decks over them.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.93, 0.92, 0.88],
        panes: (2, 2),
        room: LAMP_WARM,
        shop: [1.0, 0.92, 0.74],
    });
    m.extend([
        ("PlankMint".to_string(), plank([0.62, 0.82, 0.72])),
        ("PlankPink".to_string(), plank([0.90, 0.66, 0.66])),
        ("PlankLemon".to_string(), plank([0.92, 0.84, 0.54])),
        ("PlankSky".to_string(), plank([0.60, 0.76, 0.88])),
        ("PlankWhite".to_string(), plank([0.90, 0.89, 0.85])),
        ("Trim".to_string(), stucco(STUCCO_WHITE)),
        ("Door".to_string(), enamel([0.16, 0.42, 0.44])),
        ("DoorRed".to_string(), enamel([0.72, 0.20, 0.18])),
        ("DoorNavy".to_string(), enamel([0.16, 0.24, 0.44])),
        ("AwningRed".to_string(), canvas(AWNING_RED, AWNING_WHITE)),
        ("AwningTeal".to_string(), canvas(AWNING_TEAL, AWNING_WHITE)),
        (
            "AwningYellow".to_string(),
            canvas([0.94, 0.78, 0.22], AWNING_WHITE),
        ),
        ("Sign".to_string(), enamel([0.16, 0.30, 0.52])),
        ("Post".to_string(), plank(DECK_WOOD)),
        ("Shingle".to_string(), shingle(DRIFT_GREY)),
        ("Deck".to_string(), concrete([0.56, 0.55, 0.52])),
    ]);
    m
}
