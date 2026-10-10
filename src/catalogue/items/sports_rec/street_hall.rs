//! Berlin's hall in the sports theme's dress (#1600): the district's sports
//! hall - a box of panels or render banded in the club's colour under a
//! shallow vaulted roof, a ribbon of high windows along it - or, where it
//! trades, a sports megastore with a glazed entrance under a deep canopy
//! and a lit scoreboard for a sign.
//!
//! - **One hall, one cladding; one club, one colour.** The cladding is
//!   rolled once at the lot, and the club's colour `Pick`ed once: the broad
//!   band over the ground storey's front, the doors and the store's fascia
//!   wear it. Its gable ends are its party walls, blank, so halls stand
//!   flush in a row.
//! - **A sports hall** (`Trade` 0): the club's doors up a step under a lit
//!   crest at one end, fire doors along the rest, and a ribbon of high
//!   windows over them; its roof a shallow vault of sheet - a two-pitch
//!   curve - or a low gable (`Pick`ed once).
//! - **A megastore** (`Trade` 1): glass doors between shop windows under a
//!   canopy and a fascia in the club's colour, the ribbon either side, and
//!   a scoreboard of lit cells on the parapet round its flat deck.
//! - **An upper storey**, where it has one: a gallery's ribbon of windows
//!   front and back.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

/// The hall (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "sports_rec_street_hall",
    name: "Sports Hall",
    description: "A district sports hall banded in the club's colour, a ribbon of high windows \
                  under a shallow vaulted roof - or, where it trades, a sports megastore under a \
                  lit scoreboard sign.",
    themes: &[ThemeArchetype::SportsRec],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A sports hall's clear height for its courts, and a gallery over it.
    storey_m: (7.0, 4.0),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BAD_0005,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Band", "Canopy", "Fascia", "Sign",
    ],
};

/// The club house's render, concrete and panels with the changing rooms'
/// block and sheet, the club's four colours, steel, and the lit score.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_low::materials();
    m.extend(super::street_house::materials());
    m
}
