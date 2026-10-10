//! Berlin's long block in the outpost's dress (#1598): a hab block - the
//! Plattenbau slab as the colony prints it, a long run of hull-plated
//! module storeys over a regolith-concrete plinth, an airlock to each
//! section and its service core over it, and plant on its roof.
//!
//! - **One block, one hull livery**, rolled once at the lot; the plinth is
//!   pad concrete whatever the hull is, under a hazard-striped band.
//! - **Sections of twelve metres**, as a slab is built: each one an airlock
//!   at street level under a steel canopy, its service core's ports a column
//!   high in each band over it between two spines in the block's livery
//!   (`Pick`ed once), and a plant module on
//!   the roof over it - a hab module laid on a skid beside a beacon mast,
//!   or a solar rack, `Pick`ed once.
//! - **Module bands**: each storey's plating over a steel ring joint, so the
//!   storeys read as the printed courses they are.
//! - **Gantries stack** at the outer ends of each section's runs where the
//!   block has them (`Pick`ed once per block): grated decks, one a storey.
//! - **Depot or quarters on the street**: a trading block has supply
//!   counters in its plinth under a lit sign band; one of quarters has
//!   ports.
//! - **A flat roof** behind a parapet with a steel coping.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::catalogue::items::util::glow;
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BEACON_RED, HAZARD_YELLOW, HULL_PANEL, HULL_WHITE, INTERIOR_WARM, PAD_GREY, PV_BLUE,
    STATUS_GREEN, STEEL_DARK, VIEWPORT_LIT, concrete, hull, painted, pv, steel,
};

/// The hab block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "space_outpost_street_block",
    name: "Outpost Hab Block",
    description: "A long block of printed hab modules: hull-plated storey bands over a concrete \
                  plinth, an airlock and a service core to each section, stacked gantries and \
                  plant on the roof.",
    themes: &[ThemeArchetype::SpaceOutpost],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // The plinth storey, then the slab's low module storeys.
    storey_m: (3.6, 3.0),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BB0_0002,
    materials,
    round_meshes: &["Mast", "Vault"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Frame", "Ring", "Fascia", "Canopy", "Balcony",
        "Housing", "Panel", "Mast", "Vault",
    ],
};

/// Hull plating in four liveries, the plinth's pad concrete, and the
/// steel, hazard paint, lamps and photovoltaics over them.
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
        ("Base".to_string(), concrete([0.34, 0.33, 0.33])),
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
