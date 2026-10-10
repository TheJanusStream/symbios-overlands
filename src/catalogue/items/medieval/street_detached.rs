//! Berlin's detached house in the medieval theme's dress (#1600): a
//! timber-framed hall house standing free in its garden - oak posts and
//! rails on a stone sole, limewashed daub between them, a steep thatched
//! or tiled roof and a stone chimney stack in its gable.
//!
//! - **It stands in from its lot.** Its walls stand `Eave` in from every
//!   side, so the roof's eaves and verges reach out to the lot's edges and
//!   no further.
//! - **One frame, one infill.** The daub's limewash is rolled once at the
//!   lot, and the frame's oak `Pick`ed once, so a house is one colour of
//!   daub in one colour of timber, posts and rails on every side.
//! - **Windows on all four sides**, each between posts with a rail at its
//!   sill and its head; the door up a stone step under a lintel, and an
//!   upper storey jettied out over the front on a bressumer where the house
//!   has one.
//! - **The roof** is `Pick`ed once: a thatched hip, a thatched gable or a
//!   tiled gable, all steep; a stone chimney stack stands in the left side
//!   wall, up over the ridge of a gable or the slope of a hip, and over a
//!   jettied front the roof reaches out with the jetty.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{STONE_PALE, rough_stone};

/// The house (see the module docs); its rules are `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "medieval_street_detached",
    name: "Timber-Framed Hall House",
    description: "A timber-framed hall house in its garden: oak posts and limewashed daub on a \
                  stone sole, windows on every side, and a steep thatched or tiled roof with a \
                  stone chimney.",
    themes: &[ThemeArchetype::Medieval],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // The hall storey, and the chambers over it.
    storey_m: (3.2, 2.8),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BA2_0004,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Timber", "Shutter", "Chimney",
    ],
};

/// The craftsmen's row's palette (`street_low`) and the rubble of a
/// chimney stack.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_low::materials();
    m.extend([("Rubble".to_string(), rough_stone(STONE_PALE))]);
    m
}
