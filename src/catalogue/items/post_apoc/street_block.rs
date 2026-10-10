//! Berlin's long block in the wasteland theme's dress (#1598): a Plattenbau
//! slab the survivors are salvaging - its panels stained, its windows
//! boarded or glazed, its front wrapped in a scaffold of rusted poles and
//! plank walks with tarps hung from it, and water drums on its roof.
//!
//! - **One slab, one stain**, rolled once at the lot; the plinth storey
//!   is dark concrete whatever the panels are.
//! - **The scaffold**: a pole at each section's edge from the street to
//!   the parapet, a plank walk at every floor line, and - bay by bay, a
//!   `%` rule - a tarp hung from the walk over a section's outer bays.
//! - **Every window is a window still**: glazed, boarded or half sheeted,
//!   window by window, and a pane shows in every storey however it rolls.
//! - **Shops or flats on the street**: a trading slab has stalls behind
//!   half-drawn shutters under a sign; one of homes has barred windows.
//! - **A flat roof** behind a parapet broken to a jagged line, a water
//!   drum and a worklight over each stair.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::catalogue::items::util::glow;
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    ASH_GREY, CONCRETE_GREY, CORRUGATED_RUST, PLANK_GREY, RUST_BROWN, SIGN_YELLOW, STEEL_GREY,
    TARP_FADED, WORKLIGHT, concrete, enamel, plank, render, rusted, sheet, tarp,
};

/// The block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "post_apoc_street_block",
    name: "Salvaged Block",
    description: "A stained panel slab being stripped for salvage, its front wrapped in a \
                  scaffold of rusted poles, plank walks and hung tarps, its windows boarded.",
    themes: &[ThemeArchetype::PostApoc],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // The plinth storey, then the slab's low prefabricated storeys.
    storey_m: (3.4, 2.9),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BB5_0002,
    materials,
    round_meshes: &["Pole", "Tank"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Board", "Patch", "Shutter", "Fascia", "Pole",
        "Walk", "Tarp", "Tank", "Lamp",
    ],
};

/// Stained panel concrete in three greys, rusted scaffold, salvaged plank
/// and sheet, and two faded tarps.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.34, 0.33, 0.30],
        panes: (2, 2),
        room: [1.0, 0.72, 0.40],
        shop: [1.0, 0.80, 0.52],
    });
    // Lamp and fire light, not mains: the lit rooms glow low and orange.
    m.insert("RoomLit".to_string(), glow([0.92, 0.52, 0.22], 1.3));
    m.extend([
        ("Concrete".to_string(), concrete(CONCRETE_GREY)),
        ("Soot".to_string(), render([0.32, 0.30, 0.28])),
        ("Faded".to_string(), render([0.62, 0.58, 0.46])),
        ("Base".to_string(), concrete(ASH_GREY)),
        ("Scaffold".to_string(), rusted(STEEL_GREY)),
        ("Plank".to_string(), plank(PLANK_GREY)),
        ("RustSheet".to_string(), sheet(CORRUGATED_RUST)),
        ("Sheet".to_string(), sheet(STEEL_GREY)),
        ("Rust".to_string(), rusted(RUST_BROWN)),
        ("Tarp".to_string(), tarp(TARP_FADED)),
        ("TarpBlue".to_string(), tarp([0.22, 0.30, 0.42])),
        ("Door".to_string(), plank([0.30, 0.26, 0.22])),
        ("Sign".to_string(), enamel(SIGN_YELLOW)),
        ("Deck".to_string(), concrete([0.22, 0.21, 0.20])),
        ("Lamp".to_string(), glow(WORKLIGHT, 2.4)),
    ]);
    m
}
