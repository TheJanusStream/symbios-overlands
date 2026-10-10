//! Berlin's long block in the cyberpunk theme's dress (#1598): a megablock
//! slab - stacked storeys of concrete or dark panel, a neon band every few
//! floors, capsule balconies with lit edges, an arcade of holo-signed
//! shops along the street, and a billboard across its roof.
//!
//! - **One block, one skin, one neon**, rolled and `Pick`ed once: the
//!   cladding for every panel, the neon for its bands and the balconies'
//!   lit edges.
//! - **Sections of twelve metres**, as the slab is built: a lit doorway at
//!   street level under a canopy, a stair's column of slit windows over it,
//!   a mast over it on the roof.
//! - **Neon bands** run the whole frontage at every other floor line.
//! - **Capsule balconies stack** at the ends of each section's runs where
//!   the block has them (`Pick`ed once), each a pod with a lit lip.
//! - **The arcade or the flats**: a trading block has shopfronts under
//!   holo fascias, sign by sign its own colour; a block of homes has
//!   grilled windows over a dark plinth.
//! - **The roof**: a parapet with a neon line under its coping, and a
//!   billboard on legs over the middle of the roof, its lit panels glowing
//!   toward the street.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::catalogue::items::util::glow;
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{DARK_METAL, NEON_CYAN, NEON_LIME, NEON_MAGENTA, concrete, grille, metal};

/// The deep amber of the theme's warmer neon.
const NEON_AMBER: [f32; 3] = [1.0, 0.50, 0.06];

/// The block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "cyberpunk_street_block",
    name: "Neon Megablock",
    description: "A megablock slab of concrete or dark panel: neon bands every other floor, \
                  stacked capsule balconies, an arcade of holo signs and a rooftop billboard.",
    themes: &[ThemeArchetype::Cyberpunk],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // The arcade's storey, then the stacked flats.
    storey_m: (4.2, 3.0),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BA3_0002,
    materials,
    round_meshes: &["Mast"],
    solid_meshes: &[
        "Wall",
        "Door",
        "Pane",
        "Roof",
        "Trim",
        "Neon",
        "Sign",
        "AC",
        "Grille",
        "Balcony",
        "Canopy",
        "Mast",
        "Billboard",
    ],
};

/// Grimy concrete in two greys and dark panel, the four neons and their
/// holo signs, the capsules' light metal, AC boxes and grilles.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.10, 0.11, 0.14],
        panes: (2, 2),
        room: [0.70, 0.82, 1.0],
        shop: [1.0, 0.62, 0.88],
    });
    m.extend([
        ("Concrete".to_string(), concrete([0.40, 0.40, 0.42])),
        ("Stained".to_string(), concrete([0.26, 0.25, 0.26])),
        ("Metal".to_string(), metal(DARK_METAL)),
        ("Gunmetal".to_string(), metal([0.16, 0.17, 0.20])),
        ("Capsule".to_string(), metal([0.56, 0.58, 0.60])),
        ("Steel".to_string(), metal([0.30, 0.32, 0.36])),
        ("AC".to_string(), metal([0.52, 0.54, 0.56])),
        ("Grille".to_string(), grille()),
        ("NeonCyan".to_string(), glow(NEON_CYAN, 4.0)),
        ("NeonMagenta".to_string(), glow(NEON_MAGENTA, 4.0)),
        ("NeonLime".to_string(), glow(NEON_LIME, 4.0)),
        ("NeonAmber".to_string(), glow(NEON_AMBER, 4.0)),
        ("HoloCyan".to_string(), glow(NEON_CYAN, 1.6)),
        ("HoloMagenta".to_string(), glow(NEON_MAGENTA, 1.6)),
        ("HoloAmber".to_string(), glow(NEON_AMBER, 1.6)),
        ("Door".to_string(), metal([0.10, 0.10, 0.12])),
        ("Deck".to_string(), concrete([0.16, 0.16, 0.17])),
    ]);
    m
}
