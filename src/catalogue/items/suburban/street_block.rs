//! Berlin's long block in the suburban theme's dress (#1598): the garden
//! apartment block of a post-war suburb - face brick with painted siding
//! panels, a gabled entry porch to each stair, balconies with white rails,
//! and a long shingled roof - grown to the slab's six to twelve storeys.
//!
//! - **One block, one brick**, rolled once at the lot; the siding panels
//!   of the stair columns and the balcony bays roll their own colour once
//!   per block.
//! - **Sections of twelve metres**, as a block is built: each one a stair,
//!   its door under a little gabled porch roof, and a column of siding with
//!   a landing window in each storey over it.
//! - **Balconies stack** at the outer ends of each section's runs where the
//!   block has them (`Pick`ed once): a deck with a white rail.
//! - **Shops or flats on the street**: a trading block has a strip of shops
//!   in its ground floor under a shingled mansard fascia; one of homes has
//!   windows over a brick plinth.
//! - **The roof is a long gable** along the street, its ends the party
//!   walls, or a hipped roof `Pick`ed in its place.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    BRICK_TAN, PORCH_WARM, ROOF_GREY, SIDING_BLUE, SIDING_CREAM, SIDING_SAGE, SIGN_GLOW,
    WOOD_WHITE, brick, concrete, enamel, shingle, siding, wood,
};

/// The block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "suburban_street_block",
    name: "Suburban Garden Block",
    description: "A garden apartment block in face brick with siding panels: a gabled \
                  entry porch to each stair, white-railed balconies, and a long shingled \
                  roof, with a strip of shops below where the street trades.",
    themes: &[ThemeArchetype::Suburban],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // The plinth storey, then the block's low storeys.
    storey_m: (3.6, 2.9),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BA7_0002,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Canopy", "Balcony", "Sign",
    ],
};

/// Three face bricks, the siding panels' colours, white trim and rails,
/// the doors, the shops' fascia and signs, and the shingles.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.93, 0.93, 0.90],
        panes: (2, 2),
        room: PORCH_WARM,
        shop: [1.0, 0.95, 0.84],
    });
    m.extend([
        ("BrickTan".to_string(), brick(BRICK_TAN)),
        ("BrickRed".to_string(), brick([0.55, 0.27, 0.20])),
        ("BrickBuff".to_string(), brick([0.74, 0.64, 0.48])),
        ("SidingBlue".to_string(), siding(SIDING_BLUE)),
        ("SidingCream".to_string(), siding(SIDING_CREAM)),
        ("SidingSage".to_string(), siding(SIDING_SAGE)),
        ("SidingWhite".to_string(), siding([0.88, 0.88, 0.85])),
        ("Trim".to_string(), wood(WOOD_WHITE)),
        ("Deck".to_string(), concrete([0.56, 0.55, 0.52])),
        ("Door".to_string(), enamel([0.18, 0.26, 0.36])),
        ("Fascia".to_string(), enamel([0.12, 0.15, 0.20])),
        (
            "SignLit".to_string(),
            crate::catalogue::items::util::glow(SIGN_GLOW, 1.6),
        ),
        ("Shingle".to_string(), shingle(ROOF_GREY)),
        ("ShingleBrown".to_string(), shingle([0.36, 0.27, 0.21])),
    ]);
    m
}
