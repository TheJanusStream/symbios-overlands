//! Berlin's hall in the nordic theme's dress (#1600): a row of wharf
//! warehouses as Bergen's Bryggen stands them - steep gables turned to the
//! street side by side, each with its loading doors stacked under a
//! carved post at the peak - tarred for a works, painted where it trades,
//! with shop windows and a woven sign in every gable.
//!
//! - **One hall, one cladding.** Tar or paint is rolled once at the lot; its
//!   sides are party walls, blank, so halls stand flush in a row.
//! - **A gable to every bay.** The frontage is cut into warehouse bays, each
//!   under its own steep gable turned to the street, so the crown stays low
//!   however deep the hall runs. A works hall (`Trade` 0) has a wide plank
//!   door of two leaves in each bay between framed windows; a trading hall
//!   (`Trade` 1) has shop windows either side of its door and a woven sign
//!   over it. A loft door in each gable, under the carved post, and a
//!   loading door of two leaves in the loft where it has one.
//! - **The roof** is `Pick`ed once: tarred shakes, slate or turf.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{SHIELD_GOLD, SHIELD_RED, boards, cloth, shingle};

/// The hall (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "nordic_street_hall",
    name: "Wharf Warehouse Row",
    description: "A row of steep warehouse gables to the street: tarred boards with wide plank \
                  doors and stacked loft doors under carved peaks - or, where it trades, painted \
                  gables with shop windows and woven signs.",
    themes: &[ThemeArchetype::Nordic],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A warehouse's tall ground floor, and a loft over it.
    storey_m: (6.0, 3.6),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BA4_0005,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Plinth", "Carving", "Sign",
    ],
};

/// The boathouse row's palette (`street_low`), the town house's paints, a
/// woven sign and slate.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_low::materials();
    m.extend([
        ("Ochre".to_string(), boards([0.76, 0.56, 0.24])),
        ("White".to_string(), boards([0.86, 0.84, 0.78])),
        ("Slate".to_string(), shingle([0.30, 0.32, 0.35])),
        ("Sign".to_string(), cloth(SHIELD_RED, SHIELD_GOLD)),
    ]);
    m
}
