//! Berlin's hall in the fantasy theme's dress (#1600): a great timber barn
//! of the guilds on a stone plinth, or, where it trades, a guild market
//! hall of leaded shop windows under painted signs.
//!
//! - **One hall, one daub**, rolled once at the lot over a fieldstone
//!   plinth; its sides are party walls, so halls stand flush in a row and
//!   a lone hall's ends read as its daub's own. Dark timber posts stand
//!   between its bays and a beam runs along its front at every floor.
//! - **A barn or a market.** A barn (`Trade` 0) has a plank door for its
//!   hands at one end and bays of great wagon doors under timber lintels
//!   along the rest, some bays timbered daub, a ribbon of small leaded
//!   windows over them all; a market (`Trade` 1) has leaded shop windows
//!   over mossy plinths either side of its doors, each bay under its own
//!   painted sign board, and a glowing rune over the doors.
//! - **The roof** is steep along the street, its rise held under the
//!   crown's limit however deep the hall: golden thatch or slate
//!   (`Pick`ed once), with a slender timber fleche on its ridge under a
//!   slate spire and a gold finial.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{thatch, timber};

/// The hall (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "fantasy_street_hall",
    name: "Great Guild Hall",
    description: "A great timber barn of the guilds on a stone plinth, with wagon doors and a \
                  steep roof under a slender fleche - or, where it trades, a guild market hall \
                  of leaded shop windows under painted signs.",
    themes: &[ThemeArchetype::Fantasy],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A barn's lofty ground storey, and a guild loft over it.
    storey_m: (5.5, 3.6),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BB1_0005,
    materials,
    round_meshes: &["Cap", "Finial"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Beam", "Fascia", "Cap", "Finial",
    ],
};

/// The townhouse's daubs, stone, timbers, doors, signs, gold, rune and
/// slates (`street_house::materials`), and the cottages' thatch and a
/// green door.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_house::materials();
    m.extend([
        ("Thatch".to_string(), thatch([0.70, 0.58, 0.32])),
        ("DoorGreen".to_string(), timber([0.22, 0.38, 0.22])),
    ]);
    m
}
