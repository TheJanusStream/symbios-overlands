//! Berlin's long block in the classical theme's dress (#1598): a long
//! insula block over a stoa - a marble colonnade along the street carrying
//! a travertine entablature, the shops or the homes set back behind it, and
//! storeys of brick and plaster over it to a cornice and a low tiled roof.
//!
//! - **One block, one face**, rolled once at the lot: plaster or brick.
//! - **The stoa**: columns in a steady rhythm the whole frontage, standing
//!   out from the ground storey under an entablature, so a row of blocks
//!   shows one unbroken colonnade where their porticoes meet.
//! - **Sections of twelve metres**, each a stair: a doorway between
//!   travertine jambs behind the colonnade, and a column of landing
//!   windows on its axis; shuttered windows under brick flat arches either
//!   side of it, and a travertine course at every third floor.
//! - **Tabernae or homes** behind the colonnade: a trading block has shop
//!   bays between travertine piers, each over a counter under a painted
//!   sign; one of homes has small windows set high.
//! - **A low tiled roof** along the street over a heavy cornice.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    MARBLE_WHITE, TERRACOTTA, adobe, brick, marble, roof_tile, sandstone, terracotta, wood,
};

/// The long block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "ancient_street_block",
    name: "Roman Stoa Block",
    description: "A long Roman insula block over a marble stoa: shops behind a colonnade, \
                  storeys of brick and plaster with paired windows, a heavy cornice and a low \
                  tiled roof.",
    themes: &[ThemeArchetype::AncientClassical],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // The stoa's tall ground storey, and the block's storeys over it.
    storey_m: (4.6, 3.1),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BA1_0002,
    materials,
    round_meshes: &["Column"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Cornice", "Fascia", "Column",
    ],
};

/// The insula's plasters and brick, the marble of the colonnade, travertine
/// dressings, timber, doors, signs and the tiles over them.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.36, 0.25, 0.15],
        panes: (2, 2),
        room: [1.0, 0.70, 0.40],
        shop: [1.0, 0.80, 0.52],
    });
    m.extend([
        ("PlasterOchre".to_string(), adobe([0.90, 0.70, 0.46])),
        ("PlasterRed".to_string(), adobe([0.64, 0.30, 0.22])),
        ("PlasterCream".to_string(), adobe([0.94, 0.88, 0.74])),
        ("Brick".to_string(), brick([0.68, 0.38, 0.25])),
        ("Travertine".to_string(), sandstone([0.84, 0.79, 0.68])),
        ("Marble".to_string(), marble(MARBLE_WHITE)),
        ("Timber".to_string(), wood([0.42, 0.27, 0.15])),
        ("Door".to_string(), wood([0.30, 0.18, 0.10])),
        ("Tile".to_string(), roof_tile(TERRACOTTA)),
        ("Sign".to_string(), terracotta([0.56, 0.14, 0.10])),
    ]);
    m
}
