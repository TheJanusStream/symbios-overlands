//! Berlin's street house in the outpost's dress (#1598): a hab stack - the
//! Altbau's frontage and storeys built as pressurised modules bolted one on
//! another, each storey a band of hull plating between steel ring frames,
//! viewports in proud frames, an airlock for a front door, and a vaulted
//! hull or an antenna deck for a roof.
//!
//! - **One stack, one hull livery.** The plating is rolled once at the lot
//!   and inherited by every wall below it that names no material of its
//!   own; the rings, frames, hatches and roof name theirs.
//! - **Modules stack.** A steel ring frame runs the whole frontage at every
//!   floor, standing proud of the plating, so a row of stacks shows one
//!   line of rings where its neighbours' meet at the party wall.
//! - **An airlock for a door**: a steel hatch in hazard-striped jambs under
//!   a status lamp, up a grated step. A trading stack has supply counters
//!   either side of it - wide ports over a plinth under a lit sign band; a
//!   stack of quarters has crew ports over a high sill.
//! - **Viewports in frames**: each a port in steel jambs under a steel hood
//!   over a steel sill, a livery stripe (`Pick`ed once) along every module
//!   under them and a conduit up each end, so the storeys read as rows of
//!   modules.
//! - **Gantries stack.** `Pick` decides once per stack whether its end bays
//!   carry a service gantry's grated decks, one on every storey or none.
//! - **The roof is a vault or a deck**, `Pick`ed once: a pressure vault -
//!   a turned hull laid on its side along the street, steel hoops round it,
//!   stopping short of the party walls so a row's vaults read as modules -
//!   or a flat deck
//!   carrying a solar rack and a comms mast with a red beacon.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::catalogue::items::util::glow;
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BEACON_RED, HAZARD_YELLOW, HULL_PANEL, HULL_WHITE, INTERIOR_WARM, PAD_GREY, PV_BLUE,
    STATUS_GREEN, STEEL_DARK, VIEWPORT_LIT, concrete, hull, painted, pv, steel,
};

/// The hab stack (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "space_outpost_street_house",
    name: "Outpost Hab Stack",
    description: "A street house of stacked hab modules: hull-plated storeys between steel ring \
                  frames, framed viewports, an airlock door, and a pressure vault or an antenna \
                  deck on top.",
    themes: &[ThemeArchetype::SpaceOutpost],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // A tall ground module, its airlock and counters, and the crew decks.
    storey_m: (4.2, 3.4),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BB0_0001,
    materials,
    round_meshes: &["Mast", "Vault", "Hoop"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Frame", "Ring", "Fascia", "Balcony", "Housing",
        "Panel", "Mast", "Vault", "Hoop",
    ],
};

/// Hull plating in four liveries, and the steel, hazard paint, lamps, pad
/// and photovoltaics over it.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.52, 0.55, 0.60],
        panes: (2, 1),
        room: INTERIOR_WARM,
        shop: [0.80, 0.96, 1.0],
    });
    m.extend([
        ("HullWhite".to_string(), hull(HULL_WHITE)),
        ("HullGrey".to_string(), hull(HULL_PANEL)),
        ("HullSand".to_string(), hull([0.80, 0.76, 0.66])),
        ("HullBlue".to_string(), hull([0.62, 0.68, 0.76])),
        ("Steel".to_string(), steel(STEEL_DARK)),
        ("Frame".to_string(), steel([0.50, 0.52, 0.56])),
        ("Hazard".to_string(), painted(HAZARD_YELLOW)),
        ("LiveryOrange".to_string(), painted([0.86, 0.42, 0.10])),
        ("LiveryBlue".to_string(), painted([0.14, 0.32, 0.62])),
        ("LiveryRed".to_string(), painted([0.66, 0.12, 0.10])),
        ("HazardDark".to_string(), painted([0.10, 0.10, 0.11])),
        ("Hatch".to_string(), steel([0.40, 0.43, 0.48])),
        ("Sign".to_string(), glow(VIEWPORT_LIT, 1.4)),
        ("Status".to_string(), glow(STATUS_GREEN, 2.0)),
        ("Beacon".to_string(), glow(BEACON_RED, 2.4)),
        ("Deck".to_string(), concrete(PAD_GREY)),
        ("Solar".to_string(), pv(PV_BLUE)),
    ]);
    m
}
