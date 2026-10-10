//! Berlin's detached house in the wasteland theme's dress (#1600): a family
//! house that outlived its suburb, standing alone in a garden gone to
//! weeds - stained render, its windows boarded or sheeted or still glazed,
//! its wounds patched with scrap, and a roof patched with corrugated
//! sheet.
//!
//! - **It stands in from its lot.** Its walls stand `Eave` in from every
//!   side, so its roof reaches out over them to the lot's edges and no
//!   further.
//! - **One house, one render**, rolled once at the lot: soot-black, faded
//!   ochre, bare concrete or brick-red, as the tenement's.
//! - **Every window is a window still**, on every side: window by window a
//!   `%` rule leaves it glazed, nails boards across it or hangs a rusted
//!   sheet over half of it, and here and there a wall bay is a scrap patch.
//! - **A door up a step**, and (`Pick`ed once) a rusted rain drum beside
//!   it or a lean-to porch of corrugated sheet on rusted posts over it.
//! - **A patched roof**, `Pick`ed once: a gable along the street, its
//!   tiles patched strip by strip with rusted and bare sheet; a hipped
//!   roof, a slope here and there gone to rusted sheet; or a lean-to of
//!   corrugated sheet rising to the back.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{concrete, sheet};

/// The house (see the module docs); its rules are `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "post_apoc_street_detached",
    name: "Patched Family House",
    description: "A suburban house that outlived its city: stained render, windows boarded, \
                  sheeted or still glazed on every side, scrap patches, a sheet porch and a roof \
                  patched with corrugated sheet.",
    themes: &[ThemeArchetype::PostApoc],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // A family house's rooms, as it was built.
    storey_m: (3.0, 2.8),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BB5_0004,
    materials,
    round_meshes: &["Post", "Tank"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Board", "Patch", "Post", "Awning", "Tank",
    ],
};

/// The tenement's palette - stained render and concrete, plank, sheet,
/// rust and tarp - with the old roof tiles and a dark roof sheet.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_house::materials();
    m.extend([
        ("Tiles".to_string(), concrete([0.40, 0.25, 0.20])),
        ("TilesDark".to_string(), concrete([0.24, 0.23, 0.23])),
        ("Roof".to_string(), sheet([0.30, 0.27, 0.24])),
    ]);
    m
}
