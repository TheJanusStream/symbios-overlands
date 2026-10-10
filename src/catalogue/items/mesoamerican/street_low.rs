//! Berlin's low building in the Mesoamerican theme's dress (#1598): where it
//! houses people, a row of flat-roofed adobe houses with their roof beams
//! showing; where it trades, a market hall under a deep palm-thatch roof.
//!
//! - **The adobe row**: as many houses as the frontage holds, each a door
//!   under a timber lintel and a small deep window, a painted band along
//!   the parapet and the ends of the roof beams standing out of the wall
//!   under it; one adobe rolled for the row.
//! - **The market hall**: stall openings over stone benches in bays between
//!   painted piers, a doorway under a jade-studded lintel, and over it a
//!   steep thatch along the street, its ends the party walls.
//! - **An upper storey** where it has two: on a stepped talud, small deep
//!   openings under lintels.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::street_house::meso_street_palette;

/// The low building (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "mesoamerican_street_low",
    name: "Adobe House Row",
    description: "A row of flat-roofed adobe houses with their roof beams showing and a painted \
                  parapet band - or, where the street trades, a market hall under a deep \
                  palm thatch.",
    themes: &[ThemeArchetype::Mesoamerican],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // A ground storey, and an upper one.
    storey_m: (3.4, 2.9),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BA6_0003,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Talud", "Lintel", "Beam", "Pier",
    ],
};

/// The Mesoamerican street palette, behind firelit openings.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.30, 0.22, 0.14],
        panes: (1, 1),
        room: [1.0, 0.60, 0.30],
        shop: [1.0, 0.78, 0.46],
    });
    m.extend(meso_street_palette());
    m
}
