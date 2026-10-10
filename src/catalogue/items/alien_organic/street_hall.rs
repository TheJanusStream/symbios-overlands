//! Berlin's hall in the hive's dress (#1600): where it works, a digestion
//! hall - a long ribbed body of chitin, its great sphincter doors sealed
//! with fleshy leaves between bone fangs, a ribbon of pods over them; where
//! it trades, a spore market hall - feeding stalls along the front under a
//! fleshy bolster and a glowing spore band.
//!
//! - **One hall, one shell**, rolled once at the lot; its ends are its
//!   party walls, blank, so halls stand flush in a row, and a lone hall's
//!   ends read as its shell's own.
//! - **Ribbed bays**: a bone rib stands proud between every bay of the
//!   front, and a fleshy roll runs its length under the crown, so a long
//!   front reads as one body's flank.
//! - **A works or a market.** A works hall (`Trade` 0) has a small maw for
//!   the drones at one end and bays of wide sphincters over chitin aprons,
//!   some bays blank, under a ribbon of pods; a market (`Trade` 1) has
//!   stalls of glowing membrane over a plinth and a maw in every wide bay,
//!   under the bolster and the band.
//! - **The carapace**, `Pick`ed once: one great carapace laid along the
//!   street, ringed by bone hoops; or a row of lesser ones, side by side
//!   from the street to the back, like the segments of a grub.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::chitin;

/// The hall (see the module docs); its rules are `street_hall.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "alien_organic_street_hall",
    name: "Hive Digestion Hall",
    description: "A long ribbed hall of chitin: fleshy sphincter doors between bone fangs under a \
                  ribbon of pods and a hooped carapace - or, where it trades, a spore market of \
                  glowing stalls under a fleshy bolster.",
    themes: &[ThemeArchetype::AlienOrganic],
    kind: StreetKind::Hall,
    band: ProsperityBand::ANY,
    // A hall's tall ground segment, and a brood storey over it.
    storey_m: (6.4, 4.0),
    rules: include_str!("street_hall.cga"),
    seed: 0x005E_EDA1_7BB3_0005,
    materials,
    round_meshes: &["Tube", "Carapace", "Hoop"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Rib", "Lip", "Fascia", "Tube", "Carapace", "Hoop",
    ],
};

/// The hive's street palette and a rust-red shell.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_low::materials();
    m.insert("ChitinRust".to_string(), chitin([0.38, 0.20, 0.18]));
    m
}
