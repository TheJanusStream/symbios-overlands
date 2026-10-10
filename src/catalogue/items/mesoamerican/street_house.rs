//! Berlin's street house in the Mesoamerican theme's dress (#1598): a
//! painted stucco town house - every storey a sloping talud under a framed
//! tablero, a painted step-fret frieze along each tablero, deep openings
//! under timber lintels, and a crest of stepped merlons along its roof.
//!
//! - **One house, one stucco**, rolled once at the lot - cream, red, ochre
//!   or turquoise; the frames, friezes and lintels name their own.
//! - **Shops or a home.** A trading house has market stalls either side
//!   of its door, each an opening over a stone bench under a palm-thatch
//!   awning; a house of homes has openings over a high sill.
//! - **Talud and tablero**: each storey stands on a battered talud, a
//!   stepped slope the whole frontage, and its windows sit in a tablero
//!   framed by proud stucco mouldings; the frieze band over them is
//!   painted in step-frets, red or jade (`Pick`ed once).
//! - **The roof** is flat behind a parapet crowned by a row of stepped
//!   merlons, and where the house has one (`Pick`ed once) a pierced roof
//!   comb over its middle bearing a gold sun disc.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    GOLD_WARM, JADE_GREEN, LIMESTONE_PALE, STONE_GREY, STUCCO_CREAM, STUCCO_RED, TIMBER_BROWN,
    cobble, gold, jade, limestone, painted, patterned_floor, thatch, timber,
};

/// The house (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "mesoamerican_street_house",
    name: "Painted Talud House",
    description: "A painted stucco town house: a talud and a framed tablero to every storey, \
                  step-fret friezes, deep openings under timber lintels, market stalls under \
                  palm thatch and a crest of stepped merlons.",
    themes: &[ThemeArchetype::Mesoamerican],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // A market storey, then the house's storeys.
    storey_m: (4.0, 3.4),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BA6_0001,
    materials,
    round_meshes: &["Disc"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Talud", "Frieze", "Lintel", "Merlon", "Canopy",
        "Disc",
    ],
};

/// Painted stuccos, pale limestone, step-fret friezes, timber, jade, gold
/// and palm thatch, behind firelit openings.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.30, 0.22, 0.14],
        panes: (1, 1),
        room: [1.0, 0.62, 0.32],
        shop: [1.0, 0.78, 0.46],
    });
    m.extend(meso_street_palette());
    m
}

/// The stuccos, stones, friezes and timbers every Mesoamerican street
/// building shares (#1598): the block's and the low building's palettes
/// start from it. Its `Glass` is an open reed screen over the opening,
/// not glass: a frame and a faint card.
pub(crate) fn meso_street_palette() -> Vec<(String, SovereignMaterialSettings)> {
    vec![
        (
            "Glass".to_string(),
            crate::catalogue::items::util::window_card([0.30, 0.22, 0.14], 1, 1, 0.22, 0.07),
        ),
        ("Cream".to_string(), painted(STUCCO_CREAM)),
        ("Red".to_string(), painted(STUCCO_RED)),
        ("Ochre".to_string(), painted([0.80, 0.58, 0.30])),
        ("Turquoise".to_string(), painted([0.28, 0.56, 0.54])),
        ("White".to_string(), painted([0.88, 0.86, 0.80])),
        ("Adobe".to_string(), painted([0.66, 0.48, 0.32])),
        ("Stone".to_string(), limestone(LIMESTONE_PALE)),
        ("Grey".to_string(), limestone(STONE_GREY)),
        ("Frieze".to_string(), patterned_floor(STUCCO_RED)),
        ("FriezeJade".to_string(), patterned_floor(JADE_GREEN)),
        ("Lintel".to_string(), timber(TIMBER_BROWN)),
        ("Door".to_string(), timber([0.30, 0.18, 0.10])),
        ("Jade".to_string(), jade(JADE_GREEN)),
        ("Gold".to_string(), gold(GOLD_WARM)),
        ("Thatch".to_string(), thatch([0.70, 0.56, 0.30])),
        ("Deck".to_string(), cobble(STONE_GREY)),
    ]
}
