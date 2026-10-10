//! Berlin's detached house in the solarpunk theme's dress (#1600): a timber
//! eco house standing free in its garden - board walls on a pale lime
//! plinth, dark frame posts at its corners, solar shades over its windows
//! and creepers on its piers, the street's own town house
//! (`street_house`) let out of its row.
//!
//! - **It stands in from its lot.** Its walls stand `Eave` in from every
//!   side, so its roof reaches out over them to the lot's edges and no
//!   further.
//! - **One house, one timber**, rolled once at the lot: larch, silvered or
//!   honey board over a lime plinth, a dark frame post up every corner and
//!   a beam at every floor.
//! - **Windows on all four sides**, each under a blade of solar panel or
//!   over a window box of greens, `%` rolled window by window; the garden
//!   side opens a tall glazed door to the lawn in one bay of two.
//! - **The door** stands up a lime step under a planted timber canopy, a
//!   rain barrel beside it.
//! - **One roof**, `Pick`ed once: a sod gable along the street with a field
//!   of solar panels let into its garden slope, a mono-pitch of panels
//!   rising to the back, or a flat green roof inside a timber parapet with
//!   rows of panels on it.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{COB_EARTH, concrete};

/// The house (see the module docs); its rules are `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "solarpunk_street_detached",
    name: "Garden Eco House",
    description: "A timber eco house in its garden: board walls on a lime plinth, solar shades \
                  and creepers, a rain barrel by its planted porch, and a sod roof, a roof of \
                  panels or a green flat roof.",
    themes: &[ThemeArchetype::Solarpunk],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // A family house's rooms: a little taller below, where it is entered.
    storey_m: (3.2, 2.9),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BAF_0004,
    materials,
    round_meshes: &["Barrel"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Frame", "Shade", "Green", "PV", "Barrel",
        "Planter", "Canopy",
    ],
};

/// The town house's boards, lime, frame timber, greens and panels
/// (`street_house::materials`), and cob for a plinth.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_house::materials();
    m.extend([("Cob".to_string(), concrete(COB_EARTH))]);
    m
}
