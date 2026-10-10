//! Berlin's long block in the roadside strip's dress (#1598): the Googie
//! motor lodge or office slab of the highway's edge - bands of enamel
//! spandrel panels in the strip's colours between ribbons of windows,
//! cream brick ends, a porte-cochere canopy at each entrance, a neon blade
//! sign standing out from one end, and a butterfly-roofed pavilion on top.
//!
//! - **One slab, one panel colour**, rolled once at the lot: turquoise,
//!   orange, lemon or coral spandrels, every storey the same; a chrome
//!   strip caps each band.
//! - **Sections of twelve metres**, as a slab is built: an entrance in each
//!   one at street level under a deep canopy with a chrome edge.
//! - **Shops or a lobby on the street**: a trading slab has shopfronts
//!   under a lit sign band between its entrances; one that does not has a
//!   ribbon of lobby windows.
//! - **The blade sign**: a tall fin standing out from the front at one
//!   end, `Pick`ed left or right, its faces gridded with neon cells.
//! - **A butterfly roof** - two slopes rising out to the ends - over a
//!   glazed pavilion on the flat roof behind the parapet.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::catalogue::items::util::glow;
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    CANOPY_LIT, CHROME_BRIGHT, CONCRETE_GREY, ENAMEL_CREAM, NEON_CYAN, NEON_RED, SIGN_AMBER, brick,
    chrome, concrete, enamel,
};

/// The motor lodge (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "roadside_street_block",
    name: "Googie Motor Lodge",
    description: "A Googie motor lodge slab: enamel spandrel bands between ribbon windows, \
                  cream brick ends, porte-cochere canopies, a neon blade sign and a \
                  butterfly-roofed pavilion on top.",
    themes: &[ThemeArchetype::Roadside],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // The lobby storey, then the lodge's room storeys.
    storey_m: (4.0, 3.1),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BAB_0002,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Canopy", "Fascia", "Sign",
    ],
};

/// Four enamel spandrels, cream brick, chrome strips, the canopies, the
/// sign and its neon, and the pavilion's roof.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.78, 0.80, 0.84],
        panes: (3, 1),
        room: [1.0, 0.88, 0.66],
        shop: CANOPY_LIT,
    });
    m.extend([
        ("PanelTeal".to_string(), enamel([0.22, 0.62, 0.62])),
        ("PanelOrange".to_string(), enamel([0.96, 0.52, 0.18])),
        ("PanelLemon".to_string(), enamel([0.98, 0.88, 0.42])),
        ("PanelCoral".to_string(), enamel([0.88, 0.42, 0.40])),
        ("BrickCream".to_string(), brick([0.88, 0.82, 0.68])),
        ("Base".to_string(), brick([0.50, 0.40, 0.32])),
        ("Chrome".to_string(), chrome(CHROME_BRIGHT)),
        ("Canopy".to_string(), enamel(ENAMEL_CREAM)),
        ("Door".to_string(), chrome([0.40, 0.42, 0.46])),
        ("Fascia".to_string(), enamel([0.10, 0.12, 0.16])),
        ("SignBack".to_string(), enamel([0.10, 0.12, 0.16])),
        ("NeonRed".to_string(), glow(NEON_RED, 3.0)),
        ("NeonCyan".to_string(), glow(NEON_CYAN, 3.0)),
        ("NeonAmber".to_string(), glow(SIGN_AMBER, 3.0)),
        ("Deck".to_string(), concrete(CONCRETE_GREY)),
    ]);
    m
}
