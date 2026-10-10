//! Berlin's street house in the wasteland theme's dress (#1598): an Altbau
//! that outlived its city - stained render and bare concrete, its windows
//! boarded or sheeted or still glazed, its wounds patched with scrap, its
//! cornice broken, and a shack of salvage on its roof.
//!
//! - **One ruin, one render**, rolled once at the lot: soot-black, faded
//!   ochre, bare concrete or brick-red under lost plaster.
//! - **Every window is a window still.** Window by window a `%` rule
//!   leaves it glazed, nails boards across it or hangs a rusted sheet
//!   over half of it - a pane shows in every storey however it rolls.
//! - **Scrap patches**: here and there a wall bay is a rusted sheet or a
//!   tarp, standing a hand's breadth proud of the wall it replaced.
//! - **Shops or homes**: a trading house is a barricaded store, its
//!   shutters half down under a hand-painted plank sign; a house of homes
//!   has its ground floor sheeted and boarded, a door up a step.
//! - **Soot**: smoke from an old fire has blackened the wall over some
//!   windows, window by window.
//! - **A broken crown**: the cornice survives in pieces, the attic wall
//!   over it is broken to a jagged line, and on the deck behind it stand a
//!   water drum and a lookout's barrel fire, and a salvage shack where the
//!   house has one (`Pick`ed once).

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::catalogue::items::util::glow;
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    ASH_GREY, CONCRETE_GREY, CORRUGATED_RUST, FIRE_ORANGE, PLANK_GREY, RUST_BROWN, SIGN_YELLOW,
    STEEL_GREY, TARP_FADED, concrete, enamel, plank, render, rusted, sheet, tarp,
};

/// The house (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "post_apoc_street_house",
    name: "Patched Tenement",
    description: "A tenement that outlived its city: stained render, windows boarded, sheeted \
                  or still glazed, scrap patches, a broken cornice and a shack on its roof.",
    themes: &[ThemeArchetype::PostApoc],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // The Altbau's tall rooms, as it was built.
    storey_m: (4.2, 3.4),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BB5_0001,
    materials,
    round_meshes: &["Tank"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Cornice", "Board", "Patch", "Shutter", "Fascia",
        "Balcony", "Shack", "Tank", "Fire",
    ],
};

/// Stained render and concrete, salvaged plank and sheet, the rust on all
/// of it, and the fire on the roof.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.32, 0.30, 0.27],
        panes: (2, 2),
        room: [1.0, 0.70, 0.36],
        shop: [1.0, 0.78, 0.48],
    });
    // Lamp and fire light, not mains: the lit rooms glow low and orange.
    m.insert("RoomLit".to_string(), glow([0.92, 0.52, 0.22], 1.3));
    m.extend([
        ("Soot".to_string(), render([0.30, 0.28, 0.26])),
        ("Ochre".to_string(), render([0.62, 0.53, 0.38])),
        ("Concrete".to_string(), concrete(CONCRETE_GREY)),
        ("BrickRed".to_string(), render([0.50, 0.31, 0.24])),
        ("Trim".to_string(), render([0.56, 0.54, 0.50])),
        ("Plank".to_string(), plank(PLANK_GREY)),
        ("RustSheet".to_string(), sheet(CORRUGATED_RUST)),
        ("Sheet".to_string(), sheet(STEEL_GREY)),
        ("Rust".to_string(), rusted(RUST_BROWN)),
        ("Tarp".to_string(), tarp(TARP_FADED)),
        ("Door".to_string(), plank([0.30, 0.26, 0.22])),
        ("Sign".to_string(), enamel(SIGN_YELLOW)),
        ("Deck".to_string(), concrete(ASH_GREY)),
        ("Fire".to_string(), glow(FIRE_ORANGE, 3.0)),
    ]);
    m
}
