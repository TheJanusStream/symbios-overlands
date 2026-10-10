//! Berlin's hall in the steampunk theme's dress (#1600): the sheds of a
//! gaslight industrial quarter - a brick engine works of iron-framed bays
//! and great plank doors, or, where it trades, an iron-and-glass market
//! hall under a row of glazed gables.
//!
//! - **One hall, one brick**, rolled once at the lot; its sides are party
//!   walls, blank, so halls stand flush in a row and a lone hall's ends
//!   read as its brick's own. An iron band runs along its front at the
//!   top of every storey.
//! - **A works or a market.** A works hall (`Trade` 0) has a plank door
//!   for its hands at one end and bays of great plank doors under iron
//!   lintels along the rest, some bays blank brick, a ribbon of
//!   brass-framed high windows over them all; a market hall (`Trade` 1)
//!   has tall iron-framed shop windows between cast-iron pilasters either
//!   side of its doors, under a sign board with a brass rail and an iron
//!   canopy along the whole front.
//! - **The roof** is a works' sawtooth of amber northlights or a low gable
//!   along the street with a glazed lantern on its ridge, `Pick`ed once,
//!   copper stacks standing on it; a market's is a row of glazed gables
//!   to the street, each its own span.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{GLASS_AMBER, iron, pane_grid, slate};

/// The hall (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "steampunk_street_hall",
    name: "Steam Works Hall",
    description: "A brick engine works of great plank doors under a sawtooth of amber \
                  northlights or a lantern roof and copper stacks - or, where it trades, an \
                  iron-and-glass market hall under a row of glazed gables.",
    themes: &[ThemeArchetype::Steampunk],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A works hall's clear height below, and a drawing office over it.
    storey_m: (6.0, 4.0),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BAE_0005,
    materials,
    round_meshes: &["Stack"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Band", "Fascia", "Stack", "Canopy",
    ],
};

/// The workshop's brick, iron, brass, copper, rusted sheeting and amber
/// northlights (`street_low::materials`), with riveted plate, slate, and
/// the market's glazed roof.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_low::materials();
    m.extend([
        ("IronPlate".to_string(), iron([0.30, 0.30, 0.32])),
        ("Slate".to_string(), slate([0.24, 0.25, 0.28])),
        ("GlassRoof".to_string(), pane_grid(GLASS_AMBER, 0.3, (4, 3))),
    ]);
    m
}
