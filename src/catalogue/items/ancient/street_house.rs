//! Berlin's street house in the classical theme's dress (#1598): a Roman
//! insula as Ostia stands them - plastered or brick-faced storeys over a
//! row of tabernae, small shuttered windows under brick arches, a timber
//! maenianum along the first floor where the house has one, a travertine
//! cornice and a low tiled roof.
//!
//! - **One insula, one face.** The plaster or the brick is rolled once at
//!   the lot and inherited by every wall below it that names no material of
//!   its own; travertine, timber, doors and tiles name theirs.
//! - **Tabernae or a domus front.** A trading house (`Trade`) has shop bays
//!   between travertine piers either side of its entrance, each a wide
//!   opening over a masonry counter under a painted sign; a house of homes
//!   shows the street small windows set high, as a domus does.
//! - **The maenianum or a pentice.** `Pick` decides once per house
//!   whether a timber balcony runs the frontage at the first floor - a
//!   deck over the ground storey, a railing at its edge - or a pentice of
//!   tiles on a timber beam shelters the street.
//! - **Shuttered windows**: each under a brick flat arch over a travertine
//!   sill, most of them (`%` per window) between timber shutters.
//! - **A low tiled roof** along the street, its gable ends the party walls,
//!   over a travertine cornice the whole frontage.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{TERRACOTTA, adobe, brick, roof_tile, sandstone, terracotta, wood};

/// The street house (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "ancient_street_house",
    name: "Roman Insula",
    description: "A Roman insula: plastered or brick storeys over a row of tabernae shops, small \
                  windows under brick lintels, a timber balcony and a low tiled roof.",
    themes: &[ThemeArchetype::AncientClassical],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // The tabernae's tall ground storey, and the insula's storeys over it.
    storey_m: (4.2, 3.3),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BA1_0001,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Cornice", "Balcony", "Fascia",
    ],
};

/// Pompeian plasters and opus latericium, travertine dressings, timber,
/// the painted signs, and the tiles over them.
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
        ("Timber".to_string(), wood([0.42, 0.27, 0.15])),
        ("Door".to_string(), wood([0.30, 0.18, 0.10])),
        ("Tile".to_string(), roof_tile(TERRACOTTA)),
        ("Sign".to_string(), terracotta([0.56, 0.14, 0.10])),
    ]);
    m
}
