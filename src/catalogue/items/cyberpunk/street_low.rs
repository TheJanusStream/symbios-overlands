//! Berlin's low building in the cyberpunk theme's dress (#1598): where it
//! houses people, a row of container homes stacked on a lot, a door and a
//! lit window to each box; where it trades, a noodle bar and its market
//! stalls - counters under a lit awning, paper lanterns and a holo sign.
//!
//! - **The containers**: one box after another along the lot, each in its
//!   own corrugated colour, a door and a window apiece and a neon strip
//!   over the door; where it has two storeys, a second row of boxes on a
//!   catwalk over the first.
//! - **The noodle bar**: stall by stall, a counter under a serving window
//!   and a grille shutter rolled up over it, a door between the stalls, an
//!   awning along the whole front with a neon tube at its edge and red
//!   lanterns hung from it, and over it a holo sign to each stall in the
//!   stall's own colour; a flat roof behind a parapet.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::catalogue::items::util::glow;
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    CONTAINER_BLUE, CONTAINER_RUST, DARK_METAL, NEON_CYAN, NEON_LIME, NEON_MAGENTA, TARP_BLUE,
    concrete, corrugated, grille, metal, tarp,
};

/// The deep amber of the theme's warmer neon.
const NEON_AMBER: [f32; 3] = [1.0, 0.50, 0.06];

/// The low building (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "cyberpunk_street_low",
    name: "Container Row",
    description: "A row of stacked container homes, a neon strip over each door - or, where the \
                  street trades, a noodle bar's stalls under a lit awning and paper lanterns.",
    themes: &[ThemeArchetype::Cyberpunk],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // A container's or a stall's storey, and a box stacked over it.
    storey_m: (3.6, 2.9),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BA3_0003,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Neon", "Sign", "Awning", "Lantern", "Counter",
        "Shutter", "Rail", "Walk",
    ],
};

/// Container steel in four colours, dark metal, the stalls' tarp, grille
/// and counters, and the neons, holo signs and red lanterns.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.10, 0.11, 0.14],
        panes: (2, 1),
        room: [0.70, 0.82, 1.0],
        shop: [1.0, 0.70, 0.40],
    });
    m.extend([
        ("BoxBlue".to_string(), corrugated(CONTAINER_BLUE)),
        ("BoxRust".to_string(), corrugated(CONTAINER_RUST)),
        ("BoxTeal".to_string(), corrugated([0.14, 0.34, 0.32])),
        ("BoxGrey".to_string(), corrugated([0.40, 0.42, 0.44])),
        ("Metal".to_string(), metal(DARK_METAL)),
        ("Steel".to_string(), metal([0.30, 0.32, 0.36])),
        ("Grille".to_string(), grille()),
        ("Tarp".to_string(), tarp(TARP_BLUE)),
        ("Counter".to_string(), metal([0.42, 0.40, 0.38])),
        ("NeonCyan".to_string(), glow(NEON_CYAN, 4.0)),
        ("NeonMagenta".to_string(), glow(NEON_MAGENTA, 4.0)),
        ("NeonLime".to_string(), glow(NEON_LIME, 4.0)),
        ("NeonAmber".to_string(), glow(NEON_AMBER, 4.0)),
        ("HoloCyan".to_string(), glow(NEON_CYAN, 1.6)),
        ("HoloMagenta".to_string(), glow(NEON_MAGENTA, 1.6)),
        ("HoloAmber".to_string(), glow(NEON_AMBER, 1.6)),
        ("Lantern".to_string(), glow([1.0, 0.18, 0.10], 2.6)),
        ("Door".to_string(), metal([0.10, 0.10, 0.12])),
        ("Deck".to_string(), concrete([0.16, 0.16, 0.17])),
    ]);
    m
}
