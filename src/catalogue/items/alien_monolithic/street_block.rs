//! Berlin's long block in the monolith's dress (#1598): a monolith block -
//! the Plattenbau slab as one long black slab of obsidian, storey bands of
//! glyph-lit slits over a basalt plinth, a portal to each section with a
//! glyph spine climbing over it, and an obelisk or a capstone on its roof.
//!
//! - **One slab, one stone**, rolled once at the lot; the plinth storey is
//!   basalt whatever the slab is.
//! - **Sections of twelve metres**, as a slab is built: each one a portal at
//!   street level under a glowing lintel and a stone canopy, a spine of
//!   glyph-light climbing its axis between two slits a storey and two fins,
//!   and an obelisk
//!   or a capstone on the roof over it, `Pick`ed once.
//! - **Slit bands**: each storey's slits over a glyph line, so the storeys
//!   read as the courses of one inscription.
//! - **Fins stand** at the outer ends of each section's runs where the slab
//!   has them (`Pick`ed once per slab): a deep obsidian blade a storey.
//! - **Light or slits on the street**: a trading slab has light windows in
//!   its plinth under a glyph sign band; one of homes has slits.
//! - **A flat roof** behind a parapet with a glyph-lit coping.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::catalogue::items::util::glow;
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::street_house::slit_materials;
use super::{ENERGY_BLUE, GLYPH_CYAN, GLYPH_VIOLET, OBSIDIAN, obsidian, stone};

/// The monolith block (see the module docs); its rules are
/// `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "alien_monolithic_street_block",
    name: "Monolith Block",
    description: "A long black slab of obsidian: storey bands of glyph-lit slits over a basalt \
                  plinth, a portal and a climbing glyph spine to each section, and obelisks on \
                  the roof.",
    themes: &[ThemeArchetype::AlienMonolithic],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // The plinth storey, then the slab's storeys.
    storey_m: (4.0, 3.2),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BB4_0002,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Fin", "Cornice", "Fascia", "Canopy", "Cap",
    ],
};

/// Obsidian in four casts, the basalt plinth, the fins and portal stone,
/// and the glyph-light of the lines, spines and lintels.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = slit_materials();
    m.extend([
        ("Obsidian".to_string(), obsidian(OBSIDIAN)),
        ("ObsidianBlue".to_string(), obsidian([0.05, 0.07, 0.14])),
        ("ObsidianViolet".to_string(), obsidian([0.09, 0.05, 0.13])),
        ("Basalt".to_string(), stone([0.17, 0.17, 0.20])),
        ("Base".to_string(), stone([0.10, 0.10, 0.12])),
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
