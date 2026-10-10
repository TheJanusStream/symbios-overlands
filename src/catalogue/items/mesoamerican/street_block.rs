//! Berlin's long block in the Mesoamerican theme's dress (#1598): a palace
//! range - a long painted house of many doorways, its ground storey a
//! portico of square piers, its storeys banded by talud and frieze, and
//! a crest of merlons along its roof.
//!
//! - **One range, one stucco**, rolled once at the lot; the portico's piers
//!   and the mouldings are pale limestone.
//! - **The portico**: a row of square piers standing in front of the
//!   ground storey under a long lintel beam, and behind them a doorway to
//!   each section and, where the range trades, stall openings over stone
//!   benches; otherwise windows.
//! - **Sections of twelve metres**, each its doorway under a jade-studded
//!   lintel; the storeys over it a run of deep openings under timber
//!   lintels, each storey on a stepped talud.
//! - **Friezes**: a painted step-fret frieze over every storey, red or
//!   jade (`Pick`ed once).
//! - **The crest**: stepped merlons along the parapet, the flat roof
//!   behind it.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::street_house::meso_street_palette;

/// The block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "mesoamerican_street_block",
    name: "Palace Range Block",
    description: "A long painted palace range: a portico of square piers along the street, \
                  storeys banded by talud and step-fret friezes, deep openings under timber \
                  lintels and a crest of stepped merlons.",
    themes: &[ThemeArchetype::Mesoamerican],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // The portico storey, then the range's storeys.
    storey_m: (4.2, 3.2),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BA6_0002,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Talud", "Frieze", "Lintel", "Merlon", "Pier",
    ],
};

/// The Mesoamerican street palette, behind firelit openings.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.30, 0.22, 0.14],
        panes: (1, 1),
        room: [1.0, 0.62, 0.32],
        shop: [1.0, 0.78, 0.46],
    });
    m.extend(meso_street_palette());
    m
}
