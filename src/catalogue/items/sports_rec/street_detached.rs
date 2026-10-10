//! Berlin's detached house in the sports theme's dress (#1600): the
//! groundsman's house at the edge of a club's ground - a house of white
//! render or painted block banded in the club's colour, a veranda across
//! its front on posts in that colour, like a cricket pavilion's, and a
//! sheet roof.
//!
//! - **One house, one wall; one club, one colour.** The wall is rolled
//!   once at the lot, and the club's colour is `Pick`ed once: the band
//!   round the house at every floor line, the veranda's posts, the door and
//!   the eaves' fascia all wear it.
//! - **It stands in from its lot.** Its walls stand `Eave` in from every
//!   side, so its eaves reach the lot's edges and no further.
//! - **The veranda**: a deck a step high the width of the front, posts in
//!   the club's colour, under a flat sheet roof with the club's lit crest
//!   on its edge over the steps. Where the house has none (`Pick`ed once),
//!   the door stands up a step under a canopy.
//! - **Windows on all four sides**, wide ones to the garden at the back.
//! - **A sheet roof**, `Pick`ed once: a mono-pitch rising to the back, as
//!   the changing rooms have, or a gable along the street.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

/// The house (see the module docs); its rules are `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "sports_rec_street_detached",
    name: "Groundsman's House",
    description: "A white house at a club's ground, banded in the club's colour: a veranda on \
                  coloured posts like a pavilion's, windows all round, and a sheet mono-pitch or \
                  gable roof.",
    themes: &[ThemeArchetype::SportsRec],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // A family house's rooms: a little taller below, where it is entered.
    storey_m: (3.0, 2.8),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BAD_0004,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Band", "Canopy", "Rail", "Sign",
    ],
};

/// The changing rooms' painted block and sheet with the club house's
/// render, the club's four colours, the steel and the lit crest.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_low::materials();
    m.extend(super::street_house::materials());
    m
}
