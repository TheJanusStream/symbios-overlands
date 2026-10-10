//! Berlin's hall in the modern city's dress (#1600): the sheds of its
//! commercial and industrial areas - a works hall of profiled steel with
//! roller doors under a ribbon of high windows, or, where it trades, the
//! retail box of an out-of-town store.
//!
//! The reference hall: the conventions every theme's street buildings keep
//! are in [`crate::catalogue::items::street`], and this, with its rules in
//! `street_hall.cga`, is how a hall keeps them.
//!
//! - **One hall, one cladding.** The sheet is rolled once at the lot; its
//!   gable ends are its party walls, blank, so halls stand flush in a row
//!   and a lone hall's ends read as the sheet's own.
//! - **A works or a store.** A works hall (`Trade` 0) has a door for people
//!   at one end and bays of roller doors along the rest, each over a
//!   concrete apron, some bays blank; a store (`Trade` 1) has sliding doors
//!   between shop windows under a deep canopy and its sign, blank sheet
//!   either side. Both have a ribbon of high windows, and an office storey
//!   over the hall, where it has one, another.
//! - **The roof is a works' low gable or sawtooth, a store's flat deck**:
//!   `Pick` decides a works' once - a gable at nine degrees along the
//!   street, or teeth along it, each with its glazed north light - and a
//!   store stands a parapet round a deck.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{LAMP_WARM, concrete, enamel, sheet, steel};

/// The hall (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "city_street_hall",
    name: "City Works Hall",
    description: "A hall of profiled steel: roller doors under a ribbon of high windows and a \
                  low gable or sawtooth roof, or - where it trades - a retail box's glazed entrance \
                  under its sign.",
    themes: &[ThemeArchetype::ModernCity],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A hall's clear height below, and an office storey over it.
    storey_m: (6.5, 4.0),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BA0_0005,
    materials,
    round_meshes: &[],
    solid_meshes: &["Wall", "Door", "Pane", "Roof", "Trim", "Canopy", "Fascia"],
};

/// The sheets, the doors and the store's sign, the concrete aprons and the
/// roofing.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.80, 0.82, 0.84],
        panes: (3, 1),
        room: LAMP_WARM,
        shop: [0.98, 0.97, 0.92],
    });
    m.extend([
        ("SheetGrey".to_string(), sheet([0.58, 0.60, 0.62])),
        ("SheetWhite".to_string(), sheet([0.86, 0.87, 0.86])),
        ("SheetBlue".to_string(), sheet([0.30, 0.40, 0.52])),
        ("Panel".to_string(), concrete([0.64, 0.63, 0.60])),
        ("Trim".to_string(), steel([0.46, 0.48, 0.50])),
        ("Concrete".to_string(), concrete([0.55, 0.55, 0.54])),
        ("DoorMetal".to_string(), steel([0.38, 0.40, 0.42])),
        ("Roller".to_string(), sheet([0.70, 0.71, 0.72])),
        ("Metal".to_string(), steel([0.30, 0.32, 0.34])),
        ("Sign".to_string(), enamel([0.72, 0.12, 0.10])),
        ("Roofing".to_string(), sheet([0.40, 0.41, 0.43])),
        ("Deck".to_string(), concrete([0.21, 0.21, 0.22])),
    ]);
    m
}
