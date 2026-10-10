//! Berlin's street house in the solarpunk theme's dress (#1598): a timber
//! town house of the eco-quarter - a larch, silvered or honey board front
//! on a pale lime ground storey, planted balconies, solar shades over its
//! windows, and a green roof under rows of panels.
//!
//! - **One house, one timber**, rolled once at the lot; the ground storey
//!   is pale lime render whatever the boards are, and the frame - posts at
//!   the party walls, a beam at every floor line - is dark timber.
//! - **Planted balconies**: each bay's balconies are a deck with a planter
//!   of greens along its front, `%` rolled bay by bay and storey by storey,
//!   so the front greens unevenly as a lived-in house does.
//! - **Solar shades and creepers**: a blade of panel over every window
//!   that is not a balcony's, standing out from the boards; here and there
//!   a creeper climbs the pier beside a window.
//! - **Shops or homes**: a trading house has a greengrocer's or a cafe's
//!   windows under a timber fascia with planters at their feet; a house of
//!   homes has raised windows and a door up a step.
//! - **The roof**: a green roof behind a timber parapet, a sawtooth of
//!   solar panels over it, or a pitched roof of panels along the street
//!   (`Pick`ed once).

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    CONCRETE_PALE, LAMP_WARM, LEAF_GREEN, MOSS_GREEN, PV_BLUE, STEEL_WHITE, TIMBER_WARM, concrete,
    foliage, pv, steel, timber,
};

/// The house (see the module docs); its rules are `street_house.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "solarpunk_street_house",
    name: "Timber Townhouse",
    description: "A timber town house of the eco-quarter: board front on a lime ground storey, \
                  planted balconies, solar shades over its windows and a green roof of panels.",
    themes: &[ThemeArchetype::Solarpunk],
    kind: StreetKind::House,
    band: ProsperityBand::ANY,
    // A shop's or a hall's ground storey, then timber-framed storeys.
    storey_m: (4.0, 3.3),
    rules: include_str!("street_house.cga"),
    seed: 0x005E_EDA1_7BAF_0001,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Frame", "Balcony", "Planter", "Green", "Shade",
        "PV", "Fascia",
    ],
};

/// Larch, silvered and honey board, lime render, dark frame timber, the
/// greens of the balconies and the roof, and the solar panels.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.42, 0.32, 0.20],
        panes: (2, 3),
        room: LAMP_WARM,
        shop: [1.0, 0.90, 0.66],
    });
    m.extend([
        ("Larch".to_string(), timber(TIMBER_WARM)),
        ("Silver".to_string(), timber([0.58, 0.56, 0.52])),
        ("Honey".to_string(), timber([0.70, 0.54, 0.32])),
        ("Lime".to_string(), concrete(CONCRETE_PALE)),
        ("Frame".to_string(), timber([0.36, 0.26, 0.16])),
        ("Leaf".to_string(), foliage(LEAF_GREEN)),
        ("Moss".to_string(), foliage(MOSS_GREEN)),
        ("Bloom".to_string(), foliage([0.72, 0.36, 0.50])),
        ("PV".to_string(), pv(PV_BLUE)),
        ("Steel".to_string(), steel(STEEL_WHITE)),
        ("Door".to_string(), timber([0.30, 0.20, 0.12])),
    ]);
    m
}
