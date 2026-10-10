//! Berlin's hall in the suburban theme's dress (#1600): the big box of the
//! suburb's commercial strip - a store under a gabled entrance and a deep
//! canopy where it trades, a self-storage or service depot of roll-up doors
//! where it does not.
//!
//! - **One hall, one skin**, rolled once at the lot: tan brick, render or
//!   grey siding; its sides are party walls, blank, so halls stand flush in
//!   a row.
//! - **A store or a depot.** A store (`Trade` 1) has a gabled entrance
//!   porch standing out of the middle of its front over glass doors and
//!   shop windows, blank wall either side under a shingled mansard strip,
//!   and its lit sign; a depot (`Trade` 0) has a door for people at one
//!   end and roll-up doors along the rest, each over a concrete apron, some
//!   bays blank, under a row of small high windows.
//! - **An upper storey**, where it has one, has offices: a row of windows
//!   front and back.
//! - **The roof**: a store's flat deck behind a parapet; a depot's low
//!   shingled gable along the street, or a deck behind a shingled mansard
//!   edge along its front, `Pick`ed once.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{concrete, enamel, siding};

/// The hall (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "suburban_street_hall",
    name: "Suburban Big-Box Hall",
    description: "A big box of the commercial strip: a store under a gabled entrance porch, a \
                  deep canopy and a lit sign - or, where nothing trades, a self-storage depot \
                  of roll-up doors under a low shingled gable.",
    themes: &[ThemeArchetype::Suburban],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A store's tall sales floor, and an office storey over it.
    storey_m: (6.0, 3.8),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BA7_0005,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Canopy", "Sign", "Fascia",
    ],
};

/// The strip mall's brick, render, trim, doors, canopy, sign and shingles,
/// with grey siding, roll-up doors and a dark fascia.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_low::materials();
    m.extend([
        ("SidingGrey".to_string(), siding([0.62, 0.64, 0.65])),
        ("Roller".to_string(), enamel([0.78, 0.40, 0.16])),
        ("Fascia".to_string(), enamel([0.12, 0.15, 0.20])),
        ("Apron".to_string(), concrete([0.66, 0.65, 0.62])),
    ]);
    m
}
