//! Berlin's low building in the monolith's dress (#1598): where it houses
//! people, a row of obsidian cells under one heavy capstone slab; where it
//! trades, a glyph pavilion - a colonnade of black pillars before walls of
//! light, the kind of hall that fills a gap in a monolith street with one
//! or two storeys.
//!
//! - **The cells**: as many as the lot's frontage holds, each a glyph-lit
//!   slit and a portal under a glowing lintel - every cell its own glyph
//!   colour - and, where it has two storeys, two slits over them; one stone
//!   rolled for the row, and a capstone slab over it, standing proud front
//!   and back, its edge lit by a glyph line.
//! - **The pavilion**: walls of light behind a colonnade of round obsidian
//!   pillars, a portal where a bay is wide enough, a glyph sign band over
//!   them; a ribbon of slits where it has two storeys; and the capstone
//!   over it all.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::catalogue::items::util::glow;
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::street_house::slit_materials;
use super::{ENERGY_BLUE, GLYPH_CYAN, GLYPH_VIOLET, OBSIDIAN, obsidian, stone};

/// The low building (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "alien_monolithic_street_low",
    name: "Glyph Pavilion",
    description: "A row of obsidian cells with glyph-lit slits and portals under one capstone, \
                  or - where the street trades - a pavilion of light behind black pillars.",
    themes: &[ThemeArchetype::AlienMonolithic],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // A pavilion's tall ground storey, and a cell's upper one.
    storey_m: (4.0, 3.0),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BB4_0003,
    materials,
    round_meshes: &["Pillar"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Trim", "Fin", "Cornice", "Fascia", "Cap", "Pillar",
    ],
};

/// Obsidian in three casts, the portal and step stone, and the glyph-light
/// of the lintels, lines and sign band.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = slit_materials();
    m.extend([
        ("Obsidian".to_string(), obsidian(OBSIDIAN)),
        ("ObsidianBlue".to_string(), obsidian([0.05, 0.07, 0.14])),
        ("ObsidianViolet".to_string(), obsidian([0.09, 0.05, 0.13])),
        ("Fin".to_string(), obsidian([0.11, 0.11, 0.16])),
        ("Portal".to_string(), obsidian([0.03, 0.03, 0.05])),
        ("Step".to_string(), stone([0.28, 0.28, 0.32])),
        ("Glyph".to_string(), glow(GLYPH_CYAN, 2.0)),
        ("GlyphViolet".to_string(), glow(GLYPH_VIOLET, 2.0)),
        ("Energy".to_string(), glow(ENERGY_BLUE, 2.4)),
    ]);
    m
}
