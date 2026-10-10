//! Berlin's detached house in the Mesoamerican theme's dress (#1600): a
//! painted stucco house standing alone in its garden on a stepped stone
//! talud, deep openings under timber lintels on all four sides, a stone
//! moulding over every storey, and over it a flat roof behind a painted
//! step-fret frieze crowned with stepped merlons, or a steep palm-thatch
//! hip.
//!
//! - **One house, one stucco**, rolled once at the lot - cream, red, ochre,
//!   turquoise or white, the street house's own - and inherited by every
//!   wall; the talud, the frieze, the lintels and the roof name their own.
//! - **It stands in from its lot.** Its walls stand `Eave` in from every
//!   side, so a thatch's eaves reach out to the lot's edges and no
//!   further.
//! - **Windows on all four sides.** The door stands up a stone step under a
//!   jade-studded lintel, as the street house's does; every face has its
//!   deep openings under timber lintels, and a stepped talud of three
//!   stone courses runs round the house's foot.
//! - **The roof** is `Pick`ed once: flat behind a parapet, its roof beams'
//!   ends standing out under a painted band and a row of stepped merlons on
//!   top, as the town's houses are crowned; or a steep palm-thatch hip, the
//!   country house of the Maya.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::street_house::meso_street_palette;

/// The house (see the module docs); its rules are `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "mesoamerican_street_detached",
    name: "Painted Garden House",
    description: "A painted stucco house alone in its garden on a stepped stone talud: deep \
                  openings under timber lintels on every side, a step-fret frieze, and a \
                  merloned flat roof or a steep palm-thatch hip.",
    themes: &[ThemeArchetype::Mesoamerican],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // A house's rooms: the ground storey a little taller, where it is
    // entered.
    storey_m: (3.2, 3.0),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BA6_0004,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Talud", "Frieze", "Lintel", "Merlon", "Beam",
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
