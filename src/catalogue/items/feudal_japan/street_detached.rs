//! Berlin's detached house in the Edo theme's dress (#1600): a minka - a
//! farmhouse standing free in its garden, dark posts between plaster
//! panels and shoji lattices, an engawa veranda along its front and a deep
//! roof of thatch or tiles reaching out over all four sides.
//!
//! - **It stands in from its lot.** Its walls stand `Eave` in from every
//!   side, so the deep eaves reach out over them to the lot's edges and no
//!   further.
//! - **One house, one plaster**, rolled once at the lot - white lime,
//!   earthen ochre or grey; the posts, lattices and tiles name their own,
//!   the posts dark or bengara red (`Pick`ed once).
//! - **Lattice windows on all four sides** between posts, a sliding plank
//!   door up a stone step, and a timber engawa along the front under the
//!   eaves.
//! - **A pent eave** of tiles over the ground storey's front, where the
//!   house has an upper storey; the posts under it stop where it meets the
//!   wall.
//! - **The roof** is `Pick`ed once: a thatched hip, a tiled irimoya - a hip
//!   with a gable over it - or a tiled hip.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::street_house::feudal_street_palette;
use super::{THATCH_STRAW, TIMBER_BROWN, thatch, timber};

/// The minka (see the module docs); its rules are `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "feudal_japan_street_detached",
    name: "Minka Farmhouse",
    description: "A minka in its garden: dark posts between plaster panels and shoji lattices \
                  on every side, an engawa veranda, and a deep roof of thatch or tiles.",
    themes: &[ThemeArchetype::FeudalJapan],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // The living storey, and the rooms over it.
    storey_m: (3.2, 2.9),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BA5_0004,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Post", "Eave", "Fascia", "Deck",
    ],
};

/// The Edo street palette behind close lattice glazing, with thatch and the
/// engawa's boards.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.20, 0.14, 0.10],
        panes: (6, 4),
        room: [1.0, 0.80, 0.52],
        shop: [1.0, 0.86, 0.62],
    });
    m.extend(feudal_street_palette());
    m.extend([
        ("Thatch".to_string(), thatch(THATCH_STRAW)),
        ("Boards".to_string(), timber(TIMBER_BROWN)),
    ]);
    m
}
