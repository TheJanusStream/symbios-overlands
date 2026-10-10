//! Berlin's detached house in the civic campus's dress (#1600): the
//! professor's villa of a university suburb - a square house of dressed
//! ashlar or red brick with marble dressings, a little portico of two round
//! columns at its door, and a copper or slate roof.
//!
//! - **One villa, one stone**, rolled once at the lot: pale, buff or grey
//!   ashlar, or red brick, as the institute's townhouse wears.
//! - **It stands in from its lot.** Its walls stand `Eave` in from every
//!   side, so its eaves reach the lot's edges and no further.
//! - **The portico**: the door up marble steps between two round columns
//!   carrying an entablature and a pediment, standing out from the middle
//!   of the front.
//! - **Windows on all four sides**, tall, each on a marble sill under a
//!   hood, and a marble string course round the house at every floor
//!   line.
//! - **One roof**, `Pick`ed once: a hip or a gable along the street, in
//!   verdigris copper or slate, `Pick`ed once too.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::stone;

/// The villa (see the module docs); its rules are `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "civic_campus_street_detached",
    name: "Professor's Villa",
    description: "A square villa of ashlar or red brick in its garden: a portico of two marble \
                  columns at its door, tall windows under marble hoods all round, and a copper \
                  or slate roof.",
    themes: &[ThemeArchetype::CivicCampus],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // A villa's tall rooms: the ground storey, and every storey above it.
    storey_m: (3.6, 3.2),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BAC_0004,
    materials,
    round_meshes: &["Column"],
    solid_meshes: &["Wall", "Door", "Pane", "Roof", "Trim", "Cornice", "Column"],
};

/// The townhouse's ashlars, brick, marble, door and copper, and a slate.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_house::materials();
    m.extend([("Slate".to_string(), stone([0.27, 0.28, 0.31]))]);
    m
}
