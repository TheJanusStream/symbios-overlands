//! Berlin's detached house in the pirate theme's dress (#1600): a
//! planter's house in its own garden above the harbour - limewashed walls
//! on a coral-stone plinth, every window between a pair of louvred
//! shutters, a timber gallery on posts along its front, and a deep hipped
//! roof of shingle against the hurricanes.
//!
//! - **It stands in from its lot.** Its walls stand `Eave` in from every
//!   side, so its roof reaches out over them to the lot's edges and no
//!   further; the gallery stands out of its front, never past its sides.
//! - **One house, one limewash.** The colour is rolled once at the lot and
//!   inherited by every wall below it; the stone plinth, the shutters, the
//!   gallery and the roof name their own, and the shutters' colour is
//!   `Pick`ed once a house, as the street's townhouses do.
//! - **Shutters on all four sides**: tall windows over a sill under a
//!   hood, a louvred leaf either side, on the front and the back and down
//!   both sides, storey on storey.
//! - **The gallery** (`Pick`ed once): a veranda the whole front on a row
//!   of oak posts on a stone floor up at the plinth, a timber roof over it
//!   with a railing round it where a storey stands above, or stone steps
//!   up to the door alone under a hood.
//! - **The roof** (`Pick`ed once): the islands' deep hip, or a steep gable
//!   along the front; grey shingle or red, `Pick`ed once too.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{HULL_OAK, board, shingle};

/// The planter's house (see the module docs); its rules are
/// `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "pirate_street_detached",
    name: "Planter's House",
    description: "A planter's house in its garden: limewashed walls on a coral-stone plinth, \
                  louvred shutters on every window, a timber gallery on posts and a deep \
                  hipped shingle roof.",
    themes: &[ThemeArchetype::Pirate],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // High rooms against the heat, as the townhouses' are.
    storey_m: (3.8, 3.4),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BB7_0004,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Shutter", "Balcony", "Post", "Canopy",
    ],
};

/// The townhouse's limewash colours, coral stone, shutters, oak and
/// shingle, and the gallery's posts and a red shingle.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_house::materials();
    m.extend([
        ("Post".to_string(), board(HULL_OAK)),
        ("ShingleRed".to_string(), shingle([0.46, 0.27, 0.20])),
    ]);
    m
}
