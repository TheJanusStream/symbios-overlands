//! Berlin's hall in the civic campus's dress (#1600): an institute's hall
//! of the university's science quarter - an engineering school's machine
//! hall of red brick or ashlar, tall windows between marble pilasters, or,
//! where it trades, an exhibition hall behind a marble colonnade.
//!
//! - **One hall, one wall**, rolled once at the lot: red brick, or pale or
//!   buff ashlar; its gable ends are its party walls, blank, so halls stand
//!   flush in a row.
//! - **A machine hall** (`Trade` 0): tall windows on marble sills between
//!   marble pilasters the height of the hall, and a pair of timber doors
//!   for the machines over a stone apron, under a marble lintel and a
//!   pediment; its roof a low copper gable along the street or copper
//!   teeth, each with a glazed north light over the benches (`Pick`ed
//!   once).
//! - **An exhibition hall** (`Trade` 1): a colonnade of round marble
//!   columns on a stylobate along the middle of the front, the doors and
//!   windows behind it and the hall's name on the entablature, tall
//!   windows either side, and a marble cornice and parapet round a flat
//!   deck.
//! - **An upper storey**, where it has one: a row of windows on marble
//!   sills over the hall, front and back.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

/// The hall (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "civic_campus_street_hall",
    name: "Institute Hall",
    description: "A brick or ashlar machine hall of the science quarter: tall windows between \
                  marble pilasters and a copper roof - or, where it trades, an exhibition hall \
                  behind a marble colonnade.",
    themes: &[ThemeArchetype::CivicCampus],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A machine hall's clear height, and a storey of rooms over it.
    storey_m: (6.5, 4.2),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BAC_0005,
    materials,
    round_meshes: &["Column"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Cornice", "Column", "Fascia",
    ],
};

/// The townhouse's palette: ashlars and brick, marble, the doors, the sign
/// band, the copper and the deck.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    super::street_house::materials()
}
