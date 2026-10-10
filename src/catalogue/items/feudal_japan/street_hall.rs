//! Berlin's hall in the Edo theme's dress (#1600): a kura storehouse grown
//! to a warehouse - thick white plaster over a namako skirt of tiles set on
//! the diagonal, heavy doors in thick plaster frames and small barred
//! windows - or, where it trades, a wholesaler's hall open to the street
//! under noren and a deep pent eave, lanterns at its doors.
//!
//! - **One hall, one plaster**, rolled once at the lot; its sides are party
//!   walls, blank, so halls stand flush in a row.
//! - **A kura or a tonya.** A works hall (`Trade` 0) has heavy doors in
//!   bays along its front, each over a stone sill, between barred windows,
//!   all over a namako skirt; a trading hall (`Trade` 1) has shop bays
//!   between dark posts, each under a noren curtain its own colour, and
//!   its door between lantern posts under a sign board. Both have a tiled
//!   pent eave along the front over the ground storey, and barred mushiko
//!   windows in an upper storey where it has one.
//! - **The roof**: a tiled gable along the street, its ends the party walls.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::street_house::feudal_street_palette;
use super::{TIMBER_DARK, lacquer, namako};

/// The hall (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "feudal_japan_street_hall",
    name: "Kura Warehouse",
    description: "A great kura storehouse: thick white plaster over a namako tile skirt, heavy \
                  doors and barred windows under a tiled pent eave - or, where it trades, a \
                  wholesaler's hall open under noren and lanterns.",
    themes: &[ThemeArchetype::FeudalJapan],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A storehouse's tall floor, and the loft over it.
    storey_m: (6.0, 3.6),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BA5_0005,
    materials,
    round_meshes: &["Lantern"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Post", "Eave", "Fascia", "Sign",
    ],
};

/// The Edo street palette behind close lattice glazing, with the namako
/// skirt and the sign board.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.20, 0.14, 0.10],
        panes: (6, 3),
        room: [1.0, 0.80, 0.52],
        shop: [1.0, 0.86, 0.62],
    });
    m.extend(feudal_street_palette());
    m.extend([
        ("Namako".to_string(), namako([0.16, 0.18, 0.22])),
        ("SignBoard".to_string(), lacquer(TIMBER_DARK)),
    ]);
    m
}
