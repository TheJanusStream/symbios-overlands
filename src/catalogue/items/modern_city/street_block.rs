//! Berlin's long block in the modern city's dress (#1598): the Plattenbau
//! slab of the 1970s and 80s - the WBS 70 of the inner east, renovated in
//! pastel render, or left in its grey concrete - on a dark plinth storey.
//!
//! - **One slab, one panel colour**, rolled once at the lot; the plinth
//!   storey is dark whatever the panels are.
//! - **Sections of twelve metres**, as a slab is built: each one a stair,
//!   its door at street level under a canopy, its landing windows a column
//!   high in each band over it, and a lift's housing over it on the roof.
//! - **Panel bands**: each storey's panels over a recessed joint, so the
//!   storeys read as the prefabricated courses they are.
//! - **Loggias stack** at the outer ends of each section's runs where the
//!   slab has them (`Pick`ed once per slab).
//! - **Shops or flats on the street**: a trading slab has shopfronts in its
//!   plinth, the shops of a Kaufhalle row; one of homes has windows.
//! - **A flat roof** behind a parapet with a metal coping.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{LAMP_WARM, concrete, enamel, steel, stucco};

/// The block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "city_street_block",
    name: "City Long Block",
    description: "A Berlin Plattenbau slab: pastel or grey panel bands over a dark plinth, \
                  a stair and a canopied door to each section, stacked loggias and a flat roof.",
    themes: &[ThemeArchetype::ModernCity],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // The plinth storey, then the slab's low prefabricated storeys.
    storey_m: (3.4, 2.9),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BA0_0002,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Fascia", "Canopy", "Balcony",
    ],
};

/// Renovated render in the east's pastels, raw panel concrete, and the
/// plinth, joints, loggia fronts and metalwork over them.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.90, 0.90, 0.88],
        panes: (2, 2),
        room: LAMP_WARM,
        shop: [0.98, 0.96, 0.90],
    });
    m.extend([
        ("Cream".to_string(), stucco([0.86, 0.83, 0.74])),
        ("Sand".to_string(), stucco([0.80, 0.72, 0.56])),
        ("Sage".to_string(), stucco([0.62, 0.69, 0.60])),
        ("Terracotta".to_string(), stucco([0.80, 0.56, 0.44])),
        ("Concrete".to_string(), concrete([0.60, 0.60, 0.58])),
        ("Base".to_string(), concrete([0.27, 0.27, 0.28])),
        ("Joint".to_string(), concrete([0.20, 0.20, 0.21])),
        ("Accent".to_string(), stucco([0.92, 0.92, 0.90])),
        ("Metal".to_string(), steel([0.46, 0.48, 0.50])),
        ("Door".to_string(), enamel([0.16, 0.18, 0.20])),
        ("Deck".to_string(), concrete([0.18, 0.18, 0.19])),
    ]);
    m
}
