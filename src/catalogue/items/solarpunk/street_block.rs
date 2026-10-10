//! Berlin's long block in the solarpunk theme's dress (#1598): a
//! timber-frame eco-block - a grid of dark timber posts and beams over
//! board or lime infill, vertical gardens climbing its bays, planted
//! balconies, and a roof garden under a pergola of solar panels.
//!
//! - **One block, one infill**, rolled once at the lot; the frame is dark
//!   timber whatever the infill is.
//! - **Sections of twelve metres**, as a slab is built: a stair's glazed
//!   column over a door under a green canopy.
//! - **The frame**: a post at every bay's edge and a beam at every floor
//!   line, standing out from the infill, so the block reads as its grid.
//! - **Gardens and balconies**: the outer bays of each section's runs
//!   carry planted balconies, a vertical garden climbing the block's whole
//!   height with its windows open through the leaves, or plain windows
//!   (`Pick`ed once); a solar shade over every other plain window.
//! - **Shops or flats on the street**: a trading block has a market's
//!   shopfronts under a timber fascia; one of homes has windows over a
//!   planted plinth.
//! - **The roof**: a green roof behind a parapet, and a pergola of solar
//!   panels on steel legs over each section.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    CONCRETE_PALE, LAMP_WARM, LEAF_GREEN, MOSS_GREEN, PV_BLUE, STEEL_WHITE, TIMBER_WARM, concrete,
    foliage, pv, steel, timber,
};

/// The block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "solarpunk_street_block",
    name: "Timber Eco-Block",
    description: "A timber-frame eco-block: a grid of dark posts and beams over board or lime \
                  infill, vertical gardens up its bays, and solar panels over a roof garden.",
    themes: &[ThemeArchetype::Solarpunk],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // The market storey, then the timber-framed flats.
    storey_m: (3.8, 3.1),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BAF_0002,
    materials,
    round_meshes: &[],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Frame", "Balcony", "Planter", "Green", "Shade",
        "PV", "Fascia", "Canopy",
    ],
};

/// Larch, silvered and honey board, lime render, dark frame timber, three
/// greens, the solar panels and the white steel they stand on.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.42, 0.32, 0.20],
        panes: (2, 2),
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
