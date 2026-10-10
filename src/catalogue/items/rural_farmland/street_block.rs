//! Berlin's long block in the farm country's dress (#1598): the roller
//! mill and granary of a market town - a long block of brick or of red
//! board-and-batten on a fieldstone base, rows of small square windows,
//! in each section a column of loading doors under a timber lucam (the
//! hoist housing that juts from a mill's roof), the mill's name painted
//! across its front, and a long ribbed-metal roof with galvanised
//! headhouses standing through it.
//!
//! - **One mill, one cladding**, rolled once at the lot: red brick, barn-red
//!   boards or weathered grey boards; the base, the trim, the doors, the
//!   lucams and the roof name theirs.
//! - **Sections of twelve metres**: each one a door up a step to its stair
//!   and, on its axis, a loading door in every storey under a lucam with
//!   its own little gable; over most, a headhouse clearing the ridge.
//! - **A feed store or a mill floor on the street**: a trading block opens
//!   its base in shop bays under a tin canopy; one that only mills has
//!   small windows high in the base. Over either, a sign band the whole
//!   frontage, its lettering standing out of it.
//! - **The roof** is a gable along the street, its ends the party walls.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BARN_RED, LAMP_WARM, ROOF_GREY, SILO_STEEL, STONE_GREY, TRIM_WHITE, WOOD_GREY, barn_board,
    brick, clapboard, enamel, metal_roof, silo_metal, stone, weathered,
};

/// The block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "rural_farmland_street_block",
    name: "Granary Mill Block",
    description: "A long roller mill and granary: brick or red board walls on a fieldstone \
                  base, rows of small windows, a column of loading doors under a jutting \
                  lucam in each section, and a long metal roof.",
    themes: &[ThemeArchetype::RuralFarmland],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // The mill floor, and the bin and loft floors over it.
    storey_m: (4.2, 3.2),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BA8_0002,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Canopy", "Fascia", "Lucam",
    ],
};

/// The mill's brick and boards, its fieldstone base, white trim, the
/// lucams' boards, and the metal over them.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: TRIM_WHITE,
        panes: (3, 3),
        room: LAMP_WARM,
        shop: [1.0, 0.88, 0.62],
    });
    m.extend([
        ("BrickRed".to_string(), brick([0.54, 0.25, 0.18])),
        ("BarnRed".to_string(), barn_board(BARN_RED)),
        ("BoardGrey".to_string(), weathered(WOOD_GREY)),
        ("Stone".to_string(), stone(STONE_GREY)),
        ("Trim".to_string(), clapboard(TRIM_WHITE)),
        ("Door".to_string(), barn_board([0.36, 0.24, 0.15])),
        ("Lucam".to_string(), clapboard(TRIM_WHITE)),
        ("MetalRoof".to_string(), metal_roof(ROOF_GREY)),
        ("Silo".to_string(), silo_metal(SILO_STEEL)),
        ("Sign".to_string(), enamel([0.86, 0.82, 0.70])),
        ("Lettering".to_string(), enamel([0.16, 0.20, 0.16])),
    ]);
    m
}
