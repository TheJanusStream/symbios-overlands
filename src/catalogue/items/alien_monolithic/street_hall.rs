//! Berlin's hall in the monolith's dress (#1600): where it works, a vault
//! hall - a long block of obsidian, its tall portal doors deep in frames of
//! fins over stone aprons, a ribbon of glyph-lit slits over them; where it
//! trades, a hall of light - walls of blue light behind a colonnade of
//! round black pillars under an entablature lit along its edge.
//!
//! - **One block, one stone**, rolled once at the lot; its ends are its
//!   party walls, blank, so halls stand flush in a row, and a lone hall's
//!   ends read as its obsidian's own. A glyph line runs the front at the
//!   floor of an upper hall, in the hall's one glyph colour.
//! - **A works or a hall of light.** A works hall (`Trade` 0) has a portal
//!   for people at one end and bays of great portal doors under glowing
//!   lintels along the rest, some bays blank, fins between them; a hall of
//!   light (`Trade` 1) has its pillars along the whole front, a portal in
//!   every wide bay and a sign band over them.
//! - **The crown**, `Pick`ed once: a capstone slab over the whole hall,
//!   standing out front and back, glyph-light along its underside's edges;
//!   a low pyramidion along the street over a heavy cornice; or setback
//!   tiers on a deck, each lit along its front and back edges.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

/// The hall (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "alien_monolithic_street_hall",
    name: "Monolith Vault Hall",
    description: "A long obsidian hall: great portal doors deep in frames of fins under glowing \
                  lintels and a ribbon of glyph-lit slits - or, where it trades, walls of light \
                  behind a colonnade of black pillars.",
    themes: &[ThemeArchetype::AlienMonolithic],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A vault's monumental clear height, and an upper hall over it.
    storey_m: (7.0, 4.4),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BB4_0005,
    materials,
    round_meshes: &["Pillar"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Fin", "Cap", "Cornice", "Fascia", "Pillar",
    ],
};

/// The monolith's street palette: obsidian in four casts, the fins, portal
/// and step stone, the deck, and the glyph-light of the lines, lintels and
/// sign band.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    super::street_house::materials()
}
