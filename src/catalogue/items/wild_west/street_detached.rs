//! Berlin's detached house in the frontier theme's dress (#1600): the
//! homesteader's clapboard farmhouse standing alone in its yard - painted
//! boards, tall sash windows between plank shutters, a porch on posts
//! along its front, a fieldstone chimney up one end, and a steep roof of
//! tin or of wooden shakes.
//!
//! - **It stands in from its lot.** Its walls stand `Eave` in from every
//!   side, so its roof reaches out over them to the lot's edges and no
//!   further; the porch stands out of its front, never past its sides.
//! - **One house, one coat of paint.** The clapboard's colour is rolled
//!   once at the lot and inherited by every wall below it; the trim, the
//!   shutters, the doors, the porch and the roof name their own.
//! - **Windows on all four sides**: tall sashes over a sill under a board
//!   head on the front and the back, between a pair of plank shutters
//!   where the house has them (`Pick`ed once), and a sash or two down
//!   each side, storey on storey.
//! - **The porch** (`Pick`ed once): a veranda the whole front on a row of
//!   posts, a sleeping porch's rail round its roof where a storey stands
//!   above (`Pick`ed once), or a stoop over the door alone - a plank floor
//!   a step high and a tin roof on two posts.
//! - **The roof** (`Pick`ed once): a steep gable along the yard's front,
//!   or one turned end-on to it, or a hipped roof; tin or shakes, `Pick`ed
//!   once too, and a fieldstone chimney rising through it.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{STONE_TAN, lap_siding, stone};

/// The farmhouse (see the module docs); its rules are `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "wild_west_street_detached",
    name: "Frontier Farmhouse",
    description: "A homesteader's clapboard farmhouse alone in its yard: shuttered sash \
                  windows all round, a porch on posts, a fieldstone chimney and a steep roof of \
                  tin or wooden shakes.",
    themes: &[ThemeArchetype::WildWest],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // A farmhouse's rooms: the kitchen and the parlour below, bedrooms
    // over them under the roof.
    storey_m: (3.2, 2.9),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BB6_0004,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall",
        "Door",
        "Pane",
        "Roof",
        "Trim",
        "Shutter",
        "Post",
        "Boardwalk",
        "Canopy",
        "Balcony",
        "Chimney",
    ],
};

/// The street's clapboard paints, doors, boards and tin (the low
/// building's), and the shutters, the shakes and the chimney's fieldstone.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_low::materials();
    m.extend([
        ("ClapGreen".to_string(), lap_siding([0.40, 0.48, 0.38])),
        ("Shutter".to_string(), lap_siding([0.24, 0.30, 0.22])),
        ("ShutterRed".to_string(), lap_siding([0.42, 0.16, 0.12])),
        ("Shakes".to_string(), lap_siding([0.38, 0.30, 0.22])),
        ("Fieldstone".to_string(), stone(STONE_TAN)),
    ]);
    m
}
