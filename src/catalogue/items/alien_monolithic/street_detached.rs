//! Berlin's detached house in the monolith's dress (#1600): a dwelling
//! block standing alone on its plinth of dead ground - one block of black
//! obsidian, glyph-lit slits between fins on all four faces, a glyph line
//! wrapping it at every upper floor, a portal under a glowing lintel, and a
//! capstone, a pyramid or stepped tiers on top.
//!
//! - **It stands in from its lot.** Its block stands `Eave` in from every
//!   side, so its capstone reaches out over it to the lot's edges and no
//!   further.
//! - **One block, one stone.** The obsidian is rolled once at the lot; the
//!   fins, glyphs, portal and crown name theirs, as the monolith house's
//!   do, and the dwelling has one glyph colour.
//! - **Slits on every face**: tall narrow slits between proud fins, barred
//!   into glyph-like segments, on the front, the back and both flanks; a
//!   glyph line wraps the block at the floor of every upper storey, and
//!   the walls stand to the top.
//! - **A portal for a door**: a tall dark leaf deep in a frame of fins,
//!   under a glowing lintel, up a stone step.
//! - **The crown**, `Pick`ed once: a capstone slab reaching out to the
//!   lot's edges, glyph-light along its underside; a pyramid on a cornice,
//!   its eaves lit by a glyph fascia; or setback tiers on a cornice, each
//!   capped with a glyph line.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

/// The dwelling block (see the module docs); its rules are
/// `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "alien_monolithic_street_detached",
    name: "Monolith Dwelling",
    description: "A dwelling block of black obsidian standing alone: glyph-lit slits between fins \
                  on every face, a glyph line at every floor, a portal under a glowing lintel, and \
                  a capstone, a pyramid or stepped tiers.",
    themes: &[ThemeArchetype::AlienMonolithic],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // A tall ground storey for the portal, and monumental rooms over it.
    storey_m: (3.8, 3.4),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BB4_0004,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Fin", "Cap", "Cornice",
    ],
};

/// The monolith's street palette: obsidian in four casts, the fins and
/// portal stone, the deck, and the glyph-light.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    super::street_house::materials()
}
