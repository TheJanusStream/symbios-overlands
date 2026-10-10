//! Berlin's low building in the hive's dress (#1598): where it houses the
//! brood, a row of pods grown shoulder to shoulder, each under its own
//! domed carapace; where it trades, a spore market - a fleshy hall of the
//! kind that fills a gap in a hive street with one or two storeys.
//!
//! - **The pod row**: as many pods as the lot's frontage holds, each a pod
//!   window and a maw between bone ribs - every pod its own flesh at the
//!   maw - and, where it has two storeys, two pods over them; one shell
//!   rolled for the row, a fleshy roll at every floor, and a domed carapace
//!   over every pod - three turned, tapered shells, one on the next, some
//!   crowned with a glowing spore vent - so the row reads as a cluster of
//!   humps.
//! - **The spore market**: feeding stalls in bays, a maw where a bay is
//!   wide enough, a deep fleshy bolster - a tube laid along the whole
//!   front - for a canopy, and a glowing
//!   spore band over it; a ribbon of pods where it has two storeys; and a
//!   carapace laid along the street over it all, ringed by bone hoops.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::catalogue::items::util::glow;
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::street_house::pod_materials;
use super::{CHITIN_DARK, CHITIN_GREEN, FLESH_PINK, FLESH_RED, HUSK, SAC_GLOW, chitin, flesh};

/// The low building (see the module docs); its rules are `street_low.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "alien_organic_street_low",
    name: "Hive Pod Row",
    description: "A row of brood pods under domed carapaces, each with its own maw, or - where \
                  the street trades - a spore market under a fleshy canopy and a glowing band.",
    themes: &[ThemeArchetype::AlienOrganic],
    kind: StreetKind::Low,
    band: ProsperityBand::ANY,
    // A market's tall ground storey, and a pod's upper one.
    storey_m: (3.6, 2.9),
    rules: include_str!("street_low.cga"),
    seed: 0x005E_EDA1_7BB3_0003,
    materials,
    round_meshes: &["Tube", "Pod", "Carapace", "Hoop"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Rib", "Lip", "Fascia", "Tube", "Pod", "Carapace",
        "Hoop",
    ],
};

/// Chitin shells in three hues, the maws' flesh, ribs and sinew, and the
/// spore band's glow.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = pod_materials();
    m.extend([
        ("ChitinViolet".to_string(), chitin(CHITIN_DARK)),
        ("ChitinGreen".to_string(), chitin(CHITIN_GREEN)),
        ("ChitinAmber".to_string(), chitin([0.34, 0.22, 0.10])),
        ("Rib".to_string(), chitin(HUSK)),
        ("Carapace".to_string(), chitin([0.16, 0.12, 0.20])),
        ("Flesh".to_string(), flesh(FLESH_RED)),
        ("FleshDark".to_string(), flesh([0.36, 0.16, 0.22])),
        ("Lip".to_string(), flesh(FLESH_PINK)),
        ("Sinew".to_string(), flesh([0.42, 0.20, 0.24])),
        ("Spore".to_string(), glow(SAC_GLOW, 1.8)),
    ]);
    m
}
