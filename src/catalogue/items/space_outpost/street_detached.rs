//! Berlin's detached house in the outpost's dress (#1600): a crew's hab pod
//! standing free on its plot - one pressurised module, hull plating
//! between steel ring frames, framed viewports on every side and an
//! airlock for a door, the street's own hab stack (`street_house`) set down
//! alone.
//!
//! - **It stands in from its lot.** Its hull stands `Eave` in from every
//!   side, so nothing it carries reaches past the lot's edges.
//! - **One pod, one hull livery**, rolled once at the lot; a steel ring
//!   frame runs round it at every floor and at its crown, and a livery
//!   stripe (`Pick`ed once) runs round every deck under its viewports.
//! - **Viewports on all four sides**, each a port in steel jambs under a
//!   steel hood over a steel sill.
//! - **An airlock for a door**: a steel hatch in hazard-striped jambs, up
//!   a grated step, under a status lamp, its leaf a crew's own colour.
//! - **The roof**, `Pick`ed once: a pressure vault along the street - a
//!   turned hull laid on its side, steel hoops round it - or a deck with a
//!   solar rack and a comms mast with its red beacon.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{painted, steel};

/// The hab pod (see the module docs); its rules are `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "space_outpost_street_detached",
    name: "Outpost Hab Pod",
    description: "A crew's freestanding hab pod: hull plating between steel ring frames, framed \
                  viewports all round, an airlock door under a status lamp, and a pressure \
                  vault or an antenna deck on top.",
    themes: &[ThemeArchetype::SpaceOutpost],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // A pod's entry deck, a little taller, and its crew decks.
    storey_m: (3.4, 3.0),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BB0_0004,
    materials,
    round_meshes: &["Mast", "Vault", "Hoop"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Frame", "Ring", "Housing", "Panel", "Mast",
        "Vault", "Hoop",
    ],
};

/// The hab stack's hull liveries, steel, hazard paint, lamps, deck and
/// photovoltaics (`street_house::materials`), and the cabins' hatch
/// colours.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_house::materials();
    m.extend([
        ("HatchOrange".to_string(), painted([0.80, 0.38, 0.10])),
        ("HatchBlue".to_string(), painted([0.16, 0.30, 0.56])),
        ("HatchGrey".to_string(), steel([0.40, 0.43, 0.48])),
    ]);
    m
}
