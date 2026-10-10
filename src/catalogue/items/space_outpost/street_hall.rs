//! Berlin's hall in the outpost's dress (#1600): the sheds off the landing
//! pad - a hull-plated hangar of hazard-striped bay doors, or, where it
//! trades, a supply commissary of lit counters under a deep canopy.
//!
//! - **One hangar, one hull livery**, rolled once at the lot; its sides
//!   are party walls, so hangars stand flush in a row and a lone hangar's
//!   ends read as its plating's own. A steel ring frame runs along its
//!   front at the top of every storey.
//! - **A hangar or a commissary.** A hangar (`Trade` 0) has a crew hatch
//!   at one end and bays of tall bay doors along the rest, each in
//!   hazard-striped jambs over a grated apron, some bays plated, and a
//!   ribbon of viewports over them all; a commissary (`Trade` 1) has
//!   supply counters - wide ports over a plinth - either side of its
//!   double hatch, a deep steel canopy along the whole front and a lit
//!   sign band over it.
//! - **The roof** is a hangar's pressure vault along the street - a turned
//!   hull laid on its side, a steel hoop every six metres - or a deck inside a
//!   hazard-striped parapet with solar racks and a comms mast, `Pick`ed
//!   once; a commissary's is always the deck.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{painted, steel};

/// The hangar (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "space_outpost_street_hall",
    name: "Outpost Hangar",
    description: "A hull-plated hangar of hazard-striped bay doors under a pressure vault or a \
                  deck of solar racks and a beacon mast - or, where it trades, a supply \
                  commissary of lit counters under a deep canopy.",
    themes: &[ThemeArchetype::SpaceOutpost],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A hangar's clear height below, and a control deck over it.
    storey_m: (6.5, 4.0),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BB0_0005,
    materials,
    round_meshes: &["Mast", "Vault", "Hoop"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Frame", "Ring", "Fascia", "Canopy", "Housing",
        "Panel", "Mast", "Vault", "Hoop",
    ],
};

/// The hab stack's hull liveries, steel, hazard paint, lamps, deck and
/// photovoltaics (`street_house::materials`), and the depot's hatch
/// colours.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_house::materials();
    m.extend([
        ("HatchOrange".to_string(), painted([0.80, 0.38, 0.10])),
        ("HatchGrey".to_string(), steel([0.40, 0.43, 0.48])),
    ]);
    m
}
