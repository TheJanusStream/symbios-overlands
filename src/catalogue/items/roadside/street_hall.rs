//! Berlin's hall in the roadside strip's dress (#1600): a highway truck
//! garage of corrugated steel with its row of coloured roller doors, or,
//! where it trades, a roadside discount store with a chrome canopy and a
//! big neon sign on steel legs over its roof.
//!
//! - **One hall, one cladding.** A garage's corrugated sheet - grey,
//!   rust or cream - or a store's painted brick is rolled once at the lot;
//!   its gable ends are its party walls, blank, so halls stand flush in a
//!   row.
//! - **A garage** (`Trade` 0): a door for people at one end, bays of
//!   roller doors along the rest over a concrete apron, the doors one
//!   enamel colour `Pick`ed once and some bays blank, under a ribbon of
//!   high windows and a low corrugated gable.
//! - **A discount store** (`Trade` 1): glass doors between shop windows
//!   under a chrome canopy, chrome bands over painted brick either side,
//!   a parapet with a chrome coping round a flat deck, and the motor
//!   strip's sign - a board of neon cells on steel legs - over the entrance.
//! - **An upper storey**, where it has one: offices, a ribbon of windows
//!   front and back.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{CORRUGATED_GREY, ENAMEL_CREAM, RUST_BROWN, corrugated};

/// The garage or store (see the module docs); its rules are
/// `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "roadside_street_hall",
    name: "Truck Garage",
    description: "A corrugated-steel truck garage with a row of coloured roller doors under a \
                  low gable - or, where it trades, a discount store with a chrome canopy and a \
                  neon sign on steel legs over its roof.",
    themes: &[ThemeArchetype::Roadside],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A truck's clear height below, and an office storey over it.
    storey_m: (6.0, 4.0),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BAB_0005,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Canopy", "Sign", "Steel",
    ],
};

/// The diner's and motor court's enamels, brick, chrome and neon, and a
/// garage's corrugated sheet in three colours and its roller doors.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_low::materials();
    m.extend([
        ("SheetGrey".to_string(), corrugated(CORRUGATED_GREY)),
        ("SheetRust".to_string(), corrugated(RUST_BROWN)),
        ("SheetCream".to_string(), corrugated(ENAMEL_CREAM)),
        ("Roofing".to_string(), corrugated([0.42, 0.43, 0.45])),
        ("Roller".to_string(), corrugated([0.74, 0.75, 0.76])),
    ]);
    m
}
