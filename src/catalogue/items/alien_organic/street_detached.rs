//! Berlin's detached house in the hive's dress (#1600): a brood pod grown
//! alone in its patch of creep - one body of ribbed chitin, segmented by
//! fleshy rolls at its floors, pod windows with fleshy lips on all four
//! sides, a maw between bone fangs for a door, and a carapace over it.
//!
//! - **It stands in from its lot.** Its body stands `Eave` in from every
//!   side, so its carapace's rim reaches out over it to the lot's edges
//!   and no further.
//! - **One body, one shell.** The chitin is rolled once at the lot; the
//!   ribs, lips, rolls, maw and carapace name their own, as the tower
//!   house's do.
//! - **Segmented all round**: a fleshy roll - a tube half sunk in the
//!   shell - wraps the body at every floor, and pods of three sizes, grown
//!   pod by pod, look out of every side, a bone rib by each corner.
//!   `Pick` decides once whether the end bays of its upper fronts carry a
//!   fleshy sac ledge.
//! - **A maw for a door**: a fleshy leaf between tapering fangs up a chitin
//!   step, under a fleshy brow.
//! - **The carapace**, `Pick`ed once, over a rim reaching out to the
//!   lot's edges: a dome of three tapering shells, taller over a deeper
//!   body, crowned by a shell or a glowing spore vent; a carapace laid
//!   along the street, ringed by bone hoops; or a low hump bristling with
//!   bone spines.

use std::collections::HashMap;

use crate::catalogue::items::street::{StreetKind, StreetSpec};
use crate::pds::SovereignMaterialSettings;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::chitin;

/// The brood pod (see the module docs); its rules are
/// `street_detached.cga`.
pub const SPEC: StreetSpec = StreetSpec {
    slug: "alien_organic_street_detached",
    name: "Hive Brood Pod",
    description: "A brood pod grown alone in its creep: a ribbed chitin body segmented by fleshy \
                  rolls, lipped pod windows on every side, a maw between fangs, and a domed or \
                  hooped carapace or a crown of spines.",
    themes: &[ThemeArchetype::AlienOrganic],
    kind: StreetKind::Detached,
    band: ProsperityBand::ANY,
    // A pod's tall ground segment, its maw, and the brood segments over it.
    storey_m: (3.4, 3.0),
    rules: include_str!("street_detached.cga"),
    seed: 0x005E_EDA1_7BB3_0004,
    materials,
    round_meshes: &["Tube", "Pod", "Carapace", "Hoop"],
    solid_meshes: &[
        "Wall", "Door", "Pane", "Roof", "Trim", "Rib", "Lip", "Tube", "Pod", "Carapace", "Hoop",
        "Spike",
    ],
};

/// The hive's street palette - chitin shells, the maws' flesh, ribs and
/// sinew, the pods' membranes and the spore glow - and a rust-red shell.
pub(crate) fn materials() -> HashMap<String, SovereignMaterialSettings> {
    let mut m = super::street_low::materials();
    m.insert("ChitinRust".to_string(), chitin([0.38, 0.20, 0.18]));
    m
}
