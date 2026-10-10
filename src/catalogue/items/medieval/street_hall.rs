//! Berlin's hall in the medieval theme's dress (#1600): a tithe barn -
//! rubble walls between stone buttresses, a great cart door in a gabled
//! porch and slit windows, under a steep roof - or, where it trades, a
//! market hall of timber-framed stalls, each a window over a shutter let
//! down as a counter under its sign.
//!
//! - **One hall, one wall.** The stone or the daub is rolled once at the
//!   lot; its gable ends are its party walls, blank, so halls stand flush
//!   in a row.
//! - **A barn or a market.** A works hall (`Trade` 0) has its cart doors
//!   in porches standing out of its front, a door for people beside them,
//!   buttresses along the rest and a ribbon of slit windows set high; a
//!   market hall (`Trade` 1) has a framed stall in every bay and its doors
//!   between them. An upper storey, where it has one, is a framed loft
//!   jettied over the street.
//! - **A steep roof in spans**: as many steep gables along the street as
//!   its depth holds, side by side behind the front one, so a deep hall's
//!   crown stays low; thatch, tile or slate `Pick`ed once.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{STONE_PALE, rough_stone};

/// The hall (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "medieval_street_hall",
    name: "Tithe Barn",
    description: "A great stone barn: buttressed rubble walls, cart doors in gabled porches and \
                  slit windows under a steep roof - or, where it trades, a market hall of \
                  timber-framed stalls with shutters and signs.",
    themes: &[ThemeArchetype::Medieval],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A barn's threshing floor, and a framed loft over it.
    storey_m: (6.0, 3.6),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BA2_0005,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Timber", "Shutter", "Sign",
    ],
};

/// The craftsmen's row's palette (`street_low`) and a barn's rubble.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_low::materials();
    m.extend([("Rubble".to_string(), rough_stone(STONE_PALE))]);
    m
}
