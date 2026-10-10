//! Berlin's detached house in the classical theme's dress (#1600): a villa
//! suburbana standing free in its garden - plastered walls on a travertine
//! plinth, small windows under brick arches, a columned porch at its door
//! and a low tiled roof over a travertine cornice.
//!
//! - **It stands in from its lot.** Its walls stand `Eave` in from every
//!   side, so the roof reaches out over them to the lot's edges and no
//!   further.
//! - **One villa, one plaster.** The plaster - ochre, Pompeian red, cream or
//!   white - is rolled once at the lot and inherited by every wall that
//!   names no material of its own; travertine, brick, marble, doors and
//!   tiles name theirs.
//! - **Windows on all four sides**: small windows under brick flat arches,
//!   shuttered one in two (`%` per window), and wider ones to the garden.
//! - **The porch** is `Pick`ed once: a portico of two turned marble columns
//!   under a pediment, or a doorway between travertine jambs under a
//!   lintel.
//! - **The roof** is `Pick`ed once too: a tiled hip, or a gable turned to
//!   the street as a temple front, its pediment over the cornice.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{BRONZE_GREEN, adobe, bronze};

/// The villa (see the module docs); its rules are `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "ancient_street_detached",
    name: "Roman Villa",
    description: "A Roman villa in its garden: plastered walls on a travertine plinth, small \
                  arched windows on every side, a columned porch, and a low tiled roof over a \
                  travertine cornice.",
    themes: &[ThemeArchetype::AncientClassical],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // The atrium storey, a little taller, and the rooms over it.
    storey_m: (3.4, 3.0),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BA1_0004,
    materials,
    round_meshes: &["Column"],
    solid_meshes: &["Wall", "Door", "Pane", "Roof", "Trim", "Cornice", "Column"],
};

/// The tabernae's and the domus's palette (`street_low`), a white lime
/// plaster and a bronze door.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_low::materials();
    m.extend([
        ("PlasterWhite".to_string(), adobe([0.93, 0.91, 0.85])),
        ("Bronze".to_string(), bronze(BRONZE_GREEN)),
    ]);
    m
}
