//! Berlin's street house in the monolith's dress (#1598): a black monolith
//! house - the Altbau's frontage and storeys cut from one block of polished
//! obsidian, its windows narrow slits lit by glyph-light from within, a
//! tall portal for a door, a glyph line at every floor, and a stepped
//! crown or a pyramidion on top.
//!
//! - **One block, one stone.** The obsidian is rolled once at the lot and
//!   inherited by every wall below it that names no material of its own;
//!   the fins, glyphs, portal and crown name theirs.
//! - **Glyph lines mark the storeys**: a thin line of glyph-light runs the
//!   whole frontage at every floor, standing just proud of the stone, so a
//!   row of houses shows one unbroken line where its neighbours' meet at
//!   the party wall; now and then a pier between two slits carries a glyph
//!   stroke.
//! - **Slits for windows**: groups of tall narrow slits between proud fins,
//!   each slit's light barred into glyph-like segments (the `Glass` card
//!   is obsidian, the rooms behind it glyph-light, cyan where lit and a dim
//!   violet where not).
//! - **A portal for a door**: a tall dark leaf under a glowing lintel,
//!   recessed in a deep frame, up a stone step. A trading house has light
//!   windows either side of it over a plinth under a glyph sign band; a
//!   house of homes has slits over a high sill.
//! - **The crown is stepped or pointed**, `Pick`ed once: setback tiers
//!   back from the street, each capped with a glyph line, or a pyramidion
//!   along the street, its ends the party walls.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::catalogue::items::util::{glow, window_card};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{ENERGY_BLUE, GLYPH_CYAN, GLYPH_VIOLET, OBSIDIAN, obsidian, stone};

/// The monolith house (see the module docs); its rules are
/// `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "alien_monolithic_street_house",
    name: "Monolith House",
    description: "A street house cut from black obsidian: tall glyph-lit slits between fins, a \
                  glyph line at every floor, a portal door under a glowing lintel, and a stepped \
                  crown or a pyramidion.",
    themes: &[ThemeArchetype::AlienMonolithic],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // Monumental storeys: a tall ground storey, its portal, and the rest.
    storey_m: (4.6, 3.8),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BB4_0001,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Fin", "Cornice", "Fascia", "Cap",
    ],
};

/// The monolith's openings: an obsidian card barring each slit into
/// glyph-like segments, cyan glyph-light behind a lit one and a dim violet
/// behind the rest, and a shop's light window glowing electric blue.
pub(crate) fn slit_materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: OBSIDIAN,
        panes: (1, 3),
        room: GLYPH_CYAN,
        shop: ENERGY_BLUE,
    });
    m.extend([
        ("Glass".to_string(), window_card(OBSIDIAN, 1, 3, 0.42, 0.10)),
        (
            "ShopGlass".to_string(),
            window_card(OBSIDIAN, 3, 2, 0.38, 0.05),
        ),
        ("RoomLit".to_string(), glow(GLYPH_CYAN, 1.6)),
        ("RoomDark".to_string(), glow(GLYPH_VIOLET, 0.45)),
        ("ShopLit".to_string(), glow([0.30, 0.55, 1.0], 1.8)),
    ]);
    m
}

/// Obsidian in four casts, the fins and portal stone, and the glyph-light
/// of the lines, marks and lintels.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = slit_materials();
    m.extend([
        ("Obsidian".to_string(), obsidian(OBSIDIAN)),
        ("ObsidianBlue".to_string(), obsidian([0.05, 0.07, 0.14])),
        ("ObsidianViolet".to_string(), obsidian([0.09, 0.05, 0.13])),
        ("Basalt".to_string(), stone([0.17, 0.17, 0.20])),
        ("Fin".to_string(), obsidian([0.11, 0.11, 0.16])),
        ("Portal".to_string(), obsidian([0.03, 0.03, 0.05])),
        ("Step".to_string(), stone([0.28, 0.28, 0.32])),
        ("Glyph".to_string(), glow(GLYPH_CYAN, 2.0)),
        ("GlyphViolet".to_string(), glow(GLYPH_VIOLET, 2.0)),
        ("Energy".to_string(), glow(ENERGY_BLUE, 2.4)),
        ("Deck".to_string(), stone([0.12, 0.12, 0.15])),
    ]);
    m
}
