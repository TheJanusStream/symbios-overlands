//! Berlin's hall in the gothic horror theme's dress (#1600): where it works,
//! a Victorian mill or foundry shed - soot-black brick between stepped
//! buttresses, tall pointed windows over arched cart doors; where it
//! trades, a covered market hall - shop windows in black-framed bays, a
//! pointed entrance gable over its doors.
//!
//! - **One hall, one brick**, rolled once at the lot; its gable ends are
//!   its party walls, blank, so halls stand flush in a row, and a lone
//!   hall's ends read as its brick's own.
//! - **Buttressed bays**: a buttress of the hall's masonry, capped with
//!   dressed stone, between every bay of the front, so a long front reads
//!   as a nave's flank.
//! - **A works or a market.** A works hall (`Trade` 0) has a door for men at
//!   one end and bays of tall timber cart doors over stone aprons, some bays
//!   blank, and a row of pointed windows over them; a market (`Trade` 1) has
//!   black-framed shop windows in every bay under a black fascia, a tall door
//!   under a stained fanlight in the middle, and a pointed gable over that
//!   door.
//! - **The roof is slate**: a works' a slate gable along the street under
//!   iron cresting, or (`Pick`ed once) a weaving shed's sawtooth with its
//!   glazed north lights; a market's always the gable and its cresting.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{brick, wood};

/// The hall (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "gothic_horror_street_hall",
    name: "Gothic Mill Hall",
    description: "A Victorian mill of soot-black brick between buttresses, tall pointed windows \
                  over arched cart doors under a slate roof - or, where it trades, a covered \
                  market hall with a pointed entrance gable.",
    themes: &[ThemeArchetype::GothicHorror],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A mill floor's clear height below, and a loft of offices over it.
    storey_m: (6.0, 4.0),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BB2_0005,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Fascia", "Iron", "Buttress",
    ],
};

/// The theme's street palette, a mill's red brick, and the tarred timber
/// of its cart doors.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_low::materials();
    m.extend([
        ("RedBrick".to_string(), brick([0.42, 0.22, 0.16])),
        ("Timber".to_string(), wood([0.20, 0.15, 0.12])),
    ]);
    m
}
