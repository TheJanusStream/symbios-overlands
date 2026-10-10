//! Berlin's hall in the pirate theme's dress (#1600): the harbour's
//! warehouses - a bonded store of coral stone or tarred strakes, arched
//! cart doors under dressed voussoirs, small barred windows behind plank
//! shutters, loading doors high in the wall under a hoist beam, and a
//! steep shingle roof; where it trades, a ship chandler's or a market
//! exchange behind an arcade of stone arches.
//!
//! - **One hall, one wall.** Its stone or its strakes are rolled once at
//!   the lot; its ends are its party walls, blank, so halls stand flush
//!   along a quay and a lone hall's ends read as its own masonry.
//! - **A warehouse or an exchange.** A warehouse (`Trade` 0) has a door
//!   for people at one end and bays along the rest, most an arched cart
//!   door with a loading door over it under a hoist beam, some a pair of
//!   shuttered windows; an exchange (`Trade` 1) has a row of arched shop
//!   windows on a plinth, a broad door every few arches, a stone course
//!   over them and its sign hung out on iron brackets.
//! - **The roof** (`Pick`ed once): a row of steep gables turned to the
//!   quay, store after store, or one long gable along it; the exchange's
//!   is the long gable, shingle either way.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{GOLD_LEAF, HULL_OAK, ashlar, board, tar};

/// The warehouse (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "pirate_street_hall",
    name: "Harbour Warehouse",
    description: "A harbour warehouse of coral stone or tarred strakes: arched cart doors, \
                  loading doors under hoist beams and a steep shingle roof - or, where it \
                  trades, a market exchange of arched shop windows under its hanging signs.",
    themes: &[ThemeArchetype::Pirate],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A store's floor high enough for a cart and its load, and a loft over
    // it.
    storey_m: (5.4, 3.6),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BB7_0005,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Cornice", "Shutter", "Fascia", "Bracket", "Beam",
    ],
};

/// The tavern's strakes, quay stone, limewash, doors, shutters, sign, iron
/// and shingle, and the warehouse's coral stone, its oak beams and the
/// gilt of a sign.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_low::materials();
    m.extend([
        (
            "CoralStone".to_string(),
            ashlar([0.74, 0.68, 0.56], 0xA5_0031),
        ),
        ("Beam".to_string(), board(HULL_OAK)),
        ("Gilt".to_string(), tar(GOLD_LEAF)),
    ]);
    m
}
