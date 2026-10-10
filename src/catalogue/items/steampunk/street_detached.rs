//! Berlin's detached house in the steampunk theme's dress (#1600): an
//! engineer's villa of the gaslight age standing free in its garden -
//! sooty brick between riveted iron bands, brass-framed windows on every
//! side, copper downpipes at its front corners and copper stacks over its
//! roof, the street's own townhouse (`street_house`) let out of its row.
//!
//! - **It stands in from its lot.** Its walls stand `Eave` in from every
//!   side, so its roof reaches out over them to the lot's edges and no
//!   further.
//! - **One villa, one brick**, rolled once at the lot; iron, brass and
//!   copper name their own. An iron band runs round it at every floor.
//! - **Windows on all four sides**, each in a brass frame under an iron
//!   lintel with a brass keystone; the garden side has a tall window to
//!   the lawn in one bay of two.
//! - **The door** stands up an iron step under a fanlight, `Pick`ed once
//!   under an iron hood or a deeper iron porch canopy on its brackets.
//! - **One roof**, `Pick`ed once: a steep mansard all round in riveted
//!   iron plate, slate or verdigris copper, a hipped roof, or a gable along
//!   the street; a pair of copper stacks with brass caps stands over it.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::copper;

/// The villa (see the module docs); its rules are `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "steampunk_street_detached",
    name: "Engineer's Villa",
    description: "A gaslight engineer's villa in its garden: sooty brick between riveted iron \
                  bands, brass-framed windows all round, copper downpipes and stacks, and an \
                  iron-plated mansard, a hipped or a gabled roof.",
    themes: &[ThemeArchetype::Steampunk],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // A villa's tall rooms: a little taller below, where it is entered.
    storey_m: (3.4, 3.1),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BAE_0004,
    materials,
    round_meshes: &["Pipe", "Stack"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Band", "Pipe", "Stack", "Canopy",
    ],
};

/// The street house's brick, iron, brass, copper and slate
/// (`street_house::materials`), and a verdigris copper for a roof.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_house::materials();
    m.extend([("Verdigris".to_string(), copper([0.34, 0.54, 0.46]))]);
    m
}
