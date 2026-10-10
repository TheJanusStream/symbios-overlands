//! Berlin's hall in the Mesoamerican theme's dress (#1600): a long painted
//! storehouse - the maize and cloth stores of a market town - on a stepped
//! stone talud, or, where it trades, a market hall of stalls between
//! painted piers.
//!
//! - **One hall, one stucco**, rolled once at the lot - adobe, ochre,
//!   white or red; its sides are party walls, blank, so halls stand flush
//!   in a row.
//! - **A storehouse or a market.** A storehouse (`Trade` 0) has wide
//!   doorways under timber lintels in some bays of its front, a small door
//!   for people up a step under a jade-studded lintel, and a row of high
//!   openings over them all; a market hall (`Trade` 1) has stall openings
//!   over stone benches between painted piers, each under a palm-thatch
//!   awning, and its doorway in the middle. A painted step-fret frieze
//!   runs the whole frontage over the ground storey.
//! - **An upper storey**, where it has one, stands on a stepped talud with
//!   small deep openings under lintels, front and back.
//! - **The roof** is `Pick`ed once: rows of steep palm-thatch ridges along
//!   the street, one to every twelve metres or so of depth, or a flat deck
//!   behind a parapet crowned with stepped merlons.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::street_house::meso_street_palette;

/// The hall (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "mesoamerican_street_hall",
    name: "Painted Storehouse Hall",
    description: "A long painted storehouse on a stepped stone talud: wide doorways under \
                  timber lintels and high openings, or market stalls under palm-thatch \
                  awnings, beneath rows of palm-thatch ridges or a merloned flat roof.",
    themes: &[ThemeArchetype::Mesoamerican],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A storehouse's tall floor, and a loft storey over it.
    storey_m: (5.5, 3.6),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BA6_0005,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Talud", "Frieze", "Lintel", "Merlon", "Canopy",
        "Pier",
    ],
};

/// The Mesoamerican street palette, behind firelit openings and a market's
/// brighter stalls.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.30, 0.22, 0.14],
        panes: (1, 1),
        room: [1.0, 0.60, 0.30],
        shop: [1.0, 0.78, 0.46],
    });
    m.extend(meso_street_palette());
    m
}
