//! Berlin's hall in the frontier theme's dress (#1600): where it trades, a
//! boomtown's emporium - a long false front with its painted sign over a
//! plank boardwalk and a tin veranda, display windows down the whole
//! frontage; where it does not, a livery barn or a freight barn - wide
//! plank doors over fieldstone aprons, hay hoists over them, and a row of
//! stable windows.
//!
//! - **One hall, one coat of boards.** The barn's boards or the store's
//!   paint are rolled once at the lot; its ends are its party walls, blank,
//!   so halls stand flush in a row and a lone hall's ends read as its own
//!   boarding.
//! - **A barn or a store.** A barn (`Trade` 0) has a door for people at one
//!   end and bays along the rest, most a plank door - a hay door over it
//!   under a hoist beam where the barn has one storey, in its loft where
//!   it has two - some a pair of stable windows; a store (`Trade` 1) has
//!   display windows on a plinth and a double door every few bays, under one
//!   veranda on posts and behind one boardwalk, as the street's hotel has.
//! - **The roof**: a barn's is `Pick`ed once - a row of steep gables turned
//!   to the street, barn after barn, or one long gable along it; a store's
//!   is a row of low gables hidden behind its false front, which stands
//!   flat or with a pediment over its sign (`Pick`ed once).

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{STONE_TAN, lap_siding, stone};

/// The hall (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "wild_west_street_hall",
    name: "Frontier Livery Barn",
    description: "A livery barn of weathered boards: plank doors under hay hoists, stable \
                  windows and a row of steep gables - or, where it trades, a boomtown emporium \
                  behind a long false front over a boardwalk and a tin veranda.",
    themes: &[ThemeArchetype::WildWest],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A barn's floor high enough for a loaded wagon, and a hayloft or a
    // store's upper floor over it.
    storey_m: (5.6, 3.4),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BB6_0005,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall",
        "Door",
        "Pane",
        "Roof",
        "Trim",
        "Cornice",
        "Fascia",
        "Canopy",
        "Post",
        "Boardwalk",
        "Beam",
    ],
};

/// The street's clapboard paints, doors, signs, boards and tin (the low
/// building's), and the barn's red boards, its plank doors, its hoist
/// beams and the fieldstone of its aprons.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_low::materials();
    m.extend([
        ("BarnRed".to_string(), lap_siding([0.46, 0.17, 0.13])),
        ("BarnDoor".to_string(), lap_siding([0.34, 0.25, 0.16])),
        ("Beam".to_string(), lap_siding([0.30, 0.22, 0.14])),
        ("Shakes".to_string(), lap_siding([0.38, 0.30, 0.22])),
        ("Fieldstone".to_string(), stone(STONE_TAN)),
    ]);
    m
}
