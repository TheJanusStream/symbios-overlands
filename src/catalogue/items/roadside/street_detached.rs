//! Berlin's detached house in the roadside strip's dress (#1600): the
//! ranch house of the 1950s highway suburb - one long low storey or two of
//! painted brick under a low-pitched shingle roof, a picture window to the
//! road, a brick chimney up one side, and a covered walk on steel posts
//! like the motor court's.
//!
//! - **One house, one paint**, rolled once at the lot: cream, mint or
//!   salmon brick, as the motor court and the motor hotel wear.
//! - **It stands in from its lot.** Its walls stand `Eave` in from every
//!   side, so its eaves reach the lot's edges and no further.
//! - **The front**: the door up a step, every house its own door colour, a
//!   wide picture window of many panes beside it, and windows along the
//!   rest; a covered walk the width of the front on steel posts under a
//!   corrugated roof, or a flat canopy over the door alone (`Pick`ed once).
//! - **Windows on all four sides**, and a brick chimney up the outside of
//!   one side's wall where it has one (`Pick`ed once).
//! - **A low roof**, `Pick`ed once: a hip, a gable along the road, or a
//!   gable to it; its shingle grey or brown, `Pick`ed once too.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{BRICK_TAN, ENAMEL_CREAM, asphalt, brick, enamel};

/// The ranch house (see the module docs); its rules are
/// `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "roadside_street_detached",
    name: "Ranch House",
    description: "A low painted-brick ranch house in its yard: a picture window and a coloured \
                  door under a covered walk on steel posts, a brick chimney, and a low hipped or \
                  gabled roof.",
    themes: &[ThemeArchetype::Roadside],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // A ranch house's rooms, all of a height.
    storey_m: (3.0, 2.8),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BAB_0004,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Canopy", "Steel", "Chimney",
    ],
};

/// The motor court's painted brick, doors, steel and corrugated walk roof,
/// and a ranch house's own trim, chimney brick and roof shingle.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_low::materials();
    m.extend([
        ("Trim".to_string(), enamel(ENAMEL_CREAM)),
        ("ChimneyBrick".to_string(), brick(BRICK_TAN)),
        ("ShingleGrey".to_string(), asphalt([0.30, 0.30, 0.31])),
        ("ShingleBrown".to_string(), asphalt([0.36, 0.25, 0.19])),
    ]);
    m
}
