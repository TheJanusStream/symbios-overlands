//! Berlin's detached house in the coastal resort's dress (#1600): the
//! clapboard seaside house of a beach town's back streets - painted plank
//! walls in the boardwalk's pastels, white trim, shuttered windows on every
//! side, a veranda on white posts, and a cedar-shingle roof.
//!
//! - **One house, one paint.** The planks' pastel is rolled once at the lot,
//!   as the boardwalk cottages' are one by one; the shutters' colour is
//!   `Pick`ed once, so every window of a house wears the same.
//! - **It stands in from its lot.** Its walls stand `Eave` in from every
//!   side, so its shingle eaves and verges reach the lot's edges and no
//!   further.
//! - **A veranda or a stoop** (`Pick`ed once): a plank deck a step high the
//!   width of the front on white posts under a flat white roof, or the door
//!   up a white step under a hood.
//! - **Windows on all four sides**: those on the front each between a pair
//!   of shutters, and a wide one to the garden in one bay of two at the
//!   back.
//! - **One roof**, `Pick`ed once: a gable along the street, a gable to the
//!   street, or a hip; grey or cedar shingle, `Pick`ed once too.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::shingle;

/// The house (see the module docs); its rules are `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "coastal_resort_street_detached",
    name: "Seaside Clapboard House",
    description: "A pastel clapboard house in its garden: white trim, shuttered windows all \
                  round, a veranda on white posts or a stoop, under a gable or hipped roof of \
                  grey or cedar shingle.",
    themes: &[ThemeArchetype::CoastalResort],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // A family house's rooms: a little taller below, where it is entered.
    storey_m: (3.0, 2.8),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BAA_0004,
    materials,
    round_meshes: &[],
    solid_meshes: &["Wall", "Door", "Pane", "Roof", "Trim", "Canopy"],
};

/// The boardwalk cottages' painted planks, trim, doors and shingle, and a
/// cedar shingle beside the grey.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_low::materials();
    m.extend([("ShingleCedar".to_string(), shingle([0.50, 0.37, 0.26]))]);
    m
}
