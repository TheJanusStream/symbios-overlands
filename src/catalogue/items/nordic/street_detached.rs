//! Berlin's detached house in the nordic theme's dress (#1600): a painted
//! wooden house standing free in its garden - board cladding on a
//! fieldstone plinth, white corner boards and window frames, a gabled
//! porch at its door and a steep roof of shakes, turf or slate.
//!
//! - **It stands in from its lot.** Its walls stand `Eave` in from every
//!   side, so the roof's eaves and verges reach out to the lot's edges and
//!   no further.
//! - **One house, one paint**, rolled once at the lot: falu red, ochre,
//!   white or grey-blue boards, inherited by every wall that names no
//!   material of its own; the white trim names its own.
//! - **Framed windows on all four sides**: each a white frame proud of the
//!   boards, a sill under it and a head board over it, between white
//!   corner boards.
//! - **The door** up a stone step, `Pick`ed once under a little gabled
//!   porch on two posts or under a plain head board.
//! - **The roof** is `Pick`ed once: shakes, turf or slate on a steep gable
//!   along the street, or turf on a lower one. Most houses of more than one
//!   storey under a steep roof have a cross-gable over the door: a framed
//!   window in a storey of its own over the eaves, under a little gable.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::timber;

/// The house (see the module docs); its rules are `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "nordic_street_detached",
    name: "Nordic Timber House",
    description: "A painted wooden house in its garden: falu red, ochre or white boards on a \
                  fieldstone plinth, white-framed windows all round, a porch at the door, and a \
                  steep roof of shakes, turf or slate.",
    themes: &[ThemeArchetype::Nordic],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // The ground storey over its plinth, and the rooms over it.
    storey_m: (3.0, 2.7),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BA4_0004,
    materials,
    round_meshes: &[],
    solid_meshes: &["Wall", "Door", "Pane", "Roof", "Trim", "Plinth"],
};

/// The town house's palette (`street_house`) and two more door colours.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_house::materials();
    m.extend([
        ("DoorBlue".to_string(), timber([0.17, 0.25, 0.38])),
        ("DoorRed".to_string(), timber([0.46, 0.14, 0.10])),
    ]);
    m
}
