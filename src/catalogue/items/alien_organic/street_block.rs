//! Berlin's long block in the hive's dress (#1598): a hive block - the
//! Plattenbau slab grown as one long body, storey after storey of brood
//! cells in a chitin shell over a fleshy foot, a maw to each section with
//! a segmented gullet climbing over it, and sacs and spines on its back.
//!
//! - **One body, one shell**, rolled once at the lot; the foot storey is
//!   flesh whatever the shell is.
//! - **Sections of twelve metres**, as a slab is built: each one a maw at
//!   street level under a fleshy brow, a gullet of fleshy segments climbing
//!   its axis, ringed by a bone hoop at every floor, and a brood sac with a
//!   glowing tip or a pair of spines
//!   on the roof over it, `Pick`ed once.
//! - **Cell bands**: each storey's cells over a fleshy roll - a tube laid
//!   along the street, half sunk in the shell - so the storeys read as the
//!   segments they are, bone ribs standing proud between the cells, and
//!   every cell one of three sizes, chosen cell by cell.
//! - **Sac ledges stack** at the outer ends of each section's runs where the
//!   block has them (`Pick`ed once per block).
//! - **Stalls or brood on the street**: a trading block has feeding stalls
//!   in its foot under a glowing spore band; one of homes has pods.
//! - **A flat back** behind a chitin parapet with a carapace coping.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::catalogue::items::util::glow;
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::street_house::pod_materials;
use super::{CHITIN_DARK, CHITIN_GREEN, FLESH_PINK, FLESH_RED, HUSK, SAC_GLOW, chitin, flesh};

/// The hive block (see the module docs); its rules are `street_block.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "alien_organic_street_block",
    name: "Hive Block",
    description: "A long hive body of brood cells in a ribbed chitin shell over a fleshy foot, \
                  a maw and a climbing gullet to each section, sac ledges and brood sacs on its \
                  back.",
    themes: &[ThemeArchetype::AlienOrganic],
    kind: StreetKind::Block,
    band: ProsperityBand::ANY,
    // The foot storey, then the low brood storeys.
    storey_m: (3.6, 3.0),
    rules: include_str!("street_block.cga"),
    seed: 0x005E_EDA1_7BB3_0002,
    materials,
    round_meshes: &["Tube", "Pod", "Hoop"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Rib", "Lip", "Shell", "Fascia", "Canopy", "Spike",
        "Tube", "Pod", "Hoop",
    ],
};

/// Chitin shells in four hues, the fleshy foot, ribs and sinew, and the
/// glow of the spore bands and the sacs.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = pod_materials();
    m.extend([
        ("ChitinViolet".to_string(), chitin(CHITIN_DARK)),
        ("ChitinGreen".to_string(), chitin(CHITIN_GREEN)),
        ("ChitinAmber".to_string(), chitin([0.34, 0.22, 0.10])),
        ("ChitinRust".to_string(), chitin([0.38, 0.20, 0.18])),
        ("Rib".to_string(), chitin(HUSK)),
        ("Carapace".to_string(), chitin([0.16, 0.12, 0.20])),
        ("Foot".to_string(), flesh([0.36, 0.20, 0.22])),
        ("Flesh".to_string(), flesh(FLESH_RED)),
        ("Lip".to_string(), flesh(FLESH_PINK)),
        ("Sinew".to_string(), flesh([0.42, 0.20, 0.24])),
        ("Spore".to_string(), glow(SAC_GLOW, 1.8)),
        ("Deck".to_string(), flesh([0.24, 0.16, 0.20])),
    ]);
    m
}
