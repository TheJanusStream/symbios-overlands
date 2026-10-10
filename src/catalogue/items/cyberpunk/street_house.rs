//! Berlin's street house in the cyberpunk theme's dress (#1598): an Altbau
//! refitted for the neon city - dark panelled metal or grimy concrete, a
//! neon tube along every floor line, air-conditioning boxes hung under its
//! windows, holo-sign fascias over its ground floor, a blade sign down one
//! end, and a lit billboard on its roof.
//!
//! - **One house, one skin, one neon.** The cladding is rolled once at the
//!   lot, and the neon's colour - cyan, magenta, lime or amber - is
//!   `Pick`ed once, for the floor-line tubes and the blade sign.
//! - **Windows and their boxes**: window by window a `%` rule hangs an AC
//!   unit under the sill or leaves the wall bare, and draws a grille
//!   shutter half down over the glass or leaves it open.
//! - **Shops or homes**: a trading house has shopfronts under glowing holo
//!   fascias, each sign its own colour; a house of homes has shuttered
//!   windows over a high sill and a lit door.
//! - **The blade sign**: a lit panel standing out from the front at one
//!   end, storey on storey from the first floor up, a tube along its edge,
//!   where the house has one (`Pick`ed once, and its colour with it).
//! - **The roof**: a deck behind a parapet with a steel coping, a mast,
//!   and a billboard on legs, its two lit panels glowing toward the
//!   street.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::catalogue::items::util::glow;
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{DARK_METAL, NEON_CYAN, NEON_LIME, NEON_MAGENTA, concrete, grille, metal};

/// The deep amber of the theme's warmer neon.
const NEON_AMBER: [f32; 3] = [1.0, 0.50, 0.06];

/// The house (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "cyberpunk_street_house",
    name: "Neon Tenement",
    description: "A tenement refitted for the neon city: dark metal or grimy concrete, a neon \
                  tube on every floor, AC boxes under its windows and holo signs over its shops.",
    themes: &[ThemeArchetype::Cyberpunk],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // A shop's tall ground storey, then cramped flats.
    storey_m: (4.6, 3.2),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BA3_0001,
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
        "Mast",
        "Billboard",
    ],
};

/// Dark panelled metal and grimy concrete, the four neons and their holo
/// signs, AC boxes and grilles.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.10, 0.11, 0.14],
        panes: (2, 2),
        room: [0.70, 0.82, 1.0],
        shop: [1.0, 0.62, 0.88],
    });
    m.extend([
        ("Metal".to_string(), metal(DARK_METAL)),
        ("Gunmetal".to_string(), metal([0.16, 0.17, 0.20])),
        ("Concrete".to_string(), concrete([0.34, 0.34, 0.36])),
        ("Stained".to_string(), concrete([0.24, 0.23, 0.24])),
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
