//! Berlin's hall in the coastal resort's dress (#1600): a boatyard's shed
//! of weathered planks with tall sliding doors for the boats, or, where it
//! trades, a seafront amusement arcade in the beachfront's pastel stucco.
//!
//! - **One hall, one cladding.** A boat shed's planks - drift grey, white or
//!   sky - or an arcade's pastel are rolled once at the lot; its gable ends
//!   are its party walls, blank, so halls stand flush in a row.
//! - **A boat shed** (`Trade` 0): a door for people at one end, tall
//!   sliding boat doors along the rest over a plank slipway, some bays
//!   blank, under a ribbon of high windows; its roof a row of shingle
//!   gables to the street, one a bay, or one long gable along it (`Pick`ed
//!   once).
//! - **An arcade** (`Trade` 1): a run of shop windows on a white plinth,
//!   each under a striped awning, either side of a glazed entrance under a
//!   white marquee; a neon-banded sign fin over the entrance, and a
//!   parapet stepped up round it over a flat deck.
//! - **An upper storey**, where it has one: a ribbon of windows front and
//!   back - a net loft, or the arcade's cafe.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{DRIFT_GREY, plank};

/// The hall (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "coastal_resort_street_hall",
    name: "Boat Shed",
    description: "A weathered plank boat shed with tall sliding doors under a row of shingle \
                  gables - or, where it trades, a pastel seafront arcade under striped awnings \
                  and a neon sign fin.",
    themes: &[ThemeArchetype::CoastalResort],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A boat's clear height below, and a net loft or a cafe over it.
    storey_m: (6.0, 3.8),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BAA_0005,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Canopy", "Fascia", "Fin",
    ],
};

/// The boardwalk's planks and shingle, the hotel's pastels, awnings and
/// neon, and weathered planks for a boat shed and its doors.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_low::materials();
    m.extend(super::street_house::materials());
    m.extend([
        ("PlankDrift".to_string(), plank(DRIFT_GREY)),
        ("BoatDoor".to_string(), plank([0.22, 0.34, 0.42])),
    ]);
    m
}
