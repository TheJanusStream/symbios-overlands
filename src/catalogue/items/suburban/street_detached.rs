//! Berlin's detached house in the suburban theme's dress (#1600): the
//! family house of the suburb in its own yard - painted lap siding, white
//! corner boards, shuttered windows on every side, a front porch, a brick
//! chimney and a shingled roof.
//!
//! - **One house, one siding colour**, rolled once at the lot - the street
//!   house's own blue, cream, sage, yellow or grey - and inherited by every
//!   wall; the trim, the porch, the doors and the roof name their own.
//! - **It stands in from its lot.** Its walls stand `Eave` in from every
//!   side, so its eaves and verges reach out to the lot's edges and no
//!   further.
//! - **Windows on all four sides**, white-framed on a sill, the front's
//!   between louvred shutters (`Pick`ed once a house: blue, white or red);
//!   white corner boards turn every corner.
//! - **A porch and a garage.** The front door stands under a porch on two
//!   posts, `Pick`ed once - an entry porch at the door, or a porch along
//!   the whole front - and a house wide enough has a garage door on its
//!   ground floor over the drive.
//! - **The roof** is `Pick`ed once: a side gable along the street, a front
//!   gable turned to it, or a hip; grey or brown shingle, a brick chimney
//!   up one end where the house has one.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{brick, enamel, shingle};

/// The house (see the module docs); its rules are `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "suburban_street_detached",
    name: "Suburban Family House",
    description: "A family house in its own yard: painted lap siding and white corner boards, \
                  shuttered windows all round, a front porch, a garage door where it is wide \
                  enough, a brick chimney and a shingled gable or hip.",
    themes: &[ThemeArchetype::Suburban],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // A family house's rooms: a little taller below, where it is entered.
    storey_m: (3.0, 2.8),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BA7_0004,
    materials,
    round_meshes: &[],
    solid_meshes: &["Wall", "Door", "Pane", "Roof", "Trim", "Porch", "Chimney"],
};

/// The street house's siding, trim, porch and doors, with a garage door, a
/// chimney's brick and brown shingles.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_house::materials();
    m.extend([
        ("Garage".to_string(), enamel([0.86, 0.86, 0.83])),
        ("Chimney".to_string(), brick([0.52, 0.28, 0.22])),
        ("ShingleBrown".to_string(), shingle([0.36, 0.27, 0.21])),
    ]);
    m
}
