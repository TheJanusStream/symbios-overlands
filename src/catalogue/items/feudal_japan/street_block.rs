//! Berlin's long block in the Edo theme's dress (#1598): a merchant-row
//! block - a castle town's range of shop-houses and storehouses stacked
//! under a tiled eave at every storey, white kura plaster over a skirt of
//! namako tile, and a long gabled roof of kawara.
//!
//! - **One range, one plaster**, rolled once at the lot; the skirt of
//!   diagonal namako tile runs under the whole ground storey.
//! - **Shop-houses of twelve metres**, each a sliding door between two
//!   lanterns and, where the range trades, shops under noren either side,
//!   barred windows otherwise; a plastered fire-wall pier stands between
//!   each and the next the height of the storeys.
//! - **Stacked eaves**: a tiled pent eave over every storey the whole
//!   frontage.
//! - **Storehouse windows**: the upper storeys' windows barred behind
//!   lattice between thick plaster jambs, each storey on a wainscot of
//!   dark boards.
//! - **The roof**: a long tiled gable along the street, its ends the party
//!   walls.

use std::collections::HashMap;

use crate::catalogue::items::street::{self, Glazing, StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::namako;
use super::street_house::feudal_street_palette;

/// The block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "feudal_japan_street_block",
    name: "Merchant Row Block",
    description: "A long range of Edo shop-houses and storehouses: white plaster over namako \
                  tile, a tiled eave over every storey, fire walls between the houses, noren \
                  and lanterns at the doors, and a long tiled roof.",
    themes: &[ThemeArchetype::FeudalJapan],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // A shop's ground storey, then the storehouse storeys.
    storey_m: (3.8, 3.1),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BA5_0002,
    materials,
    round_meshes: &["Lantern"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Post", "Eave", "Fascia", "Firewall",
    ],
};

/// The Edo street palette, with namako tile for the skirt, behind barred
/// lattice glazing.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = street::opening_materials(Glazing {
        frame: [0.18, 0.13, 0.09],
        panes: (5, 3),
        room: [1.0, 0.78, 0.50],
        shop: [1.0, 0.86, 0.62],
    });
    m.extend(feudal_street_palette());
    m.insert("Namako".to_string(), namako([0.16, 0.17, 0.19]));
    m
}
