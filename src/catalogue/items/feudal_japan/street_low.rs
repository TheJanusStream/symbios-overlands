//! Berlin's low building in the Edo theme's dress (#1598): a machiya of one
//! or two storeys - a lattice-fronted town house, or where the street
//! trades a shop open under noren - beneath a tiled pent eave and a tiled
//! gable along the street.
//!
//! - **One house, one plaster**, rolled once; posts, lattices, the skirt
//!   board and the tiles name their own.
//! - **A shop or a home.** A trading machiya opens its front under noren,
//!   each curtain its own colour, a lantern at the door; a home is a close
//!   lattice front with a sliding door, and its lattice is dark or bengara
//!   red (`Pick`ed once).
//! - **A loft** where it has two storeys: the low mushiko storey of plaster
//!   with barred windows, under the main roof.
//! - **The roof**: a tiled gable along the street, its ends the party
//!   walls, over a pent eave across the shop front.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::street_house::feudal_street_palette;

/// The low building (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "feudal_japan_street_low",
    name: "Machiya Town House",
    description: "A one- or two-storey machiya: a lattice front or a shop under noren and \
                  lanterns, a tiled pent eave, a barred loft, and a tiled gable along the \
                  street.",
    themes: &[ThemeArchetype::FeudalJapan],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // The shop storey, and the low loft over it.
    storey_m: (3.4, 2.6),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BA5_0003,
    materials,
    round_meshes: &["Lantern"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Post", "Eave", "Fascia",
    ],
};

/// The Edo street palette behind close lattice glazing.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.20, 0.14, 0.10],
        panes: (6, 3),
        room: [1.0, 0.80, 0.52],
        shop: [1.0, 0.86, 0.62],
    });
    m.extend(feudal_street_palette());
    m
}
