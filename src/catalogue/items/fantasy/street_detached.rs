//! Berlin's detached house in the fantasy theme's dress (#1600): a
//! storybook cottage standing free in its garden - a fieldstone ground
//! floor, timbered daub over it, leaded windows with flower boxes, a round
//! stone turret under a slate cone, and a steep roof, the street's own
//! turret townhouse (`street_house`) let out of its row.
//!
//! - **It stands in from its lot.** Its walls stand `Eave` in from every
//!   side, so its roof reaches out over them to the lot's edges and no
//!   further.
//! - **One cottage, one daub**, rolled once at the lot and inherited by
//!   every wall over the stone ground floor; a timber jetty beam runs round
//!   it at every floor, and a post stands at every bay.
//! - **Windows on all four sides**, each leaded, under a timber lintel,
//!   over a flower box where the `%` roll gives it one.
//! - **The door** is a plank leaf in its own colour up a stone step, a
//!   glowing rune over it.
//! - **A turret**, `Pick`ed once beside the door or not at all: a round
//!   stone drum the house's full height, standing half out of its front,
//!   a lancet window on every storey and a slate cone with a gold finial
//!   over the eaves.
//! - **One roof**, `Pick`ed once: a steep golden thatch, or blue or green
//!   slate as a gable along the street or a hipped roof; a round stone
//!   chimney stands on it.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{matte, thatch, timber};

/// The cottage (see the module docs); its rules are `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "fantasy_street_detached",
    name: "Storybook Cottage",
    description: "A storybook cottage in its garden: a stone ground floor, timbered daub over \
                  it, leaded windows with flower boxes, a round turret under a slate cone, \
                  and a steep thatch or slate roof.",
    themes: &[ThemeArchetype::Fantasy],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // A cottage's stone ground storey, and the timbered storeys over it.
    storey_m: (3.2, 2.9),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BB1_0004,
    materials,
    round_meshes: &["Turret", "Cap", "Finial", "Chimney"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Beam", "Turret", "Cap", "Finial", "Chimney",
        "Planter",
    ],
};

/// The townhouse's daubs, stone, timbers, doors, gold, rune and slates
/// (`street_house::materials`), and the cottages' thatch, painted doors
/// and flower boxes.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_house::materials();
    m.extend([
        ("Thatch".to_string(), thatch([0.70, 0.58, 0.32])),
        ("DoorRed".to_string(), timber([0.50, 0.18, 0.12])),
        ("DoorGreen".to_string(), timber([0.22, 0.38, 0.22])),
        ("Leaf".to_string(), matte([0.24, 0.44, 0.20])),
    ]);
    m
}
