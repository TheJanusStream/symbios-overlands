//! Berlin's low building in the solarpunk theme's dress (#1598): where it
//! houses people, a row of cob and timber cottages under one green roof;
//! where it trades, a greenhouse market - a timber frame glazed to the
//! ridge, crates of greens along its front under a canopy.
//!
//! - **The cottages**: as many as the lot's frontage holds, each a
//!   round-headed window and a door under a planted hood, a rain barrel
//!   beside its door; the walls rolled once for the row (cob, lime or
//!   larch), and a sod roof along the street, its gable ends the party
//!   walls, a solar panel set into it over some of the cottages; where it
//!   has two storeys, a timber storey of windows over them.
//! - **The greenhouse market**: bays of tall glazing between timber posts
//!   either side of a door, planters of greens at the glass's feet, a green
//!   canopy along the front and a clerestory over it, and a glass roof
//!   along the street, its gable ends the party walls; where it has two
//!   storeys, a gallery's ribbon of windows.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    COB_EARTH, CONCRETE_PALE, CROP_GREEN, GLASS_CLEAN, LAMP_WARM, LEAF_GREEN, MOSS_GREEN, PV_BLUE,
    TIMBER_WARM, concrete, foliage, pane_grid, pv, timber,
};

/// The low building (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "solarpunk_street_low",
    name: "Green-Roof Cottages",
    description: "A row of cob and timber cottages under one sod roof - or, where the street \
                  trades, a greenhouse market glazed to the ridge with crates of greens.",
    themes: &[ThemeArchetype::Solarpunk],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // A cottage's or the market's storey, and a timber storey over it.
    storey_m: (3.4, 2.9),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BAF_0003,
    materials,
    round_meshes: &["Barrel"],
    solid_meshes: &[
        "Wall",
        "Door",
        "Pane",
        "Roof",
        "Trim",
        "Frame",
        "Green",
        "Planter",
        "Canopy",
        "PV",
        "Barrel",
        "Glasshouse",
    ],
};

/// Cob, lime render and larch, dark frame timber, the sod and the greens,
/// the greenhouse's glass and the solar panels.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.42, 0.32, 0.20],
        panes: (2, 2),
        room: LAMP_WARM,
        shop: [0.96, 0.98, 0.86],
    });
    m.extend([
        ("Cob".to_string(), concrete(COB_EARTH)),
        ("Lime".to_string(), concrete(CONCRETE_PALE)),
        ("Larch".to_string(), timber(TIMBER_WARM)),
        ("Frame".to_string(), timber([0.36, 0.26, 0.16])),
        ("Sod".to_string(), foliage(MOSS_GREEN)),
        ("Leaf".to_string(), foliage(LEAF_GREEN)),
        ("Crop".to_string(), foliage(CROP_GREEN)),
        ("Bloom".to_string(), foliage([0.72, 0.36, 0.50])),
        (
            "GlassRoof".to_string(),
            pane_grid(GLASS_CLEAN, 0.25, (4, 2)),
        ),
        ("PV".to_string(), pv(PV_BLUE)),
        ("Door".to_string(), timber([0.30, 0.20, 0.12])),
    ]);
    m
}
