//! Berlin's long block in the fantasy theme's dress (#1598): an elven guild
//! hall range - ashlar storeys between pale buttress piers, tall lancet
//! windows under pointed hoods, a round stair tower to each section under a
//! slate cone, a row of pinnacles along its parapet, and a steep slate
//! roof.
//!
//! - **One range, one stone**, rolled once at the lot - sandstone, grey or
//!   rosy; the piers, courses and towers are the palest stone, so they
//!   stand out of any of them, and the plinth is mossy.
//! - **Sections of twelve metres**, each a door under a gilded pointed
//!   hood with a glowing rune over it, and over the door a round stair
//!   tower on a corbel of rings, a lancet in each storey, rising through
//!   the parapet to a slate cone with a gold tip.
//! - **Buttress piers** stand proud between the bays the whole height of
//!   the storeys, a string course at every floor.
//! - **Shops or halls on the street**: a trading range has guild shops in
//!   its ground storey under painted signs; one of homes has windows.
//! - **The crown**: a parapet with a gold-tipped pinnacle over every pier,
//!   and behind it a steep slate roof along the street, verdigris or blue
//!   (`Pick`ed with a flat roof walk, once).

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{CRYSTAL_CYAN, GOLD, STONE_MOSS, gold, matte, mossy, slate, stone, timber};

/// The block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "fantasy_street_block",
    name: "Elven Guild Range",
    description: "A long elven guild hall: ashlar storeys between pale buttress piers, lancet \
                  windows under pointed hoods, rune-lit doors under round stair towers with \
                  slate cones, gold-tipped pinnacles and a steep slate roof.",
    themes: &[ThemeArchetype::Fantasy],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // A hall's ground storey, then the range's storeys.
    storey_m: (4.0, 3.2),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BB1_0002,
    materials,
    round_meshes: &["Spire"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Pier", "Hood", "Spire", "Fascia", "Cornice",
    ],
};

/// Elven ashlars, the pale trim stone, a mossy plinth, gilding, a verdigris
/// or blue slate, and the runes' cold light.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.62, 0.52, 0.30],
        panes: (2, 4),
        room: [1.0, 0.80, 0.54],
        shop: [0.80, 0.90, 1.0],
    });
    m.extend([
        ("Sandstone".to_string(), stone([0.80, 0.70, 0.54])),
        ("Grey".to_string(), stone([0.64, 0.64, 0.62])),
        ("Rosy".to_string(), stone([0.78, 0.66, 0.62])),
        ("Plinth".to_string(), mossy(STONE_MOSS)),
        ("Trim".to_string(), stone([0.90, 0.88, 0.82])),
        ("Gold".to_string(), gold(GOLD)),
        ("Door".to_string(), timber([0.30, 0.18, 0.10])),
        ("Sign".to_string(), matte([0.18, 0.22, 0.46])),
        (
            "Rune".to_string(),
            crate::catalogue::items::util::glow(CRYSTAL_CYAN, 2.4),
        ),
        ("Slate".to_string(), slate([0.26, 0.38, 0.36])),
        ("SlateBlue".to_string(), slate([0.28, 0.30, 0.44])),
        ("Deck".to_string(), stone([0.40, 0.40, 0.40])),
    ]);
    m
}
